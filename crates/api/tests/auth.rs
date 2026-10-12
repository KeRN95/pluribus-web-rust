//! Signing an API request in (crates/api/src/auth.rs): a bearer token of a good session, of the
//! same table as the pages' cookie; `/me`, the signed-in user; when a session was last used; and a
//! route anyone may call, which reads a token if there is one.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

#[sqlx::test(migrator = "MIGRATOR")]
async fn without_a_good_token_me_answers_401_in_the_envelope(pool: PgPool) {
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "android").await;
    let unauthorized = json!({"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}});
    let asked = [
        ("no token", None, None),
        ("a wrong token", Some("Bearer not-a-session".to_string()), None),
        ("another kind of Authorization", Some(format!("Basic {token}")), None),
        // The pages' cookie, of the same session: the API reads tokens alone.
        ("the pages' cookie", None, Some(format!("session={token}"))),
    ];
    for (what, authorization, cookie) in asked {
        let mut request = request("GET", "/api/v1/me", None);
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        if let Some(cookie) = cookie {
            request = request.header(header::COOKIE, cookie);
        }
        let answer = send(api(pool.clone()), request.body(Body::empty()).unwrap()).await;
        assert_eq!((answer.status, answer.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "{what}");
        assert_eq!(answer.header("www-authenticate"), Some("Bearer"), "{what}");
    }
    // The token signs in until its session expires, and while its account is active.
    assert_eq!(get(pool.clone(), "/api/v1/me", Some(&token)).await.status, StatusCode::OK);
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 minute'").execute(&pool).await.unwrap();
    let expired = get(pool.clone(), "/api/v1/me", Some(&token)).await;
    assert_eq!((expired.status, expired.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "expired");
    sqlx::query("UPDATE sessions SET expires_at = now() + interval '1 day'").execute(&pool).await.unwrap();
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE id = $1::uuid").bind(&id).execute(&pool).await.unwrap();
    let suspended = get(pool, "/api/v1/me", Some(&token)).await;
    assert_eq!((suspended.status, suspended.json()), (StatusCode::UNAUTHORIZED, unauthorized), "suspended");
}

/// How many minutes ago the one session was last used; none if never.
async fn minutes_since_used(pool: &PgPool) -> Option<f64> {
    sqlx::query_scalar("SELECT (extract(epoch FROM now() - last_used_at) / 60)::float8 FROM sessions")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn me_answers_the_signed_in_user_and_moves_when_the_session_was_last_used(pool: PgPool) {
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "ios").await;
    assert_eq!(minutes_since_used(&pool).await, None, "not used yet");
    let me = get(pool.clone(), "/api/v1/me", Some(&token)).await;
    assert_eq!(me.status, StatusCode::OK);
    let created: OffsetDateTime = sqlx::query_scalar("SELECT created_at FROM users WHERE id = $1::uuid").bind(&id).fetch_one(&pool).await.unwrap();
    assert_eq!(
        me.json(),
        json!({
            "id": id,
            "display_name": "Ana",
            "phone": "+355690000001",
            "email": null,
            "preferred_language": "sq",
            "since": created.format(&Rfc3339).unwrap(),
        })
    );
    assert!(minutes_since_used(&pool).await.unwrap() < 1.0, "used now");
    // Within ten minutes it stays as it was, a write now and then; after them it moves again.
    sqlx::query("UPDATE sessions SET last_used_at = now() - interval '5 minutes'").execute(&pool).await.unwrap();
    assert_eq!(get(pool.clone(), "/api/v1/me", Some(&token)).await.status, StatusCode::OK);
    assert!(minutes_since_used(&pool).await.unwrap() > 4.0, "five minutes ago");
    sqlx::query("UPDATE sessions SET last_used_at = now() - interval '11 minutes'").execute(&pool).await.unwrap();
    assert_eq!(get(pool.clone(), "/api/v1/me", Some(&token)).await.status, StatusCode::OK);
    assert!(minutes_since_used(&pool).await.unwrap() < 1.0, "used now again");
    // The scheme in any case (RFC 7235).
    let request = request("GET", "/api/v1/me", None).header(header::AUTHORIZATION, format!("bearer {token}"));
    assert_eq!(send(api(pool), request.body(Body::empty()).unwrap()).await.status, StatusCode::OK);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_route_for_anyone_reads_a_bearer_token_if_there_is_one(pool: PgPool) {
    let id = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &id, "android").await;
    let probe = |authorization: Option<String>| {
        let pool = pool.clone();
        async move {
            let mut request = request("POST", "/api/v1/probe", None).header(header::CONTENT_TYPE, "application/json");
            if let Some(authorization) = authorization {
                request = request.header(header::AUTHORIZATION, authorization);
            }
            send(probed(pool), request.body(Body::from(r#"{"name": "Golf", "count": 2}"#)).unwrap()).await
        }
    };
    let signed_in = probe(Some(format!("Bearer {token}"))).await;
    assert_eq!((signed_in.status, signed_in.json()["user"].clone()), (StatusCode::OK, json!(id)));
    for (what, authorization) in [("no token", None), ("another kind of Authorization", Some("Basic YW5hOmFuYQ==".to_string()))] {
        let nobody = probe(authorization).await;
        assert_eq!((nobody.status, nobody.json()["user"].clone()), (StatusCode::OK, Value::Null), "{what}");
    }
    // A token no good session has: 401, so the app hears its session ended.
    let ended = probe(Some("Bearer an-ended-session".to_string())).await;
    assert_eq!(ended.status, StatusCode::UNAUTHORIZED);
    assert_eq!(ended.json()["error"]["code"], "unauthorized");
}
