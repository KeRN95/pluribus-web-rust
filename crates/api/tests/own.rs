//! A seller's own listings, through the API (crates/api/src/own.rs): photos uploaded first and a
//! car posted with them, as the pages' form posts one, and listed as "Njoftimet e mia" lists it; an
//! upload a listing can't take, which leaves nothing of it; each field checked as the form checks
//! it, in either language; the day's limits; a photo another seller has, which waits for a
//! moderator; an edit that takes photos out and puts them in order, their files gone; the seller's
//! buttons, each by where the listing stands; the zone a listing's words name and what cars of a
//! model ask; and uploads no listing took, forgotten.

mod common;

use std::time::Duration;

use axum::{
    Router,
    body::Body,
    http::{StatusCode, header},
};
use pluribus_services::{Moderators, media::Media, photos::UPLOADS_KEPT};
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use common::*;

/// The app as the binary serves it, its photos kept in `media`, which a test looks into; with a
/// moderator where `moderated` (MODERATORS in the settings), for whom a duplicate photo waits.
fn app(pool: &PgPool, media: &Media, moderated: bool) -> Router {
    at_the_root();
    let moderators = if moderated { Moderators::new(["+355690000099"]) } else { Moderators::default() };
    let state = pluribus_web::state_with(pool.clone(), media.clone(), moderators, None);
    pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state))
}

/// `method` at `path`, signed in with `token` if one is given, with `body` as JSON if one is given.
async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> ApiResponse {
    let mut request = request(method, path, token);
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    send(app.clone(), request.body(body).unwrap()).await
}

/// A plain JPEG of that size, as a camera might send, its pixels all `shade`: photos of one size
/// and shade are the very same file.
fn jpeg(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&image::RgbImage::from_pixel(width, height, image::Rgb([shade, shade, shade])))
        .unwrap();
    out
}

