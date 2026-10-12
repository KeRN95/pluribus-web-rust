//! The JSON API the phone app calls, under /api/v1 (`PREFIX`), which the binary nests beside the
//! pages over the same state (pluribus-services' `AppState`), so that the work that runs on its own
//! runs once, and the pages and the API call the same functions, in pluribus-services, pluribus-db
//! and pluribus-core, never the pages themselves (pluribus-web). JSON in (`JsonBody`) and out;
//! every failure answered in one envelope (error.rs); the language each request asks for, never a
//! cookie (lang.rs); a request signed in by a bearer token, a session of the same table the pages'
//! cookie names (auth.rs); the shapes it answers in, made from the rows and never the rows
//! themselves (dto.rs). None of the pages' layers apply to it: neither their language cookie, nor
//! the check on forms from other sites, nor their security headers, nor the signed-in cookie, nor
//! the rail's path.

mod auth;
pub mod browse;
mod dto;
mod error;
mod lang;
pub mod reference;

use axum::{
    Json, Router,
    extract::{FromRequest, OriginalUri, Request, State},
    middleware,
    routing::get,
};
use pluribus_db as db;
use pluribus_services::AppState;
use serde::de::DeserializeOwned;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

pub use auth::{ApiUser, OptionalApiUser};
pub use dto::{HealthDto, MeDto, Page};
pub use error::{ApiError, ErrorBody, ErrorEnvelope};

/// Where the binary nests the API beside the pages: every path of it starts so.
pub const PREFIX: &str = "/api/v1";

/// The API, its paths under PREFIX, where the binary nests it beside the pages once their layers
/// are on: `pluribus_web::router(state.clone()).nest(PREFIX, pluribus_api::router(state))`.
pub fn router(state: AppState) -> Router {
    framed(routes(), state)
}

/// Every route of the API (README, "Routes", "API"): `GET /health`, for anyone, and `GET /me`,
/// signed in (`ApiUser`); the reference data the app loads first, for anyone (reference.rs):
/// `GET /categories`, `GET /cities` and `GET /categories/{slug}/fields`; and `GET /listings` and
/// `GET /home`, for anyone, a signed-in caller's lists without the listings of the accounts they
/// blocked (browse.rs).
fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/me", get(me))
        .route("/categories", get(reference::categories))
        .route("/cities", get(reference::cities))
        .route("/categories/{slug}/fields", get(reference::fields))
        .route("/listings", get(browse::listings))
        .route("/home", get(browse::home))
}

/// Routes in the API's frame, which `router` puts the API's own in, and the tests a probe of
/// theirs: any other path under PREFIX answered as not found in the envelope
/// (`ApiError::NotFound`), not with the pages' 404 page; the language each request asks for, and
/// every answer's `Vary` saying it differs with it and with the token (lang.rs); compression; the
/// requests' log (`span`); and the state.
pub fn framed(routes: Router<AppState>, state: AppState) -> Router {
    routes
        .fallback(not_found)
        .layer(CompressionLayer::new())
        .layer(middleware::from_fn(lang::language))
        .layer(TraceLayer::new_for_http().make_span_with(span))
        .with_state(state)
}

/// A request's span in the log, as tower-http's own but with its whole path (`/api/v1/me`), where
/// the nesting leaves the API's layers the path under PREFIX (`/me`); under tower-http's target,
/// as the pages' are, so that one setting (`tower_http=debug`) logs both.
fn span(request: &Request) -> tracing::Span {
    let uri = request.extensions().get::<OriginalUri>().map_or(request.uri(), |original| &original.0);
    tracing::debug_span!(
        target: "tower_http::trace::make_span",
        "request",
        method = %request.method(),
        uri = %uri,
        version = ?request.version(),
    )
}

/// `GET /health`: `{"ok": true}`, once the database answers.
async fn health(State(state): State<AppState>) -> Result<Json<HealthDto>, ApiError> {
    db::ping(&state.db).await?;
    Ok(Json(HealthDto { ok: true }))
}

/// `GET /me`: the signed-in user, as their account has them.
async fn me(State(state): State<AppState>, ApiUser(user): ApiUser) -> Result<Json<MeDto>, ApiError> {
    let account = db::account_settings(&state.db, user.id).await?;
    Ok(Json(MeDto::from((&user, &account))))
}

/// Any other path under PREFIX.
async fn not_found() -> ApiError {
    ApiError::NotFound
}

/// A request's JSON body, read into `T`, as a handler's last argument. Where axum's own `Json`
/// answers a body it can't read with a status of its own and a line of English, this answers the
/// envelope: a validation error on `body` (`ApiError::from(JsonRejection)`), saying in the
/// request's language whether the body wasn't JSON, wasn't sent as JSON (its `Content-Type`), or
/// lacked a field or had one of the wrong type.
pub struct JsonBody<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> FromRequest<S> for JsonBody<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &S) -> Result<Self, ApiError> {
        let Json(value) = Json::<T>::from_request(request, state).await?;
        Ok(Self(value))
    }
}
