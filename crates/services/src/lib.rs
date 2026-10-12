//! What the site does besides answering a page, which its pages (pluribus-web) and its JSON API
//! (pluribus-api) share: the application's state (`AppState`), who moderates (`Moderators`), how
//! what signs someone in is kept (`hash`), the site's address for what goes out on its own
//! (`site_origin`); photos and their store (media.rs), sending codes, notices and alerts
//! (sender.rs), the AI that fills in the listing form (ai.rs), the euro rate (rates.rs), the views
//! (views.rs), the recommendations (recommended.rs), and the work that runs on its own, which
//! `pluribus_web::state` starts: the notices of unread messages (notices.rs), the listings' expiry
//! (expiry.rs) and the promotions in time (promotions.rs).

pub mod ai;
pub mod expiry;
pub mod media;
pub mod notices;
pub mod promotions;
pub mod rates;
pub mod recommended;
pub mod sender;
pub mod views;

use std::sync::Arc;

use pluribus_core::format::Contact;
use pluribus_db::SessionUser;
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::{ai::Ai, media::Media, recommended::Recommended, views::Views};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Where listing photos are kept.
    pub media: Media,
    /// Who may approve what sellers type in ("Tjetër"), on /moderim.
    pub moderators: Moderators,
    /// The AI that fills in the listing form from what a seller wrote (pluribus-web's
    /// ai_fill.rs); None without ANTHROPIC_API_KEY, and then the form has no "Plotëso pjesën
    /// tjetër me AI".
    pub ai: Option<Ai>,
    /// The listings' pages seen, gathered for the day's statistics (views.rs).
    pub views: Views,
    /// Each visitor's recommended listings, kept for a while (recommended.rs).
    pub recommended: Recommended,
}

/// The people who may approve the makes, models and brands sellers type in when a list lacks
/// theirs: the phone numbers and e-mail addresses they sign in with. Staff roles come from here
/// rather than from the users table.
#[derive(Clone, Default)]
pub struct Moderators(Arc<Vec<Contact>>);

impl Moderators {
    /// Contacts as people type them ("069 123 4567", "ana@example.com"); ones that aren't are
    /// left out.
    pub fn new<'a>(contacts: impl IntoIterator<Item = &'a str>) -> Self {
        Self(Arc::new(contacts.into_iter().filter_map(Contact::parse).collect()))
    }

    /// MODERATORS in .env, comma-separated.
    pub fn from_env() -> Self {
        Self::new(std::env::var("MODERATORS").unwrap_or_default().split(','))
    }

    /// Whether the site has none, and so no one to review listings (db's review.rs).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether this signed-in user is one, by the phone number or address of their account.
    pub fn include(&self, user: &SessionUser) -> bool {
        self.0.iter().any(|contact| match contact {
            Contact::Phone(phone) => user.phone.as_deref() == Some(phone.as_str()),
            Contact::Email(email) => user.email.as_deref().is_some_and(|own| own.eq_ignore_ascii_case(email)),
        })
    }
}

/// SHA-256, as the database keeps what signs someone in (migration 0007), so that a copy of it
/// signs no one in: a session's token (`sessions.token_hash`), the pages' cookie or the phone
/// app's bearer token, and a sign-in code with its contact (`login_codes.code_hash`). The pages
/// (pluribus-web's account.rs) and the API (pluribus-api's auth.rs) hash alike with it.
pub fn hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

/// The site's address for links made outside a request, which has none to go by: the notices of
/// unread messages' (notices.rs), the expiry's e-mails' (expiry.rs), and the saved searches'
/// alerts' and a store's invitations' (pluribus-web). PUBLIC_URL, or the development server's
/// address; within a request, pluribus-web's seo::origin goes by the request's own.
pub fn site_origin() -> String {
    public_url().unwrap_or_else(|| "http://127.0.0.1:3000".to_string())
}

/// The site's address as PUBLIC_URL gives it, without a slash at its end; none where it gives none.
pub fn public_url() -> Option<String> {
    let url = std::env::var("PUBLIC_URL").ok()?;
    let url = url.trim().trim_end_matches('/');
    (!url.is_empty()).then(|| url.to_string())
}
