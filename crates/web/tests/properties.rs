//! What the listing form asks of each kind of thing (web/properties.rs): electronics, property,
//! clothes, pets, jobs and motorcycles; a car, its checks and the title made from what it is; a
//! part and the vehicle it fits; and the makes and models the lists lack, which sellers type in.

mod common;

use axum::http::StatusCode;
use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn electronics_subcategories_have_properties_of_their_own(pool: PgPool) {
    // A phone's form: phones' brands (Huawei, not Canon) in their own order, the models under
    // them, storage from 32 GB, RAM, battery, colour, features, a warranty and what comes with it;
    // no screen size.
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    for name in ["phone_model", "electronics_storage", "electronics_ram", "phone_battery", "electronics_color",
                 "electronics_features", "electronics_warranty", "electronics_included"] {
        assert!(phones.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(!phones.contains("prop_electronics_screen"));
    assert!(phones.contains(r#"<option value="HUAWEI">Huawei</option>"#));
    assert!(!phones.contains(r#"<option value="CANON">"#));
    let brands = positions(&phones, &[r#"<option value="APPLE">"#, r#"<option value="SAMSUNG">"#,
                                      r#"<option value="HUAWEI">"#, r#"<option value="OTHER">"#]);
    assert!(brands.is_sorted(), "{brands:?}");
    assert!(phones.contains(r#"<optgroup label="Apple" data-parent="APPLE">"#));
    assert!(phones.contains(r#"<option value="APPLE_IPHONE_13">iPhone 13</option>"#));
    assert!(phones.contains(r#"<option value="GB32">32 GB</option>"#) && !phones.contains(r#"value="TB2""#));
    // A number's unit inside its field at its end, and in its name for screen readers.
    assert!(phones.contains(r#"Bateria<span class="visually-hidden"> (%)</span></label>"#), "{phones}");
    assert!(phones.contains(r#"<span class="unit" aria-hidden="true">%</span>"#), "{phones}");

    // A TV's: its size as "Diagonalja", in inches, resolution, panel and Smart TV, TV brands, and
    // its remote among what comes with it; no storage, no pen.
    let tvs = get(pool.clone(), "/shto/fushat?category=televizore").await.body;
    assert!(tvs.contains(r#"Diagonalja<span class="visually-hidden"> (inç)</span></label>"#), "{tvs}");
    assert!(tvs.contains(r#"<span class="unit" aria-hidden="true">inç</span>"#), "{tvs}");
    for name in ["tv_resolution", "tv_panel", "tv_smart"] {
        assert!(tvs.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(!tvs.contains("prop_electronics_storage"));
    assert!(tvs.contains(r#"<option value="TCL">TCL</option>"#) && !tvs.contains(r#"<option value="APPLE">"#));
    assert!(tvs.contains(r#"value="REMOTE""#) && !tvs.contains(r#"value="PEN""#));

    // A laptop's: processor, RAM from 4 GB to 64 and more, disk, screen, graphics and system.
    let laptops = get(pool.clone(), "/shto/fushat?category=laptope").await.body;
    for name in ["computer_cpu", "electronics_ram", "computer_disk", "electronics_screen", "computer_gpu", "computer_os"] {
        assert!(laptops.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(laptops.contains(r#"value="GB64""#) && !laptops.contains(r#"value="GB3""#));

    // Posted with them, a phone says them on its page, and the filters find it by them.
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = listing_fields(("title", "iPhone 13, 128 GB"), &started);
    fields.extend_from_slice(&[
        ("prop_electronics_brand", "APPLE"),
        ("prop_phone_model", "APPLE_IPHONE_13"),
        ("prop_electronics_storage", "GB128"),
        ("prop_electronics_ram", "GB4"),
        ("prop_phone_battery", "89"),
        ("prop_electronics_color", "BLUE"),
        ("prop_electronics_features", "FIVE_G"),
        ("prop_electronics_features", "ESIM"),
        ("prop_electronics_warranty", "1"),
        ("prop_electronics_included", "BOX"),
    ]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let page = get(pool.clone(), &posted.location.unwrap()).await.body;
    // RAM's pill says "4 GB RAM", not "RAM 4 GB RAM".
    assert!(!page.contains(r#"<span class="attribute-label" aria-hidden="true">RAM</span>"#));
    for value in [">iPhone 13</dd>", "</svg>4 GB RAM</dd>", ">Blu</dd>", ">5G</dd>", ">eSIM</dd>",
                  ">Kutia origjinale</dd>", r#"title="Me garanci""#] {
        assert!(page.contains(value), "{value}");
    }
    assert!(page.contains("89\u{a0}%") || page.contains("89 %"), "the battery");
    for (uri, found) in [
        ("/c/telefona?electronics_ram=GB4", 1),
        ("/c/telefona?electronics_ram=GB8", 0),
        ("/c/telefona?electronics_brand=APPLE&phone_model=APPLE_IPHONE_13", 1),
        ("/c/telefona?phone_battery_min=85", 1),
        ("/c/telefona?phone_battery_min=90", 0),
        ("/c/telefona?electronics_features=ESIM", 1),
    ] {
        assert_eq!(card_numbers(&html(pool.clone(), uri).await).len(), found, "{uri}");
    }
    // The phones' filters offer phones' brands; a TV page filters by size, resolution and panel.
    let phone_page = html(pool.clone(), "/c/telefona").await;
    assert!(phone_page.contains(r#"value="HUAWEI""#) && !phone_page.contains(r#"value="CANON""#));
    let tv_page = html(pool.clone(), "/c/televizore").await;
    for name in ["electronics_screen_min", "tv_resolution", "tv_panel", "tv_smart"] {
        assert!(tv_page.contains(&format!(r#"name="{name}""#)), "{name}");
    }
    assert!(!tv_page.contains(r#"name="electronics_storage""#));

    // A brand phones don't offer is refused, as one that doesn't exist is.
    let mut canon = listing_fields(("title", "Telefon Canon"), &started);
    canon.push(("prop_electronics_brand", "CANON"));
    let refused = post_form(pool.clone(), "/shto", &canon, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh një nga mundësitë."));

    // A brand typed in for a phone, once approved, is offered for phones, before Tjetër, and not
    // for TVs.
    let mut typed = listing_fields(("title", "Fairphone 5, 256 GB"), &started);
    typed.extend_from_slice(&[("prop_electronics_brand", "__other__"), ("prop_electronics_brand_other", "Fairphone")]);
    let posted = post_form(pool.clone(), "/shto", &typed, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let moderator = new_seller(&pool, MODERATOR).await;
    let fairphone = option_id(&pool, "Fairphone").await;
    let approved = post_form(pool.clone(), &format!("/moderim/{fairphone}/mirato"), &[("label", "Fairphone")], Some(&moderator)).await;
    assert_eq!(approved.status, StatusCode::SEE_OTHER);
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    let order = positions(&phones, &[r#"<option value="NOKIA">"#, r#"<option value="FAIRPHONE">Fairphone</option>"#,
                                     r#"<option value="OTHER">"#]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!get(pool.clone(), "/shto/fushat?category=televizore").await.body.contains("Fairphone"));
}

/// The listing form's fields for a category: the frame it loads when the category changes.
async fn form_fields(pool: &PgPool, category: &str) -> String {
    get(pool.clone(), &format!("/shto/fushat?category={category}")).await.body
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn each_kind_of_thing_asks_for_what_fits_it(pool: PgPool) {
    let has = |fields: &str, name: &str| fields.contains(&format!(r#"name="prop_{name}""#));
    // A flat: its rooms and bathrooms, floor, condition, heating, and what a tenant asks. Central
    // heating is a kind of heating, not one of its amenities.
    let flat = form_fields(&pool, "apartamente").await;
    for name in ["property_rooms", "property_bathrooms", "property_floor", "property_condition", "property_heating", "rent_pets"] {
        assert!(has(&flat, name), "{name}");
    }
    assert!(flat.contains(r#"<option value="CENTRAL">Qendrore</option>"#));
    assert!(!flat.contains(r#"value="HEATING""#));
    // A plot: what the land is for, its permit and its road; no rooms, floor or furnishing.
    let plot = form_fields(&pool, "truall").await;
    for name in ["land_type", "land_permit", "land_road"] {
        assert!(has(&plot, name), "{name}");
    }
    for name in ["property_rooms", "property_floor", "property_furnished"] {
        assert!(!has(&plot, name), "{name}");
    }
    // Shoes: their size, EU 20 to 48, and shoes' brands; a jacket clothes' sizes and children's;
    // a bag no size.
    let shoes = form_fields(&pool, "kepuce").await;
    assert!(shoes.contains(r#"<option value="EU42">42</option>"#) && !has(&shoes, "fashion_size"));
    assert!(shoes.contains(r#"<option value="NIKE">Nike</option>"#) && !shoes.contains(r#"<option value="ROLEX">"#));
    let jackets = form_fields(&pool, "xhaketa-dhe-pallto").await;
    assert!(has(&jackets, "fashion_size") && has(&jackets, "kids_size"));
    let bags = form_fields(&pool, "canta").await;
    assert!(has(&bags, "material") && !has(&bags, "fashion_size") && !has(&bags, "shoe_size"));
    // A dog: its breed, of dogs' breeds, its age, sex and vaccines.
    let dogs = form_fields(&pool, "qen").await;
    for name in ["pet_breed", "pet_age", "pet_sex", "pet_vaccinated"] {
        assert!(has(&dogs, name), "{name}");
    }
    assert!(dogs.contains(r#"<option value="LABRADOR">Labrador</option>"#) && !dogs.contains(r#"value="PERSIAN""#));
    // A job: its contract, where it is done, the experience and languages it asks.
    let job = form_fields(&pool, "pune-it").await;
    for name in ["job_contract", "job_place", "job_experience", "job_languages"] {
        assert!(has(&job, name), "{name}");
    }
    // A motorcycle's form starts with its own make, with none of a car's tiles: its year after.
    let motorcycle = form_fields(&pool, "motocikleta").await;
    assert!(!motorcycle.contains("tile"));
    let order = positions(&motorcycle, &[r#"for="prop_moto_make""#, r#"for="prop_moto_type""#, r#"for="prop_model_name""#,
                                         r#"for="prop_vehicle_year""#]);
    assert!(order.is_sorted(), "{order:?}");
    // A car's: its main fields first, then its papers, engine and history, its options last; no
    // tiles with icons any more, each field its name over it as the rest of the form's.
    let car = form_fields(&pool, "makina").await;
    assert!(!car.contains("tile"), "{car}");
    let order = positions(&car, &[r#"for="prop_vehicle_color""#, r#"for="prop_vehicle_customs""#, r#"for="prop_vehicle_engine""#,
                                  r#"name="prop_vehicle_no_accident""#, r#"<span class="field-label">Opsionet</span>"#]);
    assert!(order.is_sorted(), "{order:?}");

    // Posted with them, a car says them on its page, and the filters find it by them.
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = car_fields(("", ""), &started);
    fields.extend_from_slice(&[
        ("prop_vehicle_customs", "CLEARED"),
        ("prop_vehicle_plates", "ALBANIAN"),
        ("prop_vehicle_engine", "1598"),
        ("prop_vehicle_power", "115"),
        ("prop_vehicle_no_accident", "1"),
        ("prop_vehicle_owners", "1"),
    ]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let page = get(pool.clone(), &posted.location.unwrap()).await.body;
    for value in [">E zhdoganuar</dd>", ">Shqiptare</dd>", "1\u{a0}598\u{a0}cm³</dd>", "115\u{a0}HP</dd>", "Pa aksident</dt>"] {
        assert!(page.contains(value), "{value}");
    }
    // On a car's page the details stand in a grid, each its name with its icon over its value; a
    // box ticked says its name, not "Po".
    let spec = |label: &str| {
        let end = page.find(&format!("{label}</dt>")).unwrap_or_else(|| panic!("{label}"));
        let start = page[..end].rfind("<dt>").unwrap();
        page[start..end + page[end..].find("</dd>").unwrap()].to_string()
    };
    for label in ["Dogana", "Targat", "Motorri", "Fuqia", "Pronarë", "Pa aksident"] {
        assert!(spec(label).starts_with(r#"<dt><svg class="spec-icon""#), "{label}: {}", spec(label));
    }
    assert!(spec("Dogana").ends_with("Dogana</dt><dd>E zhdoganuar"));
    assert!(spec("Pronarë").ends_with("Pronarë</dt><dd>1"));
    assert!(spec("Pa aksident").ends_with("Pa aksident</dt><dd>Pa aksident"));
    for (uri, found) in [
        ("/c/makina?vehicle_customs=CLEARED", 1),
        ("/c/makina?vehicle_customs=NOT_CLEARED", 0),
        ("/c/makina?vehicle_engine_min=1500&vehicle_engine_max=2000", 1),
        ("/c/makina?vehicle_power_min=150", 0),
        ("/c/makina?vehicle_no_accident=1", 1),
    ] {
        assert_eq!(card_numbers(&html(pool.clone(), uri).await).len(), found, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn parts_say_which_vehicle_they_fit(pool: PgPool) {
    // The fields, under the parts' own names for them, each its name over it; none required.
    let fields = get(pool.clone(), "/shto/fushat?category=gps-dhe-navigatore").await.body;
    let labels = positions(&fields, &[
        r#"<label class="field-label" for="prop_vehicle_make">Marka e automjetit</label>"#,
        r#"<option value="">Zgjidh</option>"#,
        r#"<label class="field-label" for="prop_vehicle_model">Modeli i automjetit</label>"#,
        r#"<label class="field-label" for="prop_fits_from_year">Nga viti</label>"#,
        r#"<input id="prop_fits_from_year" name="prop_fits_from_year" value="" inputmode="decimal">"#,
        r#"<label class="field-label" for="prop_fits_to_year">Deri në vitin</label>"#,
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
    assert!(car_fields.contains(r#"<label class="field-label" for="prop_vehicle_make">Marka <span class="required" aria-hidden="true">*</span></label>"#), "{car_fields}");
    assert!(car_fields.contains(r#"<select id="prop_vehicle_make" name="prop_vehicle_make" required>"#), "{car_fields}");
    assert!(car_fields.contains(r#"<option value="">Zgjidh</option>"#) && !car_fields.contains("Marka *"), "{car_fields}");
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

    // The listing says it, under the same names, under its category's breadcrumb: the one above
    // it and its own, as the category's page has them.
    let page = get(pool.clone(), &url).await.body;
    positions(&page, &[
        r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">"#,
        r#"<a class="breadcrumb-here" href="/c/gps-dhe-navigatore">"#,
    ]);
    for (label, value) in [("Marka e automjetit", "Volkswagen"), ("Modeli i automjetit", "Golf"), ("Nga viti", "2012"), ("Deri në vitin", "2019")] {
        assert!(page.contains(&format!(">{label}</dt>")), "{label}");
        assert!(page.contains(&format!(">{value}</dd>")), "{value}");
    }
    // The years have a calendar, and their names before them; so do the make and model, which
    // alone would read as the part's own.
    assert!(page.contains(r#"</svg><span class="attribute-label" aria-hidden="true">Nga viti</span>2012</dd>"#));
    assert!(page.contains(r#"</svg><span class="attribute-label" aria-hidden="true">Marka e automjetit</span>Volkswagen</dd>"#));
    // Its card gives the vehicle after the city; a search for the make finds it.
    let path = url.trim_start_matches("http://localhost");
    let parts = get(pool.clone(), "/c/pjese-dhe-aksesore").await.body;
    assert!(parts.contains(&format!(r#"href="{path}">Navigator origjinal për Golf</a>"#)));
    assert!(parts.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>Volkswagen · Golf</p>"));
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_car_is_posted_with_its_make_model_and_year(pool: PgPool) {
    // The fields come with the category: the frame the form reloads when it changes.
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(frame.starts_with(r#"<turbo-frame id="listing-properties">"#));
    assert!(frame.contains(r#"<input id="prop_vehicle_year" name="prop_vehicle_year" value="" inputmode="decimal" required>"#));
    // Its main fields two to a line, each its name over it, as the canvas has them: make, model,
    // year, fuel, gearbox, body, colour and mileage; then the rest, and its options last. (Tiles
    // with icons, named inside them, in a card of their own with its name as a header, once.)
    assert!(frame.contains(r#"<fieldset class="property-fields">"#));
    // The fields' name for screen readers, out of sight: the box they are in is the category's.
    assert!(frame.contains(r#"<legend class="visually-hidden">Detajet</legend>"#) && !frame.contains("property-heading"));
    let order = positions(&frame, &[
        r#"for="prop_vehicle_make""#,
        r#"for="prop_vehicle_model""#,
        r#"for="prop_vehicle_year""#,
        r#"for="prop_vehicle_fuel""#,
        r#"for="prop_vehicle_transmission""#,
        r#"for="prop_vehicle_body_type""#,
        r#"for="prop_vehicle_color""#,
        r#"for="prop_vehicle_mileage""#,
        r#"<span class="field-label">Opsionet</span>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!frame.contains("tile-icon"));
    // The mileage's unit inside its field, at its end; in its name for screen readers.
    assert!(frame.contains(r#"<label class="field-label" for="prop_vehicle_mileage">Kilometrazhi<span class="visually-hidden"> (km)</span></label>"#), "{frame}");
    assert!(frame.contains(r#"<input id="prop_vehicle_mileage" name="prop_vehicle_mileage" value="" inputmode="decimal">"#), "{frame}");
    assert!(frame.contains(r#"<span class="unit" aria-hidden="true">km</span>"#), "{frame}");
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
    let card = "<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2017 · 120\u{a0}000\u{a0}km · Naftë</p>";
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_title_is_made_from_what_it_is(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = car_fields(("prop_vehicle_body_type", "SEDAN"), &started);
    fields.extend([("prop_vehicle_color", "BLACK"), ("prop_vehicle_engine", "1598")]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();

    // The page's title is made from its make, model, engine (in litres, to a tenth) and year, and
    // kept with it.
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains("<h1>Volkswagen Golf 7 1.6 TDI, 2017</h1>") && page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf 1.6, 2017</span> · "#), "{page}");
    let generated: Option<String> = sqlx::query_scalar("SELECT generated_title FROM listings WHERE public_no = $1")
        .bind(no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(generated.as_deref(), Some("Volkswagen Golf 1.6, 2017"));
    // The seller's own heads the page, not the description; the details leave out what the made
    // title says, but for the engine's exact size; the year stands among the key facts.
    assert!(!page.contains("description-heading"), "the seller's own heads the page, not the description");
    for said in ["Marka", "Modeli"] {
        assert!(!page.contains(&format!(">{said}</dt>")), "{said}");
    }
    assert!(page.contains("<dt>Viti</dt><dd>2017</dd>") && page.matches("Viti</dt>").count() == 1);
    for shown in ["Karroceria", "Ngjyra", "Motorri"] {
        assert!(page.contains(&format!("{shown}</dt>")), "{shown}");
    }
    // Its card says it too.
    let card = format!(r#"href="{url}">Volkswagen Golf 1.6, 2017</a>"#);
    assert!(get(pool.clone(), "/c/makina").await.body.contains(&card));

    // An edit makes it again: without the engine, the make, the model and the year.
    let fields = car_fields(("prop_vehicle_year", "2018"), &started);
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &fields, Some(&ana)).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    let edited = get(pool.clone(), &url).await.body;
    assert!(edited.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf, 2018</span> · "#), "{edited}");
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
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
    // They make its title, as a listed make and model would: the two as the heading, the year
    // under it.
    assert!(page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Zastava Yugo, 2017</span> · "#), "{page}");
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
    assert!(get(pool.clone(), "/c/makina?vehicle_make=BMW").await.body.contains(">BMW Seria 1, 2017</a>"));
    // A listed model typed without its punctuation ("id buzz") is the listed one too, ID. Buzz: it
    // comes to the same code.
    let mut buzz = typed_car("volkswagen", "id buzz", &started);
    buzz.iter_mut().find(|field| field.0 == "title").unwrap().1 = "Volkswagen ID. Buzz, 2023";
    let posted = post_form(pool.clone(), "/shto", &buzz, Some(&besa)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let buzz = get(pool.clone(), &posted.location.unwrap()).await.body;
    assert!(buzz.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen ID. Buzz, 2017</span> · "#), "{buzz}");
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

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_makes_and_models_have_the_codes_typing_them_gives(pool: PgPool) {
    // As the posting form codes a typed option (properties.rs), a model's after its make's.
    let code = |name: &str| pluribus_core::format::slug(name).to_uppercase().replace('-', "_");
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
