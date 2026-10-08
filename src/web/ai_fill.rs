//! "Plotëso pjesën tjetër me AI" on the listing form: from the title, the description and the
//! photos a seller has put in, the AI (ai.rs) fills in the rest, the category and its properties,
//! the kind of listing, the condition, the price and the town, for the seller to check before
//! publishing (ai_fill_controller.js). Its instructions give it the form's own lists to choose
//! from, and its answer is checked against them as the form checks what a seller sends: anything
//! that doesn't fit is left out. Each fill is a call to the AI, which costs, hence the limits.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use rust_decimal::{Decimal, prelude::FromPrimitive};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{
    Result,
    account::{ClientIp, current_user},
    filters::parse_number,
    photos::Submission,
    post::{CONDITIONS, LISTING_TYPES, PRICE_TYPES, zoned},
    properties::{self, OTHER, field_name},
};
use crate::{
    AppState,
    ai::Tool,
    db::{self, CatalogueProperty, Categories, Category, City, FormAttribute},
    media::{self, MAX_UPLOAD_BYTES},
};

/// The photos the AI looks at: the first few, the cover first...
const PHOTOS: usize = 4;
/// ...each at most this many pixels on its longer side: enough to tell a car's make or the state
/// of a flat, and about a thousand of the AI's tokens a photo.
const PHOTO_SIDE: u32 = 1024;
/// Fills a seller (their account, or their network when they aren't signed in) may ask for in an
/// hour.
const FILLS_PER_HOUR: usize = 10;
/// And the whole site in a day: a ceiling on what the AI can cost, whatever happens.
const FILLS_PER_DAY: usize = 2_000;
/// The longest title and description the form takes (post.rs), and so all the AI reads.
const TITLE_CHARS: usize = 120;
const DESCRIPTION_CHARS: usize = 5000;

const WRITE_FIRST: &str = "Shkruaj së pari titullin dhe përshkrimin: AI plotëson pjesën tjetër prej tyre.";
const TOO_MANY: &str = "Ke kërkuar plotësim me AI shumë herë brenda një ore. Plotëso vetë fushat, ose provo më vonë.";
const BUSY: &str = "AI ka pasur shumë kërkesa sot. Plotëso vetë fushat, ose provo nesër.";
const UNAVAILABLE: &str = "AI nuk u përgjigj tani. Plotëso vetë fushat e tjera, ose provo pak më vonë.";

/// The tool the AI answers with.
const TOOL: &str = "fill_listing_form";

/// How the AI is to fill the form in. The lists it chooses from follow (instructions).
const RULES: &str = "\
You fill in the posting form of Pluribus, an Albanian classifieds site, from what a seller wrote \
(a title and a description, in Albanian) and the photos they added. Answer by calling \
fill_listing_form once.

