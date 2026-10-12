//! Push notifications through Firebase Cloud Messaging (push.rs), against a stand-in for Google's
//! token endpoint and FCM's `messages:send` on a port of the test's own, as sender.rs's tests stand
//! one in for WhatsApp's: the access token asked for by a JWT that is well formed and signed by the
//! service account's key (the fixture's, tests/fixtures/fcm-test-key.json, a throwaway made once
//! with openssl), kept for the messages after it and had again on a 401; a message carrying its
//! title, its body, its data, and its badge and sound for APNs; a device's token FCM no longer
//! knows forgotten and the others kept; the expiry told by push to the seller's phone in the
//! language of their account; and push set up by a key file, and off without one.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Form, Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use pluribus_db as db;
use pluribus_services::{
    config::Config,
    expiry,
    push::{KEEP_TOKEN, Push, PushError, PushMessage, ServiceAccount},
    sender::Senders,
};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// The throwaway service account's key file.
const KEY_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fcm-test-key.json");
/// Its key's public half, as openssl gave it apart from the key file (`openssl rsa
/// -RSAPublicKey_out -outform DER`, base64): the RSAPublicKey ring verifies RS256 by.
const PUBLIC_KEY: &str = "MIIBCgKCAQEAj8cFIMw7iJ+qYo2xpD65tjuMoURTvPyHGFsesjNVy9Dgr/lOaqvmf+uZLfWVJbhV568HGV1VRNdjtiItgccW7SncQl1rjzgF/BmU1YoTa88UvQ0SCIyw5/l/G8D0UYH+epS1gg7zh8eqvabXB1tW3aXo1yItv3oemM1wkply/0y6FjimGs0O3MBva4M//GCqczHqyKDQqvZzUhqBfFmxwKkyIavvCEpGhoDoKIz0WfRQTualJj5bKl1gbPmzqi9KxUiBFCiIqlrlmDNT/ohi3ObPovzahXn1tHcni3ceZX/4bq/NOuaEyxHcD/6zEG1wlfCcOV+9JpfJ0iuoodVP5wIDAQAB";
/// The fixtures' Ana (crates/web/tests/fixtures/listings.sql), whose are listings 1005 and 1007.
const ANA: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);
/// Where the e-mails' links start.
const ORIGIN: &str = "https://pluribus.example";

/// Firebase as the tests stand it in, on a port of their own: Google's token endpoint (`/token`),
/// which answers `token-1`, `token-2`... and keeps every form it is sent; and FCM's
/// `messages:send` for the fixture's project, which keeps every request, the bearer it came with
/// and its JSON, and answers as FCM would by the device's token: one starting `gone` unknown
/// (404, UNREGISTERED), one starting `bad` refused (400, INVALID_ARGUMENT), any other taken; and
/// 401 to as many requests as `refuse` says, as to an access token revoked.
#[derive(Clone, Default)]
struct Firebase {
    asked: Arc<Mutex<Vec<HashMap<String, String>>>>,
    sent: Arc<Mutex<Vec<(String, Value)>>>,
    refuse: Arc<AtomicUsize>,
}

