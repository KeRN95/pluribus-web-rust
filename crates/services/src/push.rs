//! Push notifications to the phone app, Android's and iOS's alike, through Firebase Cloud
//! Messaging (FCM), Google's service that hands a message to the phone a token names: by
//! Android's own way, and by Apple's APNs to an iPhone, which Firebase passes it on to. Set up by
//! FCM_SERVICE_ACCOUNT, the path to a Firebase service account's key file, read once as the server
//! starts (`Push::from_config`); without it there is no push, and the log says so then. The
//! senders carry it beside e-mail and WhatsApp (`Senders::push`, sender.rs): whatever else
//! reaches someone, their phones hear by push of the notifications written for them, once each
//! notification's row is written, never inside a transaction and never failing what wrote it (a
//! failure goes to the log): a notice of unread messages (`tell_notice`, from notices.rs), a
//! listing about to expire or expired (`tell_expiry`, from expiry.rs), a price dropped of a
//! listing kept (`after_price_drops`, which an edit calls once its transaction is through, with
//! those db::update_listing_with_drops told: the pages', pluribus-web's post.rs, and the API's,
//! pluribus-api's own.rs). Each is said as the API says its notification
//! (pluribus-api's notifications.rs): its title from what
//! the notification keeps (domain::notices::title), what it says under it, in the language kept on
//! the account (users.preferred_language), with what a tap opens in the app (`data`: its kind, and
//! the listing's number or the conversation), and the number on the app's icon on an iPhone, its
//! unread notifications (`badge`, as `/counts`' `notifications`). The app tells the server its
//! devices' tokens (pluribus-api's devices.rs, db::save_device_token).
//!
//! FCM's HTTP v1 API takes a short-lived access token, which the service account gets from Google
//! (OAuth 2.0 for server-to-server applications): a JWT signed with the account's key (RS256, by
//! ring, which rustls builds already), sent to the account's token_uri, and the token it answers
//! with kept 55 minutes of its hour (KEEP_TOKEN), and had again at once when FCM answers 401. A
//! device's token FCM no longer knows (404, or UNREGISTERED: the app was removed, or Firebase gave
//! it another) is forgotten (db::forget_device_token); any other refusal is logged, the token kept.