- Use only the codes listed below. Leave out any field the listing doesn't say or clearly show: \
never guess a year, a mileage, an area, a floor, a number of rooms or a price.
- category: the most specific category the listing belongs in.
- listing_type: one of the category's types: SELL (for sale), BUY (wants to buy), RENT (for rent), \
WANTED_RENT (wants to rent), SERVICE (offers a service), JOB (a job offered or wanted).
- condition: only for a category marked (condition): NEW, LIKE_NEW (like new), USED, or FOR_PARTS \
(for parts, or not working).
- properties: only the chosen category's, by property code, all of them inside the properties \
object rather than beside category and the other fields: a listed option's code, a number, true \
for one that applies, or for a list the codes of all the options that apply. A model must be one \
listed under the make chosen.
- price: only when the seller wrote one, as a plain number (\"9 500 €\" is 9500, \"95 mijë euro\" \
95000, \"1,2 milion lekë\" 1200000), with currency EUR for euros (€, euro, eur) or ALL for lekë. \
price_type: NEGOTIABLE when the price can be discussed (\"i diskutueshëm\", \"negociohet\"), FREE \
when it is given away (\"falas\", \"dhurohet\"), ON_REQUEST when the seller asks to be contacted for \
the price (\"me marrëveshje\"), otherwise FIXED.
- city: the town the listing is in, when the text names it; zone: one of that town's zones, when \
the text names it.";

/// What the form gets back: its fields by name (category, listing_type, condition, price_type,
/// price, currency, city, zone) and the category's properties by field name ("prop_vehicle_make"),
/// each with its value or values.
#[derive(Debug, Default, Serialize)]
struct Filled {
    fields: BTreeMap<&'static str, String>,
    properties: BTreeMap<String, Vec<String>>,
}

/// The form's title, description and photos, and the category if the seller chose it themselves:
/// what the AI made of them, checked, as JSON; or, with an error status, a message to show.
pub async fn fill(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Submission { pairs, photos }: Submission,
) -> Result<Response> {
    let Some(ai) = state.ai.clone() else {
        return Ok(refusal(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE));
    };
    let field = |name: &str| pairs.iter().find(|(key, _)| key == name).map_or("", |(_, value)| value.trim());
    let title: String = field("title").chars().take(TITLE_CHARS).collect();
    let description: String = field("description").chars().take(DESCRIPTION_CHARS).collect();
    if title.chars().count() < 5 || description.chars().count() < 10 {
        return Ok(refusal(StatusCode::UNPROCESSABLE_ENTITY, WRITE_FIRST));
    }
    let user = current_user(&state, &headers).await?;
    let seller = match (&user, &ip) {
        (Some(user), _) => Some(format!("account {}", user.id)),
        (None, Some(ip)) => Some(format!("network {ip}")),
        (None, None) => None,
    };
    if let Err(message) = take_fill(seller.as_deref()) {
        return Ok(refusal(StatusCode::TOO_MANY_REQUESTS, message));
    }

    let (categories, cities, codes, catalogue) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        db::category_properties(&state.db),
        db::catalogue_properties(&state.db),
    )?;
    let leaves = leaves(&categories);
    // A category the seller chose themselves stands.
    let chosen = leaf(&categories, field("category"));
    let mut text = format!("Titulli: {title}\n\nPërshkrimi:\n{description}");
    if let Some(category) = chosen {
        text.push_str(&format!(
            "\n\nThe seller chose the category themselves: {} ({}). Keep it, and fill in its properties.",
            category.slug,
            path(&categories, category)
        ));
    }
    let photos = small_copies(photos).await;
    let instructions = instructions(&categories, &leaves, &cities, &codes, &catalogue);
    let answer = match ai.call_tool(&instructions, &tool(&leaves), &photos, &text).await {
        Ok(answer) => answer,
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "the AI couldn't fill in the listing form");
            return Ok(refusal(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE));
        }
    };
    let filled = checked(&state, &categories, &cities, &answer, chosen).await?;
    Ok(Json(filled).into_response())
}

fn refusal(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "message": message }))).into_response()
}

/// The first PHOTOS photos, as JPEGs no bigger than PHOTO_SIDE; one that can't be read is left out.
async fn small_copies(photos: Vec<Bytes>) -> Vec<Vec<u8>> {
    let photos: Vec<Bytes> = photos.into_iter().filter(|photo| photo.len() <= MAX_UPLOAD_BYTES).take(PHOTOS).collect();
    tokio::task::spawn_blocking(move || {
        photos.iter().filter_map(|photo| media::small_copy(photo, PHOTO_SIDE).ok()).collect::<Vec<_>>()
    })
    .await
    .unwrap_or_default()
}

/// The categories listings go into, in the form's order.
fn leaves(categories: &Categories) -> Vec<&Category> {
    fn add<'a>(categories: &'a Categories, category: &'a Category, leaves: &mut Vec<&'a Category>) {
        if categories.has_children(category.id) {
            for child in categories.children(category.id) {
                add(categories, child, leaves);
            }
        } else {
            leaves.push(category);
        }
    }
    let mut leaves = Vec::new();
    for top in categories.top_level() {
        add(categories, top, &mut leaves);
    }
    leaves
}

