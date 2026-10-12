//! The pages that list listings (web/browse.rs): the welcome page, its categories, the listings
//! seen lately, the most seen and those recommended; a category's page, its way up and its
//! subcategories; a seller's profile; the cards, small, big or in a list; and the results' count,
//! order and pages, which follow on as the reader scrolls.

mod common;

use axum::{body::Body, http::{Request, StatusCode, header}};
use common::*;
use http_body_util::BodyExt;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn pages_that_find_nothing_offer_ways_on(pool: PgPool) {
    // Words that find nothing: how to search for more, and a wanted ad for what no one sells yet.
    let page = get(pool.clone(), "/search?q=xyzzyqqq").await.body;
    assert!(page.contains("Nuk u gjet asnjë njoftim.") && page.contains("Kontrollo si i ke shkruar fjalët"), "{page}");
    assert!(page.contains(r#"<a class="button secondary" href="/shto"><svg"#) && page.contains("</svg>Shto një njoftim „Kërkoj të blej“</a>"));
    // An empty category, with no words searched, says neither.
    let empty = get(pool.clone(), "/c/aparate-fotografike").await.body;
    assert!(empty.contains("Nuk u gjet asnjë njoftim.") && !empty.contains("Kontrollo si i ke shkruar") && !empty.contains("Shto një njoftim „Kërkoj të blej“"));
    // An address with nothing: why that may be, a search in every category, and the ways home and
    // to posting, in the site's frame.
    let missing = get(pool.clone(), "/kjo-faqe-nuk-ekziston").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.body.contains("<h1>Faqja nuk u gjet</h1>"));
    assert!(missing.body.contains(r#"<form class="search not-found-search" action="/search" method="get" role="search">"#));
    // The way home grey, posting filled after its plus, as the canvas has them.
    assert!(missing.body.contains(r#"<a class="button secondary" href="/">Faqja kryesore</a>"#), "{}", missing.body);
    assert!(missing.body.contains(r#"<a class="button" href="/shto"><svg"#) && missing.body.contains("</svg>Shto një njoftim</a>"), "{}", missing.body);
    // A request that fails: the site's frame too, with what to do and the way home, never the error.
    use axum::response::IntoResponse;
    let response = pluribus_web::AppError::from(std::io::Error::other("the database went away")).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<h1>Diçka shkoi keq</h1>") && body.contains(r#"<header class="site-header">"#), "{body}");
    assert!(body.contains(r#"<a href="/kontakt">Kontakt</a>"#) && !body.contains("database"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn home_page_lists_categories(pool: PgPool) {
    let response = get(pool, "/").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Automjete"));
    assert!(response.body.contains("Nuk u gjet asnjë njoftim."));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn welcome_page_asks_for_words_and_offers_the_categories(pool: PgPool) {
    let body = get(pool.clone(), "/").await.body;
    // The question is for screen readers alone, with no line under it; the creature that was the
    // site's icon is on no page (it was the logo's too, and once stood behind the search).
    assert!(body.contains(r#"<h1 class="visually-hidden">Çfarë po kërkon?</h1>"#));
    assert!(!body.contains("Shkruaj atë që të duhet"));
    assert!(!body.contains(r#"<svg class="thinking""#));
    // Words only: no city, no category; the results' filters narrow them. Suggestions open under
    // the words as they are typed.
    assert!(body.contains(r#"<form class="welcome-form" action="/search" method="get" role="search" data-controller="suggest""#));
    assert!(body.contains(r#"<input id="welcome-q" type="search" name="q""#));
    assert!(body.contains(r#"<div class="suggestions" id="welcome-suggestions" role="listbox""#));
    // While the box has words, a cross empties it (suggest_controller.js shows it).
    assert!(body.contains(r#"<button type="button" class="search-clear" aria-label="Pastro" title="Pastro" data-suggest-target="clear" data-action="suggest#clear" hidden>"#));
    // The search button is a magnifier at the end of the field's pill, in the accent; the field
    // asks the page's question.
    assert!(body.contains(r#"<button type="submit" class="icon-button welcome-search" aria-label="Kërko""#));
    assert!(body.contains(r#"placeholder="Çfarë po kërkon?""#));
    // The search is alone in a row of its own over the categories; no button opens the search,
    // which stands open. "Posto", after it on phones until 2026-10-11, is in the bar, its plus just
    // before the account (the user's ask: "put the plus icon near the account one"), named for
    // screen readers as the rail's is; the rail has it from 720px up (base.css). The listings kept
    // and the conversations, round buttons after "Posto" once, went at the user's word (the menu
    // has them); the theme is the menu's too.
    let nav = body.find(r#"<nav class="shortcuts tiles" data-controller="drag-row" aria-label="Kategoritë">"#).unwrap();
    let start = body.find(r#"<div class="welcome-tools">"#).unwrap();
    let row = &body[start..nav];
    assert!(row.contains(r#"<form class="welcome-form""#) && !row.contains("/shto"), "{row}");
    assert!(!body.contains("welcome-post"));
    let bar = &body[body.find(r#"<header class="site-header">"#).unwrap()..body.find("</header>").unwrap()];
    let end = positions(bar, &[
        r#"<div class="header-end">"#,
        r#"<a class="icon-button header-post" href="/shto" aria-label="Posto njoftim" title="Posto njoftim">"#,
        r#"<svg data-icon="plus""#,
        r#"<a class="icon-button account-button" href="/llogaria""#,
    ]);
    assert!(end.is_sorted(), "{end:?}");
    assert!(!body.contains("circle-shortcut") && !body.contains("saved-shortcut") && !body.contains("inbox-shortcut"), "{body}");
    assert!(!body.contains("theme-shortcut"));
    assert!(!body.contains("search-shortcut") && !body.contains("welcome-search-panel") && !body.contains("welcome-scrim"));
    // Then every category's pill straight in the list, no group round any of them, so that they
    // wrap wherever the line ends: Automjete, Prona, Elektronikë and Veshje first, then
    // every other category in its order.
    let list = &body[nav..];
    let list = &list[..list.find("</nav>").unwrap()];
    assert!(!list.contains("<div") && !body.contains("welcome-main"), "{list}");
    let order = positions(list, &[
        r#"<a class="shortcut" href="/c/automjete">"#,
        r#"<a class="shortcut" href="/c/prona">"#,
        r#"<a class="shortcut" href="/c/elektronike">"#,
        r#"<a class="shortcut" href="/c/veshje-dhe-aksesore">"#,
        r#"<a class="shortcut" href="/c/shtepi-dhe-kopsht">"#,
        r#"<a class="shortcut" href="/c/sport-dhe-hobi">"#,
        r#"<a class="shortcut" href="/c/kafshe-shtepiake">"#,
        r#"<a class="shortcut" href="/c/te-tjera">"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!list.contains("circle-shortcut"), "{list}");
    // Every page's top bar has the account alone at its right end, as the user asked on 2026-10-11,
    // the welcome page's too: the text's size and the theme are the menu's, posting, the listings
    // kept and the conversations the rail's (on phones the menu's, and posting the welcome page's
    // bar's too, its plus just before the account). (The search's magnifier before it is a
    // listing's alone, hidden elsewhere: a_listing_folds_the_bars_search_behind_a_magnifier.)
    let cars = get(pool.clone(), "/c/automjete").await.body;
    for (page, posting) in [(&body, true), (&cars, false)] {
        assert!(!page.contains("post-button"));
        // The typeface panel ("Ff") stands there only while TYPE_TRIAL is on (.env).
        let trial = std::env::var("TYPE_TRIAL").is_ok_and(|value| value == "true");
        assert_eq!(page.contains(r#"<details class="type-menu" data-controller="dropdown type-trial""#), trial);
        let bar = &page[page.find(r#"<header class="site-header">"#).unwrap()..page.find("</header>").unwrap()];
        assert_eq!(bar.contains(r#"<a class="icon-button header-post" href="/shto""#), posting, "{bar}");
        assert_eq!(bar.matches("icon-button").count(), 2 + usize::from(trial) + usize::from(posting), "{bar}");
        assert!(bar.contains(r#"<button type="button" class="icon-button search-button""#), "{bar}");
        assert!(bar.contains(r#"<a class="icon-button account-button" href="/llogaria""#));
        for gone in ["theme-toggle", "text-size-menu", "saved-button", "inbox-button"] {
            assert!(!bar.contains(gone), "{gone} in {bar}");
        }
    }
    assert!(!body.contains("saved-button") && !body.contains("inbox-button"));
    assert!(!cars.contains("circle-shortcut"));
    assert!(!body.contains("data-panel-target") && !body.contains("<noscript>"));
    assert!(!body.contains(r#"name="city""#) && !body.contains(r#"name="category""#));
    // A shortcut to every main category: Automjete, Prona, Elektronikë and Veshje
    // first, then the others in their order, each its icon before its name; no more listing-like
    // cards.
    let main = [
        ("automjete", "Automjete"),
        ("prona", "Prona"),
        ("elektronike", "Elektronikë"),
        ("veshje-dhe-aksesore", "Veshje"),
        ("shtepi-dhe-kopsht", "Shtëpi"),
        ("pune", "Punë"),
        ("sherbime", "Shërbime"),
        ("sport-dhe-hobi", "Sport"),
        ("kafshe-shtepiake", "Kafshë"),
        ("makineri-dhe-biznes", "Makineri"),
        ("te-tjera", "Të tjera"),
    ];
    let links: Vec<String> =
        main.iter().map(|(slug, _)| format!(r#"<a class="shortcut" href="/c/{slug}">"#)).collect();
    let shortcuts = positions(&body, &links.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(shortcuts.is_sorted(), "{shortcuts:?}");
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), main.len());
    for (_, name) in main {
        assert!(body.contains(&format!(r#"</svg></span>
      <span class="shortcut-name">{name}</span>"#)), "{name}");
    }
    // Each with its icon.
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), body.matches(r#"<svg class="shortcut-icon""#).count());
    assert!(!body.contains(r#"class="tile""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_welcome_page_shows_the_listings_seen_lately(pool: PgPool) {
    // None seen, no such row.
    assert!(!html(pool.clone(), "/").await.contains("Shikuar së fundmi"));
    // A listing's page gives its number, which application.js puts first in the cookie as the page
    // shows; its seller's own page gives none.
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    assert!(html(pool.clone(), golf).await.contains(r#" data-listing-number="1001""#));
    let ana = session_for(&pool, ANA_ID).await;
    assert!(!get_with_cookie(pool.clone(), golf, &ana).await.body.contains("data-listing-number"));
    // The cookie's listings, the latest first and each once, those still on the site: a paused
    // one, a number no listing has and what isn't a number are left out.
    sqlx::query("UPDATE listings SET status = 'PAUSED' WHERE public_no = 1002").execute(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), "/", "shikuar=1005.1002.999999.x.1001.1005").await.body;
    let row = section(&page, "seen-heading");
    assert_eq!(card_numbers(row), [1005, 1001]);
    assert!(row.contains(r#"<h2 id="seen-heading">Shikuar së fundmi</h2>"#) && row.contains(r#"data-action="recent#forget">Pastro</button>"#), "{row}");
    // Over the newest listings (fewer than four like those seen are on the site to recommend:
    // the laptop alone, like the phone, and the Mercedes, like the Golf, paused), its cards with
    // ids of their own, the first photo the page's lead.
    assert!(page.find(r#"id="seen-heading""#) < page.find(r#"id="latest-heading""#));
    assert!(row.contains(r#"id="seen-1005""#) && row.contains(r#"fetchpriority="high""#), "{row}");
    // A sold one stays, saying so over its photo.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1005").execute(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), "/", "shikuar=1005.1001").await.body;
    let row = section(&page, "seen-heading");
    assert_eq!(card_numbers(row), [1005, 1001]);
    assert!(row.contains(r#"<p class="card-status is-sold">Shitur</p>"#), "{row}");
    // Smaller than the other rows' cards (the user's ask of 2026-10-11): about six to the screen,
    // six and a quarter on the page's column and three and a half on a phone (five and a quarter
    // until the user asked them a bit bigger there, later that day, then four and a quarter until
    // they asked once more), the title alone under the photo on two lines at most, no heart; on a
    // phone no price over the photo either.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", asset_version())).await.body;
    for rule in [
        ".welcome ~ .seen-lately .card-row > .card{width:calc((100% - 6 * var(--space-12)) / 6.25);}",
        ".welcome ~ .seen-lately .card :is(.card-summary,.heart){display:none;}",
        "-webkit-line-clamp:2;line-clamp:2;}",
        ".welcome ~ .seen-lately .card-row > .card{width:calc((100% + var(--gutter) - 3 * var(--space-8)) / 3.5);}",
        ".welcome ~ .seen-lately .card-media::after,.welcome ~ .seen-lately .card-media .card-price{display:none;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn bumped_listing_comes_first(pool: PgPool) {
    let body = get(pool, "/").await.body;
    let house = body.find("Shtëpi private në shitje në Vlorë").unwrap();
    let phone = body.find("iPhone 13, 128 GB, si i ri").unwrap();
    assert!(house < phone, "the house was bumped after the phone was published");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_welcome_page_goes_from_its_categories_to_the_newest_listings(pool: PgPool) {
    let body = html(pool.clone(), "/").await;
    // Under the categories, with no listing seen yet, the newest listings straight away, in the
    // grid of the browse pages' small cards: no row of them over it, nor a stream of new ones.
    assert!(!body.contains("Sapo u shtuan") && !body.contains("/te-rejat") && !body.contains("data-cursor"), "{body}");
    let latest = section(&body, "latest-heading");
    assert!(latest.contains(&format!(r#"sizes="{SMALL_SIZES}""#)), "{latest}");
    // Its first photo comes before anything else on the page: on a phone, the biggest thing on the
    // first screen.
    assert_eq!(body.matches(r#"fetchpriority="high""#).count(), 1);
    let first = &latest[latest.find(r#"<article class="card""#).unwrap()..];
    assert!(first[..first.find("</article>").unwrap()].contains(r#"fetchpriority="high""#), "{latest}");
    // The stream's address is gone.
    assert_eq!(get(pool, "/te-rejat").await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn category_page_includes_subcategories(pool: PgPool) {
    // Automjete has no listings of its own; the cars sit in its Makina subcategory.
    let response = get(pool, "/c/automjete").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("iPhone 13"));
    assert!(response.body.contains(r#"<p class="result-count" id="result-count" data-show="Shfaq 2 njoftime">2 njoftime</p>"#));
    // Cards show the cover photo's thumbnail.
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=480"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn card_details_start_with_the_city(pool: PgPool) {
    // The city, with no pin before it, in a span of its own (the list hides it), then the
    // category's card properties.
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(!cars.contains(r#"<p class="card-summary"><svg"#));
    assert!(cars.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2+1 · 85\u{a0}m²</p>"));
    // Rents are per month.
    assert!(property.contains("<span class=\"card-figure\">450\u{a0}€</span><span class=\"card-note\">/muaj</span>"));
    // Without properties, the line is just the city.
    let other = get(pool.clone(), "/c/te-tjera").await.body;
    assert!(other.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Berat</span></p>"));
    // A phone shows its storage on the card; its brand only on its page.
    let phones = get(pool.clone(), "/c/elektronike").await.body;
    assert!(phones.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>128 GB</p>"));
    // Under the price, for the list, where it is: the pin and the town, in every card, in a
    // search in that town too.
    assert!(phones.contains(r#"<p class="card-place" translate="no"><svg class="pin-icon" width="14" height="14""#));
    // Phosphor's map pin (icons.rs, PIN), as every icon of the site is since 2026-10-11.
    assert!(phones.contains(r#"aria-hidden="true"><g transform="scale(.09375)" stroke="none"><path d="M128,24a80,80,0,0,0-80,80"#));
    assert!(phones.contains("</g></svg>Tiranë</p>"));
    // Searching in Tirana, the Tirana cards leave the city out and show the brand instead...
    let in_tirana = get(pool.clone(), "/c/elektronike?city=tirane").await.body;
    assert!(in_tirana.contains(r#"<p class="card-summary">Apple · 128 GB</p>"#));
    assert!(in_tirana.contains("</g></svg>Tiranë</p>"));
    // ...and a car the first property its title doesn't name: not the make, nor the model.
    let cars = get(pool, "/c/automjete?city=tirane").await.body;
    assert!(cars.contains("<p class=\"card-summary\">Hatchback · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_links_to_its_listing_through_the_title(pool: PgPool) {
    let body = get(pool, "/").await.body;
    // The title link is stretched over the whole card (see .card-link in page.css).
    assert!(body.contains(
        r#"<h2 class="card-title"><a class="card-link" href="/listing/1001-volkswagen-golf-7-2016-nafte">"#
    ));
    // Every listing once in the grid.
    assert_eq!(body.matches(r#"<article class="card" id="listing-"#).count(), 12);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn prona_shows_big_cards_until_the_visitor_chooses_small_ones_there(pool: PgPool) {
    let big = r#"<div class="browse cards-big" data-controller="filters cards place""#;
    let small = r#"<div class="browse" data-controller="filters cards place""#;
    // Prona and the categories under it: big cards, their own cookie for the view button...
    let prona = get(pool.clone(), "/c/prona").await.body;
    assert!(prona.contains(big) && prona.contains(r#"data-cards-cookie-value="cards-prona""#), "{prona}");
    assert!(prona.contains(r#"aria-label="Pamja: Kartela të mëdha""#));
    assert!(get(pool.clone(), "/c/apartamente").await.body.contains(big));
    // ...which a choice on other pages leaves as they are; one made there doesn't.
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards=small").await.body.contains(big));
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards-prona=small").await.body.contains(small));
    // Other categories: small cards, unless the visitor chose big ones.
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(cars.contains(small) && cars.contains(r#"data-cards-cookie-value="cards""#));
    assert!(get_with_cookie(pool.clone(), "/c/automjete", "cards-prona=big").await.body.contains(small));
    assert!(get_with_cookie(pool, "/c/automjete", "cards=big").await.body.contains(big));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

    // Small cards unless the visitor chose big ones: a small card shows its first photo (page.css),
    // the copy fetched for a column of the grid.
    let small = get(pool.clone(), "/c/makina").await.body;
    assert!(small.contains(r#"<div class="browse" data-controller="filters cards place""#));
    assert!(small.contains(r#"aria-label="Pamja: Kartela të vogla""#));
    assert!(small.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));

    // Big ones, from the cookie the view menu sets: every photo, the cover first, each with its
    // full copy for the browser to pick, and the dots and arrows to slide through them.
    let request = Request::get("/c/makina").header("cookie", "cards=big").body(Body::empty()).unwrap();
    let big = send(pool.clone(), request).await.body;
    assert!(big.contains(r#"<div class="browse cards-big" data-controller="filters cards place""#));
    assert!(big.contains(r#"aria-label="Pamja: Kartela të mëdha""#));
    assert!(!big.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
    assert_eq!(big.matches(r#"<div class="card-media" data-controller="slides">"#).count(), 1, "only the Golf has more than one");
    let golf = &big[big.find(r#"<div class="card-media" data-controller="slides">"#).unwrap()..];
    let golf = &golf[..golf.find("</article>").unwrap()];
    assert!(golf.contains(r#"href="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    assert_eq!(golf.matches(r#"<a class="card-slide""#).count(), 3);
    let cover = golf.find("photo-1591230740238").unwrap();
    assert!(cover < golf.find("card-1.jpg").unwrap() && golf.find("card-1.jpg").unwrap() < golf.find("card-2.jpg").unwrap());
    assert!(golf.contains(&format!(r#"srcset="https://example.com/card-2.jpg 600w, https://example.com/full-2.jpg 1600w" sizes="{BIG_SIZES}""#)));
    assert_eq!(golf.matches(r#"<span class="is-current"></span><span></span><span></span>"#).count(), 1, "a dot for each photo");
    // Over the photo's top left, who sells it, their initial in a circle, and since when they are
    // a user.
    assert!(golf.contains(r#"<p class="card-seller"><span class="avatar" aria-hidden="true">A</span><span class="card-seller-text"><span class="card-seller-name" translate="no">Ana</span><span>Përdorues që nga "#));
    assert!(golf.contains(r#"aria-label="Fotoja tjetër" hidden>"#));

    // The next pages come the same way, and the map page's list stays small.
    let pinned = Request::get("/harta").header("cookie", "cards=big").body(Body::empty()).unwrap();
    assert!(send(pool, pinned).await.body.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_list_shows_a_line_for_each_listing(pool: PgPool) {
    // The view menu: small cards, big ones or the list, the one now ticked and its icon on the
    // pill; picking one switches the cards at once (cards_controller.js).
    let cars = get(pool.clone(), "/c/makina").await.body;
    let menu = &cars[cars.find(r#"<details class="sort-menu cards-menu""#).unwrap()..];
    let menu = &menu[..menu.find("</details>").unwrap()];
    let options = positions(menu, &[
        r#"value="small" data-action="change->cards#choose" checked>"#,
        r#"value="big" data-action="change->cards#choose">"#,
        r#"value="list" data-action="change->cards#choose">"#,
    ]);
    assert!(options.is_sorted(), "{options:?}");
    assert!(menu.contains("</svg>Kartela të vogla</label>") && menu.contains("</svg>Listë</label>"));
    assert!(cars.contains(&format!(r#"data-cards-list-value="{LIST_SIZES}""#)));

    // Chosen, from its cookie: the page says so, the photos fetched for the line's left, each
    // card saying when it was posted after its details line (page.css leaves that out of cards).
    let request = Request::get("/c/makina").header("cookie", "cards=list").body(Body::empty()).unwrap();
    let list = send(pool.clone(), request).await.body;
    assert!(list.contains(r#"<div class="browse cards-list" data-controller="filters cards place""#));
    assert!(list.contains(r#"aria-label="Pamja: Listë""#));
    assert!(list.contains(r#"value="list" data-action="change->cards#choose" checked>"#));
    assert!(list.contains(&format!(r#"sizes="{LIST_SIZES}""#)));
    let golf = &list[list.find(r#"<article class="card" id="listing-1001">"#).unwrap()..];
    let golf = &golf[..golf.find("</article>").unwrap()];
    assert!(golf.contains("Naftë</p>\n    <p class=\"card-when\">"));

    // Prona keeps its own: the list chosen elsewhere leaves its big cards; the map's list is small.
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards=list").await.body.contains(r#"<div class="browse cards-big""#));
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards-prona=list").await.body.contains(r#"<div class="browse cards-list""#));
    let map = Request::get("/harta").header("cookie", "cards=list").body(Body::empty()).unwrap();
    assert!(send(pool, map).await.body.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_count_goes_on_past_a_thousand_once_the_page_shows(pool: PgPool) {
    // With numri=1, only the count, as far as 10 000, for the heading's line and the button at
    // the foot of the filters ("Shfaq 2 njoftime"), which the page asks for when its own count
    // stopped (filters_controller.js); never kept by the browser.
    let few = get(pool.clone(), "/c/makina?numri=1").await;
    assert_eq!(few.content_type.as_deref(), Some("application/json"));
    assert_eq!(few.body, r#"{"count":2,"show":"Shfaq 2 njoftime","text":"2 njoftime"}"#);
    let page = html(pool.clone(), "/c/makina").await;
    assert!(page.contains(r#"<button type="button" class="button filters-show" data-filters-target="show" data-action="filters#showResults">Shfaq 2 njoftime</button>"#));
    assert!(!page.contains("data-capped"));
    let none = get(pool.clone(), "/c/makina?city=shkoder&numri=1").await;
    assert_eq!(none.body, r#"{"count":0,"show":"Asnjë njoftim","text":"0 njoftime"}"#);

    // 1 000 more in Të tjera: the page counts to 1 000 and says it stopped there; asked, the count
    // goes on.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 'ACTIVE', now()
         FROM generate_series(1, 1000) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let many = html(pool.clone(), "/c/te-tjera").await;
    assert!(many.contains("<p class=\"result-count\" id=\"result-count\" data-show=\"Shfaq 1k+ njoftime\" data-capped>1k+ njoftime</p>"));
    let counted = get(pool, "/c/te-tjera?numri=1").await;
    assert_eq!(counted.body, "{\"count\":1001,\"show\":\"Shfaq 1k njoftime\",\"text\":\"1k njoftime\"}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_parent_category_pages_in_order_across_its_subcategories(pool: PgPool) {
    // Thirty more each in Makina and Motoçikleta, both under Automjete, their prices and their
    // distances from Tirana's middle taking turns between the two. A page over Automjete reads
    // each subcategory's first in order and merges them (db::push_ordered); page after page, the
    // cards come as one ORDER BY over all of them would have them.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, price_normalized, currency, status,
                               published_at, latitude, longitude)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                (SELECT id FROM categories WHERE slug = CASE WHEN g % 2 = 0 THEN 'makina' ELSE 'motocikleta' END),
                'rradhe-' || g, 'Rradhë ' || g, '', 1000 + g * 37 % 60 * 10, 1000 + g * 37 % 60 * 10,
                'EUR', 'ACTIVE', now() - g * interval '1 hour', 41.3275 + g * 0.001, 19.8187
         FROM generate_series(1, 60) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let subtree = "WITH RECURSIVE tree AS (SELECT id FROM categories WHERE slug = 'automjete'
                                          UNION ALL
                                          SELECT c.id FROM categories c JOIN tree ON c.parent_id = tree.id)
                   SELECT id FROM tree";
    let distance = "(ll_to_earth(l.latitude::float8, l.longitude::float8) <-> ll_to_earth(41.3275, 19.8187))";
    for (uri, condition, order) in [
        ("/c/automjete", "true".to_string(), "l.sort_at DESC, l.id DESC".to_string()),
        ("/c/automjete?sort=price_asc", "true".to_string(), "coalesce(l.price_normalized, 'Infinity'), l.id".to_string()),
        (
            "/c/automjete?sort=price_desc",
            "true".to_string(),
            "coalesce(l.price_normalized, '-Infinity') DESC, l.id DESC".to_string(),
        ),
        (
            "/c/automjete?lat=41.3275&lng=19.8187&radius=50",
            format!("l.latitude IS NOT NULL AND {distance} <= 50000"),
            format!("{distance}, l.id"),
        ),
    ] {
        let expected: Vec<i64> = sqlx::query_scalar(&format!(
            "SELECT l.public_no FROM listings l
             WHERE l.status = 'ACTIVE' AND l.category_id IN ({subtree}) AND {condition}
             ORDER BY {order}"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(expected.len() > 2 * 24, "{uri}: more than two pages");
        assert_eq!(every_page(&pool, uri).await, expected, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_welcome_page_recommends_listings_like_those_looked_at(pool: PgPool) {
    // As under a listing (a_listing_shows_listings_like_it): three Golfs from Besnik at about the
    // Golf's price, Ana's own second Golf, and two Audis from Dritan, all in Makina in Tirana; a
    // second phone from Ana; and thirty more listings elsewhere, so that the newest take more than
    // a page.
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
    let numbers: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE slug = ANY('{audi-a3,audi-a4,golf-a,golf-b,golf-c,golf-d,iphone-12}')
         ORDER BY slug",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let [audi_a3, audi_a4, golf_a, golf_b, golf_c, golf_d, phone] = numbers[..] else { panic!("{numbers:?}") };
    let home = |cookie: String| {
        let pool = pool.clone();
        async move { get_with_cookie(pool, "/", &cookie).await.body.replace("&#38;", "&") }
    };
    let frame = |frame_id: String, src: String, cookie: &'static str| {
        let pool = pool.clone();
        async move {
            let request =
                Request::get(&src).header("turbo-frame", &frame_id).header(header::COOKIE, cookie).body(Body::empty()).unwrap();
            send(pool, request).await.body.replace("&#38;", "&")
        }
    };

    // With nothing looked at, the newest, as before.
    let page = html(pool.clone(), "/").await;
    assert!(page.contains(r#"<h2 id="latest-heading">Njoftimet më të reja</h2>"#) && !page.contains("recommended-heading"), "{page}");

    // The Golf seen, the cars like it in their place, the most alike first, as under the Golf
    // (a_listing_shows_listings_like_it), but Ana's second Golf too, which is another seller's to
    // the visitor; two of Besnik's three, the closest in price; not the Golf itself, which
    // "Shikuar së fundmi" has over them.
    let page = home("shikuar=1001".into()).await;
    let recommended = section(&page, "recommended-heading");
    assert!(recommended.contains(r#"<h2 id="recommended-heading">Të rekomanduara për ty</h2>"#), "{recommended}");
    assert!(!page.contains("latest-heading"));
    assert!(page.find(r#"id="seen-heading""#).expect("seen") < page.find(r#"id="recommended-heading""#).expect("recommended"));
    assert_eq!(card_numbers(recommended), [golf_d, golf_a, golf_b, audi_a3, audi_a4, 1002]);
    // Then every other listing, newest first, in the frames the page names: the first with the
    // seeds in its address (the Golf, at 1.0) and where they go on from, the sixth, so that they
    // go on as the page did whatever the visitor saw meanwhile; the next after the last listing
    // the first showed, as the newest go on.
    let mut shown = card_numbers(recommended);
    let (frame_id, src) = next_frame(recommended).expect("a next-page frame");
    assert_eq!((frame_id.as_str(), src.as_str()), ("page-reco-6", "/?sipas=1001x10&nga=6"));
    let rest = frame(frame_id.clone(), src.clone(), "shikuar=1001").await;
    assert!(rest.starts_with(r#"<turbo-frame id="page-reco-6" target="_top">"#), "{rest}");
    assert_eq!(rest, frame(frame_id, src, "shikuar=1005.1003").await);
    assert_eq!(card_numbers(&rest).len(), 24);
    shown.extend(card_numbers(&rest));
    let (frame_id, src) = next_frame(&rest).expect("a frame after it");
    assert!(src.starts_with("/?sipas=1001x10&after="), "{src}");
    let last = frame(frame_id.clone(), src, "shikuar=1001").await;
    assert!(last.starts_with(&format!(r#"<turbo-frame id="{frame_id}" target="_top">"#)), "{last}");
    assert!(next_frame(&last).is_none(), "{last}");
    shown.extend(card_numbers(&last));
    // Each listing on the site once, Besnik's third Golf and the Golf seen among the newest.
    let mut active: Vec<i64> =
        sqlx::query_scalar("SELECT public_no FROM listings WHERE status = 'ACTIVE'").fetch_all(&pool).await.unwrap();
    assert_eq!(shown.len(), active.len(), "{shown:?}");
    shown.sort_unstable();
    active.sort_unstable();
    assert_eq!(shown, active);
    assert!(card_numbers(&rest).contains(&golf_c) && card_numbers(&rest).contains(&1001));

    // The phone seen after the Golf: the listings like it among the cars, each category as often
    // as its seeds weigh, its best first (the phone at 1.0, the Golf at .9): Ana's second phone
    // second; not Besnik's laptop, two of his Golfs being there already.
    let page = home("shikuar=1005.1001".into()).await;
    let mixed = card_numbers(section(&page, "recommended-heading"));
    assert_eq!(mixed, [golf_d, phone, golf_a, golf_b, audi_a3, audi_a4, 1002]);

    // Signed in, Ana isn't recommended her own Golf; nor anyone the listings of an account they
    // blocked (Besnik).
    let ana = session_for(&pool, ANA_ID).await;
    let page = home(format!("{ana}; shikuar=1001")).await;
    assert_eq!(card_numbers(section(&page, "recommended-heading")), [golf_a, golf_b, audi_a3, audi_a4, 1002]);
    let reader = new_seller(&pool, "+355692223344").await;
    post_form(pool.clone(), &format!("/listing/{golf_a}/blloko"), &[("blloko", "1")], Some(&reader)).await;
    let page = home(format!("{reader}; shikuar=1001")).await;
    assert_eq!(card_numbers(section(&page, "recommended-heading")), [golf_d, audi_a3, audi_a4, 1002]);

    // Signed in, the listings kept seed them too, with nothing seen (the Golf kept, at 1.5), and
    // those written to a seller about (the Mercedes, at 2), which isn't among them.
    let keeper = new_seller(&pool, "+355692223355").await;
    assert_eq!(send(pool.clone(), tap_heart("1001", "1", Some(&keeper))).await.status, StatusCode::NO_CONTENT);
    let page = home(keeper).await;
    let recommended = section(&page, "recommended-heading");
    assert!(!page.contains("seen-heading"));
    assert_eq!(card_numbers(recommended), [golf_d, golf_a, golf_b, audi_a3, audi_a4, 1002]);
    assert_eq!(next_frame(recommended).map(|(_, src)| src).as_deref(), Some("/?sipas=1001x15&nga=6"));
    let writer = new_seller(&pool, "+355692223366").await;
    sqlx::query(
        "INSERT INTO conversations (listing_id, buyer_user_id, seller_business_id, last_message_at)
         SELECT l.id, u.id, l.seller_business_id, now() FROM listings l, users u
         WHERE l.public_no = 1002 AND u.phone_e164 = '+355692223366'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let page = home(writer).await;
    let recommended = section(&page, "recommended-heading");
    let cars = card_numbers(recommended);
    assert!(cars.len() >= 4 && !cars.contains(&1002), "{cars:?}");
    assert_eq!(next_frame(recommended).map(|(_, src)| src), Some(format!("/?sipas=1002x20&nga={}", cars.len())));

    // In English.
    let page = home("gjuha=en; shikuar=1001".into()).await;
    assert!(page.contains(r#"<h2 id="recommended-heading">Recommended for you</h2>"#), "{page}");

    // A listing with too few like it on the site (the dog, alone in its category): the newest, as
    // for one who looked at nothing.
    let page = home("shikuar=1009".into()).await;
    assert!(page.contains(r#"id="latest-heading""#) && !page.contains("recommended-heading"), "{page}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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
    let next = send(pool.clone(), request).await;
    assert_eq!(next.status, StatusCode::OK);
    assert!(next.body.starts_with(&format!(r#"<turbo-frame id="{frame_id}""#)));
    assert!(!next.body.contains("<html"));
    // 42 listings: 24 on the first page, the remaining 18 here, and no further page.
    assert_eq!(next.body.matches(r#"class="card""#).count(), 18);
    assert!(!next.body.contains("Shfaq më shumë"));

    // The same address, asked for without the frame (a search engine, a link opened anew), is the
    // whole page: both answers say they differ with Turbo-Frame, so that a cache keeps them apart.
    let vary = |response: &TestResponse| {
        response.headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap().to_ascii_lowercase()).collect::<Vec<_>>()
    };
    assert!(vary(&next).iter().any(|value| value.contains("turbo-frame")), "{:?}", vary(&next));
    let whole = get(pool, &src).await;
    assert_eq!(whole.status, StatusCode::OK);
    assert!(whole.body.contains("<html") && whole.body.contains(r#"<link rel="canonical""#));
    assert!(vary(&whole).iter().any(|value| value.contains("turbo-frame")), "{:?}", vary(&whole));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn parts_have_their_kinds_a_level_down(pool: PgPool) {
    // Automjete lists its four subcategories, each with its icon; the kinds of parts sit under the
    // fourth.
    let vehicles = html(pool.clone(), "/c/automjete").await;
    for (slug, name) in [
        ("makina", "Makina"),
        ("motocikleta", "Motoçikleta"),
        ("automjete-tregtare", "Tregtare"),
        ("pjese-dhe-aksesore", "Pjesë"),
    ] {
        let pill = &vehicles[vehicles.find(&format!(r#"<a class="shortcut" href="/c/{slug}">"#)).expect(slug)..];
        assert!(pill.starts_with(&format!(r#"<a class="shortcut" href="/c/{slug}"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#)), "{slug}");
        assert!(pill[..pill.find("</a>").unwrap()].ends_with(&format!(r#"</svg></span><span class="shortcut-name">{name}</span>"#)), "{slug}");
    }
    assert!(!vehicles.contains(r#"<a class="shortcut" href="/c/gps-dhe-navigatore">"#));
    // Every main category's subcategories have icons of their own too.
    for slug in ["prona", "elektronike", "shtepi-dhe-kopsht", "pune", "sherbime", "veshje-dhe-aksesore", "sport-dhe-hobi",
                 "kafshe-shtepiake", "makineri-dhe-biznes"] {
        let page = html(pool.clone(), &format!("/c/{slug}")).await;
        // The subcategories' tiles, a line that slides sideways, each its icon on a shape over its
        // name (the user's ask of 2026-10-11; pills until then).
        let nav = &page[page.find(r#"<nav class="subcategories tiles" data-controller="drag-row" aria-label="Nënkategoritë">"#).expect(slug)..];
        let nav = &nav[..nav.find("</nav>").unwrap()];
        let pills = nav.matches(r#"<a class="shortcut""#).count();
        assert!(pills > 0 && nav.matches(r#"<svg class="shortcut-icon""#).count() == pills, "{slug}: {nav}");
        assert_eq!(nav.matches(r#"<span class="shortcut-shape" aria-hidden="true">"#).count(), pills, "{slug}: {nav}");
        assert_eq!(nav.matches(r#"<span class="shortcut-name">"#).count(), pills, "{slug}: {nav}");
    }

    // The parts page lists its kinds in order, the spare parts first, under its old address.
    let parts = html(pool.clone(), "/c/pjese-dhe-aksesore").await;
    let kinds = positions(&parts, &[
        r#"<a class="shortcut" href="/c/pjese-kembimi"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/goma-dhe-disqe"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/audio-dhe-video"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/gps-dhe-navigatore"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/styling-dhe-pajisje-sportive"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/tapete-makine"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/rimorkio"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/mbushes-per-makina-elektrike"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/pjese-te-tjera"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
    ]);
    assert!(kinds.is_sorted(), "{kinds:?}");

    // A kind's page: its heading, the way back before it, to its parent; no pills under, as it has
    // no subcategories.
    let gps = html(pool.clone(), "/c/gps-dhe-navigatore").await;
    let head = positions(&gps, &[
        r#"<a class="breadcrumb-back title-back" href="/c/pjese-dhe-aksesore" aria-label="Kthehu""#,
        r#"<h1 class="browse-title">GPS</h1>"#,
    ]);
    assert!(head.is_sorted(), "{head:?}");
    assert!(!gps.contains(r#"<nav class="subcategories"#), "no subcategories, no tiles under");
    // Every branch closes; the other two </details> are the order's and the view's (and the
    // typeface panel's, while TYPE_TRIAL is on).
    let others = 2 + gps.matches("<details class=\"type-menu\"").count();
    // ...and the row's filter dropdowns' (filter_drop.html).
    let others = others + gps.matches("<details class=\"filter-drop\"").count();
    assert_eq!(gps.matches("<details class=\"menu-branch\"").count(), gps.matches("</details>").count() - others, "balanced");

    // The old address still works.
    assert_eq!(get(pool.clone(), "/c/pjese-kembimi").await.status, StatusCode::OK);
    // Every dropdown of categories shows the kinds, a level further in: the search's filter too.
    let search = get(pool, "/search").await.body;
    assert!(search.contains("<option value=\"gps-dhe-navigatore\">\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}GPS</option>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_category_pages_way_up_keeps_the_words_and_the_town(pool: PgPool) {
    // A main category's way back, before its name, is the welcome page (it has no breadcrumb); its
    // subcategories, the pills under its heading, keep the town and the words, and so does the way
    // up from one of them.
    let page = html(pool.clone(), "/c/prona?city=durres&q=shtepi").await;
    assert!(page.contains(r#"<a class="breadcrumb-back title-back" href="/""#) && !page.contains(r#"<nav class="breadcrumb""#));
    assert!(page.contains(r#"<a class="shortcut" href="/c/apartamente?q=shtepi&city=durres">"#));
    let flats = html(pool.clone(), "/c/apartamente?city=durres&q=shtepi").await;
    assert!(flats.contains(r#"<a class="breadcrumb-back title-back" href="/c/prona?q=shtepi&city=durres" aria-label="Kthehu""#), "{flats}");
    // A search page's heading is just a heading.
    assert!(!get(pool, "/search?q=golf").await.body.contains(r#"<nav class="breadcrumb""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_category_page_says_where_it_is_over_its_name(pool: PgPool) {
    // Makina: its way back before its name (leading a level up without scripts), on a circle in
    // the accent; no pills under, as it has no subcategories; and the trail for search engines
    // (schema.org's BreadcrumbList). A line of words over its name, "‹ Automjete › Makina", said
    // it until the user's ask of 2026-10-11, and then the category above alone, small, until the
    // user asked it away the same day.
    let cars = html(pool.clone(), "/c/makina").await;
    assert!(!cars.contains("browse-up") && !cars.contains("breadcrumb-up"), "{cars}");
    let order = positions(&cars, &[
        r#"<a class="breadcrumb-back title-back" href="/c/automjete" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#,
        r#"<h1 class="browse-title">Makina</h1>"#,
        r#"<p class="result-count" id="result-count""#,
        r#"<div class="browse-tools""#,
        r#"<details class="sort-menu cards-menu""#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!cars.contains(r#"<nav class="breadcrumb""#) && !cars.contains("breadcrumb-here"), "{cars}");
    assert!(!cars.contains("category-switch") && !cars.contains("category-level") && !cars.contains("Faqja kryesore"));
    let head = &cars[cars.find(r#"<div class="browse-head">"#).unwrap()..cars.find(r#"<div class="browse-tools""#).unwrap()];
    assert!(!head.contains(r#"<a class="shortcut""#), "no subcategories, no pills: {head}");
    assert!(cars.contains(r#""@type":"BreadcrumbList""#));
    assert!(cars.contains(r#"{"@type":"ListItem","item":"http://127.0.0.1:3000/c/makina","name":"Makina","position":3}"#), "{cars}");
    // Three levels down, the way up is the category above alone.
    let gps = html(pool.clone(), "/c/gps-dhe-navigatore").await;
    assert!(gps.contains(r#"<a class="breadcrumb-back title-back" href="/c/pjese-dhe-aksesore" aria-label="Kthehu""#));
    assert!(!gps.contains("breadcrumb-up"), "{gps}");
    // The way back is the bare chevron in the words' colour, a grey circle under the pointer (on a
    // circle in the accent for a moment on 2026-10-11, which the user asked back the same day).
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    for rule in [
        ".breadcrumb-back{display:flex;flex:none;align-items:center;justify-content:center;width:var(--size-button-sm);height:var(--size-button-sm);margin:0 -4px 0 -12px;border-radius:var(--radius-pill);color:var(--text);}",
        ".title-back{align-self:flex-start;height:1.2em;margin:-.03em -8px 0 -12px;font-size:1.875rem;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(".browse-up") && !css.contains(".breadcrumb-back:active"));
    // A main category's page has no breadcrumb since the user's ask of 2026-10-11 (it said "Kreu"
    // over the page's name): its way back, to the welcome page without scripts, stands before its
    // name; its subcategories stand under its heading. A search's heading stays a heading, without
    // either.
    let vehicles = html(pool.clone(), "/c/automjete").await;
    assert!(vehicles.contains(r#"<a class="breadcrumb-back title-back" href="/" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#), "back to the welcome page without scripts");
    assert!(!vehicles.contains(r#"<nav class="breadcrumb""#) && !vehicles.contains("browse-up") && !vehicles.contains(r#"<a class="breadcrumb-up" href="/">Kreu</a>"#));
    let at = positions(&vehicles, &[
        r#"<h1 class="browse-title">Automjete</h1>"#,
        r#"<nav class="subcategories tiles" data-controller="drag-row" aria-label="Nënkategoritë">"#,
        r#"<a class="shortcut" href="/c/makina"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#,
    ]);
    assert!(at.is_sorted(), "{at:?}");
    let search = html(pool, "/search?q=golf").await;
    assert!(search.contains(r#"<h1 class="browse-title">Rezultatet për „golf“</h1>"#) && !search.contains(r#"<nav class="breadcrumb""#) && !search.contains("BreadcrumbList"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_phone_sees_the_listings_on_its_first_screen(pool: PgPool) {
    // The user's ask of 2026-10-11, Makina on a phone: the map, the view and the order join the
    // filters' line, which slides sideways, rather than standing on a line of their own under it,
    // Filtrat, the filters picked, the map, the order and the view first, the dropdowns after; the
    // order a grey pill there, and what it and the view open placed by the head, so that the
    // line's sliding doesn't cut it. On a wide screen the line is no box, the row as it was.
    let cars = html(pool.clone(), "/c/makina?city=tirane").await;
    let open = r#"<div class="browse-tools" data-filters-target="tools" data-action="click->filters#foldFromRow">"#;
    let row = &cars[cars.find(open).unwrap() + open.len()..cars.find(r#"<aside class="filters""#).unwrap()];
    assert!(row.trim_start().starts_with(r#"<div class="browse-filters-line">"#), "{row}");
    let order = positions(row, &[
        r#"<button type="button" class="button secondary round-button labelled filters-open""#,
        r#"<ul class="active-filters""#,
        r#"<section class="map-card map-hidden""#,
        r#"<details class="sort-menu cards-menu""#,
        r#"<details class="sort-menu" data-controller="dropdown""#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    let after_order = &row[row.rfind("</details>").unwrap()..];
    assert_eq!(after_order.matches("</div>").count(), 2, "the line ends after the order, then the row: {after_order}");
    // With a town, the map's button is its pin alone (the town's chip says it, "Tiranë" twice
    // until then), the town its tooltip and what screen readers hear.
    assert!(row.contains(r#"class="button secondary round-button labelled map-button is-set""#) && row.contains(r#"aria-label="Harta: Tiranë" title="Harta: Tiranë""#), "{row}");
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    for rule in [
        ".browse-filters-line > .active-filters{order:1;}.browse-filters-line .map-button{order:2;margin-left:0;}.browse-filters-line > .sort-menu{order:3;}.browse-filters-line > .cards-menu{order:4;}.browse-filters-line > .most-viewed-button{order:5;}.browse-filters-line > .filter-drop{order:6;}.browse-filters-line > .clear-filters{order:7;}",
        "@media (max-width:719px){.browse-filters-line > .sort-menu{position:static;}",
        ".browse-filters-line > .sort-menu:not(.cards-menu) > .sort-options{top:calc(100% + var(--drop));right:auto;left:0;",
        ".browse-tools .button.round-button.labelled.map-button.is-set{width:var(--row);padding:0;}",
        ".browse-tools .map-button.is-set .button-label{display:none;}",
        "@media (max-width:719px){.browse-filters-line .button.map-button{flex:none;}}",
        // Closer gaps over the results and "Më të klikuarat".
        "@media (max-width:719px){.results-head{margin:20px 0 10px;}}",
        "@media (max-width:719px){.browse > .most-viewed{margin-top:20px;}.most-viewed > h2{margin-bottom:10px;}}",
        // A promoted card's price on the listing page's band, in the accent, the month and
        // "(i diskutueshëm)" off it (the user's ask of the same day, in place of a pill saying
        // "I promovuar" over its photo).
        // In the dark theme's band in both themes, as dark as the photo (the user's pick of the
        // same day, the accent itself first).
        ".card.is-promoted .card-figure{text-decoration-line:underline;text-decoration-color:var(--price-mark-photo);text-decoration-thickness:.26em;text-decoration-skip-ink:none;text-underline-offset:-.12em;}",
        "--price-mark-photo:color-mix(in srgb,var(--highlight) var(--filter-share-dark),#1b1b1b);",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains("featured-row") && !css.contains(".row-note"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

/// What a listing's page sends once it shows (application.js): it was seen, from this address and
/// browser.
fn seen(no: i64, ip: &str, agent: &str) -> Request<Body> {
    Request::post(format!("/listing/{no}/pare"))
        .header("x-forwarded-for", ip)
        .header(header::USER_AGENT, agent)
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_most_seen_listings_have_a_row_on_the_welcome_page(pool: PgPool) {
    // Seen by nobody yet: no row.
    assert!(!html(pool.clone(), "/").await.contains("Më të klikuarat"));

    // A listing's page asks to count the view, but not its seller's.
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    assert!(page.contains(r#"data-seen-url="/listing/1001/pare""#));
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let own = get_with_cookie(pool.clone(), &format!("/listing/{no}-samsung-galaxy-s21-128-gb"), &ana).await.body;
    assert!(own.contains("Ndrysho") && !own.contains("data-seen-url"));

    // 1002 seen by three visitors, one of them twice, which counts once; 1001 by two; a bot's
    // visit and a number no listing has count for nothing.
    let app = app(pool.clone());
    let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1";
    for (no, ip) in [(1002, "203.0.113.1"), (1002, "203.0.113.1"), (1002, "203.0.113.2"), (1002, "203.0.113.3"),
                     (1001, "203.0.113.1"), (1001, "203.0.113.2"), (999_999, "203.0.113.1")] {
        assert_eq!(send_to(app.clone(), seen(no, ip, safari)).await.status, StatusCode::NO_CONTENT);
    }
    send_to(app.clone(), seen(1003, "203.0.113.4", "Mozilla/5.0 (compatible; Googlebot/2.1)")).await;
    let views = |no: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT sum(s.detail_views)::bigint FROM listing_daily_stats s JOIN listings l ON l.id = s.listing_id WHERE l.public_no = $1",
            )
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!((views(1002).await, views(1001).await, views(1003).await), (Some(3), Some(2), None));

    // The row, over the newest, the most seen first, each saying how many times it was seen, and
    // none that nobody saw.
    let home = html(pool.clone(), "/").await;
    let row = section(&home, "most-viewed-heading");
    assert_eq!(card_numbers(row), vec![1002, 1001]);
    // A row of small cards that slides sideways, as "Shikuar së fundmi" (card_row.html), their ids
    // their own, as the user asked on 2026-10-11 (two big cards earlier that day).
    assert!(home.contains(r#"<section class="most-viewed" data-controller="drag-row" aria-labelledby="most-viewed-heading">"#), "{home}");
    assert!(row.contains(r#"<div class="card-row"><article class="card" id="most-viewed-1002">"#), "{row}");
    // Their photos are fetched as wide as the row's cards; the first one's before anything else on
    // the page.
    assert!(row.contains(&format!(r#"sizes="{ROW_SIZES}""#)) && !row.contains(BIG_SIZES), "{row}");
    assert_eq!(home.matches(r#"fetchpriority="high""#).count(), 1);
    let first = &row[row.find(r#"<article class="card""#).unwrap()..];
    assert!(first[..first.find("</article>").unwrap()].contains(r#"fetchpriority="high""#), "{row}");
    assert!(row.contains(r#"<h2 class="card-title">"#) && row.contains(r#"<p class="card-when">"#), "{row}");
    assert!(row.contains(r#"<span class="card-views">3 shikime këtë javë</span><span class="card-views-short">3 shikime</span>"#), "{row}");
    assert!(row.contains(r#"<span class="card-views">2 shikime këtë javë</span>"#), "{row}");
    assert!(!home[home.find(r#"id="latest-heading""#).unwrap()..].contains("card-views"));

    // The listing's own page says how many times it was seen in all, under the heart; one nobody
    // saw says nothing.
    let page = get(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.body;
    assert!(page.contains(r#"<p class="listing-views" title="3 shikime">"#), "{page}");
    let unseen = get(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await.body;
    assert!(unseen.contains("listing-meta") && !unseen.contains("listing-views"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn most_seen_are_eight_in_a_row(pool: PgPool) {
    // Ten listings seen this week, each fewer times than the one before: "Më të klikuarat" shows
    // the eight seen the most, the most first, in a row of small cards as "Shikuar së fundmi" has
    // them (the user's ask of 2026-10-11, "Load 8 posts").
    sqlx::query(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views)
         SELECT id, current_date, 100 - public_no % 100 FROM listings WHERE public_no BETWEEN 1001 AND 1010",
    )
    .execute(&pool)
    .await
    .unwrap();
    let home = html(pool.clone(), "/").await;
    let row = section(&home, "most-viewed-heading");
    assert_eq!(card_numbers(row), (1001..=1008).collect::<Vec<_>>(), "{row}");
    assert!(row.contains(r#"<div class="card-row">"#), "{row}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_main_categories_and_a_few_big_ones_have_their_own_most_seen(pool: PgPool) {
    // The apartment and the house in their own subcategories of Prona, the iPhone among the phones.
    for (slug, no) in [("apartamente", 1003), ("vila-dhe-shtepi", 1004), ("telefona", 1005)] {
        sqlx::query("UPDATE listings SET category_id = (SELECT id FROM categories WHERE slug = $1) WHERE public_no = $2")
            .bind(slug)
            .bind(no)
            .execute(&pool)
            .await
            .unwrap();
    }
    // The Mercedes seen by two visitors, the Golf, the apartment, the house and the iPhone by one.
    let app = app(pool.clone());
    let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1";
    for (no, ip) in [(1002, "203.0.113.1"), (1002, "203.0.113.2"), (1001, "203.0.113.1"), (1003, "203.0.113.1"),
                     (1004, "203.0.113.1"), (1005, "203.0.113.1")] {
        assert_eq!(send_to(app.clone(), seen(no, ip, safari)).await.status, StatusCode::NO_CONTENT);
    }
    // Each main category's page has its own, and each big subcategory's, the most seen first, under
    // the page's buttons...
    let row = |page: &str| {
        let mut numbers = card_numbers(section(page, "most-viewed-heading"));
        numbers.sort();
        numbers
    };
    let vehicles = html(pool.clone(), "/c/automjete").await;
    assert_eq!(card_numbers(section(&vehicles, "most-viewed-heading")), [1002, 1001]);
    let (buttons, seen, results) = (vehicles.find(r#"class="browse-tools""#), vehicles.find("Më të klikuarat"), vehicles.find(r#"class="results""#));
    assert!(buttons < seen && seen < results, "under the buttons, over the results");
    // The listings under it, headed by their order; a search across the categories by the order
    // alone.
    assert!(vehicles.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të reja</h2>"#));
    let cheapest = html(pool.clone(), "/c/automjete?sort=price_asc").await;
    assert!(cheapest.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të lira</h2>"#));
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<h2 class="results-heading" id="results-heading">Më të përshtatshmet</h2>"#));
    assert_eq!(row(&html(pool.clone(), "/c/prona").await), [1003, 1004]);
    assert_eq!(row(&html(pool.clone(), "/c/elektronike").await), [1005]);
    assert_eq!(row(&html(pool.clone(), "/c/makina").await), [1001, 1002]);
    assert_eq!(row(&html(pool.clone(), "/c/apartamente").await), [1003]);
    assert_eq!(row(&html(pool.clone(), "/c/vila-dhe-shtepi").await), [1004]);
    // The welcome page's has all five seen (up to eight, MOST_VIEWED_SHOWN), the most first, in
    // the row, their photos fetched as wide as the row's cards (ROW_SIZES).
    let home = html(pool.clone(), "/").await;
    let all = section(&home, "most-viewed-heading");
    assert_eq!(all.matches(r#"<article class="card""#).count(), 5, "{all}");
    assert_eq!(card_numbers(all)[0], 1002);
    assert!(all.contains(&format!(r#"sizes="{ROW_SIZES}""#)) && !all.contains(SMALL_SIZES) && !all.contains(BIG_SIZES), "{all}");
    // A round button beside the view menu folds it away, pressed while it shows; the cookie it
    // sets (most_viewed_controller.js) keeps it folded on the next pages, where it stays in the
    // page, hidden, its photos waiting, and the results' first photo comes first instead.
    assert!(vehicles.contains(r#"class="button secondary round-button most-viewed-button" aria-controls="most-viewed" aria-pressed="true""#));
    assert!(vehicles.contains(r#"<section class="most-viewed" id="most-viewed" data-controller="drag-row" aria-labelledby="most-viewed-heading">"#));
    assert!(section(&vehicles, "most-viewed-heading").contains(r#"fetchpriority="high""#));
    let folded = get_with_cookie(pool.clone(), "/c/automjete", "most-viewed=hidden").await.body;
    assert!(folded.contains(r#"aria-controls="most-viewed" aria-pressed="false""#));
    assert!(folded.contains(r#"<section class="most-viewed" id="most-viewed" data-controller="drag-row" hidden aria-labelledby="most-viewed-heading">"#));
    let row_html = section(&folded, "most-viewed-heading");
    let lazy = |html: &str| html.matches(r#"loading="lazy""#).count();
    assert!(!row_html.contains(r#"fetchpriority="high""#), "{row_html}");
    assert!(lazy(row_html) > lazy(section(&vehicles, "most-viewed-heading")), "{row_html}");
    assert_eq!(folded.matches(r#"fetchpriority="high""#).count(), 1);
    assert!(section(&folded, "results-heading").contains(r#"fetchpriority="high""#));
    // ...but no other subcategory's, nor a page with a filter or words on, which it would take no
    // notice of.
    assert!(!html(pool.clone(), "/c/telefona").await.contains("Më të klikuarat"));
    assert!(!html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await.contains("Më të klikuarat"));
    assert!(!html(pool, "/c/makina?q=golf").await.contains("Më të klikuarat"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_carries_the_start_of_its_description(pool: PgPool) {
    // The Golf's whole description, short enough, under its title, for the list to show.
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let description = positions(&golf, &[
        r#"<h2 class="card-title">"#,
        r#"<p class="card-description" data-lines-target="text"><span class="card-description-text">Makina është në gjendje shumë të mirë, e mirëmbajtur rregullisht. Kambio manuale, 148 000 km.</span></p>"#,
    ]);
    assert!(description.is_sorted(), "{golf}");
    // A long one, its lines run together, is cut at a word near 400 characters, ending in "…".
    let long = format!("Shumë e mirë.\n\n{}", "fjalë ".repeat(120));
    sqlx::query("UPDATE listings SET description = $2 WHERE public_no = $1").bind(1001_i64).bind(&long).execute(&pool).await.unwrap();
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let open = r#"<p class="card-description" data-lines-target="text"><span class="card-description-text">"#;
    let text = &golf[golf.find(open).unwrap() + open.len()..];
    let text = &text[..text.find("</span>").unwrap()];
    assert!(text.starts_with("Shumë e mirë. fjalë fjalë") && text.ends_with("fjalë…"), "{text}");
    assert!(text.chars().count() <= 401, "{}", text.chars().count());
    // An empty one, no block.
    sqlx::query("UPDATE listings SET description = '' WHERE public_no = $1").bind(1001_i64).execute(&pool).await.unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1001).contains("card-description"));
    // In the list a listing has no box round it but under the pointer or the keyboard's focus, and
    // its description fills the room under its details, as many whole lines as fit, the last ending
    // in "…" (lines_controller.js sets --lines; two without scripts, one on a phone), on a phone
    // too: the user's asks of 2026-10-11. (The stylesheet comes minified: web::site_css.)
    assert!(html(pool.clone(), "/c/makina").await.contains(r#"<section class="results" data-controller="lines" aria-labelledby="results-heading">"#));
    let css = get(pool, &format!("/static/css/site.css?v={}", asset_version())).await.body;
    let list_card = &css[css.find(".cards-list .results .card{").expect("the list's card")..];
    assert!(!list_card[..list_card.find('}').unwrap()].contains("background:"), "no box at rest");
    assert!(css.contains(".cards-list .results .card:hover{background:var(--surface);box-shadow:var(--shadow);}"));
    assert!(css.contains(".cards-list .results .card:focus-within{background:var(--surface);box-shadow:var(--shadow);}"));
    // The room is the stretched paragraph; the clamp is on the words in it, so that no line shows
    // past the last (on the stretched box itself the next one peeked out under it).
    let room = &css[css.find(".cards-list .results .card-description{").expect("the list's description")..];
    let room = &room[..room.find('}').unwrap()];
    assert!(room.contains("align-self:stretch;") && room.contains("overflow:hidden;") && !room.contains("line-clamp"), "{room}");
    assert!(css.contains(".cards-list .results .card-description-text{display:-webkit-box;overflow:hidden;-webkit-box-orient:vertical;-webkit-line-clamp:var(--lines,2);line-clamp:var(--lines,2);}"));
    assert!(css.contains(".cards-list .results .card-description-text{-webkit-line-clamp:var(--lines,1);line-clamp:var(--lines,1);}"));
    // The place and when it was posted read as one line, "Tiranë · para 2 ditësh": the rule over both
    // sets only their block's margins, so that the one taking the columns' gap back holds.
    assert!(css.contains(".cards-list .results :is(.card-media .card-place,.card-when){display:block;align-self:end;margin-block:0;"));
    assert!(css.contains(".cards-list .results .card-when{grid-area:when;margin-left:-20px;}"));
    assert!(css.contains(".cards-list .results .card-when{margin-left:-14px;}"));
    assert!(css.contains("grid-template-rows:auto auto minmax(0,1fr) auto;"));
    // Roomier, its words the size they were (the user's ask of the same day, "rather the spaces"):
    // on a phone 5px between its lines, 14px between the photo and the words and between the
    // listings; from 720px up 20px and 16px.
    assert!(css.contains(".cards-list .results .grid{grid-template-columns:minmax(0,1fr);row-gap:var(--space-16);}"));
    assert!(css.contains("column-gap:20px;row-gap:6px;") && css.contains("column-gap:14px;row-gap:5px;"));
    assert!(css.contains(".cards-list .results .grid{row-gap:14px;}"));
    assert!(css.contains(".cards-list .results .card-title{font-size:1.0714rem;}") && css.contains(".cards-list .results .card-summary{font-size:.9286rem;}"));
    // A tap anywhere on a card opens its listing but on its heart: the price over the photo lets
    // it through to the photo and the card's link under it, as the photo's other words do (it
    // caught it, and in the list, where its box covers the whole photo, a tap on the photo opened
    // nothing; the user's report of the same day).
    let price = &css[css.find(".card-media .card-price{").expect("the price on the photo")..];
    assert!(price[..price.find('}').unwrap()].contains("pointer-events:none;"), "{price}");
}
