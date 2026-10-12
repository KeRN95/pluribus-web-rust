//! What the request tests share. Each file in tests/ is a crate of its own, and takes this in with
//! `mod common;`: the app as the tests run it, with its photos kept in memory and a moderator of
//! its own; the migrations, embedded once in pluribus-db (`MIGRATOR`); requests sent through the
//! full router, and what they answer; the sessions and the sellers they come from; the listing
//! form's fields; and the readers of a page's cards and frames. Each test gets a fresh database
//! with the migrations applied, plus the fixtures named in its attribute (tests/fixtures). The
//! tests run from the repository's root, as the server does (`at_the_root`).

// Each file uses some of what is here, none all of it: what one leaves unused is no warning there.
#![allow(dead_code)]

use std::sync::{Once, OnceLock};

use axum::{Router, body::Body, http::{Request, StatusCode, header}};
use http_body_util::BodyExt;
use pluribus_services::{Moderators, media::Media};
use sqlx::PgPool;
use tower::ServiceExt;

/// The migrations, embedded once, in pluribus-db, for every test (`migrator = "MIGRATOR"` in each
/// attribute): without it, `#[sqlx::test]` would read and embed all of them again for each test,
/// which took three quarters of a test file's build, and from a migrations/ beside this crate's
/// Cargo.toml, which there isn't.
pub use pluribus_db::MIGRATOR;

/// The tests' moderator (MODERATORS in .env): whoever signs in with this number.
pub const MODERATOR: &str = "+355690000099";

/// The app, with photos kept in memory, one store for all the tests: their photos' names never
/// collide, and a photo posted in one request is there for the next. MODERATOR moderates.
pub fn app(pool: PgPool) -> Router {
    pluribus_web::app_with(pool, media(), Moderators::new([MODERATOR]))
}

/// The tests' photo store, which every app they build is given: the tests run from the
/// repository's root before one is (`at_the_root`).
pub fn media() -> Media {
    at_the_root();
    static MEDIA: OnceLock<Media> = OnceLock::new();
    MEDIA.get_or_init(Media::in_memory).clone()
}

/// Makes the repository's root the tests' working directory, once. The site reads static/ (the
/// stylesheets, the scripts, every file under /static) from the directory it runs in, which for
/// the server is the root; cargo runs a crate's tests from the crate's own directory. It comes
/// before the app first reads a file: every app the tests build is given `media`, and the
/// fingerprint they work out themselves is `asset_version`'s.
pub fn at_the_root() {
    static ROOT: Once = Once::new();
    ROOT.call_once(|| {
        std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).expect("the repository's root");
    });
}

/// The fingerprint of the site's CSS and JavaScript (pluribus_web::asset_version), worked out from
/// the files at the repository's root, as the pages give it, though no page has been asked for
/// yet.
pub fn asset_version() -> &'static str {
    at_the_root();
    pluribus_web::asset_version()
}

pub struct TestResponse {
    pub status: StatusCode,
    pub location: Option<String>,
    pub content_type: Option<String>,
    /// Set-Cookie headers, as "name=value" without their attributes.
    pub cookies: Vec<String>,
    pub headers: axum::http::HeaderMap,
    pub body: String,
}

impl TestResponse {
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.iter().find_map(|cookie| cookie.strip_prefix(&format!("{name}=")))
    }
}

pub async fn send(pool: PgPool, request: Request<Body>) -> TestResponse {
    send_to(app(pool), request).await
}

pub async fn send_to(app: Router, request: Request<Body>) -> TestResponse {
    let response = app.oneshot(request).await.unwrap();
    let header = |name| {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap().to_string())
    };
    let (status, location, content_type) = (
        response.status(),
        header(header::LOCATION),
        header(header::CONTENT_TYPE),
    );
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().split(';').next().unwrap().to_string())
        .collect();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    TestResponse { status, location, content_type, cookies, headers, body: String::from_utf8_lossy(&body).into_owned() }
}

pub async fn get(pool: PgPool, uri: &str) -> TestResponse {
    send(pool, Request::get(uri).body(Body::empty()).unwrap()).await
}