use std::{
    collections::BTreeMap,
    fmt,
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use pluribus_core::{
    domain::{NotificationKind, notices},
    i18n::{self, Lang},
};
use pluribus_db::{self as db, ExpiryNotice, PriceDropTold};
use reqwest::StatusCode;
use ring::{
    rand::SystemRandom,
    signature::{RSA_PKCS1_SHA256, RsaKeyPair},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::{
    AppState,
    config::Config,
    sender::{Notice, Senders},
};

/// What the access token lets the server do: send messages through FCM, nothing else.
pub const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
/// How long an access token is kept: Google's last an hour, and one kept 55 minutes never goes out
/// in its last five.
pub const KEEP_TOKEN: Duration = Duration::from_secs(55 * 60);
/// How long the signed request for an access token asks it to last, in seconds: an hour, the most
/// Google gives.
const TOKEN_SECONDS: u64 = 60 * 60;
/// What the token endpoint is asked for: an access token for a signed JWT (RFC 7523).
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

/// A Firebase service account's key file, as Firebase's console gives it ("Project settings",
/// "Service accounts", "Generate new private key"): what push needs of it.
#[derive(Clone)]
pub struct ServiceAccount {
    /// The Firebase project, whose `messages:send` the messages go to.
    pub project_id: String,
    /// The account's address, which asks for the access token (the JWT's `iss`).
    pub client_email: String,
    /// Its key, which signs the request: PEM, PKCS#8 ("-----BEGIN PRIVATE KEY-----").
    pub private_key: String,
    /// Where an access token is had: Google's https://oauth2.googleapis.com/token, or the tests'
    /// stand-in's.
    pub token_uri: String,
}

impl ServiceAccount {
    /// The key file at `path`, as FCM_SERVICE_ACCOUNT names it.
    pub fn read(path: &str) -> Result<Self, String> {
        let file = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
        Self::parse(&file)
    }

    /// A key file's JSON; one that isn't one, or lacks a field push needs, says why.
    pub fn parse(file: &str) -> Result<Self, String> {
        let file: Value = serde_json::from_str(file).map_err(|error| format!("not a service account's key file: {error}"))?;
        let field = |name: &str| {
            let value = file.get(name).and_then(Value::as_str).filter(|value| !value.trim().is_empty());
            value.map(str::to_string).ok_or_else(|| format!("the service account's key file has no {name}"))
        };
        Ok(Self {
            project_id: field("project_id")?,
            client_email: field("client_email")?,
            private_key: field("private_key")?,
            token_uri: field("token_uri")?,
        })
    }
}

/// The DER of a PKCS#8 key in PEM, as a service account's key file has it: the base64 between
/// its BEGIN and END lines.
fn pkcs8(pem: &str) -> Result<Vec<u8>, String> {
    let body: String = pem
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "-----BEGIN PRIVATE KEY-----")
        .skip(1)
        .take_while(|line| !line.starts_with("-----END"))
        .collect();
    if body.is_empty() {
        return Err("the service account's private_key isn't a PKCS#8 key (-----BEGIN PRIVATE KEY-----)".into());
    }
    STANDARD.decode(body).map_err(|error| format!("the service account's private_key: {error}"))
}

/// A push notification: what the phone shows, and what the app reads of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PushMessage {
    /// What it says: a notification's title, in its user's language.
    pub title: String,
    /// What it says under the title, where it says more: a message notice's start of the message.
    pub body: Option<String>,
    /// What the app reads of it, each value text, as FCM takes them: the notification's `kind` (a
    /// NotificationKind), and what a tap opens, `listing_no` or `conversation_id`.
    pub data: BTreeMap<String, String>,
    /// The number on the app's icon on an iPhone, its user's unread notifications; none leaves it
    /// as it was. Android's launchers count for themselves.
    pub badge: Option<u32>,
}

impl PushMessage {
    /// FCM's request for it to the device `token` names: the notification both platforms show, the
    /// data, Android's high priority (shown at once, a sleeping phone woken), and for APNs the
    /// icon's number and the default sound.
    pub fn payload(&self, token: &str) -> Value {
        let mut notification = json!({ "title": self.title });
        if let Some(body) = &self.body {
            notification["body"] = json!(body);
        }
        let mut aps = json!({ "sound": "default" });
        if let Some(badge) = self.badge {
            aps["badge"] = json!(badge);
        }
        json!({
            "message": {
                "token": token,
                "notification": notification,
                "data": self.data,
                "android": { "priority": "high" },
                "apns": { "payload": { "aps": aps } }
            }
        })
    }
}

/// Why a push didn't go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PushError {
    /// FCM no longer knows the device's token (404, or UNREGISTERED in its error's details): the
    /// app was removed from the phone, or Firebase gave it another. Its caller forgets it.
    Unregistered,
    /// Anything else, with why: no access token, FCM refusing the message, the network.
    Failed(String),
}

impl PushError {
    /// What FCM's refusal, its `status` and its error's JSON (`body`), means for the token.
    fn refused(status: StatusCode, body: &str) -> Self {
        let answer: Value = serde_json::from_str(body).unwrap_or_default();
        let details = answer.pointer("/error/details").and_then(Value::as_array);
        let unregistered = details.is_some_and(|details| details.iter().any(|detail| detail["errorCode"] == "UNREGISTERED"));
        if status == StatusCode::NOT_FOUND || unregistered {
            Self::Unregistered
        } else {
            Self::Failed(format!("FCM {status}: {body}"))
        }
    }
}