/// The category by its code, if listings go into it.
fn leaf<'a>(categories: &'a Categories, slug: &str) -> Option<&'a Category> {
    categories.by_slug(slug).filter(|category| !categories.has_children(category.id))
}

/// "Automjete › Makina".
fn path(categories: &Categories, category: &Category) -> String {
    let mut names: Vec<&str> = categories.ancestors(category).iter().map(|above| above.name.as_str()).collect();
    names.push(&category.name);
    names.join(" › ")
}

/// The rules, then the lists: the categories with their kinds of listing and properties, each
/// property with what it takes (a car's models under their makes), the towns and their zones.
/// Built from the database each time, in a fixed order, so the text is the same from one fill to
/// the next and Anthropic's cache keeps it.
fn instructions(
    categories: &Categories,
    leaves: &[&Category],
    cities: &[City],
    codes: &HashMap<Uuid, Vec<String>>,
    catalogue: &[CatalogueProperty],
) -> String {
    let mut text = String::from(RULES);
    text.push_str("\n\nCategories (code: path [listing types] (condition) properties):\n");
    for category in leaves {
        let properties = codes.get(&category.id).filter(|codes| !codes.is_empty()).map_or("none".to_string(), |codes| codes.join(", "));
        let condition = if category.uses_condition { " (condition)" } else { "" };
        text.push_str(&format!(
            "{}: {} [{}]{condition}: {properties}\n",
            category.slug,
            path(categories, category),
            category.listing_types.join(", ")
        ));
    }
    text.push_str("\nProperties (code: name, and what it takes):\n");
    let used: HashSet<&str> = codes.values().flatten().map(String::as_str).collect();
    for property in catalogue.iter().filter(|property| used.contains(property.code.as_str())) {
        text.push_str(&describe(property));
        text.push('\n');
    }
    text.push_str("\nTowns (code name):\n");
    text.push_str(&cities.iter().map(|city| format!("{} {}", city.slug, city.name)).collect::<Vec<_>>().join(" | "));
    text.push_str("\n\nZones, in the towns that have them (town: code name):\n");
    for city in cities.iter().filter(|city| !city.zones.is_empty()) {
        let zones: Vec<String> = city.zones.iter().map(|zone| format!("{} {}", zone.slug, zone.name)).collect();
        text.push_str(&format!("{}: {}\n", city.slug, zones.join(" | ")));
    }
    text
}

/// A property for the instructions: "vehicle_fuel: Karburanti (one of): PETROL Benzinë | ...".
fn describe(property: &CatalogueProperty) -> String {
    let head = format!("{}: {}", property.code, property.name);
    let option = |option: &db::CatalogueOption| format!("{} {}", option.code, option.label);
    match property.data_type.as_str() {
        "SELECT" | "MULTISELECT" => {
            let how = if property.data_type == "SELECT" { "one of" } else { "a list of any of" };
            match &property.parent_code {
                // A car's models, under their makes.
                Some(parent) => {
                    let mut groups: Vec<(&str, Vec<String>)> = Vec::new();
                    for item in property.options.iter() {
                        let under = item.parent.as_deref().unwrap_or("");
                        match groups.last_mut() {
                            Some((last, items)) if *last == under => items.push(option(item)),
                            _ => groups.push((under, vec![option(item)])),
                        }
                    }
                    let lines: Vec<String> = groups.iter().map(|(under, items)| format!("  {under}: {}", items.join(" | "))).collect();
                    format!("{head} ({how}, those under the {parent} chosen):\n{}", lines.join("\n"))
                }
                None => format!("{head} ({how}): {}", property.options.iter().map(option).collect::<Vec<_>>().join(" | ")),
            }
        }
        "NUMBER" => {
            let mut what = String::from("a number");
            if let Some(unit) = &property.unit {
                what.push_str(&format!(" in {unit}"));
            }
            if let Some(min) = property.min {
                what.push_str(&format!(", at least {}", min.normalize()));
            }
            if let Some(max) = property.max {
                what.push_str(&format!(", at most {}", max.normalize()));
            }
            format!("{head} ({what})")
        }
        "BOOLEAN" => format!("{head} (true when it applies)"),
        _ => format!("{head} (a short text)"),
    }
}

