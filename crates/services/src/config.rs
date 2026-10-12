//! The site's settings, every one of them, read once as the server starts (`Config::from_env`, from
//! the environment, which `main` fills from `.env` first) into one struct that the application's
//! state carries (`AppState::config`): the pages, the senders, the photo store, the AI and the work
//! that runs on its own take what they need from it, and nothing else reads the environment. Each
//! setting is named as `.env.example` and README's "Configuration" name it; one left out, or left
//! blank, is at its default, or off. `Config::new` and `Config::for_tests` make one without an
//! environment, for the tests.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use anyhow::Context;

use crate::Moderators;

/// Where the server listens unless BIND_ADDR says otherwise.
pub const DEFAULT_BIND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000);
/// The site's address for what goes out on its own without PUBLIC_URL: the development server's.
pub const DEVELOPMENT_ORIGIN: &str = "http://127.0.0.1:3000";
/// SMTP's port unless SMTP_PORT says otherwise: 587, with STARTTLS.
pub const DEFAULT_SMTP_PORT: u16 = 587;
/// The WhatsApp template's language, and the Graph API's version, unless set.
pub const DEFAULT_WHATSAPP_LANGUAGE: &str = "sq";
pub const DEFAULT_WHATSAPP_API_VERSION: &str = "v21.0";
/// The photo store's region unless S3_REGION says otherwise: "auto", as R2 takes it.
pub const DEFAULT_S3_REGION: &str = "auto";
/// Where pages say the photos are unless MEDIA_URL names a CDN: under /media, which the site
/// serves itself.
pub const DEFAULT_MEDIA_URL: &str = "/media";
/// Haiku: quick and cheap, and it reads listings and photos well. ANTHROPIC_MODEL names another.
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-haiku-4-5-20251001";
/// Anthropic's API, unless ANTHROPIC_BASE_URL names another (the tests' stand-in).
pub const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";
/// Firebase Cloud Messaging's API, unless FCM_BASE_URL names another (the tests' stand-in).
pub const DEFAULT_FCM_URL: &str = "https://fcm.googleapis.com";

/// Every setting of README's "Configuration", typed: `Option` where leaving one out turns
/// something off, its default where it has one. Its `Debug` hides the secrets.
#[derive(Clone, PartialEq)]
pub struct Config {
    /// DATABASE_URL: the PostgreSQL database; required.
    pub database_url: String,
    /// BIND_ADDR: where the server listens (DEFAULT_BIND_ADDR).
    pub bind_addr: SocketAddr,
    /// PUBLIC_URL: the site's address, without a slash at its end, which the links it gives of
    /// itself start with (`site_origin`, pluribus-web's `seo::origin`); None: within a request, the
    /// address it came to.
    pub public_url: Option<String>,
    /// SMTP_HOST, SMTP_PORT (DEFAULT_SMTP_PORT), SMTP_USERNAME, SMTP_PASSWORD and EMAIL_FROM: the
    /// codes, notices and alerts to e-mail addresses (sender.rs), which need all four of the
    /// others.
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub email_from: Option<String>,
    /// WHATSAPP_TOKEN, WHATSAPP_PHONE_NUMBER_ID and WHATSAPP_TEMPLATE: the codes to phone numbers by
    /// WhatsApp, which need all three; WHATSAPP_LANGUAGE and WHATSAPP_API_VERSION, the template's
    /// language and the Graph API's version.
    pub whatsapp_token: Option<String>,
    pub whatsapp_phone_number_id: Option<String>,
    pub whatsapp_template: Option<String>,
    pub whatsapp_language: String,
    pub whatsapp_api_version: String,
    /// WHATSAPP_NOTICE_TEMPLATE, WHATSAPP_SEARCH_TEMPLATE and WHATSAPP_EXPIRY_TEMPLATE: the utility
    /// templates the notices of unread messages, the saved searches' alerts and the expiry's
    /// warnings reach phone numbers by; without one, phone numbers get none of its kind.
    pub whatsapp_notice_template: Option<String>,
    pub whatsapp_search_template: Option<String>,
    pub whatsapp_expiry_template: Option<String>,
    /// S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and S3_SECRET_ACCESS_KEY: the photo store
    /// (media.rs), which needs all four, else photos are kept in memory; S3_REGION, its region.
    pub s3_endpoint: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_access_key_id: Option<String>,
    pub s3_secret_access_key: Option<String>,
    pub s3_region: String,
    /// MEDIA_URL: where pages say the photos are, without a slash at its end.
    pub media_url: String,
    /// SIGN_IN_CODES_ON_SCREEN=true: test mode, the code page shows the code and fills it in
    /// (pluribus-web's account.rs), so anyone can sign in as any number; never for a public site.
    pub sign_in_codes_on_screen: bool,
    /// TYPE_TRIAL=true: the top bar's typeface panel (pluribus-web's `type_trial`), and the
    /// Content-Security-Policy letting Google Fonts and unpkg's icons in.
    pub type_trial: bool,
    /// ANTHROPIC_API_KEY, which turns the AI on (ai.rs); ANTHROPIC_MODEL and ANTHROPIC_BASE_URL.
    pub anthropic_api_key: Option<String>,
    pub anthropic_model: String,
    pub anthropic_base_url: String,
    /// FCM_SERVICE_ACCOUNT, the path to a Firebase service account's key file, which turns push
    /// notifications to the phone app on (push.rs); FCM_BASE_URL, where Firebase Cloud Messaging's
    /// API is.
    pub fcm_service_account: Option<String>,
    pub fcm_base_url: String,
    /// MODERATORS: the phone numbers and e-mail addresses, comma-separated, of those who moderate;
    /// what isn't one is left out.
    pub moderators: Moderators,
    /// CONTACT_EMAIL: the address the contact, privacy and promotions pages give.
    pub contact_email: Option<String>,
    /// OPERATOR: who runs the site, whom the privacy page names.
    pub operator: Option<String>,
}

