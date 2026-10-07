//! Requests through the full router. Each test gets a fresh database with the migrations applied,
//! plus the fixtures named in its attribute.

use std::sync::OnceLock;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use marketplace::{Moderators, media::Media};
use sqlx::PgPool;
use tower::ServiceExt;

/// The tests' moderator (MODERATORS in .env): whoever signs in with this number.
const MODERATOR: &str = "+355690000099";

/// The app, with photos kept in memory, one store for all the tests: their photos' names never
/// collide, and a photo posted in one request is there for the next. MODERATOR moderates.
fn app(pool: PgPool) -> Router {
    static MEDIA: OnceLock<Media> = OnceLock::new();
    marketplace::app_with(pool, MEDIA.get_or_init(Media::in_memory).clone(), Moderators::new([MODERATOR]))
}

struct TestResponse {
    status: StatusCode,
    location: Option<String>,
    content_type: Option<String>,
    /// Set-Cookie headers, as "name=value" without their attributes.
    cookies: Vec<String>,
    body: String,
}

impl TestResponse {
    fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.iter().find_map(|cookie| cookie.strip_prefix(&format!("{name}=")))
    }
}

async fn send(pool: PgPool, request: Request<Body>) -> TestResponse {
    let response = app(pool).oneshot(request).await.unwrap();
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
    let body = response.into_body().collect().await.unwrap().to_bytes();
    TestResponse { status, location, content_type, cookies, body: String::from_utf8_lossy(&body).into_owned() }
}

async fn get(pool: PgPool, uri: &str) -> TestResponse {
    send(pool, Request::get(uri).body(Body::empty()).unwrap()).await
}

/// Posts a form, as a browser would, with the cookie given ("name=value") if any.
async fn post_form(pool: PgPool, uri: &str, fields: &[(&str, &str)], cookie: Option<&str>) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(pool, request.body(Body::from(body)).unwrap()).await
}

/// A form sent from this network: the address a proxy in front of the app says it came from.
async fn post_form_from(pool: PgPool, uri: &str, fields: &[(&str, &str)], ip: &str) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", ip);
    send(pool, request.body(Body::from(body)).unwrap()).await
}

/// The listing form as the browser sends it with photos: multipart, the photos as files.
async fn post_with_photos(
    pool: PgPool,
    uri: &str,
    fields: &[(&str, &str)],
    photos: &[Vec<u8>],
    cookie: &str,
) -> TestResponse {
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
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap();
    send(pool, request).await
}

/// A plain JPEG of that size, as a camera might send.
fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&image::DynamicImage::new_rgb8(width, height).to_rgb8())
        .unwrap();
    out
}

