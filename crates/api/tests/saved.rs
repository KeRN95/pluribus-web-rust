//! What a user keeps, through the API (crates/api/src/saved.rs), on the pages' sample listings
//! (crates/web/tests/fixtures/listings.sql): a listing kept and let go, `/saved` with its cards, a
//! price drop shown until it is seen, the listings kept that left the site, and 401 without a
//! token.

mod common;

use std::time::Duration;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use pluribus_db::ListingAction;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use common::*;

/// The fixtures' Ana, who sells the Golf (1001), the iPhone (1005) and the sofa (1007).
const ANA: &str = "11111111-1111-4111-8111-111111111111";

/// `method` at `path`, signed in with `token` if one is given, with `body` as JSON if one is given.
async fn call(pool: &PgPool, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> ApiResponse {
    let mut request = request(method, path, token);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    send(api(pool.clone()), request.body(body).unwrap()).await
}

/// `/api/v1/saved` for the user signed in with `token`, answered 200: its JSON.
async fn saved(pool: &PgPool, token: &str) -> Value {
    let answer = call(pool, "GET", "/api/v1/saved", Some(token), None).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    answer.json()
}

/// The numbers of a list's cards, or its rows', in its order.
fn numbers(list: &Value) -> Vec<i64> {
    list.as_array().unwrap_or_else(|| panic!("not a list: {list}")).iter().map(|item| item["no"].as_i64().unwrap()).collect()
}

/// A new buyer, signed in from the app: their id and their token.
async fn buyer(pool: &PgPool) -> (String, String) {
    let id = new_user(pool, "+355692223344", "Blerina").await;
    let token = token_for(pool, &id, "android").await;
    (id, token)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_listing_kept_is_on_saved_until_it_is_let_go(pool: PgPool) {
    let (buyer, token) = buyer(&pool).await;
    // Kept, once however often; the last kept first.
    for _ in 0..2 {
        assert_eq!(call(&pool, "PUT", "/api/v1/listings/1001/saved", Some(&token), None).await.status, StatusCode::NO_CONTENT);
    }
    assert_eq!(call(&pool, "PUT", "/api/v1/listings/1005/saved", Some(&token), None).await.status, StatusCode::NO_CONTENT);
    let kept: i64 = sqlx::query_scalar("SELECT count(*) FROM saved_listings WHERE user_id = $1::uuid")
        .bind(&buyer)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(kept, 2);
    let list = saved(&pool, &token).await;
    assert_eq!(numbers(&list["listings"]), [1005, 1001]);
    assert_eq!((&list["price_drops"], &list["gone"]), (&json!([]), &json!([])));
    // Each a card, as the lists give it.
    let golf = &list["listings"][1];
    assert_eq!((&golf["title"], &golf["status"], &golf["price"]["amount"]), (&json!("Volkswagen Golf 7, 2016, naftë"), &json!("ACTIVE"), &json!("9500")));

    // Only a listing up can be kept: not one paused, nor a number no listing has, nor one that isn't
    // a number.
    sqlx::query("UPDATE listings SET status = 'PAUSED' WHERE public_no = 1006").execute(&pool).await.unwrap();
    for path in ["/api/v1/listings/1006/saved", "/api/v1/listings/99999/saved", "/api/v1/listings/golf/saved"] {
        let refused = call(&pool, "PUT", path, Some(&token), None).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::NOT_FOUND, &json!("not_found")), "{path}");
    }

    // Let go, once however often; a number no listing has is not found.
    for _ in 0..2 {
        assert_eq!(call(&pool, "DELETE", "/api/v1/listings/1001/saved", Some(&token), None).await.status, StatusCode::NO_CONTENT);
    }
    assert_eq!(numbers(&saved(&pool, &token).await["listings"]), [1005]);
    assert_eq!(call(&pool, "DELETE", "/api/v1/listings/99999/saved", Some(&token), None).await.status, StatusCode::NOT_FOUND);

    // Nobody signed in: 401 in the envelope, whatever is asked.
    for (method, path, body) in [
        ("PUT", "/api/v1/listings/1001/saved", None),
        ("DELETE", "/api/v1/listings/1005/saved", None),
        ("GET", "/api/v1/saved", None),
        ("POST", "/api/v1/saved/seen", Some(json!({"up_to": Uuid::nil()}))),
    ] {
        let refused = call(&pool, method, path, None, body).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::UNAUTHORIZED, &json!("unauthorized")), "{method} {path}");
    }
    assert_eq!(numbers(&saved(&pool, &token).await["listings"]), [1005]);
}

