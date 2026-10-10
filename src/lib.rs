//! The Pluribus web application: pages rendered on the server with Askama, enhanced in the
//! browser by Turbo and Stimulus.

pub mod ai;
pub mod db;
pub mod format;
pub mod media;
pub mod notices;
pub mod sender;
pub mod views;
pub mod web;

use std::sync::Arc;

use sqlx::PgPool;

use crate::{ai::Ai, db::SessionUser, format::Contact, media::Media, views::Views};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Where listing photos are kept.
    pub media: Media,
    /// Who may approve what sellers type in ("Tjetër"), on /moderim.
    pub moderators: Moderators,
    /// The AI that fills in the listing form from what a seller wrote (web/ai_fill.rs); None
    /// without ANTHROPIC_API_KEY, and then the form has no "Plotëso pjesën tjetër me AI".
    pub ai: Option<Ai>,
    /// The listings' pages seen, gathered for the day's statistics (views.rs).
    pub views: Views,
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

    /// Whether this signed-in user is one, by the phone number or address of their account.
    pub fn include(&self, user: &SessionUser) -> bool {
        self.0.iter().any(|contact| match contact {
            Contact::Phone(phone) => user.phone.as_deref() == Some(phone.as_str()),
            Contact::Email(email) => user.email.as_deref().is_some_and(|own| own.eq_ignore_ascii_case(email)),
        })
    }
}

/// The application's router. `main` serves it, with photos in the store .env names (the small and
/// AVIF copies of those kept before there were any made meanwhile), its moderators, the AI if .env
/// has a key for it, the views written every minute, and the notices of unread messages sent as
/// they fall due (notices.rs).
pub fn app(db: PgPool) -> axum::Router {
    let ai = Ai::from_env();
    if ai.is_none() {
        tracing::info!("ANTHROPIC_API_KEY isn't set: the listing form has no AI to fill it in");
    }
    let views = Views::buffered(views::FLUSH_EVERY);
    views.flush_now_and_then(db.clone());
    let media = Media::from_env();
    media.fill_in_copies();
    notices::start(db.clone());
    web::router(AppState { db, media, moderators: Moderators::from_env(), ai, views })
}

/// The router with photos kept in `media` and these moderators: the integration tests keep their
/// photos in memory, and name their own moderator. No AI.
pub fn app_with(db: PgPool, media: Media, moderators: Moderators) -> axum::Router {
    app_with_ai(db, media, moderators, None)
}

/// The same with an AI to fill in the listing form: the tests' stands in for Anthropic's API.
/// Views are written as they come.
pub fn app_with_ai(db: PgPool, media: Media, moderators: Moderators, ai: Option<Ai>) -> axum::Router {
    web::router(AppState { db, media, moderators, ai, views: Views::immediate() })
}