/// The tool the AI answers with: the form's fields, the category one of the codes listed.
fn tool<'a>(leaves: &[&Category]) -> Tool<'a> {
    let codes: Vec<&str> = leaves.iter().map(|category| category.slug.as_str()).collect();
    Tool {
        name: TOOL,
        description: "Fills in the listing form with what the listing says or shows; what it doesn't is left out.",
        input_schema: json!({
            "type": "object",
            "properties": {
                "category": {"type": "string", "enum": codes},
                "listing_type": {"type": "string", "enum": LISTING_TYPES},
                "condition": {"type": "string", "enum": CONDITIONS},
                "properties": {
                    "type": "object",
                    "description": "The category's properties, by code: an option's code, a number, true, or a list of option codes",
                    "additionalProperties": {
                        "anyOf": [
                            {"type": "string"},
                            {"type": "number"},
                            {"type": "boolean"},
                            {"type": "array", "items": {"type": "string"}}
                        ]
                    }
                },
                "price": {"type": "number"},
                "currency": {"type": "string", "enum": ["EUR", "ALL"]},
                "price_type": {"type": "string", "enum": PRICE_TYPES.map(|(code, _)| code)},
                "city": {"type": "string", "description": "A town's code"},
                "zone": {"type": "string", "description": "A zone's code, in the town given"}
            },
            "required": ["category"],
            "additionalProperties": false
        }),
    }
}

/// The AI's answer in the form's terms, each value checked as the form checks what a seller sends
/// (post.rs, properties.rs); what doesn't fit is left out.
async fn checked(
    state: &AppState,
    categories: &Categories,
    cities: &[City],
    answer: &Value,
    chosen: Option<&Category>,
) -> Result<Filled> {
    let mut filled = Filled::default();
    let text = |name: &str| answer[name].as_str().map(str::trim).filter(|value| !value.is_empty());

    let category = chosen.or_else(|| text("category").and_then(|slug| leaf(categories, slug)));
    if let Some(category) = category {
        filled.fields.insert("category", category.slug.clone());
        let listing_type = match category.listing_types.as_slice() {
            [only] => Some(only.as_str()),
            types => text("listing_type").filter(|wanted| types.iter().any(|kind| kind == wanted)),
        };
        if let Some(listing_type) = listing_type {
            filled.fields.insert("listing_type", listing_type.to_string());
        }
        if let Some(condition) = text("condition").filter(|condition| category.uses_condition && CONDITIONS.contains(condition)) {
            filled.fields.insert("condition", condition.to_string());
        }
        let attributes = db::form_attributes(&state.db, category.id).await?;
        let given = given_properties(answer);
        filled.properties = checked_properties(&attributes, &given);
        // What the form couldn't take, for the log: a code it doesn't have, a value it doesn't list.
        let dropped: Vec<&str> = given
            .keys()
            .filter(|code| !filled.properties.contains_key(&field_name(code)))
            .map(String::as_str)
            .collect();
        if !dropped.is_empty() {
            tracing::info!(category = %category.slug, ?dropped, "the form doesn't take some of the AI's properties");
        }
    }

    // The price: an amount for a fixed or negotiable one, the kind alone for the others.
    let price_type = text("price_type").filter(|wanted| PRICE_TYPES.iter().any(|(code, _)| code == wanted));
    let price = number(&answer["price"]).filter(|price| *price > Decimal::ZERO && *price < Decimal::from(1_000_000_000));
    match (price_type, price) {
        (Some(kind @ ("FREE" | "ON_REQUEST")), _) | (Some(kind), None) => {
            filled.fields.insert("price_type", kind.to_string());
        }
        (kind, Some(price)) => {
            filled.fields.insert("price_type", kind.unwrap_or("FIXED").to_string());
            filled.fields.insert("price", typed(price));
            if let Some(currency) = text("currency").filter(|currency| matches!(*currency, "EUR" | "ALL")) {
                filled.fields.insert("currency", currency.to_string());
            }
        }
        (None, None) => {}
    }

    // The town, and a property's zone in it.
    if let Some(city) = text("city").and_then(|slug| cities.iter().find(|city| city.slug == slug)) {
        filled.fields.insert("city", city.slug.clone());
        let in_zones = category.is_some_and(|category| zoned(categories, category));
        if let Some(zone) = text("zone").filter(|_| in_zones).and_then(|slug| city.zones.iter().find(|zone| zone.slug == slug)) {
            filled.fields.insert("zone", zone.slug.clone());
        }
    }
    Ok(filled)
}

