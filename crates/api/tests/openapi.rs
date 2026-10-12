//! The OpenAPI document (crates/api/src/openapi.rs), the contract the app's client is generated
//! from: held to the router, every route documented and every documented path a route, answered
//! with the methods the document says, and signed in as its security says; served as the
//! repository keeps it, `openapi.json` at its root, which
//! `UPDATE_OPENAPI=1 cargo test -p pluribus-api --test openapi` writes anew after a change to the
//! API; and whole: every reference in it to something it has, every operation named once, tagged,
//! described and with its path's parameters, read back by utoipa as it was written, and written
//! pretty with its keys sorted.

mod common;

use std::collections::BTreeSet;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use serde_json::Value;
use sqlx::PgPool;

use common::*;

/// Where the repository keeps the document: `openapi.json`, at its root.
const COMMITTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi.json");

/// What a test says when the committed document isn't the one served.
const WRITE_IT_ANEW: &str = "write it anew with `UPDATE_OPENAPI=1 cargo test -p pluribus-api --test openapi`, and commit it with the change";

/// The methods an operation can have, as the document names them.
const METHODS: [&str; 8] = ["get", "put", "post", "delete", "options", "head", "patch", "trace"];

/// The document as `GET /api/v1/openapi.json` serves it, for anyone.
async fn served(pool: PgPool) -> ApiResponse {
    get(pool, "/api/v1/openapi.json", None).await
}

/// The document's operations, each its method and its path, with what the document says of it.
fn operations(document: &Value) -> Vec<(String, String, &Value)> {
    let paths = document["paths"].as_object().expect("the document's paths");
    paths
        .iter()
        .flat_map(|(path, item)| {
            METHODS.iter().filter_map(move |method| item.get(*method).map(|operation| (method.to_string(), path.clone(), operation)))
        })
        .collect()
}

/// A documented path with a value in each of its parameters (`/listings/{no}` as `/listings/1`),
/// which the router answers as it would any.
fn concrete(path: &str) -> String {
    let mut concrete = String::new();
    let mut rest = path;
    while let Some((before, after)) = rest.split_once('{') {
        concrete.push_str(before);
        rest = after.split_once('}').map_or("", |(_, after)| after);
        concrete.push('1');
    }
    concrete.push_str(rest);
    concrete
}

/// The names of a path's parameters: `{no}` and the like.
fn parameters_of(path: &str) -> BTreeSet<String> {
    path.split('{').skip(1).filter_map(|part| part.split_once('}')).map(|(name, _)| name.to_string()).collect()
}