impl Firebase {
    /// Serves it; and push to it, by the fixture's service account, its token endpoint the
    /// stand-in's.
    async fn start() -> (Self, Push, String) {
        let firebase = Self::default();
        let app = Router::new()
            .route("/token", post(token))
            .route("/v1/projects/pluribus-test/messages:send", post(send))
            .with_state(firebase.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut account = ServiceAccount::read(KEY_FILE).unwrap();
        account.token_uri = format!("http://{address}/token");
        let push = Push::new(account, &format!("http://{address}/")).unwrap();
        (firebase, push, format!("http://{address}/token"))
    }

    /// The bearers the messages came with, in order.
    fn bearers(&self) -> Vec<String> {
        self.sent.lock().unwrap().iter().map(|(bearer, _)| bearer.clone()).collect()
    }

    /// The messages sent, in order: each `message` of FCM's requests.
    fn messages(&self) -> Vec<Value> {
        self.sent.lock().unwrap().iter().map(|(_, request)| request["message"].clone()).collect()
    }
}

async fn token(State(firebase): State<Firebase>, Form(form): Form<HashMap<String, String>>) -> Json<Value> {
    let mut asked = firebase.asked.lock().unwrap();
    asked.push(form);
    Json(json!({ "access_token": format!("token-{}", asked.len()), "expires_in": 3599, "token_type": "Bearer" }))
}

async fn send(State(firebase): State<Firebase>, headers: HeaderMap, Json(request): Json<Value>) -> (StatusCode, Json<Value>) {
    let bearer = headers["authorization"].to_str().unwrap().to_string();
    let device = request["message"]["token"].as_str().unwrap_or_default().to_string();
    firebase.sent.lock().unwrap().push((bearer, request));
    let fcm_error = |code: u16, status: &str, error_code: &str| {
        let detail = json!({ "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", "errorCode": error_code });
        Json(json!({ "error": { "code": code, "message": "…", "status": status, "details": [detail] } }))
    };
    if firebase.refuse.load(Ordering::SeqCst) > 0 {
        firebase.refuse.fetch_sub(1, Ordering::SeqCst);
        return (StatusCode::UNAUTHORIZED, Json(json!({ "error": { "code": 401, "status": "UNAUTHENTICATED" } })));
    }
    if device.starts_with("gone") {
        (StatusCode::NOT_FOUND, fcm_error(404, "NOT_FOUND", "UNREGISTERED"))
    } else if device.starts_with("bad") {
        (StatusCode::BAD_REQUEST, fcm_error(400, "INVALID_ARGUMENT", "INVALID_ARGUMENT"))
    } else {
        (StatusCode::OK, Json(json!({ "name": "projects/pluribus-test/messages/0:1" })))
    }
}

/// Seconds since 1970, as a JWT counts them.
fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
}

/// A part of a JWT, decoded: its JSON.
fn decoded(part: &str) -> Value {
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
}

#[tokio::test]
async fn the_access_token_is_asked_for_by_a_signed_jwt_kept_and_had_again_on_a_401() {
    let (firebase, push, token_uri) = Firebase::start().await;
    let message = PushMessage { title: "Çmimi ra: Volkswagen Golf 7".into(), ..PushMessage::default() };
    let before = now();
    push.send("phone-1", &message).await.unwrap();
    push.send("phone-2", &message).await.unwrap();

    // One access token for both, asked for once, by a JWT.
    let asked = firebase.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0]["grant_type"], "urn:ietf:params:oauth:grant-type:jwt-bearer");
    let jwt = &asked[0]["assertion"];
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3, "{jwt}");
    assert_eq!(decoded(parts[0]), json!({ "alg": "RS256", "typ": "JWT" }));
    // The service account asking for FCM's scope, of the token endpoint, for an hour from now.
    let claims = decoded(parts[1]);
    assert_eq!(claims["iss"], "firebase-adminsdk-test@pluribus-test.iam.gserviceaccount.com");
    assert_eq!(claims["scope"], "https://www.googleapis.com/auth/firebase.messaging");
    assert_eq!(claims["aud"], token_uri);
    let issued = claims["iat"].as_u64().unwrap();
    assert!((before..=now()).contains(&issued), "{claims}");
    assert_eq!(claims["exp"].as_u64(), Some(issued + 3600));
    assert_eq!(claims.as_object().unwrap().len(), 5, "{claims}");
    // Signed with the service account's key (RS256 over its first two parts): its public half,
    // openssl's, verifies it, and not another text.
    let public = STANDARD.decode(PUBLIC_KEY).unwrap();
    let key = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &public);
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    key.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature).expect("signed by the service account's key");
    assert!(key.verify(format!("{}.{}", parts[0], parts[0]).as_bytes(), &signature).is_err());
    // Kept 55 minutes of its hour: both messages went with it.
    assert_eq!(KEEP_TOKEN, Duration::from_secs(55 * 60));
    assert_eq!(firebase.bearers(), ["Bearer token-1", "Bearer token-1"]);

    // FCM refuses it, revoked or expired early: another, and the message again, once.
    firebase.refuse.store(1, Ordering::SeqCst);
    push.send("phone-3", &message).await.unwrap();
    assert_eq!(firebase.asked.lock().unwrap().len(), 2);
    assert_eq!(firebase.bearers()[2..], ["Bearer token-1", "Bearer token-2"]);
    assert_eq!(firebase.messages().iter().filter(|sent| sent["token"] == "phone-3").count(), 2);
    // And the new one kept for the next.
    push.send("phone-4", &message).await.unwrap();
    assert_eq!(firebase.bearers()[4..], ["Bearer token-2"]);
    // Refused again with the new one, it gives up, an error with FCM's reason.
    firebase.refuse.store(2, Ordering::SeqCst);
    let refused = push.send("phone-5", &message).await.unwrap_err();
    assert!(matches!(&refused, PushError::Failed(why) if why.starts_with("FCM 401 Unauthorized") && why.contains("UNAUTHENTICATED")), "{refused}");
    assert_eq!(firebase.asked.lock().unwrap().len(), 3);
}