/// Posts a form, as a browser would, with the cookie given ("name=value") if any.
pub async fn post_form(pool: PgPool, uri: &str, fields: &[(&str, &str)], cookie: Option<&str>) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(pool, request.body(Body::from(body)).unwrap()).await
}

/// The listing form as the browser sends it with photos: multipart, the photos as files.
pub async fn post_with_photos(
    pool: PgPool,
    uri: &str,
    fields: &[(&str, &str)],
    photos: &[Vec<u8>],
    cookie: &str,
) -> TestResponse {
    let (content_type, body) = multipart(fields, photos);
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap();
    send(pool, request).await
}

/// A form with photos as the browser sends it: its content type and its body.
pub fn multipart(fields: &[(&str, &str)], photos: &[Vec<u8>]) -> (String, Vec<u8>) {
    let boundary = "----marketplace-test-boundary";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
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

/// The account behind a test session's cookie (session_for).
pub fn account_of(cookie: &str) -> &str {
    cookie.trim_start_matches("session=test-token-")
}

/// A plain JPEG of that size, as a camera might send.
pub fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&image::DynamicImage::new_rgb8(width, height).to_rgb8())
        .unwrap();
    out
}

pub async fn get_with_cookie(pool: PgPool, uri: &str, cookie: &str) -> TestResponse {
    send(pool, Request::get(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await
}

/// When a form was shown: long enough ago for a person to have filled it in.
pub fn started() -> String {
    (time::OffsetDateTime::now_utc().unix_timestamp() - 30).to_string()
}

/// Signs the user in by writing a session straight into the database; returns the cookie.
pub async fn session_for(pool: &PgPool, user_id: &str) -> String {
    let token = format!("test-token-{user_id}");
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES (sha256(convert_to($1, 'UTF8')), $2::uuid, now() + interval '1 day')",
    )
    .bind(&token)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    format!("session={token}")
}

/// A seller who hasn't posted anything yet, signed in with this phone number; returns the cookie.
pub async fn new_seller(pool: &PgPool, phone: &str) -> String {
    let id: String = sqlx::query_scalar(
        "INSERT INTO users (auth_subject, phone_e164, display_name) VALUES ('phone:' || $1, $1, 'Përdorues')
         RETURNING id::text",
    )
    .bind(phone)
    .fetch_one(pool)
    .await
    .unwrap();
    session_for(pool, &id).await
}

/// The page's HTML with `&` unescaped (Askama writes it as &#38;), so URLs can be compared.
pub async fn html(pool: PgPool, uri: &str) -> String {
    get(pool, uri).await.body.replace("&#38;", "&")
}

/// The id and URL of the lazy frame that loads the next page, if there is one.
pub fn next_frame(html: &str) -> Option<(String, String)> {
    html.match_indices("<turbo-frame id=\"").find_map(|(start, _)| {
        let tag = &html[start..start + html[start..].find('>')?];
        let attribute = |name: &str| {
            let value_start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
            Some(tag[value_start..value_start + tag[value_start..].find('"')?].to_string())
        };
        Some((attribute("id")?, attribute("src")?))
    })
}

/// The listing numbers of the cards on a page, in order.
pub fn card_numbers(html: &str) -> Vec<i64> {
    html.match_indices(r#"class="card-link" href="/listing/"#)
        .map(|(start, matched)| {
            let rest = &html[start + matched.len()..];
            rest[..rest.find('-').unwrap()].parse().unwrap()
        })
        .collect()
}

/// A section of cards under a listing, named by its heading's id: from its opening tag to its end.
pub fn section<'a>(html: &'a str, heading_id: &str) -> &'a str {
    let start = html
        .find(&format!(r#"aria-labelledby="{heading_id}">"#))
        .unwrap_or_else(|| panic!("no section {heading_id}"));
    let rest = &html[start..];
    &rest[..rest.find("</section>").unwrap()]
}

/// The listing numbers of the cards on a page and on every page after it, as scrolling loads them.
pub async fn every_page(pool: &PgPool, uri: &str) -> Vec<i64> {
    let mut page = html(pool.clone(), uri).await;
    let mut numbers = card_numbers(&page);
    while let Some((frame_id, src)) = next_frame(&page) {
        let request = Request::get(&src).header("turbo-frame", &frame_id).body(Body::empty()).unwrap();
        page = send(pool.clone(), request).await.body.replace("&#38;", "&");
        numbers.extend(card_numbers(&page));
    }
    numbers
}

/// Positions of the texts in the page, which must all be there.
pub fn positions(html: &str, texts: &[&str]) -> Vec<usize> {
    texts.iter().map(|text| html.find(text).unwrap_or_else(|| panic!("missing {text}"))).collect()
}

/// Taps the heart of listing `number` as heart_controller.js does: `keep` "1" or "0".
pub fn tap_heart(number: &str, keep: &str, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::post(format!("/listing/{number}/ruaj"))
        .header(header::ACCEPT, "application/json")
        .header(header::HOST, "localhost")
        .header(header::REFERER, "http://localhost/c/automjete?vehicle_fuel=DIESEL")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::from(format!("ruaj={keep}"))).unwrap()
}

/// How wide the cards' photos show, for the browser to pick a copy (Cards::sizes in browse.rs): a
/// column of the grid, a big card, the list's photo, and a card in a row that slides sideways
/// (ROW_SIZES).
pub const SMALL_SIZES: &str = "(min-width: 1100px) 255px, (min-width: 960px) calc((100vw - 80px) / 4), (min-width: 724px) calc((100vw - 64px) / 3), (min-width: 720px) calc((100vw - 48px) / 2), calc((100vw - 38px) / 2)";
pub const BIG_SIZES: &str = "(min-width: 1100px) 522px, (min-width: 960px) calc((100vw - 56px) / 2), (min-width: 720px) 640px, (min-width: 588px) 560px, calc(100vw - 28px)";
pub const LIST_SIZES: &str = "(min-width: 720px) 240px, 116px";
pub const ROW_SIZES: &str = "(min-width: 720px) 236px, 180px";

/// Moves a listing into one of a town's zones.
pub async fn in_zone(pool: &PgPool, number: i64, town: &str, zone: &str) {
    sqlx::query(
        "UPDATE listings SET location_id = (SELECT z.id FROM locations z JOIN locations t ON t.id = z.parent_id
                                            WHERE t.slug = $1 AND z.slug = $2 AND z.location_type = 'AREA')
         WHERE public_no = $3",
    )
    .bind(town)
    .bind(zone)
    .bind(number)
    .execute(pool)
    .await
    .unwrap();
}