/// Every `$ref` in `value`.
fn references(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                match (key.as_str(), value) {
                    ("$ref", Value::String(reference)) => found.push(reference.clone()),
                    _ => references(value, found),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| references(item, found)),
        _ => {}
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn every_route_is_documented_and_every_documented_path_is_a_route(pool: PgPool) {
    let document = served(pool.clone()).await.json();
    // The routes the router is made from, each under the prefix, and the document's operations.
    let routes: BTreeSet<(String, String)> = pluribus_api::routes()
        .into_iter()
        .map(|(method, path)| (method.as_str().to_lowercase(), format!("{}{path}", pluribus_api::PREFIX)))
        .collect();
    let documented: BTreeSet<(String, String)> =
        operations(&document).into_iter().map(|(method, path, _)| (method, path)).collect();
    assert_eq!(routes.len(), pluribus_api::routes().len(), "a route twice");
    let undocumented: Vec<_> = routes.difference(&documented).collect();
    assert!(undocumented.is_empty(), "routes the document lacks: {undocumented:?}");
    let unrouted: Vec<_> = documented.difference(&routes).collect();
    assert!(unrouted.is_empty(), "documented operations no route has: {unrouted:?}");
    // And the router answers each documented path with the document's methods: asked with one none
    // of the path's routes has, it answers 405 and names the methods it has (`Allow`, a GET's HEAD
    // among them), where a path the API lacks would answer its 404.
    for (path, item) in document["paths"].as_object().unwrap() {
        let probe = request("OPTIONS", &concrete(path), None).body(Body::empty()).unwrap();
        let answer = send(api(pool.clone()), probe).await;
        assert_eq!(answer.status, StatusCode::METHOD_NOT_ALLOWED, "{path}: {}", answer.body);
        let allowed: BTreeSet<String> = answer
            .header("allow")
            .unwrap_or_default()
            .split(',')
            .map(|method| method.trim().to_lowercase())
            .filter(|method| method != "head")
            .collect();
        let methods: BTreeSet<String> = METHODS.iter().filter(|method| item.get(**method).is_some()).map(|method| method.to_string()).collect();
        assert_eq!(allowed, methods, "{path}");
    }
}

/// Each operation is signed in as the document says: one that needs the bearer token
/// (`[{"bearer": []}]`) answers 401 without one; one where it is optional (`[{}, {"bearer": []}]`)
/// answers without one, and 401 for a token no session has, as `OptionalApiUser` does; and one
/// that needs none (no `security`) answers whatever token comes, never 401. Each asked once with
/// nothing in it, on a database with nothing in it.
#[sqlx::test(migrator = "MIGRATOR")]
async fn every_operation_is_signed_in_as_its_security_says(pool: PgPool) {
    let document = served(pool.clone()).await.json();
    let bearer = serde_json::json!([{"bearer": []}]);
    let optional = serde_json::json!([{}, {"bearer": []}]);
    for (method, path, operation) in operations(&document) {
        let ask = |token: Option<&'static str>| {
            let request = request(&method.to_uppercase(), &concrete(&path), token).body(Body::empty()).unwrap();
            send(api(pool.clone()), request)
        };
        let named = format!("{} {path}", method.to_uppercase());
        match operation.get("security") {
            Some(security) if *security == bearer => {
                let answer = ask(None).await;
                assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{named} without a token: {}", answer.body);
                assert_eq!(answer.json()["error"]["code"], "unauthorized", "{named}");
            }
            Some(security) if *security == optional => {
                let answer = ask(None).await;
                assert_ne!(answer.status, StatusCode::UNAUTHORIZED, "{named} without a token: {}", answer.body);
                let answer = ask(Some("a-token-no-session-has")).await;
                assert_eq!(answer.status, StatusCode::UNAUTHORIZED, "{named} with a token no session has: {}", answer.body);
            }
            Some(security) => panic!("{named}: a security the API doesn't have: {security}"),
            None => {
                let answer = ask(Some("a-token-no-session-has")).await;
                assert_ne!(answer.status, StatusCode::UNAUTHORIZED, "{named} with a token no session has: {}", answer.body);
            }
        }
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_document_served_is_the_one_the_repository_keeps(pool: PgPool) {
    let served = served(pool).await;
    // For anyone, as JSON, kept five minutes.
    assert_eq!(served.status, StatusCode::OK);
    assert_eq!(served.header("content-type"), Some("application/json"));
    assert_eq!(served.header(header::CACHE_CONTROL.as_str()), Some("public, max-age=300"));
    assert_eq!(served.json()["openapi"], "3.1.0");
    if std::env::var("UPDATE_OPENAPI").is_ok_and(|update| update == "1") {
        std::fs::write(COMMITTED, &served.body).expect("openapi.json written");
        println!("openapi.json written anew: {} bytes", served.body.len());
        return;
    }
    // Byte for byte, but for the carriage returns a Windows checkout gives its lines.
    let committed = std::fs::read_to_string(COMMITTED).unwrap_or_default().replace("\r\n", "\n");
    if committed != served.body {
        let line = committed.lines().zip(served.body.lines()).position(|(kept, serving)| kept != serving);
        let line = line.unwrap_or_else(|| committed.lines().count().min(served.body.lines().count()));
        panic!(
            "openapi.json isn't the document the API serves (from line {}: {:?} kept, {:?} served): {WRITE_IT_ANEW}",
            line + 1,
            committed.lines().nth(line).unwrap_or_default(),
            served.body.lines().nth(line).unwrap_or_default(),
        );
    }
}

#[test]
fn the_document_holds_together() {
    let text = pluribus_api::openapi::json();
    let document: Value = serde_json::from_str(text).expect("the document is JSON");
    assert_eq!(document["openapi"], "3.1.0");
    assert_eq!((document["info"]["title"].as_str(), document["info"]["version"].as_str()), (Some("Pluribus API"), Some(env!("CARGO_PKG_VERSION"))));
    // Pretty, two spaces, every object's keys sorted, so that a change shows as the lines it changed.
    let mut written = serde_json::to_string_pretty(&document).unwrap();
    written.push('\n');
    assert!(written == text, "the document isn't written pretty with its keys sorted");
    // utoipa reads it back as it wrote it: every part of it the shape OpenAPI has for it.
    let read: utoipa::openapi::OpenApi = serde_json::from_str(text).expect("utoipa reads the document");
    let again: Value = serde_json::from_str(&read.to_pretty_json().unwrap()).unwrap();
    assert!(again == document, "utoipa writes back other than it read");
    // Every reference is to something the document has.
    let mut found = Vec::new();
    references(&document, &mut found);
    assert!(found.len() > 100, "{} references", found.len());
    for reference in &found {
        let pointer = reference.strip_prefix('#').unwrap_or_else(|| panic!("{reference}: not in the document"));
        assert!(document.pointer(pointer).is_some(), "{reference}: nothing there");
    }
    // Every schema and response the document has, something refers to.
    for kind in ["schemas", "responses"] {
        for name in document["components"][kind].as_object().unwrap().keys() {
            let reference = format!("#/components/{kind}/{name}");
            assert!(found.contains(&reference), "{reference}: nothing refers to it");
        }
    }
    let tags: BTreeSet<&str> = document["tags"].as_array().unwrap().iter().map(|tag| tag["name"].as_str().unwrap()).collect();
    let schemes: BTreeSet<&String> = document["components"]["securitySchemes"].as_object().unwrap().keys().collect();
    assert_eq!(schemes.into_iter().collect::<Vec<_>>(), ["bearer"]);
    assert_eq!(document["components"]["securitySchemes"]["bearer"], serde_json::json!({
        "type": "http",
        "scheme": "bearer",
        "description": document["components"]["securitySchemes"]["bearer"]["description"],
    }));
    let mut names = BTreeSet::new();
    let mut tagged = BTreeSet::new();
    for (method, path, operation) in operations(&document) {
        let named = format!("{} {path}", method.to_uppercase());
        assert!(path.starts_with(pluribus_api::PREFIX), "{named}: not under the API's prefix");
        // Named once, for the client's method.
        let id = operation["operationId"].as_str().unwrap_or_else(|| panic!("{named}: no operationId"));
        assert!(names.insert(id.to_string()), "{named}: {id} named twice");
        // Under one of the document's tags, said in a summary and a sentence of its own.
        let tag = operation["tags"][0].as_str().unwrap_or_else(|| panic!("{named}: no tag"));
        assert!(tags.contains(tag), "{named}: tag {tag} not the document's");
        tagged.insert(tag);
        for said in ["summary", "description"] {
            assert!(operation[said].as_str().is_some_and(|text| !text.is_empty()), "{named}: no {said}");
        }
        // Its path's parameters each declared, required, and no other in the path.
        let declared: BTreeSet<String> = operation["parameters"]
            .as_array()
            .map(|parameters| parameters.iter().filter(|parameter| parameter["in"] == "path").collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .map(|parameter| {
                assert_eq!(parameter["required"], true, "{named}: {} not required", parameter["name"]);
                parameter["name"].as_str().unwrap().to_string()
            })
            .collect();
        assert_eq!(declared, parameters_of(&path), "{named}: the path's parameters");
        // Each answer described, a failure's the document's own.
        let responses = operation["responses"].as_object().unwrap_or_else(|| panic!("{named}: no responses"));
        assert!(responses.keys().any(|status| status.starts_with('2')), "{named}: no success");
        for (status, response) in responses {
            match status.starts_with('2') {
                true => assert!(response["description"].as_str().is_some_and(|text| !text.is_empty()), "{named} {status}: no description"),
                false => assert!(response["$ref"].as_str().is_some_and(|to| to.starts_with("#/components/responses/")), "{named} {status}: not a failure of the document's"),
            }
        }
    }
    assert_eq!(tagged, tags, "a tag no operation has");
}
