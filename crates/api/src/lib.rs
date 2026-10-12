//! The JSON API the phone app calls, under /api/v1 (`PREFIX`), which the binary nests beside the
//! pages over the same state (pluribus-services' `AppState`), so that the work that runs on its own
//! runs once, and the pages and the API call the same functions, in pluribus-services, pluribus-db
//! and pluribus-core, never the pages themselves (pluribus-web). JSON in (`JsonBody`) and out;
//! every failure answered in one envelope (error.rs); the language each request asks for, never a
//! cookie (lang.rs); a request signed in by a bearer token, a session of the same table the pages'
//! cookie names (auth.rs); the shapes it answers in, made from the rows and never the rows
//! themselves (dto.rs); and its OpenAPI document, which says all of that of every route, the
//! contract the app's client is generated from (openapi.rs). None of the pages' layers apply to
//! it: neither their language cookie, nor the check on forms from other sites, nor their security
//! headers, nor the signed-in cookie, nor the rail's path.

pub mod account;
mod auth;
pub mod browse;
pub mod devices;
mod dto;
mod error;
mod lang;
pub mod listing;
pub mod messages;
pub mod notifications;
pub mod openapi;
pub mod own;
pub mod reference;
pub mod safety;
pub mod saved;
pub mod searches;
pub mod signin;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, FromRequest, OriginalUri, Request, State},
    handler::Handler,
    http::Method,
    middleware,
    routing::{MethodFilter, MethodRouter, on},
};
use pluribus_db as db;
use pluribus_services::{AppState, photos};
use serde::de::DeserializeOwned;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};

pub use auth::{ApiUser, OptionalApiUser};
pub use dto::{CardPage, HealthDto, MeDto, NotificationPage, Page};
pub use error::{ApiError, ErrorBody, ErrorEnvelope};

use openapi::{Internal, Unauthorized};

/// Where the binary nests the API beside the pages: every path of it starts so.
pub const PREFIX: &str = "/api/v1";

/// The API, its paths under PREFIX, where the binary nests it beside the pages once their layers
/// are on: `pluribus_web::router(state.clone()).nest(PREFIX, pluribus_api::router(state))`.
pub fn router(state: AppState) -> Router {
    framed(api(), state)
}

/// Every route of the API, each its method and its path under PREFIX (`/listings/{no}`), from the
/// one table the router is made from too (`table`): what crates/api/tests/openapi.rs holds the
/// OpenAPI document's paths to, every route documented and every documented path a route.
pub fn routes() -> Vec<(Method, &'static str)> {
    table().into_iter().map(|route| (route.method, route.path)).collect()
}

/// The routes of the table, in one router: a path's methods each its own route there, as axum
/// merges those of a path into one.
fn api() -> Router<AppState> {
    table().into_iter().fold(Router::new(), |router, route| router.route(route.path, route.handler))
}

/// One route of the API: the method and the path under PREFIX it answers, and its handler.
struct Route {
    method: Method,
    path: &'static str,
    handler: MethodRouter<AppState>,
}

impl Route {
    /// The route, taking a body of up to `bytes` (`DefaultBodyLimit`) rather than the 2 MB the
    /// others take.
    fn up_to(mut self, bytes: usize) -> Self {
        self.handler = self.handler.layer(DefaultBodyLimit::max(bytes));
        self
    }
}

/// `handler` answering `method` at `path` (a GET's also answering HEAD, as axum's `get` does).
fn route<H: Handler<T, AppState>, T: 'static>(method: Method, path: &'static str, handler: H) -> Route {
    let filter = MethodFilter::try_from(method.clone()).expect("a method a route answers");
    Route { method, path, handler: on(filter, handler) }
}

// The methods the table's routes answer, by their names alone.
const GET: Method = Method::GET;
const POST: Method = Method::POST;
const PUT: Method = Method::PUT;
const PATCH: Method = Method::PATCH;
const DELETE: Method = Method::DELETE;

