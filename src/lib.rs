//! Marketplace web application: pages rendered on the server with Askama, enhanced in the
//! browser by Turbo and Stimulus.

pub mod db;
pub mod format;
pub mod sender;
pub mod web;

use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

/// The application's router. `main` serves it; the integration tests call it directly.
pub fn app(db: PgPool) -> axum::Router {
    web::router(AppState { db })
}
