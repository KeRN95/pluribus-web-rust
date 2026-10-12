//! Trust and safety through the API (crates/api/src/safety.rs), on the pages' sample listings
//! (crates/web/tests/fixtures/listings.sql): the reasons a listing can be reported for; a report,
//! checked as the pages' form checks it; three people reporting a listing for a serious reason
//! taking it off the site; and a seller blocked, their listings leaving the buyer's lists, and let
//! be again; 401 without a token.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;

use common::*;

/// The fixtures' sellers: Ana, who sells the Golf (1001); Besnik; and Dritan, who posted the
/// Mercedes (1002) for Auto Durrësi, and his own job (1008).
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";
const DRITAN: &str = "33333333-3333-4333-8333-333333333333";

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

/// Reports listing `no` for `reason`, with `note` if one is given, as the user signed in with
/// `token`.
async fn report(pool: &PgPool, token: &str, no: i64, reason: &str, note: Option<&str>) -> ApiResponse {
    call(pool, "POST", &format!("/api/v1/listings/{no}/report"), Some(token), Some(json!({"reason": reason, "note": note}))).await
}

/// A new account, signed in from the app: its token.
async fn signed_in(pool: &PgPool, phone: &str) -> String {
    let id = new_user(pool, phone, "Blerina").await;
    token_for(pool, &id, "android").await
}

/// The numbers of every listing `/listings` gives the caller (all of them fit a page here).
async fn listed(pool: &PgPool, token: Option<&str>) -> Vec<i64> {
    let answer = call(pool, "GET", "/api/v1/listings", token, None).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    answer.json()["items"].as_array().unwrap().iter().map(|card| card["no"].as_i64().unwrap()).collect()
}

