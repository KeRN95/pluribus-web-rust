//! Marketplace web application: pages rendered on the server with Askama, enhanced in the
//! browser by Turbo and Stimulus.

pub mod db;
pub mod format;
pub mod media;
pub mod sender;
pub mod web;

use std::sync::Arc;

use sqlx::PgPool;

use crate::{db::SessionUser, format::Contact, media::Media};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Where listing photos are kept.
    pub media: Media,
    /// Who may approve what sellers type in ("Tjetër"), on /moderim.
    pub moderators: Moderators,
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

/// The application's router. `main` serves it, with photos in the store .env names, and its
/// moderators.
pub fn app(db: PgPool) -> axum::Router {
    app_with(db, Media::from_env(), Moderators::from_env())
}

/// The router with photos kept in `media` and these moderators: the integration tests keep their
/// photos in memory, and name their own moderator.
pub fn app_with(db: PgPool, media: Media, moderators: Moderators) -> axum::Router {
    web::router(AppState { db, media, moderators })
}
