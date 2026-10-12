//! What a search box suggests through the API (crates/api/src/suggestions.rs), on the pages' sample
//! listings (crates/web/tests/fixtures/listings.sql): the words finished, the categories the first
//! is found in, the makes and models the words name and the towns, as data; the same searches,
//! in the same order, as the pages' rows under their search boxes (`/sugjerime`) link to; nothing
//! for too little; and never kept.

mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

use common::*;

/// A search: a page's path and its query's pairs.
type Search = (String, Vec<(String, String)>);

/// `/api/v1/suggestions` for `typed` (as it goes in an address), answered 200: its JSON.
async fn suggestions(pool: &PgPool, typed: &str) -> Value {
    let answer = get(pool.clone(), &format!("/api/v1/suggestions?q={typed}"), None).await;
    assert_eq!(answer.status, StatusCode::OK, "{typed}: {}", answer.body);
    answer.json()
}

/// The searches the pages' rows for `typed` link to, in their order.
async fn rows(pool: &PgPool, typed: &str) -> Vec<Search> {
    let page = get(pool.clone(), &format!("/sugjerime?q={typed}"), None).await.body;
    let link = r#"<a class="suggestion" role="option" href=""#;
    page.split(link).skip(1).map(|row| search(&row[..row.find('"').unwrap()])).collect()
}

/// A link's address as its search: its path, and its query's pairs as a form reads them (its "&"
/// escaped in the page as "&#38;").
fn search(href: &str) -> Search {
    let href = href.replace("&#38;", "&").replace("&amp;", "&");
    let (path, query) = href.split_once('?').unwrap_or((href.as_str(), ""));
    let pairs = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
            (decoded(name), decoded(value))
        })
        .collect();
    (path.to_string(), pairs)
}

/// A query's text as a form reads it: "+" a space, "%C3%AB" an ë.
fn decoded(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        match byte {
            b'+' => bytes.push(b' '),
            b'%' if after.len() >= 2 => {
                bytes.push(u8::from_str_radix(std::str::from_utf8(&after[..2]).unwrap(), 16).unwrap());
                rest = &after[2..];
                continue;
            }
            byte => bytes.push(byte),
        }
        rest = after;
    }
    String::from_utf8(bytes).unwrap()
}

/// The searches the API's answer leads to, in the order the pages' rows go: the first words, them
/// in each category, the makes and models by their filters in theirs, the places, and the rest of
/// the words.
fn searches(answer: &Value) -> Vec<Search> {
    let text = |value: &Value| value.as_str().unwrap_or_else(|| panic!("not text: {value}")).to_string();
    let pair = |name: &str, value: &Value| (name.to_string(), text(value));
    let words = answer["words"].as_array().unwrap();
    let mut searches = Vec::new();
    if let Some(first) = words.first() {
        searches.push(("/search".to_string(), vec![pair("q", first)]));
        for category in answer["categories"].as_array().unwrap() {
            searches.push((format!("/c/{}", text(&category["slug"])), vec![pair("q", first)]));
        }
    }
    for option in answer["options"].as_array().unwrap() {
        let filters = option["filters"].as_array().unwrap().iter().map(|filter| (text(&filter["code"]), text(&filter["value"])));
        searches.push((format!("/c/{}", text(&option["category"]["slug"])), filters.collect()));
    }
    for place in answer["places"].as_array().unwrap() {
        assert_eq!(place["kind"], "city", "{place}");
        let words = place["q"].as_str().map(|q| ("q".to_string(), q.to_string()));
        searches.push(("/search".to_string(), words.into_iter().chain([pair("city", &place["slug"])]).collect()));
    }
    for more in words.iter().skip(1) {
        searches.push(("/search".to_string(), vec![pair("q", more)]));
    }
    searches
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn suggestions_are_the_pages_rows_as_data(pool: PgPool) {
    // "gol": the word finished as the listings have it, the category it is found in, and the make
    // and model it names, by their filters there.
    let gol = suggestions(&pool, "gol").await;
    assert_eq!(gol["words"][0], json!("golf"));
    assert_eq!(gol["categories"], json!([{"slug": "makina", "name": "Makina", "name_en": "Cars"}]));
    assert_eq!(
        gol["options"][0],
        json!({
            "name": "Volkswagen Golf",
            "category": {"slug": "makina", "name": "Makina", "name_en": "Cars"},
            "filters": [{"code": "vehicle_make", "value": "VOLKSWAGEN"}, {"code": "vehicle_model", "value": "VOLKSWAGEN_GOLF"}],
        })
    );
    assert_eq!(gol["places"], json!([]));
    // "tir": Tirana, all of its listings; "golf tir", golf there; "makina në": the towns cars are
    // in, the car there, nothing else.
    let tir = suggestions(&pool, "tir").await;
    assert_eq!(tir["places"], json!([{"kind": "city", "city": "Tiranë", "zone": null, "slug": "tirane", "q": null}]));
    let there = suggestions(&pool, "golf%20tir").await;
    assert_eq!(there["places"], json!([{"kind": "city", "city": "Tiranë", "zone": null, "slug": "tirane", "q": "golf"}]));
    let cars = suggestions(&pool, "makina%20n%C3%AB").await;
    assert_eq!((&cars["words"], &cars["categories"], &cars["options"]), (&json!([]), &json!([]), &json!([])));
    let towns: Vec<&Value> = cars["places"].as_array().unwrap().iter().map(|place| &place["slug"]).collect();
    assert_eq!(towns.len(), 2, "{cars}");
    assert!(cars["places"].as_array().unwrap().iter().all(|place| place["q"] == "makina"), "{cars}");

    // Row for row the pages' own: the same searches, in the same order.
    for typed in [
        "gol",
        "tir",
        "durr",
        "golf%20durr",
        "golf%20ne%20durr",
        "ne%20durr",
        "makina%20ne",
        "makina%20n%C3%AB%20",
        "mercedez",
        "apple",
        "golf%207",
        "g",
    ] {
        let answer = suggestions(&pool, typed).await;
        assert_eq!(searches(&answer), rows(&pool, typed).await, "{typed}: {answer}");
    }

    // Too little to go on: nothing; and never kept.
    let little = get(pool.clone(), "/api/v1/suggestions?q=g", None).await;
    assert_eq!(little.json(), json!({"words": [], "categories": [], "options": [], "places": []}));
    assert_eq!(little.header("cache-control"), Some("no-store"));
    assert_eq!(get(pool, "/api/v1/suggestions", None).await.json()["words"], json!([]));
}
