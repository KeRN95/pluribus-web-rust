//! What the API's tests share. Each file in tests/ is a crate of its own, and takes this in with
//! `mod common;`: the whole app as the binary serves it, the pages with the API nested beside them
//! over one state, its photos kept in memory, no moderator and no AI (`api`), or with routes of a
//! test's own in the API's frame in place of the API's (`api_with`; the probe, `probed`, which
//! reads a JSON body and who signed in); requests to it, signed in by a bearer token or not
//! (`request`, `get`, `post_json`, `send`), and what they answer (`ApiResponse`, its JSON); and
//! accounts with their tokens (`new_user`, `token_for`). Each test gets a fresh database with the
//! migrations applied (`migrator = "MIGRATOR"`), and runs from the repository's root, as the
//! server does (`at_the_root`).

// Each file uses some of what is here, none all of it: what one leaves unused is no warning there.
#![allow(dead_code)]

use std::sync::Once;

use axum::{
    Json, Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header, request::Builder},
    routing::post,
};
use http_body_util::BodyExt;
use pluribus_api::{JsonBody, OptionalApiUser};
use pluribus_services::{AppState, Moderators, media::Media};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// The migrations, embedded once, in pluribus-db, for every test (`migrator = "MIGRATOR"` in each
/// attribute), rather than read and embedded again for each.
pub use pluribus_db::MIGRATOR;

/// The app as the binary serves it: the pages, and the API nested beside them under /api/v1 once
/// the pages' layers are on, over one state (`pluribus_web::state_with`, the tests' state).
pub fn api(pool: PgPool) -> Router {
    let state = state(pool);
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// The same with `routes` in the API's frame in place of the API's own (`pluribus_api::framed`): a
/// test's probe, which sees what the API's routes see (the language asked for, the envelope, the
/// not-found answer for any other path).
pub fn api_with(pool: PgPool, routes: Router<AppState>) -> Router {
    let state = state(pool);
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::framed(routes, state))
}

/// The app with a probe in the API's frame (`api_with`), `POST /api/v1/probe`, which reads a JSON
/// body of two fields (`Probe`, through `JsonBody`) and whoever a bearer token signs in, if anyone
/// (`OptionalApiUser`), and answers what it read: `{"name", "count", "user"}`, the user's id or
/// null.
pub fn probed(pool: PgPool) -> Router {
    api_with(pool, Router::new().route("/probe", post(probe)))
}

/// What the probe reads.
#[derive(Deserialize)]
pub struct Probe {
    pub name: String,
    pub count: u32,
}

async fn probe(OptionalApiUser(user): OptionalApiUser, JsonBody(probe): JsonBody<Probe>) -> Json<Value> {
    Json(json!({"name": probe.name, "count": probe.count, "user": user.map(|user| user.id.to_string())}))
}

/// The tests' state: photos kept in memory, no moderator, no AI.
fn state(pool: PgPool) -> AppState {
    at_the_root();
    pluribus_web::state_with(pool, Media::in_memory(), Moderators::default(), None)
}

/// Makes the repository's root the tests' working directory, once, as the pages' tests do: the
/// site reads static/ from the directory it runs in, which cargo makes the crate's own for its
/// tests.
pub fn at_the_root() {
    static ROOT: Once = Once::new();
    ROOT.call_once(|| {
        std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).expect("the repository's root");
    });
}

/// What a request answered.
pub struct ApiResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: String,
}

impl ApiResponse {
    /// The body as JSON: one that isn't fails the test.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|error| panic!("not JSON ({error}): {}", self.body))
    }

    /// A header's value, if the answer has it.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }

    /// Every `Vary` of the answer, as one list.
    pub fn vary(&self) -> String {
        self.headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap()).collect::<Vec<_>>().join(", ")
    }
}

pub async fn send(app: Router, request: Request<Body>) -> ApiResponse {
    let response = app.oneshot(request).await.unwrap();
    let (status, headers) = (response.status(), response.headers().clone());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    ApiResponse { status, headers, body: String::from_utf8_lossy(&body).into_owned() }
}

/// A request to `path` ("/api/v1/me"), signed in with `token` if one is given, as the app signs
/// its requests in.
pub fn request(method: &str, path: &str, token: Option<&str>) -> Builder {
    let request = Request::builder().method(method).uri(path);
    match token {
        Some(token) => request.header(header::AUTHORIZATION, format!("Bearer {token}")),
        None => request,
    }
}

pub async fn get(pool: PgPool, path: &str, token: Option<&str>) -> ApiResponse {
    send(api(pool), request("GET", path, token).body(Body::empty()).unwrap()).await
}

/// Posts `body` as JSON: its text as it is, so that a test can send what isn't JSON too.
pub async fn post_json(pool: PgPool, path: &str, token: Option<&str>, body: &str) -> ApiResponse {
    let request = request("POST", path, token).header(header::CONTENT_TYPE, "application/json");
    send(api(pool), request.body(Body::from(body.to_string())).unwrap()).await
}

/// A new account with this phone number and name; its id.
pub async fn new_user(pool: &PgPool, phone: &str, name: &str) -> String {
    sqlx::query_scalar("INSERT INTO users (auth_subject, phone_e164, display_name) VALUES ('phone:' || $1, $1, $2) RETURNING id::text")
        .bind(phone)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Signs the user in from `client` ("web", "ios" or "android") by writing a session straight into
/// the database, its token kept as Postgres's own SHA-256 of it, as the pages' tests do
/// (crates/web/tests/common's `session_for`); returns the bearer token.
pub async fn token_for(pool: &PgPool, user_id: &str, client: &str) -> String {
    let token = format!("api-token-{client}-{user_id}");
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at, client)
         VALUES (sha256(convert_to($1, 'UTF8')), $2::uuid, now() + interval '1 day', $3)",
    )
    .bind(&token)
    .bind(user_id)
    .bind(client)
    .execute(pool)
    .await
    .unwrap();
    token
}
