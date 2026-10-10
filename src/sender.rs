//! Sending sign-in codes: to an e-mail address by SMTP, which every e-mail service offers (Amazon
//! SES, Postmark, Brevo...), and to a phone number by WhatsApp, through Meta's WhatsApp Cloud API
//! and an authentication template. Each is set up in .env (see .env.example); a channel that isn't
//! writes its codes to the server's log instead, which is how development runs. The same channels
//! carry the notices of messages someone hasn't read (notices.rs), the alerts of saved searches
//! (web/searches.rs) and the warnings of listings about to expire or expired (expiry.rs): by
//! e-mail, or by WhatsApp with a utility template of each one's own (WHATSAPP_NOTICE_TEMPLATE,
//! WHATSAPP_SEARCH_TEMPLATE, WHATSAPP_EXPIRY_TEMPLATE); a phone number gets none of a kind
//! whose template isn't set up.

use std::{sync::OnceLock, time::Duration};

use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, header::ContentType},
    transport::smtp::authentication::Credentials,
};
use serde_json::{Value, json};

use crate::format::Contact;

/// How a code reaches its contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Email,
    WhatsApp,
    /// Not set up: the code goes to the server's log.
    Log,
}

/// A notice of messages someone hasn't read (notices.rs): who wrote, about which listing, the
/// latest message, how many are unread, and the address of the conversation.
#[derive(Debug, Clone)]
pub struct Notice {
    pub writer: String,
    pub listing: Option<String>,
    pub excerpt: String,
    pub unread: i64,
    pub url: String,
}

impl Notice {
    /// "Ana të shkroi për „Volkswagen Golf 7“": the e-mail's subject, and the notification's title.
    pub fn title(&self) -> String {
        match &self.listing {
            Some(listing) => format!("{} të shkroi për „{listing}“", self.writer),
            None => format!("{} të shkroi në Pluribus", self.writer),
        }
    }

    /// The e-mail's text: who wrote and about what, the latest message (and how many more), the
    /// link to answer, and how often these come and how to stop them.
    pub fn text(&self) -> String {
        let about = match &self.listing {
            Some(listing) => format!(" për „{listing}“"),
            None => String::new(),
        };
        let more = match self.unread {
            ..=1 => String::new(),
            2 => "\n(dhe 1 mesazh tjetër i palexuar)\n".to_string(),
            unread => format!("\n(dhe {} mesazhe të tjera të palexuara)\n", unread - 1),
        };
        format!(
            "{} të shkroi në Pluribus{about}:\n\n    {}\n{more}\nPërgjigju këtu: {}\n\n\
             Për një bisedë nuk të vjen më shumë se një email në orë. Për të mos i marrë më, \
             hiq shenjën te „Mesazhet që nuk i ke lexuar“ te „Cilësimet“.\n",
            self.writer,
            self.excerpt.replace('\n', "\n    "),
            self.url,
        )
    }
}

/// An alert, for the channel that takes it: an e-mail's subject and text, and what its WhatsApp
/// template fills in.
#[derive(Debug, Clone)]
pub struct Alert {
    pub subject: String,
    pub body: String,
    pub whatsapp: AlertTemplate,
}

/// What an alert's WhatsApp template fills in.
#[derive(Debug, Clone)]
pub enum AlertTemplate {
    /// A saved search's new listings: how many ("3 njoftime të reja", {{1}}) and the search's
    /// name ({{2}}); its id ends the button's address (/kerkimet/{{1}}), which opens it.
    Search { new: String, name: String, id: String },
    /// A listing about to expire or expired: its title ({{1}}) and what becomes of it ("skadon më
    /// 16.10.2026", "skadoi", {{2}}); its button opens "Njoftimet e mia", the same for all.
    Expiry { title: String, what: String },
}

