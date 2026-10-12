//! What every answer of the API shares (crates/api/src/lib.rs, error.rs, lang.rs): JSON, and none
//! of the pages' layers; the health check; the envelope for a path the API lacks; the language
//! each request asks for; and a JSON body the API can't read, answered in the envelope.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use serde_json::json;
use sqlx::PgPool;

use common::*;

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_health_check_answers_in_json_without_the_pages_layers(pool: PgPool) {
    // A browser's cookies, which the pages would answer with theirs (the signed-in cookie).
    let with_cookies = |path: &str| request("GET", path, None).header(header::COOKIE, "session=a-token; gjuha=en").body(Body::empty()).unwrap();
    let health = send(api(pool.clone()), with_cookies("/api/v1/health")).await;
    assert_eq!(health.status, StatusCode::OK);
    assert_eq!(health.header("content-type"), Some("application/json"));
    assert_eq!(health.json(), json!({"ok": true}));
    assert!(health.vary().contains("Accept-Language") && health.vary().contains("Authorization"), "{}", health.vary());
    // None of the pages' layers: their security headers, the cookie that tells their scripts
    // someone is signed in.
    for name in ["content-security-policy", "x-frame-options", "set-cookie"] {
        assert_eq!(health.header(name), None, "{name}");
    }
    // The pages keep them.
    let page = send(api(pool), with_cookies("/healthz")).await;
    assert_eq!((page.status, page.body.as_str()), (StatusCode::OK, "ok"));
    assert!(page.header("content-security-policy").is_some() && page.header("set-cookie").is_some());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_path_the_api_lacks_answers_the_envelope_not_the_404_page(pool: PgPool) {
    let not_found = json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}});
    for path in ["/api/v1/nothing", "/api/v1/me/nothing", "/api/v1"] {
        let missing = get(pool.clone(), path, None).await;
        assert_eq!((missing.status, missing.json()), (StatusCode::NOT_FOUND, not_found.clone()), "{path}");
        assert_eq!(missing.header("content-type"), Some("application/json"), "{path}");
    }
    let missing = post_json(pool.clone(), "/api/v1/nothing", None, "{}").await;
    assert_eq!((missing.status, missing.json()), (StatusCode::NOT_FOUND, not_found));
    // A page the site lacks is still the pages' own.
    let page = get(pool, "/nothing", None).await;
    assert_eq!(page.status, StatusCode::NOT_FOUND);
    assert!(page.header("content-type").unwrap().starts_with("text/html"));
}

/// The message of the 401 that `/me` answers without a token, asked for with this query, these
/// languages and these cookies.
async fn unauthorized_message(pool: &PgPool, query: &str, languages: Option<&str>, cookie: Option<&str>) -> String {
    let mut request = request("GET", &format!("/api/v1/me{query}"), None);
    if let Some(languages) = languages {
        request = request.header(header::ACCEPT_LANGUAGE, languages);
    }
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    let answer = send(api(pool.clone()), request.body(Body::empty()).unwrap()).await;
    assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
    answer.json()["error"]["message"].as_str().unwrap().to_string()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_api_answers_in_the_language_the_request_asks_for(pool: PgPool) {
    let (albanian, english) = ("Hyr në llogari për të vazhduar.", "Sign in to continue.");
    assert_eq!(unauthorized_message(&pool, "", None, None).await, albanian, "nothing asked");
    assert_eq!(unauthorized_message(&pool, "", Some("en"), None).await, english);
    assert_eq!(unauthorized_message(&pool, "", Some("it-IT,it;q=0.9,en;q=0.8"), None).await, english);
    assert_eq!(unauthorized_message(&pool, "", Some("en;q=0, de"), None).await, albanian, "English refused");
    assert_eq!(unauthorized_message(&pool, "?lang=sq", Some("en-GB"), None).await, albanian, "the query first");
    assert_eq!(unauthorized_message(&pool, "?lang=en", None, None).await, english);
    // The pages' language cookie is theirs alone.
    assert_eq!(unauthorized_message(&pool, "", None, Some("gjuha=en")).await, albanian);
    assert_eq!(unauthorized_message(&pool, "", Some("en"), Some("gjuha=sq")).await, english);
}

/// Posts `body` to the probe (common::probed) as `content_type`, with these headers too.
async fn post_probe(pool: &PgPool, body: &str, content_type: &str, headers: &[(&str, &str)]) -> ApiResponse {
    let mut request = request("POST", "/api/v1/probe", None).header(header::CONTENT_TYPE, content_type);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    send(probed(pool.clone()), request.body(Body::from(body.to_string())).unwrap()).await
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_json_body_the_api_cant_read_answers_the_validation_envelope(pool: PgPool) {
    let golf = r#"{"name": "Golf", "count": 2}"#;
    let read = post_probe(&pool, golf, "application/json", &[]).await;
    assert_eq!((read.status, read.json()), (StatusCode::OK, json!({"name": "Golf", "count": 2, "user": null})));
    let refused = |why: &str| json!({"error": {"code": "validation", "message": "Disa fusha nuk janë në rregull.", "fields": {"body": why}}});
    let not_json = post_probe(&pool, r#"{"name": "Golf", "#, "application/json", &[]).await;
    assert_eq!(not_json.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(not_json.json(), refused("Trupi i kërkesës nuk është JSON i vlefshëm."));
    let not_sent_as_json = post_probe(&pool, golf, "text/plain", &[]).await;
    assert_eq!(not_sent_as_json.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(not_sent_as_json.json(), refused("Trupi i kërkesës duhet të dërgohet si JSON, me Content-Type: application/json."));
    for wrong in [r#"{"name": "Golf"}"#, r#"{"name": "Golf", "count": "two"}"#, "[]"] {
        let answer = post_probe(&pool, wrong, "application/json", &[]).await;
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{wrong}");
        let why = "Trupi i kërkesës nuk ka atë që duhet: i mungon një fushë, ose një fushë nuk është e llojit të duhur.";
        assert_eq!(answer.json(), refused(why), "{wrong}");
    }
    // In the language asked for.
    let english = post_probe(&pool, "{", "application/json", &[("accept-language", "en")]).await;
    assert_eq!(
        english.json(),
        json!({"error": {"code": "validation", "message": "Some fields aren’t right.", "fields": {"body": "The request body isn’t valid JSON."}}})
    );
    // Sent from another site's page, it isn't refused, as a form posted to the pages from there is:
    // an API request is signed in by its token, never by a cookie a browser adds on its own.
    let elsewhere = post_probe(&pool, golf, "application/json", &[("origin", "https://another.example"), ("host", "localhost")]).await;
    assert_eq!(elsewhere.status, StatusCode::OK);
}
