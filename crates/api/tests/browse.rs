//! Browsing through the API (crates/api/src/browse.rs), on the pages' sample listings
//! (crates/web/tests/fixtures/listings.sql): `/listings` read in the pages' own query vocabulary,
//! paged by its cursor, counted (the count alone too, `count=precise`'s, with `count=only`),
//! narrowed by a category, a price, a property, a town and words, the
//! listings of an account the caller blocked left out; the cards it answers; `/home`'s rows, those
//! recommended paged on through `/listings`; and the envelope for a category that doesn't exist and
//! a cursor that doesn't read.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use pluribus_services::config::Config;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

/// The fixtures' sellers: Ana, private; Besnik, private; Dritan, who posts for Auto Durrësi.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";

/// The numbers of a list's cards, in its order.
fn numbers(list: &Value) -> Vec<i64> {
    list.as_array().unwrap_or_else(|| panic!("not a list: {list}")).iter().map(|card| card["no"].as_i64().unwrap()).collect()
}

/// `/api/v1/listings` with `query`, answered 200: its JSON.
async fn listings(pool: &PgPool, query: &str, token: Option<&str>) -> Value {
    let answer = get(pool.clone(), &format!("/api/v1/listings?{query}"), token).await;
    assert_eq!(answer.status, StatusCode::OK, "{query}: {}", answer.body);
    answer.json()
}

/// Every page of `/listings` with `query`, from its first: their cards' numbers, in order, and the
/// first page's count.
async fn every_page(pool: &PgPool, query: &str, token: Option<&str>) -> (Vec<i64>, Value) {
    pages_from(pool, query, None, token).await
}

/// Every page of `/listings` with `query` from the one `after` names (the first, without), each
/// next one asked for with the same query and the last one's `next` as `after`, until there is
/// none: their cards' numbers, in order, and the first page's count.
async fn pages_from(pool: &PgPool, query: &str, mut after: Option<String>, token: Option<&str>) -> (Vec<i64>, Value) {
    let (mut seen, mut count) = (Vec::new(), None);
    loop {
        let address = match &after {
            Some(after) => format!("{query}&after={after}"),
            None => query.to_string(),
        };
        let page = listings(pool, &address, token).await;
        count.get_or_insert_with(|| page["count"].clone());
        seen.extend(numbers(&page["items"]));
        let Some(next) = page["next"].as_str() else { break };
        // A cursor goes into an address as it is.
        assert!(next.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)), "{next}");
        assert!(seen.len() < 1_000, "{address}: no end");
        after = Some(next.to_string());
    }
    (seen, count.expect("a first page"))
}

