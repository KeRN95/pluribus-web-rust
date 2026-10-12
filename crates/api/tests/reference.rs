//! The reference data the app loads first (crates/api/src/reference.rs): the categories as their
//! tree, the towns with their zones, and a category's properties as its filters offer them or its
//! posting form asks for them, each as the pages load it, its names in both languages at once and
//! the English an English page says; the answers kept five minutes; and a category the site hasn't,
//! or a `for` that is neither, answered in the envelope.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use pluribus_core::i18n::{LANG, Lang, t};
use pluribus_db::{Categories, Category};
use serde_json::{Value, json};
use sqlx::PgPool;

use common::*;

const KEPT: &str = "public, max-age=300";

/// Checks `nodes`, the categories under `parent` as the API gives them, against the pages' own
/// (`categories`, db::categories): the same, in the same order, each with what the pages say of
/// it, its name in English as an English page says it; and those under each, likewise. How many
/// it checked.
fn check_tree(nodes: &Value, parent: Option<&Category>, categories: &Categories) -> usize {
    let under = parent.map_or("the top", |parent| parent.slug.as_str());
    let expected: Vec<&Category> = match parent {
        Some(parent) => categories.children(parent.id).collect(),
        None => categories.top_level().collect(),
    };
    let nodes = nodes.as_array().unwrap_or_else(|| panic!("under {under}: not a list"));
    let slugs: Vec<&str> = nodes.iter().map(|node| node["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, expected.iter().map(|category| category.slug.as_str()).collect::<Vec<_>>(), "under {under}");
    let mut checked = 0;
    for (node, category) in nodes.iter().zip(expected) {
        let english = LANG.sync_scope(Lang::En, || t(&category.name).to_string());
        let mut own = node.clone();
        own.as_object_mut().unwrap().remove("children");
        assert_eq!(
            own,
            json!({
                "slug": category.slug,
                "name": category.name,
                "name_en": english,
                "parent_slug": parent.map(|parent| parent.slug.as_str()),
                "allow_listings": !categories.has_children(category.id),
                "default_currency": category.default_currency,
                "listing_types": category.listing_types,
                "uses_condition": category.uses_condition,
            }),
            "under {under}"
        );
        checked += 1 + check_tree(&node["children"], Some(category), categories);
    }
    checked
}

/// The category of this slug, wherever it is in the tree.
fn find<'a>(nodes: &'a Value, slug: &str) -> Option<&'a Value> {
    nodes.as_array()?.iter().find_map(|node| match node["slug"] == slug {
        true => Some(node),
        false => find(&node["children"], slug),
    })
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_categories_come_as_the_pages_tree_named_in_both_languages(pool: PgPool) {
    let answer = get(pool.clone(), "/api/v1/categories", None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header("content-type"), Some("application/json"));
    assert_eq!(answer.header("cache-control"), Some(KEPT));
    let tree = answer.json();
    // Every category the pages list, once, under its own, in their order.
    let categories = pluribus_db::categories(&pool).await.unwrap();
    assert_eq!(check_tree(&tree, None, &categories), categories.iter().count());
    assert!(tree.as_array().unwrap().len() >= 10, "the main categories");

    let vehicles = find(&tree, "automjete").unwrap();
    assert_eq!((&vehicles["name"], &vehicles["name_en"]), (&json!("Automjete"), &json!("Vehicles")));
    // Listings go into its subcategories, a car's into Makina.
    assert_eq!((&vehicles["allow_listings"], &vehicles["default_currency"]), (&json!(false), &json!("EUR")));
    // No icon: the pages draw a category's by its slug (web/icons.rs).
    assert_eq!(vehicles.get("icon"), None);
    let cars = find(&vehicles["children"], "makina").unwrap();
    assert_eq!(
        json!([cars["name"], cars["name_en"], cars["parent_slug"], cars["allow_listings"], cars["listing_types"], cars["uses_condition"]]),
        json!(["Makina", "Cars", "automjete", true, ["SELL", "BUY", "RENT"], true])
    );
    assert_eq!(cars["children"], json!([]));
    // The one-word names (migration 0042), and an English page's English for them: Home, not the
    // "Home & Garden" that name_i18n keeps for search.
    let home = find(&tree, "shtepi-dhe-kopsht").unwrap();
    assert_eq!((&home["name"], &home["name_en"]), (&json!("Shtëpi"), &json!("Home")));
    let property = find(&tree, "prona").unwrap();
    assert_eq!((&property["name"], &property["name_en"]), (&json!("Prona"), &json!("Property")));

    // Both languages at once, whichever the request asks for.
    let request = request("GET", "/api/v1/categories", None).header(header::ACCEPT_LANGUAGE, "en");
    let english = send(api(pool), request.body(Body::empty()).unwrap()).await;
    assert_eq!(english.json(), tree);
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_towns_come_as_the_filters_list_them_with_their_zones(pool: PgPool) {
    let answer = get(pool.clone(), "/api/v1/cities", None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header("cache-control"), Some(KEPT));
    let towns = answer.json();
    let towns = towns.as_array().unwrap();
    // The pages' towns, in their order: by county, each county's main town first.
    let cities = pluribus_db::cities(&pool).await.unwrap();
    let slugs: Vec<&str> = towns.iter().map(|town| town["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, cities.iter().map(|city| city.slug.as_str()).collect::<Vec<_>>());

    let tirana = towns.iter().find(|town| town["slug"] == "tirane").unwrap();
    let city = cities.iter().find(|city| city.slug == "tirane").unwrap();
    let mut own = tirana.clone();
    own.as_object_mut().unwrap().remove("zones");
    assert_eq!(
        own,
        json!({"id": city.id, "name": "Tiranë", "slug": "tirane", "county": "Tiranë", "latitude": city.latitude, "longitude": city.longitude})
    );
    assert!((city.latitude - 41.33).abs() < 0.01 && (city.longitude - 19.82).abs() < 0.01);
    // Its zones, as the town has them, Blloku among them.
    let zones = tirana["zones"].as_array().unwrap();
    assert!(zones.len() > 30, "{}", zones.len());
    assert_eq!(*zones, city.zones.iter().map(|zone| json!({"id": zone.id, "name": zone.name, "slug": zone.slug})).collect::<Vec<_>>());
    let blloku = zones.iter().find(|zone| zone["slug"] == "blloku").unwrap();
    assert_eq!(blloku["name"], "Blloku");
    // Most towns have none.
    let without = cities.iter().find(|city| city.zones.is_empty()).unwrap();
    let town = towns.iter().find(|town| town["slug"] == without.slug.as_str()).unwrap();
    assert_eq!(town["zones"], json!([]));
}

/// The fields' codes, in their order.
fn codes(fields: &Value) -> Vec<&str> {
    fields.as_array().unwrap().iter().map(|field| field["code"].as_str().unwrap()).collect()
}

/// The field of this code.
fn field<'a>(fields: &'a Value, code: &str) -> &'a Value {
    fields.as_array().unwrap().iter().find(|field| field["code"] == code).unwrap_or_else(|| panic!("no {code}"))
}

/// A field's own keys but its options and its place.
fn shape(field: &Value) -> Value {
    let mut shape = field.clone();
    let object = shape.as_object_mut().unwrap();
    object.remove("options");
    object.remove("order");
    shape
}

/// Checks that each field is in its place (`order`), and that each name, a field's and an
/// option's, comes with the English an English page says for it.
fn check_order_and_english(fields: &Value) {
    LANG.sync_scope(Lang::En, || {
        for (order, field) in fields.as_array().unwrap().iter().enumerate() {
            assert_eq!(field["order"], order);
            assert_eq!(field["label_en"], t(field["label"].as_str().unwrap()), "{}", field["code"]);
            if let Some(unit) = field["unit"].as_str() {
                assert_eq!(field["unit_en"], t(unit), "{}", field["code"]);
            }
            for option in field["options"].as_array().unwrap() {
                assert_eq!(option["label_en"], t(option["label"].as_str().unwrap()), "{}: {}", field["code"], option["code"]);
            }
        }
    });
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_categorys_form_fields_are_what_its_posting_form_asks_for(pool: PgPool) {
    let answer = get(pool.clone(), "/api/v1/categories/makina/fields?for=form", None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header("cache-control"), Some(KEPT));
    let fields = answer.json();
    // The posting form's properties, in its order: its own and those it takes from above.
    let categories = pluribus_db::categories(&pool).await.unwrap();
    let form = pluribus_db::form_attributes(&pool, categories.by_slug("makina").unwrap().id).await.unwrap();
    assert_eq!(codes(&fields), form.iter().map(|attribute| attribute.code.as_str()).collect::<Vec<_>>());
    check_order_and_english(&fields);

    // The make, which every car needs, open to one the list lacks.
    let make = field(&fields, "vehicle_make");
    assert_eq!(
        shape(make),
        json!({"code": "vehicle_make", "label": "Marka", "label_en": "Brand", "data_type": "SELECT", "unit": null, "unit_en": null,
               "required": true, "allow_other": true, "parent_code": null, "validation": {}})
    );
    let makes = make["options"].as_array().unwrap();
    assert!(makes.contains(&json!({"code": "VOLKSWAGEN", "label": "Volkswagen", "label_en": "Volkswagen", "parent_option": null})));
    // The model, which depends on it: each of its options under one of the makes.
    let model = field(&fields, "vehicle_model");
    assert_eq!(
        shape(model),
        json!({"code": "vehicle_model", "label": "Modeli", "label_en": "Model", "data_type": "SELECT", "unit": null, "unit_en": null,
               "required": true, "allow_other": true, "parent_code": "vehicle_make", "validation": {}})
    );
    let models = model["options"].as_array().unwrap();
    assert!(models.contains(&json!({"code": "VOLKSWAGEN_GOLF", "label": "Golf", "label_en": "Golf", "parent_option": "VOLKSWAGEN"})));
    let make_codes: Vec<&Value> = makes.iter().map(|make| &make["code"]).collect();
    assert!(models.iter().all(|model| make_codes.contains(&&model["parent_option"])), "every model under a make offered");
    let offered = form.iter().find(|attribute| attribute.code == "vehicle_model").unwrap().options.len();
    assert_eq!(models.len(), offered, "every model offered");
    // A number's bounds as the property keeps them, and its unit.
    assert_eq!(
        shape(field(&fields, "vehicle_year")),
        json!({"code": "vehicle_year", "label": "Viti", "label_en": "Year", "data_type": "NUMBER", "unit": null, "unit_en": null,
               "required": true, "allow_other": false, "parent_code": null, "validation": {"min": 1900}})
    );
    assert_eq!(
        shape(field(&fields, "vehicle_mileage")),
        json!({"code": "vehicle_mileage", "label": "Kilometrazhi", "label_en": "Mileage", "data_type": "NUMBER", "unit": "km", "unit_en": "km",
               "required": false, "allow_other": false, "parent_code": null, "validation": {"min": 0}})
    );
    assert_eq!(field(&fields, "vehicle_engine")["validation"], json!({"min": 25, "max": 10000}));
    // A choice, named in both languages; a box to tick; several choices.
    let diesel = json!({"code": "DIESEL", "label": "Naftë", "label_en": "Diesel", "parent_option": null});
    assert!(field(&fields, "vehicle_fuel")["options"].as_array().unwrap().contains(&diesel));
    assert_eq!(field(&fields, "vehicle_no_accident")["data_type"], "BOOLEAN");
    assert_eq!(field(&fields, "vehicle_features")["data_type"], "MULTISELECT");
    // What the form asks for that no filter offers.
    assert!(codes(&fields).contains(&"vehicle_doors"));

    // A category that only groups others takes no listings: its form asks for nothing.
    let vehicles = get(pool, "/api/v1/categories/automjete/fields?for=form", None).await;
    assert_eq!((vehicles.status, vehicles.json()), (StatusCode::OK, json!([])));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_categorys_filter_fields_are_what_its_filters_offer(pool: PgPool) {
    let answer = get(pool.clone(), "/api/v1/categories/makina/fields?for=filters", None).await;
    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(answer.header("cache-control"), Some(KEPT));
    let fields = answer.json();
    // The filters' properties, in their order: what listings can be found by.
    let filters = pluribus_db::filter_attributes(&pool, "makina").await.unwrap();
    assert_eq!(codes(&fields), filters.iter().map(|attribute| attribute.code.as_str()).collect::<Vec<_>>());
    check_order_and_english(&fields);
    assert!(codes(&fields).contains(&"vehicle_mileage") && !codes(&fields).contains(&"vehicle_doors"));
    // The model depends on the make here too; nothing is typed in a filter, nor bounded.
    let model = field(&fields, "vehicle_model");
    assert_eq!(
        shape(model),
        json!({"code": "vehicle_model", "label": "Modeli", "label_en": "Model", "data_type": "SELECT", "unit": null, "unit_en": null,
               "required": true, "allow_other": false, "parent_code": "vehicle_make", "validation": null})
    );
    let golf = json!({"code": "VOLKSWAGEN_GOLF", "label": "Golf", "label_en": "Golf", "parent_option": "VOLKSWAGEN"});
    assert!(model["options"].as_array().unwrap().contains(&golf));
    for each in fields.as_array().unwrap() {
        assert_eq!((&each["allow_other"], &each["validation"]), (&json!(false), &Value::Null), "{}", each["code"]);
    }
    // A main category's filters, as its page has them.
    let vehicles = get(pool.clone(), "/api/v1/categories/automjete/fields?for=filters", None).await;
    let filters = pluribus_db::filter_attributes(&pool, "automjete").await.unwrap();
    assert_eq!(codes(&vehicles.json()), filters.iter().map(|attribute| attribute.code.as_str()).collect::<Vec<_>>());
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_category_the_site_hasnt_or_a_for_that_is_neither_answers_the_envelope(pool: PgPool) {
    let not_found = json!({"error": {"code": "not_found", "message": "Ajo që kërkove nuk u gjet."}});
    for path in ["/api/v1/categories/nothing/fields?for=form", "/api/v1/categories/nothing/fields?for=filters"] {
        let missing = get(pool.clone(), path, None).await;
        assert_eq!((missing.status, missing.json()), (StatusCode::NOT_FOUND, not_found.clone()), "{path}");
        assert_eq!(missing.header("cache-control"), None, "{path}: a failure isn't kept");
    }
    // A category taken off the site, gone from the tree too.
    sqlx::query("UPDATE categories SET active = false WHERE slug = 'rimorkio'").execute(&pool).await.unwrap();
    let gone = get(pool.clone(), "/api/v1/categories/rimorkio/fields?for=filters", None).await;
    assert_eq!((gone.status, gone.json()), (StatusCode::NOT_FOUND, not_found));
    assert_eq!(find(&get(pool.clone(), "/api/v1/categories", None).await.json(), "rimorkio"), None);

    let why = "Duhet të jetë „filters“ ose „form“.";
    let refused = json!({"error": {"code": "validation", "message": "Disa fusha nuk janë në rregull.", "fields": {"for": why}}});
    for query in ["?for=everything", "?for=FORM", "?for=", "?lang=sq", ""] {
        let path = format!("/api/v1/categories/makina/fields{query}");
        let wrong = get(pool.clone(), &path, None).await;
        assert_eq!((wrong.status, wrong.json()), (StatusCode::UNPROCESSABLE_ENTITY, refused.clone()), "{path}");
        assert_eq!(wrong.header("cache-control"), None, "{path}");
    }
    // In the language asked for.
    let english = get(pool, "/api/v1/categories/makina/fields?for=all&lang=en", None).await;
    assert_eq!(
        english.json(),
        json!({"error": {"code": "validation", "message": "Some fields aren’t right.", "fields": {"for": "Must be “filters” or “form”."}}})
    );
}
