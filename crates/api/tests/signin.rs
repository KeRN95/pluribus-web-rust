//! Signing in from the app (crates/api/src/signin.rs, through pluribus-services' sign_in.rs): a
//! code asked for by a phone number and by an e-mail address, read from what the senders kept, as
//! a person reads theirs, and verified for a bearer token that `/me` takes until it signs out; the
//! session as the app made it (its client, the phone's name, a year); a wrong code's tries, the
//! code tried out, and one unknown or used; a second code within the minute, five within the hour
//! and twenty from one network, each saying when to ask again; a contact that is neither; a
//! suspended account; the language a request asks for, the account's from then on; test mode.

mod common;

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
};
use pluribus_services::{
    AppState, Moderators,
    config::Config,
    media::Media,
    sender::{Senders, Sent},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

/// The app as the binary serves it, over a state whose senders are the tests' stand-in, which
/// keeps every message it is given rather than sending it (`Senders::recording`): where a test
/// reads its codes, as a person reads theirs, none written into the database by hand.
fn app(pool: &PgPool, senders: &Arc<Senders>) -> Router {
    app_with(pool, senders, Config::for_tests())
}

/// The same with settings of a test's own (test mode).
fn app_with(pool: &PgPool, senders: &Arc<Senders>, config: Config) -> Router {
    at_the_root();
    let state = AppState {
        senders: senders.clone(),
        config: Arc::new(config),
        ..pluribus_web::state_with(pool.clone(), Media::in_memory(), Moderators::default(), None)
    };
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// Posts `body` as JSON to `path` of `app`, signed in with `token` if one is given.
async fn post(app: &Router, path: &str, token: Option<&str>, body: Value) -> ApiResponse {
    let request = request("POST", path, token).header(header::CONTENT_TYPE, "application/json");
    send(app.clone(), request.body(Body::from(body.to_string())).unwrap()).await
}

/// Asks for a code for `contact`, as typed.
async fn ask(app: &Router, contact: &str) -> ApiResponse {
    post(app, "/api/v1/auth/code", None, json!({"contact": contact})).await
}

/// Sends `code` back for the code `code_id` names, from `client`.
async fn verify(app: &Router, code_id: &Value, code: &str, client: &str) -> ApiResponse {
    post(app, "/api/v1/auth/verify", None, json!({"code_id": code_id, "code": code, "client": client})).await
}

/// The latest code the stand-in kept: whom it went to, and the code.
fn last_code(senders: &Senders) -> (String, String) {
    match senders.sent().pop() {
        Some(Sent::Code { to, code }) => (to.as_str().to_string(), code),
        other => panic!("no code kept: {other:?}"),
    }
}

/// The session a token names, as the database keeps it: what made it, the phone's name, and in
/// how many days it ends.
async fn session(pool: &PgPool, token: &str) -> (String, Option<String>, f64) {
    sqlx::query_as(
        "SELECT client, device_name, (extract(epoch FROM expires_at - now()) / 86400)::float8 FROM sessions
         WHERE token_hash = sha256(convert_to($1, 'UTF8'))",
    )
    .bind(token)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_code_to_a_phone_or_an_email_signs_in_for_a_token_until_it_signs_out(pool: PgPool) {
    let senders = Arc::new(Senders::recording());
    let app = app(&pool, &senders);
    let asked = [
        ("069 123 4567", Some("  Erion   Hoxha "), "+355691234567", "+355 69 *** **67", "whatsapp", "ios", "iPhone i Erionit"),
        ("Ana@Example.com", None, "ana@example.com", "a***@example.com", "email", "android", "Pixel 8"),
    ];
    for (typed, name, contact, sent_to, via, client, device) in asked {
        // The code goes out by the contact's channel, the stand-in's WhatsApp for a number and
        // e-mail for an address, and the answer says where as the pages show it; the code itself
        // only the contact gets, outside test mode.
        let sent = post(&app, "/api/v1/auth/code", None, json!({"contact": typed, "name": name})).await;
        assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
        let sent = sent.json();
        assert_eq!((sent["sent_to"].as_str(), sent["via"].as_str()), (Some(sent_to), Some(via)), "{sent}");
        assert!(sent["code_id"].is_string() && sent.get("code").is_none(), "{sent}");
        let (to, code) = last_code(&senders);
        assert_eq!(to, contact);

        // Back with the code, a new account for the contact, signed in: a token, the user as /me
        // gives them, and that the account is new.
        let verified = post(&app, "/api/v1/auth/verify", None, json!({"code_id": sent["code_id"], "code": code, "client": client, "device_name": device})).await;
        assert_eq!(verified.status, StatusCode::OK, "{}", verified.body);
        let verified = verified.json();
        let token = verified["token"].as_str().expect("a token").to_string();
        let (id, created): (String, OffsetDateTime) =
            sqlx::query_as("SELECT id::text, created_at FROM users WHERE phone_e164::text = $1 OR email::text = $1")
                .bind(contact)
                .fetch_one(&pool)
                .await
                .unwrap();
        let user = json!({
            "id": id,
            "display_name": if name.is_some() { "Erion Hoxha" } else { "Përdorues" },
            "phone": contact.starts_with('+').then_some(contact),
            "email": (!contact.starts_with('+')).then_some(contact),
            "preferred_language": "sq",
            "since": created.format(&Rfc3339).unwrap(),
        });
        assert_eq!(verified, json!({"token": token, "user": user, "first_time": true}));
        // Its contact is proven, as signing in on the pages proves it.
        let verified_value: String = sqlx::query_scalar("SELECT verified_value FROM user_verifications WHERE user_id = $1::uuid")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(verified_value, contact);

        // The token signs /me in. The database keeps its hash alone, with the session as the app
        // made it: its client, the phone's name, a year long.
        assert_eq!(token.len(), 64, "{token}");
        let me = get(pool.clone(), "/api/v1/me", Some(&token)).await;
        assert_eq!((me.status, me.json()), (StatusCode::OK, user.clone()));
        let (kept_client, kept_device, days) = session(&pool, &token).await;
        assert_eq!((kept_client.as_str(), kept_device.as_deref()), (client, Some(device)));
        assert!((364.9..365.1).contains(&days), "{days}");

        // The code is used: not found again.
        let again = verify(&app, &sent["code_id"], &code, client).await;
        let used = json!({"error": {"code": "not_found", "message": "Ky kod skadoi ose u përdor. Kërko një kod të ri."}});
        assert_eq!((again.status, again.json()), (StatusCode::NOT_FOUND, used));

        // Signing out ends that session: 204, and the token signs nothing in any more.
        let out = post(&app, "/api/v1/auth/sign-out", Some(&token), json!({})).await;
        assert_eq!((out.status, out.body.as_str()), (StatusCode::NO_CONTENT, ""));
        let after = get(pool.clone(), "/api/v1/me", Some(&token)).await;
        assert_eq!((after.status, after.json()["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("unauthorized")));
        assert_eq!(post(&app, "/api/v1/auth/sign-out", Some(&token), json!({})).await.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(count(&pool, "SELECT count(*) FROM sessions").await, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_wrong_code_counts_its_tries_and_the_sixth_is_refused(pool: PgPool) {
    let senders = Arc::new(Senders::recording());
    let app = app(&pool, &senders);
    let sent = ask(&app, "0691234567").await.json();
    let (_, code) = last_code(&senders);
    let wrong = if code == "000000" { "111111" } else { "000000" };
    // Each wrong guess says how many tries are left, the fifth that it was the last, on `code` and
    // as the envelope's message.
    let tries = [
        (1, "Kodi nuk është i saktë. Të kanë mbetur 4 prova."),
        (2, "Kodi nuk është i saktë. Të kanë mbetur 3 prova."),
        (3, "Kodi nuk është i saktë. Të kanë mbetur 2 prova."),
        (4, "Kodi nuk është i saktë. Të ka mbetur edhe një provë."),
        (5, "Kodi nuk është i saktë, dhe kjo ishte prova e fundit. Kërko një kod të ri."),
    ];
    for (tried, said) in tries {
        let answer = verify(&app, &sent["code_id"], wrong, "android").await;
        let refused = json!({"error": {"code": "validation", "message": said, "fields": {"code": said}}});
        assert_eq!((answer.status, answer.json()), (StatusCode::UNPROCESSABLE_ENTITY, refused), "guess {tried}");
        assert_eq!(count(&pool, "SELECT attempts::int8 FROM login_codes").await, tried);
    }
    // The sixth is refused before it is checked, the right code too: a new code is needed, and
    // nobody is signed in.
    let tried_out = json!({"error": {"code": "rate_limited", "message": "Kodi u provua shumë herë. Kërko një kod të ri."}});
    for typed in [wrong, code.as_str()] {
        let answer = verify(&app, &sent["code_id"], typed, "android").await;
        assert_eq!((answer.status, answer.json()), (StatusCode::TOO_MANY_REQUESTS, tried_out.clone()));
        assert_eq!(answer.header("retry-after"), None);
    }
    assert_eq!(count(&pool, "SELECT attempts::int8 FROM login_codes").await, 5);
    assert_eq!(count(&pool, "SELECT count(*) FROM sessions").await, 0);

    // A code nobody asked for: not found.
    let unknown = verify(&app, &json!("0192f0c1-0000-7000-8000-000000000000"), &code, "android").await;
    assert_eq!((unknown.status, unknown.json()["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));

    // A new code, a minute on, typed with a space: its digits are what count.
    sqlx::query("UPDATE login_codes SET created_at = now() - interval '2 minutes'").execute(&pool).await.unwrap();
    let sent = ask(&app, "069 123 4567").await.json();
    let (_, code) = last_code(&senders);
    let spaced = format!("{} {}", &code[..3], &code[3..]);
    assert_eq!(verify(&app, &sent["code_id"], &spaced, "android").await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn asking_again_too_soon_or_too_often_says_when_to_ask(pool: PgPool) {
    let senders = Arc::new(Senders::recording());
    let app = app(&pool, &senders);
    // Neither a mobile number nor an e-mail address: 422 on `contact`, and nothing goes out.
    let landline = ask(&app, "04 222 3333").await;
    let neither = json!({"error": {
        "code": "validation",
        "message": "Disa fusha nuk janë në rregull.",
        "fields": {"contact": "Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i."},
    }});
    assert_eq!((landline.status, landline.json()), (StatusCode::UNPROCESSABLE_ENTITY, neither));
    assert!(senders.sent().is_empty());

    // One code a minute for a contact: a second within it says in how many seconds one may go, in
    // the envelope and in Retry-After, and goes nowhere.
    assert_eq!(ask(&app, "ana@example.com").await.status, StatusCode::OK);
    let again = ask(&app, "Ana@Example.com").await;
    let body = again.json();
    assert_eq!(again.status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["error"]["code"], "rate_limited");
    assert_eq!(body["error"]["message"], "Të dërguam një kod pak më parë. Prit një minutë para se të kërkosh një tjetër.");
    let wait = body["error"]["retry_after"].as_u64().expect("retry_after");
    assert!((50..=60).contains(&wait), "{wait}");
    assert_eq!(again.header("retry-after"), Some(wait.to_string().as_str()));
    assert_eq!(senders.sent().len(), 1, "the second went nowhere");

    // Five an hour: that code a minute and a half ago and four more within the hour, the earliest
    // fifty minutes ago, so a sixth may go once that one is an hour old, in ten minutes.
    sqlx::query("UPDATE login_codes SET created_at = now() - interval '90 seconds'").execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, expires_at, created_at)
         SELECT 'ana@example.com', decode('00', 'hex'), now(), now() - make_interval(mins => m)
         FROM unnest(ARRAY[50, 40, 30, 20]) m",
    )
    .execute(&pool)
    .await
    .unwrap();
    let hour = ask(&app, "ana@example.com").await;
    let body = hour.json();
    assert_eq!(hour.status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["error"]["message"], "Shumë kërkesa për kode. Provo përsëri më vonë.");
    let wait = body["error"]["retry_after"].as_u64().expect("retry_after");
    assert!((590..=600).contains(&wait), "{wait}");

    // Twenty an hour from one network, the address behind the tunnel (X-Forwarded-For, with no
    // connection's address of its own here): the twenty-first waits for the earliest of the twenty
    // to be an hour old. Besa's code goes from another.
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at, created_at)
         SELECT 'n' || g || '@example.com', decode('00', 'hex'), '203.0.113.7', now(), now() - interval '30 minutes'
         FROM generate_series(1, 20) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let from_network = request("POST", "/api/v1/auth/code", None)
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::from(json!({"contact": "besa@example.com"}).to_string()))
        .unwrap();
    let network = send(app.clone(), from_network).await;
    let body = network.json();
    assert_eq!(network.status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    let wait = body["error"]["retry_after"].as_u64().expect("retry_after");
    assert!((1790..=1800).contains(&wait), "{wait}");
    assert_eq!(ask(&app, "besa@example.com").await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_suspended_account_cannot_sign_in(pool: PgPool) {
    let senders = Arc::new(Senders::recording());
    let app = app(&pool, &senders);
    let id = new_user(&pool, "+355690000001", "Ana").await;
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE id = $1::uuid").bind(&id).execute(&pool).await.unwrap();
    // The code goes, as it does on the pages; right, it signs nobody in: 403, which says why.
    let sent = ask(&app, "069 000 0001").await.json();
    let (_, code) = last_code(&senders);
    let answer = verify(&app, &sent["code_id"], &code, "ios").await;
    let suspended = json!({"error": {"code": "forbidden", "message": "Kjo llogari është pezulluar."}});
    assert_eq!((answer.status, answer.json()), (StatusCode::FORBIDDEN, suspended));
    assert_eq!(answer.header("www-authenticate"), None);
    assert_eq!(count(&pool, "SELECT count(*) FROM sessions").await, 0);
    let status: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1::uuid").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "SUSPENDED");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn verify_makes_the_language_asked_for_the_accounts_and_reads_the_client(pool: PgPool) {
    let senders = Arc::new(Senders::recording());
    let app = app(&pool, &senders);
    let id = new_user(&pool, "+355690000001", "Ana").await;
    // In English (Accept-Language), its answers too.
    let in_english = |path: &str, body: Value| {
        request("POST", path, None)
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT_LANGUAGE, "en-GB,en;q=0.9")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let sent = send(app.clone(), in_english("/api/v1/auth/code", json!({"contact": "069 000 0001"}))).await.json();
    let (_, code) = last_code(&senders);
    // A client the API doesn't know: 422 on `client`, the code not tried.
    let unknown = send(app.clone(), in_english("/api/v1/auth/verify", json!({"code_id": sent["code_id"], "code": code, "client": "windows"}))).await;
    let refused = json!({"error": {"code": "validation", "message": "Some fields aren’t right.", "fields": {"client": "Must be “ios”, “android” or “web”."}}});
    assert_eq!((unknown.status, unknown.json()), (StatusCode::UNPROCESSABLE_ENTITY, refused));
    assert_eq!(count(&pool, "SELECT attempts::int8 FROM login_codes").await, 0);
    let wrong = send(app.clone(), in_english("/api/v1/auth/verify", json!({"code_id": sent["code_id"], "code": "x", "client": "web"}))).await;
    assert_eq!(wrong.json()["error"]["message"], "That code isn’t right. You have 4 tries left.");

    // Right: Ana's account, there already, and English its language now, as /me says; a session
    // of the pages' length for the pages' client, the phone's name tidied and cut to 80 characters.
    let device = format!("  {}  ", "x".repeat(100));
    let verified = send(
        app.clone(),
        in_english("/api/v1/auth/verify", json!({"code_id": sent["code_id"], "code": code, "client": "web", "device_name": device})),
    )
    .await;
    assert_eq!(verified.status, StatusCode::OK, "{}", verified.body);
    let body = verified.json();
    assert_eq!((body["user"]["id"].as_str(), body["user"]["preferred_language"].as_str()), (Some(id.as_str()), Some("en")));
    assert_eq!(body["first_time"], false);
    let language: String = sqlx::query_scalar("SELECT preferred_language FROM users WHERE id = $1::uuid").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(language, "en");
    let (client, device, days) = session(&pool, body["token"].as_str().unwrap()).await;
    assert_eq!((client.as_str(), device), ("web", Some("x".repeat(80))));
    assert!((29.9..30.1).contains(&days), "{days}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn in_test_mode_the_code_comes_back_with_its_id(pool: PgPool) {
    // In test mode (SIGN_IN_CODES_ON_SCREEN), as the pages show the code, the answer gives it.
    let senders = Arc::new(Senders::recording());
    let app = app_with(&pool, &senders, Config { sign_in_codes_on_screen: true, ..Config::for_tests() });
    let sent = ask(&app, "069 123 4567").await.json();
    let (_, code) = last_code(&senders);
    assert_eq!(sent["code"].as_str(), Some(code.as_str()), "{sent}");
    assert_eq!(verify(&app, &sent["code_id"], &code, "ios").await.status, StatusCode::OK);
    // The tests' own state sends by no channel: the code goes to the log, and the answer says so.
    let logged = post_json(pool, "/api/v1/auth/code", None, r#"{"contact": "069 765 4321"}"#).await;
    assert_eq!((logged.status, logged.json()["via"].as_str()), (StatusCode::OK, Some("log")));
}