impl Config {
    /// The settings in the process's environment, which `main` has filled from `.env`: read once,
    /// as the server starts. Without DATABASE_URL, or with a BIND_ADDR that isn't an address, the
    /// server doesn't start.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_lookup(|name| std::env::var(name).ok())
    }

    /// The settings `lookup` gives by their names, as `from_env` reads them from the environment:
    /// most trimmed, and none where blank; a switch on as "true" alone; a port that isn't one
    /// the default.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let setting = |name: &str| lookup(name).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        let switch = |name: &str| lookup(name).is_some_and(|value| value == "true");
        let or = |name: &str, default: &str| setting(name).unwrap_or_else(|| default.to_string());
        let database_url = lookup("DATABASE_URL").context("DATABASE_URL is not set")?;
        let bind_addr = match lookup("BIND_ADDR") {
            Some(addr) => addr.parse().context("BIND_ADDR is not a valid address")?,
            None => DEFAULT_BIND_ADDR,
        };
        Ok(Self {
            database_url,
            bind_addr,
            public_url: setting("PUBLIC_URL").map(|url| url.trim_end_matches('/').to_string()).filter(|url| !url.is_empty()),
            smtp_host: setting("SMTP_HOST"),
            smtp_port: setting("SMTP_PORT").and_then(|port| port.parse().ok()).unwrap_or(DEFAULT_SMTP_PORT),
            smtp_username: setting("SMTP_USERNAME"),
            smtp_password: setting("SMTP_PASSWORD"),
            email_from: setting("EMAIL_FROM"),
            whatsapp_token: setting("WHATSAPP_TOKEN"),
            whatsapp_phone_number_id: setting("WHATSAPP_PHONE_NUMBER_ID"),
            whatsapp_template: setting("WHATSAPP_TEMPLATE"),
            whatsapp_language: or("WHATSAPP_LANGUAGE", DEFAULT_WHATSAPP_LANGUAGE),
            whatsapp_api_version: or("WHATSAPP_API_VERSION", DEFAULT_WHATSAPP_API_VERSION),
            whatsapp_notice_template: setting("WHATSAPP_NOTICE_TEMPLATE"),
            whatsapp_search_template: setting("WHATSAPP_SEARCH_TEMPLATE"),
            whatsapp_expiry_template: setting("WHATSAPP_EXPIRY_TEMPLATE"),
            s3_endpoint: setting("S3_ENDPOINT"),
            s3_bucket: setting("S3_BUCKET"),
            s3_access_key_id: setting("S3_ACCESS_KEY_ID"),
            s3_secret_access_key: setting("S3_SECRET_ACCESS_KEY"),
            s3_region: or("S3_REGION", DEFAULT_S3_REGION),
            media_url: setting("MEDIA_URL").map_or_else(|| DEFAULT_MEDIA_URL.to_string(), |url| url.trim_end_matches('/').to_string()),
            sign_in_codes_on_screen: switch("SIGN_IN_CODES_ON_SCREEN"),
            type_trial: switch("TYPE_TRIAL"),
            anthropic_api_key: setting("ANTHROPIC_API_KEY"),
            anthropic_model: or("ANTHROPIC_MODEL", DEFAULT_ANTHROPIC_MODEL),
            anthropic_base_url: or("ANTHROPIC_BASE_URL", DEFAULT_ANTHROPIC_URL),
            fcm_service_account: setting("FCM_SERVICE_ACCOUNT"),
            fcm_base_url: or("FCM_BASE_URL", DEFAULT_FCM_URL).trim_end_matches('/').to_string(),
            moderators: Moderators::new(lookup("MODERATORS").unwrap_or_default().split(',')),
            contact_email: setting("CONTACT_EMAIL"),
            operator: setting("OPERATOR"),
        })
    }

    /// The settings of an environment that gives DATABASE_URL alone: every other at its default,
    /// so no channel, photo store or AI, no moderators, no public address, no test mode.
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            bind_addr: DEFAULT_BIND_ADDR,
            public_url: None,
            smtp_host: None,
            smtp_port: DEFAULT_SMTP_PORT,
            smtp_username: None,
            smtp_password: None,
            email_from: None,
            whatsapp_token: None,
            whatsapp_phone_number_id: None,
            whatsapp_template: None,
            whatsapp_language: DEFAULT_WHATSAPP_LANGUAGE.to_string(),
            whatsapp_api_version: DEFAULT_WHATSAPP_API_VERSION.to_string(),
            whatsapp_notice_template: None,
            whatsapp_search_template: None,
            whatsapp_expiry_template: None,
            s3_endpoint: None,
            s3_bucket: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_region: DEFAULT_S3_REGION.to_string(),
            media_url: DEFAULT_MEDIA_URL.to_string(),
            sign_in_codes_on_screen: false,
            type_trial: false,
            anthropic_api_key: None,
            anthropic_model: DEFAULT_ANTHROPIC_MODEL.to_string(),
            anthropic_base_url: DEFAULT_ANTHROPIC_URL.to_string(),
            fcm_service_account: None,
            fcm_base_url: DEFAULT_FCM_URL.to_string(),
            moderators: Moderators::default(),
            contact_email: None,
            operator: None,
        }
    }

    /// The tests' settings: `new`'s, without a database's address, their pools coming from
    /// `#[sqlx::test]`. A test that needs another sets it (`Config { type_trial: true,
    /// ..Config::for_tests() }`).
    pub fn for_tests() -> Self {
        Self::new("")
    }

    /// The site's address for links made outside a request, which has none to go by: the notices
    /// of unread messages' (notices.rs), the expiry's e-mails' (expiry.rs), the saved searches'
    /// alerts' and a store's invitations' (pluribus-web). PUBLIC_URL, or the development server's
    /// address; within a request, pluribus-web's `seo::origin` goes by the request's own.
    pub fn site_origin(&self) -> String {
        self.public_url.clone().unwrap_or_else(|| DEVELOPMENT_ORIGIN.to_string())
    }
}

