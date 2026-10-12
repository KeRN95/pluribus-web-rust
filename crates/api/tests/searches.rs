//! Saved searches through the API (crates/api/src/searches.rs, pluribus-services' searches.rs), on
//! the pages' sample listings (crates/web/tests/fixtures/listings.sql): a search kept from a car
//! page's filters, written and named as its page writes and names it, listed, its alerts off and
//! on, let go, another user's not found and a path that isn't a search's refused; its listings
//! paged without a gap or a repeat, the first page opening it; one search more than the most
//! refused; and a search kept from the app the very one its page keeps, for every kind of filter.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use common::*;

/// The fixtures' Ana, who sells the Golf (1001), and Besnik.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";

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

/// Keeps the search at `path` with `query` for the user signed in with `token`, answered 200: the
/// search.
async fn keep(pool: &PgPool, token: &str, path: &str, query: &str) -> Value {
    let kept = call(pool, "POST", "/api/v1/searches", Some(token), Some(json!({"path": path, "query": query}))).await;
    assert_eq!(kept.status, StatusCode::OK, "{path}?{query}: {}", kept.body);
    kept.json()
}

/// The user's searches, answered 200.
async fn searches(pool: &PgPool, token: &str) -> Value {
    let list = call(pool, "GET", "/api/v1/searches", Some(token), None).await;
    assert_eq!(list.status, StatusCode::OK, "{}", list.body);
    list.json()
}

/// The validation envelope's field for an answer that must be one.
fn invalid(answer: &ApiResponse) -> Vec<String> {
    assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.body);
    let error = &answer.json()["error"];
    assert_eq!(error["code"], json!("validation"));
    error["fields"].as_object().expect("fields").keys().cloned().collect()
}

/// The numbers of a list's cards, in its order.
fn numbers(list: &Value) -> Vec<i64> {
    list.as_array().unwrap_or_else(|| panic!("not a list: {list}")).iter().map(|card| card["no"].as_i64().unwrap()).collect()
}