/// Photos as the app sends them: multipart, each a file of the field `photos`; its content type
/// and its body.
fn multipart(photos: &[Vec<u8>]) -> (String, Vec<u8>) {
    let boundary = "----pluribus-api-test-boundary";
    let mut body = Vec::new();
    for (i, photo) in photos.iter().enumerate() {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"photos\"; filename=\"photo{i}.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(photo);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// `POST /api/v1/photos` with these photos, signed in with `token`.
async fn upload(app: &Router, token: Option<&str>, photos: &[Vec<u8>]) -> ApiResponse {
    let (content_type, body) = multipart(photos);
    let request = request("POST", "/api/v1/photos", token).header(header::CONTENT_TYPE, content_type);
    send(app.clone(), request.body(Body::from(body)).unwrap()).await
}

/// The uploads' ids, the photos uploaded and answered 201.
async fn uploaded(app: &Router, token: &str, photos: &[Vec<u8>]) -> Vec<String> {
    let answer = upload(app, Some(token), photos).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    answer.json().as_array().unwrap().iter().map(|upload| upload["id"].as_str().unwrap().to_string()).collect()
}

/// A new seller, signed in from the app: their id and their token.
async fn seller(pool: &PgPool, phone: &str, name: &str) -> (String, String) {
    let id = new_user(pool, phone, name).await;
    let token = token_for(pool, &id, "ios").await;
    (id, token)
}

/// A Golf for sale, as the app sends it, with these photos.
fn golf(photo_ids: &[String], cover: Option<&str>) -> Value {
    json!({
        "category": "makina",
        "listing_type": "SELL",
        "condition": "USED",
        "title": "Volkswagen Golf 7 1.6 TDI, 2017",
        "description": "Makinë e mbajtur mirë, me servis të rregullt dhe pa aksidente.",
        "price_type": "FIXED",
        "price": 11500,
        "currency": "EUR",
        "city": "tirane",
        "properties": {
            "vehicle_make": "VOLKSWAGEN",
            "vehicle_model": "VOLKSWAGEN_GOLF",
            "vehicle_year": 2017,
            "vehicle_mileage": "120 000",
            "vehicle_fuel": "DIESEL",
            "vehicle_transmission": "MANUAL",
            "vehicle_features": ["ABS", "NAVIGATION"]
        },
        "photo_ids": photo_ids,
        "cover": cover
    })
}

/// A phone for sale, titled `title`, without photos.
fn phone(title: &str) -> Value {
    json!({
        "category": "telefona",
        "listing_type": "SELL",
        "condition": "USED",
        "title": title,
        "description": "Telefon në gjendje shumë të mirë, me kuti dhe karikues.",
        "price_type": "FIXED",
        "price": 32000,
        "currency": "ALL",
        "city": "tirane"
    })
}

/// A listing posted, answered 201: its JSON.
async fn posted(app: &Router, token: &str, body: Value) -> Value {
    let answer = call(app, "POST", "/api/v1/listings", Some(token), Some(body)).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    answer.json()
}

/// A count of rows.
async fn count(pool: &PgPool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

/// The ids of a listing's photos, in their order.
fn photo_ids(listing: &Value) -> Vec<String> {
    listing["photos"].as_array().unwrap().iter().map(|photo| photo["id"].as_str().unwrap().to_string()).collect()
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn photos_are_uploaded_then_a_car_is_posted_with_them_and_listed_as_its_sellers(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;

    // Two photos: each made into its copies at once, as the pages' are, and kept for a listing.
    let answer = upload(&app, Some(&token), &[jpeg(1600, 1200, 10), jpeg(800, 600, 20)]).await;
    assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    let uploads = answer.json();
    let (big, small) = (&uploads[0], &uploads[1]);
    assert_eq!((&big["width"], &big["height"], &small["width"], &small["height"]), (&json!(1600), &json!(1200), &json!(800), &json!(600)));
    let copies = &big["copies"];
    let name = copies["full"].as_str().unwrap().strip_suffix(".webp").unwrap().to_string();
    assert!(name.starts_with("http://127.0.0.1:3000/media/photos/"), "{name}");
    assert_eq!(copies["card"], json!(format!("{name}-card.webp?2")));
    assert_eq!(copies["small"], json!(format!("{name}-small.webp?2")));
    assert_eq!(copies["mini_avif"], json!(format!("{name}-mini.avif?2")));
    assert_eq!((&copies["middle"], &copies["jpeg"]), (&json!(format!("{name}-middle.webp")), &json!(format!("{name}.jpg"))));
    assert_eq!(small["copies"]["middle"], Value::Null, "no bigger than the middle copy");
    assert!(big["placeholder"].as_str().unwrap().starts_with("data:image/webp;base64,"));
    let key = name.trim_start_matches("http://127.0.0.1:3000/media/").to_string();
    assert!(media.get(&format!("{key}-card.avif")).await.unwrap().is_some());
    assert_eq!(count(&pool, "SELECT count(*) FROM photo_uploads").await, 2);

    // A car with them, the second its cover, its properties as the form's.
    let ids: Vec<String> = [big, small].iter().map(|upload| upload["id"].as_str().unwrap().to_string()).collect();
    let car = posted(&app, &token, golf(&ids, Some(&ids[1]))).await;
    assert_eq!((&car["status"], &car["own"], &car["heading"]), (&json!("ACTIVE"), &json!(true), &json!("Volkswagen Golf 7 1.6 TDI, 2017")));
    assert_eq!(car["actions"], json!(["edit", "pause", "sell", "reserve", "delete"]));
    assert_eq!((&car["price"]["amount"], &car["price"]["currency"], &car["place"]["city"]), (&json!("11500"), &json!("EUR"), &json!("Tiranë")));
    // Its photos, the cover first, each keeping its upload's id, its copies its upload's.
    assert_eq!(photo_ids(&car), [ids[1].clone(), ids[0].clone()]);
    assert_eq!((&car["photos"][0]["cover"], &car["photos"][1]["copies"]), (&json!(true), copies));
    let attribute = |code: &str| car["attributes"].as_array().unwrap().iter().find(|attribute| attribute["code"] == code).cloned().unwrap_or_default();
    assert_eq!((&attribute("vehicle_year")["number"], &attribute("vehicle_mileage")["number"]), (&json!("2017"), &json!("120000")));
    let features = attribute("vehicle_features");
    let codes: Vec<&str> = features["options"].as_array().unwrap().iter().map(|option| option["code"].as_str().unwrap()).collect();
    assert_eq!(codes, ["ABS", "NAVIGATION"]);
    // The uploads wait no more: they are its photos.
    assert_eq!(count(&pool, "SELECT count(*) FROM photo_uploads").await, 0);
    assert_eq!(count(&pool, "SELECT count(*) FROM listing_media").await, 2);

    // "Njoftimet e mia": the car, where it stands, its cover, its price, what it drew, its buttons.
    let mine = call(&app, "GET", "/api/v1/me/listings", Some(&token), None).await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.body);
    let mine = mine.json();
    assert_eq!(mine.as_array().unwrap().len(), 1);
    let row = &mine[0];
    assert_eq!((&row["no"], &row["status"], &row["status_label"], &row["status_note"]), (&car["no"], &json!("ACTIVE"), &Value::Null, &Value::Null));
    assert_eq!((&row["actions"], &row["views"], &row["promotions"]), (&car["actions"], &json!(0), &json!([])));
    assert_eq!(row["photo"], car["photos"][0]["copies"]["card"]);
    assert_eq!((&row["price"]["amount"], &row["price"]["type"]), (&json!("11500"), &json!("FIXED")));
    assert!(row["published_at"].is_string() && row["expires_at"].is_string());
    // None of it for anyone else, nor without a token.
    assert_eq!(call(&app, "GET", "/api/v1/me/listings", None, None).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(call(&app, "POST", "/api/v1/listings", None, Some(golf(&ids, None))).await.status, StatusCode::UNAUTHORIZED);
    assert_eq!(upload(&app, None, &[jpeg(40, 30, 0)]).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_photo_the_listing_cant_take_leaves_nothing_of_it(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (ana, token) = seller(&pool, "+355691112233", "Ana").await;
    let (_, besa) = seller(&pool, "+355691112244", "Besa").await;
    let mine = uploaded(&app, &token, &[jpeg(800, 600, 10)]).await;
    let hers = uploaded(&app, &besa, &[jpeg(800, 600, 20)]).await;
    // An upload that isn't there, or another's: no listing, no photo, the uploads waiting as they were.
    for other in ["0199a1b2-c3d4-7e5f-8a6b-7c8d9e0f1a2b", hers[0].as_str()] {
        let answer = call(&app, "POST", "/api/v1/listings", Some(&token), Some(golf(&[mine[0].clone(), other.to_string()], None))).await;
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.body);
        assert_eq!(answer.json()["error"]["fields"]["photo_ids"], json!("Disa nga fotot nuk janë më: ngarkoji përsëri."));
        let listings: i64 = sqlx::query_scalar("SELECT count(*) FROM listings WHERE created_by_user_id = $1::uuid").bind(&ana).fetch_one(&pool).await.unwrap();
        assert_eq!(listings, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM listing_media").await, 0);
        assert_eq!(count(&pool, "SELECT count(*) FROM photo_uploads").await, 2);
    }
    // Its own alone, named twice: taken once.
    let car = posted(&app, &token, golf(&[mine[0].clone(), mine[0].clone()], None)).await;
    assert_eq!(photo_ids(&car), [mine[0].clone()]);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn each_field_is_checked_as_the_form_checks_it(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let refused = |body: Value, lang: &'static str| {
        let app = app.clone();
        let token = token.clone();
        async move {
            let answer = call(&app, "POST", &format!("/api/v1/listings?lang={lang}"), Some(&token), Some(body)).await;
            assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.body);
            let error = answer.json()["error"].clone();
            assert_eq!(error["code"], json!("validation"));
            error["fields"].clone()
        }
    };
    let with = |changes: Value| {
        let mut body = golf(&[], None);
        for (field, value) in changes.as_object().unwrap() {
            body[field] = value.clone();
        }
        body
    };

    // A title too short, no price for a fixed one, a year past next year's and no make: each under
    // its field, a property's under its code, in the form's words.
    let next = OffsetDateTime::now_utc().year() + 1;
    let fields = refused(
        with(json!({"title": "Golf", "price": null, "properties": {"vehicle_model": "VOLKSWAGEN_GOLF", "vehicle_year": 3000}})),
        "sq",
    )
    .await;
    assert_eq!(fields["title"], json!("Titulli duhet të ketë 5 deri në 120 shkronja."));
    assert_eq!(fields["price"], json!("Shkruaj çmimin, ose zgjidh „Falas“ ose „Me marrëveshje“."));
    assert_eq!(fields["properties.vehicle_year"], json!(format!("Duhet të jetë nga 1900 deri në {next}.")));
    assert_eq!(fields["properties.vehicle_make"], json!("Plotëso „Marka“."));
    // In English, as an English page says them.
    let fields = refused(with(json!({"title": "Golf"})), "en").await;
    assert_eq!(fields["title"], json!("The title needs 5 to 120 characters."));

    // A point far from the town, a cover that isn't one of its photos, a store that isn't the
    // seller's.
    let fields = refused(with(json!({"point": {"lat": 40.0, "lng": 20.0}, "cover": "0199a1b2-c3d4-7e5f-8a6b-7c8d9e0f1a2b", "store": "dyqani-i-dikujt"})), "sq").await;
    assert_eq!(fields["point"], json!("Pika në hartë është larg qytetit që zgjodhe: vendose te qyteti, ose hiqe."));
    assert_eq!(fields["cover"], json!("Kopertina duhet të jetë një nga fotot e njoftimit."));
    assert_eq!(fields["store"], json!("Zgjidh një nga dyqanet e tua, ose veten."));

    // A flat in a zone of another town.
    let flat = json!({
        "category": "apartamente", "listing_type": "RENT", "title": "Apartament 2+1 me qira",
        "description": "Apartament i mobiluar, afër qendrës, me ballkon.", "price": 500, "currency": "EUR",
        "city": "durres", "zone": "blloku"
    });
    let fields = refused(flat, "sq").await;
    assert_eq!(fields["zone"], json!("Zgjidh një zonë të qytetit."));
    // Nothing kept of any of them.
    assert_eq!(count(&pool, "SELECT count(*) FROM listings").await, 0);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_seller_posts_a_few_listings_a_day_each_title_once(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    posted(&app, &token, phone("Njoftim prove numër 1")).await;
    // The same title again: under the title, in the pages' words.
    let again = call(&app, "POST", "/api/v1/listings", Some(&token), Some(phone("Njoftim prove numër 1"))).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(again.json()["error"]["fields"]["title"], json!("Ke publikuar tashmë një njoftim me këtë titull sot."));
    for n in 2..=5 {
        posted(&app, &token, phone(&format!("Njoftim prove numër {n}"))).await;
    }
    // The sixth: too many requests, as the page says it.
    let sixth = call(&app, "POST", "/api/v1/listings", Some(&token), Some(phone("Njoftimi i gjashtë sot"))).await;
    assert_eq!(sixth.status, StatusCode::TOO_MANY_REQUESTS, "{}", sixth.body);
    assert_eq!(sixth.json()["error"]["message"], json!("Ke publikuar 5 njoftime sot. Mund të publikosh të tjera nesër."));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_photo_another_seller_has_waits_for_a_moderator(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, true);
    let (_, ana) = seller(&pool, "+355691112233", "Ana").await;
    let (_, besa) = seller(&pool, "+355691112244", "Besa").await;
    // Ana's car goes up; Besa's, with the very same photo, waits for a moderator, as on the pages.
    let first = uploaded(&app, &ana, &[jpeg(800, 600, 10)]).await;
    assert_eq!(posted(&app, &ana, golf(&first, None)).await["status"], json!("ACTIVE"));
    let copied = uploaded(&app, &besa, &[jpeg(800, 600, 10)]).await;
    let waiting = posted(&app, &besa, golf(&copied, None)).await;
    assert_eq!(waiting["status"], json!("PENDING_REVIEW"));
    assert_eq!(
        waiting["status_note"],
        json!("Njoftimi yt pret miratimin e një moderatori: askush tjetër nuk e sheh ende. Zakonisht brenda pak orësh.")
    );
    let reason: String = sqlx::query_scalar("SELECT review_reason FROM listings WHERE public_no = $1").bind(waiting["no"].as_i64()).fetch_one(&pool).await.unwrap();
    assert_eq!(reason, "DUPLICATE_PHOTOS");
    // On "Njoftimet e mia", waiting, with the button that deletes it alone.
    let mine = call(&app, "GET", "/api/v1/me/listings", Some(&besa), None).await.json();
    assert_eq!((&mine[0]["status_label"], &mine[0]["actions"]), (&json!("Në pritje"), &json!(["delete"])));
    assert_eq!(mine[0]["status_note"], waiting["status_note"]);
    // Ana's own photo again, on a listing of hers, holds up nothing.
    let again = uploaded(&app, &ana, &[jpeg(800, 600, 10)]).await;
    let mut second = golf(&again, None);
    second["title"] = json!("Volkswagen Golf 7, e dyta");
    assert_eq!(posted(&app, &ana, second).await["status"], json!("ACTIVE"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn an_edit_takes_photos_out_adds_others_and_puts_them_in_order(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let (_, besa) = seller(&pool, "+355691112244", "Besa").await;
    let ids = uploaded(&app, &token, &[jpeg(800, 600, 10), jpeg(800, 600, 20)]).await;
    let car = posted(&app, &token, golf(&ids, None)).await;
    let no = car["no"].as_i64().unwrap();
    let first_key: String = sqlx::query_scalar("SELECT storage_key FROM listing_media WHERE id = $1::uuid").bind(&ids[0]).fetch_one(&pool).await.unwrap();

    // The first out, a new one in, first; the title changed.
    let new = uploaded(&app, &token, &[jpeg(800, 600, 30)]).await;
    let mut edit = golf(&new, None);
    edit["title"] = json!("Volkswagen Golf 7 1.6 TDI, 2017, i rregulluar");
    edit["removed"] = json!([ids[0]]);
    edit["order"] = json!([new[0], ids[1]]);
    // Not another's to edit, and not with an upload that isn't there: the listing as it was.
    let path = format!("/api/v1/listings/{no}");
    assert_eq!(call(&app, "PUT", &path, Some(&besa), Some(edit.clone())).await.status, StatusCode::NOT_FOUND);
    let mut missing = edit.clone();
    missing["photo_ids"] = json!(["0199a1b2-c3d4-7e5f-8a6b-7c8d9e0f1a2b"]);
    let refused = call(&app, "PUT", &path, Some(&token), Some(missing)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", refused.body);
    assert_eq!(refused.json()["error"]["fields"]["photo_ids"], json!("Disa nga fotot nuk janë më: ngarkoji përsëri."));
    let unchanged = call(&app, "GET", &path, None, None).await.json();
    assert_eq!((&unchanged["heading"], photo_ids(&unchanged)), (&car["heading"], ids.clone()));

    let answer = call(&app, "PUT", &path, Some(&token), Some(edit)).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    let edited = answer.json();
    assert_eq!(edited["heading"], json!("Volkswagen Golf 7 1.6 TDI, 2017, i rregulluar"));
    assert_eq!(photo_ids(&edited), [new[0].clone(), ids[1].clone()]);
    assert_eq!(edited["photos"][0]["cover"], json!(true));
    // The removed one's files are gone, every copy; the others' stay.
    let name = first_key.trim_end_matches(".webp");
    for copy in [".webp", "-middle.webp", "-card.webp", "-small.webp", "-mini.webp", "-card.avif", ".jpg"] {
        assert!(media.get(&format!("{name}{copy}")).await.unwrap().is_none(), "{copy}");
    }
    let kept: String = sqlx::query_scalar("SELECT storage_key FROM listing_media WHERE id = $1::uuid").bind(&ids[1]).fetch_one(&pool).await.unwrap();
    assert!(media.get(&kept).await.unwrap().is_some());
    // Not a listing no one has, nor one that isn't a number.
    assert_eq!(call(&app, "PUT", "/api/v1/listings/999999", Some(&token), Some(golf(&[], None))).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "PUT", "/api/v1/listings/golf", Some(&token), Some(golf(&[], None))).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_seller_pauses_resumes_reserves_sells_renews_and_deletes(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let (_, besa) = seller(&pool, "+355691112244", "Besa").await;
    let no = posted(&app, &token, phone("Samsung Galaxy S21, 128 GB")).await["no"].as_i64().unwrap();
    let act = |action: &'static str, token: String| {
        let app = app.clone();
        async move {
            let (method, path) = match action {
                "delete" => ("DELETE", format!("/api/v1/listings/{no}")),
                action => ("POST", format!("/api/v1/listings/{no}/{action}")),
            };
            call(&app, method, &path, Some(&token), None).await.status
        }
    };
    let status = || {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool).await.unwrap() }
    };
    // Each by where it stands, as the pages' buttons: what doesn't apply is not found.
    for (action, done, then) in [
        ("pause", StatusCode::NO_CONTENT, "PAUSED"),
        ("pause", StatusCode::NOT_FOUND, "PAUSED"),
        ("resume", StatusCode::NO_CONTENT, "ACTIVE"),
        ("unreserve", StatusCode::NOT_FOUND, "ACTIVE"),
        ("reserve", StatusCode::NO_CONTENT, "RESERVED"),
        ("unreserve", StatusCode::NO_CONTENT, "ACTIVE"),
        ("sell", StatusCode::NO_CONTENT, "SOLD"),
        ("renew", StatusCode::NOT_FOUND, "SOLD"),
        ("resume", StatusCode::NO_CONTENT, "ACTIVE"),
    ] {
        assert_eq!(act(action, token.clone()).await, done, "{action}");
        assert_eq!(status().await, then, "{action}");
    }
    // Another's buttons are none of these.
    for action in ["pause", "sell", "renew", "delete"] {
        assert_eq!(act(action, besa.clone()).await, StatusCode::NOT_FOUND, "{action}");
    }
    assert_eq!(status().await, "ACTIVE");

    // Expiring within the week: "renew" among its buttons, and 60 days more from now.
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = $1").bind(no).execute(&pool).await.unwrap();
    let mine = call(&app, "GET", "/api/v1/me/listings", Some(&token), None).await.json();
    assert_eq!(mine[0]["actions"], json!(["edit", "pause", "sell", "reserve", "renew", "delete"]));
    assert_eq!(act("renew", token.clone()).await, StatusCode::NO_CONTENT);
    let expires = call(&app, "GET", "/api/v1/me/listings", Some(&token), None).await.json()[0]["expires_at"].as_str().unwrap().to_string();
    let expires = OffsetDateTime::parse(&expires, &Rfc3339).unwrap();
    assert!(expires - OffsetDateTime::now_utc() > time::Duration::days(59), "{expires}");
    // Expired: back up.
    sqlx::query("UPDATE listings SET status = 'EXPIRED', expires_at = now() - interval '1 day' WHERE public_no = $1").bind(no).execute(&pool).await.unwrap();
    let mine = call(&app, "GET", "/api/v1/me/listings", Some(&token), None).await.json();
    assert_eq!((&mine[0]["status_label"], &mine[0]["actions"]), (&json!("Skaduar"), &json!(["renew", "delete"])));
    assert_eq!(act("renew", token.clone()).await, StatusCode::NO_CONTENT);
    assert_eq!(status().await, "ACTIVE");

    // Deleted: gone from "Njoftimet e mia" too, and nothing more to do to it.
    assert_eq!(act("delete", token.clone()).await, StatusCode::NO_CONTENT);
    assert_eq!(status().await, "DELETED");
    assert_eq!(call(&app, "GET", "/api/v1/me/listings", Some(&token), None).await.json(), json!([]));
    assert_eq!(act("delete", token.clone()).await, StatusCode::NOT_FOUND);
    // A number no listing has, or one that isn't a number; no token.
    assert_eq!(call(&app, "POST", "/api/v1/listings/999999/pause", Some(&token), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "POST", "/api/v1/listings/golf/pause", Some(&token), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "POST", &format!("/api/v1/listings/{no}/pause"), None, None).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn uploads_are_checked_as_the_forms_photos_are(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let refused = |answer: ApiResponse, field: &str| {
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", answer.body);
        answer.json()["error"]["fields"][field].as_str().unwrap().to_string()
    };
    // What isn't a photo; more than a listing takes; none at all; and what isn't multipart.
    let message = refused(upload(&app, Some(&token), &[b"not a photo".to_vec()]).await, "photos");
    assert_eq!(message, "Një nga fotot nuk u lexua. Zgjidh foto JPG, PNG ose WebP.");
    let nine: Vec<Vec<u8>> = (0..9).map(|shade| jpeg(40, 30, shade)).collect();
    assert_eq!(refused(upload(&app, Some(&token), &nine).await, "photos"), "Një njoftim mund të ketë deri në 8 foto.");
    assert_eq!(refused(upload(&app, Some(&token), &[]).await, "photos"), "Dërgo të paktën një foto, në fushën „photos“.");
    let json = call(&app, "POST", "/api/v1/photos", Some(&token), Some(json!({"photos": []}))).await;
    assert_eq!(refused(json, "body"), "Dërgo fotot si multipart/form-data, në fushën „photos“.");
    // The route takes the posting form's 64 MB, not axum's 2: a photo over 15 MB is said to be one,
    // and a body over 64 MB too big to read.
    let big = upload(&app, Some(&token), &[vec![0; 16 * 1024 * 1024]]).await;
    assert_eq!(refused(big, "photos"), "Secila foto mund të jetë deri në 15 MB.");
    let huge = upload(&app, Some(&token), &[vec![0; 65 * 1024 * 1024]]).await;
    assert_eq!(refused(huge, "photos"), "Fotot janë shumë të mëdha. Zgjidh më pak, ose më të vogla.");
    assert_eq!(count(&pool, "SELECT count(*) FROM photo_uploads").await, 0);
    // More than four listings' photos waiting: too many.
    sqlx::query(
        "INSERT INTO photo_uploads (user_id, storage_key)
         SELECT s.user_id, 'photos/waiting-' || n FROM sessions s, generate_series(1, 32) n",
    )
    .execute(&pool)
    .await
    .unwrap();
    let full = upload(&app, Some(&token), &[jpeg(40, 30, 0)]).await;
    assert_eq!(full.status, StatusCode::TOO_MANY_REQUESTS, "{}", full.body);
}

/// Golfs up for sale, as the fixtures' 1001 is but for their years and their prices in euros
/// (crates/web/tests/listings.rs's own).
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn the_zone_a_listings_words_name_and_what_cars_of_a_model_ask(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let zone = |words: Value| {
        let (app, token) = (app.clone(), token.clone());
        async move { call(&app, "POST", "/api/v1/listings/zone", Some(&token), Some(words)).await.json() }
    };
    // As the pages' form guesses it: in the town chosen, or in one the words name too.
    assert_eq!(zone(json!({"title": "Apartament 2+1 në Bllok", "city": "tirane"})).await, json!({"city": "tirane", "zone": "blloku"}));
    assert_eq!(zone(json!({"title": "Apartament", "description": "Shumë afër, Blloku, Tiranë."})).await, json!({"city": "tirane", "zone": "blloku"}));
    assert_eq!(zone(json!({"title": "Apartament pranë plazhit"})).await, json!({"city": null, "zone": null}));

    // Too few Golfs to say; then ten, whose median is 10 250 €, the middle half's 9 500 to 11 375.
    let hint = |query: &'static str| {
        let (app, token) = (app.clone(), token.clone());
        async move { call(&app, "GET", &format!("/api/v1/listings/price-hint?{query}"), Some(&token), None).await }
    };
    pluribus_db::refresh_car_prices(&pool).await.unwrap();
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.status, StatusCode::NO_CONTENT);
    golfs(&pool, &[(2014, 8000), (2015, 9000), (2015, 9500), (2016, 10000), (2016, 10500), (2017, 11000), (2017, 11500), (2018, 12000), (2018, 12500)]).await;
    pluribus_db::refresh_car_prices(&pool).await.unwrap();
    let answer = hint("category=makina&make=VOLKSWAGEN&model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=8000").await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(
        answer.json(),
        json!({
            "text": "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9\u{a0}500–11\u{a0}400\u{a0}€, mesatarja 10\u{a0}300\u{a0}€.",
            "verdict": "good",
            "said": "Te faqja e makinës, çmimi yt del „Çmim i mirë“."
        })
    );
    let far_under = hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=900").await.json();
    assert_eq!((&far_under["verdict"], &far_under["said"]), (&Value::Null, &json!("Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?")));
    let no_price = hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.json();
    assert_eq!((&no_price["verdict"], &no_price["said"]), (&Value::Null, &Value::Null));
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=dikur&currency=EUR").await.status, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "GET", "/api/v1/listings/price-hint?model=VOLKSWAGEN_GOLF&year=2016", None, None).await.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn uploads_no_listing_took_are_forgotten_a_day_on(pool: PgPool) {
    let media = Media::in_memory();
    let app = app(&pool, &media, false);
    let (_, token) = seller(&pool, "+355691112233", "Ana").await;
    let old = uploaded(&app, &token, &[jpeg(800, 600, 10)]).await;
    let fresh = uploaded(&app, &token, &[jpeg(800, 600, 20)]).await;
    sqlx::query("UPDATE photo_uploads SET created_at = now() - interval '25 hours' WHERE id = $1::uuid").bind(&old[0]).execute(&pool).await.unwrap();
    let key: String = sqlx::query_scalar("SELECT storage_key FROM photo_uploads WHERE id = $1::uuid").bind(&old[0]).fetch_one(&pool).await.unwrap();
    // The old one's key, for its files to go; the fresh one waits on.
    let forgotten = pluribus_db::forget_uploads(&pool, UPLOADS_KEPT).await.unwrap();
    assert_eq!(forgotten, std::slice::from_ref(&key));
    let left: Vec<String> = sqlx::query_scalar("SELECT id::text FROM photo_uploads").fetch_all(&pool).await.unwrap();
    assert_eq!(left, fresh);
    // Its files go as the expiry deletes them, every copy.
    assert!(media.get(&key).await.unwrap().is_some());
    pluribus_services::media::delete_photos(&media, &forgotten).await;
    assert!(media.get(&key).await.unwrap().is_none());
    assert!(media.get(&key.replace(".webp", "-card.avif")).await.unwrap().is_none());
    assert!(pluribus_db::forget_uploads(&pool, Duration::from_secs(24 * 60 * 60)).await.unwrap().is_empty());
}
