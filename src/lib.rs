//! Marketplace web application: pages rendered on the server with Askama, enhanced in the
//! browser by Turbo and Stimulus.

pub mod db;
pub mod format;
pub mod media;
pub mod sender;
pub mod web;

use sqlx::PgPool;

use crate::media::Media;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    /// Where listing photos are kept.
    pub media: Media,
}

/// The application's router. `main` serves it, with photos in the store .env names.
pub fn app(db: PgPool) -> axum::Router {
    app_with_media(db, Media::from_env())
}

/// The router with photos kept in `media`: the integration tests keep theirs in memory.
pub fn app_with_media(db: PgPool, media: Media) -> axum::Router {
    web::router(AppState { db, media })
}