/// Every setting, but the secrets as "…" where they are set: the database's address (with its
/// password), SMTP's password, WhatsApp's token, the photo store's secret key and Anthropic's key,
/// so that a config written to the log gives none of them away. Every field is named below, so
/// that a setting added fails to build until it is shown here, or hidden.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            database_url,
            bind_addr,
            public_url,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            email_from,
            whatsapp_token,
            whatsapp_phone_number_id,
            whatsapp_template,
            whatsapp_language,
            whatsapp_api_version,
            whatsapp_notice_template,
            whatsapp_search_template,
            whatsapp_expiry_template,
            s3_endpoint,
            s3_bucket,
            s3_access_key_id,
            s3_secret_access_key,
            s3_region,
            media_url,
            sign_in_codes_on_screen,
            type_trial,
            anthropic_api_key,
            anthropic_model,
            anthropic_base_url,
            fcm_service_account,
            fcm_base_url,
            moderators,
            contact_email,
            operator,
        } = self;
        let hidden = |secret: &Option<String>| secret.as_ref().map(|_| "…");
        f.debug_struct("Config")
            .field("database_url", &(!database_url.is_empty()).then_some("…"))
            .field("bind_addr", bind_addr)
            .field("public_url", public_url)
            .field("smtp_host", smtp_host)
            .field("smtp_port", smtp_port)
            .field("smtp_username", smtp_username)
            .field("smtp_password", &hidden(smtp_password))
            .field("email_from", email_from)
            .field("whatsapp_token", &hidden(whatsapp_token))
            .field("whatsapp_phone_number_id", whatsapp_phone_number_id)
            .field("whatsapp_template", whatsapp_template)
            .field("whatsapp_language", whatsapp_language)
            .field("whatsapp_api_version", whatsapp_api_version)
            .field("whatsapp_notice_template", whatsapp_notice_template)
            .field("whatsapp_search_template", whatsapp_search_template)
            .field("whatsapp_expiry_template", whatsapp_expiry_template)
            .field("s3_endpoint", s3_endpoint)
            .field("s3_bucket", s3_bucket)
            .field("s3_access_key_id", s3_access_key_id)
            .field("s3_secret_access_key", &hidden(s3_secret_access_key))
            .field("s3_region", s3_region)
            .field("media_url", media_url)
            .field("sign_in_codes_on_screen", sign_in_codes_on_screen)
            .field("type_trial", type_trial)
            .field("anthropic_api_key", &hidden(anthropic_api_key))
            .field("anthropic_model", anthropic_model)
            .field("anthropic_base_url", anthropic_base_url)
            .field("fcm_service_account", fcm_service_account)
            .field("fcm_base_url", fcm_base_url)
            .field("moderators", moderators)
            .field("contact_email", contact_email)
            .field("operator", operator)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    /// An environment of these settings alone, as `from_lookup` reads one.
    fn environment(settings: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let settings: HashMap<String, String> = settings.iter().map(|(name, value)| (name.to_string(), value.to_string())).collect();
        move |name| settings.get(name).cloned()
    }

    #[test]
    fn an_environment_with_the_database_alone_leaves_every_other_setting_at_its_default() {
        let config = Config::from_lookup(environment(&[("DATABASE_URL", "postgres://localhost/pluribus")])).unwrap();
        assert_eq!(config, Config::new("postgres://localhost/pluribus"));
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:3000");
        assert_eq!((config.smtp_port, config.whatsapp_language.as_str(), config.whatsapp_api_version.as_str()), (587, "sq", "v21.0"));
        assert_eq!((config.s3_region.as_str(), config.media_url.as_str()), ("auto", "/media"));
        assert_eq!(config.anthropic_model, "claude-haiku-4-5-20251001");
        assert_eq!(config.anthropic_base_url, "https://api.anthropic.com");
        assert_eq!((config.fcm_service_account.as_deref(), config.fcm_base_url.as_str()), (None, "https://fcm.googleapis.com"));
        assert!(config.moderators.is_empty() && !config.sign_in_codes_on_screen && !config.type_trial);
        assert_eq!(config.site_origin(), "http://127.0.0.1:3000");
    }

    #[test]
    fn every_setting_is_read_once_into_its_field() {
        let config = Config::from_lookup(environment(&[
            ("DATABASE_URL", "postgres://marketplace:db-secret-1@db:5432/marketplace"),
            ("BIND_ADDR", "0.0.0.0:8080"),
            ("PUBLIC_URL", " https://pluribus.al/ "),
            ("SMTP_HOST", "smtp.example.com"),
            ("SMTP_PORT", "465"),
            ("SMTP_USERNAME", "pluribus"),
            ("SMTP_PASSWORD", "smtp-secret-2"),
            ("EMAIL_FROM", "Pluribus <kodi@example.com>"),
            ("WHATSAPP_TOKEN", "wa-secret-3"),
            ("WHATSAPP_PHONE_NUMBER_ID", "123"),
            ("WHATSAPP_TEMPLATE", "kodi_hyrjes"),
            ("WHATSAPP_LANGUAGE", "en"),
            ("WHATSAPP_API_VERSION", "v22.0"),
            ("WHATSAPP_NOTICE_TEMPLATE", "mesazh_i_ri"),
            ("WHATSAPP_SEARCH_TEMPLATE", "kerkim_i_ruajtur"),
            ("WHATSAPP_EXPIRY_TEMPLATE", "njoftim_skadon"),
            ("S3_ENDPOINT", "http://localhost:3900"),
            ("S3_BUCKET", "marketplace-media"),
            ("S3_REGION", "garage"),
            ("S3_ACCESS_KEY_ID", "GK1"),
            ("S3_SECRET_ACCESS_KEY", "s3-secret-4"),
            ("MEDIA_URL", "https://foto.example.al/"),
            ("SIGN_IN_CODES_ON_SCREEN", "true"),
            ("TYPE_TRIAL", "true"),
            ("ANTHROPIC_API_KEY", "ai-secret-5"),
            ("ANTHROPIC_MODEL", "claude-sonnet-4-5"),
            ("ANTHROPIC_BASE_URL", "http://127.0.0.1:9999"),
            ("FCM_SERVICE_ACCOUNT", " /etc/pluribus/firebase.json "),
            ("FCM_BASE_URL", "http://127.0.0.1:9998/"),
            ("MODERATORS", "069 123 4567, Ana@Example.com, jo"),
            ("CONTACT_EMAIL", " info@pluribus.al "),
            ("OPERATOR", "Pluribus sh.p.k."),
        ]))
        .unwrap();
        let expected = Config {
            bind_addr: "0.0.0.0:8080".parse().unwrap(),
            public_url: Some("https://pluribus.al".into()),
            smtp_host: Some("smtp.example.com".into()),
            smtp_port: 465,
            smtp_username: Some("pluribus".into()),
            smtp_password: Some("smtp-secret-2".into()),
            email_from: Some("Pluribus <kodi@example.com>".into()),
            whatsapp_token: Some("wa-secret-3".into()),
            whatsapp_phone_number_id: Some("123".into()),
            whatsapp_template: Some("kodi_hyrjes".into()),
            whatsapp_language: "en".into(),
            whatsapp_api_version: "v22.0".into(),
            whatsapp_notice_template: Some("mesazh_i_ri".into()),
            whatsapp_search_template: Some("kerkim_i_ruajtur".into()),
            whatsapp_expiry_template: Some("njoftim_skadon".into()),
            s3_endpoint: Some("http://localhost:3900".into()),
            s3_bucket: Some("marketplace-media".into()),
            s3_access_key_id: Some("GK1".into()),
            s3_secret_access_key: Some("s3-secret-4".into()),
            s3_region: "garage".into(),
            media_url: "https://foto.example.al".into(),
            sign_in_codes_on_screen: true,
            type_trial: true,
            anthropic_api_key: Some("ai-secret-5".into()),
            anthropic_model: "claude-sonnet-4-5".into(),
            anthropic_base_url: "http://127.0.0.1:9999".into(),
            fcm_service_account: Some("/etc/pluribus/firebase.json".into()),
            fcm_base_url: "http://127.0.0.1:9998".into(),
            moderators: Moderators::new(["+355691234567", "ana@example.com"]),
            contact_email: Some("info@pluribus.al".into()),
            operator: Some("Pluribus sh.p.k.".into()),
            ..Config::new("postgres://marketplace:db-secret-1@db:5432/marketplace")
        };
        assert_eq!(config, expected);
        assert_eq!(config.site_origin(), "https://pluribus.al");
        // Written to the log, it gives no secret away.
        let shown = format!("{config:?}");
        assert!(!shown.contains("-secret-"), "{shown}");
        assert!(shown.contains(r#"smtp_password: Some("…")"#) && shown.contains(r#"public_url: Some("https://pluribus.al")"#), "{shown}");
    }

    #[test]
    fn a_blank_setting_is_left_out_and_a_switch_is_on_as_true_alone() {
        let config = Config::from_lookup(environment(&[
            ("DATABASE_URL", "postgres://localhost/pluribus"),
            ("PUBLIC_URL", " / "),
            ("SMTP_HOST", "  "),
            ("SMTP_PORT", "smtp"),
            ("WHATSAPP_LANGUAGE", ""),
            ("MEDIA_URL", " "),
            ("FCM_SERVICE_ACCOUNT", " "),
            ("FCM_BASE_URL", ""),
            ("SIGN_IN_CODES_ON_SCREEN", "TRUE"),
            ("TYPE_TRIAL", "yes"),
            ("MODERATORS", ""),
            ("OPERATOR", "\t"),
        ]))
        .unwrap();
        assert_eq!(config, Config::new("postgres://localhost/pluribus"));
    }

    #[test]
    fn without_a_database_or_with_an_address_that_isnt_one_the_server_doesnt_start() {
        let missing = Config::from_lookup(environment(&[("BIND_ADDR", "127.0.0.1:3000")])).unwrap_err();
        assert_eq!(missing.to_string(), "DATABASE_URL is not set");
        let wrong = Config::from_lookup(environment(&[("DATABASE_URL", "postgres://localhost/pluribus"), ("BIND_ADDR", "localhost")])).unwrap_err();
        assert_eq!(wrong.to_string(), "BIND_ADDR is not a valid address");
    }
}