/// The numbers of the listings with these slugs, in the order of the slugs given.
async fn numbered(pool: &PgPool, slugs: &[&str]) -> Vec<i64> {
    let mut numbers = Vec::new();
    for slug in slugs {
        numbers.push(sqlx::query_scalar("SELECT public_no FROM listings WHERE slug = $1").bind(slug).fetch_one(pool).await.unwrap());
    }
    numbers
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn listings_page_by_their_cursor_without_gaps_or_repeats(pool: PgPool) {
    // 40 more in Të tjera, as the pages' price_pages_continue_where_the_last_one_ended has them:
    // 30 with prices, many of them equal so that the id breaks ties, and 10 on request.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                CASE WHEN g <= 30 THEN g % 7 * 1000 END, 'ALL',
                CASE WHEN g <= 30 THEN 'FIXED' ELSE 'ON_REQUEST' END,
                CASE WHEN g <= 30 THEN g % 7 * 10 END,
                'ACTIVE', now() - g * interval '1 minute'
         FROM generate_series(1, 40) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    for (query, order) in [
        ("category=te-tjera", "sort_at DESC, id DESC"),
        ("category=te-tjera&sort=price_asc", "price_normalized ASC NULLS LAST, id ASC"),
        ("category=te-tjera&sort=price_desc", "price_normalized DESC NULLS LAST, id DESC"),
    ] {
        let expected: Vec<i64> = sqlx::query_scalar(&format!(
            "SELECT public_no FROM listings
             WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
             ORDER BY {order}"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(expected.len(), 41);
        let (seen, count) = every_page(&pool, query, None).await;
        assert_eq!(seen, expected, "{query}");
        assert_eq!(count, json!({"n": 41, "more": false}), "{query}");
    }
    // Two pages: the first PAGE_SIZE, then the rest, the last with no next.
    let first = listings(&pool, "category=te-tjera", None).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 24);
    let rest = listings(&pool, &format!("category=te-tjera&after={}", first["next"].as_str().unwrap()), None).await;
    assert_eq!((rest["items"].as_array().unwrap().len(), &rest["next"]), (17, &Value::Null));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_category_a_price_a_property_and_a_town_narrow_the_listings_and_their_count(pool: PgPool) {
    let all = listings(&pool, "", None).await;
    assert_eq!(all["count"], json!({"n": 12, "more": false}));
    // The newest first, the house in Vlorë bumped to the top ten minutes ago.
    assert_eq!(numbers(&all["items"])[..3], [1004, 1005, 1003]);
    for (query, expected) in [
        ("category=automjete", vec![1001, 1002]),
        ("category=automjete&price_max=10000&currency=EUR", vec![1001]),
        // A price typed as people type it, and the bounds the wrong way round.
        ("category=automjete&price_min=10.000&price_max=5000&currency=EUR", vec![1001]),
        ("category=makina&vehicle_transmission=AUTOMATIC", vec![1002]),
        ("category=elektronike&price_min=60000&currency=ALL", vec![1006]),
        ("category=prona&type=RENT", vec![1003]),
        ("condition=NEW", vec![1012]),
        ("city=tirane", vec![1005, 1003, 1001, 1012]),
        // What doesn't exist is ignored, as on the pages.
        ("category=automjete&vehicle_fuel=NUCLEAR&city=atlantis&sort=sideways", vec![1001, 1002]),
    ] {
        let found = listings(&pool, query, None).await;
        assert_eq!(numbers(&found["items"]), expected, "{query}");
        assert_eq!(found["count"], json!({"n": expected.len(), "more": false}), "{query}");
        assert!(found["next"].is_null(), "{query}");
    }
    // Asked to count further, as a page does once it shows: the same, here.
    assert_eq!(listings(&pool, "category=automjete&count=precise", None).await["count"], json!({"n": 2, "more": false}));

    // A point on the map and the distance around it (each listing stands at its town's point):
    // the nearest first, Durrës's two, then Tirana's four, some 30 km off, each town's in their
    // ids' order; each saying how far it is.
    let (latitude, longitude): (f64, f64) = sqlx::query_as(
        "SELECT latitude::float8, longitude::float8 FROM locations WHERE slug = 'durres' AND location_type = 'MUNICIPALITY'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let near = listings(&pool, &format!("lat={latitude}&lng={longitude}&radius=50"), None).await;
    assert_eq!(numbers(&near["items"]), [1002, 1008, 1001, 1003, 1005, 1012], "{near}");
    let distance = |at: usize| near["items"][at]["distance_m"].as_f64().unwrap();
    assert!(distance(0) < 1.0 && (20_000.0..50_000.0).contains(&distance(2)), "{near}");
    assert_eq!(listings(&pool, "", None).await["items"][0]["distance_m"], Value::Null);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn count_only_answers_the_precise_count_alone(pool: PgPool) {
    // A thousand more in Të tjera, past what a count goes up to (1 000) and within a precise one's.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title, description,
                               price_amount, currency, status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 'ACTIVE', now() - g * interval '1 minute'
         FROM generate_series(1, 1000) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(listings(&pool, "", None).await["count"], json!({"n": 1000, "more": true}));
    assert_eq!(listings(&pool, "count=only", None).await, json!({"count": {"n": 1012, "more": false}}));
    // The same count as a precise one's, whatever the query: words, a category, a town, a price, a
    // property, a recommended list's seeds; nothing besides it.
    for query in [
        "q=golf",
        "category=automjete",
        "city=tirane&condition=USED",
        "category=te-tjera&price_max=100&currency=ALL",
        "category=makina&vehicle_transmission=AUTOMATIC",
        "sipas=1001x10",
    ] {
        let precise = listings(&pool, &format!("{query}&count=precise"), None).await;
        assert_eq!(listings(&pool, &format!("{query}&count=only"), None).await, json!({"count": precise["count"]}), "{query}");
    }
    // A signed-in buyer's leaves out the sellers they blocked, as their list does.
    let buyer = new_user(&pool, "+355692223344", "Blerina").await;
    let token = token_for(&pool, &buyer, "android").await;
    sqlx::query("INSERT INTO user_blocks (blocker_user_id, blocked_user_id) VALUES ($1::uuid, $2::uuid)")
        .bind(&buyer)
        .bind(BESNIK)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(listings(&pool, "count=only", Some(&token)).await, json!({"count": {"n": 1007, "more": false}}), "without Besnik's five");
    let precise = listings(&pool, "category=prona&count=precise", Some(&token)).await;
    assert_eq!(listings(&pool, "category=prona&count=only", Some(&token)).await, json!({"count": precise["count"]}));
    // A cursor that doesn't read is refused still.
    let refused = get(pool, "/api/v1/listings?after=garbage&count=only", None).await;
    assert_eq!((refused.status, &refused.json()["error"]["code"]), (StatusCode::UNPROCESSABLE_ENTITY, &json!("validation")));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn words_are_searched_as_the_pages_search_them(pool: PgPool) {
    let golf = listings(&pool, "q=golf", None).await;
    assert_eq!((numbers(&golf["items"]), &golf["count"]), (vec![1001], &json!({"n": 1, "more": false})));
    // A property's label finds its listings ("naftë", the diesel cars), and a typo is forgiven.
    assert_eq!(numbers(&listings(&pool, "q=nafte&category=automjete", None).await["items"]), [1001, 1002]);
    assert_eq!(numbers(&listings(&pool, "q=golff", None).await["items"]), [1001]);
    assert_eq!(numbers(&listings(&pool, "q=iphone", None).await["items"]), [1005]);
    assert_eq!(listings(&pool, "q=zzzqqq", None).await["count"], json!({"n": 0, "more": false}));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_blocked_sellers_listings_are_left_out_for_the_buyer_who_blocked_them(pool: PgPool) {
    let buyer = new_user(&pool, "+355692223344", "Blerina").await;
    let token = token_for(&pool, &buyer, "android").await;
    sqlx::query("INSERT INTO user_blocks (blocker_user_id, blocked_user_id) VALUES ($1::uuid, $2::uuid)")
        .bind(&buyer)
        .bind(BESNIK)
        .execute(&pool)
        .await
        .unwrap();
    let besnik: Vec<i64> = vec![1003, 1004, 1006, 1009, 1011];

    let (theirs, count) = every_page(&pool, "", Some(&token)).await;
    assert_eq!(count, json!({"n": 7, "more": false}));
    assert!(theirs.iter().all(|no| !besnik.contains(no)), "{theirs:?}");
    // Anyone else, and the buyer signed out, see them.
    let (everyone, count) = every_page(&pool, "", None).await;
    assert_eq!((everyone.len(), count), (12, json!({"n": 12, "more": false})));
    // Searched for by name too.
    assert_eq!(numbers(&listings(&pool, "q=laptop", Some(&token)).await["items"]), Vec::<i64>::new());
    assert_eq!(numbers(&listings(&pool, "q=laptop", None).await["items"]), [1006]);

    // Nor on the front page: neither among those seen lately, nor among the most seen.
    sqlx::query(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views)
         SELECT id, current_date, 10 FROM listings WHERE public_no IN (1003, 1005)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let home = get(pool.clone(), "/api/v1/home?seen=1003.1005", Some(&token)).await.json();
    assert_eq!((numbers(&home["seen_lately"]), numbers(&home["most_viewed"])), (vec![1005], vec![1005]));
    let home = get(pool.clone(), "/api/v1/home?seen=1003.1005", None).await.json();
    assert_eq!(numbers(&home["seen_lately"]), [1003, 1005]);
    assert_eq!(numbers(&home["most_viewed"]).len(), 2);

    // A token no good session has is refused, rather than answered as for nobody.
    let stale = get(pool.clone(), "/api/v1/listings", Some("not-a-session")).await;
    assert_eq!((stale.status, &stale.json()["error"]["code"]), (StatusCode::UNAUTHORIZED, &json!("unauthorized")));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_card_carries_what_its_card_and_stand_in_show(pool: PgPool) {
    // The Golf's photo kept as WebP, with its copies, on the site's own /media.
    sqlx::query(
        "UPDATE listing_media
         SET public_url = '/media/photos/ab12.webp', thumbnail_url = '/media/photos/ab12-card.webp',
             middle_url = '/media/photos/ab12-middle.webp', jpeg_url = '/media/photos/ab12.jpg',
             placeholder = 'data:image/webp;base64,UklGRg==', mime_type = 'image/webp', width_px = 1600, height_px = 1200
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1001)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = request("GET", "/api/v1/listings?category=automjete", None)
        .header(header::HOST, "pluribus.test")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap();
    let answer = send(api(pool.clone()), request).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let cards = answer.json()["items"].clone();
    // The site's address: PUBLIC_URL when the settings give one, else the one the request came to,
    // the tests' settings giving none.
    let site = Config::for_tests().public_url.unwrap_or_else(|| "https://pluribus.test".to_string());
    let when: OffsetDateTime =
        sqlx::query_scalar("SELECT sort_at FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap();
    let golf = &cards[0];
    // Its properties as the database gives them, which the pages word themselves.
    let details = golf["details"].as_array().expect("details");
    assert!(details.iter().any(|detail| *detail == json!({"code": "vehicle_year", "number": "2016"})), "{details:?}");
    // A choice's English as the English pages say it (the catalogue's).
    assert!(
        details.iter().any(|detail| *detail == json!({"code": "vehicle_fuel", "label": "Naftë", "label_en": "Diesel"})),
        "{details:?}"
    );
    let mut golf = golf.clone();
    golf["details"] = json!([]);
    assert_eq!(
        golf,
        json!({
            "no": 1001,
            "slug": "volkswagen-golf-7-2016-nafte",
            "title": "Volkswagen Golf 7, 2016, naftë",
            "heading": null,
            "deal": true,
            "kind": "SELL",
            "condition": "USED",
            "status": "ACTIVE",
            "price": {"amount": "9500", "currency": "EUR", "type": "FIXED", "was": null, "per_month": false},
            "photo": {
                "card": format!("{site}/media/photos/ab12-card.webp?2"),
                "card_webp": format!("{site}/media/photos/ab12-card.webp?2"),
                "full": format!("{site}/media/photos/ab12.jpg"),
                "placeholder": "data:image/webp;base64,UklGRg==",
                "width": 1600,
                "height": 1200,
            },
            "place": "Tiranë",
            "when": when.format(&Rfc3339).unwrap(),
            "details": [],
            "promoted": false,
            "highlighted": false,
            "good_price": false,
            "seller_kind": "user",
            "views": null,
            "distance_m": null,
        })
    );
    // The Mercedes, a business's, negotiable; its photo kept before WebP, from elsewhere, as it is.
    let mercedes = &cards[1];
    assert_eq!((&mercedes["no"], &mercedes["seller_kind"]), (&json!(1002), &json!("business")));
    assert_eq!(mercedes["price"], json!({"amount": "21500", "currency": "EUR", "type": "NEGOTIABLE", "was": null, "per_month": false}));
    let photo = &mercedes["photo"];
    assert!(photo["card"].as_str().unwrap().starts_with("https://images.unsplash.com/photo-1624085568108"), "{photo}");
    assert_eq!((&photo["card_webp"], &photo["placeholder"], &photo["width"]), (&Value::Null, &Value::Null, &Value::Null));
    // A rent is a month's; a job's pay on request has no amount; something free neither.
    let flat = &listings(&pool, "category=prona&type=RENT", None).await["items"][0];
    assert_eq!((&flat["kind"], &flat["condition"], &flat["price"]["per_month"]), (&json!("RENT"), &Value::Null, &json!(true)));
    let job = &listings(&pool, "category=pune", None).await["items"][0];
    assert_eq!((&job["kind"], &job["price"]["type"], &job["price"]["amount"]), (&json!("JOB"), &json!("ON_REQUEST"), &Value::Null));
    let books = &listings(&pool, "q=libra", None).await["items"][0];
    assert_eq!((&books["price"]["type"], &books["price"]["amount"]), (&json!("FREE"), &Value::Null));

    // A price dropped lately says what it was; a promoted, highlighted listing says so.
    sqlx::query("UPDATE listings SET price_amount = 9000, price_normalized = 9000, featured_until = now() + interval '1 day', highlighted_until = now() + interval '1 day' WHERE public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    let golf = &listings(&pool, "q=golf", None).await["items"][0];
    assert_eq!((&golf["price"]["amount"], &golf["price"]["was"]), (&json!("9000"), &json!("9500")));
    assert_eq!((&golf["promoted"], &golf["highlighted"]), (&json!(true), &json!(true)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn home_gives_the_front_pages_rows_and_pages_its_recommendations_on(pool: PgPool) {
    // As the pages' the_welcome_page_recommends_listings_like_those_looked_at: three Golfs from
    // Besnik at about the Golf's price, Ana's own second Golf and two Audis from Dritan, all in
    // Makina in Tirana; a second phone from Ana; and thirty more listings elsewhere.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at,
                               latitude, longitude)
         SELECT v.seller::uuid, v.seller::uuid, (SELECT id FROM categories WHERE slug = 'makina'),
                v.slug, v.title, '', v.price, v.price, 'EUR', 'ACTIVE', now() - interval '1 hour',
                41.3275, 19.8187
         FROM (VALUES
             ('22222222-2222-4222-8222-222222222222', 'golf-a', 'Golf A', 9400),
             ('22222222-2222-4222-8222-222222222222', 'golf-b', 'Golf B', 9700),
             ('22222222-2222-4222-8222-222222222222', 'golf-c', 'Golf C', 9900),
             ('11111111-1111-4111-8111-111111111111', 'golf-d', 'Golf D', 9500),
             ('33333333-3333-4333-8333-333333333333', 'audi-a3', 'Audi A3', 9600),
             ('33333333-3333-4333-8333-333333333333', 'audi-a4', 'Audi A4', 12000)
         ) AS v(seller, slug, title, price)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
         SELECT l.id, a.id, a.data_type, o.id
         FROM listings l
         CROSS JOIN (VALUES ('vehicle_make', 'VOLKSWAGEN'), ('vehicle_model', 'VOLKSWAGEN_GOLF')) AS v(code, value_code)
         JOIN attributes a ON a.code = v.code
         JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = v.value_code
         WHERE l.slug LIKE 'golf-_'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at,
                               latitude, longitude)
         SELECT seller_user_id, created_by_user_id, category_id, 'iphone-12', 'iPhone 12', '', 50000,
                price_normalized * 50000 / 55000, 'ALL', 'ACTIVE', now() - interval '1 hour', latitude, longitude
         FROM listings WHERE public_no = 1005",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g,
                '', 100, 'ALL', 'ACTIVE', now() - g * interval '1 day'
         FROM generate_series(1, 30) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let [audi_a3, audi_a4, golf_a, golf_b, golf_d] = numbered(&pool, &["audi-a3", "audi-a4", "golf-a", "golf-b", "golf-d"]).await[..]
    else {
        unreachable!()
    };
    let mut active: Vec<i64> =
        sqlx::query_scalar("SELECT public_no FROM listings WHERE status = 'ACTIVE'").fetch_all(&pool).await.unwrap();
    active.sort_unstable();
    let home = |query: &'static str, token: Option<String>| {
        let pool = pool.clone();
        async move {
            let answer = get(pool, &format!("/api/v1/home{query}"), token.as_deref()).await;
            assert_eq!(answer.status, StatusCode::OK, "{query}: {}", answer.body);
            answer.json()
        }
    };

    // The Golf seen: the cars like it, the most alike first, as the welcome page recommends them,
    // and the next page's `after` naming the seeds (the Golf, at 1.0) and the place reached.
    let page = home("?seen=1001", None).await;
    assert_eq!(page["for_you"], json!(true));
    assert_eq!(numbers(&page["seen_lately"]), [1001]);
    let recommended = &page["recommended"];
    assert_eq!(numbers(&recommended["items"]), [golf_d, golf_a, golf_b, audi_a3, audi_a4, 1002]);
    assert_eq!(recommended["next"], json!("1001x10~6"));
    // Paged on through /listings, as the welcome page's frames go on: the rest of those ranked,
    // then every other listing, newest first, each listing on the site once in all.
    let mut shown = numbers(&recommended["items"]);
    let (rest, count) = pages_from(&pool, "", Some("1001x10~6".to_string()), None).await;
    assert_eq!(count, json!({"n": active.len(), "more": false}));
    shown.extend(rest);
    shown.sort_unstable();
    assert_eq!(shown, active);
    // The page after it, asked for by its seeds and its place apart, is the same; its next names
    // where the newest go on from, after the seeds.
    let next = listings(&pool, "after=1001x10~6", None).await;
    assert_eq!(listings(&pool, "sipas=1001x10&after=6", None).await["items"], next["items"]);
    assert_eq!(next["items"].as_array().unwrap().len(), 24);
    assert!(next["next"].as_str().is_some_and(|next| next.starts_with("1001x10~") && next.contains('_')), "{next}");
    // From the start, by its seeds alone.
    assert_eq!(numbers(&listings(&pool, "sipas=1001x10", None).await["items"]), numbers(&recommended["items"]));

    // Signed in, Ana isn't recommended her own Golf.
    let ana = token_for(&pool, ANA, "android").await;
    let page = home("?seen=1001", Some(ana)).await;
    assert_eq!(numbers(&page["recommended"]["items"]), [golf_a, golf_b, audi_a3, audi_a4, 1002]);
    // A listing kept seeds them too (at 1.5), the app's own kept with nothing seen.
    let page = home("?kept=1001", None).await;
    assert_eq!(numbers(&page["recommended"]["items"]), [golf_d, golf_a, golf_b, audi_a3, audi_a4, 1002]);
    assert_eq!(page["recommended"]["next"], json!("1001x15~6"));
    assert_eq!(page["seen_lately"], json!([]));

    // Nothing looked at: the newest, their next a plain cursor that /listings goes on from.
    let page = home("", None).await;
    assert_eq!(page["for_you"], json!(false));
    let newest = &page["recommended"];
    let after = newest["next"].as_str().expect("more than a page").to_string();
    assert!(!after.contains('~'), "{after}");
    let (rest, _) = pages_from(&pool, "", Some(after), None).await;
    let (all, _) = every_page(&pool, "", None).await;
    assert_eq!([numbers(&newest["items"]), rest].concat(), all);

    // The main categories, as the welcome page lists them, each with its English name.
    let welcome = send(api(pool.clone()), Request::get("/").body(Body::empty()).unwrap()).await.body;
    let tiles: Vec<&str> =
        welcome.split(r#"<a class="shortcut" href="/c/"#).skip(1).map(|rest| &rest[..rest.find('"').unwrap()]).collect();
    let slugs: Vec<&str> = page["categories"].as_array().unwrap().iter().map(|category| category["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, tiles);
    assert_eq!(page["categories"][0], json!({"slug": "automjete", "name": "Automjete", "name_en": "Vehicles"}));

    // The most seen this week, eight at most, the most first, each with its views.
    sqlx::query(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views)
         SELECT id, current_date, 100 - public_no % 100 FROM listings WHERE public_no BETWEEN 1001 AND 1010",
    )
    .execute(&pool)
    .await
    .unwrap();
    let page = home("", None).await;
    assert_eq!(numbers(&page["most_viewed"]), (1001..=1008).collect::<Vec<_>>());
    assert_eq!((&page["most_viewed"][0]["views"], &page["most_viewed"][7]["views"]), (&json!(99), &json!(92)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_category_that_doesnt_exist_is_not_found_and_a_cursor_that_doesnt_read_is_refused(pool: PgPool) {
    let missing = get(pool.clone(), "/api/v1/listings?category=nuk-ekziston", None).await;
    assert_eq!(
        (missing.status, missing.json()),
        (StatusCode::NOT_FOUND, json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}}))
    );
    for (query, field) in [
        ("after=garbage", "after"),
        ("category=makina&after=12_not-a-uuid", "after"),
        ("after=1001x10~garbage", "after"),
        ("after=x~6", "after"),
        ("sipas=1001", "sipas"),
    ] {
        let refused = get(pool.clone(), &format!("/api/v1/listings?{query}"), None).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{query}");
        let error = &refused.json()["error"];
        assert_eq!((&error["code"], error["fields"].as_object().map(|fields| fields.len())), (&json!("validation"), Some(1)), "{query}");
        assert!(error["fields"][field].is_string(), "{query}: {error}");
    }
    // In the language asked for.
    let english = get(pool, "/api/v1/listings?after=garbage&lang=en", None).await.json();
    assert_eq!(english["error"]["fields"]["after"], json!("Where the list goes on from can’t be read. Start again from the first page."));
}