#[tokio::test]
async fn a_message_carries_its_title_body_and_data_and_apns_its_badge_and_sound() {
    let (firebase, push, _) = Firebase::start().await;
    let conversation = "0192f0c1-0000-7000-8000-00000000000c";
    let message = PushMessage {
        title: "Besa të shkroi për „Volkswagen Golf 7“".into(),
        body: Some("A është ende në shitje?".into()),
        data: BTreeMap::from([("kind".into(), "NEW_MESSAGE".into()), ("conversation_id".into(), conversation.into())]),
        badge: Some(3),
    };
    push.send("phone-1", &message).await.unwrap();
    assert_eq!(
        firebase.messages(),
        [json!({
            "token": "phone-1",
            "notification": { "title": "Besa të shkroi për „Volkswagen Golf 7“", "body": "A është ende në shitje?" },
            "data": { "kind": "NEW_MESSAGE", "conversation_id": conversation },
            "android": { "priority": "high" },
            "apns": { "payload": { "aps": { "badge": 3, "sound": "default" } } }
        })]
    );
    // A token FCM no longer knows: Unregistered, which the caller forgets; another refusal an error.
    assert_eq!(push.send("gone-1", &message).await, Err(PushError::Unregistered));
    assert!(matches!(push.send("bad-1", &message).await, Err(PushError::Failed(why)) if why.starts_with("FCM 400 Bad Request")));
}

#[sqlx::test(migrator = "pluribus_db::MIGRATOR")]
async fn a_token_firebase_no_longer_knows_is_forgotten_and_the_others_kept(pool: PgPool) -> sqlx::Result<()> {
    let (firebase, push, _) = Firebase::start().await;
    let ana: Uuid = sqlx::query_scalar("INSERT INTO users (auth_subject, phone_e164, display_name) VALUES ('phone:+355690000001', '+355690000001', 'Ana') RETURNING id")
        .fetch_one(&pool)
        .await?;
    for (platform, token) in [("ANDROID", "phone-ana"), ("IOS", "gone-ana"), ("IOS", "bad-ana")] {
        db::save_device_token(&pool, ana, platform, token).await?;
    }
    let message = PushMessage { title: "Njoftimi skadoi: Divan".into(), ..PushMessage::default() };
    // Sent to all three, taken by one.
    assert_eq!(push.send_all(&pool, ana, &message).await?, 1);
    let mut tried: Vec<String> = firebase.messages().iter().map(|sent| sent["token"].as_str().unwrap().to_string()).collect();
    tried.sort();
    assert_eq!(tried, ["bad-ana", "gone-ana", "phone-ana"]);
    // The one FCM no longer knows is forgotten; the one it refused for another reason is kept.
    let mut kept: Vec<String> = db::device_tokens(&pool, ana).await?.into_iter().map(|(_, token)| token).collect();
    kept.sort();
    assert_eq!(kept, ["bad-ana", "phone-ana"]);
    Ok(())
}

