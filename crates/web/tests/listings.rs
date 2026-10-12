//! A listing's page (web/listings.rs, car_prices.rs): its price, its details and its photos; its
//! seller, how quickly they answer and their profile; the phone revealed; the listings like it and
//! those either side of it in its category; the loan calculator, and its price weighed against like
//! cars or its zone's square metre; its map; a price lowered lately; and an old address, sent on.

mod common;

use axum::{body::Body, http::{Request, StatusCode}};
use common::*;
use serde_json::Value;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_sellers_card_says_how_quickly_they_answer_and_when_they_were_here(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let listing = "/listing/1001-volkswagen-golf-7-2016-nafte";
    assert!(!get(pool.clone(), listing).await.body.contains("seller-activity"));
    let seen = |pool: PgPool| async move {
        sqlx::query_as::<_, (Option<time::OffsetDateTime>, time::OffsetDateTime)>("SELECT last_seen_at, updated_at FROM users WHERE id = $1::uuid")
            .bind(ANA_ID)
            .fetch_one(&pool)
            .await
            .unwrap()
    };

    // Signed in, Ana is on the site: today. The moment holds for ten minutes, and the account's
    // own updated_at stays as it was.
    let (_, updated) = seen(pool.clone()).await;
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (first, still) = seen(pool.clone()).await;
    assert!(first.is_some());
    assert_eq!(updated, still);
    sqlx::query("UPDATE users SET last_seen_at = now() - interval '5 minutes' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (kept, _) = seen(pool.clone()).await;
    assert!(kept.unwrap() < time::OffsetDateTime::now_utc() - time::Duration::minutes(4));
    sqlx::query("UPDATE users SET last_seen_at = now() - interval '20 minutes' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (moved, _) = seen(pool.clone()).await;
    assert!(moved.unwrap() > time::OffsetDateTime::now_utc() - time::Duration::minutes(1));
    assert!(get(pool.clone(), listing).await.body.contains("<span class=\"seller-activity\">Aktiv\u{a0}sot</span>"));

    // Three buyers wrote, each answered a quarter of an hour on: she answers within the hour. Two
    // wouldn't have been enough to say so.
    for (i, phone) in ["+355691112233", "+355691112244", "+355691112255"].into_iter().enumerate() {
        let buyer = new_seller(&pool, phone).await;
        let thread = write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await.location.unwrap();
        write(&pool, &thread, "Po, është.", &ana).await;
        sqlx::query(
            "UPDATE messages m SET created_at = m.created_at - interval '15 minutes'
             FROM conversations c WHERE c.id = m.conversation_id AND m.sender_user_id = c.buyer_user_id",
        )
        .execute(&pool)
        .await
        .unwrap();
        pluribus_db::refresh_response_times(&pool).await.unwrap();
        let page = get(pool.clone(), listing).await.body;
        assert_eq!(page.contains("Përgjigjet zakonisht"), i == 2, "{i}");
    }
    let page = get(pool.clone(), listing).await.body;
    assert!(page.contains("<span class=\"seller-activity\">Përgjigjet zakonisht brenda 1 ore ·\u{a0}Aktiv\u{a0}sot</span>"), "{page}");
    let profile = get(pool.clone(), &format!("/perdorues/{ANA_ID}")).await.body;
    assert!(profile.contains("<p class=\"profile-activity\">Përgjigjet zakonisht brenda 1 ore ·\u{a0}Aktiv\u{a0}sot</p>"), "{profile}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_detail_has_its_icon_its_categorys_where_that_says_more(pool: PgPool) {
    // A phone's brand is drawn as a phone, as its category is; a flat's rooms and furnishing say
    // what they are beside their icons, its floor after its name.
    let phone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(phone.contains(concat!(
        r#"<dd class="attribute" title="Marka"><svg class="attribute-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">"#,
        r#"<g transform="scale(.09375)" stroke="none"><path d="M192,56V200H64V56Z"/>"#,
    )));
    // A flat's page is laid out as a car's, as the canvas has it: its rooms, area, floor and
    // furnishing are its key facts in a row under its title, each with its icon, its name over its
    // value (they were pills once, among its details).
    let flat = html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    let start = flat.find(r#"<dl class="key-facts">"#).unwrap();
    let facts = &flat[start..start + flat[start..].find("</dl>").unwrap()];
    assert_eq!(facts.matches(r#"<div class="key-fact">"#).count(), 4, "{facts}");
    assert_eq!(facts.matches(r#"<svg class="key-fact-icon""#).count(), 4, "{facts}");
    for value in ["<dd>2+1</dd>", "<dd>85\u{a0}m²</dd>", "<dd>4</dd>", "<dd>I mobiluar</dd>"] {
        assert!(facts.contains(value), "{value} in {facts}");
    }
    assert!(!flat.contains(r#"<dl class="attributes">"#));
    // Beside a photo, among the pills, the category's key facts, those its cards show, are lit.
    assert!(phone.contains(r#"<dd class="attribute is-key""#), "{phone}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listing_page_shows_price_and_attributes(pool: PgPool) {
    let response = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("9\u{a0}500\u{a0}€"));
    // Each detail a pill, its icon before its value; its name for screen readers and as a
    // tooltip. The options, several, a pill each.
    // Where and when it went up, on a car's page in the line under the title, as the canvas has
    // it (over the photo's corner on every other page): published two hours ago, "Sot"
    // (format::when), or "Dje" in the first two hours of the day; its number shorter at the end of
    // that line.
    let now = time::OffsetDateTime::now_utc();
    let when = pluribus_core::format::when(now - time::Duration::hours(2), now);
    assert!(response.body.contains(&format!(r#" · <span translate="no">Tiranë</span> · <span class="visually-hidden">Publikuar </span>{when} · <span class="listing-number">Nr. 1001</span></p>"#)), "{}", response.body);
    assert!(!response.body.contains(r#"<p class="listing-meta">"#) && !response.body.contains("Numri i njoftimit: 1001"));
    // One photo: no mosaic, the slider alone.
    assert!(!response.body.contains("mosaic-side"));
    // Its condition under the title, not among the details; what kind of listing it is under the
    // price, in the accent's colour (the user's ask of 2026-10-12: it headed the line under the
    // title).
    assert!(response.body.contains(r#"<p class="card-meta listing-kind">E përdorur · <span translate="no">Tiranë</span>"#));
    assert!(response.body.contains(r#"<p class="price-under"><span class="price-kind">Në shitje</span></p>"#), "{}", response.body);
    assert!(!response.body.contains(">Gjendja</dt>"));
    // A car's page, laid out as FINN's: its key facts, the year, the mileage, the gearbox and the
    // fuel, in a row under the title, each with its icon and its name over its value; the rest of
    // its details in a grid after the description, a name with its icon over its value, the facts
    // left out; and its options in a list of their own, each with a check. No pills.
    let facts = &response.body[response.body.find(r#"<dl class="key-facts">"#).expect("the key facts")..];
    let facts = &facts[..facts.find("</dl>").unwrap()];
    let order = positions(facts, &["<dd>2016</dd>", "<dd>148\u{a0}000\u{a0}km</dd>", "<dd>Manuale</dd>", "<dd>Naftë</dd>"]);
    assert!(order.is_sorted(), "{order:?}");
    assert_eq!(facts.matches(r#"<div class="key-fact">"#).count(), 4, "{facts}");
    assert_eq!(facts.matches(r#"<svg class="key-fact-icon""#).count(), 4, "{facts}");
    assert!(facts.contains("<dt>Kilometrazhi</dt><dd>148\u{a0}000\u{a0}km</dd>"), "{facts}");
    assert!(response.body.find("</header>").unwrap() < response.body.find(r#"<dl class="key-facts">"#).unwrap());
    let specs = &response.body[response.body.find(r#"<dl class="spec-grid">"#).expect("the details' grid")..];
    let specs = &specs[..specs.find("</dl>").unwrap()];
    assert!(specs.contains("Karroceria</dt><dd>Hatchback</dd>"), "{specs}");
    assert!(specs.contains(r#"<div class="spec">"#) && specs.contains(r#"<dt><svg class="spec-icon""#), "{specs}");
    assert!(!specs.contains("Kilometrazhi") && !specs.contains("Naftë"), "the facts leave the details: {specs}");
    // The map under its own heading, "Lokacioni", on a car's page.
    let order = positions(&response.body, &[r#"<section class="listing-map-section""#, "<h2>Lokacioni</h2>", r#"<div class="listing-map""#]);
    assert!(order.is_sorted(), "{order:?}");
    // What qualifies the price on a line of its own under it, without its brackets, out of the
    // price's line, so that the figure stands level with its buttons (the user's ask of
    // 2026-10-11: "separate stuff like me qera, i diskutueshem from the price, and put it right
    // below, very close to the price").
    let mercedes = html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await;
    assert!(mercedes.contains(r#"<p class="price-under"><span class="price-kind">Në shitje</span><span class="price-note">i diskutueshëm</span></p>"#), "{mercedes}");
    let line = &mercedes[mercedes.find(r#"<p class="listing-price">"#).expect("the price")..];
    assert!(line[..line.find("</p>").unwrap()].ends_with("</span>") && !line[..line.find("</p>").unwrap()].contains("price-note"));
    // And on every other listing's page, on every screen (the user's ask of 2026-10-11): it was
    // after the figure there, half its size and in brackets.
    let sofa = html(pool.clone(), "/listing/1007-divan-qoshe-ngjyre-gri").await;
    assert!(sofa.contains(r#"<span class="price-note">i diskutueshëm</span>"#) && !sofa.contains(r#"class="price-note"> ("#), "{sofa}");
    let site_css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    // What kind of listing it is first on that line, semibold, in the accent's colour: its shade
    // for words on the light theme, the accent's line on the dark (the user's ask of 2026-10-12).
    assert!(site_css.contains(".price-under{margin:-2px 0 0;color:var(--muted);font-size:var(--lp-small);line-height:1.4;}"));
    assert!(site_css.contains(".price-kind{margin-right:.5rem;color:var(--highlight-text);font-weight:var(--semibold);}.price-note{margin-right:.5rem;}"));
    // Its shade 45% of the accent in our near-black, lime's 38%, 5:1 at least on the page.
    assert!(site_css.contains("--highlight-text:color-mix(in srgb,var(--highlight) var(--text-share),#1b1b1b);"));
    assert_eq!(site_css.matches("--highlight-text:var(--highlight-line);").count(), 2, "the dark theme's, picked and the device's");
    // The price's buttons as tall as its line, 36px, level with the figure (the user's ask of the
    // same day, "a bit smaller, size of the row of the price so they are more in line"; a finger
    // has 40px of each), and the seller under a hairline, with no heading.
    for rule in [
        ".price-row{position:relative;display:flex;align-items:center;gap:8px;min-height:var(--size-chip);}",
        ".price-row > .bubble > summary::after{content:\"\";position:absolute;inset:-2px;}",
        ".loan > summary{justify-content:center;width:var(--size-chip);height:var(--size-chip);padding:0;}",
        ".seller-section > h2,.seller-section > .seller-card:first-child > .seller-head{padding-top:1.25rem;border-top:1px solid var(--border);}",
    ] {
        assert!(site_css.contains(rule), "{rule}");
    }
    assert!(!site_css.contains(".listing-car .price-note"));
    assert!(!response.body.contains(r#"<dd class="attribute"#), "no pills on a car's page");
    assert!(response.body.contains("</svg>ABS</li>") && response.body.contains("</svg>Kondicioner</li>"));
    let order = positions(&response.body, &["<h2>Përshkrimi</h2>", "<h2>Detajet</h2>", r#"<section class="listing-options">"#, r#"<ul class="options">"#]);
    assert!(order.is_sorted(), "{order:?}");
    // Where it is, as a category page says it and the canvas draws it: a line of words, the way
    // back first, leading to its category without scripts, and no house (the logo leads to the
    // welcome page), then the category above its own, quieter, and its own, each a link to its
    // page.
    let path = positions(&response.body, &[
        r#"<nav class="breadcrumb listing-path" aria-label="Kategoria e njoftimit">"#,
        r#"<a class="breadcrumb-back" href="/c/makina" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#,
        r#"<a class="breadcrumb-up" href="/c/automjete">Automjete</a>"#,
        r#"<span class="breadcrumb-sep" aria-hidden="true">›</span>"#,
        r#"<a class="breadcrumb-here" href="/c/makina">Makina</a>"#,
    ]);
    assert!(path.is_sorted(), "{path:?}");
    // Its own underlined in the accent, as the site marks where the reader is (the user's ask of
    // 2026-10-12; under the pointer only until then).
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    let underlined = ".listing-path .breadcrumb-here{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}";
    assert!(css.contains(underlined), "the listing's own category underlined");
    assert!(!response.body.contains("Faqja kryesore"));
    assert!(!response.body.contains("breadcrumbs"));
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=1200"));
    // Three levels down, the category above its own and its own, as on a category page.
    sqlx::query("UPDATE listings SET category_id = (SELECT id FROM categories WHERE slug = 'gps-dhe-navigatore') WHERE public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    let gps = get(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let path = positions(&gps, &[
        r#"<a class="breadcrumb-back" href="/c/gps-dhe-navigatore" aria-label="Kthehu""#,
        r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">Pjesë</a>"#,
        r#"<a class="breadcrumb-here" href="/c/gps-dhe-navigatore">GPS</a>"#,
    ]);
    assert!(path.is_sorted(), "{path:?}");
    assert!(!gps.contains("Faqja kryesore"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_photos_are_a_mosaic(pool: PgPool) {
    // Four photos: from 960 px up the slider, and the next two beside it, each going to its photo
    // in the slider, the second saying how many more there are and showing the filmstrip too.
    for (at, name) in ["two", "three", "four"].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url, width_px, height_px, sort_order)
             SELECT id, 'IMAGE', 'READY', $1, $2, $3, 1200, 900, $4 FROM listings WHERE public_no = 1001",
        )
        .bind(format!("test/{name}"))
        .bind(format!("https://example.com/{name}.jpg"))
        .bind(format!("https://example.com/{name}-card.jpg"))
        .bind(at as i32 + 1)
        .execute(&pool)
        .await
        .unwrap();
    }
    let page = html(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let side = &page[page.find(r#"<div class="mosaic-side">"#).expect("the mosaic's side")..];
    let side = &side[..side.find("</div>").unwrap()];
    assert_eq!(side.matches(r#"class="mosaic-tile""#).count(), 2, "{side}");
    assert!(side.contains(r#"<button type="button" class="mosaic-tile" data-action="slides#tile" data-slides-index-param="1" aria-label="Foto 2 nga 4">"#), "{side}");
    assert!(side.contains(r#"data-slides-index-param="2" data-slides-more-param="true" aria-label="Foto 3 nga 4">"#), "{side}");
    assert!(side.contains(r#"<span class="mosaic-more" aria-hidden="true">+1</span>"#), "{side}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_steps_to_its_neighbours_on_its_categorys_page(pool: PgPool) {
    // At the right end of the path, after its pills, three round buttons like the back one: to
    // the category's list, to the listing before this one on it and to the one after it, in the
    // list's own order, newest first. The Golf is Makina's newest, so it has nothing before it,
    // the button there greyed and no link, and the Mercedes after it; the Mercedes has the Golf
    // before it and nothing after.
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let mercedes = "/listing/1002-mercedes-benz-c-220-2018-automatike";
    let off_before = r#"<span class="back-button step-button is-off" aria-hidden="true"><svg data-icon="chevron-left""#;
    let off_after = r#"<span class="back-button step-button is-off" aria-hidden="true"><svg data-icon="chevron-right""#;
    let to_golf = format!(r#"<a class="back-button step-button" href="{golf}" aria-label="Njoftimi i mëparshëm" title="Njoftimi i mëparshëm"><svg data-icon="chevron-left""#);
    let to_mercedes = format!(r#"<a class="back-button step-button" href="{mercedes}" aria-label="Njoftimi tjetër" title="Njoftimi tjetër"><svg data-icon="chevron-right""#);
    let page = html(pool.clone(), golf).await;
    // The path at the page's top, as the canvas starts every listing (a banner's room stood over
    // it once).
    let top = positions(&page, &[r#"<article class="listing"#, r#"<div class="listing-tags">"#]);
    assert!(top.is_sorted(), "{top:?}");
    assert!(!page.contains("listing-banner"));
    let steps = positions(&page, &[
        "Makina</a>",
        r#"<nav class="listing-steps" aria-label="Njoftimet e tjera">"#,
        r#"<a class="back-button list-button" href="/c/makina" aria-label="Lista e njoftimeve" title="Lista e njoftimeve"><svg data-icon="list""#,
        off_before,
        &to_mercedes,
    ]);
    assert!(steps.is_sorted(), "{steps:?}");
    assert!(!page.contains("Njoftimi i mëparshëm"));
    let page = html(pool.clone(), mercedes).await;
    let steps = positions(&page, &[&to_golf, off_after]);
    assert!(steps.is_sorted(), "{steps:?}");
    assert!(!page.contains("Njoftimi tjetër"));
    // Sold, the Golf is off the list, so the Mercedes has nothing before it; the Golf's own page
    // still steps to those either side of where it was.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001").execute(&pool).await.unwrap();
    let page = html(pool.clone(), mercedes).await;
    assert!(page.contains(off_before) && !page.contains(&to_golf));
    assert!(html(pool.clone(), golf).await.contains(&to_mercedes));
    sqlx::query("UPDATE listings SET status = 'ACTIVE', sold_at = NULL WHERE public_no = 1001").execute(&pool).await.unwrap();
    // Bumped, the Mercedes goes first: the Golf then has it before, and nothing after.
    sqlx::query("UPDATE listings SET bumped_at = now() WHERE public_no = 1002").execute(&pool).await.unwrap();
    let page = html(pool.clone(), golf).await;
    let mercedes_before = to_mercedes.replace("Njoftimi tjetër", "Njoftimi i mëparshëm").replace("chevron-right", "chevron-left");
    assert!(page.contains(&mercedes_before) && page.contains(off_after));
    // For Besnik, who blocked Ana, her Golf isn't on the list: the Mercedes has nothing after it.
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&besnik)).await;
    let golf_after = to_golf.replace("Njoftimi i mëparshëm", "Njoftimi tjetër").replace("chevron-left", "chevron-right");
    assert!(html(pool.clone(), mercedes).await.contains(&golf_after));
    assert!(get_with_cookie(pool.clone(), mercedes, &besnik).await.body.contains(off_after));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn costly_things_on_sale_offer_a_loan_calculator(pool: PgPool) {
    let car = html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await;
    // A circle at the price's line's end, its calculator's icon alone ("Llogarit kredinë" said to
    // screen readers and under the pointer: the user's ask of 2026-10-11), before the price per
    // square metre's, which opens the calculator in a card under the line the same way: the years
    // and the interest, and what they come to, the down payment a set share of the price (what the
    // banks ask, LOAN_DOWN_PERCENT), the monthly payment last. On a car's page too.
    let start = r#"<details class="bubble loan" data-controller="dropdown loan" data-loan-price-value="21500.00" data-loan-currency-value="€" data-loan-down-percent-value="35">"#;
    let loan = &car[car.find(start).expect("the loan calculator's bubble")..];
    let loan = &loan[..loan.find("</details>").unwrap()];
    let summary = &loan[..loan.find("</summary>").unwrap()];
    assert!(summary.contains(r#"<summary title="Llogarit kredinë">"#) && summary.contains(r#"<span class="visually-hidden">Llogarit kredinë</span>"#), "{summary}");
    let row = &car[car.find(r#"<div class="price-row">"#).expect("the price's row")..];
    assert!(row.find(r#"<p class="listing-price">"#).unwrap() < row.find(start).unwrap(), "the calculator after the price, on its line");
    assert!(!loan.contains("loan-down\""), "no field for the down payment");
    let card = positions(loan, &[
        r#"<div class="bubble-card loan-card">"#,
        r#"<input id="loan-years""#,
        r#"<input id="loan-rate""#,
        "<dt>Paradhënia (35%)</dt>",
        "<dt>Kredia</dt>",
        "<dt>Kësti mujor</dt>",
    ]);
    assert!(card.is_sorted(), "{card:?}");
    assert!(car.find(r#"<div class="price-box">"#).unwrap() < car.find(start).unwrap() && !car.contains("listing-loan"));
    // Anything on sale for more than 500 € has it, the Golf at 9 500 € too; a 4 500 Lekë jacket
    // doesn't.
    assert!(html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.contains("Llogarit kredinë"));
    assert!(!html(pool.clone(), "/listing/1012-xhakete-dimri-per-meshkuj-masa-m").await.contains("data-loan-price-value"));
    // In Lekë, the limit is about the same as in euros: 50 000 Lekë isn't more than it, 50 001 is.
    for (price, offered) in [(50_000, false), (50_001, true)] {
        sqlx::query("UPDATE listings SET price_amount = $1::numeric, currency = 'ALL', price_type = 'FIXED' WHERE public_no = 1005")
            .bind(price)
            .execute(&pool)
            .await
            .unwrap();
        let page = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
        assert_eq!(page.contains("Llogarit kredinë"), offered, "{price}");
    }
    // A rent is paid monthly already.
    let rent = html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    // What kind it is under the price, smaller, off the band of the accent across the figure's
    // foot (as Tradera marks its prices, 2026-10-11) and out of the price's line; no month there
    // (the user's ask of 2026-10-12, "remove the / month marking": its card keeps it); a rent
    // says nothing of square metres.
    assert!(rent.contains("<p class=\"listing-price\"><span class=\"price-amount\">450\u{a0}€</span></p>"));
    let under = rent.find("<p class=\"price-under\"><span class=\"price-kind\">Me qira</span></p>").expect("the kind under the price");
    let box_at = rent.find("<div class=\"price-box\">").unwrap();
    assert!(rent.find("<div class=\"price-row\">").unwrap() < under && !rent[box_at..under + 60].contains("/muaj"));
    assert!(!rent.contains("Llogarit kredinë"));
    assert!(!rent.contains("price-per-area"));
    // A house on sale says what a square metre costs, 185 000 € for 200 m², in a circle at its
    // price's line's end after the loan calculator's, its icon alone (the figure said to screen
    // readers and under the pointer, as a car's price has it: the user's asks of 2026-10-11), which
    // opens how it was worked out.
    let house = html(pool, "/listing/1004-shtepi-private-ne-shitje-ne-vlore").await;
    let row = &house[house.find(r#"<div class="price-row">"#).expect("the price's row")..];
    assert!(row.contains("<p class=\"listing-price\"><span class=\"price-amount\">185\u{a0}000\u{a0}€</span></p>"));
    assert!(row.find(r#"<details class="bubble loan""#).unwrap() < row.find(r#"<details class="bubble area-price""#).unwrap());
    let bubble = &row[row.find(r#"<details class="bubble area-price" data-controller="dropdown">"#).expect("the bubble")..];
    let summary = &bubble[..bubble.find("</summary>").unwrap()];
    assert!(summary.contains("<summary title=\"Si llogaritet: 925\u{a0}€/m²\">") && summary.contains("<span class=\"visually-hidden\">925\u{a0}€/m²</span>"), "{summary}");
    for line in ["<dd>185\u{a0}000\u{a0}€</dd>", "<dd>200\u{a0}m²</dd>", "<dd>925\u{a0}€/m²</dd>", "rrumbullakuar në euro."] {
        assert!(bubble.contains(line), "{line}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn outdated_slug_redirects_to_the_current_one(pool: PgPool) {
    let response = get(pool, "/listing/1001-old-title").await;
    assert_eq!(response.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.location.as_deref(),
        Some("/listing/1001-volkswagen-golf-7-2016-nafte")
    );
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_shows_listings_like_it(pool: PgPool) {
    // Three more Golfs from Besnik at about the Golf's price, Ana's own second Golf, and an Audi
    // from Dritan, all in Makina in Tirana.
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
             ('33333333-3333-4333-8333-333333333333', 'audi-a3', 'Audi A3', 9600)
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
    let number = |slug: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT public_no FROM listings WHERE slug = $1")
                .bind(slug)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let (golf_a, golf_b, audi) = (number("golf-a").await, number("golf-b").await, number("audi-a3").await);

    // Under the Golf: the other sellers' cars, the same model first, the closest in price before;
    // two of Besnik's three Golfs, none of Ana's own, not the Golf itself, and no flats. The
    // Mercedes comes last: it shares only the fuel, costs twice as much, and is in Durrës.
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let related = &page[page.find(r#"<section class="related" aria-labelledby="related-heading">"#).expect("related")..];
    let related = &related[..related.find("</section>").unwrap()];
    assert!(related.contains(r#"<h2 id="related-heading">Njoftime të ngjashme</h2>"#));
    assert_eq!(card_numbers(related), vec![golf_a, golf_b, audi, 1002]);
    // Four being all of them, no frame asks for more. The frame's answer, from the third on, in
    // the frame the page would have named, with no frame after it either; from the first, all
    // four. Asked for as a page, it leads back to the listing.
    assert!(!related.contains("page-related-"), "{related}");
    let frame = |from: i64| {
        let pool = pool.clone();
        async move {
            let request = Request::get(format!("/listing/1001/te-ngjashme?nga={from}"))
                .header("turbo-frame", format!("page-related-{from}"))
                .body(Body::empty())
                .unwrap();
            send(pool, request).await.body
        }
    };
    let rest = frame(2).await;
    assert!(rest.starts_with(r#"<turbo-frame id="page-related-2" target="_top">"#), "{rest}");
    assert_eq!(card_numbers(&rest), vec![audi, 1002]);
    assert!(!rest.contains("page-related-4"));
    assert_eq!(card_numbers(&frame(0).await), vec![golf_a, golf_b, audi, 1002]);
    let page = get(pool.clone(), "/listing/1001/te-ngjashme?nga=2").await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert_eq!(page.location.as_deref(), Some("/listing/1001"));

    // A listing alone in its category has nothing under it.
    let dog = html(pool, "/listing/1009-kelysh-labrador-3-muajsh").await;
    assert!(!dog.contains("related-heading"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_or_a_car_shows_where_it_is_on_a_map_under_its_seller(pool: PgPool) {
    // The flat and the Golf, in Tirana: a map round their point, the town's, the town named over it,
    // under the seller's card.
    for uri in ["/listing/1003-apartament-2-1-me-qira-ne-tirane", "/listing/1001-volkswagen-golf-7-2016-nafte"] {
        let page = html(pool.clone(), uri).await;
        let map = page.find(r#"<section class="listing-map-section" aria-label="Vendndodhja""#).unwrap_or_else(|| panic!("{uri}"));
        assert!(page.find(r#"<section class="seller-section""#).unwrap() < map, "{uri}");
        let section = &page[map..map + page[map..].find("</section>").unwrap()];
        assert!(section.contains(r#"<div class="listing-map" data-controller="listing-map""#), "{section}");
        assert!(section.contains(r#"data-listing-map-latitude-value="41.328333" data-listing-map-longitude-value="19.818056""#), "{section}");
        assert!(section.contains(r#"<p class="listing-map-place" translate="no">Tiranë</p>"#), "{section}");
    }
    // A phone's page has none.
    assert!(!html(pool, "/listing/1005-iphone-13-128-gb-si-i-ri").await.contains("listing-map"));
}

/// Golfs up for sale, as 1001 is but for their years and their prices in euros.
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_price_is_compared_with_what_cars_of_its_model_and_year_ask(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let refresh = || pluribus_db::refresh_car_prices(&pool);
    // Its price, now, with no history of it: a drop would have its own pill on the card.
    let priced = |price: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE listings SET price_amount = $1::numeric, price_normalized = $1::numeric WHERE public_no = 1001")
                .bind(price)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("DELETE FROM listing_price_history WHERE listing_id = 'a0000000-0000-4000-8000-000000001001'").execute(&pool).await.unwrap();
        }
    };
    let hint = |query: &'static str| {
        let pool = pool.clone();
        async move { get(pool, &format!("/shto/cmimi?{query}")).await }
    };

    // Too few Golfs to say anything.
    refresh().await.unwrap();
    assert!(!get(pool.clone(), golf).await.body.contains("car-price"));
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.status, StatusCode::NO_CONTENT);

    // Nine more, from 2014 to 2018: ten, whose median is 10 250 €, the middle half's 9 500 to
    // 11 375; its 9 500 € is about it.
    golfs(&pool, &[(2014, 8000), (2015, 9000), (2015, 9500), (2016, 10000), (2016, 10500), (2017, 11000), (2017, 11500), (2018, 12000), (2018, 12500)]).await;
    refresh().await.unwrap();
    let page = get(pool.clone(), golf).await.body;
    assert!(page.contains(r#"<details class="bubble car-price price-near" data-controller="dropdown">"#), "{page}");
    assert!(page.contains("Rreth mesatares"));
    // A circle at the price's line's end, after the loan calculator's, its tag alone, on the verdict's colour (a light
    // one under our black on the light theme, a deeper one under our white on the dark), the
    // verdict said to screen readers and under the pointer, and first in its card: the user's ask
    // of 2026-10-11.
    assert!(page.contains(r#"<div class="price-row">"#), "{page}");
    assert!(page.find(r#"<details class="bubble loan""#).unwrap() < page.find(r#"<details class="bubble car-price"#).unwrap());
    assert!(page.contains(r#"<summary title="Si krahasohet: Rreth mesatares">"#), "{page}");
    assert!(page.contains(r#"<span class="visually-hidden">Rreth mesatares</span>"#), "{page}");
    assert!(page.contains(r#"<p class="car-price-verdict">Rreth mesatares</p>"#), "{page}");
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    assert!(css.contains(":is(.car-price,.area-price):is(.price-below,.price-near,.price-above) > summary{background:var(--tone-fill);color:var(--text);}"));
    assert!(css.contains("--near-fill:#f8e3a0;") && css.contains("--near-fill:#85660a;"));
    assert!(page.contains("<div><dt>Mesatarja</dt><dd>10\u{a0}300\u{a0}€</dd></div>"), "{page}");
    assert!(page.contains("<div><dt>Shumica kërkojnë</dt><dd>9\u{a0}500–11\u{a0}400\u{a0}€</dd></div>"), "{page}");
    assert!(page.contains("Nga 10 Volkswagen Golf 2014–2018 në shitje, pa makinat për pjesë."), "{page}");
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("card-bargain"));

    // At 8 000 €, against the median as it stands (no rebuild), a good price, on its card too; at
    // 13 000 €, over it; at 4 000 €, so far under it that it says nothing.
    priced(8000).await;
    let page = get(pool.clone(), golf).await.body;
    assert!(page.contains(r#"<details class="bubble car-price price-below""#) && page.contains("Çmim i mirë"), "{page}");
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert_eq!(cars.matches(r#"<span class="card-bargain">Çmim i mirë</span>"#).count(), 1, "{cars}");
    priced(13000).await;
    assert!(get(pool.clone(), golf).await.body.contains(r#"<details class="bubble car-price price-above""#));
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("card-bargain"));
    priced(4000).await;
    assert!(!get(pool.clone(), golf).await.body.contains("car-price"));

    // The listing form's hint: what such cars ask, and how the price typed will compare, in the
    // currency chosen; too few cars, or no model, and it says nothing.
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=8%20000").await.body).unwrap();
    assert_eq!(answer["text"], "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9\u{a0}500–11\u{a0}400\u{a0}€, mesatarja 10\u{a0}300\u{a0}€.");
    assert_eq!(answer["verdict"], "Te faqja e makinës, çmimi yt del „Çmim i mirë“.");
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=900").await.body).unwrap();
    assert_eq!(answer["verdict"], "Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?");
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.body).unwrap();
    assert_eq!(answer["verdict"], Value::Null);
    let lek: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=ALL&price=1000000").await.body).unwrap();
    assert!(lek["text"].as_str().unwrap().ends_with("0\u{a0}Lekë."), "{lek}");
    assert_eq!(lek["verdict"], "Te faqja e makinës, çmimi yt del „Rreth mesatares“.");
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=2025&currency=EUR").await.status, StatusCode::NO_CONTENT);
    assert_eq!(hint("model=BMW_3_SERIES&year=2016&currency=EUR").await.status, StatusCode::NO_CONTENT);
    assert_eq!(hint("year=2016").await.status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_in_a_zone_compares_its_square_metre_with_the_zones(pool: PgPool) {
    // Eleven flats on sale in Blloku, 100 m² each: ten from 700 to 1 200 € a square metre, and one
    // at 5 000 € as a mistyped price would have it. The zone's average leaves out the cheapest and
    // the dearest tenth, so the mistake doesn't pull it: it is 1 000 €/m².
    sqlx::query(
        "INSERT INTO listings (public_no, seller_user_id, created_by_user_id, category_id, location_id, slug,
                               title, description, listing_type, price_amount, price_normalized, currency,
                               price_type, status, published_at)
         OVERRIDING SYSTEM VALUE
         SELECT 2000 + n, '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                (SELECT id FROM categories WHERE slug = 'apartamente'),
                (SELECT id FROM locations WHERE slug = 'blloku' AND location_type = 'AREA'),
                'apartament-' || n, 'Apartament ' || n, 'Apartament në Bllok.', 'SELL',
                price, price, 'EUR', 'FIXED', 'ACTIVE', now()
         FROM (VALUES (1, 70000), (2, 80000), (3, 89000), (4, 100000), (5, 100000), (6, 100000),
                      (7, 100000), (8, 100000), (9, 111000), (10, 120000), (11, 500000)) AS v (n, price)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
         SELECT l.id, a.id, a.data_type, 100
         FROM listings l JOIN attributes a ON a.code = 'property_area'
         WHERE l.public_no BETWEEN 2001 AND 2011",
    )
    .execute(&pool)
    .await
    .unwrap();
    pluribus_db::refresh_zone_prices(&pool).await.unwrap();

    let page = |n: i64| format!("/listing/{n}-apartament-{}", n - 2000);
    // The bubble under the price, from its opening tag to its end.
    let bubble = |html: String| {
        let start = html.find(r#"<details class="bubble area-price"#).expect("the bubble");
        html[start..start + html[start..].find("</details>").unwrap()].to_string()
    };

    // 200 € a square metre under the average, or more: green, and the card says the average, by
    // how much, and what the average is worked out from.
    let cheap = bubble(html(pool.clone(), &page(2002)).await);
    assert!(cheap.starts_with(r#"<details class="bubble area-price price-below" data-controller="dropdown">"#), "{cheap}");
    for line in [
        "<dd>800\u{a0}€/m²</dd>",
        "<dt>Mesatarja e zonës</dt><dd>1\u{a0}000\u{a0}€/m²</dd>",
        "Nën mesataren: 200\u{a0}€/m² më lirë",
        "Mesatarja: 11 njoftime në shitje në Blloku (Apartamente), pa çmimet më të ulëta e më të larta.",
    ] {
        assert!(cheap.contains(line), "{line}");
    }
    // Less than 200 € from it either way: yellow.
    let near = bubble(html(pool.clone(), &page(2003)).await);
    assert!(near.starts_with(r#"<details class="bubble area-price price-near""#));
    assert!(near.contains("Afër mesatares: 110\u{a0}€/m² më lirë"));
    let even = bubble(html(pool.clone(), &page(2004)).await);
    assert!(even.starts_with(r#"<details class="bubble area-price price-near""#));
    assert!(even.contains("Njësoj si mesatarja"));
    // 200 € over it, or more: red.
    let dear = bubble(html(pool.clone(), &page(2010)).await);
    assert!(dear.starts_with(r#"<details class="bubble area-price price-above""#));
    assert!(dear.contains("Mbi mesataren: 200\u{a0}€/m² më shtrenjtë"));

    // A price in Lekë meets the average in Lekë: 1 000 € is 100 000 Lekë.
    sqlx::query("UPDATE listings SET price_amount = 10000000, currency = 'ALL' WHERE public_no = 2004")
        .execute(&pool)
        .await
        .unwrap();
    let in_lek = bubble(html(pool.clone(), &page(2004)).await);
    assert!(in_lek.contains("<dt>Mesatarja e zonës</dt><dd>100\u{a0}000\u{a0}Lekë/m²</dd>"), "{in_lek}");
    assert!(in_lek.contains("Njësoj si mesatarja"));

    // A starting price says too little to compare.
    sqlx::query("UPDATE listings SET price_type = 'FROM' WHERE public_no = 2010").execute(&pool).await.unwrap();
    let from = bubble(html(pool.clone(), &page(2010)).await);
    assert!(from.starts_with(r#"<details class="bubble area-price" data-controller="dropdown">"#), "{from}");
    assert!(!from.contains("Mesatarja"));

    // Fewer than five sales in the zone make no average.
    sqlx::query("DELETE FROM listings WHERE public_no BETWEEN 2001 AND 2008").execute(&pool).await.unwrap();
    pluribus_db::refresh_zone_prices(&pool).await.unwrap();
    let alone = bubble(html(pool.clone(), &page(2009)).await);
    assert!(alone.starts_with(r#"<details class="bubble area-price" data-controller="dropdown">"#), "{alone}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn revealing_a_phone_returns_a_turbo_stream_and_counts_it(pool: PgPool) {
    let request = Request::post("/listing/1001/phone").body(Body::empty()).unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.content_type.as_deref(),
        Some("text/vnd.turbo-stream.html; charset=utf-8")
    );
    assert!(response.body.contains(r#"<turbo-stream action="replace" target="phone-reveal">"#));
    assert!(response.body.contains(r#"href="tel:+355690000001" translate="no"><svg class="contact-icon""#));
    assert!(response.body.contains("</svg>+355 69 000 0001</a>"));
    // Under the price alone: the seller's card at the foot has no buttons of its own.
    assert_eq!(response.body.matches("<turbo-stream ").count(), 1);
    // Beside it, a button that copies it, as a phone dials it.
    assert!(response.body.contains(r#"data-controller="copy" data-copy-text-value="+355690000001""#));
    assert!(response.body.contains(r#"aria-label="Kopjo numrin""#));
    // Then a circle that opens a WhatsApp chat with it, in a new tab or the app, a first message
    // naming the listing and its address typed in, percent-encoded (a space %20, not "+"). The
    // number was in no page until the reveal, which counted it.
    let whatsapp = r#"<a class="button secondary icon-circle whatsapp-button" href="https://wa.me/355690000001?text=P%C3%ABrsh%C3%ABndetje%21%20Ju%20shkruaj%20p%C3%ABr%20njoftimin%20%E2%80%9E"#;
    assert!(response.body.contains(whatsapp), "{}", response.body);
    assert!(response.body.contains(r#"%E2%80%9C%20n%C3%AB%20Pluribus%3A%20http%3A%2F%2F"#), "{}", response.body);
    assert!(response.body.contains(r#"%2Flisting%2F1001-volkswagen-golf-7-2016-nafte" target="_blank" rel="noopener" aria-label="Shkruaj në WhatsApp" title="Shkruaj në WhatsApp">"#), "{}", response.body);
    assert!(response.body.find("copy-button").unwrap() < response.body.find("whatsapp-button").unwrap());

    let reveals: i64 = sqlx::query_scalar("SELECT phone_reveals FROM listing_daily_stats")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reveals, 1);

    // An Albanian landline (+355 4..., Tirana's) has no WhatsApp: the number and its copy alone.
    sqlx::query(
        "UPDATE listing_contacts SET phone_e164 = '+35542234567'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1003)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = Request::post("/listing/1003/phone").body(Body::empty()).unwrap();
    let landline = send(pool, request).await.body;
    assert!(landline.contains(r#"href="tel:+35542234567""#) && landline.contains("copy-button"), "{landline}");
    assert!(!landline.contains("whatsapp"), "{landline}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_listing_shows_its_seller_and_their_profile_lists_their_listings(pool: PgPool) {
    // A private seller: Ana, with the five fixture listings she sells.
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let ana = "/perdorues/11111111-1111-4111-8111-111111111111";
    // The category above the photo; the title under it; then the deal, the price in its box, on a
    // car's page the seller's card, under a hairline with no heading ("Shitësi" its name to screen
    // readers; the user's ask of 2026-10-11, "remove shitesi label and separate shitesi with a
    // line on top"), her initial, her name, since when she is on the site and after
    // a dot how many listings, leading to her profile, and in the card the contact bar, the phone
    // and the message, wide pills (contact_bar.html; under the price on every other page), and
    // the share, a link in a circle, and the report, beside the message (listing.css); then the
    // description and the details; then her other listings.
    let order = positions(&page, &[
        r#"<nav class="breadcrumb listing-path""#,
        r#"<div class="gallery">"#,
        "<h1>Volkswagen Golf 7, 2016, naftë</h1>",
        r#"<p class="card-meta listing-kind">E përdorur · "#,
        r#"<dl class="key-facts">"#,
        r#"<div class="price-box">"#,
        r#"<p class="listing-price">"#,
        r#"<div class="listing-deal""#,
        r#"<section class="seller-section" aria-label="Shitësi">"#,
        r#"<div class="seller-card">"#,
        &format!(r#"<a class="seller" href="{ana}">"#),
        r#"<span class="avatar large" aria-hidden="true">A</span>"#,
        r#"<span class="seller-name"><span translate="no">Ana</span></span>"#,
        r#"<span class="seller-summary"><span class="seller-since">Që nga "#,
        r#"</span> <span class="seller-count">· 5 njoftime</span></span>"#,
        r#"<aside class="contact-bar" aria-label="Kontakti" data-controller="sticky">"#,
        r#"<form id="phone-reveal" method="post""#,
        "</aside>",
        r#"<div class="listing-share">"#,
        r#"<div class="actions">"#,
        r#"aria-label="Shpërndaje" title="Shpërndaje">"#,
        "<h2>Përshkrimi</h2>",
        "<h2>Detajet</h2>",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    // The description can fold past a dozen lines (fold_controller.js): its button waits hidden,
    // and nothing is folded in the page itself.
    assert!(page.contains(r#"<div class="description-fold" data-controller="fold">"#));
    assert!(page.contains(r#"<button type="button" class="description-more" data-fold-target="button" data-action="fold#open" hidden>"#));
    assert!(page.contains("Shfaq të gjithë përshkrimin") && !page.contains("is-folded"));
    // The bar is under the seller, the page's one bar, and the share and the report in a section
    // of their own after the seller's (the user's asks of 2026-10-11); on every other page too,
    // the seller after the description, said the same way: the initial in a ring of 48 px, since
    // when, and how many listings, the bar under them.
    let card = &page[page.find(r#"<section class="seller-section""#).unwrap()..];
    let card = &card[..card.find("</section>").unwrap()];
    assert!(card.contains(r#"<aside class="contact-bar""#) && page.matches(r#"<aside class="contact-bar""#).count() == 1, "{card}");
    assert!(!card.contains(r#"<div class="actions">"#) && page.matches(r#"<div class="actions">"#).count() == 1, "the share and the report out of the card: {card}");
    let iphone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    let card = &iphone[iphone.find(r#"<section class="seller-section""#).unwrap()..];
    let card = &card[..card.find("</section>").unwrap()];
    assert!(card.contains(r#"<span class="avatar large" aria-hidden="true">A</span>"#) && card.contains(r#"<aside class="contact-bar""#), "{card}");
    assert!(card.contains(r#"<span class="seller-summary"><span class="seller-since">Që nga "#) && card.contains(r#"<span class="seller-count">· 5 njoftime</span>"#), "{card}");
    let order = positions(&iphone, &[
        "<h2>Përshkrimi</h2>",
        r#"<section class="seller-section""#,
        r#"<aside class="contact-bar""#,
        r#"<div class="listing-share">"#,
        r#"aria-label="Shpërndaje" title="Shpërndaje">"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert_eq!(iphone.matches(r#"<aside class="contact-bar""#).count(), 1);

    // Her profile says so too.
    let profile = get(pool.clone(), ana).await.body;
    assert!(profile.contains(r#"<h1 translate="no">Ana</h1>"#));
    assert!(profile.contains("Në Pluribus që nga ") && profile.contains(" · 5 njoftime</p>"));
    assert_eq!(card_numbers(&profile).len(), 5);
    assert!(profile.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!profile.contains("Mercedes-Benz C 220"), "Auto Durrësi's, not Ana's");

    // A business: its listings, on its own page.
    let page = get(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.body;
    assert!(page.contains(r#"<a class="seller" href="/biznes/auto-durresi">"#));
    // A business says so on the since line of a car's card.
    assert!(page.contains(r#"<span class="seller-summary"><span class="seller-since">Biznes · që nga "#) && page.contains(r#"</span> <span class="seller-count">· 1 njoftim</span></span>"#), "{page}");
    let business = get(pool.clone(), "/biznes/auto-durresi").await.body;
    assert!(business.contains(r#"<h1 translate="no">Auto Durrësi</h1>"#) && business.contains("Biznes · "));
    assert_eq!(card_numbers(&business), vec![1002]);

    // No such seller, or one not yet confirmed.
    assert_eq!(get(pool.clone(), "/perdorues/not-a-uuid").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(pool.clone(), "/biznes/nobody").await.status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE users SET status = 'PENDING' WHERE display_name = 'Ana'").execute(&pool).await.unwrap();
    assert_eq!(get(pool, ana).await.status, StatusCode::NOT_FOUND);
}

/// Sets a listing's price, as its seller's edit does, which listing_price_history records.
async fn set_price(pool: &PgPool, number: i64, amount: &str) {
    sqlx::query("UPDATE listings SET price_amount = $2::numeric WHERE public_no = $1")
        .bind(number)
        .bind(amount)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_but_a_cars_or_a_propertys_stands_beside_its_photo(pool: PgPool) {
    // From 960 px up the page is split (listing.css, .listing-split): the photo at the left and
    // the rest, .listing-side, at its right, the seller's card in it (its photos sized for seven
    // twelfths of the page, SPLIT_GALLERY_SIZES: listings_get_photos). A car's or a property's page keeps its
    // photos across the page, and the map under the seller (wide_photos in listings.rs).
    let phone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(phone.contains(r#"<article class="listing listing-split""#), "{phone}");
    let side = &phone[phone.find(r#"<div class="listing-side">"#).unwrap()..];
    let article_end = side.find("</article>").unwrap();
    assert!(side.find(r#"<section class="seller-section" aria-label="Shitësi">"#).unwrap() < article_end);
    assert!(!phone.contains("seller-heading"), "no heading over the seller but on their own car's or property's page");
    assert!(side[..article_end].contains(r#"<h2>Përshkrimi</h2>"#) && !phone.contains("listing-map-section"));
    // The price, then the deal (on the seller's own page their buttons), are in the flow there,
    // the seller's card after the description, not in the deal.
    let deal_at = phone.find(r#"<div class="listing-deal">"#).unwrap();
    let deal = &phone[deal_at..phone.find(r#"<section class="listing-details">"#).unwrap()];
    assert!(phone.find(r#"<div class="price-box">"#).unwrap() < deal_at && !deal.contains("seller-section"), "{deal}");
    assert!(!phone.contains("key-facts") && !phone.contains("spec-grid"));
    let order = positions(&phone, &["<h2>Detajet</h2>", "<h2>Përshkrimi</h2>", r#"<section class="seller-section""#]);
    assert!(order.is_sorted(), "{order:?}");
    // A car's page is laid out as FINN's (.listing-car): the seller's card in the deal, at the
    // right from 960 px up (listing_page_shows_price_and_attributes has the rest).
    let golf = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert!(golf.contains(r#"<article class="listing listing-car""#), "{golf}");
    // Its map can move into that column, where it has room enough (map_aside_controller.js): the
    // controller on the columns' box, the column and the map its targets.
    assert!(golf.contains(r#"<div class="listing-side" data-controller="map-aside">"#), "{golf}");
    assert!(golf.contains(r#"<div class="listing-deal" data-map-aside-target="deal">"#));
    assert!(golf.contains(r#"<section class="listing-map-section" aria-label="Vendndodhja" data-map-aside-target="map">"#));
    assert!(!phone.contains("map-aside"), "only a car's page");
    // The price before the deal, which holds the seller's card: from 960 px up the price stands
    // beside the title, in its row, the deal under it from the next (the user's ask of
    // 2026-10-11, "make sure the price gets same space as title vertically wise").
    let deal_at = golf.find(r#"<div class="listing-deal""#).unwrap();
    let deal = &golf[deal_at..golf.find(r#"<section class="listing-description">"#).unwrap()];
    assert!(golf.find(r#"<div class="price-box">"#).unwrap() < deal_at && deal.contains(r#"<section class="seller-section""#), "{deal}");
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    for rule in [
        ".listing-car .listing-side > .listing-deal{display:grid;grid-template-columns:minmax(0,1fr);align-content:start;grid-column:2;grid-row:2 / span 40;align-self:start;}",
        ".listing-car .listing-side > .price-box{grid-column:2;grid-row:1;padding-bottom:18px;border-bottom:1px solid var(--border);}",
        ".listing-car .listing-side > .price-box > .price-row{height:calc(var(--lp-title) * 1.25);min-height:0;}",
        ".listing-car .listing-side > .price-box > .price-under{margin-top:.35rem;line-height:1.55;}",
        // From 720 px up the ways to reach the seller first, the seller under them after their
        // hairline (the user's ask of 2026-10-12, "put seller below buttons in desktop"); the page
        // keeps the seller first, for screen readers and a phone, where the bar settles under
        // them: on a car's page under 960 px the bar is the grid item before the seller's; from
        // 960 px up a seller with no bar stands right under the price's hairline.
        "@media (min-width:720px){.seller-card{display:flex;flex-direction:column;}.seller-card > .contact-bar{order:-1;margin:0 0 1.5rem;}}",
        ".listing-car .listing-deal .seller-card > .contact-bar{grid-column:1 / -1;order:1;margin:1.75rem 0 0;}",
        ".listing-car .listing-deal > .seller-section > h2,.listing-car .listing-deal > .seller-section > .seller-card:first-child > .seller-head:only-child{padding-top:16px;border-top:0;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let card = &golf[golf.find(r#"<div class="seller-card">"#).unwrap()..];
    assert!(card.find(r#"<div class="seller-head">"#).unwrap() < card.find(r#"<aside class="contact-bar""#).unwrap(), "the seller first in the page");
    // A property's is laid out as a car's, as the canvas has it (its photos ran across the page
    // once, its details as pills): the deal at the right, its amenities under their own name, its
    // map able to move into the column.
    let flat = html(pool, "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    assert!(flat.contains("listing-car") && !flat.contains("listing-split") && !flat.contains(r#"<dl class="attributes">"#), "{flat}");
    assert!(flat.contains(r#"<div class="listing-side" data-controller="map-aside">"#), "{flat}");
    assert!(flat.contains(r#"<ul class="options">"#), "{flat}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_price_lowered_lately_says_by_how_much(pool: PgPool) {
    // The Golf, from 9 500 € to 9 000 €: 5% less, after its price with what it was struck
    // through, on its card as on its page.
    set_price(&pool, 1001, "9000").await;
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let badge = "<p class=\"card-price\"><span class=\"card-amount\"><span class=\"card-figure\">9\u{a0}000\u{a0}€</span></span><del class=\"card-was\"><span class=\"visually-hidden\">Çmimi ishte </span>9\u{a0}500\u{a0}€</del><span class=\"card-drop\">−5%</span></p>";
    assert!(golf.contains(badge), "{golf}");
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let was = "<span class=\"price-was\"><span class=\"visually-hidden\">Çmimi ishte </span><del>9\u{a0}500\u{a0}€</del> <span class=\"price-drop\">−5%</span></span>";
    assert!(page.contains(was), "{page}");

    // Dearer for a while and then back under that, the Mercedes is no cheaper than it was at its
    // lowest of the month, 21 500 €; at 20 000 €, it is, by 7%.
    set_price(&pool, 1002, "30000").await;
    set_price(&pool, 1002, "25000").await;
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1002).contains("card-drop"));
    set_price(&pool, 1002, "20000").await;
    assert!(card_of(&html(pool.clone(), "/c/makina").await, 1002).contains("20\u{a0}000\u{a0}€</span><span class=\"card-note card-negotiable\"> (i diskutueshëm)</span></span><del class=\"card-was\"><span class=\"visually-hidden\">Çmimi ishte </span>21\u{a0}500\u{a0}€</del><span class=\"card-drop\">−7%</span>"));
    // A narrow photo, a small card's or the list's on a phone, keeps the price, its month and the
    // pill: "(i diskutueshëm)" and what it was cut the price short there (the user, 2026-10-11).
    // A big card has room for them.
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    assert!(css.contains("@media (max-width:719px){:is(.card-negotiable,.card-was){display:none;}.cards-big .results :is(.card-negotiable,.card-was){display:revert;}}"));
    assert!(!css.contains("big-cards"));

    // Less than 1% is no drop: the iPhone, from 55 000 to 54 700 Lekë. Nor is a price in another
    // currency: the bike, from 25 000 Lekë to 200 €.
    set_price(&pool, 1005, "54700").await;
    assert!(!card_of(&html(pool.clone(), "/c/elektronike").await, 1005).contains("card-drop"));
    sqlx::query("UPDATE listings SET price_amount = 200, currency = 'EUR' WHERE public_no = 1010").execute(&pool).await.unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/sport-dhe-hobi").await, 1010).contains("card-drop"));

    // A month on, the Golf's drop is just its price.
    sqlx::query(
        "UPDATE listing_price_history SET changed_at = changed_at - interval '31 days'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1001)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1001).contains("card-drop"));
    assert!(!html(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.contains("price-was"));
}
