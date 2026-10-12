//! The JSON API the phone app will call, under /api/v1: born empty, filled by Phase C, and not
//! mounted yet. Once it is, the binary nests it beside the pages
//! (`.nest("/api/v1", pluribus_api::router())`), over the same state (pluribus-services'
//! `AppState`); it calls what the pages call, in pluribus-services, pluribus-db and pluribus-core,
//! and never the pages themselves (pluribus-web).

use axum::Router;
use pluribus_services::AppState;

/// The API's routes: none yet, until Phase C.
pub fn router() -> Router<AppState> {
    Router::new()
}