/// Meta's WhatsApp Cloud API, sending an authentication template: Meta's fixed text with the code
/// in it, and a button that copies the code. With a notice template too (WHATSAPP_NOTICE_TEMPLATE,
/// a utility template), the notices of messages someone hasn't read.
pub struct WhatsApp {
    client: reqwest::Client,
    url: String,
    token: String,
    template: String,
    language: String,
    /// The utility template that tells someone they have a message: its body says who wrote
    /// ({{1}}) and about what ({{2}}), and its button opens the conversation, its address's end
    /// the conversation's own (the template's URL ending in {{1}}).
    notice_template: Option<String>,
    /// The utility templates of the alerts (AlertTemplate): a saved search's new listings, and a
    /// listing about to expire or expired.
    search_template: Option<String>,
    expiry_template: Option<String>,
}

impl WhatsApp {
    pub fn new(base_url: &str, version: &str, phone_number_id: &str, token: &str, template: &str, language: &str) -> Self {
        Self {
            client: reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("an HTTP client"),
            url: format!("{base_url}/{version}/{phone_number_id}/messages"),
            token: token.to_string(),
            template: template.to_string(),
            language: language.to_string(),
            notice_template: None,
            search_template: None,
            expiry_template: None,
        }
    }

    /// The same, sending notices of unread messages with this utility template too.
    pub fn with_notices(mut self, template: Option<String>) -> Self {
        self.notice_template = template;
        self
    }

    /// The same, sending the alerts with these utility templates too, those set up.
    pub fn with_alerts(mut self, search: Option<String>, expiry: Option<String>) -> Self {
        self.search_template = search;
        self.expiry_template = expiry;
        self
    }

    /// The notice: the template's text with who wrote and about what, and its button to the
    /// conversation, the end of whose address (after /mesazhet/) it fills in.
    pub fn notice_payload(&self, phone: &str, notice: &Notice) -> Option<Value> {
        let template = self.notice_template.as_ref()?;
        let conversation = notice.url.rsplit('/').next().unwrap_or_default();
        let texts = [notice.writer.as_str(), notice.listing.as_deref().unwrap_or("Pluribus")];
        Some(self.template_payload(phone, template, &texts, Some(conversation)))
    }

    /// An alert, by its kind's template if one is set up.
    pub fn alert_payload(&self, phone: &str, alert: &AlertTemplate) -> Option<Value> {
        Some(match alert {
            AlertTemplate::Search { new, name, id } => self.template_payload(phone, self.search_template.as_ref()?, &[new.as_str(), name.as_str()], Some(id.as_str())),
            AlertTemplate::Expiry { title, what } => self.template_payload(phone, self.expiry_template.as_ref()?, &[title.as_str(), what.as_str()], None),
        })
    }

    /// A utility template's message: its body's texts in order, and the end of its URL button's
    /// address where that address takes one.
    fn template_payload(&self, phone: &str, template: &str, texts: &[&str], button: Option<&str>) -> Value {
        let texts: Vec<Value> = texts.iter().map(|text| json!({ "type": "text", "text": text })).collect();
        let mut components = vec![json!({ "type": "body", "parameters": texts })];
        if let Some(button) = button {
            components.push(json!({ "type": "button", "sub_type": "url", "index": "0",
                                    "parameters": [{ "type": "text", "text": button }] }));
        }
        json!({
            "messaging_product": "whatsapp",
            "to": phone.trim_start_matches('+'),
            "type": "template",
            "template": { "name": template, "language": { "code": self.language }, "components": components }
        })
    }

    async fn post(&self, payload: &Value) -> Result<(), String> {
        let response = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .json(payload)
            .send()
            .await
            .map_err(|error| format!("WhatsApp: {error}"))?;
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }
        let body = response.text().await.unwrap_or_default();
        Err(format!("WhatsApp {status}: {body}"))
    }

    /// The message: the code in the template's text, and again behind its "copy code" button.
    pub fn payload(&self, phone: &str, code: &str) -> Value {
        json!({
            "messaging_product": "whatsapp",
            "to": phone.trim_start_matches('+'),
            "type": "template",
            "template": {
                "name": self.template,
                "language": { "code": self.language },
                "components": [
                    { "type": "body", "parameters": [{ "type": "text", "text": code }] },
                    { "type": "button", "sub_type": "url", "index": "0",
                      "parameters": [{ "type": "text", "text": code }] }
                ]
            }
        })
    }

    pub async fn send(&self, phone: &str, code: &str) -> Result<(), String> {
        self.post(&self.payload(phone, code)).await
    }

    /// Sends a notice, if a notice template is set up; false if none is.
    pub async fn send_notice(&self, phone: &str, notice: &Notice) -> Result<bool, String> {
        match self.notice_payload(phone, notice) {
            Some(payload) => self.post(&payload).await.map(|_| true),
            None => Ok(false),
        }
    }

    /// Sends an alert, if its kind's template is set up; false if none is.
    pub async fn send_alert(&self, phone: &str, alert: &AlertTemplate) -> Result<bool, String> {
        match self.alert_payload(phone, alert) {
            Some(payload) => self.post(&payload).await.map(|_| true),
            None => Ok(false),
        }
    }
}