/// Its seller lowers the listing's price to `price` euros, as an edit does: the listing changed,
/// and those who keep it told, in one transaction (db::notify_price_drop, which db::update_listing
/// calls).
async fn lower(pool: &PgPool, no: i64, price: i64) {
    let mut tx = pool.begin().await.unwrap();
    let id: Uuid = sqlx::query_scalar(
        "UPDATE listings SET price_amount = $2::numeric, price_normalized = $2::numeric
         WHERE public_no = $1 AND currency = 'EUR' RETURNING id",
    )
    .bind(no)
    .bind(price)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    pluribus_db::notify_price_drop(&mut tx, id).await.unwrap();
    tx.commit().await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_price_drop_shows_until_it_is_seen(pool: PgPool) {
    let (_, token) = buyer(&pool).await;
    call(&pool, "PUT", "/api/v1/listings/1001/saved", Some(&token), None).await;
    // Ana lowers the Golf from 9 500 € to 9 000 €: the buyer who keeps it hears of it.
    lower(&pool, 1001, 9000).await;
    let list = saved(&pool, &token).await;
    let drops = list["price_drops"].as_array().expect("price_drops").clone();
    assert_eq!(drops.len(), 1, "{list}");
    let first = drops[0]["id"].as_str().unwrap().to_string();
    let at = OffsetDateTime::parse(drops[0]["at"].as_str().unwrap(), &Rfc3339).unwrap();
    assert!((OffsetDateTime::now_utc() - at).whole_seconds().abs() < 60, "{at}");
    let mut drop = drops[0].clone();
    drop["id"] = json!(null);
    drop["at"] = json!(null);
    assert_eq!(
        drop,
        json!({"id": null, "no": 1001, "title": "Volkswagen Golf 7, 2016, naftë", "from": "9500", "to": "9000", "currency": "EUR", "at": null})
    );
    // Its card says what it was too.
    assert_eq!(list["listings"][0]["price"]["was"], json!("9500"));

    // Lowered again before the buyer looks: one drop, from what it was when they last saw it.
    tokio::time::sleep(Duration::from_millis(5)).await;
    lower(&pool, 1001, 8500).await;
    let drops = saved(&pool, &token).await["price_drops"].clone();
    assert_eq!((drops.as_array().unwrap().len(), &drops[0]["from"], &drops[0]["to"]), (1, &json!("9500"), &json!("8500")));
    let latest = drops[0]["id"].as_str().unwrap().to_string();
    assert_ne!(latest, first);
    // Seen up to the first: the one that came after stays, as the page's beacon has it; seen up to
    // it, none.
    let seen = call(&pool, "POST", "/api/v1/saved/seen", Some(&token), Some(json!({"up_to": first}))).await;
    assert_eq!(seen.status, StatusCode::NO_CONTENT, "{}", seen.body);
    assert_eq!(numbers(&saved(&pool, &token).await["price_drops"]), [1001]);
    call(&pool, "POST", "/api/v1/saved/seen", Some(&token), Some(json!({"up_to": latest}))).await;
    assert_eq!(saved(&pool, &token).await["price_drops"], json!([]));
    // An `up_to` that isn't a drop's id: the validation envelope, on the body.
    let wrong = call(&pool, "POST", "/api/v1/saved/seen", Some(&token), Some(json!({"up_to": "latest"}))).await;
    assert_eq!((wrong.status, wrong.json()["error"]["fields"]["body"].is_string()), (StatusCode::UNPROCESSABLE_ENTITY, true));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_listing_kept_that_left_the_site_says_why(pool: PgPool) {
    let (_, token) = buyer(&pool).await;
    for no in [1001, 1005, 1007] {
        tokio::time::sleep(Duration::from_millis(2)).await;
        call(&pool, "PUT", &format!("/api/v1/listings/{no}/saved"), Some(&token), None).await;
    }
    // Ana sells the Golf, pauses the iPhone and deletes the sofa, as "Njoftimet e mia" does
    // (db::change_own_listing): the Golf's card stays, saying so; the other two are gone.
    let ana: Uuid = ANA.parse().unwrap();
    for (no, action) in [(1001, ListingAction::Sell), (1005, ListingAction::Pause), (1007, ListingAction::Delete)] {
        assert!(pluribus_db::change_own_listing(&pool, no, ana, action).await.unwrap(), "{no}");
    }
    let list = saved(&pool, &token).await;
    assert_eq!(numbers(&list["listings"]), [1001]);
    assert_eq!(list["listings"][0]["status"], json!("SOLD"));
    assert_eq!(
        list["gone"],
        json!([
            {"no": 1007, "title": "Divan qoshe ngjyrë gri", "status": "DELETED", "why": "U fshi"},
            {"no": 1005, "title": "iPhone 13, 128 GB, si i ri", "status": "PAUSED", "why": "Çaktivizuar nga shitësi"},
        ])
    );
    // Why, in the language asked for.
    let english = call(&pool, "GET", "/api/v1/saved?lang=en", Some(&token), None).await.json();
    assert_eq!((&english["gone"][0]["why"], &english["gone"][1]["why"]), (&json!("Deleted"), &json!("Paused by the seller")));
    // Let go from there.
    assert_eq!(call(&pool, "DELETE", "/api/v1/listings/1005/saved", Some(&token), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(numbers(&saved(&pool, &token).await["gone"]), [1007]);
}