impl fmt::Display for PushError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unregistered => f.write_str("FCM no longer knows the device's token"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for PushError {}

/// Push notifications through FCM, set up once (`from_config`): where messages go, the service
/// account's address and key, and the access token kept.
pub struct Push {
    client: reqwest::Client,
    /// `{FCM_BASE_URL}/v1/projects/{project_id}/messages:send`.
    send_url: String,
    client_email: String,
    token_uri: String,
    key: RsaKeyPair,
    random: SystemRandom,
    /// The access token, and when it was had (KEEP_TOKEN).
    bearer: Mutex<Option<(String, Instant)>>,
}

impl Push {
    /// Push as the settings set it up: FCM_SERVICE_ACCOUNT's key file, read once, its messages to
    /// FCM_BASE_URL. None without one, the log saying so as the server starts; nor with one that
    /// can't be read, or whose key isn't one, the log saying why.
    pub fn from_config(config: &Config) -> Option<Self> {
        let Some(path) = &config.fcm_service_account else {
            tracing::info!("FCM_SERVICE_ACCOUNT isn't set: no push notifications to the phone app");
            return None;
        };
        match ServiceAccount::read(path).and_then(|account| Self::new(account, &config.fcm_base_url)) {
            Ok(push) => Some(push),
            Err(error) => {
                tracing::error!(%error, "push notifications to the phone app are off");
                None
            }
        }
    }

    /// Push by `account`, its messages to FCM's API at `base_url` (DEFAULT_FCM_URL, or the tests'
    /// stand-in); an error where its key isn't an RSA key in PKCS#8.
    pub fn new(account: ServiceAccount, base_url: &str) -> Result<Self, String> {
        let key = RsaKeyPair::from_pkcs8(&pkcs8(&account.private_key)?)
            .map_err(|error| format!("the service account's private_key: {error}"))?;
        Ok(Self {
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("an HTTP client"),
            send_url: format!("{}/v1/projects/{}/messages:send", base_url.trim_end_matches('/'), account.project_id),
            client_email: account.client_email,
            token_uri: account.token_uri,
            key,
            random: SystemRandom::new(),
            bearer: Mutex::new(None),
        })
    }

    /// The signed request for an access token (a JWT, RS256): the service account asking, at
    /// `now` (seconds since 1970), the token endpoint for FCM's SCOPE, for TOKEN_SECONDS.
    fn assertion(&self, now: u64) -> Result<String, PushError> {
        let header = json!({ "alg": "RS256", "typ": "JWT" });
        let claims = json!({
            "iss": self.client_email,
            "scope": SCOPE,
            "aud": self.token_uri,
            "iat": now,
            "exp": now + TOKEN_SECONDS,
        });
        let signed = format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
        let mut signature = vec![0; self.key.public().modulus_len()];
        self.key
            .sign(&RSA_PKCS1_SHA256, &self.random, signed.as_bytes(), &mut signature)
            .map_err(|_| PushError::Failed("the request for an access token couldn't be signed".into()))?;
        Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature)))
    }

    /// The access token kept, while under KEEP_TOKEN old.
    fn kept_token(&self) -> Option<String> {
        let kept = self.bearer.lock().unwrap();
        kept.as_ref().filter(|(_, had)| had.elapsed() < KEEP_TOKEN).map(|(token, _)| token.clone())
    }

    /// Forgets the access token kept, if it is still `refused`, one FCM no longer takes.
    fn forget_token(&self, refused: &str) {
        let mut kept = self.bearer.lock().unwrap();
        if kept.as_ref().is_some_and(|(token, _)| token == refused) {
            *kept = None;
        }
    }

    /// The access token: the one kept, or a new one from the token endpoint, which is kept.
    async fn access_token(&self) -> Result<String, PushError> {
        if let Some(token) = self.kept_token() {
            return Ok(token);
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
        let assertion = self.assertion(now)?;
        let response = self
            .client
            .post(&self.token_uri)
            .form(&[("grant_type", GRANT_TYPE), ("assertion", assertion.as_str())])
            .send()
            .await
            .map_err(|error| PushError::Failed(format!("Google's token endpoint: {error}")))?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(PushError::Failed(format!("Google's token endpoint {status}: {body}")));
        }
        let answer: Value = serde_json::from_str(&body).unwrap_or_default();
        let Some(token) = answer["access_token"].as_str().filter(|token| !token.is_empty()) else {
            return Err(PushError::Failed(format!("Google's token endpoint gave no access token: {body}")));
        };
        *self.bearer.lock().unwrap() = Some((token.to_string(), Instant::now()));
        Ok(token.to_string())
    }

    /// Sends `message` to the device `token` names: Ok once FCM took it; `PushError::Unregistered`
    /// where FCM no longer knows the token; any other refusal or failure `PushError::Failed`, with
    /// why. A 401 has the access token had again, and the message sent once more.
    pub async fn send(&self, token: &str, message: &PushMessage) -> Result<(), PushError> {
        let payload = message.payload(token);
        let mut again = true;
        loop {
            let bearer = self.access_token().await?;
            let response = self
                .client
                .post(&self.send_url)
                .bearer_auth(&bearer)
                .json(&payload)
                .send()
                .await
                .map_err(|error| PushError::Failed(format!("FCM: {error}")))?;
            let status = response.status();
            if status.is_success() {
                return Ok(());
            }
            let body = response.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED && again {
                // The access token expired early, or was revoked: another, and once more.
                self.forget_token(&bearer);
                again = false;
                continue;
            }
            return Err(PushError::refused(status, &body));
        }
    }

    /// Sends `message` to every device of `user_id`'s (db::device_tokens), forgetting those FCM no
    /// longer knows; how many took it. Another failure, in the log, leaves its token kept.
    pub async fn send_all(&self, db: &PgPool, user_id: Uuid, message: &PushMessage) -> sqlx::Result<usize> {
        let devices = db::device_tokens(db, user_id).await?;
        self.send_each(db, user_id, devices, message).await
    }

    /// Sends `message` to each of `devices`, `user_id`'s (each a platform and a token), as
    /// `send_all` does.
    async fn send_each(&self, db: &PgPool, user_id: Uuid, devices: Vec<(String, String)>, message: &PushMessage) -> sqlx::Result<usize> {
        let mut sent = 0;
        for (platform, token) in devices {
            match self.send(&token, message).await {
                Ok(()) => sent += 1,
                Err(PushError::Unregistered) => {
                    db::forget_device_token(db, &token, None).await?;
                    tracing::info!(user = %user_id, %platform, "push: a device's token Firebase no longer knows, forgotten");
                }
                Err(error) => tracing::warn!(%error, user = %user_id, %platform, "push not sent"),
            }
        }
        Ok(sent)
    }
}