/// The fields of the validation envelope an answer must be.
fn invalid(answer: &ApiResponse) -> Value {
    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.body);
    answer.json()["error"]["fields"].clone()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn the_reasons_are_the_forms_in_both_languages(pool: PgPool) {
    // For anyone, the same whichever language is asked for, and kept a while.
    let reasons = get(pool.clone(), "/api/v1/report/reasons", None).await;
    assert_eq!(reasons.status, StatusCode::OK);
    assert_eq!(reasons.header("cache-control"), Some("public, max-age=300"));
    let english = get(pool.clone(), "/api/v1/report/reasons?lang=en", None).await;
    assert_eq!(english.json(), reasons.json());
    let reasons = reasons.json();
    let codes: Vec<&str> = reasons.as_array().unwrap().iter().map(|reason| reason["code"].as_str().unwrap()).collect();
    assert_eq!(codes, pluribus_db::REASONS);
    let serious: Vec<&str> = reasons.as_array().unwrap().iter().filter(|reason| reason["serious"] == json!(true)).map(|reason| reason["code"].as_str().unwrap()).collect();
    assert_eq!(serious, pluribus_db::SERIOUS_REASONS);
    assert_eq!(
        reasons[0],
        json!({
            "code": "SCAM",
            "label": "Mashtrim",
            "label_en": "Scam",
            "hint": "Kërkon para përpara, ose çmimi duket shumë i mirë për të qenë i vërtetë.",
            "hint_en": "Asks for money up front, or the price looks too good to be true.",
            "serious": true,
        })
    );
    assert_eq!((&reasons[6]["label_en"], &reasons[6]["serious"]), (&json!("Other"), &json!(false)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_report_is_checked_as_the_form_checks_it(pool: PgPool) {
    let reporter = new_user(&pool, "+355692223344", "Blerina").await;
    let token = token_for(&pool, &reporter, "android").await;
    // Nobody signed in: reports come from accounts alone, so that a stranger can't take someone
    // else's listing off the site.
    let anonymous = call(&pool, "POST", "/api/v1/listings/1001/report", None, Some(json!({"reason": "SCAM"}))).await;
    assert_eq!((anonymous.status, &anonymous.json()["error"]["code"]), (StatusCode::UNAUTHORIZED, &json!("unauthorized")));

    // A reason that isn't one; OTHER without saying what; a note over 1 000 characters.
    assert_eq!(invalid(&report(&pool, &token, 1001, "BORING", None).await), json!({"reason": "Zgjidh çfarë nuk shkon me njoftimin."}));
    assert_eq!(invalid(&report(&pool, &token, 1001, "OTHER", Some("  ")).await), json!({"note": "Shkruaj shkurt çfarë nuk shkon me njoftimin."}));
    let long = "a".repeat(1001);
    let english = call(&pool, "POST", "/api/v1/listings/1001/report?lang=en", Some(&token), Some(json!({"reason": "", "note": long}))).await;
    assert_eq!(
        invalid(&english),
        json!({"reason": "Choose what’s wrong with the listing.", "note": "The explanation can be up to 1 000 characters."})
    );
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM reports").fetch_one(&pool).await.unwrap();
    assert_eq!(reports, 0);

    // OTHER with what is wrong: kept, with its note, once however often.
    for _ in 0..2 {
        assert_eq!(report(&pool, &token, 1001, "OTHER", Some(" Numri nuk përgjigjet. ")).await.status, StatusCode::NO_CONTENT);
    }
    let kept: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT reason_code, details FROM reports").fetch_all(&pool).await.unwrap();
    assert_eq!(kept, [("OTHER".to_string(), Some("Numri nuk përgjigjet.".to_string()))]);

    // Not one's own listing; not one that isn't on the site; not a number no listing has.
    let ana = token_for(&pool, ANA, "ios").await;
    assert_eq!(report(&pool, &ana, 1001, "SCAM", None).await.status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE listings SET status = 'PAUSED' WHERE public_no = 1006").execute(&pool).await.unwrap();
    for no in [1006, 99999] {
        let refused = report(&pool, &token, no, "SCAM", None).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::NOT_FOUND, &json!("not_found")), "{no}");
    }
    let refused = call(&pool, "POST", "/api/v1/listings/golf/report", Some(&token), Some(json!({"reason": "SCAM"}))).await;
    assert_eq!(refused.status, StatusCode::NOT_FOUND);

    // Twenty reports a day at most, as on the pages: with nineteen more, the next is refused.
    sqlx::query(
        "INSERT INTO reports (reporter_user_id, reported_user_id, reason_code)
         SELECT $1::uuid, $2::uuid, 'SPAM' FROM generate_series(1, 19)",
    )
    .bind(&reporter)
    .bind(BESNIK)
    .execute(&pool)
    .await
    .unwrap();
    let limited = report(&pool, &token, 1005, "SCAM", None).await;
    let error = limited.json()["error"].clone();
    assert_eq!(
        (limited.status, &error["code"], &error["message"]),
        (StatusCode::TOO_MANY_REQUESTS, &json!("rate_limited"), &json!("Ke dërguar shumë raportime sot. Provo përsëri nesër."))
    );
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn three_people_reporting_a_serious_reason_take_a_listing_off_the_site(pool: PgPool) {
    let mut reporters = Vec::new();
    for phone in ["+355692220001", "+355692220002", "+355692220003", "+355692220004"] {
        reporters.push(signed_in(&pool, phone).await);
    }
    // Two people report the Golf as a scam and as fake, and a third that its details are wrong,
    // which is for a moderator to look at: it stays up.
    for (token, reason) in [(&reporters[0], "SCAM"), (&reporters[1], "FAKE"), (&reporters[2], "WRONG_DETAILS")] {
        assert_eq!(report(&pool, token, 1001, reason, None).await.status, StatusCode::NO_CONTENT, "{reason}");
    }
    // The first again: still one person.
    assert_eq!(report(&pool, &reporters[0], 1001, "SPAM", None).await.status, StatusCode::NO_CONTENT);
    assert!(listed(&pool, None).await.contains(&1001));
    // A third person with a serious reason: off the site, until a moderator decides.
    assert_eq!(report(&pool, &reporters[3], 1001, "SPAM", None).await.status, StatusCode::NO_CONTENT);
    assert!(!listed(&pool, None).await.contains(&1001));
    let status: String = sqlx::query_scalar("SELECT status FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap();
    assert_eq!(status, "SUSPENDED");
    // Nothing more to report there.
    assert_eq!(report(&pool, &reporters[2], 1001, "SCAM", None).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_blocked_seller_leaves_the_buyers_lists_until_let_be(pool: PgPool) {
    let token = signed_in(&pool, "+355692223344").await;
    let everyone = listed(&pool, None).await;
    assert!(everyone.contains(&1002) && everyone.contains(&1008));
    // Dritan blocked, once however often: the Mercedes he posted for his store, and his own job,
    // leave the buyer's lists; not anyone else's.
    for _ in 0..2 {
        let blocked = call(&pool, "PUT", &format!("/api/v1/sellers/{DRITAN}/blocked"), Some(&token), None).await;
        assert_eq!(blocked.status, StatusCode::NO_CONTENT, "{}", blocked.body);
    }
    let blocks: i64 = sqlx::query_scalar("SELECT count(*) FROM user_blocks").fetch_one(&pool).await.unwrap();
    assert_eq!(blocks, 1);
    let theirs = listed(&pool, Some(&token)).await;
    assert_eq!(theirs.len(), everyone.len() - 2);
    assert!(!theirs.contains(&1002) && !theirs.contains(&1008), "{theirs:?}");
    assert_eq!(listed(&pool, None).await, everyone);
    // Let be again, once however often: back.
    for _ in 0..2 {
        assert_eq!(call(&pool, "DELETE", &format!("/api/v1/sellers/{DRITAN}/blocked"), Some(&token), None).await.status, StatusCode::NO_CONTENT);
    }
    assert_eq!(listed(&pool, Some(&token)).await, everyone);

    // Not oneself, nor an account that isn't on the site, nor an id that isn't one.
    let ana = token_for(&pool, ANA, "ios").await;
    for (who, path) in [
        (&ana, format!("/api/v1/sellers/{ANA}/blocked")),
        (&token, "/api/v1/sellers/00000000-0000-4000-8000-000000000000/blocked".to_string()),
        (&token, "/api/v1/sellers/dritan/blocked".to_string()),
    ] {
        let refused = call(&pool, "PUT", &path, Some(who), None).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::NOT_FOUND, &json!("not_found")), "{path}");
    }
    // Nobody signed in: 401.
    for method in ["PUT", "DELETE"] {
        let refused = call(&pool, method, &format!("/api/v1/sellers/{DRITAN}/blocked"), None, None).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::UNAUTHORIZED, &json!("unauthorized")), "{method}");
    }
}