#[sqlx::test(migrator = "pluribus_db::MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn the_expiry_reaches_the_sellers_phone_in_the_language_of_her_account(pool: PgPool) -> sqlx::Result<()> {
    let (firebase, push, _) = Firebase::start().await;
    let senders = Senders::none().with_push(push);
    db::save_device_token(&pool, ANA, "IOS", "phone-ana").await?;
    sqlx::query("UPDATE users SET preferred_language = 'en' WHERE id = $1").bind(ANA).execute(&pool).await?;
    // Her sofa's time is up, and her iPhone's two days off; she has no channel set up, and her
    // phone hears of both all the same.
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = 1007").execute(&pool).await?;
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = 1005").execute(&pool).await?;
    assert_eq!(expiry::run(&pool, &senders, ORIGIN).await?, (1, 1));
    let title = |no: i64| sqlx::query_scalar::<_, String>("SELECT coalesce(generated_title, title) FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool);
    let (sofa, iphone) = (title(1007).await?, title(1005).await?);
    let sent = firebase.messages();
    assert_eq!(sent.len(), 2, "{sent:#?}");
    // In English, as her account has it, the expired one first; a tap opens the listing; the
    // icon counts her notifications unread, one and then two.
    assert_eq!(sent[0]["token"], "phone-ana");
    assert_eq!(sent[0]["notification"], json!({ "title": format!("Listing expired: {sofa}") }));
    assert_eq!(sent[0]["data"], json!({ "kind": "EXPIRED", "listing_no": "1007" }));
    assert_eq!(sent[0]["apns"]["payload"]["aps"], json!({ "badge": 1, "sound": "default" }));
    assert_eq!(sent[1]["notification"], json!({ "title": format!("Listing expires soon: {iphone}") }));
    assert_eq!(sent[1]["data"], json!({ "kind": "EXPIRING", "listing_no": "1005" }));
    assert_eq!(sent[1]["apns"]["payload"]["aps"]["badge"], 2);

    // In Albanian, each says what its notification keeps.
    sqlx::query("UPDATE users SET preferred_language = 'sq' WHERE id = $1").bind(ANA).execute(&pool).await?;
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = 1005").execute(&pool).await?;
    assert_eq!(expiry::run(&pool, &senders, ORIGIN).await?, (0, 1));
    let kept: String =
        sqlx::query_scalar("SELECT title FROM notifications WHERE user_id = $1 AND notification_type = 'EXPIRED' AND payload->>'public_no' = '1005'")
            .bind(ANA)
            .fetch_one(&pool)
            .await?;
    assert_eq!(firebase.messages()[2]["notification"]["title"], kept);
    assert_eq!(kept, format!("Njoftimi skadoi: {iphone}"));
    Ok(())
}

#[test]
fn push_is_set_up_by_a_key_file_and_off_without_one() {
    let with = Config { fcm_service_account: Some(KEY_FILE.into()), ..Config::for_tests() };
    assert!(Push::from_config(&with).is_some());
    assert!(Senders::from_config(&with).push().is_some());
    // Without one, or with one that can't be read, none: the log says why as the server starts.
    assert!(Push::from_config(&Config::for_tests()).is_none());
    assert!(Senders::from_config(&Config::for_tests()).push().is_none() && Senders::none().push().is_none());
    let missing = Config { fcm_service_account: Some("/nowhere/firebase.json".into()), ..Config::for_tests() };
    assert!(Push::from_config(&missing).is_none());
}
