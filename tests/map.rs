//! The map (web/browse.rs's map page, and the map card on a category's page): a pin and its radius,
//! the nearest listings first and the pages after them; the town picked, kept for the next
//! category; a dot for every result; and what a page that finds nothing around its pin offers.

mod common;

use axum::{body::Body, http::{Request, StatusCode}};
use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_map_pin_finds_the_listings_within_its_radius_nearest_first(pool: PgPool) {
    // Tirana's centre, where the fixture puts the Tirana listings. Durrës is 31 km away.
    let body = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=10").await;
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>0\u{a0}m · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    assert!(body.contains(r#"<a class="filter-chip" href="/search" aria-label="Hiq filtrin Brenda 10 km nga pika">"#));
    assert!(body.contains(r#"<input class="visually-hidden" type="radio" name="sort" value="" form="filter-form" data-action="filters#submit" checked>Më afër</label>"#));

    // From Durrës, 50 km reaches Tirana: Durrës's listings first, then Tirana's.
    let body = get(pool, "/search?lat=41.31333&lng=19.44583&radius=50").await.body;
    let order = positions(&body, &[
        "Mercedes-Benz C 220, 2018, automatike",
        "Volkswagen Golf 7, 2016, naftë",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(body.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>31\u{a0}km · 2016"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_map_page_sets_its_map_from_a_box_beside_it(pool: PgPool) {
    // As the canvas lays it out: the map, then the box of what sets it, one under another: how far,
    // the category, a town to move the map onto, "Vendndodhja ime", then "Vendos hartën" and the
    // button that folds the map; the results under both, after how many there are.
    let page = html(pool.clone(), "/harta").await;
    let order = positions(&page, &[
        r#"<div class="map-layout">"#,
        r#"<div class="map-canvas" id="map-canvas""#,
        r#"<form id="map-form" class="map-form map-controls""#,
        r#"<div class="field map-radius">"#,
        r#"<label class="field-label" for="map-category">Kategoria</label>"#,
        r#"<label class="field-label" for="map-town">Qyteti</label>"#,
        r#"<button type="button" class="button secondary map-locate" data-action="map#locate">"#,
        r#"<div class="map-actions">"#,
        r#"<button type="button" class="button map-confirm" data-action="map#confirm">Vendos hartën</button>"#,
        r#"<turbo-frame id="map-results""#,
        r#"<div class="map-count">"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    // The town moves the map, and says so until one is picked.
    assert!(page.contains(r#"<select id="map-town" aria-label="Qyteti" data-map-target="town" data-action="map#goToTown">"#));
    assert!(page.contains(r#"<option value="">Zgjidh qytetin</option>"#));
    // "Vendndodhja ime" has the crosshair (Phosphor's, its ring the tone), here and in a category's
    // map card.
    let crosshair = r#"<path d="M160,128a32,32,0,1,1-32-32A32,32,0,0,1,160,128Z"/>"#;
    assert!(page[page.find("map-locate").unwrap()..].contains(crosshair));
    let cars = html(pool, "/c/makina").await;
    assert!(cars[cars.find(r#"class="map-pill map-locate""#).unwrap()..].contains(crosshair));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn any_whole_distance_up_to_50_km_can_be_picked(pool: PgPool) {
    let seven = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=7").await;
    assert!(seven.contains("Brenda 7 km nga pika"));
    // Out of range or not a whole number: the usual 10 km.
    for radius in ["0", "51", "2.5"] {
        let page = html(pool.clone(), &format!("/search?lat=41.32833&lng=19.81806&radius={radius}")).await;
        assert!(page.contains("Brenda 10 km nga pika"), "{radius}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn category_pages_keep_their_map_folded(pool: PgPool) {
    // Every category page has its map, in a card folded to its header; with no point, the
    // results are from everywhere. Nothing else opens it.
    let cars = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await;
    assert!(cars.contains(r#"<section class="map-card map-hidden" data-controller="map" data-map-lazy-value="true""#));
    assert!(cars.contains(r#"aria-controls="map-card-body" aria-expanded="false""#));
    assert!(cars.contains(r#"aria-label="Harta: Kudo" title="Harta: Kudo""#));
    // With no point, it starts on Tirana, its circle round the pin all the same (map_controller.js).
    assert!(!cars.contains("data-map-latitude-value") && !cars.contains("data-map-ring-value"));
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
    // Beside "Vendndodhja ime", a pill, the towns' menu, by county, all of Albania first, each the
    // page in it with its point, which the map moves onto; it has no name, so the map's form sends
    // no town of its own.
    assert!(cars.contains(r#"<button type="button" class="map-pill map-locate" data-action="map#locate">"#));
    assert!(cars.contains(r#"<select aria-label="Qyteti" data-map-target="town" data-action="map#pickTown">"#));
    assert!(cars.contains(r#"<option value="tirane" data-lat="41.328333" data-lng="19.818056" data-url="/c/makina?city=tirane&vehicle_fuel=DIESEL">Tiranë</option>"#));

    // With a point, the page stays the category's, with its heading and filters, the card still
    // folded, saying how far around the point; the filter form and the category menu keep it.
    let pinned = html(pool.clone(), "/c/makina?lat=41.32833&lng=19.81806&radius=5&sort=price_asc").await;
    assert!(pinned.contains(r#"<h1 class="browse-title">Makina</h1>"#));
    assert!(pinned.contains(r#"<section class="map-card map-hidden""#));
    assert!(pinned.contains(r#"data-map-latitude-value="41.32833""#));
    // The map's button says where, the point's nearest town and how far (in its tooltip and to
    // screen readers; set, it shows its pin alone, the point's chip saying it), and is marked as
    // set; the filters' summary leaves the point to it, and to its chip. Its circle shows from the
    // start.
    assert!(pinned.contains(r#"aria-label="Harta: Tiranë · 5 km" title="Harta: Tiranë · 5 km""#));
    assert!(pinned.contains(r#"<span class="button-label">Tiranë · 5 km</span>"#));
    assert!(pinned.contains(r#"class="button secondary round-button labelled map-button is-set""#));
    assert!(pinned.contains(r#"aria-label="Filtrat: Asnjë filtër""#));
    // The distance is what the circle covers at the map's zoom (map_controller.js), said beside its
    // name and carried by the radius field: no slider.
    assert!(pinned.contains(r#"<p class="map-distance"><span class="visually-hidden">Largësia </span><output data-map-target="radiusLabel">5 km</output></p>"#));
    assert!(!pinned.contains("Zmadho hartën") && !pinned.contains("map-radius"));
    assert!(pinned.contains(r#"<input type="hidden" name="radius" value="5" data-map-target="radius">"#));
    assert!(!pinned.contains(r#"id="map-radius""#) && !pinned.contains("map-radius-steps"));
    // Any whole distance from a link, the zoom showing it as it is.
    let seven = html(pool.clone(), "/c/makina?lat=41.32833&lng=19.81806&radius=7").await;
    assert!(seven.contains(r#"<output data-map-target="radiusLabel">7 km</output>"#));
    for (name, value) in [("lat", "41.32833"), ("lng", "19.81806"), ("radius", "5")] {
        assert!(pinned.contains(&format!(r#"<input type="hidden" name="{name}" value="{value}">"#)), "{name}");
    }
    // A new pin or radius keeps the order picked.
    let map_form = &pinned[pinned.find(r#"<form class="map-form""#).unwrap()..];
    let map_form = &map_form[..map_form.find("</form>").unwrap()];
    assert!(map_form.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(pinned.contains(r#"<a class="breadcrumb-back title-back" href="/c/automjete?lat=41.32833&lng=19.81806&radius=5" aria-label="Kthehu""#), "{pinned}");
    // The point's chip searches everywhere again, keeping the rest.
    assert!(pinned.contains(r#"<a class="filter-chip" href="/c/makina?sort=price_asc" aria-label="Hiq filtrin Brenda 5 km nga pika">"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_maps_card_picks_a_town_or_all_of_albania(pool: PgPool) {
    // In the map's card, beside "Vendndodhja ime", the towns' menu: all of Albania first (chosen,
    // there being no place), then the towns by county, each the same page in it, its filters kept.
    // No town pills under the row any more: there, the order and the cards' size alone.
    let cars = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await;
    let menu = &cars[cars.find(r#"<div class="picker map-town" data-controller="picker">"#).expect("the towns' menu")..];
    let menu = &menu[..menu.find("</select>").unwrap()];
    let order = positions(menu, &[
        r#"<option value="" data-url="/c/makina?vehicle_fuel=DIESEL" selected>Gjithë Shqipëria</option>"#,
        r#"<optgroup label="#,
        r#"data-url="/c/makina?city=tirane&vehicle_fuel=DIESEL">Tiranë</option>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(menu.contains(r#"data-url="/c/makina?city=durres&vehicle_fuel=DIESEL">Durrës</option>"#));
    assert!(!cars.contains("place-pills") && !cars.contains("place-near"));
    let pills = &cars[cars.find(r#"<div class="results-head">"#).unwrap()..cars.find(r#"<div class="grid">"#).unwrap()];
    // No pill under the row: the heading says the order, and the row's last button the view.
    assert_eq!(pills.matches(r#"class="small-pill"#).count(), 0);
    // The page's town is kept in a cookie for the next pages (place_controller.js), on the page's
    // own element.
    assert!(cars.contains(r#"data-controller="filters cards place""#) && cars.contains(r#"data-place-town-value="""#));

    // A town: chosen in the menu, which says it; kept in the cookie; its chip forgets it. The
    // map's button says it, and the map starts there.
    let durres = html(pool.clone(), "/c/makina?city=durres").await;
    assert!(durres.contains(r#"data-place-town-value="durres""#));
    assert!(durres.contains(r#"data-url="/c/makina?city=durres" selected>Durrës</option>"#));
    assert!(durres.contains(r#"<option value="" data-url="/c/makina">Gjithë Shqipëria</option>"#));
    assert!(durres.contains(r#"<a class="filter-chip" href="/c/makina" aria-label="Hiq filtrin Durrës" data-forget-town data-turbo-prefetch="false">"#));
    assert!(durres.contains(r#"aria-label="Harta: Durrës" title="Harta: Durrës""#));
    assert!(durres.contains(r#"data-map-latitude-value="41.313333" data-map-longitude-value="19.445833""#));

    // The map's point, in place of a town: the menu says it, after all of Albania, which drops it,
    // as its chip does.
    let pinned = html(pool, "/c/makina?lat=41.32833&lng=19.81806&radius=5").await;
    let order = positions(&pinned, &[
        r#"<option value="" data-url="/c/makina">Gjithë Shqipëria</option>"#,
        r#"<option value="pika" selected>Tiranë · 5 km</option>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(pinned.contains(r#"<p class="field-hint">Tani nga harta: Tiranë · 5 km.</p>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_town_picked_last_opens_the_next_category_in_it(pool: PgPool) {
    // The cookie the page keeps the town in (qyteti): a category whose address names no place
    // opens in it, as if picked there.
    let durres = get_with_cookie(pool.clone(), "/c/automjete", "qyteti=durres").await.body;
    assert!(durres.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!durres.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(durres.contains(r#"data-url="/c/automjete?city=durres" selected>Durrës</option>"#));
    // A place in the address wins: a town, or the map's point.
    let tirana = get_with_cookie(pool.clone(), "/c/automjete?city=tirane", "qyteti=durres").await.body;
    assert!(tirana.contains("Volkswagen Golf 7, 2016, naftë") && !tirana.contains("Mercedes-Benz C 220"));
    let point = get_with_cookie(pool.clone(), "/c/automjete?lat=41.32833&lng=19.81806&radius=10", "qyteti=durres").await.body;
    assert!(point.contains("Volkswagen Golf 7, 2016, naftë") && !point.contains("Mercedes-Benz C 220"));
    // The map page has its point; a town that isn't one counts for nothing.
    let map = get_with_cookie(pool.clone(), "/harta", "qyteti=durres").await.body;
    assert!(map.contains("Volkswagen Golf 7, 2016, naftë"));
    let nowhere = get_with_cookie(pool, "/c/automjete", "qyteti=atlantis").await.body;
    assert!(nowhere.contains("Mercedes-Benz C 220") && nowhere.contains("Volkswagen Golf 7"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn nothing_found_offers_what_would_find_more(pool: PgPool) {
    // Around a point in Shkodër there are no cars: a wider circle, a tap away, then a wider one,
    // and all of Albania; the filters' chips each drop one.
    let shkoder = "/c/makina?vehicle_fuel=DIESEL&lat=42.06813&lng=19.51214&radius=10";
    let page = html(pool.clone(), shkoder).await;
    assert!(page.contains("Nuk u gjet asnjë njoftim."));
    assert!(page.contains(r#"<p class="empty">Provo të heqësh ndonjë filtër nga lart.</p>"#));
    assert!(page.contains(r#"<a class="button secondary" href="/c/makina?lat=42.06813&lng=19.51214&radius=25&vehicle_fuel=DIESEL">Zgjero largësinë në 25 km</a>"#));
    assert!(page.contains(r#"<a class="button secondary" href="/c/makina?vehicle_fuel=DIESEL" data-forget-town data-turbo-prefetch="false"><svg"#));
    assert!(page.contains("</svg>Kërko në gjithë Shqipërinë</a>"));
    let wider = html(pool.clone(), "/c/makina?lat=42.06813&lng=19.51214&radius=25").await;
    assert!(wider.contains("Zgjero largësinë në 50 km</a>") && !wider.contains("Provo të heqësh"));
    let widest = html(pool.clone(), "/c/makina?lat=42.06813&lng=19.51214&radius=50").await;
    assert!(!widest.contains("Zgjero largësinë") && widest.contains("Kërko në gjithë Shqipërinë</a>"));
    // In a town with none: all of Albania. Everywhere already: only the words.
    let town = html(pool.clone(), "/c/makina?city=shkoder").await;
    assert!(!town.contains("Zgjero largësinë") && town.contains("Kërko në gjithë Shqipërinë</a>"));
    let everywhere = html(pool, "/c/makina?vehicle_fuel=ELECTRIC").await;
    assert!(everywhere.contains("Provo të heqësh") && !everywhere.contains("Kërko në gjithë Shqipërinë"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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
    let page = html(pool.clone(), "/harta?category=te-tjera&lat=41.32833&lng=19.81806&radius=10").await;
    assert_eq!(dots(&page), 500);
    assert!(page.contains(r#"<p class="map-note">Në hartë shfaqen 500 njoftimet e para të listës.</p>"#));
    // A category's card says nothing of it: its map is all map (browse.html).
    let card = html(pool, "/c/te-tjera?lat=41.32833&lng=19.81806&radius=10&harta=1").await;
    assert_eq!(dots(&card), 500);
    assert!(!card.contains("map-note"));
}