/// A notification just written, which its user's phones hear of too (`tell`).
pub struct Notified<'a> {
    /// Whose it is.
    pub user_id: Uuid,
    /// The language kept on their account (users.preferred_language: "sq", "en"), which it is
    /// said in.
    pub language: &'a str,
    pub kind: NotificationKind,
    /// What it keeps (notifications.payload), which its title is said from (domain::notices).
    pub payload: Value,
    /// What it says under its title as its writer kept it (notifications.body: a message notice's
    /// start of the message), where domain::notices says nothing of its own.
    pub body: Option<String>,
    /// What a tap opens in the app: the name `data` gives it, and its value
    /// (`("listing_no", "1001")`, `("conversation_id", …)`).
    pub opens: (&'static str, String),
}

impl Notified<'_> {
    /// The push that says it, in the language now, with `badge` on the app's icon.
    fn message(&self, badge: Option<u32>) -> PushMessage {
        let mut data = BTreeMap::from([("kind".to_string(), self.kind.as_str().to_string())]);
        data.insert(self.opens.0.to_string(), self.opens.1.clone());
        PushMessage {
            title: notices::title(self.kind, &self.payload),
            body: notices::body(self.kind, &self.payload).or_else(|| self.body.clone()),
            data,
            badge,
        }
    }
}

