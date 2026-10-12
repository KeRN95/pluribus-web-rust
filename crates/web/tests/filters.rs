//! The filters (web/filters.rs): the sheet, and the row of dropdowns beside the map and the order;
//! the place, the zone and the price, its slider, its quick choices, its currencies and the euro
//! rate the listings in lekë are priced by; a category's properties and a part's vehicle; the chips
//! that take each off, and values that mean nothing.

mod common;

use axum::http::StatusCode;
use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filters_map_and_order_stand_side_by_side(pool: PgPool) {
    // Filtrat, Harta and Rendit stand in one row over the results (the search is the top bar's),
    // the orders opening in a menu; opening one says so (section:open) and the others fold, so only
    // one is open at a time. What they open floats over the results, the page dimmed under it;
    // Escape, or a tap on the dim or on the row beside its buttons, folds it again.
    // The filters' card comes after the row and the places under it: a column beside the results
    // on a wide screen (browse.css).
    let cars = html(pool.clone(), "/c/makina").await;
    let order = positions(&cars, &[
        // The heading's line first: the page's name, how many, and "Ruaj kërkimin".
        r#"<h1 class="browse-title">Makina</h1>"#,
        r#"<p class="result-count" id="result-count""#,
        r#"<form class="save-search" method="post" action="/kerkimet/ruaj">"#,
        r#"<div class="browse-tools" data-filters-target="tools" data-action="click->filters#foldFromRow">"#,
        r#"<button type="button" class="button secondary round-button labelled filters-open""#,
        r#"<section class="map-card map-hidden""#,
        // After the map, how the cards show, round, opening its menu; last, the order.
        r#"<details class="sort-menu cards-menu" data-controller="dropdown" data-cards-target="menu""#,
        r#"<summary class="button secondary round-button cards-button" aria-label="Pamja: Kartela të vogla""#,
        r#"<summary class="sort-button" aria-label="Rendit: Më të rejat" title="Rendit: Më të rejat">Më të rejat<svg"#,
        r#"<aside class="filters" id="filters" aria-labelledby="filters-heading" data-filters-target="panel">"#,
        r#"<div class="browse-scrim" aria-hidden="true" data-filters-target="scrim" data-action="click->filters#foldAll"></div>"#,
        // Then the listings, under their heading, their order.
        r#"<section class="results""#,
        r#"<div class="results-head">"#,
        r#"<h2 class="results-heading" id="results-heading">Njoftimet më të reja</h2>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    for listener in ["filters#foldFor", "map#foldFor", "dropdown#foldFor", "filters#fit"] {
        assert!(cars.contains(&format!("section:open@document->{listener}")), "{listener}");
    }
    assert!(cars.contains("keydown.esc@document->filters#foldAll"));
    // On a phone the sheet is nine tenths of the screen, at its left as the menu is, the page
    // dimmed beside it, a tap there folding it (the user's ask of 2026-10-11; the whole screen
    // until then).
    let site_css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    assert!(site_css.contains("@media (max-width:719px){.filters{right:auto;left:0;width:min(460px,90%);}"), "{site_css}");
    // The filters and the map apply as soon as something changes, the filters also as they close,
    // the page refreshing in place, which keeps what is open as it is and each listing's card;
    // only without JavaScript do the filters have a button.
    assert!(cars.contains(r#"data-action="change->filters#changed keydown.enter->filters#enter">"#));
    assert!(cars.contains(r#"<noscript><div class="filter-actions">"#));
    for keep in [
        "turbo:before-morph-element->filters#keep",
        "turbo:before-morph-attribute->filters#keepAttribute",
        "turbo:before-morph-element->map#keep",
        "turbo:before-morph-attribute->map#keepOpen",
        "turbo:morph@document->map#reloadPoints",
    ] {
        assert!(cars.contains(keep), "{keep}");
    }
    assert!(cars.contains(r#"<article class="card" id="listing-1001">"#));
    // The filters' and the map's buttons say their names and what is on: "Filtrat", with how many
    // (none yet), and the map's place, "Harta" with none; the cards' view has its icon alone.
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button labelled map-button" data-map-target="toggle""#));
    assert!(cars.contains(r#"<span class="button-label">Harta</span>"#));
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button labelled filters-open" aria-controls="filters" aria-expanded="false""#));
    assert!(cars.contains(r#"<span class="button-label">Filtrat</span>"#) && !cars.contains("button-count"));
    assert!(!cars.contains("round-button search-button"));
    // The order is the row's last button, its words and a chevron, the orders in a menu under it
    // (open with no JavaScript); the listings' heading says it: "Njoftimet më të reja" on a
    // category page.
    let row = &cars[cars.find(r#"<div class="browse-tools""#).unwrap()..cars.find(r#"<aside class="filters""#).unwrap()];
    assert_eq!(row.matches("cards-button").count(), 1);
    assert!(row.contains(r#"<fieldset class="sort-options">"#));
    assert!(!cars[cars.find(r#"<div class="results-head">"#).unwrap()..].contains("sort-menu"));
    let cheap = html(pool.clone(), "/c/makina?sort=price_asc").await;
    assert!(cheap.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të lira</h2>"#), "{cheap}");
    assert!(cars.contains("toggle->dropdown#announce"));
    // A search's listings are headed by the order alone, the best match first for words.
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<summary class="sort-button" aria-label="Rendit: Më të përshtatshmet" title="Rendit: Më të përshtatshmet">Më të përshtatshmet<svg"#));
    assert!(search.contains(r#"<h2 class="results-heading" id="results-heading">Më të përshtatshmet</h2>"#));

    // The search is the top bar's (base.html, browse.html), the row has none: it searches the
    // page's category and keeps its filters.
    let header = &cars[..cars.find("</header>").unwrap()];
    assert!(header.contains(r#"name="q""#) && !row.contains(r#"name="q""#));
    let fuel = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&sort=price_asc").await;
    assert!(fuel.contains(r#"<span class="button-label">Filtrat</span><span class="button-count" aria-hidden="true">1</span>"#));
    let search = &fuel[fuel.find(r#"<form class="top-search" action="/c/makina""#).expect("the bar's search")..];
    let search = &search[..search.find("</form>").unwrap()];
    assert!(search.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(search.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(search.contains(r#"<input type="search" name="q" value="""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_filters_buyers_use_most_come_first(pool: PgPool) {
    // First the place and the price, then what the category asks of every listing or shows on its
    // cards, and a few more (Karroceria, Kambio); the rest under "Më shumë filtra", folded.
    let filter_form = |page: &str| {
        let form = &page[page.find(r#"<form id="filter-form""#).unwrap()..];
        form[..form.find("</form>").unwrap()].to_string()
    };
    let cars = filter_form(&html(pool.clone(), "/c/makina").await);
    let more = cars.find(r#"<details class="more-filters">"#).expect("Më shumë filtra");
    let at = |name: &str| cars.find(&format!(r#"name="{name}""#)).unwrap_or_else(|| panic!("{name}"));
    for name in [
        "city", "price_min", "vehicle_make", "vehicle_model", "vehicle_body_type", "vehicle_year_min",
        "vehicle_mileage_max", "vehicle_fuel", "vehicle_transmission",
    ] {
        assert!(at(name) < more, "{name} comes first");
    }
    for name in ["vehicle_engine_min", "vehicle_power_min", "vehicle_color", "vehicle_features", "vehicle_no_accident"] {
        assert!(at(name) > more, "{name} waits under Më shumë filtra");
    }
    // It opens with the page while one of them is on.
    let white = html(pool.clone(), "/c/makina?vehicle_color=WHITE").await;
    assert!(white.contains(r#"<details class="more-filters" open>"#));
    // A category with few has them all at once: a part's make, model and year.
    assert!(!html(pool.clone(), "/c/pjese-dhe-aksesore").await.contains("more-filters"));
    // The kind of listing comes first for property (Shitje, Qira), before the price.
    let flats = filter_form(&html(pool, "/c/apartamente").await);
    assert!(flats.find(r#"name="type""#).unwrap() < flats.find(r#"name="price_min""#).unwrap());
    let more = flats.find("more-filters").unwrap();
    assert!(flats.find(r#"name="property_rooms""#).unwrap() < more);
    assert!(flats.find(r#"name="property_heating""#).unwrap() > more);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn years_and_kilometres_are_picked_from_steps(pool: PgPool) {
    // A range whose unit has steps offers them in two dropdowns, from and to: the years from next
    // year back to 1970, the kilometres in round numbers. A value from an older link, not a step,
    // stands in its place among them.
    let cars = html(pool.clone(), "/c/makina?vehicle_year_min=2015&vehicle_mileage_max=123456").await;
    assert!(cars.contains(r#"<select name="vehicle_year_min" aria-label="Viti nga">"#));
    assert!(cars.contains(r#"<option value="">nga</option>"#) && cars.contains(r#"<option value="">deri</option>"#));
    assert!(cars.contains(r#"<option value="2015" selected>2015</option>"#));
    assert!(cars.contains(r#"<option value="1970">1970</option>"#) && !cars.contains(r#"<option value="1969">"#));
    assert!(cars.contains(r#"<select name="vehicle_mileage_max" aria-label="Kilometrazhi deri">"#));
    assert!(cars.contains(
        "<option value=\"100000\">100\u{a0}000</option><option value=\"123456\" selected>123\u{a0}456</option><option value=\"125000\">125\u{a0}000</option>"
    ));
    // A range without steps is typed.
    let phones = html(pool, "/c/telefona").await;
    assert!(phones.contains(r#"<input type="text" inputmode="numeric" name="phone_battery_min" value="" placeholder="nga""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn property_narrows_to_a_zone_of_its_town(pool: PgPool) {
    in_zone(&pool, 1003, "tirane", "komuna-e-parisit").await;
    // In its zone, the flat; in another of Tirana's, nothing, and a way to the whole town.
    let zone = html(pool.clone(), "/c/apartamente?city=tirane&zone=komuna-e-parisit").await;
    assert_eq!(card_numbers(&zone), [1003]);
    let other = html(pool.clone(), "/c/apartamente?city=tirane&zone=blloku").await;
    assert!(card_numbers(&other).is_empty());
    assert!(other.contains(r#"<a class="button secondary" href="/c/apartamente?city=tirane">Kërko në gjithë qytetin</a>"#));
    // Zona has every town's zones under it, those of the town picked showing, the zone selected.
    assert!(zone.contains(r#"<optgroup label="Tiranë" data-parent="tirane">"#));
    assert!(zone.contains(r#"<option value="komuna-e-parisit" selected>Komuna e Parisit</option>"#));
    assert!(zone.contains(r#"<optgroup label="Durrës" data-parent="durres" hidden>"#));
    // The zone's chip leaves the town; the town's takes the zone with it.
    assert!(zone.contains(r#"<a class="filter-chip" href="/c/apartamente?city=tirane" aria-label="Hiq filtrin Komuna e Parisit">"#));
    assert!(zone.contains(r#"<a class="filter-chip" href="/c/apartamente" aria-label="Hiq filtrin Tiranë" data-forget-town"#));
    // A town without zones keeps Zona hidden, and a category whose listings have none has none.
    let shkoder = html(pool.clone(), "/c/apartamente?city=shkoder").await;
    assert!(shkoder.contains(r#"<div class="filter-group filter-zone" data-filters-target="zone" hidden>"#));
    assert!(!html(pool.clone(), "/c/makina?city=tirane").await.contains("filter-zone"));
    // A zone goes with its town: without it, in another town or outside property, it is nothing.
    let lone = html(pool.clone(), "/c/apartamente?zone=komuna-e-parisit").await;
    assert!(card_numbers(&lone).contains(&1003) && !lone.contains("Hiq filtrin Komuna e Parisit"));
    assert!(!html(pool.clone(), "/c/apartamente?city=durres&zone=komuna-e-parisit").await.contains("Hiq filtrin Komuna"));
    let cars = html(pool, "/c/makina?city=tirane&zone=blloku").await;
    assert_eq!(card_numbers(&cars), [1001]);
    assert!(!cars.contains("Hiq filtrin Blloku"));
}

/// The names on the row's filter dropdowns, in their order (filter_drop.html).
fn dropdown_names(page: &str) -> Vec<&str> {
    let pill = r#"<summary class="button secondary round-button labelled filter-pill">"#;
    page.match_indices(pill)
        .map(|(at, _)| {
            let name = &page[at + pill.len()..];
            &name[..name.find("<svg").unwrap()]
        })
        .collect()
}

/// One of the row's filter dropdowns, from its pill to its end.
fn dropdown<'a>(page: &'a str, name: &str) -> &'a str {
    let start = page.find(&format!(r#"labelled filter-pill">{name}<svg"#)).unwrap_or_else(|| panic!("no {name}"));
    &page[start..start + page[start..].find("</details>").unwrap()]
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_row_offers_the_filters_buyers_use_most_as_dropdowns(pool: PgPool) {
    // After Filtrat, as the canvas has them: a car's make, model and year, the price fourth, and
    // its fuel; its body types (with icons) and kilometres (a range in a unit) are the sheet's.
    let cars = html(pool.clone(), "/c/makina").await;
    assert_eq!(dropdown_names(&cars), ["Marka", "Modeli", "Viti", "Çmimi", "Karburanti"]);
    // A main category has the condition, the price and the kind of listing; a search across every
    // category starts with the town.
    assert_eq!(dropdown_names(&html(pool.clone(), "/c/automjete").await), ["Gjendja", "Çmimi", "Lloji i njoftimit"]);
    assert_eq!(dropdown_names(&html(pool.clone(), "/search?q=golf").await), ["Qyteti", "Çmimi", "Gjendja"]);
    // A filter that is on is a chip in place of its dropdown, and the row has five at most, chips
    // and dropdowns together: with the fuel on, four dropdowns; with a price, none for the price;
    // with five filters on, none.
    let diesel = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&sort=price_asc").await;
    assert_eq!(dropdown_names(&diesel), ["Marka", "Modeli", "Çmimi", "Viti"]);
    assert_eq!(dropdown_names(&html(pool.clone(), "/c/makina?price_max=9000").await), ["Marka", "Modeli", "Viti", "Karburanti"]);
    let five = "/c/makina?city=tirane&vehicle_fuel=DIESEL&vehicle_transmission=AUTOMATIC&condition=USED&price_max=9000";
    assert!(dropdown_names(&html(pool.clone(), five).await).is_empty());
    // With room for two, the price comes after the lone other.
    let three = "/c/makina?vehicle_make=AUDI&vehicle_fuel=PETROL&vehicle_fuel=HYBRID";
    assert_eq!(dropdown_names(&html(pool.clone(), three).await), ["Modeli", "Çmimi"]);

    // Each opens a card under it, a form of its own that carries the rest of the page along, its
    // order too: a make picked from its list (the picker, inline, "Të gjitha" there for a page
    // without scripts) applies at once, with no button.
    let make = dropdown(&diesel, "Marka");
    assert!(make.contains(r#"<form class="drop-card filter-form" action="/c/makina" method="get""#));
    assert!(make.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(make.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(make.contains(r#"<div class="picker drop-list" data-picker-inline-value="true" data-controller="picker">"#));
    assert!(make.contains(r#"<select name="vehicle_make" aria-label="Marka" data-action="change->filter-drop#picked">"#));
    assert!(!make.contains("drop-apply") && !make.contains(r#"type="hidden" name="vehicle_make""#));
    // The models come under their makes.
    assert!(dropdown(&diesel, "Modeli").contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    // The price's card is the sheet's price, with a button at its foot; it carries the fuel, and
    // no price but its own.
    let price = dropdown(&diesel, "Çmimi");
    assert!(price.contains(r#"<fieldset class="filter-group price-filter" data-controller="price-range">"#));
    assert!(price.contains(r#"<button type="submit" class="button small drop-apply">Shfaq rezultatet</button>"#));
    assert!(price.contains(r#"name="vehicle_fuel" value="DIESEL""#) && !price.contains(r#"type="hidden" name="price_max""#));
    // The year's two lists, from and to, and its button.
    let year = dropdown(&diesel, "Viti");
    assert_eq!(year.matches("<select").count(), 2);
    assert!(year.contains("drop-apply"));
    // Opening one folds the row's others, and each applies through its own controller.
    assert!(diesel.contains(r#"<details class="filter-drop" data-controller="dropdown filter-drop""#));
    assert!(diesel.contains("toggle->filter-drop#toggled section:open@document->dropdown#foldFor"));
    // Without JavaScript they are gone, the sheet's form in the page.
    assert!(cars.contains(".filters-open, .filter-drop, .map-card,"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn city_filter_keeps_to_that_city(pool: PgPool) {
    let body = get(pool, "/search?city=durres").await.body;
    assert!(body.contains(r#"<h1 class="browse-title">Njoftimet në Durrës</h1>"#));
    assert!(body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("Kërkohet kamarier/e me eksperiencë"));
    assert!(!body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(body.contains(r#"<p class="result-count" id="result-count" data-show="Shfaq 2 njoftime">2 njoftime</p>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn welcome_form_narrows_to_a_category_and_city(pool: PgPool) {
    let body = html(pool, "/search?category=automjete&city=tirane").await;
    assert!(body.contains(r#"<h1 class="browse-title">Automjete</h1>"#));
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"), "the Mercedes is in Durrës");
    assert!(!body.contains("iPhone 13"), "the iPhone is in another category");
    // Both are filters here, so both can be removed.
    assert!(body.contains(r#"<a class="filter-chip" href="/search?city=tirane""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/search?category=automjete""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_bank_of_albanias_euro_rate_reprices_the_listings_in_leke(pool: PgPool) {
    use pluribus_db as db;
    use rust_decimal::Decimal;
    let number = |text: &str| text.parse::<Decimal>().unwrap();
    let day = time::Date::from_calendar_date(2026, time::Month::October, 9).unwrap();
    let iphone = "iPhone 13, 128 GB, si i ri";
    let euros = |pool: PgPool, no: i64| async move {
        sqlx::query_scalar::<_, Decimal>("SELECT price_normalized FROM listings WHERE public_no = $1")
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    // The placeholder: 100 lekë a euro, every price worked out at it.
    let seeded = db::euro_rate(&pool).await.unwrap();
    assert_eq!((seeded.lek_per_eur, seeded.rate_date, seeded.reprice_due), (number("100.00"), None, false));
    assert_eq!(pluribus_services::rates::reprice(&pool).await.unwrap(), 0);
    // Every listing last changed a day ago (which the trigger lets a statement say only so).
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.keep_updated_at', 'on', true)").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE listings SET updated_at = now() - interval '1 day'").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    // The official 91.65 is taken; the listings priced in lekë are due to be worked out again.
    assert!(db::take_euro_rate(&pool, number("91.65"), day).await.unwrap());
    let taken = db::euro_rate(&pool).await.unwrap();
    assert_eq!((taken.eur_rate, taken.lek_per_eur, taken.rate_date, taken.reprice_due), (number("0.01091107"), number("91.65"), Some(day), true));

    // Worked out again: every one priced in lekë (the iPhone's 55 000 Lekë is 600,11 € now, not
    // 550), those in euros as they were, and none of them changed as far as its updated_at, which
    // the sitemaps give, says.
    let in_leke: i64 = sqlx::query_scalar("SELECT count(*) FROM listings WHERE currency = 'ALL' AND price_amount IS NOT NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pluribus_services::rates::reprice(&pool).await.unwrap(), in_leke as u64);
    assert_eq!(euros(pool.clone(), 1005).await, number("600.11"));
    assert_eq!(euros(pool.clone(), 1001).await, number("9500.00"));
    let untouched: bool = sqlx::query_scalar("SELECT bool_and(updated_at < now() - interval '23 hours') FROM listings").fetch_one(&pool).await.unwrap();
    assert!(untouched);
    assert!(!db::euro_rate(&pool).await.unwrap().reprice_due);
    assert_eq!(pluribus_services::rates::reprice(&pool).await.unwrap(), 0);
    // The price filter in euros goes by it.
    assert!(!get(pool.clone(), "/c/elektronike?price_max=600&currency=EUR").await.body.contains(iphone));
    assert!(get(pool.clone(), "/c/elektronike?price_max=601&currency=EUR").await.body.contains(iphone));

    // A rate a few hundredths off isn't worth rewriting the prices for; half a percent off is.
    assert!(!db::take_euro_rate(&pool, number("91.40"), day).await.unwrap());
    assert!(db::take_euro_rate(&pool, number("91.10"), day).await.unwrap());
    pluribus_services::rates::reprice(&pool).await.unwrap();
    assert_eq!(euros(pool.clone(), 1005).await, number("603.73"));
    // A listing edited meanwhile changes as ever.
    sqlx::query("UPDATE listings SET title = title WHERE public_no = 1005").execute(&pool).await.unwrap();
    let edited: bool = sqlx::query_scalar("SELECT updated_at > now() - interval '1 minute' FROM listings WHERE public_no = 1005")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(edited);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_price_is_a_slider_with_two_ends(pool: PgPool) {
    // Without a price, the ends stand at the edges: no limit either way, and the fields to type
    // the prices in are empty.
    let any = html(pool.clone(), "/c/elektronike").await;
    assert!(any.contains(r#"<legend class="filter-label">Çmimi <output data-price-range-target="label">Çdo çmim</output></legend>"#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="0" aria-label="Çmimi nga, në shkallë""#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="1000" aria-label="Çmimi deri, në shkallë""#));
    assert!(any.contains(r#"<input type="text" inputmode="numeric" name="price_min" value="" placeholder="nga" aria-label="Çmimi nga""#));
    // 1 000 € is two of the scale's five decades from 10 €, so 400 of 1 000; 20 000 € is
    // log10(2 000) = 3.30 of them, so 660. The typed fields carry the prices themselves.
    let some = html(pool, "/c/elektronike?price_min=1000&price_max=20000&currency=EUR").await;
    assert!(some.contains(r#"value="400" aria-label="Çmimi nga, në shkallë""#));
    assert!(some.contains(r#"value="660" aria-label="Çmimi deri, në shkallë""#));
    assert!(some.contains(r#"<input type="text" inputmode="numeric" name="price_min" value="1000" placeholder="nga" aria-label="Çmimi nga" autocomplete="off""#));
    assert!(some.contains(r#"data-price-range-target="min" data-action="change->price-range#typed">"#));
    assert!(some.contains("<output data-price-range-target=\"label\">1\u{a0}000 – 20\u{a0}000\u{a0}€</output>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_price_has_quick_choices_by_the_categorys_prices(pool: PgPool) {
    // Too few prices to go by: no quick choices. (Worked out once for some hours, so not asked of
    // Të tjera before its prices come.)
    assert!(!html(pool.clone(), "/c/makina").await.contains("price-choices"));
    // 24 more in Të tjera, at 1 000 to 24 000 Lekë: a quarter of them up to 6 000, half up to
    // 12 000, three quarters up to 18 000, each kept to round figures (20 000).
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                g * 1000, 'ALL', 'FIXED', g * 10, 'ACTIVE', now()
         FROM generate_series(1, 24) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let page = html(pool.clone(), "/c/te-tjera").await;
    // A choice's button up to its label, its whitespace one space whatever the template's line
    // endings, and as it should be.
    let button = |page: &str, label: &str| {
        let end = page.find(&format!(">{label}</button>")).unwrap_or_else(|| panic!("{label}"));
        let start = page[..end].rfind("<button").unwrap();
        page[start..end].split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let choice = |min: &str, max: &str, pressed: bool| {
        format!(
            r#"<button type="button" class="price-choice" aria-pressed="{pressed}" data-price-range-target="choice" data-action="price-range#pick" data-price-range-min-param="{min}" data-price-range-max-param="{max}" data-price-range-currency-param="ALL""#
        )
    };
    let labels = [
        "deri 6\u{a0}000\u{a0}Lekë",
        "6\u{a0}000 – 12\u{a0}000\u{a0}Lekë",
        "12\u{a0}000 – 20\u{a0}000\u{a0}Lekë",
        "nga 20\u{a0}000\u{a0}Lekë",
    ];
    for (label, (min, max)) in labels.into_iter().zip([("", "6000"), ("6000", "12000"), ("12000", "20000"), ("20000", "")]) {
        assert_eq!(button(&page, label), choice(min, max, false), "{label}");
    }
    let ends = labels.map(|label| format!(">{label}</button>"));
    let order = positions(&page, &ends.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(order.is_sorted(), "{order:?}");
    // The one the prices are shows pressed.
    let picked = html(pool, "/c/te-tjera?price_min=6000&price_max=12000&currency=ALL").await;
    assert_eq!(button(&picked, labels[1]), choice("6000", "12000", true));
    assert_eq!(button(&picked, labels[0]), choice("", "6000", false));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filter_form_offers_the_category_properties(pool: PgPool) {
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(cars.contains(r#"<legend class="filter-label">Karburanti</legend>"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_fuel" value="DIESEL">"#));
    assert!(cars.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#), "models are grouped by make");
    assert!(cars.contains("Gjendja"));
    // A few boxes share their line with the next field (Karburanti, five, beside Motorri); more
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
    // The options, many boxes, take a line of their own too...
    let features = cars.find(r#"<legend class="filter-label">Opsionet</legend>"#).unwrap();
    assert!(cars[..features].trim_end().ends_with(r#"<fieldset class="filter-group filter-options">"#));
    // ...as does a small field left without another beside it: a motorcycle's make, before the
    // eight boxes of its kinds.
    let motorcycles = get(pool.clone(), "/c/motocikleta").await.body;
    let make = motorcycles.find(r#"<label class="filter-label" for="filter-moto_make">"#).expect("the make");
    assert!(motorcycles[..make].trim_end().ends_with(r#"<div class="filter-group">"#));

    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains(r#"<h1 class="browse-title">Prona</h1>"#));
    assert!(property.contains(r#"<input type="radio" name="type" value="RENT">"#));
    assert!(!property.contains("Gjendja"), "property has no condition");
    // Rooms are a flat's and a house's, not every property's: a plot has none.
    assert!(!property.contains(r#"name="property_rooms""#));
    assert!(get(pool.clone(), "/c/apartamente").await.body.contains(r#"value="R21""#));

    // Kinds of electronics are subcategories now, not a filter.
    let electronics = get(pool.clone(), "/c/elektronike").await.body;
    assert!(electronics.contains(r#"<a class="shortcut" href="/c/laptope"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#));
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn chosen_filters_can_be_removed_one_by_one(pool: PgPool) {
    let body = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&city=durres&sort=price_desc").await;
    // Each chip links to the page without its own filter, in a fixed parameter order.
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?vehicle_fuel=DIESEL&sort=price_desc""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?city=durres&sort=price_desc""#));
    // "Pastro filtrat", and the card's "Pastro", drop the filters but keep the place, which has
    // its pills.
    assert!(body.contains(r#"<a class="clear-filters" href="/c/makina?city=durres&sort=price_desc">"#));
    assert!(body.contains(r#"<a class="button secondary filters-clear" href="/c/makina?city=durres&sort=price_desc">Pastro</a>"#));
    // The town's chip forgets the town kept for the next pages (application.js).
    assert!(body.contains(r#"href="/c/makina?vehicle_fuel=DIESEL&sort=price_desc" aria-label="Hiq filtrin Durrës" data-forget-town data-turbo-prefetch="false">"#));
    // The filters' button says which are on, to screen readers and as its tooltip, and how many;
    // the place is the pills' and the map's to say.
    assert!(body.contains(r#"aria-label="Filtrat: Naftë" title="Filtrat: Naftë""#));
    assert!(body.contains(r#"<span class="button-count" aria-hidden="true">1</span>"#));
    // The town is the filter form's first field, Qyteti, by county, with "Afër meje" beside it.
    // The form has no field for the search words, so it carries them along: filtering keeps them,
    // and the row's search changes them.
    let filter_form = |page: &str| {
        let form = &page[page.find(r#"<form id="filter-form""#).unwrap()..];
        form[..form.find("</form>").unwrap()].to_string()
    };
    let form = filter_form(&body);
    assert!(form.contains(r#"<select id="filter-city" name="city" data-action="filters#placed">"#));
    assert!(form.contains(r#"<option value="durres" selected>Durrës</option>"#));
    assert!(form.contains(r#"<button type="button" class="button secondary filter-locate" data-action="filters#locate">"#));
    assert!(!form.contains(r#"<input type="hidden" name="city""#) && !body.contains(r#"id="filter-q""#));
    let words = html(pool, "/c/makina?q=golf").await;
    assert!(filter_form(&words).contains(r#"<input type="hidden" name="q" value="golf">"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn unknown_filter_values_are_ignored(pool: PgPool) {
    let uri = "/c/makina?vehicle_fuel=NUCLEAR&price_min=abc&sort=sideways&city=atlantis&type=JOB&x=1";
    let response = get(pool, uri).await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("filter-chip"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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
    assert!(page.contains(r#"<select id="filter-fits_year" name="fits_year">"#));
    assert!(page.contains(r#"<option value="">Çdo vit</option>"#) && page.contains(r#"<option value="1970">1970</option>"#));
    assert!(!page.contains("fits_from_year") && !page.contains("fits_to_year_min"));
    assert_eq!(found(&page), [true, true, true, true]);

    // By make: the Golfs.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN").await;
    assert_eq!(found(&page), [true, true, false, false]);
    // And year: the one whose years include it.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    assert!(page.contains(r#"<option value="2015" selected>2015</option>"#));
    assert!(page.contains(r#"<a class="filter-chip" href="/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN" aria-label="Hiq filtrin Viti i automjetit: 2015">"#));
    // A kind of part narrows the page, so its pill keeps the filters; the way up, which widens it,
    // keeps only the words, the town and the map pin.
    let kept = "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015";
    assert!(page.contains(&format!(r#"<a class="shortcut" href="{kept}"><span class="shortcut-shape" aria-hidden="true"><svg class="shortcut-icon""#)));
    assert!(page.contains(r#"<a class="breadcrumb-back title-back" href="/c/automjete" aria-label="Kthehu""#));
    let page = html(pool.clone(), "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    // A page that doesn't take a filter leaves it out of its links too: Automjete has no make.
    let page = html(pool.clone(), "/c/automjete?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert!(page.contains(r#"<a class="shortcut" href="/c/pjese-dhe-aksesore">"#));
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