/// The fields the tool takes beside the properties.
const FIELDS: [&str; 9] = ["category", "listing_type", "condition", "properties", "price", "currency", "price_type", "city", "zone"];

/// The properties the AI gave, by code: in `properties`, as it is asked to, but also where it
/// sometimes puts them instead, beside the other fields (a car's make next to its category: all of
/// a Golf's were there for some photos, and the form got none), or in `properties` as a JSON text;
/// named by the form's field name too ("prop_vehicle_make"). Those in `properties` come first.
fn given_properties(answer: &Value) -> serde_json::Map<String, Value> {
    let nested = match &answer["properties"] {
        Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
        value => value.clone(),
    };
    let mut given = serde_json::Map::new();
    let beside = answer.as_object().into_iter().flatten().filter(|(name, _)| !FIELDS.contains(&name.as_str()));
    for (name, value) in nested.as_object().into_iter().flatten().chain(beside) {
        let code = name.strip_prefix("prop_").unwrap_or(name);
        given.entry(code.to_string()).or_insert_with(|| value.clone());
    }
    given
}

/// The category's properties as the AI gave them (given_properties), by field name, those the
/// form takes: checked as the form checks them (properties::check), a list's options only those it
/// has, and a model only with its make.
fn checked_properties(attributes: &[FormAttribute], given: &serde_json::Map<String, Value>) -> BTreeMap<String, Vec<String>> {
    let mut values: HashMap<String, Vec<String>> = HashMap::new();
    for attribute in attributes {
        let value = given.get(&attribute.code).unwrap_or(&Value::Null);
        let mut texts: Vec<String> = match value {
            Value::String(text) => vec![text.trim().to_string()],
            Value::Number(_) => number(value).map(typed).into_iter().collect(),
            Value::Bool(true) => vec!["1".to_string()],
            Value::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    Value::String(text) => Some(text.trim().to_string()),
                    Value::Number(_) => number(item).map(typed),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        texts.retain(|text| !text.is_empty() && text != OTHER);
        match attribute.data_type.as_str() {
            "MULTISELECT" => texts.retain(|text| attribute.options.iter().any(|option| option.code == *text)),
            "BOOLEAN" => texts.retain(|text| text == "1"),
            _ => texts.truncate(1),
        }
        if !texts.is_empty() {
            values.insert(attribute.code.clone(), texts);
        }
    }
    let mut errors = HashMap::new();
    properties::check(attributes, &values, &mut errors);
    values.retain(|code, _| !errors.contains_key(&field_name(code)));
    // A model without its make has nothing to be checked against: it goes too.
    let kept: HashSet<Uuid> = attributes.iter().filter(|attribute| values.contains_key(&attribute.code)).map(|attribute| attribute.id).collect();
    for attribute in attributes {
        if attribute.parent_attribute_id.is_some_and(|parent| !kept.contains(&parent)) {
            values.remove(&attribute.code);
        }
    }
    values.into_iter().map(|(code, texts)| (field_name(&code), texts)).collect()
}

/// A number the AI gave, as a number or as text ("9 500").
fn number(value: &Value) -> Option<Decimal> {
    match value {
        Value::Number(number) => number.as_f64().and_then(Decimal::from_f64).map(|number| number.round_dp(2)),
        Value::String(text) => parse_number(text),
        _ => None,
    }
}

/// A number the way a seller types it into the form: "9500", "12,5".
fn typed(number: Decimal) -> String {
    number.normalize().to_string().replace('.', ",")
}

/// When fills were asked for: by seller over the last hour, and on the whole site over the last
/// day. In memory, like the posting limits (post.rs): a restart forgets them.
#[derive(Default)]
struct Fills {
    by_seller: HashMap<String, Vec<Instant>>,
    site: Vec<Instant>,
}

fn fills() -> &'static Mutex<Fills> {
    static FILLS: OnceLock<Mutex<Fills>> = OnceLock::new();
    FILLS.get_or_init(Default::default)
}