/// A complete listing form, with one field replaced.
pub fn listing_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![
        ("category", "telefona"),
        ("listing_type", "SELL"),
        ("condition", "USED"),
        ("title", "Samsung Galaxy S21, 128 GB"),
        ("description", "Telefon në gjendje shumë të mirë, me kuti dhe karikues."),
        ("price_type", "FIXED"),
        ("price", "32 000"),
        ("currency", "ALL"),
        ("city", "tirane"),
        ("name", "Ana"),
        ("website", ""),
        ("started", started),
    ];
    for field in &mut fields {
        if field.0 == replace.0 {
            field.1 = replace.1;
        }
    }
    fields
}

/// Posts the standard test listing as this seller; returns its number.
pub async fn post_listing(pool: &PgPool, seller: &str) -> String {
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(seller)).await;
    let url = posted.location.expect("posted");
    url.trim_start_matches("/listing/").split('-').next().unwrap().to_string()
}

/// A part posted by this seller, with the vehicle it fits; returns its URL.
pub async fn post_part(pool: &PgPool, seller: &str, title: &str, fits: &[(&str, &str)]) -> String {
    let started = started();
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    for field in &mut fields {
        if field.0 == "title" {
            field.1 = title;
        }
    }
    fields.extend_from_slice(fits);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(seller)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    posted.location.expect("posted")
}

