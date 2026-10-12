//! The messages (crates/api/src/messages.rs), on the pages' sample listings
//! (crates/web/tests/fixtures/listings.sql): a buyer writing to a seller about her listing, and the
//! two inboxes, each with the other side and its unread count; her answer; the messages after one,
//! as the app asks for them every few seconds; what the app showed read, and the counts with it;
//! the limits, each refusal in the envelope as the page says it (three in a row and no links until
//! an answer, an empty or too long message on `body`, eight a minute and five new sellers an hour
//! `rate_limited`); a listing that takes no messages, and one sold that a conversation goes on
//! about; a block and its lifting; a store's conversation shared by its people; and none of it
//! without a token, nor for a stranger. A message written here is one like any other: the pages
//! show it, and its notice goes out.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use pluribus_services::sender::Senders;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

/// The fixtures' people: Ana and Besnik, private sellers, and Dritan, who owns Auto Durrësi.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";
const DRITAN: &str = "33333333-3333-4333-8333-333333333333";
const AUTO_DURRESI: &str = "44444444-4444-4444-8444-444444444444";
/// Ana's Golf (1001), as its card says it.
const GOLF: &str = "Volkswagen Golf 7, 2016, naftë";
/// Where the notices' links start.
const ORIGIN: &str = "https://pluribus.example";

/// Posts `{"body": text}` to `path` (under /api/v1), signed in with `token`.
async fn write(pool: &PgPool, path: &str, token: &str, text: &str) -> ApiResponse {
    post_json(pool.clone(), &format!("/api/v1{path}"), Some(token), &json!({ "body": text }).to_string()).await
}

/// What a write that must go answered: 201 and its JSON.
async fn written(pool: &PgPool, path: &str, token: &str, text: &str) -> Value {
    let answer = write(pool, path, token, text).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{path}: {}", answer.body);
    answer.json()
}

/// The JSON of a GET under /api/v1 that must answer 200.
async fn got(pool: &PgPool, path: &str, token: &str) -> Value {
    let answer = get(pool.clone(), &format!("/api/v1{path}"), Some(token)).await;
    assert_eq!(answer.status, StatusCode::OK, "{path}: {}", answer.body);
    answer.json()
}

/// The user's inbox.
async fn inbox(pool: &PgPool, token: &str) -> Vec<Value> {
    got(pool, "/conversations", token).await.as_array().unwrap().clone()
}

/// The conversation, as the user sees it.
async fn conversation(pool: &PgPool, id: &str, token: &str) -> Value {
    got(pool, &format!("/conversations/{id}"), token).await
}

/// What a conversation says, in its order.
fn bodies(conversation: &Value) -> Vec<String> {
    conversation["messages"].as_array().unwrap().iter().map(|message| message["body"].as_str().unwrap().to_string()).collect()
}

/// How many of the user's conversations have messages unread, as the badges count them.
async fn unread_conversations(pool: &PgPool, token: &str) -> i64 {
    got(pool, "/counts", token).await["messages"].as_i64().unwrap()
}

/// Reads the conversation up to the message `up_to`: 204.
async fn read(pool: &PgPool, id: &str, token: &str, up_to: &str) {
    let path = format!("/api/v1/conversations/{id}/read");
    let answer = post_json(pool.clone(), &path, Some(token), &json!({ "up_to": up_to }).to_string()).await;
    assert_eq!((answer.status, answer.body.as_str()), (StatusCode::NO_CONTENT, ""));
}

/// The envelope a refusal answers with: the status, and the error as the envelope gives it.
fn refused(answer: &ApiResponse) -> (StatusCode, Value) {
    (answer.status, answer.json()["error"].clone())
}

/// A page of the site, for the user `token` signs in, by the same session as the pages' cookie.
async fn page(pool: &PgPool, path: &str, token: &str) -> ApiResponse {
    send(api(pool.clone()), request("GET", path, None).header(header::COOKIE, format!("session={token}")).body(Body::empty()).unwrap()).await
}