/// E-mail by SMTP: port 587 with STARTTLS, or 465 with TLS from the start.
pub struct Email {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl Email {
    pub fn new(host: &str, port: u16, username: &str, password: &str, from: &str) -> Result<Self, String> {
        let builder = match port {
            465 => AsyncSmtpTransport::<Tokio1Executor>::relay(host),
            _ => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host),
        }
        .map_err(|error| format!("SMTP host: {error}"))?;
        let transport = builder
            .port(port)
            .credentials(Credentials::new(username.to_string(), password.to_string()))
            .timeout(Some(Duration::from_secs(10)))
            .build();
        let from = from.parse().map_err(|error| format!("EMAIL_FROM: {error}"))?;
        Ok(Self { transport, from })
    }

    pub fn message(&self, to: &str, code: &str) -> Result<Message, String> {
        self.text(
            to,
            format!("{code} është kodi yt për Pluribus"),
            format!(
                "Kodi yt për të hyrë në Pluribus:\n\n    {code}\n\nVlen për 10 minuta. \
                 Nëse nuk e kërkove ti, mund ta injorosh këtë email.\n"
            ),
        )
    }

    /// A notice of messages someone hasn't read (Notice): its title the subject.
    pub fn notice_message(&self, to: &str, notice: &Notice) -> Result<Message, String> {
        self.text(to, notice.title(), notice.text())
    }

    fn text(&self, to: &str, subject: String, body: String) -> Result<Message, String> {
        Message::builder()
            .from(self.from.clone())
            .to(to.parse().map_err(|error| format!("address: {error}"))?)
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .map_err(|error| format!("message: {error}"))
    }

    pub async fn send(&self, to: &str, code: &str) -> Result<(), String> {
        self.transport.send(self.message(to, code)?).await.map(|_| ()).map_err(|error| format!("SMTP: {error}"))
    }

    pub async fn send_notice(&self, to: &str, notice: &Notice) -> Result<(), String> {
        self.transport.send(self.notice_message(to, notice)?).await.map(|_| ()).map_err(|error| format!("SMTP: {error}"))
    }

    /// Any e-mail of plain text: the alerts (Alert), the invitations into stores (web/stores.rs).
    pub async fn send_text(&self, to: &str, subject: &str, body: &str) -> Result<(), String> {
        let message = self.text(to, subject.to_string(), body.to_string())?;
        self.transport.send(message).await.map(|_| ()).map_err(|error| format!("SMTP: {error}"))
    }
}