/// Counts one more fill for `seller` (None for one not known: only the site's limit applies), or
/// says why there can't be one now.
fn take_fill(seller: Option<&str>) -> std::result::Result<(), &'static str> {
    let now = Instant::now();
    let hour_ago = now.checked_sub(Duration::from_secs(3600));
    let day_ago = now.checked_sub(Duration::from_secs(24 * 3600));
    let mut fills = fills().lock().unwrap();
    fills.site.retain(|time| day_ago.is_none_or(|start| *time > start));
    fills.by_seller.retain(|_, times| {
        times.retain(|time| hour_ago.is_none_or(|start| *time > start));
        !times.is_empty()
    });
    if fills.site.len() >= FILLS_PER_DAY {
        return Err(BUSY);
    }
    if let Some(seller) = seller {
        let times = fills.by_seller.entry(seller.to_string()).or_default();
        if times.len() >= FILLS_PER_HOUR {
            return Err(TOO_MANY);
        }
        times.push(now);
    }
    fills.site.push(now);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_come_back_as_sellers_type_them() {
        assert_eq!(number(&json!(9500)).map(typed).as_deref(), Some("9500"));
        assert_eq!(number(&json!(9500.0)).map(typed).as_deref(), Some("9500"));
        assert_eq!(number(&json!(12.5)).map(typed).as_deref(), Some("12,5"));
        assert_eq!(number(&json!("85")).map(typed).as_deref(), Some("85"));
        assert_eq!(number(&json!(true)), None);
    }

    #[test]
    fn properties_count_wherever_the_ai_puts_them() {
        let given = |answer: Value| Value::Object(given_properties(&answer));
        // In properties, as asked.
        assert_eq!(given(json!({"category": "makina", "properties": {"vehicle_make": "VOLKSWAGEN"}})), json!({"vehicle_make": "VOLKSWAGEN"}));
        // Beside the other fields, as a Golf's all were for some photos; those in properties first.
        assert_eq!(
            given(json!({
                "category": "makina", "price": 8900, "city": "tirane", "vehicle_year": 2016, "vehicle_make": "AUDI",
                "properties": {"vehicle_make": "VOLKSWAGEN"}
            })),
            json!({"vehicle_make": "VOLKSWAGEN", "vehicle_year": 2016})
        );
        // As a JSON text, and by the form's field names.
        assert_eq!(given(json!({"properties": "{\"vehicle_fuel\": \"DIESEL\"}"})), json!({"vehicle_fuel": "DIESEL"}));
        assert_eq!(given(json!({"properties": {"prop_vehicle_fuel": "DIESEL"}})), json!({"vehicle_fuel": "DIESEL"}));
        assert_eq!(given(json!({"category": "makina", "properties": "not JSON"})), json!({}));
    }
}