/// Moves what the message notices go by `minutes` into the past, as if that long went by.
async fn minutes_pass(pool: &PgPool, minutes: i32) {
    for (table, column) in [
        ("messages", "created_at"),
        ("notifications", "created_at"),
        ("conversation_participants", "last_read_at"),
        ("conversations", "last_message_at"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {column} = {column} - make_interval(mins => $1)"))
            .bind(minutes)
            .execute(pool)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_buyer_writes_to_a_seller_who_answers_and_both_inboxes_have_it(pool: PgPool) {
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);

    // Besnik writes to Ana about her Golf: the message as it went, without space around it, and
    // the conversation it started.
    let first = written(&pool, "/listings/1001/messages", &besnik, "  A është ende në shitje?\r\n").await;
    let id = first["conversation_id"].as_str().expect("its conversation").to_string();
    assert_eq!((&first["mine"], &first["kind"], &first["author"]), (&json!(true), &json!("TEXT"), &Value::Null));
    assert_eq!(first["body"], "A është ende në shitje?");
    let sent_at = OffsetDateTime::parse(first["created_at"].as_str().unwrap(), &Rfc3339).unwrap();

    // Both inboxes have it, each with the other side; hers unread.
    let his = inbox(&pool, &besnik).await;
    assert_eq!(his.len(), 1, "{his:#?}");
    let photo = his[0]["listing"]["photo"].as_str().unwrap();
    assert!(photo.starts_with("https://images.unsplash.com/"), "{photo}");
    assert_eq!(his[0]["listing"], json!({"no": 1001, "title": GOLF, "photo": photo, "status_label": null}));
    assert_eq!(his[0]["other"], json!({"id": ANA, "kind": "user", "name": "Ana", "slug": null}));
    assert_eq!((&his[0]["id"], &his[0]["store"], &his[0]["blocked"]), (&json!(id), &Value::Null, &json!(false)));
    assert_eq!((his[0]["preview"].as_str(), his[0]["unread"].as_i64()), (Some("Ti: A është ende në shitje?"), Some(0)));
    assert_eq!(OffsetDateTime::parse(his[0]["last_message_at"].as_str().unwrap(), &Rfc3339).unwrap(), sent_at);
    let hers = inbox(&pool, &ana).await;
    assert_eq!(hers[0]["other"], json!({"id": BESNIK, "kind": "user", "name": "Besnik", "slug": null}));
    assert_eq!((hers[0]["preview"].as_str(), hers[0]["unread"].as_i64()), (Some("A është ende në shitje?"), Some(1)));
    assert_eq!((unread_conversations(&pool, &ana).await, unread_conversations(&pool, &besnik).await), (1, 0));
    // In English, as an English page says it.
    let english = got(&pool, "/conversations?lang=en", &besnik).await;
    assert_eq!(english[0]["preview"], "You: A është ende në shitje?");

    // A message like any other: the page shows it to her, and left unread a few minutes, she hears
    // of it.
    let thread = page(&pool, &format!("/mesazhet/{id}"), &ana).await;
    assert!(thread.body.contains(r#"<p class="message-body">A është ende në shitje?</p>"#), "{}", thread.body);
    minutes_pass(&pool, 4).await;
    assert_eq!(pluribus_services::notices::send_due(&pool, &Senders::none(), ORIGIN).await.unwrap(), 1);

    // Writing about the listing again goes on in the same conversation.
    let second = written(&pool, "/listings/1001/messages", &besnik, "Kur mund ta shoh?").await;
    assert_eq!(second["conversation_id"], json!(id));

    // She opens it: his two messages, neither hers, from Besnik; she may write. Showing it reads
    // nothing: what the app showed, it says it read, as the page's script does.
    let shown = conversation(&pool, &id, &ana).await;
    assert_eq!(bodies(&shown), ["A është ende në shitje?", "Kur mund ta shoh?"]);
    assert!(shown["messages"].as_array().unwrap().iter().all(|message| message["mine"] == false), "{shown:#}");
    assert_eq!(shown["other"], json!({"id": BESNIK, "kind": "user", "name": "Besnik", "slug": null}));
    let expected = json!({"blocked_by_me": false, "blocked_me": false, "closed": false, "can_write": true, "why_not": null, "store": null});
    for (key, value) in expected.as_object().unwrap() {
        assert_eq!(&shown[key], value, "{key}");
    }
    assert_eq!(inbox(&pool, &ana).await[0]["unread"], 2);
    let ids: Vec<String> = shown["messages"].as_array().unwrap().iter().map(|message| message["id"].as_str().unwrap().to_string()).collect();
    read(&pool, &id, &ana, &ids[0]).await;
    assert_eq!((inbox(&pool, &ana).await[0]["unread"].as_i64(), unread_conversations(&pool, &ana).await), (Some(1), 1));
    read(&pool, &id, &ana, &ids[1]).await;
    assert_eq!((inbox(&pool, &ana).await[0]["unread"].as_i64(), unread_conversations(&pool, &ana).await), (Some(0), 0));

    // She answers: the message alone, hers.
    let answer = written(&pool, &format!("/conversations/{id}/messages"), &ana, "Po, nesër pasdite.").await;
    assert_eq!((&answer["mine"], &answer["body"]), (&json!(true), &json!("Po, nesër pasdite.")));
    assert!(answer.get("conversation_id").is_none(), "{answer}");
    let theirs = conversation(&pool, &id, &besnik).await;
    assert_eq!(bodies(&theirs), ["A është ende në shitje?", "Kur mund ta shoh?", "Po, nesër pasdite."]);
    let mine: Vec<&Value> = theirs["messages"].as_array().unwrap().iter().map(|message| &message["mine"]).collect();
    assert_eq!(mine, [&json!(true), &json!(true), &json!(false)]);
    let his = inbox(&pool, &besnik).await;
    assert_eq!((his[0]["preview"].as_str(), his[0]["unread"].as_i64()), (Some("Po, nesër pasdite."), Some(1)));
    assert_eq!(unread_conversations(&pool, &besnik).await, 1);

    // What came after a message, as the app asks every few seconds; after the last, none.
    let newer = got(&pool, &format!("/conversations/{id}?after={}", ids[0]), &besnik).await;
    assert_eq!(bodies(&newer), ["Kur mund ta shoh?", "Po, nesër pasdite."]);
    let last = answer["id"].as_str().unwrap();
    assert_eq!(got(&pool, &format!("/conversations/{id}?after={last}"), &besnik).await["messages"], json!([]));
    // Read, his count goes too.
    read(&pool, &id, &besnik, last).await;
    assert_eq!((inbox(&pool, &besnik).await[0]["unread"].as_i64(), unread_conversations(&pool, &besnik).await), (Some(0), 0));

    // An `after` that isn't a message's id: the envelope, on `after`.
    let wrong = get(pool.clone(), &format!("/api/v1/conversations/{id}?after=soon"), Some(&besnik)).await;
    let envelope = json!({
        "code": "validation",
        "message": "Disa fusha nuk janë në rregull.",
        "fields": {"after": "Vazhdimi i listës nuk lexohet. Fillo përsëri nga faqja e parë."},
    });
    assert_eq!(refused(&wrong), (StatusCode::UNPROCESSABLE_ENTITY, envelope));

    // A photo kept on the site itself, its address absolute, from the host the request named.
    sqlx::query("UPDATE listing_media SET thumbnail_url = '/media/photos/golf-card.webp' FROM listings l WHERE l.id = listing_id AND l.public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    let asked = request("GET", "/api/v1/conversations", Some(&besnik)).header(header::HOST, "pluribus.test").body(Body::empty()).unwrap();
    let listed = send(api(pool.clone()), asked).await.json();
    assert_eq!(listed[0]["listing"]["photo"], "http://pluribus.test/media/photos/golf-card.webp");

    // Dritan, no side of it, finds nothing there, whatever he asks; nor anything in his inbox.
    let dritan = token_for(&pool, DRITAN, "android").await;
    let not_found = json!({"code": "not_found", "message": "Ajo që kërkove nuk u gjet."});
    let answers = [
        get(pool.clone(), &format!("/api/v1/conversations/{id}"), Some(&dritan)).await,
        write(&pool, &format!("/conversations/{id}/messages"), &dritan, "Hej").await,
        post_json(pool.clone(), &format!("/api/v1/conversations/{id}/read"), Some(&dritan), &json!({"up_to": last}).to_string()).await,
        post_json(pool.clone(), &format!("/api/v1/conversations/{id}/block"), Some(&dritan), "").await,
        post_json(pool.clone(), &format!("/api/v1/conversations/{id}/unblock"), Some(&dritan), "").await,
        get(pool.clone(), "/api/v1/conversations/jo-nje-bisede", Some(&dritan)).await,
    ];
    for answer in &answers {
        assert_eq!(refused(answer), (StatusCode::NOT_FOUND, not_found.clone()), "{}", answer.body);
    }
    assert!(inbox(&pool, &dritan).await.is_empty());
    // Nor did asking make him one of its sides.
    let joined: i64 = sqlx::query_scalar("SELECT count(*) FROM conversation_participants WHERE user_id = $1::uuid")
        .bind(DRITAN)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(joined, 0);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn until_the_seller_answers_three_in_a_row_and_no_links_each_refusal_as_the_page_says_it(pool: PgPool) {
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);
    let validation = |message: &str, fields: Value| json!({"code": "validation", "message": message, "fields": fields});

    // A first message with a link, empty, or too long goes nowhere, with why: what is wrong with
    // the message itself on `body` too.
    let links = "Lidhjet mund t'i dërgosh pasi Ana të të përgjigjet.";
    let refusal = refused(&write(&pool, "/listings/1001/messages", &besnik, "Shiko www.example.com").await);
    assert_eq!(refusal, (StatusCode::UNPROCESSABLE_ENTITY, validation(links, json!({}))));
    let empty = "Shkruaj mesazhin.";
    let refusal = refused(&write(&pool, "/listings/1001/messages", &besnik, " \r\n ").await);
    assert_eq!(refusal, (StatusCode::UNPROCESSABLE_ENTITY, validation(empty, json!({"body": empty}))));
    let long = "Mesazhi mund të ketë deri në 1000 shkronja.";
    let refusal = refused(&write(&pool, "/listings/1001/messages", &besnik, &"a".repeat(1001)).await);
    assert_eq!(refusal, (StatusCode::UNPROCESSABLE_ENTITY, validation(long, json!({"body": long}))));
    // In the language asked for, as an English page says them.
    let refusal = refused(&write(&pool, "/listings/1001/messages?lang=en", &besnik, "").await);
    let english = validation("Write your message.", json!({"body": "Write your message."}));
    assert_eq!(refusal, (StatusCode::UNPROCESSABLE_ENTITY, english));
    // And a body that isn't one: the envelope, on `body`.
    let answer = post_json(pool.clone(), "/api/v1/listings/1001/messages", Some(&besnik), r#"{"text": "Alo?"}"#).await;
    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(answer.json()["error"]["fields"]["body"].is_string(), "{}", answer.body);
    assert!(inbox(&pool, &besnik).await.is_empty(), "nothing started");

    // Three in a row go, links aside; the fourth waits for her answer.
    let id = written(&pool, "/listings/1001/messages", &besnik, "Përshëndetje").await["conversation_id"].as_str().unwrap().to_string();
    let into = format!("/conversations/{id}/messages");
    written(&pool, &into, &besnik, "A është ende në shitje?").await;
    let refusal = refused(&write(&pool, &into, &besnik, "https://example.com/golf").await);
    assert_eq!(refusal, (StatusCode::UNPROCESSABLE_ENTITY, validation(links, json!({}))));
    written(&pool, &into, &besnik, "Kur mund ta shoh?").await;
    let fourth = refused(&write(&pool, &into, &besnik, "Alo?").await);
    let wait = "Prit që Ana të të përgjigjet para se të shkruash më shumë.";
    assert_eq!(fourth, (StatusCode::UNPROCESSABLE_ENTITY, validation(wait, json!({}))));
    // Through the listing too, into the same conversation, by its limits.
    assert_eq!(refused(&write(&pool, "/listings/1001/messages", &besnik, "Alo?").await).1["message"], wait);
    // Not a block: he may write, once she answers.
    assert_eq!(conversation(&pool, &id, &besnik).await["can_write"], true);
    assert_eq!(bodies(&conversation(&pool, &id, &ana).await), ["Përshëndetje", "A është ende në shitje?", "Kur mund ta shoh?"]);

    // Once she answers, he writes on, links and all.
    written(&pool, &into, &ana, "Po, nesër pasdite.").await;
    written(&pool, &into, &besnik, "Mirë. Ja ku jam: https://maps.example.com/x").await;
    written(&pool, &into, &besnik, "Alo?").await;
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn eight_messages_a_minute_and_five_new_sellers_an_hour_then_wait(pool: PgPool) {
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);
    let id = written(&pool, "/listings/1001/messages", &besnik, "Përshëndetje").await["conversation_id"].as_str().unwrap().to_string();
    let into = format!("/conversations/{id}/messages");
    written(&pool, &into, &ana, "Përshëndetje! Po, është.").await;

    // Eight in a minute go; the ninth waits a minute, `rate_limited`, saying how long.
    for n in 2..=8 {
        written(&pool, &into, &besnik, &format!("Mesazhi {n}")).await;
    }
    let ninth = write(&pool, &into, &besnik, "Mesazhi 9").await;
    let fast = json!({
        "code": "rate_limited",
        "message": "Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.",
        "retry_after": 60,
    });
    assert_eq!(refused(&ninth), (StatusCode::TOO_MANY_REQUESTS, fast));
    assert_eq!(ninth.header("retry-after"), Some("60"));
    // A minute on, it goes.
    minutes_pass(&pool, 2).await;
    written(&pool, &into, &besnik, "Mesazhi 9").await;

    // Five new sellers in the hour (Ana, and four more): a sixth waits, his message to her going
    // on all the same.
    sqlx::query(
        "INSERT INTO conversations (buyer_user_id, seller_user_id, created_at, last_message_at)
         SELECT $1::uuid, $2::uuid, now() - interval '10 minutes', now() - interval '10 minutes' FROM generate_series(1, 4)",
    )
    .bind(BESNIK)
    .bind(DRITAN)
    .execute(&pool)
    .await
    .unwrap();
    let sixth = write(&pool, "/listings/1002/messages", &besnik, "Përshëndetje").await;
    let sellers = json!({
        "code": "rate_limited",
        "message": "U ke shkruar 5 shitësve këtë orë. Prit pak para se t'i shkruash një tjetri.",
    });
    assert_eq!(refused(&sixth), (StatusCode::TOO_MANY_REQUESTS, sellers));
    assert_eq!(sixth.header("retry-after"), None);
    written(&pool, &into, &besnik, "Mesazhi 10").await;
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_listing_that_takes_no_messages_and_one_sold_a_conversation_goes_on_about(pool: PgPool) {
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);
    let closed = json!({"code": "validation", "message": "Ky njoftim nuk pranon mesazhe tani.", "fields": {}});

    // Her own Golf, her iPhone paused, her sofa with its messages turned off: none takes one.
    assert_eq!(refused(&write(&pool, "/listings/1001/messages", &ana, "Alo?").await), (StatusCode::UNPROCESSABLE_ENTITY, closed.clone()));
    sqlx::query("UPDATE listings SET status = 'PAUSED' WHERE public_no = 1005").execute(&pool).await.unwrap();
    assert_eq!(refused(&write(&pool, "/listings/1005/messages", &besnik, "Alo?").await), (StatusCode::UNPROCESSABLE_ENTITY, closed.clone()));
    sqlx::query("UPDATE listing_contacts SET allow_messages = false FROM listings l WHERE l.id = listing_id AND l.public_no = 1007")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(refused(&write(&pool, "/listings/1007/messages", &besnik, "Alo?").await), (StatusCode::UNPROCESSABLE_ENTITY, closed));
    // A number no listing has, one deleted, or none: not found.
    sqlx::query("UPDATE listings SET status = 'DELETED' WHERE public_no = 1010").execute(&pool).await.unwrap();
    let not_found = json!({"code": "not_found", "message": "Ajo që kërkove nuk u gjet."});
    for path in ["/listings/999999/messages", "/listings/1010/messages", "/listings/golf/messages"] {
        assert_eq!(refused(&write(&pool, path, &besnik, "Alo?").await), (StatusCode::NOT_FOUND, not_found.clone()), "{path}");
    }
    assert!(inbox(&pool, &besnik).await.is_empty());

    // Sold, the Golf takes no new conversation; the one there is goes on, as on the pages, its
    // head saying what became of it.
    let id = written(&pool, "/listings/1001/messages", &besnik, "Përshëndetje").await["conversation_id"].as_str().unwrap().to_string();
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001").execute(&pool).await.unwrap();
    let shown = conversation(&pool, &id, &besnik).await;
    assert_eq!((&shown["closed"], &shown["can_write"], &shown["why_not"]), (&json!(true), &json!(true), &Value::Null));
    assert_eq!(shown["listing"]["status_label"], "Shitur");
    assert_eq!(got(&pool, &format!("/conversations/{id}?lang=en"), &besnik).await["listing"]["status_label"], "Sold");
    assert_eq!(inbox(&pool, &ana).await[0]["listing"]["status_label"], "Shitur");
    written(&pool, &format!("/conversations/{id}/messages"), &ana, "Më vjen keq, u shit.").await;
    assert_eq!(written(&pool, "/listings/1001/messages", &besnik, "Faleminderit!").await["conversation_id"], json!(id));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_block_stops_writing_either_way_until_it_is_lifted(pool: PgPool) {
    let (ana, besnik) = (token_for(&pool, ANA, "android").await, token_for(&pool, BESNIK, "ios").await);
    let id = written(&pool, "/listings/1001/messages", &besnik, "Përshëndetje").await["conversation_id"].as_str().unwrap().to_string();
    let into = format!("/conversations/{id}/messages");

    // Ana blocks him, as the page's "Blloko" does.
    let blocked = post_json(pool.clone(), &format!("/api/v1/conversations/{id}/block"), Some(&ana), "").await;
    assert_eq!((blocked.status, blocked.body.as_str()), (StatusCode::NO_CONTENT, ""));
    let you_blocked = "E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.";
    let blocked_you = "Ana nuk pranon mesazhe nga ti.";
    let hers = conversation(&pool, &id, &ana).await;
    assert_eq!((&hers["blocked_by_me"], &hers["blocked_me"], &hers["can_write"]), (&json!(true), &json!(false), &json!(false)));
    assert_eq!(hers["why_not"], json!({"code": "blocked", "message": you_blocked}));
    let his = conversation(&pool, &id, &besnik).await;
    assert_eq!((&his["blocked_by_me"], &his["blocked_me"], &his["can_write"]), (&json!(false), &json!(true), &json!(false)));
    assert_eq!(his["why_not"], json!({"code": "blocked_by", "message": blocked_you}));
    assert_eq!((&inbox(&pool, &ana).await[0]["blocked"], &inbox(&pool, &besnik).await[0]["blocked"]), (&json!(true), &json!(false)));

    // Neither writes: he, here or about another of her listings; nor she, until she lets him.
    let refusal = |message: &str| (StatusCode::UNPROCESSABLE_ENTITY, json!({"code": "validation", "message": message, "fields": {}}));
    assert_eq!(refused(&write(&pool, &into, &besnik, "Alo?").await), refusal(blocked_you));
    assert_eq!(refused(&write(&pool, "/listings/1005/messages", &besnik, "Po iPhone?").await), refusal(blocked_you));
    assert_eq!(refused(&write(&pool, &into, &ana, "Mirupafshim").await), refusal(you_blocked));
    // In English too, his name as an English page says it.
    let english = refused(&write(&pool, &format!("{into}?lang=en"), &besnik, "Alo?").await);
    assert_eq!(english.1["message"], "Ana doesn’t accept messages from you.");
    // The page says the same to him.
    assert!(page(&pool, &format!("/mesazhet/{id}"), &besnik).await.body.contains(blocked_you));

    // She lets him write again: both may.
    let lifted = post_json(pool.clone(), &format!("/api/v1/conversations/{id}/unblock"), Some(&ana), "").await;
    assert_eq!(lifted.status, StatusCode::NO_CONTENT);
    for token in [&ana, &besnik] {
        let shown = conversation(&pool, &id, token).await;
        assert_eq!((&shown["can_write"], &shown["why_not"], &shown["blocked_by_me"], &shown["blocked_me"]), (&json!(true), &Value::Null, &json!(false), &json!(false)));
    }
    assert_eq!(inbox(&pool, &ana).await[0]["blocked"], false);
    written(&pool, &into, &besnik, "Alo?").await;
    written(&pool, &into, &ana, "Po, të dëgjoj.").await;
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_stores_people_share_its_conversations(pool: PgPool) {
    // Elira posts for Auto Durrësi, which Dritan owns and whose Mercedes (1002) he posted.
    let elira_id = new_user(&pool, "+355691112244", "Elira").await;
    sqlx::query("INSERT INTO business_members (business_id, user_id, role) VALUES ($1::uuid, $2::uuid, 'EDITOR')")
        .bind(AUTO_DURRESI)
        .bind(&elira_id)
        .execute(&pool)
        .await
        .unwrap();
    let (besnik, dritan, elira) =
        (token_for(&pool, BESNIK, "ios").await, token_for(&pool, DRITAN, "android").await, token_for(&pool, &elira_id, "android").await);

    // Besnik writes to the store.
    let id = written(&pool, "/listings/1002/messages", &besnik, "A është ende në shitje?").await["conversation_id"].as_str().unwrap().to_string();
    let his = inbox(&pool, &besnik).await;
    assert_eq!(his[0]["other"], json!({"id": AUTO_DURRESI, "kind": "business", "name": "Auto Durrësi", "slug": "auto-durresi"}));
    assert_eq!(his[0]["store"], Value::Null);
    // Each of its people has it unread, in an inbox that names the store; Elira since she joined.
    for token in [&dritan, &elira] {
        let theirs = inbox(&pool, token).await;
        assert_eq!(theirs.len(), 1, "{theirs:#?}");
        assert_eq!(theirs[0]["id"], json!(id));
        assert_eq!(theirs[0]["other"], json!({"id": BESNIK, "kind": "user", "name": "Besnik", "slug": null}));
        assert_eq!(theirs[0]["store"], json!({"slug": "auto-durresi", "name": "Auto Durrësi"}));
        assert_eq!((theirs[0]["unread"].as_i64(), unread_conversations(&pool, token).await), (Some(1), 1));
    }
    let shown = conversation(&pool, &id, &elira).await;
    assert_eq!((&shown["store"]["name"], &shown["can_write"]), (&json!("Auto Durrësi"), &json!(true)));

    // Elira answers for the store: to Besnik, the store's; to Dritan, theirs, with her name, and no
    // news: his count is Besnik's message still.
    written(&pool, &format!("/conversations/{id}/messages"), &elira, "Po, ejani nesër.").await;
    let to_him = conversation(&pool, &id, &besnik).await;
    assert_eq!(to_him["messages"][1], json!({"id": to_him["messages"][1]["id"], "mine": false, "kind": "TEXT", "author": null,
        "body": "Po, ejani nesër.", "created_at": to_him["messages"][1]["created_at"]}));
    assert_eq!(to_him["other"]["name"], "Auto Durrësi");
    let to_dritan = conversation(&pool, &id, &dritan).await;
    assert_eq!((&to_dritan["messages"][1]["mine"], &to_dritan["messages"][1]["author"]), (&json!(true), &json!("Elira")));
    let his_inbox = inbox(&pool, &dritan).await;
    assert_eq!((his_inbox[0]["preview"].as_str(), his_inbox[0]["unread"].as_i64()), (Some("Elira: Po, ejani nesër."), Some(1)));
    assert_eq!(unread_conversations(&pool, &elira).await, 0);
    assert_eq!(inbox(&pool, &besnik).await[0]["unread"], 1);

    // Let go, she sees it no more.
    sqlx::query("DELETE FROM business_members WHERE user_id = $1::uuid").bind(&elira_id).execute(&pool).await.unwrap();
    assert_eq!(get(pool.clone(), &format!("/api/v1/conversations/{id}"), Some(&elira)).await.status, StatusCode::NOT_FOUND);
    assert!(inbox(&pool, &elira).await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn none_of_it_without_a_token(pool: PgPool) {
    let unauthorized = json!({"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}});
    let id = "0192f0c1-0000-7000-8000-00000000000c";
    let routes = [
        ("GET", "/api/v1/conversations".to_string()),
        ("GET", format!("/api/v1/conversations/{id}")),
        ("POST", "/api/v1/listings/1001/messages".to_string()),
        ("POST", format!("/api/v1/conversations/{id}/messages")),
        ("POST", format!("/api/v1/conversations/{id}/read")),
        ("POST", format!("/api/v1/conversations/{id}/block")),
        ("POST", format!("/api/v1/conversations/{id}/unblock")),
    ];
    for (method, path) in routes {
        let body = Body::from(json!({"body": "Alo?", "up_to": id}).to_string());
        let asked = request(method, &path, None).header(header::CONTENT_TYPE, "application/json").body(body).unwrap();
        let answer = send(api(pool.clone()), asked).await;
        assert_eq!((answer.status, answer.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "{method} {path}");
        assert_eq!(answer.header("www-authenticate"), Some("Bearer"));
    }
}