/// The channels set up in the environment.
pub struct Senders {
    pub email: Option<Email>,
    pub whatsapp: Option<WhatsApp>,
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

impl Senders {
    /// E-mail needs SMTP_HOST, SMTP_USERNAME, SMTP_PASSWORD and EMAIL_FROM (SMTP_PORT is 587
    /// unless set); WhatsApp needs WHATSAPP_TOKEN, WHATSAPP_PHONE_NUMBER_ID and WHATSAPP_TEMPLATE
    /// (WHATSAPP_LANGUAGE is "sq", WHATSAPP_API_VERSION "v21.0" unless set). A channel set up only
    /// in part is left off, with the reason in the log.
    pub fn from_env() -> Self {
        let email = match (env("SMTP_HOST"), env("SMTP_USERNAME"), env("SMTP_PASSWORD"), env("EMAIL_FROM")) {
            (Some(host), Some(username), Some(password), Some(from)) => {
                let port = env("SMTP_PORT").and_then(|port| port.parse().ok()).unwrap_or(587);
                Email::new(&host, port, &username, &password, &from)
                    .inspect_err(|error| tracing::error!(%error, "e-mail sign-in codes are off"))
                    .ok()
            }
            (None, None, None, None) => None,
            _ => {
                tracing::error!("e-mail sign-in codes are off: SMTP_HOST, SMTP_USERNAME, SMTP_PASSWORD and EMAIL_FROM go together");
                None
            }
        };
        let whatsapp = match (env("WHATSAPP_TOKEN"), env("WHATSAPP_PHONE_NUMBER_ID"), env("WHATSAPP_TEMPLATE")) {
            (Some(token), Some(phone_number_id), Some(template)) => Some(
                WhatsApp::new(
                    "https://graph.facebook.com",
                    &env("WHATSAPP_API_VERSION").unwrap_or_else(|| "v21.0".to_string()),
                    &phone_number_id,
                    &token,
                    &template,
                    &env("WHATSAPP_LANGUAGE").unwrap_or_else(|| "sq".to_string()),
                )
                .with_notices(env("WHATSAPP_NOTICE_TEMPLATE"))
                .with_alerts(env("WHATSAPP_SEARCH_TEMPLATE"), env("WHATSAPP_EXPIRY_TEMPLATE")),
            ),
            (None, None, None) => None,
            _ => {
                tracing::error!("WhatsApp sign-in codes are off: WHATSAPP_TOKEN, WHATSAPP_PHONE_NUMBER_ID and WHATSAPP_TEMPLATE go together");
                None
            }
        };
        Self { email, whatsapp }
    }

    pub fn channel(&self, contact: &Contact) -> Channel {
        match contact {
            Contact::Email(_) if self.email.is_some() => Channel::Email,
            Contact::Phone(_) if self.whatsapp.is_some() => Channel::WhatsApp,
            _ => Channel::Log,
        }
    }

    pub async fn send(&self, contact: &Contact, code: &str) -> Result<Channel, String> {
        match (contact, &self.email, &self.whatsapp) {
            (Contact::Email(address), Some(email), _) => email.send(address, code).await.map(|_| Channel::Email),
            (Contact::Phone(phone), _, Some(whatsapp)) => whatsapp.send(phone, code).await.map(|_| Channel::WhatsApp),
            _ => {
                tracing::warn!(to = %contact.masked(), code, "sign-in code: this channel isn't set up, so it only goes to this log");
                Ok(Channel::Log)
            }
        }
    }

    /// Sends an e-mail of `subject` and `body` where `contact` is an address and e-mail is set up
    /// (an invitation into a store); a phone number, which has no WhatsApp template for these, or
    /// a channel not set up, goes to the log.
    pub async fn send_text(&self, contact: &Contact, subject: &str, body: &str) -> Result<Channel, String> {
        match (contact, &self.email) {
            (Contact::Email(address), Some(email)) => email.send_text(address, subject, body).await.map(|_| Channel::Email),
            _ => {
                tracing::info!(to = %contact.masked(), subject, "alert: this channel isn't set up, so it only goes to this log");
                Ok(Channel::Log)
            }
        }
    }

    /// Sends an alert to `contact`: by e-mail, or by WhatsApp where its kind's template is set up;
    /// otherwise it goes to the log.
    pub async fn send_alert(&self, contact: &Contact, alert: &Alert) -> Result<Channel, String> {
        match (contact, &self.email, &self.whatsapp) {
            (Contact::Email(address), Some(email), _) => email.send_text(address, &alert.subject, &alert.body).await.map(|_| Channel::Email),
            // The guard sends it, where its template is set up.
            (Contact::Phone(phone), _, Some(whatsapp)) if whatsapp.send_alert(phone, &alert.whatsapp).await? => Ok(Channel::WhatsApp),
            _ => {
                tracing::info!(to = %contact.masked(), subject = %alert.subject, "alert: this channel isn't set up, so it only goes to this log");
                Ok(Channel::Log)
            }
        }
    }