/// A car for sale, with its properties, and one field replaced (or added).
pub fn car_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = listing_fields(("category", "makina"), started);
    for field in &mut fields {
        match field.0 {
            "title" => field.1 = "Volkswagen Golf 7 1.6 TDI, 2017",
            "price" => field.1 = "11 500",
            "currency" => field.1 = "EUR",
            _ => {}
        }
    }
    fields.extend([
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_vehicle_year", "2017"),
        ("prop_vehicle_mileage", "120 000"),
        ("prop_vehicle_fuel", "DIESEL"),
        ("prop_vehicle_transmission", "MANUAL"),
        ("prop_vehicle_features", "ABS"),
        ("prop_vehicle_features", "NAVIGATION"),
    ]);
    match fields.iter_mut().find(|field| field.0 == replace.0) {
        Some(field) => field.1 = replace.1,
        None if !replace.0.is_empty() => fields.push(replace),
        None => {}
    }
    fields
}

/// A car whose make and model the seller typed, not in the lists.
pub fn typed_car<'a>(make: &'a str, model: &'a str, started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = car_fields(("prop_vehicle_make", "__other__"), started);
    for field in &mut fields {
        match field.0 {
            "prop_vehicle_model" => field.1 = "__other__",
            "title" => field.1 = "Zastava Yugo 45, 1988",
            _ => {}
        }
    }
    fields.extend([("prop_vehicle_make_other", make), ("prop_vehicle_model_other", model)]);
    fields
}

/// The id of the make or model named so.
pub async fn option_id(pool: &PgPool, label: &str) -> String {
    sqlx::query_scalar("SELECT id::text FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A listing's card on the page, from its opening tag to its end.
pub fn card_of(html: &str, number: i64) -> String {
    let start = html.find(&format!(r#"<article class="card" id="listing-{number}">"#)).unwrap_or_else(|| panic!("no card {number}"));
    let rest = &html[start..];
    rest[..rest.find("</article>").unwrap()].to_string()
}

pub const ANA_ID: &str = "11111111-1111-4111-8111-111111111111";
pub const BESNIK_ID: &str = "22222222-2222-4222-8222-222222222222";
pub const DRITAN_ID: &str = "33333333-3333-4333-8333-333333333333";

/// The value of the first `name` attribute in the HTML.
pub fn attribute(html: &str, name: &str) -> String {
    let start = html.find(&format!(r#" {name}=""#)).unwrap_or_else(|| panic!("no {name} in {html}")) + name.len() + 3;
    html[start..start + html[start..].find('"').unwrap()].to_string()
}

/// The ids of the messages on a conversation's page, in order.
pub fn message_ids(html: &str) -> Vec<String> {
    html.match_indices(r#"data-id=""#)
        .map(|(at, found)| {
            let start = at + found.len();
            html[start..start + html[start..].find('"').unwrap()].to_string()
        })
        .collect()
}

/// How many conversations have messages the user hasn't read, as the header's button asks.
pub async fn unread(pool: &PgPool, cookie: &str) -> String {
    get_with_cookie(pool.clone(), "/mesazhet/numri", cookie).await.body
}

/// Sends a message from a page's form: starting a conversation (/listing/N/mesazh) or in one.
pub async fn write(pool: &PgPool, uri: &str, body: &str, cookie: &str) -> TestResponse {
    post_form(pool.clone(), uri, &[("body", body)], Some(cookie)).await
}

/// The notices of unread messages sent so far (notifications, NEW_MESSAGE), oldest first: to whom,
/// their title, their text and their conversation.
pub async fn message_notices(pool: &PgPool) -> Vec<(String, String, String, String)> {
    sqlx::query_as(
        "SELECT user_id::text, title, body, payload ->> 'conversation' FROM notifications
         WHERE notification_type = 'NEW_MESSAGE' ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Time passes: every message, every notice about one, when each side last read and when each
/// conversation was last written in, `minutes` older.
pub async fn minutes_pass(pool: &PgPool, minutes: i32) {
    for (table, column) in [
        ("messages", "created_at"),
        ("notifications", "created_at"),
        ("conversation_participants", "last_read_at"),
        ("conversations", "last_message_at"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {column} = {column} - make_interval(mins => $1)"))
            .bind(minutes)
            .execute(pool)
            .await
            .unwrap();
    }
}