/// Tells the phones of `notified`'s user of their notification, where push is set up
/// (`Senders::push`) and they have the app (db::device_tokens): said in the language kept on their
/// account, the app's icon counting their unread notifications (db::unread_notifications). Never
/// an error: a failure goes to the log, and what wrote the notification goes on.
pub async fn tell(db: &PgPool, senders: &Senders, notified: Notified<'_>) {
    let Some(push) = senders.push() else {
        return;
    };
    let (user_id, kind) = (notified.user_id, notified.kind);
    let lang = Lang::from_code(notified.language).unwrap_or_default();
    let told = async {
        let devices = db::device_tokens(db, user_id).await?;
        if devices.is_empty() {
            return Ok(0);
        }
        let unread = db::unread_notifications(db, user_id).await?;
        let message = i18n::in_lang(lang, async { notified.message(u32::try_from(unread.all).ok()) }).await;
        push.send_each(db, user_id, devices, &message).await
    };
    match told.await {
        Ok(sent) => tracing::debug!(user = %user_id, %kind, sent, "push sent"),
        Err(error) => tracing::warn!(%error, user = %user_id, %kind, "push not sent"),
    }
}

/// The push beside a notice of unread messages (notices.rs, `send_due`), whatever channel reaches
/// its reader: to `user_id`, of `conversation`, said as its notification says it (its payload as
/// db::record_notice keeps it: who wrote, about which listing), the start of the message under
/// it, a tap opening the conversation.
pub async fn tell_notice(db: &PgPool, senders: &Senders, user_id: Uuid, language: &str, conversation: Uuid, notice: &Notice) {
    let payload = json!({ "conversation": conversation.to_string(), "writer": notice.writer, "listing": notice.listing });
    let body = Some(notice.excerpt.clone());
    let opens = ("conversation_id", conversation.to_string());
    tell(db, senders, Notified { user_id, language, kind: NotificationKind::NewMessage, payload, body, opens }).await;
}

/// The push beside an expiry's e-mail or WhatsApp message (expiry.rs, `run`): to the seller
/// `listing` names, that it expired (EXPIRED) or is about to (EXPIRING), said as its notification
/// says it (from the listing its payload keeps, as db::expire_listings and db::warn_expiring keep
/// it), a tap opening the listing.
pub async fn tell_expiry(db: &PgPool, senders: &Senders, kind: NotificationKind, listing: &ExpiryNotice) {
    let payload = json!({ "listing_id": listing.listing_id, "public_no": listing.public_no, "title": listing.title });
    let opens = ("listing_no", listing.public_no.to_string());
    let notified = Notified { user_id: listing.user_id, language: &listing.language, kind, payload, body: None, opens };
    tell(db, senders, notified).await;
}