    /// Sends a notice of unread messages to `contact`: by e-mail, or by WhatsApp where its notice
    /// template is set up; otherwise it goes to the log.
    pub async fn send_notice(&self, contact: &Contact, notice: &Notice) -> Result<Channel, String> {
        match (contact, &self.email, &self.whatsapp) {
            (Contact::Email(address), Some(email), _) => email.send_notice(address, notice).await.map(|_| Channel::Email),
            // The guard sends it, where the notice template is set up.
            (Contact::Phone(phone), _, Some(whatsapp)) if whatsapp.send_notice(phone, notice).await? => Ok(Channel::WhatsApp),
            _ => {
                tracing::info!(to = %contact.masked(), url = %notice.url, "message notice: this channel isn't set up, so it only goes to this log");
                Ok(Channel::Log)
            }
        }
    }
}

/// The senders, set up from the environment the first time a code goes out.
pub fn senders() -> &'static Senders {
    static SENDERS: OnceLock<Senders> = OnceLock::new();
    SENDERS.get_or_init(Senders::from_env)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{Json, Router, extract::State, http::HeaderMap, routing::post};

    use super::*;

    type Seen = Arc<Mutex<Option<(String, Value)>>>;

    async fn messages(State(seen): State<Seen>, headers: HeaderMap, Json(body): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
        let auth = headers["authorization"].to_str().unwrap().to_string();
        let unknown = body["to"] == "1000";
        *seen.lock().unwrap() = Some((auth, body));
        if unknown {
            (axum::http::StatusCode::BAD_REQUEST, Json(json!({ "error": { "message": "Invalid parameter" } })))
        } else {
            (axum::http::StatusCode::OK, Json(json!({ "messages": [{ "id": "wamid.1" }] })))
        }
    }

    #[tokio::test]
    async fn whatsapp_sends_the_code_in_an_authentication_template() {
        let seen: Seen = Arc::default();
        let app = Router::new().route("/v21.0/123/messages", post(messages)).with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let whatsapp = WhatsApp::new(&format!("http://{address}"), "v21.0", "123", "secret", "kodi_hyrjes", "sq");
        whatsapp.send("+355691234567", "482910").await.unwrap();
        let (auth, body) = seen.lock().unwrap().take().unwrap();
        assert_eq!(auth, "Bearer secret");
        assert_eq!(body["to"], "355691234567");
        assert_eq!(body["template"]["name"], "kodi_hyrjes");
        assert_eq!(body["template"]["language"]["code"], "sq");
        assert_eq!(body["template"]["components"][0]["parameters"][0]["text"], "482910");
        assert_eq!(body["template"]["components"][1]["sub_type"], "url");
        assert_eq!(body["template"]["components"][1]["parameters"][0]["text"], "482910");

        // Meta refusing it is an error, with Meta's reason.
        let error = whatsapp.send("+1000", "482910").await.unwrap_err();
        assert!(error.contains("400") && error.contains("Invalid parameter"), "{error}");
    }

    #[test]
    fn emails_carry_the_code() {
        let email = Email::new("smtp.example.com", 587, "user", "password", "Pluribus <kodi@example.com>").unwrap();
        let message = String::from_utf8(email.message("ana@example.com", "482910").unwrap().formatted()).unwrap();
        assert!(message.contains("To: ana@example.com"));
        assert!(message.contains("From: Pluribus <kodi@example.com>"));
        assert!(message.contains("482910"));
    }

    fn notice(unread: i64) -> Notice {
        Notice {
            writer: "Besa".into(),
            listing: Some("Volkswagen Golf 1.6, 2016".into()),
            excerpt: "A është ende në shitje?\nMund ta shoh nesër.".into(),
            unread,
            url: "https://pluribus.example/mesazhet/0192f0c1-0000-7000-8000-000000000001".into(),
        }
    }