/// A new account, signed in from `client`: its id and its token.
async fn user(pool: &PgPool, phone: &str, client: &str) -> (String, String) {
    let id = new_user(pool, phone, "Blerina").await;
    let token = token_for(pool, &id, client).await;
    (id, token)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_search_is_kept_listed_switched_and_let_go(pool: PgPool) {
    let (_, token) = user(&pool, "+355692223344", "android").await;
    let ana = token_for(&pool, ANA, "ios").await;

    // Kept from a car page's filters, asked for in any order, with an order, a cursor and what the
    // page doesn't know: written and named as the page writes and names it, without them.
    let kept = keep(&pool, &token, "/c/makina", "vehicle_fuel=DIESEL&sort=price_asc&city=tirane&after=x&ngjyra=kuqe").await;
    let id = kept["id"].as_str().expect("an id").to_string();
    let created = OffsetDateTime::parse(kept["created_at"].as_str().unwrap(), &Rfc3339).unwrap();
    assert!((OffsetDateTime::now_utc() - created).whole_seconds().abs() < 60, "{kept}");
    let mut shown = kept.clone();
    shown["created_at"] = json!(null);
    assert_eq!(
        shown,
        json!({
            "id": id,
            "name": "Makina · Tiranë · Naftë",
            "path": "/c/makina",
            "query": "city=tirane&vehicle_fuel=DIESEL",
            "alerts": true,
            "new_count": {"n": 0, "more": false},
            "created_at": null,
            "last_seen_at": null,
        })
    );
    // Kept again, the same filters asked for otherwise ("?" before them too): the one search.
    assert_eq!(keep(&pool, &token, "/c/makina", "?city=tirane&vehicle_fuel=DIESEL").await["id"], json!(id));
    assert_eq!(searches(&pool, &token).await, json!([kept]));
    // A car put up in Tiranë since: one new listing.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at)
         SELECT $1::uuid, $1::uuid, (SELECT id FROM categories WHERE slug = 'makina'),
                (SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'),
                'golf-i-ri', 'Golf i ri', '', 9000, 9000, 'EUR', 'ACTIVE', now()",
    )
    .bind(BESNIK)
    .execute(&pool)
    .await
    .unwrap();
    // A diesel car's fuel, so that it is the search's: as the fixtures give the Golf its own.
    sqlx::query(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
         SELECT l.id, a.id, a.data_type, o.id
         FROM listings l, attributes a JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = 'DIESEL'
         WHERE l.slug = 'golf-i-ri' AND a.code = 'vehicle_fuel'",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(searches(&pool, &token).await[0]["new_count"], json!({"n": 1, "more": false}));

    // Its alerts off, then on again.
    for on in [false, true] {
        let switched = call(&pool, "PUT", &format!("/api/v1/searches/{id}/alerts"), Some(&token), Some(json!({"on": on}))).await;
        assert_eq!(switched.status, StatusCode::NO_CONTENT, "{}", switched.body);
        assert_eq!(searches(&pool, &token).await[0]["alerts"], json!(on));
    }
    // Without saying which: the validation envelope, on the body.
    let unsaid = call(&pool, "PUT", &format!("/api/v1/searches/{id}/alerts"), Some(&token), Some(json!({}))).await;
    assert_eq!(invalid(&unsaid), ["body"]);

    // Anyone else's: not found, to page, to switch, to let go; nor is an id that isn't one.
    for (method, path, body) in [
        ("GET", format!("/api/v1/searches/{id}/listings"), None),
        ("PUT", format!("/api/v1/searches/{id}/alerts"), Some(json!({"on": false}))),
        ("DELETE", format!("/api/v1/searches/{id}"), None),
        ("DELETE", "/api/v1/searches/makina".to_string(), None),
        ("GET", "/api/v1/searches/makina/listings".to_string(), None),
    ] {
        let refused = call(&pool, method, &path, Some(&ana), body).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::NOT_FOUND, &json!("not_found")), "{method} {path}");
    }
    assert_eq!(searches(&pool, &ana).await, json!([]));
    assert_eq!(searches(&pool, &token).await[0]["alerts"], json!(true));

    // Only a category's page or a search; a category that doesn't exist, the page's or the one a
    // search names; nothing kept for any of them.
    for (path, query, field) in [
        ("/mesazhet", "", "path"),
        ("/listing/1001", "", "path"),
        ("/c/nuk-ka", "city=tirane", "path"),
        ("/search", "category=nuk-ka", "query"),
    ] {
        let refused = call(&pool, "POST", "/api/v1/searches", Some(&token), Some(json!({"path": path, "query": query}))).await;
        assert_eq!(invalid(&refused), [field], "{path}?{query}");
    }
    let refused = call(&pool, "POST", "/api/v1/searches?lang=en", Some(&token), Some(json!({"path": "/mesazhet"}))).await;
    assert_eq!(refused.json()["error"]["fields"]["path"], json!("Only a category’s page (/c/…) or a search (/search) can be saved."));
    assert_eq!(searches(&pool, &token).await.as_array().unwrap().len(), 1);

    // Let go, once: then not found.
    assert_eq!(call(&pool, "DELETE", &format!("/api/v1/searches/{id}"), Some(&token), None).await.status, StatusCode::NO_CONTENT);
    assert_eq!(searches(&pool, &token).await, json!([]));
    assert_eq!(call(&pool, "DELETE", &format!("/api/v1/searches/{id}"), Some(&token), None).await.status, StatusCode::NOT_FOUND);

    // Nobody signed in: 401 in the envelope, whatever is asked.
    for (method, path, body) in [
        ("GET", "/api/v1/searches".to_string(), None),
        ("POST", "/api/v1/searches".to_string(), Some(json!({"path": "/search", "query": "q=golf"}))),
        ("DELETE", format!("/api/v1/searches/{id}"), None),
        ("PUT", format!("/api/v1/searches/{id}/alerts"), Some(json!({"on": true}))),
        ("GET", format!("/api/v1/searches/{id}/listings"), None),
    ] {
        let refused = call(&pool, method, &path, None, body).await;
        assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::UNAUTHORIZED, &json!("unauthorized")), "{method} {path}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_searchs_listings_page_without_gaps_and_the_first_opens_it(pool: PgPool) {
    // Thirty more cars in Tiranë from Besnik, each a minute older than the last: with Ana's Golf,
    // 31, two pages.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at)
         SELECT $1::uuid, $1::uuid, (SELECT id FROM categories WHERE slug = 'makina'),
                (SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'),
                'makine-' || g, 'Makinë shtesë ' || g, '', 1000 + g, 1000 + g, 'EUR', 'ACTIVE',
                now() - g * interval '1 minute'
         FROM generate_series(1, 30) g",
    )
    .bind(BESNIK)
    .execute(&pool)
    .await
    .unwrap();
    let expected: Vec<i64> = sqlx::query_scalar(
        "WITH RECURSIVE tree AS (SELECT id FROM categories WHERE slug = 'makina'
                                 UNION ALL SELECT c.id FROM categories c JOIN tree ON c.parent_id = tree.id)
         SELECT public_no FROM listings
         WHERE category_id IN (SELECT id FROM tree) AND status = 'ACTIVE'
           AND location_id = (SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY')
         ORDER BY sort_at DESC, id DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(expected.len(), 31);

    let (buyer, token) = user(&pool, "+355692223344", "android").await;
    let id = keep(&pool, &token, "/c/makina", "city=tirane").await["id"].as_str().unwrap().to_string();
    // An alert of it the buyer hasn't seen (searches.rs on the pages records them).
    let (buyer_id, search_id): (Uuid, Uuid) = (buyer.parse().unwrap(), id.parse().unwrap());
    pluribus_db::record_search_alert(&pool, buyer_id, search_id, "1 njoftim i ri: Makina · Tiranë", 1, "https://pluribus.example")
        .await
        .unwrap();
    assert_eq!(pluribus_db::unseen_search_alerts(&pool, buyer_id).await.unwrap(), 1);

    // The first page opens it: its alert seen, its new listings counted from now.
    let page = |after: Option<String>| {
        let (pool, token, id) = (pool.clone(), token.clone(), id.clone());
        async move {
            let path = match after {
                Some(after) => format!("/api/v1/searches/{id}/listings?after={after}"),
                None => format!("/api/v1/searches/{id}/listings"),
            };
            let page = call(&pool, "GET", &path, Some(&token), None).await;
            assert_eq!(page.status, StatusCode::OK, "{path}: {}", page.body);
            page.json()
        }
    };
    let first = page(None).await;
    assert_eq!(pluribus_db::unseen_search_alerts(&pool, buyer_id).await.unwrap(), 0);
    let opened = searches(&pool, &token).await[0]["last_seen_at"].clone();
    assert!(opened.is_string(), "{opened}");
    // Then on, each next page asked for by the last one's `next`, until there is none: every
    // listing once, in the page's order. The pages after the first only read it.
    let (mut seen, mut next, mut pages) = (numbers(&first["items"]), first["next"].as_str().map(str::to_string), 1);
    while let Some(after) = next {
        let more = page(Some(after)).await;
        seen.extend(numbers(&more["items"]));
        next = more["next"].as_str().map(str::to_string);
        pages += 1;
        assert!(pages < 10, "no end");
    }
    assert_eq!((seen, pages), (expected, 2));
    assert_eq!(searches(&pool, &token).await[0]["last_seen_at"], opened);

    // The accounts the buyer blocked are left out of them, as out of every list: Besnik's thirty.
    let blocked = call(&pool, "PUT", &format!("/api/v1/sellers/{BESNIK}/blocked"), Some(&token), None).await;
    assert_eq!(blocked.status, StatusCode::NO_CONTENT, "{}", blocked.body);
    let page = call(&pool, "GET", &format!("/api/v1/searches/{id}/listings"), Some(&token), None).await.json();
    assert_eq!((numbers(&page["items"]), &page["next"]), (vec![1001], &Value::Null));

    // A cursor that doesn't read: the validation envelope, on `after`, in the language asked for.
    let unread = call(&pool, "GET", &format!("/api/v1/searches/{id}/listings?after=soon&lang=en"), Some(&token), None).await;
    assert_eq!(invalid(&unread), ["after"]);
    assert_eq!(unread.json()["error"]["fields"]["after"], json!("Where the list goes on from can’t be read. Start again from the first page."));
    // One whose category is gone no longer reads: its count null, its listings not found.
    sqlx::query("UPDATE saved_searches SET filters = jsonb_build_object('path', '/c/e-hequr', 'query', '') WHERE id = $1::uuid")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(searches(&pool, &token).await[0]["new_count"], Value::Null);
    assert_eq!(call(&pool, "GET", &format!("/api/v1/searches/{id}/listings"), Some(&token), None).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn one_search_more_than_the_most_is_refused(pool: PgPool) {
    let (buyer, token) = user(&pool, "+355692223344", "android").await;
    sqlx::query(
        "INSERT INTO saved_searches (user_id, name, filters)
         SELECT $1::uuid, 'Kërkimi ' || g, jsonb_build_object('path', '/search', 'query', 'q=fjala' || g)
         FROM generate_series(1, $2::int) g",
    )
    .bind(&buyer)
    .bind(pluribus_db::SEARCHES_MOST as i32)
    .execute(&pool)
    .await
    .unwrap();
    // One of those kept is kept still; one more is refused, on `searches`.
    assert_eq!(keep(&pool, &token, "/search", "q=fjala1").await["name"], json!("Kërkimi 1"));
    let refused = call(&pool, "POST", "/api/v1/searches?lang=en", Some(&token), Some(json!({"path": "/search", "query": "q=golf"}))).await;
    assert_eq!(invalid(&refused), ["searches"]);
    // Said as the page says it, in the field and as the envelope's message.
    let why = json!(format!("You’ve saved {} searches, as many as you can keep: delete one to save another.", pluribus_db::SEARCHES_MOST));
    assert_eq!((&refused.json()["error"]["fields"]["searches"], &refused.json()["error"]["message"]), (&why, &why));
    assert_eq!(searches(&pool, &token).await.as_array().unwrap().len(), pluribus_db::SEARCHES_MOST as usize);
}

/// `value` as a form's field writes it (application/x-www-form-urlencoded).
fn form_value(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => char::from(byte).to_string(),
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_search_kept_from_the_app_is_the_one_its_page_keeps(pool: PgPool) {
    // One user keeps each search with the pages' own "Ruaj kërkimin", signed in by its cookie; the
    // other through the API.
    let (_, page_token) = user(&pool, "+355692223344", "web").await;
    let (_, app_token) = user(&pool, "+355692223355", "ios").await;
    let long = "fjalë ".repeat(18);
    let searches_kept = [
        ("/c/makina", "vehicle_fuel=DIESEL&city=tirane".to_string()),
        (
            "/c/makina",
            "q=golf&vehicle_transmission=MANUAL&vehicle_fuel=DIESEL&vehicle_fuel=HYBRID&price_max=10000&currency=EUR\
             &vehicle_mileage_max=150000&vehicle_year_min=2015&condition=USED&vehicle_no_accident=1&vehicle_make=VOLKSWAGEN\
             &vehicle_model=VOLKSWAGEN_GOLF&type=SELL&sort=price_asc"
                .to_string(),
        ),
        (
            "/search",
            "category=apartamente&zone=plazh&city=durres&type=RENT&price_min=300&price_max=600&currency=EUR&property_rooms=R21&rent_pets=1"
                .to_string(),
        ),
        ("/search", "lat=41.3275&lng=19.8187&radius=20&q=iphone".to_string()),
        ("/search", "q=libra".to_string()),
        ("/search", String::new()),
        ("/c/prona", "city=tirane&zone=blloku&property_area_min=60&type=SELL".to_string()),
        ("/c/telefona", "electronics_brand=APPLE&phone_unlocked=1&condition=NEW&condition=LIKE_NEW&price_min=20 000&currency=ALL".to_string()),
        ("/c/pjese-kembimi", "fits_year=2006&vehicle_make=AUDI".to_string()),
        ("/c/makina", format!("q={long}&city=tirane&vehicle_fuel=DIESEL&vehicle_transmission=MANUAL&vehicle_inspection=1")),
    ];
    for (path, query) in &searches_kept {
        let address = if query.is_empty() { path.to_string() } else { format!("{path}?{query}") };
        let request = Request::post("/kerkimet/ruaj")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("session={page_token}"))
            .body(Body::from(format!("adresa={}", form_value(&address))))
            .unwrap();
        let kept = send(api(pool.clone()), request).await;
        assert_eq!(kept.status, StatusCode::SEE_OTHER, "{address}: {}", kept.body);
        keep(&pool, &app_token, path, query).await;
    }
    let shown = |list: Value| -> Vec<(Value, Value, Value)> {
        list.as_array().unwrap().iter().map(|search| (search["name"].clone(), search["path"].clone(), search["query"].clone())).collect()
    };
    let (pages, app) = (shown(searches(&pool, &page_token).await), shown(searches(&pool, &app_token).await));
    assert_eq!(app.len(), searches_kept.len());
    assert_eq!(app, pages);
    // What they say, the latest kept first.
    let names: Vec<&str> = app.iter().map(|(name, _, _)| name.as_str().unwrap()).collect();
    assert!(names[0].starts_with("„fjalë fjalë") && names[0].ends_with('…') && names[0].chars().count() == 120, "{}", names[0]);
    assert_eq!(names[3], "Prona · Tiranë · Blloku · Në shitje · Sipërfaqja: nga 60\u{a0}m²");
    assert_eq!(names[4], "Të gjitha njoftimet");
    assert_eq!(names[6], "„iphone“ · Brenda 20 km nga pika");
    assert_eq!(names[7], "Durrës · Plazh · Apartamente · Me qira · 300 – 600\u{a0}€ · 2+1 · Lejohen kafshët");
    assert_eq!(names[9], "Makina · Tiranë · Naftë");
    let queries: Vec<&str> = app.iter().map(|(_, _, query)| query.as_str().unwrap()).collect();
    assert_eq!(queries[6], "q=iphone&lat=41.32750&lng=19.81870&radius=20");
    assert_eq!(queries[7], "category=apartamente&city=durres&zone=plazh&type=RENT&price_min=300&price_max=600&currency=EUR&property_rooms=R21&rent_pets=1");
}