/// Every route of the API (README, "Routes", "API"), the router and the list of its routes both
/// made from it (`api`, `routes`), each handler documented in the OpenAPI document by its
/// `#[utoipa::path]` (openapi.rs): `GET /health`, for anyone, its OpenAPI document,
/// `GET /openapi.json`, for anyone, and `GET /me`, signed in (`ApiUser`); the reference data the
/// app loads first, for anyone (reference.rs): `GET /categories`, `GET /cities` and
/// `GET /categories/{slug}/fields`; `GET /listings` and `GET /home`, for anyone, a signed-in
/// caller's lists without the listings of the accounts they blocked (browse.rs); one listing
/// (listing.rs), for anyone, its seller and the moderators seeing it off the site too:
/// `GET /listings/{no}`, `GET /listings/{no}/related`, `POST /listings/{no}/seen` and
/// `POST /listings/{no}/contact`; the user's notifications, `GET /notifications` and
/// `POST /notifications/seen`, and what the app's badges count, `GET /counts`, signed in
/// (notifications.rs); signing in by a code (signin.rs): `POST /auth/code` and
/// `POST /auth/verify`, for anyone, and `POST /auth/sign-out`, signed in; the listings a user
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
/// `GET /me/sessions` and `DELETE /me/sessions/others`; and the phone app's devices, which push
/// notifications go to, signed in (devices.rs): `PUT /me/devices` and
/// `DELETE /me/devices/{token}`. And a seller's own listings, signed in
/// (own.rs): `POST /photos` (the body up to photos::UPLOAD_BODY_BYTES, this route's alone),
/// `POST /listings`, `PUT` and `DELETE /listings/{no}`, `POST /listings/{no}/pause`, `/resume`,
/// `/sell`, `/reserve`, `/unreserve` and `/renew`, `GET /me/listings`, `POST /listings/zone` and
/// `GET /listings/price-hint`.
fn table() -> Vec<Route> {
    vec![
        route(GET, "/health", health),
        route(GET, "/openapi.json", openapi::document),
        route(GET, "/me", me),
        route(PATCH, "/me", account::rename),
        route(DELETE, "/me", account::close),
        route(GET, "/me/notices", account::notices),
        route(PUT, "/me/notices", account::set_notices),
        route(PUT, "/me/language", account::language),
        route(POST, "/me/contacts", account::add_contact),
        route(POST, "/me/contacts/verify", account::verify_contact),
        route(DELETE, "/me/contacts/{kind}", account::remove_contact),
        route(GET, "/me/sessions", account::sessions),
        route(DELETE, "/me/sessions/others", account::end_other_sessions),
        route(PUT, "/me/devices", devices::register),
        route(DELETE, "/me/devices/{token}", devices::forget),
        route(GET, "/categories", reference::categories),
        route(GET, "/cities", reference::cities),
        route(GET, "/categories/{slug}/fields", reference::fields),
        route(GET, "/listings", browse::listings),
        route(POST, "/listings", own::create),
        route(GET, "/home", browse::home),
        route(GET, "/listings/{no}", listing::show),
        route(PUT, "/listings/{no}", own::update),
        route(DELETE, "/listings/{no}", own::delete),
        route(GET, "/listings/{no}/related", listing::related),
        route(POST, "/listings/{no}/seen", listing::seen),
        route(POST, "/listings/{no}/contact", listing::contact),
        route(POST, "/photos", own::upload).up_to(photos::UPLOAD_BODY_BYTES),
        route(GET, "/me/listings", own::mine),
        route(POST, "/listings/{no}/pause", own::pause),
        route(POST, "/listings/{no}/resume", own::resume),
        route(POST, "/listings/{no}/sell", own::sell),
        route(POST, "/listings/{no}/reserve", own::reserve),
        route(POST, "/listings/{no}/unreserve", own::unreserve),
        route(POST, "/listings/{no}/renew", own::renew),
        route(POST, "/listings/zone", own::zone),
        route(GET, "/listings/price-hint", own::price_hint),
        route(GET, "/notifications", notifications::list),
        route(POST, "/notifications/seen", notifications::seen),
        route(GET, "/counts", notifications::counts),
        route(POST, "/auth/code", signin::code),
        route(POST, "/auth/verify", signin::verify),
        route(POST, "/auth/sign-out", signin::sign_out),
        route(PUT, "/listings/{no}/saved", saved::keep),
        route(DELETE, "/listings/{no}/saved", saved::let_go),
        route(GET, "/saved", saved::saved),
        route(POST, "/saved/seen", saved::seen),
        route(GET, "/searches", searches::list),
        route(POST, "/searches", searches::save),
        route(DELETE, "/searches/{id}", searches::delete),
        route(PUT, "/searches/{id}/alerts", searches::alerts),
        route(GET, "/searches/{id}/listings", searches::listings),
        route(GET, "/report/reasons", safety::reasons),
        route(POST, "/listings/{no}/report", safety::report),
        route(PUT, "/sellers/{user_id}/blocked", safety::block),
        route(DELETE, "/sellers/{user_id}/blocked", safety::unblock),
        route(GET, "/conversations", messages::inbox),
        route(GET, "/conversations/{id}", messages::conversation),
        route(POST, "/listings/{no}/messages", messages::start),
        route(POST, "/conversations/{id}/messages", messages::reply),
        route(POST, "/conversations/{id}/read", messages::read),
        route(POST, "/conversations/{id}/block", messages::block),
        route(POST, "/conversations/{id}/unblock", messages::unblock),
    ]
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
#[utoipa::path(
    get,
    path = "/health",
    tag = "meta",
    operation_id = "getHealth",
    summary = "Whether the API answers",
    description = "Answers `{\"ok\": true}` once the server and its database answer.",
    responses(
        (status = 200, description = "The server and its database answer", body = HealthDto),
        (status = 500, response = Internal),
    ),
)]
async fn health(State(state): State<AppState>) -> Result<Json<HealthDto>, ApiError> {
    db::ping(&state.db).await?;
    Ok(Json(HealthDto { ok: true }))
}

/// `GET /me`: the signed-in user, as their account has them.
#[utoipa::path(
    get,
    path = "/me",
    tag = "account",
    operation_id = "getMe",
    summary = "The signed-in user",
    description = "The signed-in user, as their account has them.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The signed-in user", body = MeDto),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
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
