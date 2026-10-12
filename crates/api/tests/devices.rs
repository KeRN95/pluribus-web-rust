//! The phone app's devices (crates/api/src/devices.rs), which push notifications go to: a token
//! kept for the user who sent it, seen again as the app starts, moved with a phone that changed
//! accounts, and forgotten by its own user alone; the platform and the token checked, in the
//! language asked for; every route 401 without a token. And push itself, end to end, on the pages'
//! sample listings, against a stand-in for Firebase on a port of the test's own (the service
//! account's key the throwaway fixture of pluribus-services' tests): a message a buyer wrote in the
//! app, left unread, reaching the seller's phone with its notice, as its notification says it; a
//! price lowered as an edit lowers it reaching the phone of whoever keeps the listing, and lowered
//! by the seller's edit from the app (`PUT /listings/{no}`) once that went through; none where
//! push isn't set up, nor for an edit that lowers nothing.

mod common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Form, Json, Router,
    body::Body,
    extract::State,
    http::{StatusCode, header},
    routing::post,
};
use pluribus_services::{
    AppState, Moderators,
    media::Media,
    push::{self, Push, ServiceAccount},
    sender::Senders,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use common::*;

/// The fixtures' people: Ana, who sells the Golf (1001), and Besnik.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";
/// Where the notices' links start.
const ORIGIN: &str = "https://pluribus.example";
/// The throwaway service account of pluribus-services' tests.
const KEY_FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../services/tests/fixtures/fcm-test-key.json");

/// Sends `method` to `path` of `app`, signed in with `token` if one is given, `body` as JSON if one
/// is given.
async fn call_app(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> ApiResponse {
    let request = request(method, path, token);
    let request = match body {
        Some(body) => request.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    send(app.clone(), request.unwrap()).await
}

/// The same to the app as the binary serves it, over the tests' state (no push).
async fn call(pool: &PgPool, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> ApiResponse {
    call_app(&api(pool.clone()), method, path, token, body).await
}

/// `PUT /me/devices` with `platform` and `token`.
async fn register(pool: &PgPool, user_token: &str, platform: &str, token: &str) -> ApiResponse {
    call(pool, "PUT", "/api/v1/me/devices", Some(user_token), Some(json!({ "platform": platform, "token": token }))).await
}

/// The devices' tokens as the database keeps them: each its account, its platform and when it was
/// last seen, by the token.
async fn kept(pool: &PgPool) -> Vec<(String, String, String)> {
    sqlx::query_as("SELECT token, user_id::text, platform FROM device_tokens ORDER BY token").fetch_all(pool).await.unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_device_route_answers_401_without_a_token(pool: PgPool) {
    let unauthorized = json!({"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}});
    let routes = [("PUT", "/api/v1/me/devices", Some(json!({"platform": "ios", "token": "fcm-1"}))), ("DELETE", "/api/v1/me/devices/fcm-1", None)];
    for (method, path, body) in routes {
        for token in [None, Some("not-a-session")] {
            let answer = call(&pool, method, path, token, body.clone()).await;
            assert_eq!((answer.status, answer.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "{method} {path} {token:?}");
        }
    }
    assert!(kept(&pool).await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_device_is_kept_for_its_user_moves_with_its_phone_and_is_forgotten_by_its_own(pool: PgPool) {
    let (ana, besa) = (new_user(&pool, "+355690000001", "Ana").await, new_user(&pool, "+355690000002", "Besa").await);
    let (ana_token, besa_token) = (token_for(&pool, &ana, "ios").await, token_for(&pool, &besa, "android").await);
    let token = "dXf3:APA91bH-test_token-1";
    let answer = register(&pool, &ana_token, "ios", token).await;
    assert_eq!((answer.status, answer.body.as_str()), (StatusCode::NO_CONTENT, ""));
    assert_eq!(kept(&pool).await, [(token.to_string(), ana.clone(), "IOS".to_string())]);
    // Sent again as the app starts, once however often: seen now.
    sqlx::query("UPDATE device_tokens SET last_seen_at = now() - interval '1 day'").execute(&pool).await.unwrap();
    assert_eq!(register(&pool, &ana_token, "ios", &format!(" {token} ")).await.status, StatusCode::NO_CONTENT);
    let fresh: bool = sqlx::query_scalar("SELECT last_seen_at > now() - interval '1 minute' FROM device_tokens").fetch_one(&pool).await.unwrap();
    assert!(fresh);
    assert_eq!(kept(&pool).await.len(), 1);
    // The phone signed in to Besa's account: hers now, and Ana's no more.
    assert_eq!(register(&pool, &besa_token, "android", token).await.status, StatusCode::NO_CONTENT);
    assert_eq!(kept(&pool).await, [(token.to_string(), besa.clone(), "ANDROID".to_string())]);
    // Ana can't take it from her: 204 all the same, and it stays Besa's.
    let path = format!("/api/v1/me/devices/{token}");
    let theirs = call(&pool, "DELETE", &path, Some(&ana_token), None).await;
    assert_eq!(theirs.status, StatusCode::NO_CONTENT);
    assert_eq!(kept(&pool).await.len(), 1);
    // Besa signs out on it: forgotten; and again, nothing to forget, 204.
    for _ in 0..2 {
        let forgotten = call(&pool, "DELETE", &path, Some(&besa_token), None).await;
        assert_eq!((forgotten.status, forgotten.body.as_str()), (StatusCode::NO_CONTENT, ""));
    }
    assert!(kept(&pool).await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_device_needs_the_apps_platform_and_a_token_it_can_hold(pool: PgPool) {
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "android").await;
    let invalid = |fields: Value| json!({"error": {"code": "validation", "message": "Disa fusha nuk janë në rregull.", "fields": fields}});
    let platform = "Duhet të jetë „ios“ ose „android“.";
    let long = "Tokeni i pajisjes duhet të ketë 1 deri në 4096 shenja.";
    for (body, fields) in [
        (json!({"platform": "web", "token": "fcm-1"}), json!({"platform": platform})),
        (json!({"platform": "IOS", "token": "fcm-1"}), json!({"platform": platform})),
        (json!({"platform": "ios", "token": " "}), json!({"token": long})),
        (json!({"platform": "android", "token": "x".repeat(4097)}), json!({"token": long})),
        (json!({"platform": "", "token": ""}), json!({"platform": platform, "token": long})),
    ] {
        let answer = call(&pool, "PUT", "/api/v1/me/devices", Some(&token), Some(body.clone())).await;
        assert_eq!((answer.status, answer.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid(fields)), "{body}");
    }
    // In the language asked for.
    let english = call(&pool, "PUT", "/api/v1/me/devices?lang=en", Some(&token), Some(json!({"platform": "web", "token": ""}))).await;
    let fields = &english.json()["error"]["fields"];
    assert_eq!((fields["platform"].as_str(), fields["token"].as_str()), (Some("Must be “ios” or “android”."), Some("The device’s token needs 1 to 4096 characters.")));
    // Without a token at all, the body isn't what it should be.
    let tokenless = call(&pool, "PUT", "/api/v1/me/devices", Some(&token), Some(json!({"platform": "ios"}))).await;
    assert_eq!((tokenless.status, tokenless.json()["error"]["fields"]["body"].is_string()), (StatusCode::UNPROCESSABLE_ENTITY, true));
    assert!(kept(&pool).await.is_empty());
    // The longest it holds is kept.
    assert_eq!(register(&pool, &token, "android", &"x".repeat(4096)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(kept(&pool).await.len(), 1);
}

/// Firebase as the tests stand it in, on a port of their own: Google's token endpoint, which
/// answers every request with one access token, and FCM's `messages:send` for the fixture's
/// project, which keeps each message it is sent and takes it.
#[derive(Clone, Default)]
struct Firebase(Arc<Mutex<Vec<Value>>>);

impl Firebase {
    /// Serves it; and push to it, by the fixture's service account.
    async fn start() -> (Self, Push) {
        let firebase = Self::default();
        let app = Router::new()
            .route("/token", post(|Form(_): Form<Vec<(String, String)>>| async { Json(json!({"access_token": "token-1", "expires_in": 3599})) }))
            .route(
                "/v1/projects/pluribus-test/messages:send",
                post(|State(firebase): State<Firebase>, Json(request): Json<Value>| async move {
                    firebase.0.lock().unwrap().push(request["message"].clone());
                    Json(json!({"name": "projects/pluribus-test/messages/0:1"}))
                }),
            )
            .with_state(firebase.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let mut account = ServiceAccount::read(KEY_FILE).unwrap();
        account.token_uri = format!("http://{address}/token");
        (firebase, Push::new(account, &format!("http://{address}")).unwrap())
    }

    /// The messages it was sent, in order.
    fn sent(&self) -> Vec<Value> {
        self.0.lock().unwrap().clone()
    }

    /// The messages it was sent once `count` came, a push that goes in the background given up to
    /// five seconds, and a fifth of a second more for any that shouldn't come.
    async fn sent_once(&self, count: usize) -> Vec<Value> {
        for _ in 0..250 {
            if self.sent().len() >= count {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.sent()
    }
}

/// The app as the binary serves it over the tests' state, its push to `push`: the stand-in's.
fn pushing(pool: &PgPool, push: Push) -> Router {
    at_the_root();
    let tests = pluribus_web::state_with(pool.clone(), Media::in_memory(), Moderators::default(), None);
    let state = AppState { senders: Arc::new(Senders::none().with_push(push)), ..tests };
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// Moves what the message notices go by `minutes` into the past, as if that long went by.
async fn minutes_pass(pool: &PgPool, minutes: i32) {
    for (table, column) in [
        ("messages", "created_at"),
        ("notifications", "created_at"),
        ("conversation_participants", "last_read_at"),
        ("conversations", "last_message_at"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {column} = {column} - make_interval(mins => $1)")).bind(minutes).execute(pool).await.unwrap();
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_message_left_unread_reaches_the_sellers_phone_with_its_notice(pool: PgPool) {
    let (firebase, push) = Firebase::start().await;
    let senders = Senders::none().with_push(push);
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);
    assert_eq!(register(&pool, &ana, "android", "phone-ana").await.status, StatusCode::NO_CONTENT);
    // Besnik writes to Ana about her Golf from the app, and she leaves it unread.
    let path = "/api/v1/listings/1001/messages";
    let written = call(&pool, "POST", path, Some(&besnik), Some(json!({"body": "A është ende në shitje?"}))).await;
    assert_eq!(written.status, StatusCode::CREATED, "{}", written.body);
    let conversation = written.json()["conversation_id"].as_str().unwrap().to_string();
    // Not before its notice is due, then once with it.
    assert_eq!(pluribus_services::notices::send_due(&pool, &senders, ORIGIN).await.unwrap(), 0);
    assert!(firebase.sent().is_empty());
    minutes_pass(&pool, 4).await;
    assert_eq!(pluribus_services::notices::send_due(&pool, &senders, ORIGIN).await.unwrap(), 1);
    let sent = firebase.sent();
    assert_eq!(sent.len(), 1, "{sent:#?}");
    // Its title the one her notification keeps, in Albanian as her account is; the message under
    // it; a tap opening the conversation; the icon counting her one notification unread.
    let title: String = sqlx::query_scalar("SELECT title FROM notifications WHERE user_id = $1::uuid AND notification_type = 'NEW_MESSAGE'")
        .bind(ANA)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "Besnik të shkroi për „Volkswagen Golf 7, 2016, naftë“");
    assert_eq!(
        sent[0],
        json!({
            "token": "phone-ana",
            "notification": { "title": title, "body": "A është ende në shitje?" },
            "data": { "kind": "NEW_MESSAGE", "conversation_id": conversation },
            "android": { "priority": "high" },
            "apns": { "payload": { "aps": { "badge": 1, "sound": "default" } } }
        })
    );
    // Its notification, as the app lists it, says the same.
    let listed = get(pool.clone(), "/api/v1/notifications", Some(&ana)).await.json();
    assert_eq!((&listed["items"][0]["title"], &listed["items"][0]["body"]), (&json!(title), &json!("A është ende në shitje?")));
    // The next look sends nothing more; and once she signs out on the phone, nothing reaches it.
    assert_eq!(pluribus_services::notices::send_due(&pool, &senders, ORIGIN).await.unwrap(), 0);
    assert_eq!(call(&pool, "DELETE", "/api/v1/me/devices/phone-ana", Some(&ana), None).await.status, StatusCode::NO_CONTENT);
    let again = call(&pool, "POST", path, Some(&besnik), Some(json!({"body": "Kur mund ta shoh?"}))).await;
    assert_eq!(again.status, StatusCode::CREATED, "{}", again.body);
    minutes_pass(&pool, 70).await;
    assert_eq!(pluribus_services::notices::send_due(&pool, &senders, ORIGIN).await.unwrap(), 1);
    assert_eq!(firebase.sent().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_price_lowered_reaches_the_phone_of_whoever_keeps_the_listing(pool: PgPool) {
    let (firebase, push) = Firebase::start().await;
    at_the_root();
    let tests = pluribus_web::state_with(pool.clone(), Media::in_memory(), Moderators::default(), None);
    let state = AppState { senders: Arc::new(Senders::none().with_push(push)), ..tests.clone() };
    let besnik = token_for(&pool, BESNIK, "ios").await;
    assert_eq!(register(&pool, &besnik, "ios", "phone-besnik").await.status, StatusCode::NO_CONTENT);
    assert_eq!(call(&pool, "PUT", "/api/v1/listings/1001/saved", Some(&besnik), None).await.status, StatusCode::NO_CONTENT);
    // Ana lowers the Golf from 9 500 € to 9 000 €, as an edit does, in one transaction: those it
    // told come back, and once it is through, their phones hear of it.
    let lower = |price: i64| {
        let pool = pool.clone();
        async move {
            let mut tx = pool.begin().await.unwrap();
            let id: Uuid =
                sqlx::query_scalar("UPDATE listings SET price_amount = $1::numeric, price_normalized = $1::numeric WHERE public_no = 1001 RETURNING id")
                    .bind(price)
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap();
            let drops = pluribus_db::notify_price_drop(&mut tx, id).await.unwrap();
            tx.commit().await.unwrap();
            drops
        }
    };
    let drops = lower(9000).await;
    assert_eq!(drops.len(), 1);
    assert_eq!((drops[0].user_id.to_string(), drops[0].public_no, drops[0].language.as_str()), (BESNIK.to_string(), 1001, "sq"));
    push::after_price_drops(&state, drops).expect("a push to send").await.unwrap();
    let sent = firebase.sent();
    assert_eq!(sent.len(), 1, "{sent:#?}");
    assert_eq!(sent[0]["token"], "phone-besnik");
    assert_eq!(sent[0]["notification"], json!({ "title": "Çmimi ra: Volkswagen Golf 7, 2016, naftë" }));
    assert_eq!(sent[0]["data"], json!({ "kind": "PRICE_DROP", "listing_no": "1001" }));
    assert_eq!(sent[0]["apns"]["payload"]["aps"]["badge"], 1);
    // A change that lowers nothing tells no one; and where push isn't set up, there is no push.
    assert!(lower(9000).await.is_empty());
    assert!(push::after_price_drops(&state, Vec::new()).is_none());
    assert!(push::after_price_drops(&tests, lower(8500).await).is_none());
    assert_eq!(firebase.sent().len(), 1);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_price_the_seller_lowers_from_the_app_reaches_the_phone_of_whoever_keeps_the_listing(pool: PgPool) {
    let (firebase, push) = Firebase::start().await;
    let app = pushing(&pool, push);
    let ana = token_for(&pool, &new_user(&pool, "+355691112233", "Ana").await, "ios").await;
    let besnik = token_for(&pool, &new_user(&pool, "+355691112244", "Besnik").await, "android").await;
    // Ana puts her Golf up from the app; Besnik keeps it, the app on his phone.
    let mut golf = json!({
        "category": "makina",
        "listing_type": "SELL",
        "condition": "USED",
        "title": "Volkswagen Golf 7 1.6 TDI, 2017",
        "description": "Makinë e mbajtur mirë, me servis të rregullt dhe pa aksidente.",
        "price_type": "FIXED",
        "price": 11500,
        "currency": "EUR",
        "city": "tirane",
        "properties": {
            "vehicle_make": "VOLKSWAGEN",
            "vehicle_model": "VOLKSWAGEN_GOLF",
            "vehicle_year": 2017,
            "vehicle_mileage": "120 000",
            "vehicle_fuel": "DIESEL",
            "vehicle_transmission": "MANUAL"
        }
    });
    let posted = call_app(&app, "POST", "/api/v1/listings", Some(&ana), Some(golf.clone())).await;
    assert_eq!(posted.status, StatusCode::CREATED, "{}", posted.body);
    let no = posted.json()["no"].as_i64().unwrap();
    let path = format!("/api/v1/listings/{no}");
    let device = json!({"platform": "android", "token": "phone-besnik"});
    assert_eq!(call_app(&app, "PUT", "/api/v1/me/devices", Some(&besnik), Some(device)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(call_app(&app, "PUT", &format!("{path}/saved"), Some(&besnik), None).await.status, StatusCode::NO_CONTENT);
    // An edit that lowers nothing tells no one.
    golf["description"] = json!("Makinë e mbajtur mirë, me servis të rregullt, pa aksidente, me goma të reja.");
    let edited = call_app(&app, "PUT", &path, Some(&ana), Some(golf.clone())).await;
    assert_eq!(edited.status, StatusCode::OK, "{}", edited.body);
    // She lowers it from 11 500 € to 10 900 €: once the edit has gone through, his phone hears of
    // it, as his notification says it, a tap opening the listing.
    golf["price"] = json!(10900);
    let lowered = call_app(&app, "PUT", &path, Some(&ana), Some(golf)).await;
    assert_eq!(lowered.status, StatusCode::OK, "{}", lowered.body);
    let sent = firebase.sent_once(1).await;
    assert_eq!(sent.len(), 1, "{sent:#?}");
    let title: String = sqlx::query_scalar("SELECT title FROM notifications WHERE notification_type = 'PRICE_DROP'").fetch_one(&pool).await.unwrap();
    assert!(title.starts_with("Çmimi ra: Volkswagen Golf"), "{title}");
    assert_eq!(sent[0]["token"], "phone-besnik");
    assert_eq!(sent[0]["notification"], json!({ "title": title }));
    assert_eq!(sent[0]["data"], json!({ "kind": "PRICE_DROP", "listing_no": no.to_string() }));
    assert_eq!(sent[0]["apns"]["payload"]["aps"]["badge"], 1);
}
