//! What a reader keeps (web/saved.rs, searches.rs): a listing kept by its heart, tapped before
//! signing in too; a saved search, the listings new to it and its alerts; and the price drops those
//! who keep a listing hear of.

mod common;

use axum::{body::Body, http::{Request, StatusCode, header}};
use common::*;
use serde_json::Value;
use sqlx::PgPool;

/// The heart over listing `number`'s photo.
fn heart(number: i64) -> String {
    format!(r#"<form class="heart" method="post" action="/listing/{number}/ruaj" data-controller="heart" data-heart-number-value="{number}""#)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_listing_has_a_heart_over_its_photo(pool: PgPool) {
    // On every card, the newest listings' row's too, and on the listing's page; every page's rail
    // leads to the listings kept, and the menu does (the welcome page's round button in the row
    // with its search, on phones, went on 2026-10-11).
    let home = get(pool.clone(), "/").await.body;
    assert_eq!(home.matches(r#"<form class="heart""#).count(), home.matches(r#"<article class="card"#).count());
    assert!(home.contains(r#"<a class="menu-row" href="/te-ruajturat">"#) && !home.contains("saved-shortcut"));
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(cars.contains(r#"<a class="rail-item rail-saved" href="/te-ruajturat" title="Të ruajturat">"#));
    assert!(cars.contains(&heart(1001)));
    assert_eq!(cars.matches(r#"<form class="heart""#).count(), cars.matches(r#"<article class="card"#).count());
    let listing = get(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    assert!(listing.contains(&heart(1001)));
    assert!(listing.contains(r#"<button type="submit" class="heart-button" aria-pressed="false" aria-label="Ruaje""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_heart_keeps_a_listing_for_a_signed_in_user(pool: PgPool) {
    let session = new_seller(&pool, "+355690000011").await;
    let kept = |pool: PgPool| {
        let session = session.clone();
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
            let mut numbers: Vec<i64> = serde_json::from_value(answer["numbers"].clone()).unwrap();
            numbers.sort();
            numbers
        }
    };
    for number in ["1001", "1001", "1005"] {
        assert_eq!(send(pool.clone(), tap_heart(number, "1", Some(&session))).await.status, StatusCode::NO_CONTENT);
    }
    assert_eq!(kept(pool.clone()).await, [1001, 1005]);
    // Found again on their page, the last kept first.
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &session).await.body;
    assert!(page.contains("<h1>Të ruajturat</h1>"));
    assert_eq!(card_numbers(&page), [1005, 1001]);
    // Let go.
    assert_eq!(send(pool.clone(), tap_heart("1005", "0", Some(&session))).await.status, StatusCode::NO_CONTENT);
    assert_eq!(kept(pool.clone()).await, [1001]);
    assert_eq!(card_numbers(&get_with_cookie(pool.clone(), "/te-ruajturat", &session).await.body), [1001]);
    // No such listing.
    assert_eq!(send(pool.clone(), tap_heart("999999", "1", Some(&session))).await.status, StatusCode::NOT_FOUND);
    // Without JavaScript, the form keeps the listing and goes back to its page.
    let request = Request::post("/listing/1003/ruaj")
        .header(header::HOST, "localhost")
        .header(header::REFERER, "http://localhost/c/automjete")
        .header(header::COOKIE, &session)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("ruaj=1"))
        .unwrap();
    assert_eq!(send(pool.clone(), request).await.location.as_deref(), Some("/c/automjete"));
    assert_eq!(kept(pool.clone()).await, [1001, 1003]);
    // Nobody signed in keeps nothing, and their page says how to.
    assert_eq!(get(pool.clone(), "/te-ruajturat/numrat").await.body, r#"{"numbers":[],"price_drops":0,"searches":0}"#);
    let page = get(pool, "/te-ruajturat").await.body;
    assert!(page.contains(r#"<a class="button" href="/llogaria">Hyr</a>"#) && card_numbers(&page).is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_heart_tapped_before_signing_in_keeps_the_listing_once_signed_in(pool: PgPool) {
    // Not signed in: the heart says so, and the listing and the page wait in cookies.
    let tapped = send(pool.clone(), tap_heart("1001", "1", None)).await;
    assert_eq!(tapped.status, StatusCode::UNAUTHORIZED);
    let waiting = format!("ruaj={}; kthehu={}", tapped.cookie("ruaj").unwrap(), tapped.cookie("kthehu").unwrap());
    assert_eq!(waiting, "ruaj=1001; kthehu=/c/automjete?vehicle_fuel=DIESEL");

    let sent = post_form(pool.clone(), "/hyr", &[("contact", "069 123 4567"), ("started", &started())], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&format!("{login}; {waiting}"))).await;
    // Signed in: back to the page, with the listing kept, and the cookies gone.
    assert_eq!(signed.location.as_deref(), Some("/c/automjete?vehicle_fuel=DIESEL"));
    assert_eq!((signed.cookie("ruaj"), signed.cookie("kthehu")), (Some(""), Some("")));
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert_eq!(get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body, r#"{"numbers":[1001],"price_drops":0,"searches":0}"#);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_saved_search_counts_and_tells_of_its_new_listings(pool: PgPool) {
    let besa = new_seller(&pool, "+355691112244").await;
    let ana = new_seller(&pool, "+355691112233").await;
    let search = "/c/telefona?city=tirane";
    let alerts = || marketplace::web::send_search_alerts(&pool, "https://pluribus.example");
    let unseen = |pool: PgPool, session: String| async move {
        let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
        answer["searches"].as_i64().unwrap()
    };

    // The page offers to keep it, as its filters write it; not signed in, keeping it signs in first
    // and comes back to it.
    let page = get(pool.clone(), search).await.body;
    assert!(page.contains(r#"<form class="save-search" method="post" action="/kerkimet/ruaj">"#), "{page}");
    assert!(page.contains(&format!(r#"<input type="hidden" name="adresa" value="{search}">"#)));
    let signed_out = post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", search)], None).await;
    assert_eq!(signed_out.location.as_deref(), Some("/llogaria"));
    assert_eq!(signed_out.cookie("kthehu").map(|value| value.split(';').next().unwrap()), Some(search));
    // Only a category's or a search's page can be kept.
    assert_eq!(post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/mesazhet")], Some(&besa)).await.status, StatusCode::NOT_FOUND);

    // Kept: back on the page, which says so, once however often it is kept.
    for _ in 0..2 {
        let kept = post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", search)], Some(&besa)).await;
        assert_eq!(kept.location.as_deref(), Some(search));
    }
    let id: String = sqlx::query_scalar("SELECT id::text FROM saved_searches").fetch_one(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), search, &besa).await.body;
    assert!(page.contains(&format!(r#"<form class="save-search" method="post" action="/kerkimet/{id}/fshi">"#)), "{page}");
    assert!(page.contains(r#"</svg><span class="save-search-label">Kërkimi u ruajt</span></button>"#));
    // Its page names it by its category and filters, with nothing new yet.
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(&format!(r#"<a class="saved-search-name" href="/kerkimet/{id}">Telefona · Tiranë</a>"#)), "{list}");
    assert!(!list.contains(r#"<span class="badge state">"#));

    // A phone put up in Tiranë since: one new listing, and, once the alerts' gap has passed since
    // it was kept, an alert: a notification, the heart's dot, a word by the link on /te-ruajturat.
    post_listing(&pool, &ana).await;
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(r#"<span class="badge state">1 njoftim i ri</span>"#), "{list}");
    assert_eq!(alerts().await.unwrap(), 0);
    // Four hours pass, for the search and the listings alike.
    sqlx::query("UPDATE saved_searches SET created_at = created_at - interval '4 hours'").execute(&pool).await.unwrap();
    sqlx::query("UPDATE listings SET published_at = published_at - interval '4 hours', bumped_at = bumped_at - interval '4 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(alerts().await.unwrap(), 1);
    let (title, url): (String, String) = sqlx::query_as(
        "SELECT title, payload ->> 'url' FROM notifications WHERE notification_type = 'SAVED_SEARCH'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(title, "1 njoftim i ri: Telefona · Tiranë");
    assert_eq!(url, format!("https://pluribus.example/kerkimet/{id}"));
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 1);
    assert!(get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body.contains("Kërkimet e ruajtura · 1 me njoftime të reja"));
    // Not again within the gap.
    assert_eq!(alerts().await.unwrap(), 0);
    // Its page, once shown, says so, and the alert is seen; another alert would bring the dot back.
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(r#"<section class="saved-searches" data-seen-url="/kerkimet/pare">"#), "{list}");
    assert_eq!(post_form(pool.clone(), "/kerkimet/pare", &[], Some(&besa)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 0);
    assert!(!get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("data-seen-url"));

    // Opened, from its page or an alert: to its page, its new listings counted from now, its alert
    // seen. Nobody else opens or lets it go.
    assert_eq!(get_with_cookie(pool.clone(), &format!("/kerkimet/{id}"), &ana).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), &format!("/kerkimet/{id}/fshi"), &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    let opened = get_with_cookie(pool.clone(), &format!("/kerkimet/{id}"), &besa).await;
    assert_eq!(opened.location.as_deref(), Some(search));
    assert!(!get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains(r#"<span class="badge state">"#));
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 0);

    // Its alerts off, none, however much comes; on again from its page.
    let off = post_form(pool.clone(), &format!("/kerkimet/{id}/lajmerime"), &[], Some(&besa)).await;
    assert_eq!(off.location.as_deref(), Some("/kerkimet-e-ruajtura"));
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Lajmëromë</button>"));
    sqlx::query("UPDATE saved_searches SET last_alerted_at = now() - interval '4 hours', last_seen_at = now() - interval '4 hours'").execute(&pool).await.unwrap();
    assert_eq!(alerts().await.unwrap(), 0);
    post_form(pool.clone(), &format!("/kerkimet/{id}/lajmerime"), &[("lajmero", "1")], Some(&besa)).await;
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Ndalo lajmërimet</button>"));

    // Let go: the page offers to keep it again.
    let gone = post_form(pool.clone(), &format!("/kerkimet/{id}/fshi"), &[], Some(&besa)).await;
    assert_eq!(gone.location.as_deref(), Some("/kerkimet-e-ruajtura"));
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Ende s'ke ruajtur asnjë kërkim."));
    assert!(get_with_cookie(pool.clone(), search, &besa).await.body.contains(r#"</svg><span class="save-search-label">Ruaj kërkimin</span></button>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn those_who_keep_a_listing_hear_when_its_price_drops(pool: PgPool) {
    // Ana's phone, at 32 000 Lekë, kept by Besnik, and by Dritan, who turned price drop alerts off.
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    let dritan = session_for(&pool, "33333333-3333-4333-8333-333333333333").await;
    for keeper in [&besnik, &dritan] {
        assert_eq!(send(pool.clone(), tap_heart(&no, "1", Some(keeper))).await.status, StatusCode::NO_CONTENT);
    }
    sqlx::query("INSERT INTO notification_preferences (user_id, price_drop_alerts) VALUES ('33333333-3333-4333-8333-333333333333', false)")
        .execute(&pool)
        .await
        .unwrap();
    // How many drops the header's heart says someone hasn't seen (heart_controller.js).
    let unseen = |session: &str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
            answer["price_drops"].as_i64().unwrap()
        }
    };
    let started = started();
    let edit = |field: (&'static str, &'static str)| {
        let (pool, ana, started, no) = (pool.clone(), ana.clone(), started.clone(), no.clone());
        async move {
            let saved = post_form(pool, &format!("/listing/{no}/ndrysho"), &listing_fields(field, &started), Some(&ana)).await;
            assert_eq!(saved.status, StatusCode::SEE_OTHER);
        }
    };
    // Shown, the page says so (application.js) at the address it names: up to its latest drop.
    let saw = |page: &str, session: &str| {
        let start = page.find(r#"data-seen-url=""#).expect("no data-seen-url") + 15;
        let url = page[start..start + page[start..].find('"').unwrap()].to_string();
        let pool = pool.clone();
        let request = Request::post(url).header(header::COOKIE, session).body(Body::empty()).unwrap();
        async move { assert_eq!(send(pool, request).await.status, StatusCode::NO_CONTENT) }
    };

    // An edit that leaves the price is nothing to hear.
    edit(("title", "Samsung Galaxy S21, 128 GB, me kuti")).await;
    assert_eq!(unseen(&besnik).await, 0);
    // Lowered to 29 000 Lekë: Besnik hears, Dritan doesn't, nor its seller.
    edit(("price", "29 000")).await;
    assert_eq!((unseen(&besnik).await, unseen(&dritan).await, unseen(&ana).await), (1, 0, 0));
    // His page says so over the cards, from what to what, and asks to be told when it is shown.
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body;
    assert!(page.contains(r#"<div class="price-drops" data-seen-url="/te-ruajturat/pare?deri="#), "{page}");
    let drop = format!(
        "<li><a href=\"/listing/{no}-samsung-galaxy-s21-128-gb\">Samsung Galaxy S21, 128 GB</a>: nga <del>32\u{a0}000\u{a0}Lekë</del> në <strong>29\u{a0}000\u{a0}Lekë</strong></li>"
    );
    assert!(page.contains(&drop), "{page}");
    assert!(!get_with_cookie(pool.clone(), "/te-ruajturat", &dritan).await.body.contains("price-drops"));
    // Fetched, the page changes nothing; shown, the drop is seen.
    assert_eq!(unseen(&besnik).await, 1);
    saw(&page, &besnik).await;
    assert_eq!(unseen(&besnik).await, 0);
    assert!(!get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body.contains("price-drops"));

    // Lowered twice more before he looks again: one notice, from what it was when he last saw it.
    edit(("price", "27 000")).await;
    edit(("price", "25 000")).await;
    assert_eq!(unseen(&besnik).await, 1);
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body;
    assert!(page.contains("nga <del>29\u{a0}000\u{a0}Lekë</del> në <strong>25\u{a0}000\u{a0}Lekë</strong></li>"), "{page}");
    // A drop after the page was made isn't seen with it, as when the browser shows its copy again.
    edit(("price", "24 000")).await;
    saw(&page, &besnik).await;
    assert_eq!(unseen(&besnik).await, 1);
    // Let go, the listing's drops are none of his business.
    assert_eq!(send(pool.clone(), tap_heart(&no, "0", Some(&besnik))).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unseen(&besnik).await, 0);
}