/// After an edit lowered a listing's price, once its transaction went through
/// (db::update_listing_with_drops): the phones of those it told hear of it too (`drops`,
/// db::notify_price_drop's), each in the language kept on their account, said as their
/// notification says it, a tap opening the listing. In the background, so that the edit's answer
/// waits on no phone, and never failing it. The task, which a test awaits; none where push isn't
/// set up or nobody was told.
pub fn after_price_drops(state: &AppState, drops: Vec<PriceDropTold>) -> Option<JoinHandle<()>> {
    if drops.is_empty() {
        return None;
    }
    state.senders.push()?;
    let (db, senders) = (state.db.clone(), state.senders.clone());
    Some(tokio::spawn(async move {
        for told in drops {
            let opens = ("listing_no", told.public_no.to_string());
            let notified =
                Notified { user_id: told.user_id, language: &told.language, kind: NotificationKind::PriceDrop, payload: told.payload.0, body: None, opens };
            tell(&db, &senders, notified).await;
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_file_needs_its_four_fields_and_a_pkcs8_key() {
        let file = json!({"project_id": "p", "client_email": "a@p.iam.gserviceaccount.com", "private_key": "x", "token_uri": "https://t"});
        let account = ServiceAccount::parse(&file.to_string()).unwrap();
        assert_eq!((account.project_id.as_str(), account.token_uri.as_str()), ("p", "https://t"));
        for field in ["project_id", "client_email", "private_key", "token_uri"] {
            let mut without = file.clone();
            without.as_object_mut().unwrap().remove(field);
            let error = ServiceAccount::parse(&without.to_string()).err().unwrap();
            assert_eq!(error, format!("the service account's key file has no {field}"));
        }
        assert!(ServiceAccount::parse("{").err().unwrap().starts_with("not a service account's key file"));
        // A key that isn't PKCS#8, or isn't one at all, makes no push.
        let pkcs1 = "-----BEGIN RSA PRIVATE KEY-----\nMIIBOgIBAAJBAK\n-----END RSA PRIVATE KEY-----\n";
        assert!(pkcs8(pkcs1).unwrap_err().contains("isn't a PKCS#8 key"));
        assert_eq!(pkcs8("-----BEGIN PRIVATE KEY-----\nAQID\nBAU=\n-----END PRIVATE KEY-----\n").unwrap(), [1, 2, 3, 4, 5]);
        let account = ServiceAccount { private_key: "-----BEGIN PRIVATE KEY-----\nAQID\n-----END PRIVATE KEY-----".into(), ..account };
        assert!(Push::new(account, "https://fcm.googleapis.com").err().unwrap().starts_with("the service account's private_key"));
    }

    #[test]
    fn a_404_or_unregistered_forgets_the_token_and_nothing_else_does() {
        let unregistered = json!({"error": {"code": 404, "message": "Requested entity was not found.", "status": "NOT_FOUND",
            "details": [{"@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", "errorCode": "UNREGISTERED"}]}});
        assert_eq!(PushError::refused(StatusCode::NOT_FOUND, &unregistered.to_string()), PushError::Unregistered);
        assert_eq!(PushError::refused(StatusCode::NOT_FOUND, ""), PushError::Unregistered);
        let gone = json!({"error": {"code": 400, "status": "INVALID_ARGUMENT", "details": [{"errorCode": "UNREGISTERED"}]}});
        assert_eq!(PushError::refused(StatusCode::BAD_REQUEST, &gone.to_string()), PushError::Unregistered);
        // A message FCM refuses, or a quota, keeps the token: the fault is elsewhere.
        let invalid = json!({"error": {"code": 400, "status": "INVALID_ARGUMENT", "details": [{"errorCode": "INVALID_ARGUMENT"}]}});
        let refused = PushError::refused(StatusCode::BAD_REQUEST, &invalid.to_string());
        assert!(matches!(&refused, PushError::Failed(why) if why.starts_with("FCM 400 Bad Request: ") && why.contains("INVALID_ARGUMENT")), "{refused}");
        assert!(matches!(PushError::refused(StatusCode::TOO_MANY_REQUESTS, "quota"), PushError::Failed(_)));
    }

    #[test]
    fn a_push_says_its_notification_in_its_users_language() {
        let notified = Notified {
            user_id: Uuid::nil(),
            language: "en",
            kind: NotificationKind::NewMessage,
            payload: json!({"conversation": "c", "writer": "Besa", "listing": "Volkswagen Golf 7"}),
            body: Some("A është ende në shitje?".into()),
            opens: ("conversation_id", "c".into()),
        };
        let message = i18n::LANG.sync_scope(Lang::En, || notified.message(Some(2)));
        assert_eq!(message.title, "Besa wrote to you about “Volkswagen Golf 7”");
        assert_eq!(message.body.as_deref(), Some("A është ende në shitje?"));
        let data: Vec<(&str, &str)> = message.data.iter().map(|(name, value)| (name.as_str(), value.as_str())).collect();
        assert_eq!(data, [("conversation_id", "c"), ("kind", "NEW_MESSAGE")]);
        // Without a badge or a body, neither goes.
        let quiet = PushMessage { badge: None, body: None, ..message.clone() }.payload("t");
        assert_eq!(quiet["message"]["notification"], json!({"title": "Besa wrote to you about “Volkswagen Golf 7”"}));
        assert_eq!(quiet["message"]["apns"], json!({"payload": {"aps": {"sound": "default"}}}));
        assert_eq!(message.payload("t")["message"]["apns"]["payload"]["aps"]["badge"], 2);
    }
}
