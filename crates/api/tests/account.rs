//! The account's settings from the app (crates/api/src/account.rs), each as "Cilësimet" changes
//! it: the name, tidied, cut to 60 characters and never empty; what one hears of, round and back,
//! the e-mail box only for an account with an address; the language, as `/me` gives it; a contact
//! added and changed by a code read from what the senders kept, as a person reads theirs, and
//! the contacts refused (the account's own, another account's, one taken meanwhile), the codes
//! keeping to their purpose; a contact taken off, never the last; the devices signed in, the one
//! asking first, and the others signed out, the page's sessions and the app's alike; closing the
//! account with FSHI, anonymised as the page anonymises it, its photos out of the store and its
//! token dead; and every route 401 without a token.

mod common;

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
};
use pluribus_services::{
    AppState, Moderators,
    media::Media,
    sender::{Senders, Sent},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

/// The app as the binary serves it, over a state whose senders are the tests' stand-in, which
/// keeps every message it is given rather than sending it (`Senders::recording`), and whose photos
/// are kept in `media`, which a test looks in.
fn app(pool: &PgPool, senders: &Arc<Senders>, media: &Media) -> Router {
    at_the_root();
    let state = AppState { senders: senders.clone(), ..pluribus_web::state_with(pool.clone(), media.clone(), Moderators::default(), None) };
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// The app with a stand-in for the senders and photos in memory, and the stand-in.
fn recording(pool: &PgPool) -> (Router, Arc<Senders>) {
    let senders = Arc::new(Senders::recording());
    (app(pool, &senders, &Media::in_memory()), senders)
}

/// Sends `method` to `path` of `app`, signed in with `token` if one is given, `body` as JSON if one
/// is given.
async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> ApiResponse {
    let request = request(method, path, token);
    let request = match body {
        Some(body) => request.header(header::CONTENT_TYPE, "application/json").body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    };
    send(app.clone(), request.unwrap()).await
}

/// The user as `/me` gives them.
async fn me(app: &Router, token: &str) -> Value {
    let me = call(app, "GET", "/api/v1/me", Some(token), None).await;
    assert_eq!(me.status, StatusCode::OK, "{}", me.body);
    me.json()
}

/// The latest code the stand-in kept: whom it went to, and the code.
fn last_code(senders: &Senders) -> (String, String) {
    match senders.sent().pop() {
        Some(Sent::Code { to, code }) => (to.as_str().to_string(), code),
        other => panic!("no code kept: {other:?}"),
    }
}

/// A validation error's envelope: the field's message, and the envelope's own (`said`, as the
/// field's where the account's state refuses, else the code's).
fn invalid(field: &str, message: &str, said: Option<&str>) -> Value {
    json!({"error": {"code": "validation", "message": said.unwrap_or("Disa fusha nuk janë në rregull."), "fields": {field: message}}})
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_account_route_answers_401_without_a_token(pool: PgPool) {
    let (app, senders) = recording(&pool);
    let notices = json!({"messages": true, "price_drops": true, "searches": true, "email": true});
    let routes = [
        ("PATCH", "/api/v1/me", Some(json!({"name": "Ana"}))),
        ("DELETE", "/api/v1/me", Some(json!({"confirm": "FSHI"}))),
        ("GET", "/api/v1/me/notices", None),
        ("PUT", "/api/v1/me/notices", Some(notices)),
        ("PUT", "/api/v1/me/language", Some(json!({"lang": "en"}))),
        ("POST", "/api/v1/me/contacts", Some(json!({"contact": "069 123 4567"}))),
        ("POST", "/api/v1/me/contacts/verify", Some(json!({"code_id": "0192f0c1-0000-7000-8000-000000000000", "code": "123456"}))),
        ("DELETE", "/api/v1/me/contacts/phone", None),
        ("GET", "/api/v1/me/sessions", None),
        ("DELETE", "/api/v1/me/sessions/others", None),
    ];
    let unauthorized = json!({"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}});
    for (method, path, body) in routes {
        for token in [None, Some("not-a-session")] {
            let answer = call(&app, method, path, token, body.clone()).await;
            assert_eq!((answer.status, answer.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "{method} {path} {token:?}");
            assert_eq!(answer.header("www-authenticate"), Some("Bearer"), "{method} {path}");
        }
    }
    assert!(senders.sent().is_empty(), "no code went out");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_name_is_tidied_cut_to_sixty_characters_and_never_empty(pool: PgPool) {
    let (app, _) = recording(&pool);
    let id = new_user(&pool, "+355690000001", "Përdorues").await;
    let token = token_for(&pool, &id, "ios").await;
    // Tidied as the page tidies it: the user, renamed, as /me gives them.
    let renamed = call(&app, "PATCH", "/api/v1/me", Some(&token), Some(json!({"name": "  Ana   Kola "}))).await;
    assert_eq!(renamed.status, StatusCode::OK, "{}", renamed.body);
    assert_eq!(renamed.json()["display_name"], "Ana Kola");
    assert_eq!(renamed.json(), me(&app, &token).await);
    // Its first 60 characters, as many as the page's field holds, the rest cut as the page cuts it.
    let long = call(&app, "PATCH", "/api/v1/me", Some(&token), Some(json!({"name": "ë".repeat(70)}))).await;
    assert_eq!(long.status, StatusCode::OK, "{}", long.body);
    assert_eq!(long.json()["display_name"], "ë".repeat(60));
    // Nothing in it: 422 on `name`, in the page's words, in the language asked for; the name as it was.
    let empty = call(&app, "PATCH", "/api/v1/me", Some(&token), Some(json!({"name": " \t "}))).await;
    assert_eq!((empty.status, empty.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("name", "Shkruaj emrin që do të shohin të tjerët.", None)));
    let english = call(&app, "PATCH", "/api/v1/me?lang=en", Some(&token), Some(json!({"name": ""}))).await;
    assert_eq!(english.json()["error"]["fields"]["name"], "Enter the name others will see.");
    // No name at all: the body isn't what it should be.
    let nameless = call(&app, "PATCH", "/api/v1/me", Some(&token), Some(json!({"emri": "Ana"}))).await;
    assert_eq!((nameless.status, nameless.json()["error"]["fields"]["body"].is_string()), (StatusCode::UNPROCESSABLE_ENTITY, true));
    assert_eq!(me(&app, &token).await["display_name"], "ë".repeat(60));
}

/// What the user hears of, as the database keeps it for the page.
async fn kept_notices(pool: &PgPool, id: &str) -> (bool, bool, bool, bool) {
    sqlx::query_as("SELECT message_alerts, price_drop_alerts, saved_search_alerts, email_enabled FROM notification_preferences WHERE user_id = $1::uuid")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn what_one_hears_of_goes_and_comes_back_as_the_page_keeps_it(pool: PgPool) {
    let (app, _) = recording(&pool);
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "android").await;
    let notices = |messages: bool, price_drops: bool, searches: bool, email: bool| {
        json!({"messages": messages, "price_drops": price_drops, "searches": searches, "email": email})
    };
    // Everything, until the user says otherwise.
    let all = call(&app, "GET", "/api/v1/me/notices", Some(&token), None).await;
    assert_eq!((all.status, all.json()), (StatusCode::OK, notices(true, true, true, true)));

    // An account without an e-mail address has no e-mail box on the page: the rest as sent, that
    // one as it was.
    let set = call(&app, "PUT", "/api/v1/me/notices", Some(&token), Some(notices(true, false, false, false))).await;
    assert_eq!((set.status, set.body.as_str()), (StatusCode::NO_CONTENT, ""));
    let back = call(&app, "GET", "/api/v1/me/notices", Some(&token), None).await.json();
    assert_eq!(back, notices(true, false, false, true));
    assert_eq!(kept_notices(&pool, &id).await, (true, false, false, true));

    // With an address, the e-mail box too, and the page reads what the app set.
    sqlx::query("UPDATE users SET email = 'ana@example.test' WHERE id = $1::uuid").bind(&id).execute(&pool).await.unwrap();
    let set = call(&app, "PUT", "/api/v1/me/notices", Some(&token), Some(notices(false, true, true, false))).await;
    assert_eq!(set.status, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "GET", "/api/v1/me/notices", Some(&token), None).await.json(), notices(false, true, true, false));
    assert_eq!(kept_notices(&pool, &id).await, (false, true, true, false));
    let page = request("GET", "/llogaria/cilesimet", None).header(header::COOKIE, format!("session={token}")).body(Body::empty()).unwrap();
    let page = send(app.clone(), page).await.body;
    assert!(page.contains(r#"<input type="checkbox" name="mesazhet" value="1">"#), "{page}");
    assert!(page.contains(r#"<input type="checkbox" name="kerkimet" value="1" checked>"#) && page.contains(r#"<input type="checkbox" name="email" value="1">"#));

    // Not the same shape: the body isn't what it should be, and nothing changed.
    let short = call(&app, "PUT", "/api/v1/me/notices", Some(&token), Some(json!({"messages": true}))).await;
    assert_eq!((short.status, short.json()["error"]["fields"]["body"].is_string()), (StatusCode::UNPROCESSABLE_ENTITY, true));
    assert_eq!(kept_notices(&pool, &id).await, (false, true, true, false));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_language_set_is_the_accounts_as_me_gives_it(pool: PgPool) {
    let (app, _) = recording(&pool);
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "ios").await;
    assert_eq!(me(&app, &token).await["preferred_language"], "sq");
    let set = call(&app, "PUT", "/api/v1/me/language", Some(&token), Some(json!({"lang": "en"}))).await;
    assert_eq!((set.status, set.body.as_str()), (StatusCode::NO_CONTENT, ""));
    assert_eq!(me(&app, &token).await["preferred_language"], "en");
    let kept: String = sqlx::query_scalar("SELECT preferred_language FROM users WHERE id = $1::uuid").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(kept, "en");
    // A language the site doesn't speak: 422 on `lang`, in the language asked for; the account's as it was.
    let french = call(&app, "PUT", "/api/v1/me/language", Some(&token), Some(json!({"lang": "fr"}))).await;
    assert_eq!((french.status, french.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("lang", "Duhet të jetë „sq“ ose „en“.", None)));
    let english = call(&app, "PUT", "/api/v1/me/language?lang=en", Some(&token), Some(json!({"lang": "EN"}))).await;
    assert_eq!(english.json(), invalid("lang", "Must be “sq” or “en”.", Some("Some fields aren’t right.")));
    assert_eq!(me(&app, &token).await["preferred_language"], "en");
    let back = call(&app, "PUT", "/api/v1/me/language", Some(&token), Some(json!({"lang": "sq"}))).await;
    assert_eq!(back.status, StatusCode::NO_CONTENT);
    assert_eq!(me(&app, &token).await["preferred_language"], "sq");
}

/// The account's contacts and identity as the database keeps them.
async fn account(pool: &PgPool, id: &str) -> (Option<String>, Option<String>, String) {
    sqlx::query_as("SELECT phone_e164::text, email::text, auth_subject FROM users WHERE id = $1::uuid")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// The contact the account's verification of `kind` (PHONE, EMAIL) proves, if it has one.
async fn verified(pool: &PgPool, id: &str, kind: &str) -> Option<String> {
    sqlx::query_scalar("SELECT verified_value FROM user_verifications WHERE user_id = $1::uuid AND verification_type = $2 AND status = 'VERIFIED'")
        .bind(id)
        .bind(kind)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_email_is_added_and_the_phone_changed_by_a_code_sent_to_each(pool: PgPool) {
    let (app, senders) = recording(&pool);
    let id = new_user(&pool, "+355691112233", "Ana").await;
    let token = token_for(&pool, &id, "ios").await;

    // An address for a phone's account: a code goes to it, answered as /auth/code answers.
    let sent = call(&app, "POST", "/api/v1/me/contacts", Some(&token), Some(json!({"contact": "Ana.Kola@Example.test"}))).await;
    assert_eq!(sent.status, StatusCode::OK, "{}", sent.body);
    let sent = sent.json();
    assert_eq!((sent["sent_to"].as_str(), sent["via"].as_str()), (Some("a***@example.test"), Some("email")), "{sent}");
    assert!(sent["code_id"].is_string() && sent.get("code").is_none(), "{sent}");
    let (to, code) = last_code(&senders);
    assert_eq!(to, "ana.kola@example.test");
    // A wrong code, as /auth/verify says it, with the tries left.
    let wrong = if code == "000000" { "111111" } else { "000000" };
    let guess = call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(json!({"code_id": sent["code_id"], "code": wrong}))).await;
    let said = "Kodi nuk është i saktë. Të kanë mbetur 4 prova.";
    assert_eq!((guess.status, guess.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("code", said, Some(said))));
    assert_eq!(account(&pool, &id).await.1, None);

    // Right: the address on the account beside the number, verified as signing in verifies one,
    // and the user as /me gives them now.
    let added = call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(json!({"code_id": sent["code_id"], "code": code}))).await;
    assert_eq!(added.status, StatusCode::OK, "{}", added.body);
    let added = added.json();
    assert_eq!((added["phone"].as_str(), added["email"].as_str()), (Some("+355691112233"), Some("ana.kola@example.test")));
    assert_eq!(added, me(&app, &token).await);
    assert_eq!(verified(&pool, &id, "EMAIL").await.as_deref(), Some("ana.kola@example.test"));
    // Used: not found again.
    let again = call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(json!({"code_id": sent["code_id"], "code": code}))).await;
    let used = json!({"error": {"code": "not_found", "message": "Ky kod skadoi ose u përdor. Kërko një kod të ri."}});
    assert_eq!((again.status, again.json()), (StatusCode::NOT_FOUND, used));

    // A new number in place of the old, by WhatsApp: on the account, verified, the account's identity
    // no longer a contact, and the old number free for whoever signs in with it next.
    let sent = call(&app, "POST", "/api/v1/me/contacts", Some(&token), Some(json!({"contact": "069 111 9999"}))).await.json();
    assert_eq!((sent["sent_to"].as_str(), sent["via"].as_str()), (Some("+355 69 *** **99"), Some("whatsapp")), "{sent}");
    let (to, code) = last_code(&senders);
    assert_eq!(to, "+355691119999");
    let changed = call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(json!({"code_id": sent["code_id"], "code": code}))).await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!((changed.json()["phone"].as_str(), changed.json()["email"].as_str()), (Some("+355691119999"), Some("ana.kola@example.test")));
    assert_eq!(account(&pool, &id).await, (Some("+355691119999".into()), Some("ana.kola@example.test".into()), format!("user:{id}")));
    assert_eq!(verified(&pool, &id, "PHONE").await.as_deref(), Some("+355691119999"));
    let old: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE phone_e164::text = '+355691112233'").fetch_one(&pool).await.unwrap();
    assert_eq!(old, 0);
    // The token goes on signing the account in, with its new number.
    assert_eq!(me(&app, &token).await["phone"], "+355691119999");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_contact_the_account_has_or_another_has_is_refused_and_codes_keep_to_their_purpose(pool: PgPool) {
    let (app, senders) = recording(&pool);
    let ana = new_user(&pool, "+355691112233", "Ana").await;
    let besa = new_user(&pool, "+355691112244", "Besa").await;
    let token = token_for(&pool, &ana, "android").await;
    let add = |contact: &'static str| call(&app, "POST", "/api/v1/me/contacts", Some(&token), Some(json!({"contact": contact})));

    // Neither a mobile number nor an e-mail address, the account's own, another account's: 422 on
    // `contact`, in the page's words, and no code goes out.
    let neither = "Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.";
    assert_eq!(add("04 222 3333").await.json(), invalid("contact", neither, None));
    let own = "Ky është tashmë i llogarisë sate.";
    let answer = add("069 111 2233").await;
    assert_eq!((answer.status, answer.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("contact", own, Some(own))));
    let taken = "Ky është i një llogarie tjetër: hyr me të për ta përdorur.";
    let answer = add("069 111 2244").await;
    assert_eq!((answer.status, answer.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("contact", taken, Some(taken))));
    assert!(senders.sent().is_empty());

    // Within the limits signing in has: a second code within the minute is 429, as /auth/code says it.
    assert_eq!(add("ana@example.test").await.status, StatusCode::OK);
    let (_, code) = last_code(&senders);
    let soon = add("Ana@Example.test").await;
    let body = soon.json();
    assert_eq!(soon.status, StatusCode::TOO_MANY_REQUESTS, "{body}");
    assert_eq!(body["error"]["message"], "Të dërguam një kod pak më parë. Prit një minutë para se të kërkosh një tjetër.");
    let wait = body["error"]["retry_after"].as_u64().expect("retry_after");
    assert!((50..=60).contains(&wait), "{wait}");
    assert_eq!(soon.header("retry-after"), Some(wait.to_string().as_str()));
    assert_eq!(senders.sent().len(), 1);

    // That code signs no one in: /auth/verify finds none, and uses it up.
    let code_id: String = sqlx::query_scalar("SELECT id::text FROM login_codes WHERE contact = 'ana@example.test'").fetch_one(&pool).await.unwrap();
    let signing_in = json!({"code_id": code_id, "code": code, "client": "ios"});
    assert_eq!(call(&app, "POST", "/api/v1/auth/verify", None, Some(signing_in)).await.status, StatusCode::NOT_FOUND);
    let adding = json!({"code_id": code_id, "code": code});
    assert_eq!(call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(adding)).await.status, StatusCode::NOT_FOUND);
    // A code that signs in adds nothing here: as good as none, used up.
    let sign_in = call(&app, "POST", "/api/v1/auth/code", None, Some(json!({"contact": "069 111 7777"}))).await.json();
    let (_, code) = last_code(&senders);
    let adding = json!({"code_id": sign_in["code_id"], "code": code});
    assert_eq!(call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(adding)).await.status, StatusCode::NOT_FOUND);
    let signing_in = json!({"code_id": sign_in["code_id"], "code": code, "client": "ios"});
    assert_eq!(call(&app, "POST", "/api/v1/auth/verify", None, Some(signing_in)).await.status, StatusCode::NOT_FOUND);
    // A code another account asked for adds nothing to this one.
    let besa_token = token_for(&pool, &besa, "ios").await;
    let hers = call(&app, "POST", "/api/v1/me/contacts", Some(&besa_token), Some(json!({"contact": "besa@example.test"}))).await.json();
    let (_, code) = last_code(&senders);
    let adding = json!({"code_id": hers["code_id"], "code": code});
    assert_eq!(call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(adding)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(account(&pool, &ana).await, (Some("+355691112233".into()), None, "phone:+355691112233".into()));
    assert_eq!(account(&pool, &besa).await.1, None);

    // An address another account took while its code was on its way: 422 on `contact`, in the
    // page's words, and the account as it was.
    let sent = add("shared@example.test").await.json();
    let (_, code) = last_code(&senders);
    sqlx::query("UPDATE users SET email = 'shared@example.test' WHERE id = $1::uuid").bind(&besa).execute(&pool).await.unwrap();
    let late = call(&app, "POST", "/api/v1/me/contacts/verify", Some(&token), Some(json!({"code_id": sent["code_id"], "code": code}))).await;
    let meanwhile = "Ndërkohë ky numër ose email u bë i një llogarie tjetër, ndaj nuk u shtua.";
    assert_eq!((late.status, late.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("contact", meanwhile, Some(meanwhile))));
    assert_eq!(me(&app, &token).await["email"], Value::Null);
    assert_eq!(verified(&pool, &ana, "EMAIL").await, None);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_contact_is_taken_off_while_the_other_stays_and_never_the_last(pool: PgPool) {
    let (app, _) = recording(&pool);
    let id = new_user(&pool, "+355691112233", "Ana").await;
    sqlx::query("UPDATE users SET email = 'ana@example.test' WHERE id = $1::uuid").bind(&id).execute(&pool).await.unwrap();
    let token = token_for(&pool, &id, "ios").await;
    let not_found = json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}});
    // A kind that is neither: not found.
    let fax = call(&app, "DELETE", "/api/v1/me/contacts/fax", Some(&token), None).await;
    assert_eq!((fax.status, fax.json()), (StatusCode::NOT_FOUND, not_found.clone()));
    // The number off: the address stays, and the user as /me gives them now.
    let off = call(&app, "DELETE", "/api/v1/me/contacts/phone", Some(&token), None).await;
    assert_eq!(off.status, StatusCode::OK, "{}", off.body);
    assert_eq!((off.json()["phone"].clone(), off.json()["email"].as_str()), (Value::Null, Some("ana@example.test")));
    assert_eq!(off.json(), me(&app, &token).await);
    // A number the account no longer has: not found.
    let gone = call(&app, "DELETE", "/api/v1/me/contacts/phone", Some(&token), None).await;
    assert_eq!((gone.status, gone.json()), (StatusCode::NOT_FOUND, not_found));
    // The address, the last left, which signing in needs: 422 on `kind`, in the language asked
    // for, and it stays.
    let last = call(&app, "DELETE", "/api/v1/me/contacts/email", Some(&token), None).await;
    let needed = "Llogaria jote duhet të ketë të paktën një numër ose email, që të hysh me të.";
    assert_eq!((last.status, last.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("kind", needed, Some(needed))));
    let english = call(&app, "DELETE", "/api/v1/me/contacts/email?lang=en", Some(&token), None).await;
    assert_eq!(english.json()["error"]["message"], "Your account needs at least one number or email to sign in with.");
    assert_eq!(account(&pool, &id).await.1.as_deref(), Some("ana@example.test"));
}

/// When the user's session of `client` that is still good was made and last used, as the API writes
/// times.
async fn times(pool: &PgPool, user: &str, client: &str) -> (Value, Value) {
    let (made, used): (OffsetDateTime, Option<OffsetDateTime>) =
        sqlx::query_as("SELECT created_at, last_used_at FROM sessions WHERE user_id = $1::uuid AND client = $2 AND expires_at > now()")
            .bind(user)
            .bind(client)
            .fetch_one(pool)
            .await
            .unwrap();
    let rfc3339 = |time: OffsetDateTime| json!(time.format(&Rfc3339).unwrap());
    (rfc3339(made), used.map_or(Value::Null, rfc3339))
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_devices_signed_in_are_listed_and_the_others_signed_out_the_pages_and_the_apps_alike(pool: PgPool) {
    let (app, _) = recording(&pool);
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let besa = new_user(&pool, "+355690000002", "Besa").await;
    let phone = token_for(&pool, &id, "ios").await;
    let web = token_for(&pool, &id, "web").await;
    let android = token_for(&pool, &id, "android").await;
    let hers = token_for(&pool, &besa, "web").await;
    // The pages' session used an hour ago; the Android phone's made two days ago and not used
    // since; one ended a day ago, neither listed nor counted.
    for statement in [
        "UPDATE sessions SET device_name = 'iPhone i Anës' WHERE client = 'ios' AND user_id = $1::uuid",
        "UPDATE sessions SET created_at = now() - interval '5 days', last_used_at = now() - interval '1 hour' WHERE client = 'web' AND user_id = $1::uuid",
        "UPDATE sessions SET device_name = 'Pixel 8', created_at = now() - interval '2 days' WHERE client = 'android' AND user_id = $1::uuid",
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at)
         VALUES (sha256(convert_to('ended', 'UTF8')), $1::uuid, now() - interval '31 days', now() - interval '1 day')",
    ] {
        sqlx::query(statement).bind(&id).execute(&pool).await.unwrap();
    }

    // The one asking first, used now (as the request came in), then the latest used first.
    let listed = call(&app, "GET", "/api/v1/me/sessions", Some(&phone), None).await;
    assert_eq!(listed.status, StatusCode::OK, "{}", listed.body);
    let (ios_made, ios_used) = times(&pool, &id, "ios").await;
    let (web_made, web_used) = times(&pool, &id, "web").await;
    let (android_made, android_used) = times(&pool, &id, "android").await;
    assert!(ios_used.is_string() && web_used.is_string() && android_used.is_null());
    let devices = json!([
        {"client": "ios", "device_name": "iPhone i Anës", "created_at": ios_made, "last_used_at": ios_used, "current": true},
        {"client": "web", "device_name": null, "created_at": web_made, "last_used_at": web_used, "current": false},
        {"client": "android", "device_name": "Pixel 8", "created_at": android_made, "last_used_at": null, "current": false},
    ]);
    assert_eq!(listed.json(), devices);
    // Asked from the pages' session, that one is the current one.
    let from_web = call(&app, "GET", "/api/v1/me/sessions", Some(&web), None).await.json();
    let current: Vec<_> = from_web.as_array().unwrap().iter().map(|device| (device["client"].clone(), device["current"].clone())).collect();
    assert_eq!(current, [(json!("web"), json!(true)), (json!("ios"), json!(false)), (json!("android"), json!(false))]);

    // The others signed out, the pages' among them: two that were still signed in (the ended one
    // forgotten too), the one asking alone left; another account's untouched.
    let ended = call(&app, "DELETE", "/api/v1/me/sessions/others", Some(&phone), None).await;
    assert_eq!((ended.status, ended.json()), (StatusCode::OK, json!({"ended": 2})));
    let left = call(&app, "GET", "/api/v1/me/sessions", Some(&phone), None).await.json();
    assert_eq!(left.as_array().map(|left| (left.len(), left[0]["client"].clone(), left[0]["current"].clone())), Some((1, json!("ios"), json!(true))));
    for token in [&web, &android] {
        assert_eq!(call(&app, "GET", "/api/v1/me", Some(token), None).await.status, StatusCode::UNAUTHORIZED);
    }
    let settings = |token: &str| request("GET", "/llogaria/cilesimet", None).header(header::COOKIE, format!("session={token}")).body(Body::empty()).unwrap();
    assert!(send(app.clone(), settings(&web)).await.body.contains(r#"action="/hyr""#), "the page's session ended too");
    let count = |user: String| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sessions WHERE user_id = $1::uuid").bind(user).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!((count(id.clone()).await, count(besa.clone()).await), (1, 1));
    assert_eq!(me(&app, &hers).await["id"], besa.as_str());

    // And the page's own button signs the app out as well: one table holds both.
    let page = token_for(&pool, &id, "web").await;
    let out = request("POST", "/llogaria/pajisjet", None).header(header::COOKIE, format!("session={page}")).body(Body::empty()).unwrap();
    let out = send(app.clone(), out).await;
    assert_eq!(out.header("location"), Some("/llogaria/cilesimet?u=pajisjet"));
    assert_eq!(call(&app, "GET", "/api/v1/me", Some(&phone), None).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn closing_the_account_with_fshi_anonymises_it_as_the_page_does_and_ends_its_token(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &Arc::new(Senders::recording()), &media);
    // Ana, of the sample listings, with her listings' photos in the store.
    let ana = "11111111-1111-4111-8111-111111111111";
    let token = token_for(&pool, ana, "ios").await;
    let photos: Vec<String> = sqlx::query_scalar(
        "SELECT m.storage_key FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.created_by_user_id = $1::uuid",
    )
    .bind(ana)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(!photos.is_empty());
    for key in &photos {
        media.put(key, b"a photo".to_vec()).await.unwrap();
    }

    // Not without FSHI: 422 on `confirm`, nothing closed; nor without a body.
    let meant = "Shkruaj FSHI, që të dimë se e do vërtet.";
    for typed in ["po", "", "DELETE", "FSHIJE"] {
        let refused = call(&app, "DELETE", "/api/v1/me", Some(&token), Some(json!({"confirm": typed}))).await;
        assert_eq!((refused.status, refused.json()), (StatusCode::UNPROCESSABLE_ENTITY, invalid("confirm", meant, None)), "{typed}");
    }
    let bodiless = call(&app, "DELETE", "/api/v1/me", Some(&token), None).await;
    assert_eq!((bodiless.status, bodiless.json()["error"]["fields"]["body"].is_string()), (StatusCode::UNPROCESSABLE_ENTITY, true));
    assert_eq!(me(&app, &token).await["display_name"], "Ana");

    // FSHI, in any case, spaces aside: 204, and the token signs nothing in after.
    let closed = call(&app, "DELETE", "/api/v1/me", Some(&token), Some(json!({"confirm": " fshi "}))).await;
    assert_eq!((closed.status, closed.body.as_str()), (StatusCode::NO_CONTENT, ""));
    let after = call(&app, "GET", "/api/v1/me", Some(&token), None).await;
    assert_eq!((after.status, after.json()["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("unauthorized")));
    // Anonymised as the page's close anonymises it: no number, no address, the deleted name, the
    // identity it signed in with freed; its sessions gone.
    let (name, phone, email, subject, status): (String, Option<String>, Option<String>, String, String) =
        sqlx::query_as("SELECT display_name, phone_e164::text, email::text, auth_subject, status FROM users WHERE id = $1::uuid")
            .bind(ana)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (name.as_str(), phone, email, subject, status.as_str()),
        ("Përdorues i fshirë", None, None, format!("deleted:{ana}"), "DELETED")
    );
    // Her listings deleted, their contacts and photos with them, the files out of the store.
    let (listings, up, contacts, media_rows): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT count(*), count(*) FILTER (WHERE l.status <> 'DELETED'),
                (SELECT count(*) FROM listing_contacts c JOIN listings k ON k.id = c.listing_id WHERE k.created_by_user_id = $1::uuid),
                (SELECT count(*) FROM listing_media m JOIN listings k ON k.id = m.listing_id WHERE k.created_by_user_id = $1::uuid)
         FROM listings l WHERE l.created_by_user_id = $1::uuid",
    )
    .bind(ana)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((listings > 0, up, contacts, media_rows), (true, 0, 0, 0));
    for key in &photos {
        assert!(media.get(key).await.unwrap().is_none(), "{key} is still in the store");
    }
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions WHERE user_id = $1::uuid").bind(ana).fetch_one(&pool).await.unwrap();
    assert_eq!(sessions, 0);
    // Besnik's listings stay up.
    let his: i64 = sqlx::query_scalar("SELECT count(*) FROM listings WHERE created_by_user_id = '22222222-2222-4222-8222-222222222222' AND status = 'ACTIVE'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(his > 0);

    // In English the word is the page's English, DELETE, which FSHI still says too.
    let besnik = token_for(&pool, "22222222-2222-4222-8222-222222222222", "android").await;
    let wrong = call(&app, "DELETE", "/api/v1/me?lang=en", Some(&besnik), Some(json!({"confirm": "yes"}))).await;
    assert_eq!(wrong.json(), invalid("confirm", "Type DELETE so we know you mean it.", Some("Some fields aren’t right.")));
    let deleted = call(&app, "DELETE", "/api/v1/me?lang=en", Some(&besnik), Some(json!({"confirm": "delete"}))).await;
    assert_eq!(deleted.status, StatusCode::NO_CONTENT, "{}", deleted.body);
    assert_eq!(call(&app, "GET", "/api/v1/me", Some(&besnik), None).await.status, StatusCode::UNAUTHORIZED);
}
