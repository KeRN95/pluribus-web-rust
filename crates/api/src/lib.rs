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

pub mod account;
mod auth;
pub mod browse;
mod dto;
mod error;
mod lang;
pub mod listing;
pub mod messages;
pub mod notifications;
pub mod own;
pub mod reference;
pub mod safety;
pub mod saved;
pub mod searches;
pub mod signin;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRequest, OriginalUri, Request, State},
    middleware,
    routing::{delete, get, post, put},
};
use pluribus_db as db;
use pluribus_services::{AppState, photos};
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
/// `GET /categories`, `GET /cities` and `GET /categories/{slug}/fields`; `GET /listings` and
/// `GET /home`, for anyone, a signed-in caller's lists without the listings of the accounts they
/// blocked (browse.rs); one listing (listing.rs), for anyone, its seller and the moderators seeing
/// it off the site too: `GET /listings/{no}`, `GET /listings/{no}/related`,
/// `POST /listings/{no}/seen` and `POST /listings/{no}/contact`; the user's notifications,
/// `GET /notifications` and `POST /notifications/seen`, and what the app's badges count,
/// `GET /counts`, signed in (notifications.rs); signing in by a code (signin.rs): `POST /auth/code`
/// and `POST /auth/verify`, for anyone, and `POST /auth/sign-out`, signed in; the listings a user
/// keeps, signed in (saved.rs): `PUT` and `DELETE /listings/{no}/saved`, `GET /saved` and
/// `POST /saved/seen`; their saved searches, signed in (searches.rs): `GET` and `POST /searches`,
/// `DELETE /searches/{id}`, `PUT /searches/{id}/alerts` and `GET /searches/{id}/listings`; trust
/// and safety (safety.rs): `GET /report/reasons`, for anyone, `POST /listings/{no}/report` and
/// `PUT` and `DELETE /sellers/{user_id}/blocked`, signed in; the messages, signed in
/// (messages.rs): `GET /conversations`, `GET /conversations/{id}`, `POST /listings/{no}/messages`,
/// `POST /conversations/{id}/messages`, `POST /conversations/{id}/read`, and
/// `POST /conversations/{id}/block` and `/unblock`; and the account's settings, signed in
/// (account.rs): `PATCH /me` and `DELETE /me`, `GET` and `PUT /me/notices`, `PUT /me/language`,
/// `POST /me/contacts`, `POST /me/contacts/verify`, `DELETE /me/contacts/{kind}`,
/// `GET /me/sessions` and `DELETE /me/sessions/others`. And a seller's own listings, signed in
/// (own.rs): `POST /photos` (the body up to photos::UPLOAD_BODY_BYTES, this route's alone),
/// `POST /listings`, `PUT` and `DELETE /listings/{no}`, `POST /listings/{no}/pause`, `/resume`,
/// `/sell`, `/reserve`, `/unreserve` and `/renew`, `GET /me/listings`, `POST /listings/zone` and
/// `GET /listings/price-hint`.
fn routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/me", get(me).patch(account::rename).delete(account::close))
        .route("/me/notices", get(account::notices).put(account::set_notices))
        .route("/me/language", put(account::language))
        .route("/me/contacts", post(account::add_contact))
        .route("/me/contacts/verify", post(account::verify_contact))
        .route("/me/contacts/{kind}", delete(account::remove_contact))
        .route("/me/sessions", get(account::sessions))
        .route("/me/sessions/others", delete(account::end_other_sessions))
        .route("/categories", get(reference::categories))
        .route("/cities", get(reference::cities))
        .route("/categories/{slug}/fields", get(reference::fields))
        .route("/listings", get(browse::listings).post(own::create))
        .route("/home", get(browse::home))
        .route("/listings/{no}", get(listing::show).put(own::update).delete(own::delete))
        .route("/listings/{no}/related", get(listing::related))
        .route("/listings/{no}/seen", post(listing::seen))
        .route("/listings/{no}/contact", post(listing::contact))
        .route("/photos", post(own::upload).layer(DefaultBodyLimit::max(photos::UPLOAD_BODY_BYTES)))
        .route("/me/listings", get(own::mine))
        .route("/listings/{no}/pause", post(own::pause))
        .route("/listings/{no}/resume", post(own::resume))
        .route("/listings/{no}/sell", post(own::sell))
        .route("/listings/{no}/reserve", post(own::reserve))
        .route("/listings/{no}/unreserve", post(own::unreserve))
        .route("/listings/{no}/renew", post(own::renew))
        .route("/listings/zone", post(own::zone))
        .route("/listings/price-hint", get(own::price_hint))
        .route("/notifications", get(notifications::list))
        .route("/notifications/seen", post(notifications::seen))
        .route("/counts", get(notifications::counts))
        .route("/auth/code", post(signin::code))
        .route("/auth/verify", post(signin::verify))
        .route("/auth/sign-out", post(signin::sign_out))
        .route("/listings/{no}/saved", put(saved::keep).delete(saved::let_go))
        .route("/saved", get(saved::saved))
        .route("/saved/seen", post(saved::seen))
        .route("/searches", get(searches::list).post(searches::save))
        .route("/searches/{id}", delete(searches::delete))
        .route("/searches/{id}/alerts", put(searches::alerts))
        .route("/searches/{id}/listings", get(searches::listings))
        .route("/report/reasons", get(safety::reasons))
        .route("/listings/{no}/report", post(safety::report))
        .route("/sellers/{user_id}/blocked", put(safety::block).delete(safety::unblock))
        .route("/conversations", get(messages::inbox))
        .route("/conversations/{id}", get(messages::conversation))
        .route("/listings/{no}/messages", post(messages::start))
        .route("/conversations/{id}/messages", post(messages::reply))
        .route("/conversations/{id}/read", post(messages::read))
        .route("/conversations/{id}/block", post(messages::block))
        .route("/conversations/{id}/unblock", post(messages::unblock))
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
