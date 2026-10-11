//! The look the design system gives every page (static/css, static/js, base.html, rail.rs): the one
//! main colour and the accents, buttons, pills, chips, fields and cards as the canvas draws them;
//! the header, the menu, the rail and the footer; the front page's tiles and search; the mark a tap
//! leaves, the stand-in shown while a listing's page comes, its photos' glow, and the pickers every
//! select opens.

mod common;

use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_part_of_the_site_has_the_one_main_colour(pool: PgPool) {
    // The five parts of the site lost their colours of their own (the redesign's first step): no
    // page names its main category on its body any more, and the stylesheet has no rules for
    // them.
    for page in ["/", "/c/elektronike", "/c/shtepi-dhe-kopsht", "/c/makina", "/harta?category=elektronike", "/listing/1005-iphone-13-128-gb-si-i-ri"] {
        let page_html = html(pool.clone(), page).await;
        assert!(page_html.contains(r#"<body data-controller="menu">"#) && !page_html.contains("data-section"), "{page}");
    }
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    assert!(!css.contains("data-section"));
    // The greys and the main colour, the text's, in either theme; the old names that pointed at
    // them while the pages moved over went with the redesign's clean-up, every rule naming what
    // they stood for, and no rule names a colour that isn't there.
    let light = &css[css.find(":root{color-scheme:light;").expect("the light theme")..];
    let light = &light[..light.find('}').unwrap()];
    for token in ["--bg:#f7f7f7;", "--surface:#fff", "--surface-muted:#ececec;", "--border:#dedede;", "--text:#1b1b1b;", "--muted:#707070;", "--main:#1b1b1b;", "--choice-mint:#2de38c;", "--choice-lime:#c8ff00;"] {
        assert!(light.contains(token), "{token} in {light}");
    }
    let dark = &css[css.find(r#":root[data-theme="dark"]{"#).expect("the dark theme")..];
    let dark = &dark[..dark.find('}').unwrap()];
    for token in ["--bg:#1b1b1b;", "--surface:#242424;", "--surface-muted:#2c2c2c;", "--text:#e8e8e8;", "--muted:#a3a3a3;", "--main:#e8e8e8;"] {
        assert!(dark.contains(token), "{token} in {dark}");
    }
    for name in ["--accent", "--button", "--button-hover", "--on-button", "--badge", "--menu-border", "--menu-shadow"] {
        assert!(!css.contains(&format!("{name}:")) && !css.contains(&format!("var({name})")), "{name}");
    }
    // A kept listing's heart is filled with the site's one red, in every part of the site, lined
    // thinly in white over a photo; the welcome page's round heart and conversations buttons went
    // on 2026-10-11, and their rules with them.
    assert!(!css.contains("circle-shortcut") && !css.contains("saved-shortcut") && !css.contains("inbox-shortcut"));
    let kept = &css[css.find(r#".heart-button[aria-pressed="true"] svg{"#).expect("the kept heart")..];
    let kept = &kept[..kept.find('}').unwrap()];
    assert!(kept.contains("fill:var(--red)") && kept.contains("color:var(--on-photo)"), "{kept}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn buttons_are_pills_in_the_accent_or_the_grey(pool: PgPool) {
    // The redesign's buttons (listing.css): pills with no outline, 48px tall, their words
    // semibold; the one that does a page's thing in the accent with dark words, a shade off under
    // the pointer (the user's ask of 2026-10-11: the main colour before), a secondary one on the
    // grey of secondary controls, a danger one that grey with red words, a small one 40px tall.
    // "Posto", the rail's and the welcome page's, is the accent in both themes, as they are.
    // (The stylesheet comes minified: web::site_css, a rule right after the one before it.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let button = rule(".button");
    for part in ["min-height:var(--size-button)", "border-radius:var(--radius-pill)", "background:var(--highlight)", "color:var(--on-highlight)", "font-weight:var(--semibold)"] {
        assert!(button.contains(part), "{part} in {button}");
    }
    assert!(rule(".button:hover").contains("background:var(--highlight-hover)"));
    assert!(rule(".button.secondary").contains("background:var(--surface-muted)"));
    assert!(rule(".button.danger").contains("color:var(--danger)"));
    assert!(rule(".button.small").contains("min-height:var(--size-button-sm)"));
    // (The front page's "Posto" in a phone's bar is quiet, a grey circle: the user's ask.)
    assert!(rule(".header-post").contains("background:var(--surface-muted) content-box"));
    let light = &css[css.find(":root{color-scheme:light;").expect("the light theme")..];
    let light = &light[..light.find('}').unwrap()];
    for token in ["--post:var(--highlight);", "--on-post:var(--on-highlight);", "--post-hover:var(--highlight-hover);", "--highlight-hover:color-mix(in srgb,var(--highlight) 88%,#1b1b1b);"] {
        assert!(light.contains(token), "{token} in {light}");
    }
    // A shade darker under the pointer on the light theme, lighter on the dark one.
    let dark = &css[css.find(r#":root[data-theme="dark"]{"#).expect("the dark theme")..];
    let dark = &dark[..dark.find('}').unwrap()];
    assert!(dark.contains("--highlight-hover:color-mix(in srgb,var(--highlight) 85%,#fff") && !dark.contains("--post:"), "{dark}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_tapped_card_or_category_shows_it_in_the_accent(pool: PgPool) {
    // A card tapped or clicked underlines its title in the accent until its listing shows, with no
    // box round it in the main colour, and a category's pill fills its icon with the accent and
    // underlines its name in it, under the pointer too: the user's asks of 2026-10-11
    // (application.js marks either .pressed). No blue tap box of the browser's own anywhere: a
    // link's words, held, are underlined in the accent, the menu's rows' too.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".card:active .card-link,.card.pressed .card-link{text-decoration:underline 2px var(--highlight);text-underline-offset:.14em;}",
        ".card:has(.heart:active) .card-link{text-decoration:none;}",
        ".shortcut:active .shortcut-icon,.shortcut.pressed .shortcut-icon{fill:var(--highlight);}",
        ".shortcut:hover .shortcut-icon{fill:var(--highlight);}",
        ".shortcut:active,.shortcut.pressed{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        ".shortcut:hover{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        "html{-webkit-tap-highlight-color:transparent;}",
        "a:not([class]):active{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        "a.menu-row:active .menu-row-word{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        // Under the pointer a card's title too, but not while the pointer is on its heart.
        ".card:hover .card-link{text-decoration:underline 2px var(--highlight);text-underline-offset:.14em;}",
        ".card:has(.heart:hover) .card-link{text-decoration:none;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(".card.pressed{") && !css.contains("outline-offset:4px"), "no box round a pressed card");
    // The mark comes only once the next page has taken a moment to come, and nothing waits for it:
    // a page fetched ahead shows at once (the user's ask of 2026-10-11; it was held 200 ms after a
    // tap so that the mark was seen). (The script comes minified too: web::minified_js.)
    let script = get(pool.clone(), &format!("/static/js/application.js?v={}", marketplace::web::asset_version())).await.body;
    for part in ["closest(`a.shortcut`)", "`.card.pressed, .shortcut.pressed`", "addEventListener(`turbo:click`", "addEventListener(`turbo:before-render`"] {
        assert!(script.contains(part), "{part} in {script}");
    }
    assert!(!script.contains(".detail.resume()"), "no page held for the mark: {script}");
    // On a touch screen the next page comes in as a drawer, from the right, and going back the page
    // left slides away to the right, by the way Turbo marks on <html> while a visit lasts; the
    // page cross-fades elsewhere, and changes at once for visitors who ask for less motion.
    for part in ["data-turbo-visit-direction=", "@keyframes drawer-in{", "@keyframes drawer-out{", "prefers-reduced-motion:no-preference"] {
        assert!(css.contains(part), "{part}");
    }
    // It slides in at once as the finger lifts: a page that hasn't come yet is a stand-in, the
    // page's bar over grey shapes (a listing's with its card's photo, title and price), which
    // Turbo shows as a preview and keeps no copy of, until the page itself comes.
    for part in ["snapshotHTML", "`turbo-cache-control`", "stand-in-listing", "`.card-amount`", "listing-header"] {
        assert!(script.contains(part), "{part} in {script}");
    }
    for part in [".stand-in{", ".stand-in-text{", ".stand-in-listing .key-fact-icon{", ".gallery-photo .gallery-interim{filter:blur(6px);}"] {
        assert!(css.contains(part), "{part}");
    }
    // The card's photo stands in for the listing's, softly blurred, until the page's own comes.
    assert!(script.contains("`gallery-interim`") && script.contains("`.card-description-text`"), "{script}");
    // The page's own way back slides back in every browser, Safari's engine too, which can't say
    // whether it animated a back itself: the back button says so first.
    assert!(script.contains("`back:button`"), "{script}");
    let back = get(pool.clone(), &format!("/static/js/controllers/back_controller.js?v={}", marketplace::web::asset_version())).await.body;
    assert!(back.contains("`back:button`") && back.find("`back:button`").unwrap() < back.find("history.back()").unwrap(), "{back}");
    // Where the browser animated a way back itself (Android's Chrome under a swipe), Turbo's view
    // transition for it is skipped as it starts, the setting left in the page: taken out, it was
    // missing from Turbo's copy of the page left, which, opened again, came without its slide.
    assert!(script.contains(".skipTransition()") && !script.contains(r#"meta[name="view-transition"]"#), "{script}");
    // iPhones and iPads alone are kept from zooming into a field as it takes the focus (the page's
    // size can make its words smaller than 16px), after every page Turbo shows.
    for part in ["iPhone|iPad|iPod", ", maximum-scale=1", "`turbo:load`"] {
        assert!(script.contains(part), "{part} in {script}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_filter_rows_pills_are_a_chips_size(pool: PgPool) {
    // The row's pills, its round buttons, the filters picked and the order are a chip's 36px
    // (--size-chip), as every other pill on the site is, the filters' sheet's choices among them,
    // their words 14px (13px on phones): the user's ask of 2026-10-11, "make the filter pills a bit
    // smaller" (40px, --size-button-sm, until then). "Ruaj kërkimin" over the row is as tall.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".browse{--row:var(--size-chip);",
        "gap:6px;padding:0 12px;border-radius:var(--radius-pill);font-size:.875rem;",
        "@media (max-width:719px){.browse-tools .button.round-button.labelled{font-size:.9286rem;}}",
        ".browse-tools .filter-chip{height:var(--row);gap:6px;padding:0 10px 0 12px;font-size:.875rem;}",
        ".browse-tools .clear-filters{display:inline-flex;align-items:center;min-height:var(--row);",
        "height:var(--size-chip);padding:0 12px;",
        ".filter-form .check{",
        "min-height:var(--size-chip);",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains("--row:var(--size-button-sm)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_phones_bar_tucks_its_search_away_on_the_way_down(pool: PgPool) {
    // On a phone the bar's search row tucks away as the page scrolls down and comes back as it
    // scrolls up, the bar's own row staying (the user's ask of 2026-10-11, "when i scroll down i
    // want the search bar to hide in mobile"): the bar clipped to its first row, the search fading,
    // a hairline under the row, nothing moving; not while the search has the focus, nor on a
    // listing's page, whose search folds behind the magnifier. A stand-in's bar and a page Turbo
    // keeps have it out. (The stylesheet and the scripts come minified.)
    let version = marketplace::web::asset_version();
    let css = get(pool.clone(), &format!("/static/css/site.css?v={version}")).await.body;
    for rule in [
        ".site-header:has(> .header-inner > .top-search){clip-path:inset(0);transition:clip-path .2s ease;}",
        ".site-header:has(.top-search:focus-within){clip-path:none;}",
        ".site-header.search-tucked:has(> .header-inner > .top-search){clip-path:inset(0 0 calc(var(--size-button-sm) + 9px) 0);}",
        ".search-tucked .header-inner > .top-search{opacity:0;}",
        ".site-header.search-tucked::after{opacity:1;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let application = get(pool, &format!("/static/js/application.js?v={version}")).await.body;
    for part in ["`search-tucked`", "matchMedia(`(max-width: 719px)`)", "`.top-search:focus-within`", "`search-folded`", "`search-open`,`search-tucked`"] {
        assert!(application.contains(part), "{part} in {application}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_folds_the_bars_search_behind_a_magnifier(pool: PgPool) {
    // A listing's page has no search in its bar, but its magnifier just before the account, which
    // opens it there, ready for typing (search_fold_controller.js): the user's ask of 2026-10-11.
    // The search is still in the page, folded (search-folded), so that it opens where the other
    // pages have it.
    let golf = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let bar = &golf[golf.find(r#"<header class="site-header search-folded">"#).expect("a folded bar")..golf.find("</header>").unwrap()];
    let order = positions(bar, &[
        r#"<form class="top-search" action="/search" method="get" role="search" data-controller="suggest""#,
        r#"<div class="header-end">"#,
        r#"<button type="button" class="icon-button search-button" aria-label="Kërko" title="Kërko" aria-expanded="false""#,
        r#"data-controller="search-fold" data-action="search-fold#toggle turbo:before-cache@document->search-fold#fold">"#,
        r#"<a class="icon-button account-button" href="/llogaria""#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!bar.contains("header-post"), "{bar}");
    // Every other page's bar has its search open (the front page none), the magnifier there but
    // hidden, so that a listing's stand-in, which is the page's bar (application.js), has it.
    for page in ["/", "/c/makina", "/search?q=golf", "/promovo"] {
        let other = html(pool.clone(), page).await;
        assert!(other.contains(r#"<header class="site-header">"#) && !other.contains("search-folded"), "{page}");
        assert!(other.contains(r#"<button type="button" class="icon-button search-button""#), "{page}");
    }
    // (The stylesheet and the scripts come minified: web::site_css, web::minified_js.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".search-button{display:none;}",
        ".search-folded .search-button{display:inline-grid;width:var(--size-button-sm);height:var(--size-button-sm);margin-left:auto;border:0;}",
        ".search-folded .account-button{margin-left:0;}",
        ".search-folded.search-open .search-button{background:var(--surface-muted);}",
        ".search-folded .top-search{display:none;}",
        ".search-folded.search-open .top-search{display:flex;}",
        // On a phone the bar is one row while the search is folded, two while it is open.
        "@media (max-width:719px){:root:has(.site-header.search-folded){--size-header:var(--size-header-row);}:root:has(.site-header.search-folded.search-open){--size-header:calc(var(--size-header-row) + var(--size-button-sm) + 8px);}.search-folded .header-inner{grid-template-rows:var(--size-header-row);padding-bottom:0;}.search-folded.search-open .header-inner{grid-template-rows:var(--size-header-row) var(--size-button-sm);padding-bottom:8px;}}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let version = marketplace::web::asset_version();
    let script = get(pool.clone(), &format!("/static/js/controllers/search_fold_controller.js?v={version}")).await.body;
    for part in ["`search-open`", "`search-folded`", "`Escape`", "new FocusEvent(`focus`)"] {
        assert!(script.contains(part), "{part} in {script}");
    }
    // A listing's stand-in has its bar folded, any other page's open, and none the front page's
    // "Posto", its bar's alone (home.html, header_post).
    let application = get(pool, &format!("/static/js/application.js?v={version}")).await.body;
    assert!(application.contains("`search-folded`") && application.contains("`search-open`") && application.contains("`.header-post`"), "{application}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_listings_stand_in_holds_still_at_every_page_and_text_size(pool: PgPool) {
    // As a listing's page fills its stand-in, nothing moves, whatever the page's size and the text's
    // (the user's report of 2026-10-11, "on some settings site jumps"; measured at all sixteen on a
    // 390px phone): on a phone the stand-in's contact bar stands at the screen's foot, where the
    // page's sticky one is (the stand-in's short column ended over the foot on a small page or text
    // size, the bar floating there), and the loan's empty circle sits at its line's top, as tall as
    // the page's with its icon. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    assert!(css.contains("@media (max-width:719px){.stand-in-listing .contact-bar{position:fixed;right:0;bottom:0;left:0;margin:0;"), "the bar at the foot");
    assert!(css.contains(".stand-in-listing .bubble > summary{vertical-align:top;}"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_listings_photo_comes_in_whole(pool: PgPool) {
    // A listing's photo that hasn't come by the time its page shows, after a tap, is unseen until
    // it has loaded whole and then fades in over its blurred copy, rather than drawing itself in
    // from the top down as its bytes come, a sharp edge sweeping down the blur (the user's report
    // of 2026-10-11, "a glitch when photo loads"); one the browser has at hand shows at once,
    // untouched. (The stylesheet and the script come minified: web::site_css, web::minified_js.)
    let version = marketplace::web::asset_version();
    let css = get(pool.clone(), &format!("/static/css/site.css?v={version}")).await.body;
    assert!(css.contains(".gallery-photo img:not(.gallery-backdrop){transition:opacity .2s ease-out;}.gallery-photo img.is-coming{opacity:0;transition:none;}"));
    let script = get(pool, &format!("/static/js/application.js?v={version}")).await.body;
    for part in ["`turbo:render`", "`is-coming`", "`.gallery-photo img:not(.gallery-backdrop, .gallery-interim)`"] {
        assert!(script.contains(part), "{part}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listings_photos_glow_as_youtubes_ambient_light(pool: PgPool) {
    // Behind a listing's photos their glow, as YouTube's ambient light takes the colours of what
    // plays (the user's ask of 2026-10-11, "a blur / shadow similar to how youtube hues its
    // background based on whats its playing"): in the photos' own box, before them, a decoration,
    // the photo once more, the copy its frame shows blurred round it (here, a photo kept before
    // there were tiny copies, its card-sized one). One photo has nothing to follow; several
    // follow the one that shows (listings_get_photos).
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let opening = r#"<div class="listing-glow" aria-hidden="true"><img src=""#;
    let glow = page.find(opening).expect("the glow") + opening.len();
    let source = &page[glow..glow + page[glow..].find('"').unwrap()];
    assert!(source.starts_with("https://images.unsplash.com/photo-1591230740238-e9a71182b67c?"), "{source}");
    assert!(page[glow..].starts_with(&format!(r#"{source}" alt=""></div><div class="gallery">"#)), "{page}");
    assert!(page.contains(&format!(r#"<img class="gallery-backdrop" src="{source}" alt="""#)));
    // In a box past the photos' own, enlarged a quarter, blurred 80px and its colours livelier,
    // fading to nothing at the box's edge (a mask), at 40% on the light theme and 55% on the dark:
    // the PluribusGoogle canvas's values, the user's ask of 2026-10-11. Each layout spreads it its
    // own way (64px over, 130 under and 24 at the sides of a car's mosaic; 64, 100, 24 at the left
    // and 110 under the column beside a split page's tall photo; 72, 150 and the page's margin on
    // a phone). Under every word on the page, and under the photos' box, whose ground is the
    // page's, so that it shows round them alone. A split page's photo stands still as its column
    // scrolls, a layer of its own: there the photos' box goes under the listing's words, its glow
    // under it, and the listings like it over both. (The stylesheet and the scripts come
    // minified.)
    let version = marketplace::web::asset_version();
    let css = get(pool.clone(), &format!("/static/css/site.css?v={version}")).await.body;
    for rule in [
        "--glow:blur(80px) saturate(140%);--glow-fade:radial-gradient(ellipse 50% 50% at 50% 50%,#000 28%,transparent 100%);--glow-strength:40%;",
        ".listing-media{position:relative;border-radius:var(--radius);background:var(--bg);}.listing-glow{position:absolute;inset:-72px calc(-1 * var(--gutter)) -150px;z-index:-1;overflow:hidden;opacity:var(--glow-strength);-webkit-mask-image:var(--glow-fade);mask-image:var(--glow-fade);pointer-events:none;}.listing-glow img{position:absolute;inset:0;width:100%;height:100%;object-fit:cover;filter:var(--glow);transform:scale(1.25);}",
        "@media (min-width:960px){.listing-car .listing-glow{inset:-64px -24px -130px;}.listing-split .listing-glow{inset:-64px -110px -100px -24px;}}",
        "@media (min-width:960px){.listing-split{isolation:isolate;}.listing-split > .listing-media{z-index:-1;}.listing-split ~ .related{position:relative;}}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert_eq!(css.matches("--glow-strength:40%").count(), 1, "the light theme's");
    assert_eq!(css.matches("--glow-strength:55%").count(), 2, "the dark theme's, picked in the menu and the device's");
    // As the photos slide, the one that shows lends it its colours, fading in over the last's; a
    // stand-in's is its card's photo.
    let slides = get(pool.clone(), &format!("/static/js/controllers/slides_controller.js?v={version}")).await.body;
    assert!(slides.contains("shine(") && slides.contains("hasGlowTarget") && slides.contains("`.gallery-backdrop`"), "{slides}");
    let application = get(pool, &format!("/static/js/application.js?v={version}")).await.body;
    assert!(application.contains(r#"<div class="listing-glow" aria-hidden="true"><img src="${"#), "{application}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_carries_its_pages_words_for_the_stand_in(pool: PgPool) {
    // The stand-in a tap shows while a listing's page comes says what the card knows of the page in
    // words (the user's asks of 2026-10-11): its description, its lines as they are, as much as the
    // page shows of it folded and more (data-description); what its contact bar offers to show of
    // the seller (data-reveal); the line under its title (data-line). Attributes, which no search
    // engine reads as the page's words, and nothing the card shows.
    sqlx::query("UPDATE listings SET description = $2 WHERE public_no = $1").bind(1001_i64).bind("Rreshti i parë.\nRreshti i dytë, me fjalë.").execute(&pool).await.unwrap();
    sqlx::query("UPDATE listing_contacts c SET show_phone = false, email = 'shitesi@example.com', show_email = true FROM listings l WHERE l.id = c.listing_id AND l.public_no = 1002")
        .execute(&pool)
        .await
        .unwrap();
    let cars = html(pool.clone(), "/c/makina").await;
    let golf = card_of(&cars, 1001);
    assert!(golf.contains("data-description=\"Rreshti i parë.\nRreshti i dytë, me fjalë.\""), "{golf}");
    assert!(golf.contains(r#" data-reveal="phone""#) && !golf.contains("data-closed"), "{golf}");
    assert!(card_of(&cars, 1002).contains(r#" data-reveal="email""#));
    // A long one is cut at a word, past the dozen lines its page shows of it folded.
    sqlx::query("UPDATE listings SET description = $2 WHERE public_no = $1").bind(1001_i64).bind("fjalë ".repeat(400)).execute(&pool).await.unwrap();
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let open = "data-description=\"";
    let carried = &golf[golf.find(open).unwrap() + open.len()..];
    let carried = &carried[..carried.find('"').unwrap()];
    let length = carried.chars().count();
    assert!((1500..=1600).contains(&length) && carried.ends_with("fjalë"), "{length}");
    // The page gives the stand-in its own parts as it words them: the description's heading and
    // its fold's button, the contact bar's pills with their icons. Inert, in a template.
    let parts = &cars[cars.find(r#"<template id="stand-in-parts">"#).expect("the stand-in's parts")..];
    let parts = &parts[..parts.find("</template>").unwrap()];
    for part in [
        r#"<h2 data-part="description">Përshkrimi</h2>"#,
        r#"data-part="more" data-fold-target="button" hidden>"#,
        r#"<span class="button secondary reveal-button" data-part="phone">"#,
        r#"<span class="button-label">Shfaq numrin</span>"#,
        r#"<span class="button-label">Shfaq email-in</span>"#,
        r#"<span class="button message-button" data-part="message">"#,
        r#"<span class="button-label">Shkruaji shitësit</span>"#,
    ] {
        assert!(parts.contains(part), "{part}");
    }
    // The stand-in folds its description with the page's own controller, at hand on every page.
    // (The script comes minified: web::minified_js.)
    let script = get(pool, &format!("/static/js/application.js?v={}", marketplace::web::asset_version())).await.body;
    for part in ["`stand-in-parts`", r#"data-controller="fold""#, "controllers/fold_controller.js"] {
        assert!(script.contains(part), "{part}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_front_pages_categories_are_a_line_of_tiles(pool: PgPool) {
    // The front page's main categories are one line of tiles that slides sideways, a mouse
    // dragging it: each its icon on one of Material's soft shapes, in turn, over its name (the
    // user's asks of 2026-10-11, the pluribus-tests canvas's Material front page in the site's
    // accent, on a wide screen and then on phones); from 720px up the search over them stands in
    // the middle, 720px at most; in use it has no edge or ring round it, Material's way.
    let home = html(pool.clone(), "/").await;
    let nav = &home[home.find(r#"<nav class="shortcuts tiles" data-controller="drag-row" aria-label="Kategoritë">"#).expect("the categories")..];
    let nav = &nav[..nav.find("</nav>").unwrap()];
    assert!(nav.contains(r#"<a class="shortcut" href="/c/automjete">
      <span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#), "{nav}");
    assert!(nav.contains(r#"<span class="shortcut-name">Automjete</span>"#));
    assert_eq!(nav.matches(r#"<span class="shortcut-shape" aria-hidden="true">"#).count(), nav.matches(r#"<a class="shortcut""#).count());
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        "@media (min-width:720px){.welcome-tools{max-width:720px;margin-inline:auto;}",
        ".tiles{display:flex;flex-wrap:nowrap;gap:2px;padding-block:3px;overflow-x:auto;",
        ".welcome .tiles{margin-top:28px;}",
        "@media (min-width:720px){.welcome .tiles{justify-content:space-between;}}",
        // On phones, as the canvas's phone board has them: the line from the screen's edge to its
        // edge, the first shape level with the page's side, the tiles 84px, the shapes 56px.
        "@media (max-width:719px){.tiles{margin-inline:calc(-1 * var(--gutter));padding-inline:calc(var(--gutter) - 14px);}.welcome .tiles{margin-top:20px;}.tiles .shortcut{width:84px;font-size:.8571rem;}.tiles .shortcut-shape{width:56px;height:56px;}.tiles .shortcut-shape::before{inset:-4px;transform:scale(.875);}.tiles .shortcut-icon{width:26px;height:26px;}}",
        ".tiles .shortcut{flex:none;flex-direction:column;gap:8px;width:96px;",
        ".tiles .shortcut-shape{position:relative;display:grid;flex:none;place-items:center;width:64px;height:64px;}",
        ".tiles .shortcut:nth-child(6n + 1) .shortcut-shape::before{clip-path:path(",
        ".tiles .shortcut:nth-child(6n + 6) .shortcut-shape::before{clip-path:path(",
        ".tiles .shortcut-icon{position:relative;width:28px;height:28px;}",
        ".tiles .shortcut:hover .shortcut-shape::before{background:var(--surface-muted-hover);}",
        // A category page's subcategories are a line of them too (the user's ask of 2026-10-11),
        // from 720px up its first shape where the page's column starts.
        "@media (min-width:720px){.subcategories{margin-inline:-16px;}}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(".welcome-form:focus-within") && !css.contains("--focus-search"), "no edge or ring round the search in use");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_front_pages_search_is_materials_search_bar(pool: PgPool) {
    // The front page's search is the canvas's Material search bar (the user's ask of 2026-10-11,
    // "make search bar google style like in the design"): a grey pill 64px tall, its words 18px
    // 24px in, its button 48px, 8px in; on phones the page's width, 56px, the words 16px and the
    // button 40px. The button is its magnifier alone, on no circle, dark on the light theme and in
    // the accent on the dark one, a grey circle under the pointer only. Its edge is clear, a pixel
    // of the padding (there for forced colours). "Posto", which stood after it on phones, is the
    // bar's now, before the account, quiet: a grey circle of 36px, 40px to tap, its plus as the
    // magnifier is, which the rail has from 720px up (the user's asks: "tone the plus button down.
    // Remove the circle for the search icon. Make it darker on light theme"). (The stylesheet
    // comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".welcome-form{display:flex;align-items:center;gap:var(--space-12);height:64px;padding:0 7px 0 23px;border:1px solid transparent;border-radius:var(--radius-pill);background:var(--surface-muted);}",
        "font-size:max(16px,1.125rem);",
        ".welcome-search{width:var(--size-button);height:var(--size-button);color:var(--highlight-icon);}",
        "@media (hover:hover){.welcome-search:hover{background:var(--surface-muted-hover);}}",
        ".welcome-search svg{width:22px;height:22px;}",
        "@media (min-width:720px){.welcome-tools{max-width:720px;margin-inline:auto;}}",
        "@media (max-width:719px){.welcome-form{height:56px;}.welcome-search{width:var(--size-button-sm);height:var(--size-button-sm);}.welcome-search svg{width:20px;height:20px;}}",
        ".header-post{width:var(--size-button-sm);height:var(--size-button-sm);padding:2px;border:0;background:var(--surface-muted) content-box;color:var(--highlight-icon);",
        ".header-post:active{background-color:var(--surface-muted-hover);}",
        "@media (hover:hover){.header-post:hover{background-color:var(--surface-muted-hover);}}",
        "@media (min-width:720px){.header-post{display:none;}}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains("welcome-post") && !css.contains(".welcome-tools{display:flex"), "nothing left of the row's old parts");
    assert!(css.contains(":root{color-scheme:light;") && css.contains("--highlight-icon:#1b1b1b;") && !css.contains("--search-knob"), "the magnifier dark on the light theme, on no circle");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_front_pages_search_opens_as_googles_does(pool: PgPool) {
    // The front page's search opens as Google's does, after the pluribus-tests canvas's Material
    // boards (the user's ask of 2026-10-11, "when search is clicked make it open like google search
    // bar"): on a phone over the whole screen as its words get the focus, the way back first (the
    // site's chevron), which closes it, as Escape, Android's back (a CloseWatcher) and the page left
    // do, the keyboard put away leaving it open; from 720px up the pill and its list one card, a
    // hairline between them, one shadow round both. The bar's search on the other pages keeps its
    // list under it.
    let home = html(pool.clone(), "/").await;
    let form = &home[home.find(r#"<form class="welcome-form""#).expect("the search")..];
    let form = &form[..form.find("</form>").unwrap()];
    assert!(form.contains(r#"data-action="focusout->suggest#left turbo:before-cache@document->suggest#dismiss""#), "{form}");
    let order = positions(form, &[
        r#"<button type="button" class="icon-button search-back" aria-label="Mbyll kërkimin" title="Mbyll kërkimin" data-suggest-target="back" data-action="suggest#dismiss">"#,
        r#"<svg data-icon="chevron-left""#,
        r#"<input id="welcome-q""#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    let english = get_with_cookie(pool.clone(), "/", "gjuha=en").await.body;
    assert!(english.contains(r#"aria-label="Close search" title="Close search""#), "{english}");
    let cars = html(pool.clone(), "/c/makina").await;
    assert!(!cars.contains("search-back") && cars.contains("turbo:before-cache@document->suggest#close"), "{cars}");
    // (The stylesheet and the scripts come minified: web::site_css, web::minified_js.)
    let version = marketplace::web::asset_version();
    let css = get(pool.clone(), &format!("/static/css/site.css?v={version}")).await.body;
    for rule in [
        ".search-back{display:none;}",
        ".welcome-form .suggestions{top:calc(100% + 1px);right:-1px;left:-1px;padding:var(--space-8) 0;border-top:1px solid var(--border);border-radius:0 0 28px 28px;background:var(--surface-muted);box-shadow:none;font-size:max(16px,1rem);}",
        ".welcome-form .suggestion{min-height:56px;gap:var(--space-16);padding:0 16px 0 24px;border-radius:0;}",
        ".welcome-form :is(.suggestion:hover,.suggestion[aria-selected=\"true\"]){background:var(--surface-muted-hover);}",
        "@media (min-width:720px){.welcome-form.has-list{z-index:31;border-radius:28px 28px 0 0;filter:drop-shadow(",
        "html:has(.welcome-form.is-open){overflow:hidden;}.welcome-form.is-open{position:fixed;inset:0;z-index:60;display:grid;grid-template-columns:auto minmax(0,1fr) auto auto;grid-template-rows:72px minmax(0,1fr);",
        ".welcome-form.is-open .search-back{display:inline-grid;grid-column:1;width:var(--size-button);height:var(--size-button);color:var(--text);}",
        ".welcome-form.is-open .suggestions{position:static;grid-row:2;grid-column:1 / -1;align-self:stretch;max-height:none;",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let script = get(pool, &format!("/static/js/controllers/suggest_controller.js?v={version}")).await.body;
    for part in ["matchMedia(`(max-width: 719px)`)", "classList.add(`is-open`)", "classList.remove(`is-open`)", "toggle(`has-list`", "new CloseWatcher"] {
        assert!(script.contains(part), "{part} in {script}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_way_back_is_a_chevron_level_with_its_words(pool: PgPool) {
    // The breadcrumb's way back is Phosphor's chevron, its Regular weight's (the Duotone's caret is
    // a triangle), moved into the middle of its box as every caret is (icons.rs), and the
    // breadcrumb's words stand at the font's own line height, so that their capitals sit level
    // with it: the user found it off the middle of its line on 2026-10-11.
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let back = &page[page.find(r#"class="breadcrumb-back""#).expect("the way back")..];
    let back = &back[..back.find("</a>").unwrap()];
    assert!(back.contains(r#"<g transform="translate(.75 0) scale(.09375)" stroke="none"><path fill="currentColor" d="M165.66,202.34"#), "{back}");
    assert!(!page.contains("M160,48V208L80,128Z") && !page.contains("M176,128,96,208V48Z"), "no caret drawn as a triangle");
    // A main category's page has no breadcrumb: its way back stands before its name (the user's
    // ask of the same day); nor has a subcategory's since, the category above it over its name.
    let cars = html(pool.clone(), "/c/automjete").await;
    assert!(!cars.contains(r#"<nav class="breadcrumb""#), "{cars}");
    let title = &cars[cars.find(r#"<div class="browse-heading-text">"#).expect("the heading's line")..];
    let back = title.find(r#"<a class="breadcrumb-back title-back" href="/" aria-label="Kthehu""#).expect("the way back");
    assert!(back < title.find(r#"<h1 class="browse-title">Automjete</h1>"#).unwrap(), "{title}");
    let makina = html(pool.clone(), "/c/makina").await;
    assert!(!makina.contains(r#"<nav class="breadcrumb""#) && makina.contains(r#"<a class="breadcrumb-back title-back" href="/c/automjete" aria-label="Kthehu""#), "{makina}");
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let crumb = &css[css.find(".breadcrumb{").expect("the breadcrumb")..];
    let crumb = &crumb[..crumb.find('}').unwrap()];
    assert!(crumb.contains("line-height:normal"), "{crumb}");
    // "Ruaj kërkimin"'s bookmark is plain until the pointer or a finger is on it, when it fills
    // with the accent as a category's pill does (the user's ask of the same day).
    for rule in [
        ".save-search-button > svg{flex:none;fill:none;}",
        ".save-search-button:active > svg{fill:var(--highlight);}",
        ".save-search-button:hover > svg{fill:var(--highlight);}",
        // Kept, the pill itself filled with the accent, its tick and words dark on it.
        ".save-search-button.is-saved{background:var(--highlight);color:var(--on-highlight);}",
        ".save-search-button.is-saved:hover{background:var(--highlight-hover);}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains("--highlight-knob"));
    // On phones the bar's search has a row of the bar to itself, the whole of it, the bar's height
    // (--size-header) growing with it; in use it has no edge or ring round it (the bar only drops
    // its clip then, a_phones_bar_tucks_its_search_away_on_the_way_down).
    assert!(css.contains(".header-inner > .top-search{grid-column:1 / -1;grid-row:2;"));
    assert!(css.contains(":root:has(.header-inner > .top-search){--size-header:calc(var(--size-header-row) + var(--size-button-sm) + 8px);}"));
    assert!(!css.contains(".top-search:focus-within{"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pills_chips_and_badges_are_grey_or_the_main_colour(pool: PgPool) {
    // The redesign's pills (the categories', the path's), chips and badges: grey with no outline,
    // what is picked in the main colour, a filter that is on in the accent toned down (2026-10-11),
    // a badge's tone in its tint.
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    assert!(rule(".shortcut").contains("background:var(--surface-muted)"));
    assert!(rule(".breadcrumb-here").contains("color:var(--text)"));
    assert!(rule(".filter-chip").contains("background:var(--filter-on)"));
    // A grey badge's words a shade darker than the muted grey, 5:1 on its grey (4.2:1 once).
    let badge = rule(".badge");
    for part in ["height:1.5rem", "background:var(--surface-muted)", "color:var(--muted-on-grey)"] {
        assert!(badge.contains(part), "{part} in {badge}");
    }
    assert!(rule(".badge.danger").contains("background:var(--danger-bg)"));
    assert!(rule(".badge.state").contains("background:var(--main)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn fields_checkboxes_and_radios_are_drawn_as_the_canvas_has_them(pool: PgPool) {
    // Form fields 48px tall with the hairline and 12px corners, focused with a soft ring rather
    // than the outline; checkboxes and radios 22px, checked in the main colour; a field's name in
    // the text's colour. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let fields = &css[css.find(".picker-button{width:100%").expect("the fields' rule")..];
    let fields = &fields[..fields.find('}').unwrap()];
    for part in ["min-height:var(--size-field)", "border:1px solid var(--border)", "border-radius:var(--radius-field)", "background-color:var(--surface)"] {
        assert!(fields.contains(part), "{part} in {fields}");
    }
    assert!(css.contains("box-shadow:var(--focus-field)") && css.contains("--focus-field:0 0 0 3px rgb(27 27 27 / 14%)"));
    assert!(rule(r#"input[type="checkbox"]:checked"#).contains("background:var(--main)"));
    assert!(rule(r#"input[type="radio"]:checked"#).contains("box-shadow:inset 0 0 0 4px var(--surface)"));
    assert!(rule(".field-label").contains("color:var(--text)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn cards_are_drawn_as_the_canvas_has_them(pool: PgPool) {
    // A card's heart a white outline 16.5px with a soft shadow and no circle behind it (a quarter
    // under the canvas's 22px, which the listing's own page keeps: the user's ask of 2026-10-11); its price
    // white and bold over a shade along the photo's bottom edge; a kept listing's status in a dark
    // pill over the photo. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let heart = rule(".heart-button svg");
    assert!(heart.contains("width:16.5px") && heart.contains("drop-shadow(0 1px 3px var(--photo-shade))"), "{heart}");
    assert!(rule(".listing-media .heart-button svg").contains("width:22px"));
    assert!(!rule(".heart-button::before").contains("background:"));
    let shade = rule(".card-media::after");
    assert!(shade.contains("linear-gradient(") && shade.contains("var(--photo-shade)"), "{shade}");
    let price = rule(".card-media .card-price");
    assert!(price.contains("color:var(--on-photo)") && price.contains("font-weight:var(--bold)"), "{price}");
    assert!(rule(".card-status").contains("background:var(--photo-label)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_header_is_drawn_as_the_canvas_has_it(pool: PgPool) {
    // The bar over every page: white on the light theme and the page's colour on the dark one (the
    // header token), frosted as Apple's bars are (that colour at 80%, what scrolls under it
    // blurred, and whole where the reader asks for less transparency), a hairline under it, as
    // tall as the design system has it; the account alone
    // at its right end, a bare round button of 40px; what is unread in the accent, a dot ringed in
    // the bar's colour with its edge on the light theme, the conversations' count with its number
    // on the accent. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let header = rule(".site-header");
    assert!(header.contains("background:var(--frost-bar)") && header.contains("backdrop-filter:var(--frost)"), "{header}");
    assert!(header.contains("border-bottom:1px solid var(--border)"), "{header}");
    assert!(css.contains("--frost-bar:color-mix(in srgb,var(--header) 80%,transparent)"));
    assert!(css.contains("@media (prefers-reduced-transparency:reduce){:root{--frost:none;--frost-bar:var(--header);"));
    // The message sheet on a phone is in the glass too, in its own colour; the contact bar there
    // in the page's (the user's ask of 2026-10-11, "make background of number and message button
    // in phone same as the site background"; the header's white glass earlier that day).
    assert!(css.contains("--frost-sheet:color-mix(in srgb,var(--surface) 80%,transparent)"));
    assert!(css.contains("--frost-page:color-mix(in srgb,var(--bg) 80%,transparent)"));
    assert!(css.contains("--frost-sheet:var(--surface);--frost-page:var(--bg);"), "whole where less transparency is asked for");
    assert!(css.contains("background:var(--frost-page);-webkit-backdrop-filter:var(--frost);backdrop-filter:var(--frost);transition:box-shadow .15s"));
    assert!(rule(".header-inner").contains("height:var(--size-header)"));
    let account = rule(".account-button");
    assert!(account.contains("width:var(--size-button-sm)") && account.contains("margin-left:auto") && account.contains("border:0"), "{account}");
    let dot = rule(".account-button.has-news::before");
    assert!(dot.contains("background:var(--highlight)") && dot.contains("box-shadow:0 0 0 2px var(--header),var(--dot-edge)"), "{dot}");
    let count = rule(".inbox-count");
    assert!(count.contains("background:var(--highlight)") && count.contains("color:var(--on-highlight)"), "{count}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_says_when_its_page_frames_photos_as_a_cars(pool: PgPool) {
    // A car's card and a property's, whose pages frame the photos 4:3 on a phone, say so for the
    // stand-in a tap shows while the page comes (application.js, standIn); the others' don't. The
    // property's card is in a category under Prona. And the stand-in heads itself as the page
    // does: a car's card whose title was made from what the car is carries the title its seller
    // wrote, its page's heading (data-heading); every other card's title is its page's heading.
    let home = get(pool.clone(), "/").await.body;
    for (number, deal) in [(1001, true), (1003, true), (1004, true), (1005, false)] {
        // A place for the loan calculator's circle at the price's line's end where the page has
        // it: the Golf on sale at 9 500 € does, and the house at 185 000 €, a rent doesn't; and
        // for the circle after it that weighs the price, the house's price per square metre (the
        // Golf's model has no prices to compare with here).
        let loan = matches!(number, 1001 | 1004);
        let compare = number == 1004;
        let at = home.find(&format!(r#"-{number}">"#)).unwrap_or_else(|| panic!("no card {number}"));
        let card = &home[at..];
        let card = &card[..card.find("</article>").unwrap()];
        let head = &card[card.find(r#"<div class="card-head""#).unwrap_or_else(|| panic!("{number}: {card}"))..];
        let head = &head[..head.find('>').unwrap()];
        assert_eq!(head.contains(" data-deal"), deal, "{number}: {head}");
        if number != 1005 {
            assert_eq!(head.contains(" data-loan"), loan, "{number}: {head}");
        }
        assert_eq!(head.contains(" data-compare"), compare, "{number}: {head}");
        let link = r#"<a class="card-link" href=""#;
        let (href, rest) = card[card.find(link).unwrap() + link.len()..].split_once(r#"">"#).unwrap();
        let named = &rest[..rest.find("</a>").unwrap()];
        let heading = head.split_once(r#"data-heading=""#).map_or(named, |(_, value)| &value[..value.find('"').unwrap()]);
        let page = get(pool.clone(), href).await.body;
        let h1 = &page[page.find("<h1>").unwrap() + "<h1>".len()..];
        assert_eq!(&h1[..h1.find("</h1>").unwrap()], heading, "{number}");
        // Its page's path too: the category above its own, and its own.
        let attribute = |name: &str| {
            let (_, value) = head.split_once(&format!(r#" {name}=""#)).unwrap_or_else(|| panic!("{number}: no {name} in {head}"));
            value[..value.find('"').unwrap()].to_string()
        };
        let path = &page[page.find(r#"<nav class="breadcrumb listing-path""#).unwrap()..];
        let path = &path[..path.find("</nav>").unwrap()];
        for (name, class) in [("data-up", "breadcrumb-up"), ("data-here", "breadcrumb-here")] {
            let link = &path[path.find(&format!(r#"class="{class}""#)).unwrap()..];
            let words = &link[link.find('>').unwrap() + 1..link.find("</a>").unwrap()];
            assert_eq!(attribute(name), words, "{number}: {name}");
        }
        // And the line under its title, in the page's words (those for screen readers aside).
        let kind = &page[page.find(r#"<p class="card-meta listing-kind">"#).unwrap()..];
        let kind = &kind[kind.find('>').unwrap() + 1..kind.find("</p>").unwrap()];
        let mut words = String::new();
        let mut rest = kind;
        while let Some(at) = rest.find('<') {
            words.push_str(&rest[..at]);
            let tag = &rest[at..];
            if tag.starts_with(r#"<span class="visually-hidden">"#) {
                rest = &tag[tag.find("</span>").unwrap() + "</span>".len()..];
            } else {
                rest = &tag[tag.find('>').unwrap() + 1..];
            }
        }
        words.push_str(rest);
        assert_eq!(attribute("data-line"), words.trim(), "{number}: the line under the title");
        // What kind of listing it is, as the page says it under the price.
        let kind = &page[page.find(r#"<span class="price-kind">"#).expect("the kind") + r#"<span class="price-kind">"#.len()..];
        assert_eq!(attribute("data-kind"), &kind[..kind.find("</span>").unwrap()], "{number}: its kind");
        // And on a car's or a property's page as many key facts as the page has.
        if deal {
            assert_eq!(attribute("data-facts"), page.matches(r#"<div class="key-fact">"#).count().to_string(), "{number}: its key facts");
        }
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn no_has_rule_watches_hidden_or_a_not(pool: PgPool) {
    // A :has() whose parentheses hold [hidden] or a :not() has Chrome restyle the whole box under
    // every element a :has() rule marks, each time any element's hidden changes: a category page
    // restyled all its results 33 times over as its cards' photo sliders started (Conventions, in
    // the README). A script sets a class instead.
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let mut seen = 0;
    for (at, _) in css.match_indices(":has(") {
        let rest = &css[at + ":has(".len()..];
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map(|(i, _)| i)
            .expect("a :has() closes");
        let inside = &rest[..end];
        assert!(!inside.contains("[hidden]") && !inside.contains(":not("), ":has({inside})");
        seen += 1;
    }
    assert!(seen > 10, "{seen}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_menu_leads_everywhere(pool: PgPool) {
    // Every page has the menu (base.html): its lines ("Menyja" for screen readers) at the bar's
    // left, before the logo and the account, opening a dialog that leads to posting, the account's
    // pages, every main category in the welcome page's order (its pills', from the database, which
    // browse::menu_categories keeps), the look (the text's three sizes, the dark theme's switch,
    // the six accents), the language through /gjuha, and the pages about the site.
    let home = get(pool.clone(), "/").await.body;
    for page in [home.clone(), get(pool.clone(), "/c/automjete").await.body] {
        assert!(page.contains(r#"<body data-controller="menu">"#));
        let bar = &page[page.find(r#"<header class="site-header">"#).unwrap()..page.find("</header>").unwrap()];
        let order = positions(bar, &[r#"<button type="button" class="menu-button""#, r#"<a class="logo" id="logo""#, r#"<a class="icon-button account-button""#]);
        assert!(order.is_sorted(), "{order:?}");
        assert!(bar.contains(r#"<span class="menu-button-word">Menyja</span>"#));
        let menu = &page[page.find(r#"<dialog class="menu" id="menu" aria-label="Menyja""#).expect("the menu")..];
        let menu = &menu[..menu.find("</dialog>").unwrap()];
        for href in ["/shto", "/njoftimet-e-mia", "/te-ruajturat", "/kerkimet-e-ruajtura", "/lajmerimet", "/mesazhet", "/llogaria/cilesimet", "/kontakt"] {
            assert!(menu.contains(&format!(r#"href="{href}""#)), "{href}");
        }
        // The size, compact, normal or large, three words, normal pressed (the user's ask of
        // 2026-10-11, "compact normal and large view", in place of the text's four sizes and the
        // page's four); and the six accents, mint first, the one there is unless another is
        // picked (lime before it), the royal blue last (the user's ask of 2026-10-12).
        let sizes: Vec<String> = ["compact", "normal", "large"].iter().map(|size| format!(r#"data-look-target="size" data-look-size-param="{size}" data-action="look#size">"#)).collect();
        let order = positions(menu, &[r#"<span id="menu-size">Madhësia</span><div class="segmented sizes" role="group" aria-labelledby="menu-size">"#, sizes[0].as_str(), sizes[1].as_str(), sizes[2].as_str()]);
        assert!(order.is_sorted(), "{order:?}");
        assert!(menu.contains(r#"<button type="button" aria-pressed="true" data-look-target="size" data-look-size-param="normal" data-action="look#size">Normale</button>"#));
        assert!(menu.contains(">Kompakte</button>") && menu.contains(">E madhe</button>"));
        assert!(!menu.contains("text-sizes") && !menu.contains("page-sizes") && !menu.contains("data-look-page-param"), "{menu}");
        let swatches: Vec<String> = ["mint", "lime", "coral", "orange", "blue", "royal"].iter().map(|accent| format!(r#"<input type="radio" class="swatch" name="accent" value="{accent}""#)).collect();
        let order = positions(menu, &swatches.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(order.is_sorted(), "{order:?}");
        assert!(menu.contains(r#"value="mint" aria-label="Nenexhik" checked "#) && menu.matches(" checked ").count() == 1);
        assert!(menu.contains(r#"value="royal" aria-label="Blu mbretërore" data-look-target="accent""#));
        assert!(menu.contains(r#"<input type="checkbox" class="switch" role="switch" data-controller="theme""#));
        assert!(menu.contains(r#"<form class="segmented" method="post" action="/gjuha""#));
        assert!(menu.contains(r#"<button type="submit" name="gjuha" value="sq" lang="sq" aria-pressed="true">Shqip</button>"#));
        // A row's words stand in a span of their own, closed inside its link, and a count is a span
        // after them, hidden until there is something to count (a "</span>" inside the count's tag
        // once showed its attributes as words, in the accent).
        assert!(!menu.contains("</a></span>"), "{menu}");
        for target in ["notifications", "messages"] {
            assert!(menu.contains(&format!(r#"</span><span class="menu-count" data-menu-target="{target}" hidden>"#)), "{target}");
        }
    }
    // Its categories are the welcome page's pills, in their order.
    let slugs = |part: &str| -> Vec<String> {
        part.match_indices(r#"href="/c/"#).map(|(at, _)| part[at + 9..].split('"').next().unwrap().to_string()).collect()
    };
    let pills = &home[home.find(r#"<nav class="shortcuts tiles" data-controller="drag-row" aria-label="Kategoritë">"#).unwrap()..];
    let pills = &pills[..pills.find("</nav>").unwrap()];
    let menu = &home[home.find(r#"<h2 id="menu-categories">"#).unwrap()..];
    let menu = &menu[..menu.find("</ul>").unwrap()];
    assert_eq!(slugs(menu).len(), 11);
    assert_eq!(slugs(menu), slugs(pills));
    // The look the visitor picked comes back before the first paint (web::THEME_SCRIPT): the
    // theme, the size and the accent, from the browser's storage; a text's or a page's size picked
    // before the sizes, once the size nearest it.
    for part in ["root.dataset.theme = theme", "root.dataset.size = size", "root.dataset.accent = accent"] {
        assert!(home.contains(part), "{part}");
    }
    assert!(home.contains(r#"if (size === "compact" || size === "large") root.dataset.size = size;"#));
    assert!(home.contains(r#"size = page || text === "smaller" || text === "small" ? "compact" : text === "large" ? "large" : size;"#));
    assert!(home.contains(r#"localStorage.removeItem("textSize"); localStorage.removeItem("pageSize");"#));
    assert!(home.contains(r#"if (["lime", "coral", "orange", "blue", "royal"].includes(accent)) root.dataset.accent = accent;"#));
    // Drawn as the canvas has it: white over the page dimmed, its counts in the accent, a link's
    // icon filled with the accent under the pointer; the text's size scales every size in rem.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".menu::backdrop{background:var(--scrim);}",
        // On a phone nine tenths of the screen, the page dimmed beside it, floating as it does
        // from 720px up (the user's asks of 2026-10-11; the whole screen until then).
        "@media (max-width:719px){.menu{width:min(380px,90%);}",
        // The sizes: large, the text bigger; compact, the whole page zoomed to 90%, or, with a
        // mouse, to 80% with the text bigger in it; the maps at their own scale, the frame's
        // column as wide on the screen as before.
        r#":root[data-size="large"]{--text-scale:1.1875;}"#,
        r#":root[data-size="compact"]{--page-scale:.9;}"#,
        r#"@media (pointer:fine){:root[data-size="compact"]{--page-scale:.8;--text-scale:1.1875;}}"#,
        "html{zoom:var(--page-scale);}",
        ":is(.map-canvas,.map-pin-canvas){zoom:calc(1",
        "--content-width:calc(1068px",
        "html{font-size:calc(100% * var(--text-scale));}",
        "a.menu-row:hover > svg{fill:var(--highlight);}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let count = &css[css.find("}.menu-count{").expect("the menu's counts") + 1..];
    assert!(count[..count.find('}').unwrap()].contains("background:var(--highlight)"));
    // Every accent but one as bright as lime (2026-10-11), the same in both themes and dark words on
    // each, mint when none is picked (the user's pick, the same day), a filter that is on taking as
    // much of it as each needs; a pick from before, a darker one, comes back as its bright one. The
    // royal blue deep (the user's ask of 2026-10-12): white words on it, and on the dark theme, where
    // the accent is a line, a lighter blue for it.
    for rule in [
        "--choice-mint:#2de38c;",
        "--choice-lime:#c8ff00;",
        "--choice-coral:#ff6b6b;",
        "--choice-orange:#ff9f1c;",
        "--choice-blue:#4dabff;",
        "--choice-royal:#275bff;",
        ":root{--highlight:var(--choice-mint);--on-highlight:#1b1b1b;--highlight-line:var(--highlight);--filter-share-light:38%;--filter-share-dark:27%;--text-share:45%;}",
        r#":root[data-accent="royal"]{--highlight:var(--choice-royal);--on-highlight:#fff;--highlight-line:color-mix(in srgb,var(--choice-royal) 70%,#fff);--filter-share-light:20%;--filter-share-dark:40%;}"#,
        "--highlight-icon:var(--highlight-line);",
        r#".swatch[value="royal"]{--swatch:var(--choice-royal);}"#,
        // Where the swatches no longer fit beside "Ngjyra", they go under it, as the size's words do.
        ".menu-row:has(> :is(.sizes,.swatches)){flex-wrap:wrap;",
        r#":root[data-accent="lime"]{--highlight:var(--choice-lime);--filter-share-light:45%;--filter-share-dark:24%;--text-share:38%;}"#,
        r#":root[data-accent="coral"]{--highlight:var(--choice-coral);--filter-share-light:28%;--filter-share-dark:38%;}"#,
        "--filter-on:color-mix(in srgb,var(--highlight) var(--filter-share),var(--bg));",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(r#"[data-accent="mint"]"#) && !css.contains("--lime:"));
    assert_eq!(css.matches("--on-highlight:").count(), 2);
    // A listing's price on a band of it across the figure's foot, as Tradera marks its prices
    // (2026-10-11): a thick underline under the letters, in the tint a filter that is on takes, as
    // quiet as the user asked, and as thin, reaching an eighth of an em up the figure ("a bit
    // smaller, going less into the top"). The price as large and as bold as the title (the user's
    // ask of the same day; the headline before, 36px and a step bolder): the page's title size,
    // 22px and 19px on a phone since the user asked that evening for the page's text a step
    // smaller, as YouTube's (--lp-title, with the page's other sizes, on .listing-page).
    for rule in [
        "--price-mark:var(--filter-on);",
        ".price-amount{text-decoration-line:underline;text-decoration-color:var(--price-mark);text-decoration-thickness:.26em;text-decoration-skip-ink:none;text-underline-offset:-.12em;}",
        ".listing-page{--lp-title:1.375rem;--lp-price:calc(24px * var(--text-scale));--lp-head:1.0625rem;--lp-body:.875rem;--lp-small:.8125rem;--lp-tiny:.75rem;}@media (max-width:719px){.listing-page{--lp-title:1.357rem;--lp-head:1.0714rem;--lp-body:1rem;--lp-small:.857rem;--lp-tiny:.8214rem;}}",
        ".listing-header h1{margin:.75rem 0 .35rem;font-size:var(--lp-title);letter-spacing:-.02em;line-height:1.25;}.listing-price{margin:0;font-size:var(--lp-price);line-height:1.15;font-weight:var(--bold);letter-spacing:-.02em;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(home.contains(r#"const older = { darkred: "coral", terracotta: "orange", olive: "mint" };"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_rail_and_the_bars_search_lead_everywhere(pool: PgPool) {
    // The rail, the menu folded (base.html, rail.rs): the open menu's places in icons alone, as the
    // user asked on 2026-10-11 ("a replica of the opened menu in the sidebar of the desktop version.
    // Use only icons"): posting, the account's pages, every main category, the dark theme's switch
    // and the other language, in that order on every page, each its name for screen readers and
    // the pointer, the one the page is marked as it (a category's page its main category).
    for (path, current) in [
        ("/", ""),
        ("/c/automjete", r#"<a class="rail-item" href="/c/automjete" title="Automjete" aria-current="page">"#),
        ("/c/makina", r#"<a class="rail-item" href="/c/automjete" title="Automjete" aria-current="page">"#),
        ("/search", ""),
        ("/te-ruajturat", r#"<a class="rail-item rail-saved" href="/te-ruajturat" title="Të ruajturat" aria-current="page">"#),
        ("/mesazhet", r#"<a class="rail-item" href="/mesazhet" title="Mesazhet" aria-current="page" data-controller="inbox">"#),
        ("/listing/1001-volkswagen-golf-7-2016-nafte", ""),
    ] {
        let page = html(pool.clone(), path).await;
        let rail = &page[page.find(r#"<nav class="rail" aria-label="Faqet kryesore">"#).unwrap_or_else(|| panic!("the rail on {path}"))..];
        let rail = &rail[..rail.find("</nav>").unwrap()];
        let order = positions(rail, &[
            r#"<a class="rail-post" href="/shto" aria-label="Posto njoftim""#,
            r#"<a class="rail-item" href="/njoftimet-e-mia" title="Njoftimet e mia""#,
            r#"<a class="rail-item rail-saved" href="/te-ruajturat" title="Të ruajturat""#,
            r#"<a class="rail-item" href="/kerkimet-e-ruajtura" title="Kërkimet e ruajtura""#,
            r#"<a class="rail-item" href="/lajmerimet" title="Lajmërimet""#,
            r#"<a class="rail-item" href="/mesazhet" title="Mesazhet""#,
            r#"<a class="rail-item" href="/llogaria/cilesimet" title="Cilësimet""#,
            r#"<hr class="rail-line">"#,
            r#"<a class="rail-item" href="/c/automjete" title="Automjete""#,
            r#"<a class="rail-item" href="/c/prona" title="Prona""#,
            r#"<button type="button" class="rail-item rail-theme" title="Tema e errët" aria-pressed="false" data-controller="theme" data-action="theme#toggle">"#,
            r#"<form class="rail-language" method="post" action="/gjuha">"#,
            r#"<button type="submit" class="rail-item" name="gjuha" value="en" lang="en" title="English">"#,
        ]);
        assert!(order.is_sorted(), "{path}: {order:?}");
        assert_eq!(rail.matches(r#"<a class="rail-item" href="/c/"#).count(), 11, "every main category: {rail}");
        assert_eq!(rail.matches(r#"<hr class="rail-line">"#).count(), 2);
        assert_eq!(rail.matches(r#"aria-current="page""#).count(), usize::from(!current.is_empty()), "{path}: {rail}");
        assert!(rail.contains(current), "{path}: {rail}");
        // Icons alone: their names for screen readers, and the counts of what is new.
        assert!(rail.contains(r#"<span class="rail-label">Njoftimet e mia</span>"#) && rail.contains(r#"<span class="rail-label">Automjete</span>"#));
        assert_eq!(rail.matches(r#"<span class="inbox-count" data-inbox-target="count" hidden></span>"#).count(), 2);
        assert!(rail.contains(r#"data-controller="inbox" data-inbox-url-value="/lajmerimet/numri" data-inbox-name-value="Lajmërimet" data-inbox-counted-value="Lajmërimet: {} të reja">"#));
        assert!(!rail.contains(r#"href="/search""#) && !rail.contains(r#"<a class="rail-item" href="/""#), "{rail}");
    }
    // The bar's search, on every page but the front page, whose own is its first thing: in every
    // category and town ("Kërko njoftime"), but on a category's page, which it searches with its
    // filters kept, saying which; the row's own search is gone.
    let home = html(pool.clone(), "/").await;
    assert!(!home.contains(r#"<form class="top-search""#));
    let listing = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert!(listing.contains(r#"<form class="top-search" action="/search" method="get" role="search" data-controller="suggest""#));
    assert!(listing.contains(r#"placeholder="Kërko njoftime" aria-label="Kërko njoftime""#));
    let cars = html(pool.clone(), "/c/makina?sort=price_asc").await;
    let bar = &cars[cars.find(r#"<header class="site-header">"#).unwrap()..cars.find("</header>").unwrap()];
    assert!(bar.contains(r#"<form class="top-search" action="/c/makina" method="get" role="search" data-controller="suggest""#), "{bar}");
    assert!(bar.contains(r#"placeholder="Kërko në Makina" aria-label="Kërko në Makina""#), "{bar}");
    assert!(bar.contains(r#"<input type="hidden" name="sort" value="price_asc">"#), "{bar}");
    assert!(!cars.contains("search-card") && !cars.contains("search-panel"));
    // Drawn as the user drew them: posting's square in "Posto"'s colours, the current place on a
    // grey pill; the searches grey pills, the front page's button the accent's. (The stylesheet
    // comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    assert!(rule(".rail-post").contains("background:var(--post)"));
    assert!(css.contains(r#".rail-item[aria-current="page"] .rail-icon,.rail-item[aria-pressed="true"] .rail-icon{background:var(--surface-muted);}"#));
    assert!(css.contains(".rail-label{position:absolute;width:1px;height:1px;overflow:hidden;clip-path:inset(50%);white-space:nowrap;}"));
    assert!(css.contains("@media (min-width:720px){:root{--rail-width:72px;}"));
    assert!(rule(".top-search").contains("background:var(--surface-muted)"));
    assert!(rule(".welcome-form").contains("background:var(--surface-muted)"));
    assert!(rule(".welcome-search").contains("color:var(--highlight-icon)") && !rule(".welcome-search").contains("background"));
    // The bar's search stands in the middle of the page beside the rail: the bar's start and end
    // grow alike around it. A filter that is on is in the accent, toned down (2026-10-11).
    assert!(bar.contains(r#"<div class="header-start">"#) && bar.contains(r#"<div class="header-end">"#), "{bar}");
    assert!(css.contains(".header-start,.header-end{flex:1 1 0;min-width:max-content;}"));
    // On phones the logo stands in the bar's middle, the menu's lines at its left and the account
    // at its right: three columns, the outer two as wide as each other, the bar as far from both
    // edges of the screen (the user's ask of 2026-10-11).
    assert!(css.contains(".header-inner{display:grid;grid-template-columns:minmax(0,1fr) auto minmax(0,1fr);grid-template-rows:var(--size-header-row);align-items:center;gap:0;padding:0 calc(var(--gutter) - 8px);}"), "{css}");
    assert!(css.contains(".header-inner > .menu-button{margin:0;}") && css.contains(".header-inner > .top-search{grid-column:1 / -1;grid-row:2;max-width:none;margin:0 8px;}"));
    assert!(rule(".filter-chip").contains("background:var(--filter-on)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_footer_is_drawn_as_the_canvas_has_it(pool: PgPool) {
    // The footer: the links in the muted grey at the left and the copyright at the right, under a
    // hairline; the other language underlined; the page a column at least a screen tall, so that
    // the footer stands at its foot under a short page too. (The stylesheet comes minified.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let footer = rule(".site-footer");
    for part in ["justify-content:space-between", "border-top:1px solid var(--border)", "color:var(--muted)"] {
        assert!(footer.contains(part), "{part} in {footer}");
    }
    assert!(rule(".footer-links a").contains("min-height:var(--size-button-sm)"));
    assert!(rule(".language-switch button").contains("text-decoration:underline"));
    let body = rule("body");
    // At least the screen's height, the page's own zoom divided out (--page-scale).
    assert!(body.contains("flex-direction:column") && body.contains("min-height:calc(100svh"), "{body}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_short_descriptions_fold_button_stays_hidden(pool: PgPool) {
    // The button that unfolds a long description waits with the hidden attribute until the
    // controller finds the description long (fold_controller.js). Its own display rule would
    // override the attribute and show it under a two-line description too, so the stylesheet
    // hides it again by the attribute.
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let shown = css.find(".description-more{").expect("the button's rule");
    let hidden = css.find(".description-more[hidden]{display:none;}").expect("the hidden rule");
    assert!(shown < hidden, "the hidden rule comes after the button's, to win");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_select_is_chosen_from_a_list_in_the_page(pool: PgPool) {
    // Each select comes inside the picker (picker_controller.js), which keeps it for the form and
    // offers its options in a list in the page instead of the phone's own picker; a category's
    // list folds, its subcategories under it. (A picker may have a class of its own besides, as the
    // map's towns have.)
    let plain = r#"data-controller="picker">"#;
    let folding = r#"<div class="picker" data-controller="picker" data-picker-tree-value="true">"#;
    let ana = new_seller(&pool, "+355691112233").await;
    // (The form's currency is a two-part toggle of radios, as the filters' price has, since the
    // canvas: a list no longer.)
    let pages = [
        ("/shto", 6),
        ("/shto/fushat?category=makina", 11),
        ("/search?q=golf", 4),
        ("/c/makina", 16),
        ("/harta", 2),
    ];
    // (The price's currency is a two-part toggle of radios, no list.)
    for (uri, selects) in pages {
        let page = get_with_cookie(pool.clone(), uri, &ana).await.body;
        // The typeface panel's two lists (TYPE_TRIAL, base.html) are the browser's own: not the
        // page's forms'.
        let found: Vec<usize> = page
            .match_indices("<select")
            .map(|(at, _)| at)
            .filter(|at| !page[*at..*at + page[*at..].find('>').unwrap()].contains(r#"class="type-font""#))
            .collect();
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