    #[test]
    fn notices_say_who_wrote_about_what_and_where_to_answer() {
        let email = Email::new("smtp.example.com", 587, "user", "password", "Pluribus <kodi@example.com>").unwrap();
        let message = String::from_utf8(email.notice_message("ana@example.com", &notice(3)).unwrap().formatted()).unwrap();
        assert!(message.contains("To: ana@example.com"));
        // The subject is the notice's title, encoded as e-mail headers are; the text says it all.
        assert_eq!(notice(1).title(), "Besa të shkroi për „Volkswagen Golf 1.6, 2016“");
        let text = notice(3).text();
        assert!(text.starts_with("Besa të shkroi në Pluribus për „Volkswagen Golf 1.6, 2016“:\n\n    A është ende në shitje?\n    Mund ta shoh nesër.\n"), "{text}");
        assert!(text.contains("(dhe 2 mesazhe të tjera të palexuara)"));
        assert!(text.contains("Përgjigju këtu: https://pluribus.example/mesazhet/0192f0c1-0000-7000-8000-000000000001"));
        assert!(text.contains("„Mesazhet që nuk i ke lexuar“ te „Cilësimet“"));
        assert!(!notice(1).text().contains("palexuar"));
        assert!(notice(2).text().contains("(dhe 1 mesazh tjetër i palexuar)"));
        let no_listing = Notice { listing: None, ..notice(1) };
        assert_eq!(no_listing.title(), "Besa të shkroi në Pluribus");
    }

    #[test]
    fn whatsapp_notices_need_their_own_template() {
        let whatsapp = WhatsApp::new("http://localhost", "v21.0", "123", "secret", "kodi_hyrjes", "sq");
        assert!(whatsapp.notice_payload("+355691234567", &notice(1)).is_none());
        let whatsapp = whatsapp.with_notices(Some("mesazh_i_ri".into()));
        let payload = whatsapp.notice_payload("+355691234567", &notice(1)).unwrap();
        assert_eq!(payload["to"], "355691234567");
        assert_eq!(payload["template"]["name"], "mesazh_i_ri");
        assert_eq!(payload["template"]["components"][0]["parameters"][0]["text"], "Besa");
        assert_eq!(payload["template"]["components"][0]["parameters"][1]["text"], "Volkswagen Golf 1.6, 2016");
        assert_eq!(payload["template"]["components"][1]["parameters"][0]["text"], "0192f0c1-0000-7000-8000-000000000001");
    }

    #[test]
    fn whatsapp_alerts_need_their_kinds_templates() {
        let search = AlertTemplate::Search { new: "3 njoftime të reja".into(), name: "Golf në Tiranë".into(), id: "0192f0c1-0000-7000-8000-00000000000a".into() };
        let expiry = AlertTemplate::Expiry { title: "Volkswagen Golf 1.6, 2016".into(), what: "skadon më 16.10.2026".into() };
        let whatsapp = WhatsApp::new("http://localhost", "v21.0", "123", "secret", "kodi_hyrjes", "sq");
        assert!(whatsapp.alert_payload("+355691234567", &search).is_none());
        let whatsapp = whatsapp.with_alerts(Some("kerkim_i_ruajtur".into()), None);
        let payload = whatsapp.alert_payload("+355691234567", &search).unwrap();
        assert_eq!(payload["template"]["name"], "kerkim_i_ruajtur");
        assert_eq!(payload["template"]["components"][0]["parameters"][0]["text"], "3 njoftime të reja");
        assert_eq!(payload["template"]["components"][0]["parameters"][1]["text"], "Golf në Tiranë");
        assert_eq!(payload["template"]["components"][1]["parameters"][0]["text"], "0192f0c1-0000-7000-8000-00000000000a");
        assert!(whatsapp.alert_payload("+355691234567", &expiry).is_none());
        let whatsapp = whatsapp.with_alerts(None, Some("njoftim_skadon".into()));
        let payload = whatsapp.alert_payload("+355691234567", &expiry).unwrap();
        assert_eq!(payload["to"], "355691234567");
        assert_eq!(payload["template"]["name"], "njoftim_skadon");
        assert_eq!(payload["template"]["components"][0]["parameters"][1]["text"], "skadon më 16.10.2026");
        // Its button's address is the same for all: nothing to fill in.
        assert_eq!(payload["template"]["components"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn channels_follow_what_is_set_up() {
        let email = Email::new("smtp.example.com", 587, "user", "password", "kodi@example.com").unwrap();
        let senders = Senders { email: Some(email), whatsapp: None };
        assert_eq!(senders.channel(&Contact::Email("ana@example.com".into())), Channel::Email);
        assert_eq!(senders.channel(&Contact::Phone("+355691234567".into())), Channel::Log);
    }
}