async fn get_with_cookie(pool: PgPool, uri: &str, cookie: &str) -> TestResponse {
    send(pool, Request::get(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await
}

/// When a form was shown: long enough ago for a person to have filled it in.
fn started() -> String {
    (time::OffsetDateTime::now_utc().unix_timestamp() - 30).to_string()
}

/// Signs the user in by writing a session straight into the database; returns the cookie.
async fn session_for(pool: &PgPool, user_id: &str) -> String {
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
async fn new_seller(pool: &PgPool, phone: &str) -> String {
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
async fn html(pool: PgPool, uri: &str) -> String {
    get(pool, uri).await.body.replace("&#38;", "&")
}

/// The id and URL of the lazy frame that loads the next page, if there is one.
fn next_frame(html: &str) -> Option<(String, String)> {
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
fn card_numbers(html: &str) -> Vec<i64> {
    html.match_indices(r#"class="card-link" href="/listing/"#)
        .map(|(start, matched)| {
            let rest = &html[start + matched.len()..];
            rest[..rest.find('-').unwrap()].parse().unwrap()
        })
        .collect()
}

/// Positions of the texts in the page, which must all be there.
fn positions(html: &str, texts: &[&str]) -> Vec<usize> {
    texts.iter().map(|text| html.find(text).unwrap_or_else(|| panic!("missing {text}"))).collect()
}

#[sqlx::test]
async fn home_page_lists_categories(pool: PgPool) {
    let response = get(pool, "/").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Automjete"));
    assert!(response.body.contains("Nuk u gjet asnjë njoftim."));
}

#[sqlx::test(fixtures("listings"))]
async fn welcome_page_asks_for_words_and_offers_the_categories(pool: PgPool) {
    let body = get(pool, "/").await.body;
    assert!(body.contains("Çfarë po kërkon?"));
    // Words only: no city, no category; the results' filters narrow them. Suggestions open under
    // the words as they are typed.
    assert!(body.contains(r#"<form class="welcome-form" action="/search" method="get" role="search" data-controller="suggest""#));
    assert!(body.contains(r#"<input id="welcome-q" type="search" name="q""#));
    assert!(body.contains(r#"<div class="suggestions" id="welcome-suggestions" role="listbox""#));
    // The search button is a magnifier inside the field's pill, as in Claude's app.
    assert!(body.contains(r#"<button type="submit" class="icon-button welcome-search" aria-label="Kërko""#));
    assert!(!body.contains(r#"name="city""#) && !body.contains(r#"name="category""#));
    // A shortcut to every main category, in their order, each its icon before its name; no more
    // listing-like cards.
    let main = [
        ("automjete", "Automjete"),
        ("prona", "Prona"),
        ("elektronike", "Elektronikë"),
        ("shtepi-dhe-kopsht", "Shtëpi dhe kopsht"),
        ("pune", "Punë"),
        ("sherbime", "Shërbime"),
        ("veshje-dhe-aksesore", "Veshje dhe aksesorë"),
        ("sport-dhe-hobi", "Sport dhe hobi"),
        ("kafshe-shtepiake", "Kafshë shtëpiake"),
        ("makineri-dhe-biznes", "Makineri dhe biznes"),
        ("te-tjera", "Të tjera"),
    ];
    let links: Vec<String> =
        main.iter().map(|(slug, _)| format!(r#"<a class="shortcut" href="/c/{slug}">"#)).collect();
    let shortcuts = positions(&body, &links.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(shortcuts.is_sorted(), "{shortcuts:?}");
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), main.len());
    for (_, name) in main {
        assert!(body.contains(&format!("</svg>{name}</a>")), "{name}");
    }
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), body.matches(r#"<svg class="shortcut-icon""#).count());
    assert!(!body.contains(r#"class="tile""#));
}

#[sqlx::test(fixtures("listings"))]
async fn welcome_page_ignores_the_city_it_used_to_remember(pool: PgPool) {
    // The welcome page no longer asks for a city, so the one an older visit left in a cookie
    // would narrow the categories unseen: it is ignored.
    let request = Request::get("/")
        .header(header::COOKIE, "consent=1; city=vlore")
        .body(Body::empty())
        .unwrap();
    let body = send(pool, request).await.body;
    assert!(body.contains(r#"<a class="shortcut" href="/c/automjete">"#));
    assert!(!body.contains("city=vlore"));
}

#[sqlx::test(fixtures("listings"))]
async fn bumped_listing_comes_first(pool: PgPool) {
    let body = get(pool, "/").await.body;
    let house = body.find("Shtëpi private në shitje në Vlorë").unwrap();
    let phone = body.find("iPhone 13, 128 GB, si i ri").unwrap();
    assert!(house < phone, "the house was bumped after the phone was published");
}

#[sqlx::test(fixtures("listings"))]
async fn category_page_includes_subcategories(pool: PgPool) {
    // Automjete has no listings of its own; the cars sit in its Makina subcategory.
    let response = get(pool, "/c/automjete").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("iPhone 13"));
    assert!(response.body.contains(r#"<p class="result-count">2 njoftime</p>"#));
    // Cards show the cover photo's thumbnail.
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=480"));
}

#[sqlx::test(fixtures("listings"))]
async fn card_details_start_with_the_city(pool: PgPool) {
    // A location pin, then the city, then the category's card properties.
    let pin = r#"<p class="card-summary"><svg class="pin-icon" "#;
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(cars.contains(pin));
    assert!(cars.contains("</svg>Tiranë · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains("</svg>Tiranë · 2+1 · 85\u{a0}m²</p>"));
    // Rents are per month.
    assert!(property.contains("450\u{a0}€/muaj"));
    // Without properties, the line is just the city.
    let other = get(pool.clone(), "/c/te-tjera").await.body;
    assert!(other.contains(pin) && other.contains("</svg>Berat</p>"));
    // A phone shows its storage on the card; its brand only on its page.
    let phones = get(pool.clone(), "/c/elektronike").await.body;
    assert!(phones.contains("</svg>Tiranë · 128 GB</p>"));
    // Searching in Tirana, the Tirana cards leave the city out and show the brand instead...
    let in_tirana = get(pool.clone(), "/c/elektronike?city=tirane").await.body;
    assert!(in_tirana.contains(r#"<p class="card-summary">Apple · 128 GB</p>"#));
    // ...and a car the first property its title doesn't name: not the make, nor the model.
    let cars = get(pool, "/c/automjete?city=tirane").await.body;
    assert!(cars.contains("<p class=\"card-summary\">Hatchback · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
}

#[sqlx::test(fixtures("listings"))]
async fn a_card_links_to_its_listing_through_the_title(pool: PgPool) {
    let body = get(pool, "/").await.body;
    // The title link is stretched over the whole card (see .card-link in app.css).
    assert!(body.contains(
        r#"<h2 class="card-title"><a class="card-link" href="/listing/1001-volkswagen-golf-7-2016-nafte">"#
    ));
    assert_eq!(body.matches(r#"<article class="card""#).count(), 12);
}

#[sqlx::test(fixtures("listings"))]
async fn a_map_pin_finds_the_listings_within_its_radius_nearest_first(pool: PgPool) {
    // Tirana's centre, where the fixture puts the Tirana listings. Durrës is 31 km away.
    let body = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=10").await;
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("</svg>Tiranë · 0\u{a0}m · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    assert!(body.contains(r#"<a class="filter-chip" href="/search" aria-label="Hiq filtrin Brenda 10 km nga pika">"#));
    assert!(body.contains(r#"<input class="visually-hidden" type="radio" name="sort" value="" form="filter-form" data-action="filters#submit" checked>Më afër</label>"#));

    // From Durrës, 50 km reaches Tirana: Durrës's listings first, then Tirana's.
    let body = get(pool, "/search?lat=41.31333&lng=19.44583&radius=50").await.body;
    let order = positions(&body, &[
        "Mercedes-Benz C 220, 2018, automatike",
        "Volkswagen Golf 7, 2016, naftë",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(body.contains("</svg>Tiranë · 31\u{a0}km · 2016"));
}

#[sqlx::test(fixtures("listings"))]
async fn nearest_pages_continue_where_the_last_one_ended(pool: PgPool) {
    // 40 more in Të tjera, north of Tirana's centre at nine distances, so many are tied and the
    // id has to break the ties.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, latitude, longitude, status,
                               published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 41.328333 + g % 9 * 0.01, 19.818056, 'ACTIVE', now()
         FROM generate_series(1, 40) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut seen = Vec::new();
    let mut uri = "/c/te-tjera?lat=41.32833&lng=19.81806&radius=25".to_string();
    let mut frame_id = None;
    loop {
        let mut request = Request::get(&uri);
        if let Some(id) = &frame_id {
            request = request.header("turbo-frame", id);
        }
        let page = send(pool.clone(), request.body(Body::empty()).unwrap()).await.body;
        let page = page.replace("&#38;", "&");
        seen.extend(card_numbers(&page));
        let Some((id, src)) = next_frame(&page) else { break };
        (frame_id, uri) = (Some(id), src);
    }
    let expected: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings
         WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
           AND earth_distance(ll_to_earth(41.32833, 19.81806), ll_to_earth(latitude::float8, longitude::float8)) <= 25000
         ORDER BY earth_distance(ll_to_earth(41.32833, 19.81806), ll_to_earth(latitude::float8, longitude::float8)), id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(expected.len(), 40, "the book listing in Berat is 70 km away");
    assert_eq!(seen, expected);
}

#[sqlx::test(fixtures("listings"))]
async fn the_map_page_reloads_only_its_results(pool: PgPool) {
    let page = get(pool.clone(), "/harta").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains(r#"data-controller="map""#));
    assert!(page.body.contains(r#"<turbo-frame id="map-results""#));
    // The button that folds the map away (map_controller.js).
    assert!(page.body.contains(r#"data-action="map#toggleMap""#) && page.body.contains("Fshih hartën</button>"));
    // Until the map is moved, its pin is in the middle of Tirana, and so are the results: not the
    // Mercedes in Durrës, 31 km away. The map is the pin here, so no chip offers to drop it.
    assert!(page.body.contains(r#"data-map-latitude-value="41.3275" data-map-longitude-value="19.8187""#));
    assert!(page.body.contains(r#"<input type="hidden" name="lat" value="41.32750" data-map-target="latitude">"#));
    assert!(page.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!page.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!page.body.contains("Hiq filtrin Brenda"));
    // A dot for each result, with what its popup shows.
    assert!(page.body.contains(
        "<li data-lat=\"41.328333\" data-lng=\"19.818056\" data-url=\"/listing/1001-volkswagen-golf-7-2016-nafte\" \
         data-price=\"9\u{a0}500\u{a0}€\">Volkswagen Golf 7, 2016, naftë</li>"
    ));

    // A pin and a category: what's within the circle, in the category.
    let body = html(pool.clone(), "/harta?lat=41.32833&lng=19.81806&radius=5&category=makina").await;
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("iPhone 13"));
    assert!(body.contains(r#"data-map-latitude-value="41.32833""#));
    assert!(body.contains(r#"<input type="hidden" name="lat" value="41.32833" data-map-target="latitude">"#));

    // The results frame gets the whole page, which Turbo takes the frame out of.
    let request = Request::get("/harta?lat=41.32833&lng=19.81806&radius=5")
        .header("turbo-frame", "map-results")
        .body(Body::empty())
        .unwrap();
    assert!(send(pool.clone(), request).await.body.contains("<html"));
}

#[sqlx::test(fixtures("listings"))]
async fn any_whole_distance_up_to_50_km_can_be_picked(pool: PgPool) {
    let seven = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=7").await;
    assert!(seven.contains("Brenda 7 km nga pika"));
    // Out of range or not a whole number: the usual 10 km.
    for radius in ["0", "51", "2.5"] {
        let page = html(pool.clone(), &format!("/search?lat=41.32833&lng=19.81806&radius={radius}")).await;
        assert!(page.contains("Brenda 10 km nga pika"), "{radius}");
    }
}

#[sqlx::test(fixtures("listings"))]
async fn filters_map_and_order_stand_side_by_side(pool: PgPool) {
    // The search, Filtrat, Harta and Rendit stand in one row over the results, round buttons, the
    // orders opening in a menu; opening one says so (section:open) and the others fold, so only
    // one is open at a time. The search is in the site's colour. What they open floats over the
    // results, the page dimmed under it; Escape or a tap on the dim folds it again.
    let cars = html(pool.clone(), "/c/makina").await;
    let order = positions(&cars, &[
        r#"<div class="browse-tools" data-filters-target="tools">"#,
        r#"<div class="search-card" data-controller="panel""#,
        r#"<div class="filters-card">"#,
        r#"<section class="map-card map-hidden""#,
        r#"<details class="sort-menu" data-controller="dropdown""#,
        r#"<div class="browse-scrim" aria-hidden="true" data-filters-target="scrim" data-action="click->filters#foldAll"></div>"#,
        r#"<section class="results""#,
    ]);
    assert!(order.is_sorted());
    for listener in ["panel#foldFor", "filters#foldFor", "map#foldFor", "dropdown#foldFor", "filters#fit"] {
        assert!(cars.contains(&format!("section:open@document->{listener}")), "{listener}");
    }
    assert!(cars.contains("keydown.esc@document->filters#foldAll"));
    // The filters and the map apply as soon as something changes, the filters also as they close,
    // the page refreshing in place, which keeps what is open as it is and each listing's card;
    // only without JavaScript do the filters have a button.
    assert!(cars.contains(r#"data-action="change->filters#changed keydown.enter->filters#enter">"#));
    assert!(cars.contains(r#"<noscript><div class="filter-actions">"#));
    for keep in [
        "turbo:before-morph-element->filters#keep",
        "turbo:before-morph-attribute->filters#keepAttribute",
        "turbo:before-morph-attribute->panel#keep",
        "turbo:before-morph-element->map#keep",
        "turbo:before-morph-attribute->map#keepOpen",
        "turbo:morph@document->map#reloadPoints",
    ] {
        assert!(cars.contains(keep), "{keep}");
    }
    assert!(cars.contains(r#"<article class="card" id="listing-1001">"#));
    // The map's and the order's buttons have only their icons to see: their names and what they
    // are set to are for screen readers, and their tooltips; the orders' menu opens with no
    // JavaScript.
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button map-button" data-map-target="toggle""#));
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button filters-open" aria-controls="filters" aria-expanded="false""#));
    assert!(cars.contains(r#"<button type="button" class="button round-button search-button""#));
    assert!(cars.contains(r#"<summary class="button secondary round-button sort-button" aria-label="Rendit: Më të rejat" title="Rendit: Më të rejat">"#));
    assert!(cars.contains("toggle->dropdown#announce"));
    let menu = &cars[cars.find(r#"<details class="sort-menu""#).unwrap()..];
    assert!(menu[..menu.find("</details>").unwrap()].contains(r#"<fieldset class="sort-options">"#));

    // The top bar has no search: the row starts with a round button that opens one under it,
    // which searches the page's category and keeps its filters.
    let header = &cars[..cars.find("</header>").unwrap()];
    assert!(!header.contains(r#"name="q""#));
    let fuel = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&sort=price_asc").await;
    let search = &fuel[fuel.find(r#"<form class="search" action="/c/makina""#).expect("the row's search")..];
    let search = &search[..search.find("</form>").unwrap()];
    assert!(search.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(search.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(search.contains(r#"<input type="search" name="q" value="""#));
}

#[sqlx::test(fixtures("listings"))]
async fn big_cards_slide_through_every_photo(pool: PgPool) {
    // Two more photos for the Golf, after its cover.
    sqlx::query(
        "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url, sort_order)
         SELECT l.id, 'IMAGE', 'READY', 'extra/' || n, 'https://example.com/full-' || n || '.jpg',
                'https://example.com/card-' || n || '.jpg', n
         FROM listings l, generate_series(1, 2) n WHERE l.public_no = 1001",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Small cards unless the visitor chose big ones: a small card shows its first photo (app.css),
    // the copy fetched for a column of the grid.
    let small = get(pool.clone(), "/c/makina").await.body;
    assert!(small.contains(r#"<div class="browse" data-controller="filters cards""#));
    assert!(small.contains(r#"aria-pressed="false" aria-label="Kartela të mëdha""#));
    assert!(small.contains(r#"sizes="(min-width: 720px) 260px, 50vw""#));

    // Big ones, from the cookie the view button sets: every photo, the cover first, each with its
    // full copy for the browser to pick, and the dots and arrows to slide through them.
    let request = Request::get("/c/makina").header("cookie", "cards=big").body(Body::empty()).unwrap();
    let big = send(pool.clone(), request).await.body;
    assert!(big.contains(r#"<div class="browse cards-big" data-controller="filters cards""#));
    assert!(big.contains(r#"aria-pressed="true" aria-label="Kartela të mëdha""#));
    assert!(!big.contains(r#"sizes="(min-width: 720px) 260px, 50vw""#));
    assert_eq!(big.matches(r#"<div class="card-media" data-controller="slides">"#).count(), 1, "only the Golf has more than one");
    let golf = &big[big.find(r#"<div class="card-media" data-controller="slides">"#).unwrap()..];
    let golf = &golf[..golf.find("</article>").unwrap()];
    assert!(golf.contains(r#"href="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    assert_eq!(golf.matches(r#"<a class="card-slide""#).count(), 3);
    let cover = golf.find("photo-1591230740238").unwrap();
    assert!(cover < golf.find("card-1.jpg").unwrap() && golf.find("card-1.jpg").unwrap() < golf.find("card-2.jpg").unwrap());
    assert!(golf.contains(r#"srcset="https://example.com/card-2.jpg 600w, https://example.com/full-2.jpg 1600w" sizes="(min-width: 720px) 40rem, 100vw""#));
    assert_eq!(golf.matches(r#"<span class="is-current"></span><span></span><span></span>"#).count(), 1, "a dot for each photo");
    // Over the photo's top left, who sells it, their initial in a circle, and since when they are
    // a user.
    assert!(golf.contains(r#"<p class="card-seller"><span class="avatar" aria-hidden="true">A</span><span class="card-seller-text"><span class="card-seller-name">Ana</span><span>Përdorues që nga "#));
    assert!(golf.contains(r#"aria-label="Fotoja tjetër" hidden>"#));

    // The next pages come the same way, and the map page's list stays small.
    let pinned = Request::get("/harta").header("cookie", "cards=big").body(Body::empty()).unwrap();
    assert!(send(pool, pinned).await.body.contains(r#"sizes="(min-width: 720px) 260px, 50vw""#));
}

#[sqlx::test(fixtures("listings"))]
async fn category_pages_keep_their_map_folded(pool: PgPool) {
    // Every category page has its map, in a card folded to its header; with no point, the
    // results are from everywhere. Nothing else opens it.
    let cars = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await;
    assert!(cars.contains(r#"<section class="map-card map-hidden" data-controller="map" data-map-lazy-value="true""#));
    assert!(cars.contains(r#"aria-controls="map-card-body" aria-expanded="false""#));
    assert!(cars.contains(r#"aria-label="Harta: Kudo" title="Harta: Kudo""#));
    assert!(!cars.contains("map-open") && !cars.contains("Brenda 10 km nga pika"));
    // Its dots come when it first opens: the same page with harta=1, whose frame has them.
    assert!(cars.contains(r#"<turbo-frame id="map-points" data-map-target="frame" data-src="/c/makina?vehicle_fuel=DIESEL&harta=1">"#));
    assert!(!cars.contains(r#"data-url="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    let dots = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&harta=1").await;
    assert!(dots.contains(r#"data-url="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    // Its form sets the point, keeping the filters, by itself once the map stops: the card has no
    // buttons, and the point's chip drops it.
    assert!(cars.contains(r#"<form class="map-form" action="/c/makina" method="get" data-map-target="form">"#));
    assert!(cars.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(!cars.contains("map#confirm") && !cars.contains("map-everywhere"));
    // Beside "Vendndodhja ime", the towns, by county, each with its point, which the map moves
    // onto; the dropdown has no name, so it sends no town of its own.
    assert!(cars.contains(r#"<select aria-label="Qyteti" data-map-target="town" data-action="map#goToTown">"#));
    assert!(cars.contains(r#"<option value="tirane" data-lat="41.328333" data-lng="19.818056">Tiranë</option>"#));

    // With a point, the page stays the category's, with its heading and filters, the card still
    // folded, saying how far around the point; the filter form and the category menu keep it.
    let pinned = html(pool, "/c/makina?lat=41.32833&lng=19.81806&radius=5&sort=price_asc").await;
    assert!(pinned.contains("<h1>Makina"));
    assert!(pinned.contains(r#"<section class="map-card map-hidden""#));
    assert!(pinned.contains(r#"data-map-latitude-value="41.32833""#));
    // The map's button says how far, to screen readers and as its tooltip; the filters' summary
    // leaves the point to it, and to its chip.
    assert!(pinned.contains(r#"aria-label="Harta: 5 km""#));
    assert!(pinned.contains(r#"aria-label="Filtrat: Asnjë filtër""#));
    // The distance is a slider; 5 km stands at 41 of 100 (50 ^ 0.41 = 5).
    assert!(pinned.contains(r#"<output data-map-target="radiusLabel">5 km</output>"#));
    assert!(pinned.contains(r#"<input id="map-radius" type="range" min="0" max="100" step="1" value="41""#));
    for (name, value) in [("lat", "41.32833"), ("lng", "19.81806"), ("radius", "5")] {
        assert!(pinned.contains(&format!(r#"<input type="hidden" name="{name}" value="{value}">"#)), "{name}");
    }
    // A new pin or radius keeps the order picked.
    let map_form = &pinned[pinned.find(r#"<form class="map-form""#).unwrap()..];
    let map_form = &map_form[..map_form.find("</form>").unwrap()];
    assert!(map_form.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(pinned.contains(r#"href="/c/prona?lat=41.32833&lng=19.81806&radius=5""#));
    // The point's chip searches everywhere again, keeping the rest.
    assert!(pinned.contains(r#"<a class="filter-chip" href="/c/makina?sort=price_asc" aria-label="Hiq filtrin Brenda 5 km nga pika">"#));
}

#[sqlx::test(fixtures("listings"))]
async fn the_map_has_a_dot_for_every_result_up_to_its_limit(pool: PgPool) {
    // 501 more in Të tjera, north of Tirana's centre at nine distances, 1.1 km apart.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, latitude, longitude, status,
                               published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 41.328333 + g % 9 * 0.01, 19.818056, 'ACTIVE', now()
         FROM generate_series(1, 501) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let dots = |html: &str| html.matches("<li data-lat=").count();

    // Within 5 km, 279 of them: all on the map, while the list shows them 24 at a time.
    let page = html(pool.clone(), "/harta?category=te-tjera&lat=41.32833&lng=19.81806&radius=5").await;
    assert_eq!(dots(&page), 279);
    assert_eq!(card_numbers(&page).len(), 24);
    assert!(!page.contains("map-note"));
    // The list's next pages bring cards only.
    let (id, uri) = next_frame(&page).unwrap();
    let request = Request::get(uri).header("turbo-frame", id).body(Body::empty()).unwrap();
    let next = send(pool.clone(), request).await.body;
    assert_eq!(card_numbers(&next).len(), 24);
    assert_eq!(dots(&next), 0);

    // Within 10 km, all 501: the map shows the nearest 500, and says so.
    let page = html(pool, "/harta?category=te-tjera&lat=41.32833&lng=19.81806&radius=10").await;
    assert_eq!(dots(&page), 500);
    assert!(page.contains(r#"<p class="map-note">Në hartë shfaqen 500 njoftimet e para të listës.</p>"#));
}

#[sqlx::test(fixtures("listings"))]
async fn search_ignores_accents(pool: PgPool) {
    let response = get(pool, "/search?q=shtepi+vlore").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Shtëpi private në shitje në Vlorë"));
    assert!(!response.body.contains("Apartament 2+1"));
}

#[sqlx::test(fixtures("listings"))]
async fn search_matches_property_values(pool: PgPool) {
    // Neither the title nor the description says "telefon"; its type property does.
    let body = get(pool, "/search?q=telefon").await.body;
    assert!(body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(!body.contains("Laptop Lenovo ThinkPad T14"));
}

#[sqlx::test(fixtures("listings"))]
async fn search_forgives_typos_and_names_the_word_it_searched(pool: PgPool) {
    // No listing says "golff": the Golf is found, and the page says "golf" was searched too.
    let misspelt = get(pool.clone(), "/search?q=golff").await.body;
    assert_eq!(card_numbers(&misspelt), vec![1001]);
    assert!(misspelt.contains(r#"<p class="search-corrections">Shfaqen edhe rezultatet për „golf“.</p>"#));
    // A word listings use is searched as typed.
    let typed = get(pool, "/search?q=golf").await.body;
    assert!(typed.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!typed.contains("search-corrections"));
}

#[sqlx::test(fixtures("listings"))]
async fn search_finds_other_forms_of_a_word(pool: PgPool) {
    // The flat's title says "Apartament" and the search the plural, "apartamente": the same word,
    // which needs no note.
    let body = get(pool, "/search?q=apartamente").await.body;
    assert!(body.contains("Apartament 2+1 me qira në Tiranë"));
    assert!(!body.contains("search-corrections"));
}

#[sqlx::test(fixtures("listings"))]
async fn a_word_left_out_stays_out_of_a_widened_search(pool: PgPool) {
    // The Golf runs on diesel (naftë): leaving that out leaves the Golf out, typo or not.
    let body = get(pool, "/search?q=golff+-naft%C3%AB").await.body;
    assert!(card_numbers(&body).is_empty());
}

#[sqlx::test(fixtures("listings"))]
async fn search_finds_listings_by_their_category_and_town(pool: PgPool) {
    // The laptop says neither "elektronikë" nor "Shkodër": its category and its town do, and the
    // town is found with its article too ("Shkodra").
    assert_eq!(card_numbers(&get(pool.clone(), "/search?q=elektronike+shkoder").await.body), vec![1006]);
    assert_eq!(card_numbers(&get(pool, "/search?q=laptop+shkodra").await.body), vec![1006]);
}

#[sqlx::test(fixtures("listings"))]
async fn filler_words_needn_t_be_in_a_listing(pool: PgPool) {
    // The Mercedes is in Durrës, but nothing it says has "në".
    assert_eq!(card_numbers(&get(pool, "/search?q=mercedes+n%C3%AB+durr%C3%ABs").await.body), vec![1002]);
}

#[sqlx::test(fixtures("listings"))]
async fn a_search_across_categories_names_the_categories_it_found(pool: PgPool) {
    // Each chip narrows the search to its category, where that category's filters are.
    let found = html(pool.clone(), "/search?q=golf").await;
    assert!(found.contains(r#"<a class="chip" href="/c/makina?q=golf">Makina <span class="chip-count">1</span></a>"#));
    // Tirana's listings are in three categories.
    let tirana = html(pool.clone(), "/search?q=tirane").await;
    for category in ["makina", "apartamente", "telefona"] {
        assert!(tirana.contains(&format!(r#"<a class="chip" href="/c/{category}?q=tirane">"#)), "{category}");
    }
    // A category's page has its subcategories instead.
    assert!(!html(pool, "/c/makina?q=golf").await.contains("chip-count"));
}

#[sqlx::test(fixtures("listings"))]
async fn suggestions_finish_the_words_and_say_where_they_lead(pool: PgPool) {
    // The word finished as the listings have it, then the category it is found in, then the make
    // and model it names.
    let rows = html(pool.clone(), "/sugjerime?q=gol").await;
    let at = positions(&rows, &[
        r#"href="/search?q=golf""#,
        r#"href="/c/makina?q=golf""#,
        r#"href="/c/makina?vehicle_make=VOLKSWAGEN&vehicle_model=VOLKSWAGEN_GOLF""#,
    ]);
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{rows}");
    assert!(rows.contains(r#"<span class="suggestion-label">Volkswagen Golf</span>"#));
    // A town the last word starts: on its own, all its listings; after other words, those there.
    let town = html(pool.clone(), "/sugjerime?q=durr").await;
    assert!(town.contains(r#"href="/search?city=durres""#) && town.contains("Njoftimet në Durrës"), "{town}");
    let there = html(pool.clone(), "/sugjerime?q=golf%20durr").await;
    assert!(there.contains(r#"href="/search?q=golf&city=durres""#) && there.contains("në Durrës"), "{there}");
    // A whole word misspelt: the word it most likely misspells.
    assert!(html(pool.clone(), "/sugjerime?q=mercedez").await.contains(r#"href="/search?q=mercedes""#));
    // A brand of the very name the words have: its row, which finds it alone, not the words
    // searched in its category too.
    let brand = html(pool.clone(), "/sugjerime?q=apple").await;
    assert!(brand.contains(r#"href="/c/telefona?electronics_brand=APPLE""#), "{brand}");
    assert!(!brand.contains(r#"href="/c/telefona?q=apple""#), "{brand}");
    // Too little to go on.
    assert!(!get(pool, "/sugjerime?q=g").await.body.contains("suggestion"));
}

#[sqlx::test(fixtures("listings"))]
async fn city_filter_keeps_to_that_city(pool: PgPool) {
    let body = get(pool, "/search?city=durres").await.body;
    assert!(body.contains("<h1>Njoftimet në Durrës</h1>"));
    assert!(body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("Kërkohet kamarier/e me eksperiencë"));
    assert!(!body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(body.contains(r#"<p class="result-count">2 njoftime</p>"#));
}

#[sqlx::test(fixtures("listings"))]
async fn welcome_form_narrows_to_a_category_and_city(pool: PgPool) {
    let body = html(pool, "/search?category=automjete&city=tirane").await;
    assert!(body.contains("<h1>Automjete</h1>"));
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"), "the Mercedes is in Durrës");
    assert!(!body.contains("iPhone 13"), "the iPhone is in another category");
    // Both are filters here, so both can be removed.
    assert!(body.contains(r#"<a class="filter-chip" href="/search?city=tirane""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/search?category=automjete""#));
}

#[sqlx::test(fixtures("listings"))]
async fn price_filter_converts_currencies(pool: PgPool) {
    // 55 000 Lekë is 550 € and 62 000 Lekë is 620 €, at the placeholder rate of 100 Lekë per euro.
    let body = get(pool.clone(), "/c/elektronike?price_max=600&currency=EUR").await.body;
    assert!(body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(!body.contains("Laptop Lenovo ThinkPad T14"));
    let body = get(pool, "/c/elektronike?price_min=60.000&currency=ALL").await.body;
    assert!(!body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(body.contains("Laptop Lenovo ThinkPad T14"));
    assert!(body.contains(">nga 60\u{a0}000\u{a0}Lekë<"), "the chip shows the bound");
}

#[sqlx::test(fixtures("listings"))]
async fn the_price_is_a_slider_with_two_ends(pool: PgPool) {
    // Without a price, the ends stand at the edges: no limit either way.
    let any = html(pool.clone(), "/c/elektronike").await;
    assert!(any.contains(r#"<legend class="filter-label">Çmimi <output data-price-range-target="label">Çdo çmim</output></legend>"#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="0" aria-label="Çmimi nga""#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="1000" aria-label="Çmimi deri""#));
    // 1 000 € is two of the scale's five decades from 10 €, so 400 of 1 000; 20 000 € is
    // log10(2 000) = 3.30 of them, so 660. The hidden fields carry the prices themselves.
    let some = html(pool, "/c/elektronike?price_min=1000&price_max=20000&currency=EUR").await;
    assert!(some.contains(r#"value="400" aria-label="Çmimi nga""#));
    assert!(some.contains(r#"value="660" aria-label="Çmimi deri""#));
    assert!(some.contains(r#"<input type="hidden" name="price_min" value="1000" data-price-range-target="min">"#));
    assert!(some.contains("<output data-price-range-target=\"label\">1\u{a0}000 – 20\u{a0}000\u{a0}€</output>"));
}

#[sqlx::test(fixtures("listings"))]
async fn sorts_by_price_with_unpriced_listings_last(pool: PgPool) {
    let body = get(pool.clone(), "/search?sort=price_asc").await.body;
    let order = positions(&body, &[
        "Dhurohen libra shkollorë", // free
        "Xhaketë dimri për meshkuj, masa M",
        "Biçikletë malore 29 inç",
        "Shtëpi private në shitje në Vlorë",
        "Kërkohet kamarier/e me eksperiencë", // salary on request
    ]);
    assert!(order.is_sorted(), "{order:?}");

    let body = get(pool, "/search?sort=price_desc").await.body;
    let order = positions(&body, &[
        "Shtëpi private në shitje në Vlorë",
        "Mercedes-Benz C 220, 2018, automatike",
        "Biçikletë malore 29 inç",
        "Dhurohen libra shkollorë",
        "Kërkohet kamarier/e me eksperiencë",
    ]);
    assert!(order.is_sorted(), "{order:?}");
}

#[sqlx::test(fixtures("listings"))]
async fn price_pages_continue_where_the_last_one_ended(pool: PgPool) {
    // 40 more in Të tjera: 30 with prices, many of them equal so the id has to break ties, and
    // 10 with the price on request.
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

    for (sort, order) in [("price_asc", "ASC"), ("price_desc", "DESC")] {
        let mut seen = Vec::new();
        let mut uri = format!("/c/te-tjera?sort={sort}");
        let mut frame_id = None;
        loop {
            let mut request = Request::get(&uri);
            if let Some(id) = &frame_id {
                request = request.header("turbo-frame", id);
            }
            let page = send(pool.clone(), request.body(Body::empty()).unwrap()).await.body;
            let page = page.replace("&#38;", "&");
            seen.extend(card_numbers(&page));
            let Some((id, src)) = next_frame(&page) else { break };
            (frame_id, uri) = (Some(id), src);
        }
        let expected: Vec<i64> = sqlx::query_scalar(&format!(
            "SELECT public_no FROM listings
             WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
             ORDER BY price_normalized {order} NULLS LAST, id {order}"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(expected.len(), 41);
        assert_eq!(seen, expected, "{sort}");
    }
}

#[sqlx::test(fixtures("listings"))]
async fn filter_form_offers_the_category_properties(pool: PgPool) {
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(cars.contains(r#"<legend class="filter-label">Karburanti</legend>"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_fuel" value="DIESEL">"#));
    assert!(cars.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#), "models are grouped by make");
    assert!(cars.contains("Gjendja"));
    // A few boxes share their line with the next field (Karburanti, five, beside Kambio); more
    // take one of their own, the colours' seven, as do the body types (Karroceria), whose boxes
    // have the car's side over each name.
    let fuel = cars.find(r#"<legend class="filter-label">Karburanti</legend>"#).unwrap();
    assert!(cars[..fuel].trim_end().ends_with(r#"<fieldset class="filter-group filter-options filter-half">"#));
    let body = cars.find(r#"<legend class="filter-label">Karroceria</legend>"#).unwrap();
    assert!(cars[..body].trim_end().ends_with(r#"<fieldset class="filter-group filter-options filter-icons">"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_body_type" value="WAGON"><svg class="check-icon""#));
    assert_eq!(cars.matches(r#"<svg class="check-icon""#).count(), 7, "one for each body type, none for the others");
    let colour = cars.find(r#"<legend class="filter-label">Ngjyra</legend>"#).unwrap();
    assert!(cars[..colour].trim_end().ends_with(r#"<fieldset class="filter-group filter-options">"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_color" value="WHITE"> E bardhë</label>"#));
    assert!(!cars.contains(r#"<select id="filter-vehicle_color""#));
    // A small field left without another beside it, after the colours, takes the line itself.
    let features = cars.find(r#"<legend class="filter-label">Opsionet</legend>"#).unwrap();
    assert!(cars[..features].trim_end().ends_with(r#"<fieldset class="filter-group filter-options">"#));

    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains("<h1>Prona<svg class=\"chevron\""));
    assert!(property.contains(r#"value="R21""#));
    assert!(property.contains(r#"<input type="radio" name="type" value="RENT">"#));
    assert!(!property.contains("Gjendja"), "property has no condition");

    // Kinds of electronics are subcategories now, not a filter.
    let electronics = get(pool.clone(), "/c/elektronike").await.body;
    assert!(electronics.contains(r#"<a class="chip" href="/c/laptope">Laptopë</a>"#));
    assert!(!electronics.contains(r#"name="electronics_type""#));
    // A subcategory has its parent's properties: brand and storage for phones.
    let phones = get(pool.clone(), "/c/telefona").await.body;
    assert!(phones.contains("iPhone 13, 128 GB, si i ri"));
    assert!(phones.contains(r#"name="electronics_brand""#));

    // A chosen make narrows the models to its own: the others' wait, hidden, for the make to
    // change in the form (narrow_controller.js).
    let bmw = get(pool.clone(), "/c/makina?vehicle_make=BMW").await.body;
    assert!(bmw.contains(r#"<select id="filter-vehicle_model" name="vehicle_model" data-depends-on="vehicle_make">"#));
    assert!(bmw.contains(r#"<optgroup label="BMW" data-parent="BMW">"#));
    assert!(bmw.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN" hidden>"#));
    // A model of another make filters nothing, nor shows as chosen.
    let mixed = get(pool, "/c/makina?vehicle_make=MERCEDES_BENZ&vehicle_model=VOLKSWAGEN_GOLF").await.body;
    assert!(mixed.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!mixed.contains(r#"<option value="VOLKSWAGEN_GOLF" selected>"#));
}

#[sqlx::test(fixtures("listings"))]
async fn filters_on_properties(pool: PgPool) {
    let golf = "Volkswagen Golf 7, 2016, naftë";
    let mercedes = "Mercedes-Benz C 220, 2018, automatike";

    // SELECT: any of the chosen options.
    let body = get(pool.clone(), "/c/makina?vehicle_transmission=AUTOMATIC").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    // NUMBER: a range, typed the way people type numbers.
    let body = get(pool.clone(), "/c/makina?vehicle_mileage_max=100+000").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    // MULTISELECT: all of the chosen options.
    let body = get(pool.clone(), "/c/makina?vehicle_features=ABS&vehicle_features=NAVIGATION").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    let body = get(pool.clone(), "/c/makina?vehicle_features=ABS").await.body;
    assert!(body.contains(mercedes) && body.contains(golf));
    // The kind of listing.
    let body = get(pool, "/c/prona?type=RENT").await.body;
    assert!(body.contains("Apartament 2+1 me qira në Tiranë"));
    assert!(!body.contains("Shtëpi private në shitje në Vlorë"));
}

#[sqlx::test(fixtures("listings"))]
async fn chosen_filters_can_be_removed_one_by_one(pool: PgPool) {
    let body = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&city=durres&sort=price_desc").await;
    // Each chip links to the page without its own filter, in a fixed parameter order.
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?vehicle_fuel=DIESEL&sort=price_desc""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?city=durres&sort=price_desc""#));
    assert!(body.contains(r#"<a class="clear-filters" href="/c/makina?sort=price_desc">"#));
    // The filters' button says which are on, to screen readers and as its tooltip.
    assert!(body.contains(r#"aria-label="Filtrat: Durrës · Naftë" title="Filtrat: Durrës · Naftë""#));
    // The filter form has no fields for the town and the search words, so it carries them along:
    // filtering keeps them, and the town's chip, or the header's search, changes them.
    let filter_form = |page: &str| {
        let form = &page[page.find(r#"<form id="filter-form""#).unwrap()..];
        form[..form.find("</form>").unwrap()].to_string()
    };
    assert!(!body.contains(r#"id="filter-city""#) && !body.contains(r#"id="filter-q""#));
    assert!(filter_form(&body).contains(r#"<input type="hidden" name="city" value="durres">"#));
    let words = html(pool, "/c/makina?q=golf").await;
    assert!(filter_form(&words).contains(r#"<input type="hidden" name="q" value="golf">"#));
}

#[sqlx::test(fixtures("listings"))]
async fn unknown_filter_values_are_ignored(pool: PgPool) {
    let uri = "/c/makina?vehicle_fuel=NUCLEAR&price_min=abc&sort=sideways&city=atlantis&type=JOB&x=1";
    let response = get(pool, uri).await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("filter-chip"));
}

#[sqlx::test(fixtures("listings"))]
async fn listing_page_shows_price_and_attributes(pool: PgPool) {
    let response = get(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("9\u{a0}500\u{a0}€"));
    // Each detail a pill, its icon before its value; its name for screen readers and as a
    // tooltip. The options, several, a pill each.
    // When it went up, the date in bold, over the photo.
    assert!(response.body.contains("Publikuar më <strong>"));
    // What kind of listing it is, and its condition, over the price, not among the details.
    assert!(response.body.contains(r#"<p class="card-meta listing-kind">Në shitje · E përdorur</p>"#));
    assert!(!response.body.contains(">Gjendja</dt>"));
    assert!(response.body.contains(r#"<dd class="attribute" title="Kilometrazhi"><svg class="attribute-icon""#));
    assert!(response.body.contains(r#"<dt class="visually-hidden">Kilometrazhi</dt>"#));
    assert!(response.body.contains("</svg>148\u{a0}000\u{a0}km</dd>"));
    assert!(response.body.contains("</svg>Naftë</dd>"));
    assert!(response.body.contains("</svg>Hatchback</dd>"));
    assert!(response.body.contains("</svg>ABS</dd>") && response.body.contains("</svg>Kondicioner</dd>"));
    // Its category's path, pills with a › between them: Automjete, then Makina.
    let trail = positions(&response.body, &[
        r#"<a class="chip" href="/c/automjete">Automjete</a>"#,
        r#"<svg class="trail-sep""#,
        r#"<a class="chip" href="/c/makina">Makina</a>"#,
    ]);
    assert!(trail.is_sorted(), "{trail:?}");
    assert_eq!(response.body.matches(r#"<svg class="trail-sep""#).count(), 1);
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=1200"));
}

#[sqlx::test(fixtures("listings"))]
async fn costly_things_on_sale_offer_a_loan_calculator(pool: PgPool) {
    let car = html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await;
    assert!(car.contains(r#"data-controller="loan" data-loan-price-value="21500.00" data-loan-currency-value="€""#));
    assert!(car.contains("Llogarit kredinë"));
    // Under 10 000 € or 1 000 000 Lekë it isn't worth a loan.
    for cheap in ["/listing/1001-volkswagen-golf-7-2016-nafte", "/listing/1005-iphone-13-128-gb-si-i-ri"] {
        assert!(!html(pool.clone(), cheap).await.contains("Llogarit kredinë"), "{cheap}");
    }
    // In Lekë, the limit is about the same as in euros: 500 000 Lekë isn't enough, 1 000 000 is.
    for (price, offered) in [(500_000, false), (999_999, false), (1_000_000, true)] {
        sqlx::query("UPDATE listings SET price_amount = $1::numeric, currency = 'ALL', price_type = 'FIXED' WHERE public_no = 1005")
            .bind(price)
            .execute(&pool)
            .await
            .unwrap();
        let page = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
        assert_eq!(page.contains("Llogarit kredinë"), offered, "{price}");
    }
    // A rent is paid monthly already.
    let rent = html(pool, "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    // The month follows the price, smaller.
    assert!(rent.contains("<p class=\"listing-price\">450\u{a0}€<span class=\"price-note\">/muaj</span></p>"));
    assert!(!rent.contains("Llogarit kredinë"));
}

#[sqlx::test(fixtures("listings"))]
async fn outdated_slug_redirects_to_the_current_one(pool: PgPool) {
    let response = get(pool, "/listing/1001-old-title").await;
    assert_eq!(response.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.location.as_deref(),
        Some("/listing/1001-volkswagen-golf-7-2016-nafte")
    );
}

#[sqlx::test]
async fn unknown_pages_are_404(pool: PgPool) {
    for uri in ["/listing/999999-nothing", "/c/does-not-exist", "/nowhere"] {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(fixtures("listings"))]
async fn next_page_loads_as_a_turbo_frame(pool: PgPool) {
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

    let first = html(pool.clone(), "/").await;
    let (frame_id, src) = next_frame(&first).expect("a next-page frame");
    assert!(src.starts_with("/?after="), "{src}");

    let request = Request::get(&src)
        .header("turbo-frame", &frame_id)
        .body(Body::empty())
        .unwrap();
    let next = send(pool, request).await;
    assert_eq!(next.status, StatusCode::OK);
    assert!(next.body.starts_with(&format!(r#"<turbo-frame id="{frame_id}""#)));
    assert!(!next.body.contains("<html"));
    // 42 listings: 24 on the first page, the remaining 18 here, and no further page.
    assert_eq!(next.body.matches(r#"class="card""#).count(), 18);
    assert!(!next.body.contains("Shfaq më shumë"));
}

#[sqlx::test]
async fn static_files_have_cache_headers(pool: PgPool) {
    let cache_control = |uri: &'static str| {
        let pool = pool.clone();
        async move {
            let response = app(pool)
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            response.headers()[header::CACHE_CONTROL].to_str().unwrap().to_string()
        }
    };
    assert_eq!(
        cache_control("/static/vendor/turbo-8.0.23.js").await,
        "public, max-age=31536000, immutable"
    );
    assert_eq!(cache_control("/static/css/app.css").await, "no-cache");
    assert_eq!(
        cache_control("/static/vendor/leaflet-1.9.4/leaflet.js").await,
        "public, max-age=31536000, immutable"
    );

    for uri in ["/static/vendor/fonts/geist-v5-latin.woff2"] {
        let font = app(pool.clone())
            .oneshot(
                Request::get(uri)
                    .header(header::ACCEPT_ENCODING, "br, gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(font.status(), StatusCode::OK, "{uri}");
        assert_eq!(font.headers()[header::CONTENT_TYPE], "font/woff2");
        assert_eq!(font.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
        assert!(!font.headers().contains_key(header::CONTENT_ENCODING), "WOFF2 is compressed already");
    }
}

#[sqlx::test]
async fn open_pages_reload_in_full_when_the_css_or_javascript_changes(pool: PgPool) {
    let version = marketplace::web::asset_version();
    assert_eq!(version.len(), 8);
    let body = get(pool.clone(), "/").await.body;
    assert!(body.contains(&format!(
        r#"<link rel="stylesheet" href="/static/css/app.css?v={version}" data-turbo-track="reload">"#
    )));
    assert!(body.contains(&format!(
        r#"<script type="module" src="/static/js/application.js?v={version}" data-turbo-track="reload"></script>"#
    )));
    // The fingerprint only changes the URL; the file is served as before.
    let css = get(pool, &format!("/static/css/app.css?v={version}")).await;
    assert_eq!(css.status, StatusCode::OK);
    assert!(css.body.contains(".filters-card"));
}

#[sqlx::test(fixtures("listings"))]
async fn revealing_a_phone_returns_a_turbo_stream_and_counts_it(pool: PgPool) {
    let request = Request::post("/listing/1001/phone").body(Body::empty()).unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.content_type.as_deref(),
        Some("text/vnd.turbo-stream.html; charset=utf-8")
    );
    assert!(response.body.contains(r#"<turbo-stream action="replace" target="phone-reveal">"#));
    assert!(response.body.contains(r#"href="tel:+355690000001"><svg class="contact-icon""#));
    assert!(response.body.contains("</svg>+355 69 000 0001</a>"));
    // Beside it, a button that copies it, as a phone dials it.
    assert!(response.body.contains(r#"data-controller="copy" data-copy-text-value="+355690000001""#));
    assert!(response.body.contains(r#"aria-label="Kopjo numrin""#));

    let reveals: i64 = sqlx::query_scalar("SELECT phone_reveals FROM listing_daily_stats")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reveals, 1);
}

#[sqlx::test(fixtures("listings"))]
async fn every_listing_offers_its_sellers_phone(pool: PgPool) {
    let listings: Vec<(i64, String)> =
        sqlx::query_as("SELECT public_no, slug FROM listings ORDER BY public_no")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(listings.len(), 12);
    for (public_no, slug) in &listings {
        let page = get(pool.clone(), &format!("/listing/{public_no}-{slug}")).await.body;
        let button = format!(r#"<form id="phone-reveal" method="post" action="/listing/{public_no}/phone">"#);
        assert!(page.contains(&button), "listing {public_no} has no phone button");
    }
    // A business's listing gives the business's number.
    let request = Request::post("/listing/1002/phone").body(Body::empty()).unwrap();
    assert!(send(pool.clone(), request).await.body.contains("+355 69 000 0004"));

    // No button where the seller hides the number, or once the listing is sold.
    sqlx::query(
        "UPDATE listing_contacts SET show_phone = false
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1003)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    for uri in ["/listing/1003-apartament-2-1-me-qira-ne-tirane", "/listing/1001-volkswagen-golf-7-2016-nafte"] {
        let page = get(pool.clone(), uri).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(!page.body.contains("phone-reveal"), "{uri} still offers the phone");
    }
    let request = Request::post("/listing/1001/phone").body(Body::empty()).unwrap();
    assert_eq!(send(pool, request).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(fixtures("listings"))]
async fn signing_in_takes_a_code_sent_to_the_phone(pool: PgPool) {
    // Not signed in, "Llogaria" asks for a name and a phone number or an e-mail address.
    let page = get(pool.clone(), "/llogaria").await;
    assert!(page.body.contains("<h1>Krijo një llogari</h1>"));
    assert!(page.body.contains(r#"<form class="account-form" action="/hyr" method="post">"#));

    let sent = post_form(
        pool.clone(),
        "/hyr",
        &[("name", "  Erion   Hoxha "), ("contact", "069 123 4567"), ("started", &started())],
        None,
    )
    .await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER);
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}", sent.cookie("login").unwrap());
    // The code itself isn't stored, only its hash; give it a known value to type in.
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let page = get_with_cookie(pool.clone(), "/hyr/kodi", &login).await;
    assert!(page.body.contains("<strong>+355 69 *** **67</strong>"));

    let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "000000")], Some(&login)).await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(wrong.body.contains("Kodi nuk është i saktë. Të kanë mbetur 4 prova."));

    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123 456")], Some(&login)).await;
    assert_eq!(signed.status, StatusCode::SEE_OTHER);
    assert_eq!(signed.location.as_deref(), Some("/njoftimet-e-mia"));
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert_eq!(signed.cookie("login"), Some(""), "the pending code's cookie is cleared");

    // Signed in: the form for the listing, with the contact buyers will get.
    let form = get_with_cookie(pool.clone(), "/shto", &session).await.body;
    assert!(form.contains("Publiko njoftimin"));
    assert!(form.contains("<strong>+355 69 *** **67</strong>"));
    let (name, status): (String, String) =
        sqlx::query_as("SELECT display_name, status FROM users WHERE phone_e164 = '+355691234567'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), status.as_str()), ("Erion Hoxha", "ACTIVE"));
    // Signed in, "Llogaria" leads to one's own listings.
    let account = get_with_cookie(pool.clone(), "/llogaria", &session).await;
    assert_eq!(account.location.as_deref(), Some("/njoftimet-e-mia"));

    // A used code doesn't work twice, and signing out ends the session.
    let again = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    let out = post_form(pool.clone(), "/dil", &[], Some(&session)).await;
    assert_eq!(out.location.as_deref(), Some("/"));
    assert!(get_with_cookie(pool, "/llogaria", &session).await.body.contains("Krijo një llogari"));
}

#[sqlx::test]
async fn sign_in_codes_are_limited(pool: PgPool) {
    let codes = |pool: PgPool| async move {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM login_codes").fetch_one(&pool).await.unwrap()
    };
    // Bots: the hidden field filled in, or the form sent the moment it was shown.
    let now = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
    let trapped =
        post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("website", "x"), ("started", &started())], None)
            .await;
    let hasty = post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("started", &now)], None).await;
    assert_eq!(trapped.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(hasty.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(codes(pool.clone()).await, 0);

    // Something that is neither a mobile number nor an e-mail address.
    let landline = post_form(pool.clone(), "/hyr", &[("contact", "04 222 3333"), ("started", &started())], None).await;
    assert!(landline.body.contains("Shkruaj një numër celulari"));

    // One code a minute for a contact.
    let first = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    assert_eq!(first.status, StatusCode::SEE_OTHER);
    let second = post_form(pool.clone(), "/hyr", &[("contact", "Ana@Example.com"), ("started", &started())], None).await;
    assert!(second.body.contains("Prit një minutë"));

    // Twenty an hour from one network.
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at)
         SELECT 'n' || g || '@example.com', decode('00', 'hex'), '203.0.113.7', now() + interval '10 minutes'
         FROM generate_series(1, 20) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = Request::post("/hyr")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::from(format!("contact=besa%40example.com&started={}", started())))
        .unwrap();
    assert!(send(pool.clone(), request).await.body.contains("Shumë kërkesa"));

    // Five wrong guesses use a code up.
    sqlx::query("UPDATE login_codes SET created_at = now() - interval '2 minutes' WHERE contact = 'ana@example.com'")
        .execute(&pool)
        .await
        .unwrap();
    let sent = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    for _ in 0..4 {
        let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
        assert!(wrong.body.contains("nuk është i saktë"));
    }
    let last = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
    assert!(last.body.contains("Kërko një kod të ri"));
}

/// A complete listing form, with one field replaced.
fn listing_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
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

#[sqlx::test(fixtures("listings"))]
async fn a_signed_in_seller_posts_a_listing(pool: PgPool) {
    let started = started();
    // Not signed in, a listing needs a phone number or an e-mail address (see
    // a_listing_posted_without_signing_in_goes_live_at_once).
    let anonymous = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), None).await;
    assert_eq!(anonymous.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(anonymous.body.contains("Shkruaj një numër celulari"));

    // The fixture sellers have posted more than five listings today already.
    let ana = new_seller(&pool, "+355691112233").await;
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    assert!(url.ends_with("-samsung-galaxy-s21-128-gb"), "{url}");

    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains("<h1>Samsung Galaxy S21, 128 GB</h1>"));
    assert!(page.contains("32\u{a0}000\u{a0}Lekë"));
    assert!(page.contains(r#"<button type="submit" class="button reveal-button" aria-label="Shfaq numrin" title="Shfaq numrin"><svg class="contact-icon""#));
    let public_no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let reveal = post_form(pool.clone(), &format!("/listing/{public_no}/phone"), &[], None).await;
    assert!(reveal.body.contains("+355 69 111 2233"), "the seller's own number");

    // Published at once, in search, and on the map at Tirana's point.
    assert!(get(pool.clone(), "/search?q=galaxy").await.body.contains("Samsung Galaxy S21"));
    let (status, on_map): (String, bool) = sqlx::query_as(
        "SELECT l.status, l.latitude = loc.latitude FROM listings l JOIN locations loc ON loc.id = l.location_id
         WHERE l.title = 'Samsung Galaxy S21, 128 GB'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status.as_str(), on_map), ("ACTIVE", true));
}

#[sqlx::test(fixtures("listings"))]
async fn listing_forms_are_checked(pool: PgPool) {
    let started = started();
    // The fixture sellers have posted more than five listings today already.
    let ana = new_seller(&pool, "+355691112233").await;
    let post = |field: (&str, &str)| {
        let fields: Vec<(String, String)> =
            listing_fields(field, &started).into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let (pool, ana) = (pool.clone(), ana.clone());
        async move {
            let fields: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            post_form(pool, "/shto", &fields, Some(&ana)).await
        }
    };
    for (field, message) in [
        (("price", ""), "Shkruaj çmimin"),
        (("title", "Tel"), "Titulli duhet të ketë 5 deri në 120 shkronja."),
        (("title", "Shiko www.spam.example"), "Titulli nuk mund të ketë lidhje."),
        (("listing_type", "RENT"), "Zgjidh llojin e njoftimit."),
        (("city", "atlantis"), "Zgjidh qytetin."),
        (("website", "http://bot.example"), "Prit një çast"),
        (("description", "Bli https://a.example https://b.example https://c.example"), "deri në 2 lidhje"),
    ] {
        let response = post(field).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{field:?}");
        assert!(response.body.contains(message), "{field:?}: {message}");
        // What was typed comes back.
        assert!(response.body.contains("Samsung Galaxy") || field.0 == "title", "{field:?}");
    }

    // Free needs no price; the same title twice in a day is refused.
    assert_eq!(post(("price_type", "FREE")).await.status, StatusCode::SEE_OTHER);
    let twice = post(("", "")).await;
    assert!(twice.body.contains("Ke publikuar tashmë një njoftim me këtë titull sot."));

    // Five a day.
    for n in 2..=5 {
        let title = format!("Njoftim prove numër {n}");
        assert_eq!(post(("title", &title)).await.status, StatusCode::SEE_OTHER);
    }
    let sixth = post(("title", "Njoftimi i gjashtë sot")).await;
    assert!(sixth.body.contains("Ke publikuar 5 njoftime sot."));
}

#[sqlx::test]
async fn sellers_who_sign_in_by_email_get_an_email_button(pool: PgPool) {
    let id: String = sqlx::query_scalar(
        "INSERT INTO users (auth_subject, email, display_name)
         VALUES ('email:besa@example.com', 'besa@example.com', 'Besa')
         RETURNING id::text",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let besa = session_for(&pool, &id).await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&besa)).await;
    let url = posted.location.unwrap();
    assert!(get(pool.clone(), &url).await.body.contains(r#"aria-label="Shfaq email-in""#));
    let public_no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let reveal = post_form(pool, &format!("/listing/{public_no}/phone"), &[], None).await;
    assert!(reveal.body.contains(r#"href="mailto:besa@example.com"><svg class="contact-icon""#));
    assert!(reveal.body.contains("</svg>besa@example.com</a>"));
}

#[sqlx::test]
async fn forms_posted_from_other_sites_are_refused(pool: PgPool) {
    let post = |origin: &'static str| {
        let pool = pool.clone();
        async move {
            let request = Request::post("/dil")
                .header(header::HOST, "marketplace.example")
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            send(pool, request).await.status
        }
    };
    assert_eq!(post("https://evil.example").await, StatusCode::FORBIDDEN);
    assert_eq!(post("https://marketplace.example").await, StatusCode::SEE_OTHER);
}

/// Posts the standard test listing as this seller; returns its number.
async fn post_listing(pool: &PgPool, seller: &str) -> String {
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(seller)).await;
    let url = posted.location.expect("posted");
    url.trim_start_matches("/listing/").split('-').next().unwrap().to_string()
}

#[sqlx::test(fixtures("listings"))]
async fn sellers_edit_their_own_listings(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");

    // The seller sees "Ndrysho" on their listing; others don't.
    let edit_link = format!(r#"<a class="button secondary" href="/listing/{no}/ndrysho">Ndrysho</a>"#);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&edit_link));
    assert!(!get(pool.clone(), &url).await.body.contains(&edit_link));

    // The form starts from the listing as it is.
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"value="Samsung Galaxy S21, 128 GB""#));
    assert!(form.contains(r#"value="32000""#));
    assert!(form.contains(r#"<option value="tirane" selected>Tiranë</option>"#));
    assert!(form.contains("Ruaj ndryshimet"));

    // A new title, price and town.
    let started = started();
    let mut fields = listing_fields(("title", "Samsung Galaxy S21, 256 GB"), &started);
    for field in &mut fields {
        match field.0 {
            "price" => field.1 = "29 000",
            "city" => field.1 = "durres",
            _ => {}
        }
    }
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &fields, Some(&ana)).await;
    assert_eq!(saved.location, Some(format!("/listing/{no}-samsung-galaxy-s21-256-gb")));
    let page = get(pool.clone(), saved.location.as_deref().unwrap()).await.body;
    assert!(page.contains("<h1>Samsung Galaxy S21, 256 GB</h1>"));
    assert!(page.contains("29\u{a0}000\u{a0}Lekë"));
    // The old address leads to the new one, and the listing moved to Durrës on the map.
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::PERMANENT_REDIRECT);
    let moved: bool = sqlx::query_scalar(
        "SELECT l.latitude = loc.latitude FROM listings l JOIN locations loc ON loc.slug = 'durres' AND loc.location_type = 'MUNICIPALITY'
         WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(moved);

    // The same checks as for a new listing.
    let bad = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("title", "Tel"), &started), Some(&ana)).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(bad.body.contains("Titulli duhet të ketë 5 deri në 120 shkronja."));

    // "Njoftimet e mia" lists it, with its edit link.
    let mine = get_with_cookie(pool, "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains("Samsung Galaxy S21, 256 GB"));
    assert!(mine.contains(&format!(r#"href="/listing/{no}/ndrysho""#)));
}

#[sqlx::test(fixtures("listings"))]
async fn nobody_else_can_edit_a_listing(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let edit = format!("/listing/{no}/ndrysho");

    assert_eq!(get_with_cookie(pool.clone(), &edit, &besa).await.status, StatusCode::NOT_FOUND);
    let started = started();
    let fields = listing_fields(("title", "Shitet nga dikush tjetër"), &started);
    assert_eq!(post_form(pool.clone(), &edit, &fields, Some(&besa)).await.status, StatusCode::NOT_FOUND);
    // Not signed in: the sign-in comes first, and a post goes back to it.
    assert!(get(pool.clone(), &edit).await.body.contains(r#"action="/hyr""#));
    assert_eq!(post_form(pool.clone(), &edit, &fields, None).await.location.as_deref(), Some("/shto"));

    let title: String = sqlx::query_scalar("SELECT title FROM listings WHERE public_no = $1::bigint")
        .bind(&no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "Samsung Galaxy S21, 128 GB");

    // A sold listing can't be edited any more, even by its seller.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = $1::bigint")
        .bind(&no)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &edit, &ana).await.status, StatusCode::NOT_FOUND);
    let mine = get_with_cookie(pool, "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge">Shitur</span>"#));
    assert!(!mine.contains(&edit));
}

#[sqlx::test(fixtures("listings"))]
async fn parts_have_their_kinds_a_level_down(pool: PgPool) {
    // Automjete lists its four subcategories; the kinds of parts sit under the fourth.
    let vehicles = html(pool.clone(), "/c/automjete").await;
    for (slug, name) in [
        ("makina", "Makina"),
        ("motocikleta", "Motoçikleta"),
        ("automjete-tregtare", "Automjete tregtare"),
        ("pjese-dhe-aksesore", "Pjesë dhe aksesorë"),
    ] {
        assert!(vehicles.contains(&format!(r#"<a class="chip" href="/c/{slug}">{name}</a>"#)), "{slug}");
    }
    assert!(!vehicles.contains(r#"<a class="chip" href="/c/gps-dhe-navigatore">"#));

    // The parts page lists its kinds in order, the spare parts first, under its old address.
    let parts = html(pool.clone(), "/c/pjese-dhe-aksesore").await;
    let kinds = positions(&parts, &[
        r#"<a class="chip" href="/c/pjese-kembimi">Pjesë këmbimi</a>"#,
        r#"<a class="chip" href="/c/goma-dhe-disqe">Goma dhe disqe</a>"#,
        r#"<a class="chip" href="/c/audio-dhe-video">Audio dhe video</a>"#,
        r#"<a class="chip" href="/c/gps-dhe-navigatore">GPS dhe navigatorë</a>"#,
        r#"<a class="chip" href="/c/styling-dhe-pajisje-sportive">Styling dhe pajisje sportive</a>"#,
        r#"<a class="chip" href="/c/tapete-makine">Tapete makine</a>"#,
        r#"<a class="chip" href="/c/rimorkio">Rimorkio</a>"#,
        r#"<a class="chip" href="/c/mbushes-per-makina-elektrike">Mbushës për makina elektrike</a>"#,
        r#"<a class="chip" href="/c/pjese-te-tjera">Të tjera</a>"#,
    ]);
    assert!(kinds.is_sorted(), "{kinds:?}");

    // A kind's page: its two levels above it in the breadcrumbs, and in the menu, inside its
    // parent's branch, inside Automjete's, both open; other categories folded.
    let gps = html(pool.clone(), "/c/gps-dhe-navigatore").await;
    let crumbs = positions(&gps, &[r#"<a href="/c/automjete">Automjete</a>"#, r#"<a href="/c/pjese-dhe-aksesore">Pjesë dhe aksesorë</a>"#]);
    assert!(crumbs.is_sorted());
    let automjete = gps.find(r#"<summary class="picker-option picker-branch"><a href="/c/automjete">Automjete</a>"#).expect("Automjete's branch");
    let parts = gps.find(r#"<summary class="picker-option picker-branch"><a href="/c/pjese-dhe-aksesore">Pjesë dhe aksesorë</a>"#).expect("the parts' branch");
    let here = gps.find(r#"<a class="picker-option is-current" href="/c/gps-dhe-navigatore" aria-current="page">GPS dhe navigatorë</a>"#).expect("the page's own");
    let after = gps[here..].find("</details>").map(|at| here + at).unwrap();
    assert!(automjete < parts && parts < here && gps[parts..after].contains(r#"<a class="picker-option" href="/c/pjese-kembimi">Pjesë këmbimi</a>"#));
    assert!(gps[..automjete].trim_end().ends_with(r#"<details class="menu-branch" open>"#));
    assert!(gps[..parts].trim_end().ends_with(r#"<details class="menu-branch" open>"#));
    let prona = gps.find(r#"<summary class="picker-option picker-branch"><a href="/c/prona">Prona</a>"#).expect("Prona's branch");
    assert!(gps[..prona].trim_end().ends_with(r#"<details class="menu-branch">"#), "folded");
    // Every branch closes; the other two </details> are the category menu's own and the order's.
    assert_eq!(gps.matches("<details class=\"menu-branch\"").count(), gps.matches("</details>").count() - 2, "balanced");

    // The old address still works.
    assert_eq!(get(pool.clone(), "/c/pjese-kembimi").await.status, StatusCode::OK);
    // Every dropdown of categories shows the kinds, a level further in: the search's filter too.
    let search = get(pool, "/search").await.body;
    assert!(search.contains("<option value=\"gps-dhe-navigatore\">\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}GPS dhe navigatorë</option>"));
}

#[sqlx::test(fixtures("listings"))]
async fn the_listing_form_offers_categories_at_every_depth(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    // Vehicles: cars and the like in one group, the kinds of parts in the next, under their path.
    let vehicles = form.find(r#"<optgroup label="Automjete">"#).expect("the vehicles group");
    let parts = form.find(r#"<optgroup label="Automjete › Pjesë dhe aksesorë">"#).expect("the parts group");
    assert!(vehicles < parts);
    let vehicles_group = &form[vehicles..parts];
    assert!(vehicles_group.contains(r#"<option value="makina""#));
    assert!(vehicles_group.contains(r#"<option value="automjete-tregtare""#));
    let parts_group = &form[parts..parts + form[parts..].find("</optgroup>").unwrap()];
    assert_eq!(parts_group.matches("<option value=").count(), 9);
    assert!(parts_group.contains(r#"<option value="gps-dhe-navigatore""#));
    // Categories that only group take no listings, so they aren't offered...
    assert!(!form.contains(r#"<option value="pjese-dhe-aksesore""#));
    assert!(!form.contains(r#"<option value="automjete""#));

    // ...nor taken.
    let started = started();
    let refused = post_form(pool, "/shto", &listing_fields(("category", "pjese-dhe-aksesore"), &started), Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh nënkategorinë"));
}

/// A part posted by this seller, with the vehicle it fits; returns its URL.
async fn post_part(pool: &PgPool, seller: &str, title: &str, fits: &[(&str, &str)]) -> String {
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

#[sqlx::test(fixtures("listings"))]
async fn parts_say_which_vehicle_they_fit(pool: PgPool) {
    // The fields, under the parts' own names for them; none required.
    let fields = get(pool.clone(), "/shto/fushat?category=gps-dhe-navigatore").await.body;
    let labels = positions(&fields, &[
        r#"<label class="visually-hidden" for="prop_vehicle_make">Marka e automjetit</label>"#,
        r#"<option value="">Marka e automjetit</option>"#,
        r#"<label class="visually-hidden" for="prop_vehicle_model">Modeli i automjetit</label>"#,
        r#"<label class="visually-hidden" for="prop_fits_from_year">Nga viti</label>"#,
        r#"placeholder="Nga viti">"#,
        r#"<label class="visually-hidden" for="prop_fits_to_year">Deri në vitin</label>"#,
    ]);
    assert!(labels.is_sorted(), "{labels:?}");
    assert!(!fields.contains(" required"));
    // So nothing says what the asterisk means.
    assert!(!fields.contains("e detyrueshme"));
    // The cars' own lists: models under their make, and "Tjetër".
    assert!(fields.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    assert!(fields.contains(r#"<option value="__other__">Tjetër…</option>"#));
    // Cars keep their own names for the same properties.
    let car_fields = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(car_fields.contains(r#"<label class="visually-hidden" for="prop_vehicle_make">Marka</label>"#));
    assert!(car_fields.contains(r#"<option value="">Marka *</option>"#));
    assert!(car_fields.contains("e detyrueshme"));
    // A category that only groups has no fields to offer.
    assert!(!get(pool.clone(), "/shto/fushat?category=pjese-dhe-aksesore").await.body.contains("prop_"));

    let ana = new_seller(&pool, "+355691112233").await;
    let url = post_part(&pool, &ana, "Navigator origjinal për Golf", &[
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2012"),
        ("prop_fits_to_year", "2019"),
    ])
    .await;

    // The listing says it, under the same names, under its category's path, all three levels.
    let page = get(pool.clone(), &url).await.body;
    let trail = positions(&page, &[
        r#"<a class="chip" href="/c/automjete">Automjete</a>"#,
        r#"<a class="chip" href="/c/pjese-dhe-aksesore">Pjesë dhe aksesorë</a>"#,
        r#"<a class="chip" href="/c/gps-dhe-navigatore">GPS dhe navigatorë</a>"#,
    ]);
    assert!(trail.is_sorted(), "{trail:?}");
    assert_eq!(page.matches(r#"<svg class="trail-sep""#).count(), 2);
    for (label, value) in [("Marka e automjetit", "Volkswagen"), ("Modeli i automjetit", "Golf"), ("Nga viti", "2012"), ("Deri në vitin", "2019")] {
        assert!(page.contains(&format!(">{label}</dt>")), "{label}");
        assert!(page.contains(&format!(">{value}</dd>")), "{value}");
    }
    // The years have no icon: their names show, before them.
    assert!(page.contains(r#"<span class="attribute-label" aria-hidden="true">Nga viti</span>2012</dd>"#));
    // Its card gives the vehicle after the city; a search for the make finds it.
    let path = url.trim_start_matches("http://localhost");
    let parts = get(pool.clone(), "/c/pjese-dhe-aksesore").await.body;
    assert!(parts.contains(&format!(r#"href="{path}">Navigator origjinal për Golf</a>"#)));
    assert!(parts.contains("</svg>Tiranë · Volkswagen · Golf</p>"));
    assert!(get(pool.clone(), "/search?q=volkswagen+navigator").await.body.contains(path));

    // Only the make, or nothing at all: both fine.
    post_part(&pool, &ana, "Navigator për çdo Volkswagen", &[("prop_vehicle_make", "VOLKSWAGEN")]).await;
    post_part(&pool, &ana, "Navigator universal pa marke", &[]).await;

    // The years can't end before they start, nor come after next year.
    let started = started();
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.extend_from_slice(&[("prop_fits_from_year", "2019"), ("prop_fits_to_year", "2012")]);
    let refused = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Duhet të jetë 2019 ose më vonë."), "{}", refused.body);
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.push(("prop_fits_to_year", "2100"));
    let refused = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Duhet të jetë nga 1950 deri në"));
    // A model that isn't the make's is refused, as for cars.
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.extend_from_slice(&[("prop_vehicle_make", "AUDI"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF")]);
    let refused = post_form(pool, "/shto", &fields, Some(&ana)).await;
    assert!(refused.body.contains("Ky model nuk i përket markës së zgjedhur."));
}

#[sqlx::test(fixtures("listings"))]
async fn parts_filter_by_the_buyers_vehicle(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let newer = post_part(&pool, &ana, "GPS Golf 7", &[
        ("prop_vehicle_make", "VOLKSWAGEN"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2012"), ("prop_fits_to_year", "2019"),
    ])
    .await;
    let older = post_part(&pool, &ana, "GPS Golf 5", &[
        ("prop_vehicle_make", "VOLKSWAGEN"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2004"), ("prop_fits_to_year", "2008"),
    ])
    .await;
    let from_only = post_part(&pool, &ana, "GPS Mercedes C", &[
        ("prop_vehicle_make", "MERCEDES_BENZ"), ("prop_fits_from_year", "2014"),
    ])
    .await;
    let any = post_part(&pool, &ana, "GPS universal", &[]).await;
    let found = |html: &str| -> Vec<bool> { [&newer, &older, &from_only, &any].iter().map(|url| html.contains(url.as_str())).collect() };

    // The parts page offers the vehicle as filters, the years as one: the buyer's vehicle's.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore").await;
    assert!(page.contains(r#"<label class="filter-label" for="filter-vehicle_make">Marka e automjetit</label>"#));
    assert!(page.contains(r#"<option value="AUDI">Audi</option>"#));
    assert!(page.contains(r#"<label class="filter-label" for="filter-fits_year">Viti i automjetit</label>"#));
    assert!(page.contains(r#"name="fits_year" value="""#));
    assert!(!page.contains("fits_from_year") && !page.contains("fits_to_year_min"));
    assert_eq!(found(&page), [true, true, true, true]);

    // By make: the Golfs.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN").await;
    assert_eq!(found(&page), [true, true, false, false]);
    // And year: the one whose years include it.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    assert!(page.contains(r#"name="fits_year" value="2015""#));
    assert!(page.contains(r#"<a class="filter-chip" href="/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN" aria-label="Hiq filtrin Viti i automjetit: 2015">"#));
    // A kind of part narrows the page, so its links keep the filters, in the chips and the menu;
    // links that widen it or go elsewhere keep only the words, the town and the map pin.
    let kept = "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015";
    assert!(page.contains(&format!(r#"<a class="chip" href="{kept}">GPS dhe navigatorë</a>"#)));
    assert!(page.contains(&format!(r#"<a class="picker-option" href="{kept}">GPS dhe navigatorë</a>"#)));
    assert!(page.contains(r#"<a class="picker-option" href="/c/makina">Makina</a>"#));
    assert!(page.contains(r#"<a href="/c/automjete">Automjete</a>"#));
    let page = html(pool.clone(), "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    // A page that doesn't take a filter leaves it out of its links too: Automjete has no make.
    let page = html(pool.clone(), "/c/automjete?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert!(page.contains(r#"<a class="chip" href="/c/pjese-dhe-aksesore">Pjesë dhe aksesorë</a>"#));
    // The years' ends count: 2012 and 2019 are in, 2011 and 2020 are not.
    for (year, fits) in [(2011, false), (2012, true), (2019, true), (2020, false)] {
        let page = html(pool.clone(), &format!("/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN&fits_year={year}")).await;
        assert_eq!(found(&page)[0], fits, "{year}");
    }
    // Year alone: what may fit stays, the one only from 2014 and the one that doesn't say.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?fits_year=2016").await;
    assert_eq!(found(&page), [true, false, true, true]);
    // The same on a kind's own page, and on search with the category.
    let page = html(pool.clone(), "/c/gps-dhe-navigatore?fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    let page = html(pool.clone(), "/search?category=pjese-dhe-aksesore&fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    // A year that can't be one is ignored.
    for nonsense in ["abc", "1800", "20155"] {
        let page = html(pool.clone(), &format!("/c/pjese-dhe-aksesore?fits_year={nonsense}")).await;
        assert_eq!(found(&page), [true, true, true, true], "{nonsense}");
    }
    // Cars don't get it: their year is a range, as before.
    let cars = html(pool, "/c/makina").await;
    assert!(!cars.contains("fits_year"));
    assert!(cars.contains(r#"name="vehicle_year_min""#));
}

#[sqlx::test(fixtures("listings"))]
async fn listings_get_photos(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let fields = listing_fields(("", ""), &started);
    let posted = post_with_photos(pool.clone(), "/shto", &fields, &[jpeg(3000, 2000), jpeg(800, 600)], &ana).await;
    let url = posted.location.expect("posted");
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap().to_string();

    // The listing shows both, the first as its cover, and serves them: the full one 1600 px wide.
    let page = get(pool.clone(), &url).await.body;
    let photo_urls: Vec<&str> = page.match_indices(r#"<img src="/media/photos/"#).map(|(at, _)| {
        let start = at + r#"<img src=""#.len();
        &page[start..start + page[start..].find('"').unwrap()]
    }).collect();
    assert_eq!(photo_urls.len(), 2, "{page}");
    assert!(page.contains(r#"width="1600" height="1067""#));
    // Each whole in its frame, over a blurred copy of itself: its small copy, a decoration.
    for url in &photo_urls {
        let backdrop = format!(r#"<img class="gallery-backdrop" src="{}" alt="">"#, url.replace(".jpg", "-card.jpg"));
        let at = page.find(&backdrop).expect("the blurred copy") + backdrop.len();
        assert!(page[at..].trim_start().starts_with(&format!(r#"<img src="{url}""#)));
    }
    // Two make a strip to swipe through, one at a time, with a counter and arrows.
    assert!(page.contains(r#"<div class="gallery" data-controller="gallery">"#));
    assert!(page.contains(r#"data-gallery-target="track" tabindex="0" role="region" aria-label="Fotot e njoftimit""#));
    assert!(page.contains(r#"<span class="gallery-count" data-gallery-target="count">1 / 2</span>"#));
    assert!(page.contains(r#"aria-label="Foto e mëparshme" disabled"#) && page.contains(r#"aria-label="Foto tjetër""#));
    assert!(page.contains(", foto 2 nga 2\""));
    let photo = get(pool.clone(), photo_urls[0]).await;
    assert_eq!(photo.status, StatusCode::OK);
    assert_eq!(photo.content_type.as_deref(), Some("image/jpeg"));

    // The card uses the small copy of the cover.
    let cards = get(pool.clone(), "/search?q=Galaxy").await.body;
    assert!(cards.contains(&photo_urls[0].replace(".jpg", "-card.jpg")));

    // Editing: the photos are on the form; one removed, and the other becomes the cover.
    let edit = format!("/listing/{no}/ndrysho");
    let form = get_with_cookie(pool.clone(), &edit, &ana).await.body;
    let first_id = {
        let at = form.find(r#"name="remove_photo" value=""#).expect("a photo to remove") + r#"name="remove_photo" value=""#.len();
        form[at..at + 36].to_string()
    };
    let mut fields = listing_fields(("", ""), &started);
    fields.push(("remove_photo", &first_id));
    let saved = post_with_photos(pool.clone(), &edit, &fields, &[], &ana).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    let page = get(pool.clone(), &url).await.body;
    assert_eq!(page.matches(r#"<img src="/media/photos/"#).count(), 1);
    // One is just the photo: nothing to swipe or count.
    assert!(page.contains(r#"<div class="gallery">"#));
    assert!(!page.contains("data-controller=\"gallery\"") && !page.contains("gallery-count") && !page.contains(", foto 1 nga"));
    assert_eq!(get(pool.clone(), photo_urls[0]).await.status, StatusCode::NOT_FOUND);
    let covers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.public_no = $1::bigint AND m.is_cover",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(covers, 1);

    // What isn't a photo is refused, and too many are too.
    let refused = post_with_photos(pool.clone(), &edit, &listing_fields(("", ""), &started), &[b"not a photo".to_vec()], &ana).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Një nga fotot nuk u lexua"));
    let many: Vec<Vec<u8>> = (0..8).map(|_| jpeg(40, 30)).collect();
    let refused = post_with_photos(pool.clone(), &edit, &listing_fields(("", ""), &started), &many, &ana).await;
    assert!(refused.body.contains("deri në 8 foto"));
    // Nor are other paths in the store served.
    assert_eq!(get(pool, "/media/../.env").await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(fixtures("listings"))]
async fn sellers_pause_resume_and_delete_their_listings(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");
    let (pause, resume, delete) =
        (format!("/listing/{no}/caktivizo"), format!("/listing/{no}/aktivizo"), format!("/listing/{no}/fshi"));
    let searched = |pool: PgPool| async move { get(pool, "/search?q=Galaxy").await.body };

    // The seller has the buttons on their listing; others don't.
    let pause_form = format!(r#"action="{pause}""#);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&pause_form));
    assert!(!get(pool.clone(), &url).await.body.contains(&pause_form));

    // Nobody else can pause or delete it.
    for action in [&pause, &delete] {
        assert_eq!(post_form(pool.clone(), action, &[], Some(&besa)).await.status, StatusCode::NOT_FOUND);
    }

    // Paused: gone from the site, still on "Njoftimet e mia", with a way back.
    let paused = post_form(pool.clone(), &pause, &[], Some(&ana)).await;
    assert_eq!(paused.location.as_deref(), Some("/njoftimet-e-mia"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    assert!(!searched(pool.clone()).await.contains(&url));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge">Çaktivizuar</span>"#));
    assert!(mine.contains(&format!(r#"action="{resume}""#)));
    // Pausing it again changes nothing.
    assert_eq!(post_form(pool.clone(), &pause, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);

    // Back up.
    post_form(pool.clone(), &resume, &[], Some(&ana)).await;
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);
    assert!(searched(pool.clone()).await.contains(&url));

    // Deleted: gone everywhere, for good, and the history says who did it.
    let deleted = post_form(pool.clone(), &delete, &[], Some(&ana)).await;
    assert_eq!(deleted.location.as_deref(), Some("/njoftimet-e-mia"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(&url));
    assert_eq!(post_form(pool.clone(), &resume, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    let by_seller: bool = sqlx::query_scalar(
        "SELECT h.changed_by_user_id = l.created_by_user_id
         FROM listing_status_history h JOIN listings l ON l.id = h.listing_id
         WHERE l.public_no = $1::bigint AND h.to_status = 'DELETED'",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(by_seller);
}

#[sqlx::test(fixtures("listings"))]
async fn listings_go_into_subcategories(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    // The form groups subcategories under their category.
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    assert!(form.contains(r#"<optgroup label="Elektronikë">"#));
    assert!(form.contains(r#"<option value="telefona" data-types="SELL,BUY" data-currency="ALL" data-condition>Telefona</option>"#));
    assert!(!form.contains(r#"<option value="elektronike""#), "a category with subcategories isn't offered");

    // Posting into the category itself is refused.
    let started = started();
    let refused = post_form(pool.clone(), "/shto", &listing_fields(("category", "elektronike"), &started), Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh nënkategorinë"));

    // A phone posted in Telefona shows under Elektronikë and Telefona, and "telefon" finds it.
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/elektronike").await.body.contains("Samsung Galaxy S21"));
    assert!(get(pool.clone(), "/c/telefona").await.body.contains("Samsung Galaxy S21"));
    assert!(!get(pool.clone(), "/c/laptope").await.body.contains("Samsung Galaxy S21"));
    assert!(get(pool.clone(), "/search?q=telefon").await.body.contains("Samsung Galaxy S21"));

    // Moved to Laptopë, its kind follows.
    let url = posted.location.unwrap();
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let moved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("category", "laptope"), &started), Some(&ana)).await;
    assert_eq!(moved.status, StatusCode::SEE_OTHER);
    // Neither its title nor its description says "laptop"; its kind does.
    assert!(get(pool, "/search?q=laptop").await.body.contains("Samsung Galaxy S21"));
}

/// A car for sale, with its properties, and one field replaced (or added).
fn car_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
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

#[sqlx::test(fixtures("listings"))]
async fn a_car_is_posted_with_its_make_model_and_year(pool: PgPool) {
    // The fields come with the category: the frame the form reloads when it changes.
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(frame.starts_with(r#"<turbo-frame id="listing-properties">"#));
    assert!(frame.contains(r#"<input id="prop_vehicle_year" name="prop_vehicle_year" value="" inputmode="decimal" placeholder="Viti *" required>"#));
    // Its main fields with their icons, two to a line: make, model, year, fuel, gearbox, body,
    // mileage and colour; then its options.
    assert!(frame.contains(r#"<fieldset class="property-fields vehicle-fields">"#));
    // The box is named inside it, not in its border: for screen readers by its legend, out of
    // sight, for the eye by a header.
    assert!(frame.contains(r#"<legend class="visually-hidden">Detajet</legend>"#));
    assert!(frame.contains(r#"<div class="property-heading" aria-hidden="true">Detajet</div>"#));
    let order = positions(&frame, &[
        r#"for="prop_vehicle_make""#,
        r#"for="prop_vehicle_model""#,
        r#"for="prop_vehicle_year""#,
        r#"for="prop_vehicle_fuel""#,
        r#"for="prop_vehicle_transmission""#,
        r#"for="prop_vehicle_body_type""#,
        r#"for="prop_vehicle_mileage""#,
        r#"for="prop_vehicle_color""#,
        r#"<span class="field-label">Opsionet</span>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert_eq!(frame.matches(r#"<svg class="tile-icon""#).count(), 8);
    assert!(frame.contains(r#"name="prop_vehicle_mileage" value="" inputmode="decimal" placeholder="Kilometrazhi (km)">"#));
    // A part's make and model are fields like the rest.
    assert!(!get(pool.clone(), "/shto/fushat?category=gps-dhe-navigatore").await.body.contains("tile"));
    assert!(frame.contains(r#"<select id="prop_vehicle_model" name="prop_vehicle_model" required data-depends-on="prop_vehicle_make">"#));
    assert!(frame.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    assert!(frame.contains(r#"name="prop_vehicle_features" value="ABS""#));
    // Phones have a brand and storage, not their kind: Telefona says that.
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    assert!(phones.contains(r#"name="prop_electronics_brand""#) && !phones.contains("prop_electronics_type"));
    // A category with subcategories has no fields of its own to offer.
    assert!(!get(pool.clone(), "/shto/fushat?category=automjete").await.body.contains("prop_"));

    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &car_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let page = get(pool.clone(), &url).await.body;
    for detail in ["Volkswagen", "Golf", "2017", "120\u{a0}000\u{a0}km", "Naftë", "Manuale", "ABS"] {
        assert!(page.contains(detail), "{detail}");
    }
    // On its card, in the filters and in search, like the listings already there.
    let card = "</svg>Tiranë · 2017 · 120\u{a0}000\u{a0}km · Naftë</p>";
    assert!(get(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&vehicle_year_min=2017").await.body.contains(card));
    assert!(get(pool.clone(), "/c/makina?vehicle_features=NAVIGATION").await.body.contains(&url));
    assert!(!get(pool.clone(), "/c/makina?vehicle_fuel=PETROL").await.body.contains(&url));

    // The edit form starts from them, and an edit replaces them.
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"<option value="VOLKSWAGEN_GOLF" selected>Golf</option>"#));
    assert!(form.contains(r#"name="prop_vehicle_mileage" value="120000""#));
    assert!(form.contains(r#"value="NAVIGATION" checked"#));
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &car_fields(("prop_vehicle_fuel", "HYBRID"), &started), Some(&ana)).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/makina?vehicle_fuel=HYBRID").await.body.contains(&url));
    assert!(!get(pool, "/c/makina?vehicle_fuel=DIESEL").await.body.contains(&url));
}

#[sqlx::test(fixtures("listings"))]
async fn a_cars_title_is_made_from_what_it_is(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = car_fields(("prop_vehicle_body_type", "SEDAN"), &started);
    fields.push(("prop_vehicle_color", "BLACK"));
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();

    // The page's title is made from its make, model, body type and colour, and kept with it.
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains("<h1>Volkswagen Golf Sedan, e zezë</h1>"), "{page}");
    let generated: Option<String> = sqlx::query_scalar("SELECT generated_title FROM listings WHERE public_no = $1")
        .bind(no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(generated.as_deref(), Some("Volkswagen Golf Sedan, e zezë"));
    // The seller's own heads the description; the details leave out what the title says.
    assert!(page.contains(r#"<h3 class="description-heading">Volkswagen Golf 7 1.6 TDI, 2017</h3>"#));
    for said in ["Marka", "Modeli", "Karroceria", "Ngjyra"] {
        assert!(!page.contains(&format!(">{said}</dt>")), "{said}");
    }
    assert!(page.contains(r#"<dt class="visually-hidden">Viti</dt>"#));
    // Its card says it too.
    let card = format!(r#"href="{url}">Volkswagen Golf Sedan, e zezë</a>"#);
    assert!(get(pool.clone(), "/c/makina").await.body.contains(&card));

    // An edit makes it again; "Tjetër" for a colour says nothing.
    let mut fields = car_fields(("prop_vehicle_body_type", "SEDAN"), &started);
    fields.push(("prop_vehicle_color", "OTHER"));
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &fields, Some(&ana)).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), &url).await.body.contains("<h1>Volkswagen Golf Sedan</h1>"));
    // The edit form keeps the seller's own.
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"value="Volkswagen Golf 7 1.6 TDI, 2017""#));

    // A part's make and model are the vehicle it fits: its own title stays.
    let part = post_part(&pool, &ana, "Navigator origjinal për Golf", &[
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
    ])
    .await;
    let page = get(pool, &part).await.body;
    assert!(page.contains("<h1>Navigator origjinal për Golf</h1>") && !page.contains("description-heading"));
}

#[sqlx::test(fixtures("listings"))]
async fn car_properties_are_checked(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    for (field, message) in [
        (("prop_vehicle_make", ""), "Plotëso „Marka“."),
        (("prop_vehicle_year", ""), "Plotëso „Viti“."),
        (("prop_vehicle_year", "1850"), "Duhet të jetë nga 1900 deri në"),
        (("prop_vehicle_year", "3000"), "Duhet të jetë nga 1900 deri në"),
        (("prop_vehicle_mileage", "-5"), "Duhet të jetë të paktën 0."),
        (("prop_vehicle_mileage", "shumë"), "Shkruaj një numër."),
        (("prop_vehicle_model", "BMW_3_SERIES"), "Ky model nuk i përket markës së zgjedhur."),
        (("prop_vehicle_fuel", "STEAM"), "Zgjidh një nga mundësitë."),
    ] {
        let response = post_form(pool.clone(), "/shto", &car_fields(field, &started), Some(&ana)).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{field:?}");
        assert!(response.body.contains(message), "{field:?}: {message}");
        // What was typed comes back, in the car's fields.
        assert!(response.body.contains(r#"name="prop_vehicle_transmission""#), "{field:?}");
    }
}

/// A car whose make and model the seller typed, not in the lists.
fn typed_car<'a>(make: &'a str, model: &'a str, started: &'a str) -> Vec<(&'a str, &'a str)> {
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

#[sqlx::test(fixtures("listings"))]
async fn sellers_type_a_make_or_model_the_list_lacks(pool: PgPool) {
    // The make and model lists end with "Tjetër", which opens a box to type in.
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(frame.contains(r#"<option value="__other__">Tjetër…</option>"#));
    assert!(frame.contains(r#"name="prop_vehicle_make_other""#) && frame.contains(r#"name="prop_vehicle_model_other""#));
    assert!(!frame.contains(r#"name="prop_vehicle_fuel_other""#), "only some properties take a typed option");

    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &typed_car("Zastava", "Yugo", &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let page = get(pool.clone(), &url).await.body;
    // They make its title, as a listed make and model would.
    assert!(page.contains("<h1>Zastava Yugo</h1>"), "{page}");
    // Search finds it by them; the lists and filters don't offer them until someone approves them.
    assert!(get(pool.clone(), "/search?q=yugo").await.body.contains(&url));
    assert!(!get(pool.clone(), "/shto/fushat?category=makina").await.body.contains(">Zastava<"));
    assert!(!get(pool.clone(), "/c/makina").await.body.contains(">Zastava<"));
    let (active, model_under_make): (bool, bool) = sqlx::query_as(
        "SELECT make.active, model.parent_option_id = make.id
         FROM attribute_options make, attribute_options model
         WHERE make.label = 'Zastava' AND model.label = 'Yugo'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((active, model_under_make), (false, true));

    // The edit form shows them as typed.
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"<option value="__other__" selected>Tjetër…</option>"#));
    assert!(form.contains(r#"name="prop_vehicle_make_other" value="Zastava""#));

    // Typed again, by someone else and in other letters, it is the same make; and "bmw" is the
    // listed BMW, not a new one.
    let besa = new_seller(&pool, "+355691112244").await;
    let again = post_form(pool.clone(), "/shto", &typed_car("  ZASTAVA ", "yugo", &started), Some(&besa)).await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.body);
    let zastavas: i64 = sqlx::query_scalar("SELECT count(*) FROM attribute_options WHERE lower(label) IN ('zastava', 'yugo')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(zastavas, 2, "one Zastava, one Yugo");
    let mut bmw = typed_car("bmw", "Seria 1", &started);
    bmw.iter_mut().find(|field| field.0 == "title").unwrap().1 = "BMW 118d, 2015";
    assert_eq!(post_form(pool.clone(), "/shto", &bmw, Some(&besa)).await.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/makina?vehicle_make=BMW").await.body.contains(">BMW Seria 1</a>"));
    // A listed model typed without its punctuation ("id buzz") is the listed one too, ID. Buzz: it
    // comes to the same code.
    let mut buzz = typed_car("volkswagen", "id buzz", &started);
    buzz.iter_mut().find(|field| field.0 == "title").unwrap().1 = "Volkswagen ID. Buzz, 2023";
    let posted = post_form(pool.clone(), "/shto", &buzz, Some(&besa)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    assert!(get(pool.clone(), &posted.location.unwrap()).await.body.contains("<h1>Volkswagen ID. Buzz</h1>"));
    let typed: i64 = sqlx::query_scalar("SELECT count(*) FROM attribute_options WHERE lower(label) = 'id buzz'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(typed, 0);

    // Checked like everything else.
    for ((make, model), message) in [
        (("", "Yugo"), "Shkruaj „Marka“."),
        (("www.zastava.al", "Yugo"), "Shkruaj vetëm emrin"),
        (("!!", "Yugo"), "Shkruaj vetëm emrin"),
    ] {
        let response = post_form(pool.clone(), "/shto", &typed_car(make, model, &started), Some(&ana)).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{make}");
        assert!(response.body.contains(message), "{make}: {message}");
    }
    // A listed model doesn't go with a typed make.
    let mut golf = typed_car("Zastava", "", &started);
    golf.iter_mut().find(|field| field.0 == "prop_vehicle_model").unwrap().1 = "VOLKSWAGEN_GOLF";
    let response = post_form(pool, "/shto", &golf, Some(&ana)).await;
    assert!(response.body.contains("Ky model nuk i përket markës së zgjedhur."));
}

/// The id of the make or model named so.
async fn option_id(pool: &PgPool, label: &str) -> String {
    sqlx::query_scalar("SELECT id::text FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Whether the option named so is offered, approved.
async fn option_active(pool: &PgPool, label: &str) -> Option<bool> {
    sqlx::query_scalar("SELECT active FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(fixtures("listings"))]
async fn moderators_approve_or_merge_what_sellers_typed(pool: PgPool) {
    let started = started();
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let moderator = new_seller(&pool, MODERATOR).await;
    // Typed in: a make the list lacks, with its model; a listed make spelt short, with its model;
    // and a model of a listed make, spelt long.
    let post = |seller: String, make: &'static str, model: &'static str, title: &'static str| {
        let pool = pool.clone();
        let started = started.clone();
        async move {
            let mut fields = typed_car(make, model, &started);
            fields.iter_mut().find(|field| field.0 == "title").unwrap().1 = title;
            let posted = post_form(pool, "/shto", &fields, Some(&seller)).await;
            assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
            posted.location.unwrap()
        }
    };
    let yugo = post(ana.clone(), "Zastava", "Yugo", "Zastava Yugo 45, 1988").await;
    let mercedes = post(besa.clone(), "Mercedes", "C 220", "Mercedes C 220, 2012").await;
    let golf = post(ana.clone(), "volkswagen", "Golf IV", "VW Golf IV 1.9 TDI").await;
    let zastava_id = option_id(&pool, "Zastava").await;
    let yugo_id = option_id(&pool, "Yugo").await;
    let golf_iv_id = option_id(&pool, "Golf IV").await;
    let mercedes_id = option_id(&pool, "Mercedes").await;

    // Only for moderators: anyone else finds nothing, and changes nothing.
    assert_eq!(get(pool.clone(), "/moderim").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), "/moderim", &ana).await.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", "Zastava")], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &option_id(&pool, "Golf").await)], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    assert_eq!(option_active(&pool, "Zastava").await, Some(false));
    assert_eq!(option_active(&pool, "Golf IV").await, Some(false));
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains("/moderim"));

    // The moderator's own page links to it, with how many wait.
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains(r#"<a class="button secondary small" href="/moderim">Për miratim (5)</a>"#));
    // Each, with its make, how many listings have it, who typed it; the likeliest listed one
    // comes chosen for a merge; a model whose make waits waits for it.
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await;
    assert_eq!(page.status, StatusCode::OK);
    let page = page.body;
    assert!(page.contains(r#"<strong class="pending-label">Zastava</strong>"#));
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Zastava</span>"#));
    assert!(page.contains("Mirato ose bashko më parë markën, „Zastava“."));
    assert!(page.contains(r#"<a href="/search?q=Golf+IV">1 njoftim</a> · shtuar nga Përdorues"#));
    let golf_id = option_id(&pool, "Golf").await;
    assert!(page.contains(&format!(r#"<option value="{golf_id}" selected>Golf</option>"#)));
    let mercedes_benz_id = option_id(&pool, "Mercedes-Benz").await;
    assert!(page.contains(&format!(r#"<option value="{mercedes_benz_id}" selected>Mercedes-Benz</option>"#)));
    // The model of a make that waits can't be approved yet.
    let early = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Yugo")], Some(&moderator)).await;
    assert_eq!(early.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Yugo").await, Some(false));

    // Approved, the make is offered, in its alphabetical place: after Volvo.
    let approved = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", " Zastava ")], Some(&moderator)).await;
    assert_eq!((approved.status, approved.location.as_deref()), (StatusCode::SEE_OTHER, Some("/moderim")));
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    let places = positions(&frame, &[">Volkswagen<", ">Volvo<", ">Zastava<"]);
    assert!(places.is_sorted(), "{places:?}");
    // Its model, with its name put right: "Koral", which search finds it by now too.
    let approved = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Koral")], Some(&moderator)).await;
    assert_eq!(approved.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Koral").await, Some(true));
    assert!(get(pool.clone(), &yugo).await.body.contains("<h1>Zastava Koral</h1>"));
    assert!(get(pool.clone(), "/search?q=koral").await.body.contains(&yugo));
    assert!(get(pool.clone(), "/c/makina").await.body.contains(">Zastava<"));

    // A name the list has already, or one that can't be a name, is not approved.
    for (label, message) in [("golf", "„golf“ është tashmë në listë: bashkoje me të."), ("x", "Shkruaj vetëm emrin, me 2 deri në 40 shkronja.")] {
        let refused = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/mirato"), &[("label", label)], Some(&moderator)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{label}");
        assert!(refused.body.contains(message), "{label}");
        assert_eq!(option_active(&pool, "Golf IV").await, Some(false), "{label}");
    }

    // Merged, the listing has the listed model, and the typed one is gone.
    let merged = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Golf IV").await, None);
    let page = get(pool.clone(), &golf).await.body;
    assert!(page.contains("<h1>Volkswagen Golf</h1>") && !page.contains(">Golf IV<"));
    // A typed make merged into the listed one takes its typed model along, under the listed make.
    let merged = post_form(pool.clone(), &format!("/moderim/{mercedes_id}/bashko"), &[("into", &mercedes_benz_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), &mercedes).await.body.contains("<h1>Mercedes-Benz C 220</h1>"));
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Mercedes-Benz</span>"#));
    assert!(page.contains(r#"<strong class="pending-label">C 220</strong>"#));
    assert!(!page.contains("më parë markën"));
    // No listed model is like it, so the merge asks which, and won't go without one.
    assert!(page.contains(r#"<option value="" selected>Bashkoje me…</option>"#));
    let c220_id = option_id(&pool, "C 220").await;
    let unchosen = post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", "")], Some(&moderator)).await;
    assert_eq!(unchosen.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(unchosen.body.contains("Zgjidh me cilin emër të listës ta bashkosh."));
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    // Not into a model of another make.
    post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains("Për miratim (1)"));
}

#[sqlx::test]
async fn the_makes_and_models_have_the_codes_typing_them_gives(pool: PgPool) {
    // As the posting form codes a typed option (properties.rs), a model's after its make's.
    let code = |name: &str| marketplace::format::slug(name).to_uppercase().replace('-', "_");
    let options: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT o.label, o.value_code, make.value_code
         FROM attribute_options o
         JOIN attributes a ON a.id = o.attribute_id
         LEFT JOIN attribute_options make ON make.id = o.parent_option_id
         WHERE a.code IN ('vehicle_make', 'vehicle_model') AND o.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(options.len() > 500, "{}", options.len());
    for (label, value_code, make) in options {
        let expected = match make {
            Some(make) => format!("{make}_{}", code(&label)),
            None => code(&label),
        };
        assert_eq!(value_code, expected, "{label}");
    }
}

#[sqlx::test(fixtures("listings"))]
async fn every_select_is_chosen_from_a_list_in_the_page(pool: PgPool) {
    // Each select comes inside the picker (picker_controller.js), which keeps it for the form and
    // offers its options in a list in the page instead of the phone's own picker; a category's
    // list folds, its subcategories under it.
    let plain = r#"<div class="picker" data-controller="picker">"#;
    let folding = r#"<div class="picker" data-controller="picker" data-picker-tree-value="true">"#;
    let ana = new_seller(&pool, "+355691112233").await;
    let pages = [
        ("/shto", 6),
        ("/shto/fushat?category=makina", 6),
        ("/search?q=golf", 3),
        ("/c/makina", 4),
        ("/harta", 2),
    ];
    for (uri, selects) in pages {
        let page = get_with_cookie(pool.clone(), uri, &ana).await.body;
        let found: Vec<usize> = page.match_indices("<select").map(|(at, _)| at).collect();
        assert_eq!(found.len(), selects, "{uri}");
        for at in found {
            let tag = &page[at..at + page[at..].find('>').unwrap()];
            let category = tag.contains(r#"name="category""#);
            let wrapper = if category { folding } else { plain };
            let start = page[..at].rfind(wrapper).unwrap_or_else(|| panic!("{uri}: {tag} has no picker"));
            assert!(page[start + wrapper.len()..at].trim().is_empty(), "{uri}: {tag} isn't its picker's");
        }
    }
}

#[sqlx::test(fixtures("listings"))]
async fn a_listing_posted_without_signing_in_goes_live_at_once(pool: PgPool) {
    // Not signed in, the form asks for a phone number or an e-mail address as well, and no code.
    let form = get(pool.clone(), "/shto").await.body;
    assert!(form.contains(r#"<input id="contact" name="contact" value="" required"#));
    assert!(form.contains("njoftimi publikohet menjëherë"));
    assert!(form.contains("llogaria krijohet vetë"));
    assert!(!form.contains("Të dërgojmë një kod") && !form.contains("të dërgojmë kodin"));

    let started = started();
    let mut fields = listing_fields(("name", "Dorina"), &started);
    fields.push(("contact", "069 777 8899"));
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.clone().unwrap();
    assert!(url.ends_with("-samsung-galaxy-s21-128-gb"), "{url}");
    // Posting signs no one in: that takes a code, so typing someone's number opens nothing.
    assert!(posted.cookie("session").is_none());

    // Live at once, with its seller: a new account, active, named as typed.
    assert!(get(pool.clone(), "/search?q=galaxy").await.body.contains("Samsung Galaxy S21"));
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains(r#"<span class="seller-name">Dorina</span>"#));
    let (name, status): (String, String) =
        sqlx::query_as("SELECT display_name, status FROM users WHERE phone_e164 = '+355697778899'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), status.as_str()), ("Dorina", "ACTIVE"));

    // Its page says so, once, to whoever posted it, and how to change it later.
    let notice = "Njoftimi yt u publikua. Për ta ndryshuar ose fshirë më vonë";
    assert!(!page.contains(notice));
    let posted_cookie = format!("posted={}", posted.cookie("posted").unwrap());
    let shown = get_with_cookie(pool.clone(), &url, &posted_cookie).await;
    assert!(shown.body.contains(notice));
    assert_eq!(shown.cookie("posted"), Some(""), "said once");

    // Changing it takes signing in, with a code sent to that number.
    let sent = post_form(pool.clone(), "/hyr", &[("contact", "069 777 8899"), ("started", &started)], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))")
        .execute(&pool)
        .await
        .unwrap();
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &session).await.body.contains("Samsung Galaxy S21"));

    // An e-mail address does as well; with no name, the account is "Përdorues".
    let mut fields = listing_fields(("name", ""), &started);
    fields.extend([("title", "Laptop Lenovo ThinkPad T14"), ("contact", "Ana.Re@Example.com")]);
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE email = 'ana.re@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Përdorues");

    // A number with an account already: the listing goes under it, which keeps its name.
    let mut fields = listing_fields(("name", "Dikush tjetër"), &started);
    fields.extend([("title", "Telefon tjetër për shitje"), ("contact", "+355 69 000 0003")]);
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    assert!(posted.cookie("session").is_none());
    let owner: String = sqlx::query_scalar(
        "SELECT u.display_name FROM listings l JOIN users u ON u.id = l.created_by_user_id
         WHERE l.title = 'Telefon tjetër për shitje' AND l.status = 'ACTIVE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, "Dritan");

    // Not under a suspended account, nor without a contact that can be one.
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE phone_e164 = '+355690000003'")
        .execute(&pool)
        .await
        .unwrap();
    for (contact, message) in [("069 000 0003", "Kjo llogari është pezulluar."), ("04 222 3333", "Shkruaj një numër celulari")] {
        let mut fields = listing_fields(("title", "Edhe një telefon"), &started);
        fields.push(("contact", contact));
        let refused = post_form(pool.clone(), "/shto", &fields, None).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{contact}");
        assert!(refused.body.contains(message), "{contact}");
    }
}

#[sqlx::test(fixtures("listings"))]
async fn posting_without_signing_in_is_limited_per_network(pool: PgPool) {
    // Twenty an hour from one network (IP address), each under a new number; the next waits.
    let started = started();
    for i in 0..21 {
        let (title, phone) = (format!("Njoftim nga i njëjti rrjet {i}"), format!("069 700 {i:04}"));
        let mut fields = listing_fields(("title", &title), &started);
        fields.push(("contact", &phone));
        let response = post_form_from(pool.clone(), "/shto", &fields, "203.0.113.9").await;
        if i < 20 {
            assert_eq!(response.status, StatusCode::SEE_OTHER, "{i}: {}", response.body);
        } else {
            assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(response.body.contains("Shumë njoftime nga ky rrjet brenda një ore."));
        }
    }
    // Another network posts as before, and so does someone signed in on the first.
    let mut fields = listing_fields(("title", "Nga një rrjet tjetër"), &started);
    fields.push(("contact", "069 701 0000"));
    assert_eq!(post_form_from(pool.clone(), "/shto", &fields, "203.0.113.10").await.status, StatusCode::SEE_OTHER);
    let ana = new_seller(&pool, "+355691112233").await;
    let fields = listing_fields(("title", "E Anës, e hyrë në llogari"), &started);
    let signed_in = send(
        pool.clone(),
        Request::post("/shto")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("x-forwarded-for", "203.0.113.9")
            .header(header::COOKIE, ana.as_str())
            .body(Body::from(form_urlencoded::Serializer::new(String::new()).extend_pairs(&fields).finish()))
            .unwrap(),
    )
    .await;
    assert_eq!(signed_in.status, StatusCode::SEE_OTHER, "{}", signed_in.body);
}

#[sqlx::test(fixtures("listings"))]
async fn every_listing_shows_its_seller_and_their_profile_lists_their_listings(pool: PgPool) {
    // A private seller: Ana, with the five fixture listings she sells.
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let ana = "/perdorues/11111111-1111-4111-8111-111111111111";
    // The category above the photo; the title and the price under it; then her initial, her
    // name, since when and how many listings, leading to her profile; then the buttons, the
    // share one a link in a circle; then the details.
    let order = positions(&page, &[
        r#"<nav class="listing-trail""#,
        r#"<div class="gallery">"#,
        r#"<p class="listing-meta">"#,
        "<h1>Volkswagen Golf 7, 2016, naftë</h1>",
        r#"<p class="card-meta listing-kind">Në shitje · E përdorur</p>"#,
        r#"<p class="listing-price">"#,
        r#"<aside class="listing-contact" aria-label="Shitësi">"#,
        &format!(r#"<a class="seller" href="{ana}">"#),
        r#"<span class="seller-name">Ana</span>"#,
        r#"<span class="seller-summary">Në Marketplace që nga "#,
        r#"<div class="actions">"#,
        r#"aria-label="Shpërndaje" title="Shpërndaje">"#,
        "</aside>",
        "<h2>Detajet</h2>",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(page.contains(" · 5 njoftime</span>"));

    // Her profile says so too.
    let profile = get(pool.clone(), ana).await.body;
    assert!(profile.contains("<h1>Ana</h1>"));
    assert!(profile.contains("Në Marketplace që nga ") && profile.contains(" · 5 njoftime</p>"));
    assert_eq!(card_numbers(&profile).len(), 5);
    assert!(profile.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!profile.contains("Mercedes-Benz C 220"), "Auto Durrësi's, not Ana's");

    // A business: its listings, on its own page.
    let page = get(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.body;
    assert!(page.contains(r#"<a class="seller" href="/biznes/auto-durresi">"#));
    let business = get(pool.clone(), "/biznes/auto-durresi").await.body;
    assert!(business.contains("<h1>Auto Durrësi</h1>") && business.contains("Biznes · "));
    assert_eq!(card_numbers(&business), vec![1002]);

    // No such seller, or one not yet confirmed.
    assert_eq!(get(pool.clone(), "/perdorues/not-a-uuid").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(pool.clone(), "/biznes/nobody").await.status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE users SET status = 'PENDING' WHERE display_name = 'Ana'").execute(&pool).await.unwrap();
    assert_eq!(get(pool, ana).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(fixtures("listings"))]
async fn a_category_page_heading_switches_category(pool: PgPool) {
    let page = html(pool.clone(), "/c/prona?city=durres&q=shtepi").await;
    assert!(page.contains(r#"<details class="category-switch" data-controller="dropdown">"#));
    // Every category and subcategory, keeping the town and the words; the current one marked.
    assert!(page.contains(r#"<a class="picker-option" href="/c/makina?q=shtepi&city=durres">Makina</a>"#));
    assert!(page.contains(r#"<a class="picker-option" href="/c/apartamente?q=shtepi&city=durres">Apartamente</a>"#));
    assert!(page.contains(r#"<summary class="picker-option picker-branch is-current"><a href="/c/prona?q=shtepi&city=durres" aria-current="page">Prona</a>"#));
    assert!(page.contains(r#"<a class="picker-option" href="/search?q=shtepi&city=durres">Të gjitha njoftimet</a>"#));
    // A search page's heading is just a heading.
    assert!(!get(pool, "/search?q=golf").await.body.contains("category-switch"));
}

#[sqlx::test(fixtures("listings"))]
async fn a_profile_searches_its_sellers_listings(pool: PgPool) {
    let ana = "/perdorues/11111111-1111-4111-8111-111111111111";
    let profile = get(pool.clone(), ana).await.body;
    assert!(profile.contains(&format!(r#"<form class="profile-search" action="{ana}" method="get" role="search">"#)));

    // Only Ana's: the golf is hers, the Mercedes (also a car) is Auto Durrësi's.
    let found = get(pool.clone(), &format!("{ana}?q=golf")).await.body;
    assert_eq!(card_numbers(&found), vec![1001]);
    assert!(found.contains("1 njoftim për „golf“"));
    assert!(found.contains(r#"value="golf""#));
    // Misspelt, it still finds the golf.
    assert_eq!(card_numbers(&get(pool.clone(), &format!("{ana}?q=golff")).await.body), vec![1001]);
    let none = get(pool, &format!("{ana}?q=mercedes")).await.body;
    assert!(card_numbers(&none).is_empty() && none.contains("0 njoftime për „mercedes“"));
}
