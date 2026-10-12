//! One listing through the API (crates/api/src/listing.rs), on the pages' sample listings
//! (crates/web/tests/fixtures/listings.sql): a car's, a property's and a plain thing's, each as its
//! page shows it, as data; one off the site, for its seller and the moderators alone; the listings
//! like it, a page at a time; its page seen, for the view counts; its seller's contact revealed and
//! counted; a number no listing has; and the words it says, in the language asked for.

mod common;

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
};
use pluribus_services::{Moderators, config::Config, media::Media};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use common::*;

/// The fixtures' sellers: Ana, private; Besnik, private; Dritan, who posts for Auto Durrësi.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";
const DRITAN: &str = "33333333-3333-4333-8333-333333333333";

/// The tests' moderator, MODERATORS in the settings, as the pages' tests have theirs: whoever signs
/// in with this number.
const MODERATOR: &str = "+355690000099";

/// The app as `common::api` makes it, with MODERATOR among the settings' moderators.
fn moderated(pool: PgPool) -> Router {
    at_the_root();
    let state = pluribus_web::state_with(pool, Media::in_memory(), Moderators::new([MODERATOR]), None);
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// `/api/v1/listings/{no}` as `token` sees it, answered 200: its JSON.
async fn listing(pool: &PgPool, no: i64, token: Option<&str>) -> Value {
    let answer = get(pool.clone(), &format!("/api/v1/listings/{no}"), token).await;
    assert_eq!(answer.status, StatusCode::OK, "{no}: {}", answer.body);
    answer.json()
}

/// A moment the database keeps, as the API writes it (RFC 3339), or null.
async fn moment(pool: &PgPool, sql: &str) -> Value {
    let at: Option<OffsetDateTime> = sqlx::query_scalar(sql).fetch_one(pool).await.unwrap();
    at.map_or(Value::Null, |at| json!(at.format(&Rfc3339).unwrap()))
}

/// The property with `code` among a listing's, without its `key`, which its category says.
fn attribute(listing: &Value, code: &str) -> Value {
    let attributes = listing["attributes"].as_array().unwrap_or_else(|| panic!("no attributes: {listing}"));
    let mut found = attributes.iter().find(|attribute| attribute["code"] == code).cloned().unwrap_or_else(|| panic!("no {code}"));
    found.as_object_mut().unwrap().remove("key");
    found
}

/// The numbers of a list's cards, in its order.
fn numbers(list: &Value) -> Vec<i64> {
    list.as_array().unwrap_or_else(|| panic!("not a list: {list}")).iter().map(|card| card["no"].as_i64().unwrap()).collect()
}

/// Golfs up for sale, as 1001 is but for their years and their prices in euros (Ana's, as the
/// pages' listings tests make them).
async fn golfs(pool: &PgPool, cars: &[(i32, i64)]) {
    for (year, price) in cars {
        sqlx::query(
            "WITH l AS (
                 INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                                       listing_type, condition, price_amount, currency, price_type, price_normalized, status, published_at)
                 SELECT seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                        listing_type, condition, $2::numeric, 'EUR', 'FIXED', $2::numeric, 'ACTIVE', now() - interval '3 days'
                 FROM listings WHERE public_no = 1001
                 RETURNING id
             )
             INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text, value_number, value_boolean, value_date, value_option_id)
             SELECT l.id, v.attribute_id, v.data_type, v.value_text,
                    CASE WHEN a.code = 'vehicle_year' THEN $1::numeric ELSE v.value_number END,
                    v.value_boolean, v.value_date, v.value_option_id
             FROM l, listing_attribute_values v JOIN attributes a ON a.id = v.attribute_id
             WHERE v.listing_id = 'a0000000-0000-4000-8000-000000001001'",
        )
        .bind(year)
        .bind(price)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn run(pool: &PgPool, sql: &str) {
    sqlx::query(sql).execute(pool).await.unwrap();
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_cars_listing_carries_what_its_page_shows(pool: PgPool) {
    // Its title made from what the car is, its seller's own heading its description; its photo
    // kept as WebP, with its copies, on the site's own /media; and nine more Golfs, from 2014 to
    // 2018, for what cars of its model and year ask: ten, whose median is 10 250 €.
    run(&pool, "UPDATE listings SET generated_title = 'Volkswagen Golf 1.6, 2016' WHERE public_no = 1001").await;
    run(
        &pool,
        "UPDATE listing_media
         SET public_url = '/media/photos/ab12.webp', thumbnail_url = '/media/photos/ab12-card.webp',
             middle_url = '/media/photos/ab12-middle.webp', jpeg_url = '/media/photos/ab12.jpg',
             placeholder = 'data:image/webp;base64,UklGRg==', mime_type = 'image/webp', width_px = 1600, height_px = 1200
         WHERE listing_id = 'a0000000-0000-4000-8000-000000001001'",
    )
    .await;
    golfs(&pool, &[(2014, 8000), (2015, 9000), (2015, 9500), (2016, 10000), (2016, 10500), (2017, 11000), (2017, 11500), (2018, 12000), (2018, 12500)])
        .await;
    pluribus_db::refresh_car_prices(&pool).await.unwrap();

    let request = request("GET", "/api/v1/listings/1001", None)
        .header(header::HOST, "pluribus.test")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap();
    let answer = send(api(pool.clone()), request).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let mut golf = answer.json();
    // The site's address: PUBLIC_URL when the settings give one, else the one the request came to.
    let site = Config::for_tests().public_url.unwrap_or_else(|| "https://pluribus.test".to_string());

    // Its properties as data, each as its page words it: a number with its unit, a choice with its
    // code and its English as the English pages say it, a multi-select's each.
    assert_eq!(
        attribute(&golf, "vehicle_mileage"),
        json!({"code": "vehicle_mileage", "label": "Kilometrazhi", "label_en": "Mileage", "shown": ["148\u{a0}000\u{a0}km"],
               "multi": false, "number": "148000", "unit": "km", "unit_en": "km"})
    );
    assert_eq!(attribute(&golf, "vehicle_year")["number"], json!("2016"));
    assert_eq!(
        attribute(&golf, "vehicle_fuel"),
        json!({"code": "vehicle_fuel", "label": "Karburanti", "label_en": "Fuel", "shown": ["Naftë"], "multi": false,
               "options": [{"code": "DIESEL", "label": "Naftë", "label_en": "Diesel"}]})
    );
    assert_eq!(
        attribute(&golf, "vehicle_features"),
        json!({"code": "vehicle_features", "label": "Opsionet", "label_en": "Features", "shown": ["ABS", "Kondicioner"], "multi": true,
               "options": [{"code": "ABS", "label": "ABS", "label_en": "ABS"},
                           {"code": "AIR_CONDITIONING", "label": "Kondicioner", "label_en": "Air conditioning"}]})
    );
    golf["attributes"] = json!([]);

    let photo: Uuid = sqlx::query_scalar("SELECT id FROM listing_media WHERE listing_id = 'a0000000-0000-4000-8000-000000001001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let ana_listings: i64 = sqlx::query_scalar("SELECT count(*) FROM listings WHERE seller_user_id = $1::uuid AND status = 'ACTIVE'")
        .bind(ANA)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(ana_listings, 14, "her five, and the nine Golfs");
    let copy = |name: &str| format!("{site}/media/photos/ab12{name}");
    assert_eq!(
        golf,
        json!({
            "no": 1001,
            "slug": "volkswagen-golf-7-2016-nafte",
            "title": "Volkswagen Golf 1.6, 2016",
            "heading": "Volkswagen Golf 7, 2016, naftë",
            "deal": true,
            "kind": "SELL",
            "condition": "USED",
            "status": "ACTIVE",
            "status_note": null,
            "price": {"amount": "9500", "currency": "EUR", "type": "FIXED", "was": null, "per_month": false, "drop": null, "eur_rate": "1"},
            "car_price": {
                "median": "10300",
                "verdict": "near",
                "text": "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9\u{a0}500–11\u{a0}400\u{a0}€, mesatarja 10\u{a0}300\u{a0}€.",
            },
            "area_price": null,
            "loan": {"amount": "9500", "down_percent": 35},
            "description": "Makina është në gjendje shumë të mirë, e mirëmbajtur rregullisht. Kambio manuale, 148 000 km.",
            "attributes": [],
            "photos": [{
                "id": photo,
                "cover": true,
                "copies": {
                    "card": copy("-card.webp?2"),
                    "small": copy("-small.webp?2"),
                    "mini": copy("-mini.webp?2"),
                    "middle": copy("-middle.webp"),
                    "full": copy(".webp"),
                    "jpeg": copy(".jpg"),
                    "card_avif": copy("-card.avif?2"),
                    "small_avif": copy("-small.avif?2"),
                    "mini_avif": copy("-mini.avif?2"),
                },
                "width": 1600,
                "height": 1200,
                "placeholder": "data:image/webp;base64,UklGRg==",
            }],
            "place": {"city": "Tiranë", "zone": null, "point": {"lat": 41.328333, "lng": 19.818056}},
            "seller": {
                "kind": "user",
                "user_id": ANA,
                "slug": null,
                "name": "Ana",
                "logo": null,
                "verified": false,
                "since": moment(&pool, &format!("SELECT created_at FROM users WHERE id = '{ANA}'")).await,
                "listings": 14,
                "answers_within": null,
                "active_today": false,
                "suspended": false,
            },
            "contact": {"kinds": ["phone"], "revealed": false},
            "own": false,
            "actions": [],
            "published_at": moment(&pool, "SELECT published_at FROM listings WHERE public_no = 1001").await,
            "expires_at": moment(&pool, "SELECT expires_at FROM listings WHERE public_no = 1001").await,
            "views": null,
        })
    );

    // The number it shows is one Ana signed in with, and she was here today; a price lowered lately
    // says by how much, and a car's whose price is well under what such cars ask, a good price.
    run(
        &pool,
        &format!(
            "INSERT INTO user_verifications (user_id, verification_type, status, verified_value)
             VALUES ('{ANA}', 'PHONE', 'VERIFIED', '+355690000001')"
        ),
    )
    .await;
    run(&pool, &format!("UPDATE users SET last_seen_at = now() WHERE id = '{ANA}'")).await;
    run(&pool, "UPDATE listings SET price_amount = 8000, price_normalized = 8000 WHERE public_no = 1001").await;
    let golf = listing(&pool, 1001, None).await;
    assert_eq!((&golf["seller"]["verified"], &golf["seller"]["active_today"]), (&json!(true), &json!(true)));
    assert_eq!(golf["price"]["was"], json!("9500"));
    assert_eq!(golf["price"]["drop"], json!({"from": "9500", "to": "8000", "percent": 16}));
    assert_eq!((&golf["car_price"]["median"], &golf["car_price"]["verdict"]), (&json!("10300"), &json!("good")));

    // A store's car: its slug and its name, and which of its people posted it, whom a block from
    // the listing takes.
    let mercedes = listing(&pool, 1002, None).await;
    assert_eq!(
        (&mercedes["seller"]["kind"], &mercedes["seller"]["user_id"], &mercedes["seller"]["slug"], &mercedes["seller"]["name"]),
        (&json!("business"), &json!(DRITAN), &json!("auto-durresi"), &json!("Auto Durrësi"))
    );
    assert_eq!(mercedes["car_price"], Value::Null, "too few of its model");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_propertys_listing_weighs_its_square_metre_against_its_zones(pool: PgPool) {
    // The house in Vlorë: 185 000 € over its 200 m², in no zone; on the map at its point.
    let house = listing(&pool, 1004, None).await;
    let (latitude, longitude): (f64, f64) = sqlx::query_as("SELECT latitude::float8, longitude::float8 FROM listings WHERE public_no = 1004")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((&house["deal"], &house["car_price"]), (&json!(true), &Value::Null));
    assert_eq!(house["area_price"], json!({"per_m2": "925", "zone_per_m2": null, "verdict": null}));
    assert_eq!(house["place"], json!({"city": "Vlorë", "zone": null, "point": {"lat": latitude, "lng": longitude}}));
    assert_eq!(house["loan"], json!({"amount": "185000", "down_percent": 35}));
    // A rent has no price by the square metre, nor a loan; its price is a month's.
    let flat = listing(&pool, 1003, None).await;
    assert_eq!((&flat["area_price"], &flat["loan"], &flat["price"]["per_month"]), (&Value::Null, &Value::Null, &json!(true)));
    assert_eq!(attribute(&flat, "property_area")["shown"], json!(["85\u{a0}m²"]));

    // Six flats on sale in Blloku, 100 m² each, at 100 000 €: the zone's average is 1 000 €/m².
    run(
        &pool,
        "INSERT INTO listings (public_no, seller_user_id, created_by_user_id, category_id, location_id, slug,
                               title, description, listing_type, price_amount, price_normalized, currency,
                               price_type, status, published_at)
         OVERRIDING SYSTEM VALUE
         SELECT 3000 + n, '22222222-2222-4222-8222-222222222222', '22222222-2222-4222-8222-222222222222',
                (SELECT id FROM categories WHERE slug = 'apartamente'),
                (SELECT id FROM locations WHERE slug = 'blloku' AND location_type = 'AREA'),
                'apartament-' || n, 'Apartament ' || n, 'Apartament në Bllok.', 'SELL',
                100000, 100000, 'EUR', 'FIXED', 'ACTIVE', now()
         FROM generate_series(1, 6) n",
    )
    .await;
    run(
        &pool,
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
         SELECT l.id, a.id, a.data_type, 100
         FROM listings l JOIN attributes a ON a.code = 'property_area'
         WHERE l.public_no BETWEEN 3001 AND 3006",
    )
    .await;
    pluribus_db::refresh_zone_prices(&pool).await.unwrap();
    let even = listing(&pool, 3001, None).await;
    assert_eq!(even["area_price"], json!({"per_m2": "1000", "zone_per_m2": "1000", "verdict": "same"}));
    assert_eq!((&even["place"]["city"], &even["place"]["zone"]), (&json!("Tiranë"), &json!("Blloku")));
    // Against the average as it stands: 200 € a square metre under it, below; in lekë, the average
    // in lekë, 10 000 under it near; a starting price says too little to compare.
    run(&pool, "UPDATE listings SET price_amount = 80000 WHERE public_no = 3002").await;
    run(&pool, "UPDATE listings SET price_amount = 9000000, currency = 'ALL' WHERE public_no = 3003").await;
    run(&pool, "UPDATE listings SET price_type = 'FROM' WHERE public_no = 3004").await;
    assert_eq!(listing(&pool, 3002, None).await["area_price"], json!({"per_m2": "800", "zone_per_m2": "1000", "verdict": "below"}));
    assert_eq!(listing(&pool, 3003, None).await["area_price"], json!({"per_m2": "90000", "zone_per_m2": "100000", "verdict": "near"}));
    assert_eq!(listing(&pool, 3004, None).await["area_price"], json!({"per_m2": "1000", "zone_per_m2": null, "verdict": null}));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_plain_things_listing_has_its_loan_but_no_map_nor_comparison(pool: PgPool) {
    let phone = listing(&pool, 1005, None).await;
    assert_eq!((&phone["deal"], &phone["kind"], &phone["condition"]), (&json!(false), &json!("SELL"), &json!("LIKE_NEW")));
    assert_eq!(
        phone["price"],
        json!({"amount": "55000", "currency": "ALL", "type": "FIXED", "was": null, "per_month": false, "drop": null, "eur_rate": "0.01"})
    );
    assert_eq!((&phone["car_price"], &phone["area_price"]), (&Value::Null, &Value::Null));
    assert_eq!(phone["loan"], json!({"amount": "55000", "down_percent": 35}));
    assert_eq!(phone["place"], json!({"city": "Tiranë", "zone": null, "point": null}), "a map for a car or a property alone");
    let brand = attribute(&phone, "electronics_brand");
    assert_eq!((&brand["shown"], &brand["options"]), (&json!(["Apple"]), &json!([{"code": "APPLE", "label": "Apple", "label_en": "Apple"}])));
    // A job's pay on request: no amount, a month's; nothing to borrow for. Something free neither.
    let job = listing(&pool, 1008, None).await;
    assert_eq!((&job["kind"], &job["price"]["type"], &job["price"]["amount"]), (&json!("JOB"), &json!("ON_REQUEST"), &Value::Null));
    assert_eq!((&job["price"]["per_month"], &job["loan"], &job["condition"]), (&json!(true), &Value::Null, &Value::Null));
    let books = listing(&pool, 1011, None).await;
    assert_eq!((&books["price"]["type"], &books["price"]["amount"], &books["loan"]), (&json!("FREE"), &Value::Null, &Value::Null));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_listing_off_the_site_is_its_sellers_and_the_moderators_alone(pool: PgPool) {
    let ana = token_for(&pool, ANA, "android").await;
    let besnik = token_for(&pool, BESNIK, "ios").await;
    let dritan = token_for(&pool, DRITAN, "ios").await;

    // Up, for anyone: its seller sees their buttons, by where it stands; a store's, its people who
    // post for it, and renewing it within a week of its expiry.
    let theirs = listing(&pool, 1001, None).await;
    assert_eq!((&theirs["own"], &theirs["actions"]), (&json!(false), &json!([])));
    let own = listing(&pool, 1001, Some(&ana)).await;
    assert_eq!((&own["own"], &own["actions"]), (&json!(true), &json!(["edit", "pause", "sell", "reserve", "delete"])));
    run(&pool, "UPDATE listings SET status = 'RESERVED', expires_at = now() + interval '3 days' WHERE public_no = 1002").await;
    let store = listing(&pool, 1002, Some(&dritan)).await;
    assert_eq!((&store["own"], &store["actions"]), (&json!(true), &json!(["edit", "pause", "sell", "unreserve", "renew", "delete"])));
    assert_eq!(listing(&pool, 1002, Some(&besnik)).await["own"], json!(false));

    // Paused: not found for anyone else, signed in or not; its seller is told how to put it back.
    run(&pool, "UPDATE listings SET status = 'PAUSED' WHERE public_no = 1005").await;
    for token in [None, Some(besnik.as_str())] {
        let hidden = get(pool.clone(), "/api/v1/listings/1005", token).await;
        assert_eq!(
            (hidden.status, hidden.json()),
            (StatusCode::NOT_FOUND, json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}}))
        );
    }
    let paused = listing(&pool, 1005, Some(&ana)).await;
    assert_eq!((&paused["status"], &paused["own"]), (&json!("PAUSED"), &json!(true)));
    assert_eq!(paused["actions"], json!(["edit", "resume", "delete"]));
    assert_eq!(
        paused["status_note"],
        json!("E ke çaktivizuar këtë njoftim: askush tjetër nuk e sheh. „Aktivizo“ e kthen në faqe aty ku ishte.")
    );
    assert_eq!(paused["contact"], json!({"kinds": [], "revealed": false}), "nothing to reveal off the site");
    // A token no good session has is refused, rather than answered as for nobody.
    assert_eq!(get(pool.clone(), "/api/v1/listings/1001", Some("not-a-session")).await.status, StatusCode::UNAUTHORIZED);

    // Waiting for a moderator: for its seller, who is told so, and for the moderators.
    run(&pool, "UPDATE listings SET status = 'PENDING_REVIEW' WHERE public_no = 1007").await;
    let moderator = new_user(&pool, MODERATOR, "Moderatori").await;
    let moderator = token_for(&pool, &moderator, "android").await;
    let as_moderator = |no: i64| {
        let (pool, moderator) = (pool.clone(), moderator.clone());
        async move {
            let request = request("GET", &format!("/api/v1/listings/{no}"), Some(&moderator)).body(Body::empty()).unwrap();
            send(moderated(pool), request).await
        }
    };
    let waiting = as_moderator(1007).await;
    assert_eq!(waiting.status, StatusCode::OK, "{}", waiting.body);
    let waiting = waiting.json();
    assert_eq!((&waiting["status"], &waiting["own"], &waiting["actions"]), (&json!("PENDING_REVIEW"), &json!(false), &json!([])));
    assert_eq!(waiting["status_note"], json!("Ky njoftim pret miratimin e moderatorëve."));
    assert_eq!(get(pool.clone(), "/api/v1/listings/1007", None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(pool.clone(), "/api/v1/listings/1007", Some(&moderator)).await.status, StatusCode::NOT_FOUND, "no moderator here");
    let posted = listing(&pool, 1007, Some(&ana)).await;
    assert!(posted["status_note"].as_str().unwrap().starts_with("Njoftimi yt pret miratimin e një moderatori"), "{posted}");
    assert_eq!(posted["actions"], json!(["delete"]));

    // Off the site with its seller's account, which a moderator suspended: the moderators are
    // told so.
    run(&pool, &format!("UPDATE users SET status = 'SUSPENDED' WHERE id = '{ANA}'")).await;
    run(&pool, "UPDATE listings SET status = 'SUSPENDED' WHERE public_no = 1010").await;
    let suspended = as_moderator(1010).await.json();
    assert_eq!(suspended["seller"]["suspended"], json!(true));
    assert_eq!(suspended["status_note"], json!("Ky njoftim nuk shfaqet në faqe: llogaria e shitësit është pezulluar."));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn the_listings_like_one_come_a_page_at_a_time(pool: PgPool) {
    // Ten more Golfs, each from a seller of its own, at the Golf's price in Tirana.
    run(
        &pool,
        "WITH sellers AS (
             INSERT INTO users (auth_subject, phone_e164, display_name)
             SELECT 'phone:+3556910000' || lpad(n::text, 2, '0'), '+3556910000' || lpad(n::text, 2, '0'), 'Shitës ' || n
             FROM generate_series(1, 10) n
             RETURNING id
         )
         INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at, latitude, longitude)
         SELECT id, id, (SELECT id FROM categories WHERE slug = 'makina'), 'golf', 'Golf', '', 9500, 9500, 'EUR',
                'ACTIVE', now() - interval '1 hour', 41.3275, 19.8187
         FROM sellers",
    )
    .await;
    let golf = Uuid::parse_str("a0000000-0000-4000-8000-000000001001").unwrap();
    let alike: Vec<i64> =
        pluribus_db::related_listings(&pool, golf, 100, 0).await.unwrap().iter().map(|card| card.public_no).collect();
    assert_eq!(alike.len(), 11, "the ten, and the Mercedes");
    let page = |after: Option<&'static str>, token: Option<String>| {
        let pool = pool.clone();
        async move {
            let path = match after {
                Some(after) => format!("/api/v1/listings/1001/related?after={after}"),
                None => "/api/v1/listings/1001/related".to_string(),
            };
            let answer = get(pool, &path, token.as_deref()).await;
            assert_eq!(answer.status, StatusCode::OK, "{path}: {}", answer.body);
            answer.json()
        }
    };

    // Eight at a time, as the page's frames go on, the most alike first, then where the next
    // starts; the cards as the lists' are.
    let first = page(None, None).await;
    assert_eq!((numbers(&first["items"]), &first["next"]), (alike[..8].to_vec(), &json!("8")));
    assert_eq!(first["items"][0]["kind"], json!("SELL"));
    let rest = page(Some("8"), None).await;
    assert_eq!((numbers(&rest["items"]), &rest["next"]), (alike[8..].to_vec(), &Value::Null));

    // A buyer who blocked one of the sellers doesn't see their Golf; the pages go on as they were.
    let buyer = new_user(&pool, "+355692223344", "Blerina").await;
    let token = token_for(&pool, &buyer, "android").await;
    run(
        &pool,
        &format!(
            "INSERT INTO user_blocks (blocker_user_id, blocked_user_id)
             SELECT '{buyer}', seller_user_id FROM listings WHERE public_no = {}",
            alike[0]
        ),
    )
    .await;
    let blocked = page(None, Some(token.clone())).await;
    assert_eq!((numbers(&blocked["items"]), &blocked["next"]), (alike[1..8].to_vec(), &json!("8")));
    assert_eq!(numbers(&page(Some("8"), Some(token)).await["items"]), alike[8..].to_vec());

    // A place in the list that doesn't read; a listing off the site.
    for after in ["later", "-8"] {
        let refused = get(pool.clone(), &format!("/api/v1/listings/1001/related?after={after}"), None).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{after}");
        assert!(refused.json()["error"]["fields"]["after"].is_string(), "{after}");
    }
    run(&pool, "UPDATE listings SET status = 'PAUSED' WHERE public_no = 1001").await;
    assert_eq!(get(pool.clone(), "/api/v1/listings/1001/related", None).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_listings_page_seen_counts_each_visitor_once_in_a_while(pool: PgPool) {
    // One app, as the server is one: the visitors it counted lately are its own (views.rs).
    let app = api(pool.clone());
    let seen = |no: i64, agent: Option<&'static str>, from: &'static str| {
        let app = app.clone();
        async move {
            let mut request = request("POST", &format!("/api/v1/listings/{no}/seen"), None).header("x-forwarded-for", from);
            if let Some(agent) = agent {
                request = request.header(header::USER_AGENT, agent);
            }
            send(app, request.body(Body::empty()).unwrap()).await.status
        }
    };
    let views = || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT coalesce(sum(detail_views), 0)::bigint FROM listing_daily_stats
                 WHERE listing_id = 'a0000000-0000-4000-8000-000000001001' AND stat_date = current_date",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    // Into the day's statistics at once (the tests' views are written as they come), each visitor
    // once in a while, by their address and their app; neither a bot nor what says nothing of
    // itself.
    assert_eq!(seen(1001, Some("Pluribus/1.0 (Android)"), "10.0.0.1").await, StatusCode::NO_CONTENT);
    assert_eq!(views().await, 1);
    assert_eq!(seen(1001, Some("Pluribus/1.0 (Android)"), "10.0.0.1").await, StatusCode::NO_CONTENT);
    assert_eq!(views().await, 1, "the same visitor again");
    assert_eq!(seen(1001, Some("Pluribus/1.0 (Android)"), "10.0.0.2").await, StatusCode::NO_CONTENT);
    assert_eq!(seen(1001, Some("Googlebot/2.1"), "10.0.0.3").await, StatusCode::NO_CONTENT);
    assert_eq!(seen(1001, None, "10.0.0.4").await, StatusCode::NO_CONTENT);
    assert_eq!(views().await, 2);
    // Its page says so.
    assert_eq!(listing(&pool, 1001, None).await["views"], json!(2));
    // Not for a listing off the site.
    run(&pool, "UPDATE listings SET status = 'PAUSED' WHERE public_no = 1005").await;
    assert_eq!(seen(1005, Some("Pluribus/1.0 (Android)"), "10.0.0.1").await, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn revealing_the_contact_gives_the_number_and_whatsapp_and_counts_it(pool: PgPool) {
    let reveal = |path: &'static str| {
        let pool = pool.clone();
        async move { send(api(pool), request("POST", path, None).body(Body::empty()).unwrap()).await }
    };
    let reveals = || {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT phone_reveals FROM listing_daily_stats WHERE listing_id = 'a0000000-0000-4000-8000-000000001001'",
            )
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    // The number, and the chat WhatsApp opens with it about the listing, its first message typed
    // as the page's button has it: counted, as the page's reveal is.
    let golf = reveal("/api/v1/listings/1001/contact").await;
    assert_eq!(golf.status, StatusCode::OK, "{}", golf.body);
    let golf = golf.json();
    assert_eq!((&golf["phone"], &golf["email"]), (&json!("+355690000001"), &Value::Null));
    let whatsapp = golf["whatsapp_url"].as_str().unwrap();
    assert!(whatsapp.starts_with("https://wa.me/355690000001?text=P%C3%ABrsh%C3%ABndetje%21%20Ju%20shkruaj"), "{whatsapp}");
    assert!(whatsapp.ends_with("%2Flisting%2F1001-volkswagen-golf-7-2016-nafte"), "{whatsapp}");
    assert_eq!(reveals().await, 1);
    // In the language asked for, which the buyer writes in.
    let english = reveal("/api/v1/listings/1001/contact?lang=en").await.json();
    assert!(english["whatsapp_url"].as_str().unwrap().contains("text=Hello%21%20I%E2%80%99m%20writing"), "{english}");
    assert_eq!(reveals().await, 2);

    // An Albanian landline has no WhatsApp; a seller who shows an address alone, the address.
    run(&pool, "UPDATE listing_contacts SET phone_e164 = '+35542234567' WHERE listing_id = 'a0000000-0000-4000-8000-000000001003'").await;
    assert_eq!(
        reveal("/api/v1/listings/1003/contact").await.json(),
        json!({"phone": "+35542234567", "email": null, "whatsapp_url": null})
    );
    run(
        &pool,
        "UPDATE listing_contacts SET show_phone = false, show_email = true, email = 'besnik@example.test'
         WHERE listing_id = 'a0000000-0000-4000-8000-000000001006'",
    )
    .await;
    assert_eq!(
        reveal("/api/v1/listings/1006/contact").await.json(),
        json!({"phone": null, "email": "besnik@example.test", "whatsapp_url": null})
    );
    assert_eq!(listing(&pool, 1006, None).await["contact"], json!({"kinds": ["email"], "revealed": false}));

    // Nothing once it is sold, as on its page.
    run(&pool, "UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1010").await;
    assert_eq!(reveal("/api/v1/listings/1010/contact").await.status, StatusCode::NOT_FOUND);
    assert_eq!(listing(&pool, 1010, None).await["contact"]["kinds"], json!([]));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn a_number_no_listing_has_is_not_found(pool: PgPool) {
    let not_found = json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}});
    for (method, path) in [
        ("GET", "/api/v1/listings/999999"),
        ("GET", "/api/v1/listings/golf"),
        ("GET", "/api/v1/listings/999999/related"),
        ("POST", "/api/v1/listings/999999/seen"),
        ("POST", "/api/v1/listings/999999/contact"),
        ("POST", "/api/v1/listings/golf/contact"),
    ] {
        let request = request(method, path, None).header(header::USER_AGENT, "Pluribus/1.0").body(Body::empty()).unwrap();
        let answer = send(api(pool.clone()), request).await;
        assert_eq!((answer.status, answer.json()), (StatusCode::NOT_FOUND, not_found.clone()), "{method} {path}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn its_words_are_in_the_language_asked_for(pool: PgPool) {
    let in_english = |path: String, token: Option<String>| {
        let pool = pool.clone();
        async move {
            let request = request("GET", &path, token.as_deref()).header(header::ACCEPT_LANGUAGE, "en-GB,en;q=0.9").body(Body::empty()).unwrap();
            let answer = send(api(pool), request).await;
            assert_eq!(answer.status, StatusCode::OK, "{path}: {}", answer.body);
            answer.json()
        }
    };
    // Its properties as an English page says them; their English names whatever the language.
    let golf = in_english("/api/v1/listings/1001".to_string(), None).await;
    assert_eq!(attribute(&golf, "vehicle_fuel")["shown"], json!(["Diesel"]));
    assert_eq!(attribute(&golf, "vehicle_features")["shown"], json!(["ABS", "Air conditioning"]));
    assert_eq!(attribute(&golf, "vehicle_mileage")["shown"], json!(["148\u{a0}000\u{a0}km"]));
    let albanian = listing(&pool, 1001, None).await;
    assert_eq!(attribute(&albanian, "vehicle_fuel")["shown"], json!(["Naftë"]));
    for golf in [&golf, &albanian] {
        let fuel = attribute(golf, "vehicle_fuel");
        assert_eq!((&fuel["label_en"], &fuel["options"][0]["label_en"]), (&json!("Fuel"), &json!("Diesel")));
    }
    // What its page says over it.
    run(&pool, "UPDATE listings SET status = 'PAUSED' WHERE public_no = 1005").await;
    let ana = token_for(&pool, ANA, "ios").await;
    let paused = in_english("/api/v1/listings/1005".to_string(), Some(ana)).await;
    assert_eq!(
        paused["status_note"],
        json!("You’ve paused this listing: nobody else can see it. “Reactivate” puts it back on the site where it was.")
    );
}
