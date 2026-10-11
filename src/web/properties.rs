//! A category's properties in the listing form: a car's make, model, year and mileage, a flat's
//! rooms and area. The fields come from the database (README, "Categories and their properties"),
//! so a new property needs a migration, not code.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{RawQuery, State},
    response::Response,
};
use rust_decimal::Decimal;
use time::OffsetDateTime;

use super::{
    Result,
    filters::{Choice, Params, parse_number},
    render,
};
use crate::{
    AppState,
    db::{self, FormAttribute, ListingProperty, PropertyValue},
    format,
    i18n::{t, t1, t2},
};

/// A select's options grouped under the option of the property they depend on: car models under
/// their make. `parent` is the make's code, which listing_form_controller.js narrows by.
pub struct OptionGroup {
    pub label: String,
    pub parent: String,
    pub choices: Vec<Choice>,
}

/// A property as the form shows it.
pub struct PropertyField {
    pub name: String,
    pub label: String,
    pub unit: Option<String>,
    pub required: bool,
    /// select, multi, number, check or text.
    pub kind: &'static str,
    pub value: String,
    pub checked: bool,
    pub choices: Vec<Choice>,
    pub groups: Vec<OptionGroup>,
    /// The field this one narrows to: "prop_vehicle_make" for the model.
    pub depends_on: Option<String>,
    /// "Tjetër": the seller may type an option the list lacks, in the field named `other_name`.
    pub allow_other: bool,
    pub other_selected: bool,
    pub other_name: String,
    pub other_value: String,
    pub error: Option<String>,
}

/// A car's main fields, first and in this order, two to a line as the canvas has them: the make
/// and the model, the year and the fuel, the gearbox and the body, the colour and the mileage.
/// (Tiles with an icon before the name once, the mileage before the colour.)
const CAR_ORDER: [&str; 8] = [
    "vehicle_make",
    "vehicle_model",
    "vehicle_year",
    "vehicle_fuel",
    "vehicle_transmission",
    "vehicle_body_type",
    "vehicle_color",
    "vehicle_mileage",
];

/// What a car's title says as the details would (listing_generated_title, migration 0027: "Volkswagen
/// Golf 1.6, 2016"): its listing page leaves them out of the details. Its engine stays among them,
/// the title having it only to a tenth of a litre.
pub const TITLE_CODES: [&str; 3] = ["vehicle_make", "vehicle_model", "vehicle_year"];

/// A car's body types, each the car's side, facing right, its windows and its wheels, drawn in
/// strokes in a box twice as wide as it is tall (48 by 24 units), every car on the same ground:
/// the filters show them over the names. A body type added later has none until it is drawn.
const BODY_TYPES: [(&str, &str); 7] = [
    ("SEDAN", r#"<path d="M8.5 18H5a1 1 0 0 1-1-1v-3.1a1.5 1.5 0 0 1 1.3-1.49L13 11.5l4.1-4.24A2 2 0 0 1 18.5 6.6h11.3a2 2 0 0 1 1.4.58L36 11.5l6.8 1.13A1.5 1.5 0 0 1 44 14.1V17a1 1 0 0 1-1 1h-3.5M15.5 18h17M13 11.5h23M24.5 6.6v4.9"/><circle cx="12" cy="18" r="3"/><circle cx="36" cy="18" r="3"/>"#),
    ("HATCHBACK", r#"<path d="M11 18H8.5a1 1 0 0 1-1-1v-5l2.9-4.6a2 2 0 0 1 1.7-.8h14.9a2 2 0 0 1 1.4.58L33.1 11.5l6.2 1.13A1.5 1.5 0 0 1 40.5 14.1V17a1 1 0 0 1-1 1h-.5M18 18h14M7.8 11.5h25.3M21 6.6v4.9"/><circle cx="14.5" cy="18" r="3"/><circle cx="35.5" cy="18" r="3"/>"#),
    ("WAGON", r#"<path d="M8.5 18H5a1 1 0 0 1-1-1v-4.2l1.1-5.1A1.5 1.5 0 0 1 6.6 6.6h23.2a2 2 0 0 1 1.4.58L36 11.5l6.8 1.13A1.5 1.5 0 0 1 44 14.1V17a1 1 0 0 1-1 1h-3.5M15.5 18h17M4.3 11.5H36M14 6.6v4.9M25 6.6v4.9"/><circle cx="12" cy="18" r="3"/><circle cx="36" cy="18" r="3"/>"#),
    ("SUV", r#"<path d="M8.6 17.5H5a1 1 0 0 1-1-1V9.9l.8-4.7A1.5 1.5 0 0 1 6.3 4h22.5a2 2 0 0 1 1.5.67L34.5 9.5l7.9 1.1A1.5 1.5 0 0 1 44 12.1v4.4a1 1 0 0 1-1 1h-3.6M16.4 17.5h15.2M4.1 9.5h30.4M23.5 4v5.5"/><circle cx="12.5" cy="17.5" r="3.5"/><circle cx="35.5" cy="17.5" r="3.5"/>"#),
    ("COUPE", r#"<path d="M8.5 18H5a1 1 0 0 1-1-1v-3a1.5 1.5 0 0 1 1.2-1.47L9 11.9l7.6-3.9a4 4 0 0 1 1.8-.45h5.1a3 3 0 0 1 2 .76L30 12l12.8 1.1A1.5 1.5 0 0 1 44 14.6V17a1 1 0 0 1-1 1h-3.5M15.5 18h17M9 11.9 30 12"/><circle cx="12" cy="18" r="3"/><circle cx="36" cy="18" r="3"/>"#),
    ("CONVERTIBLE", r#"<path d="M8.5 18H5a1 1 0 0 1-1-1v-3.1a1.5 1.5 0 0 1 1.3-1.49L13 11.5h20l9.8 1.13A1.5 1.5 0 0 1 44 14.1V17a1 1 0 0 1-1 1h-3.5M15.5 18h17M33 11.5l-3.4-4.6M17 11.5V9.2a1.5 1.5 0 0 1 3 0v2.3"/><circle cx="12" cy="18" r="3"/><circle cx="36" cy="18" r="3"/>"#),
    ("VAN", r#"<path d="M8.5 18H5a1 1 0 0 1-1-1V6a2.5 2.5 0 0 1 2.5-2.5h21.3a2 2 0 0 1 1.5.68L38.5 10l4.3 1.1A1.5 1.5 0 0 1 44 12.6V17a1 1 0 0 1-1 1h-3.5M15.5 18h17M4 10h34.5M23 3.5V10"/><circle cx="12" cy="18" r="3"/><circle cx="36" cy="18" r="3"/>"#),
];

/// The icon of one of a property's options, where it has one: a car's body type, its side (the
/// inside of a 48-by-24 SVG drawn in strokes, BODY_TYPES).
pub fn option_icon(code: &str, option: &str) -> Option<&'static str> {
    match code {
        "vehicle_body_type" => BODY_TYPES.iter().find(|(of, _)| *of == option).map(|(_, icon)| *icon),
        _ => None,
    }
}

/// The value a select sends for "Tjetër".
pub const OTHER: &str = "__other__";

/// What's wrong with a name typed for "Tjetër" that isn't plausible_name; the form says it in the
/// page's language (t), the moderators' page in Albanian.
pub const NAME_PROBLEM: &str = "Shkruaj vetëm emrin, me 2 deri në 40 shkronja.";

/// A name typed for "Tjetër", or put right by a moderator, that can be one: 2 to 40 characters,
/// some of them letters or digits, and no address in it.
pub fn plausible_name(text: &str) -> bool {
    let lower = text.to_lowercase();
    text.chars().any(char::is_alphanumeric)
        && (2..=40).contains(&text.chars().count())
        && !["http", "www.", "@"].iter().any(|bit| lower.contains(bit))
}

pub fn field_name(code: &str) -> String {
    format!("prop_{code}")
}

/// Whether the form says what the asterisk means: not where nothing is required (a part's fields).
pub fn any_required(fields: &[PropertyField]) -> bool {
    fields.iter().any(|field| field.required)
}

/// The form's fields for these properties, holding what was typed (or what the listing has), their
/// names, units and options in the page's language.
pub fn fields(
    attributes: &[FormAttribute],
    values: &HashMap<String, Vec<String>>,
    errors: &HashMap<String, String>,
) -> Vec<PropertyField> {
    let mut fields: Vec<PropertyField> = attributes
        .iter()
        .map(|attribute| {
            let name = field_name(&attribute.code);
            let typed = values.get(&attribute.code).map(Vec::as_slice).unwrap_or_default();
            let chosen = |code: &str| typed.iter().any(|value| value == code);
            let choice = |option: &db::AttributeOption| Choice {
                value: option.code.clone(),
                label: t(&option.label).to_string(),
                selected: chosen(&option.code),
            };
            let parent = attribute
                .parent_attribute_id
                .and_then(|id| attributes.iter().find(|a| a.id == id));
            let (choices, groups) = match parent {
                // Dependent options, under their parent's: models under makes.
                Some(parent) => {
                    let groups = parent
                        .options
                        .iter()
                        .map(|parent_option| OptionGroup {
                            label: t(&parent_option.label).to_string(),
                            parent: parent_option.code.clone(),
                            choices: attribute
                                .options
                                .iter()
                                .filter(|option| option.parent_id == Some(parent_option.id))
                                .map(choice)
                                .collect(),
                        })
                        .filter(|group| !group.choices.is_empty())
                        .collect();
                    (Vec::new(), groups)
                }
                None => (attribute.options.iter().map(choice).collect(), Vec::new()),
            };
            PropertyField {
                label: t(&attribute.name).to_string(),
                unit: attribute.unit.as_deref().map(|unit| t(unit).to_string()),
                required: attribute.required,
                kind: match attribute.data_type.as_str() {
                    "SELECT" => "select",
                    "MULTISELECT" => "multi",
                    "NUMBER" => "number",
                    "BOOLEAN" => "check",
                    _ => "text",
                },
                value: typed.first().cloned().unwrap_or_default(),
                checked: chosen("1"),
                choices,
                groups,
                depends_on: parent.map(|parent| field_name(&parent.code)),
                allow_other: attribute.allow_other,
                other_selected: chosen(OTHER),
                other_name: field_name(&format!("{}_other", attribute.code)),
                other_value: values
                    .get(&format!("{}_other", attribute.code))
                    .and_then(|values| values.first().cloned())
                    .unwrap_or_default(),
                error: errors.get(&name).cloned(),
                name,
            }
        })
        .collect();
    // A car's (a part's make and model aren't, nor a motorcycle's year and mileage, which come
    // after its own make): its main fields first, in their order (CAR_ORDER), then the rest, then
    // its options, a long list.
    let has = |code: &str| attributes.iter().any(|attribute| attribute.code == code);
    if has("vehicle_make") && has("vehicle_year") {
        let rank = |field: &PropertyField| CAR_ORDER.iter().position(|code| field.name == field_name(code));
        fields.sort_by_key(|field| rank(field).unwrap_or(CAR_ORDER.len() + usize::from(field.kind == "multi")));
    }
    fields
}

/// The largest sensible value: a property's own maximum, and for a year, next year at most.
fn max_for(attribute: &FormAttribute) -> Option<Decimal> {
    let next_year = attribute
        .code
        .ends_with("_year")
        .then(|| Decimal::from(OffsetDateTime::now_utc().year() + 1));
    match (attribute.max, next_year) {
        (Some(max), Some(year)) => Some(max.min(year)),
        (max, year) => max.or(year),
    }
}

/// Checks the typed values against the category's properties; problems go into `errors` under
/// the field's name, in the page's language.
pub fn check(
    attributes: &[FormAttribute],
    values: &HashMap<String, Vec<String>>,
    errors: &mut HashMap<String, String>,
) -> Vec<ListingProperty> {
    let mut properties = Vec::new();
    // The options chosen so far, by property, None for a typed one: a model must belong to the
    // chosen make.
    let mut chosen: HashMap<uuid::Uuid, Option<uuid::Uuid>> = HashMap::new();
    for attribute in attributes {
        let name = field_name(&attribute.code);
        let typed: Vec<&str> = values
            .get(&attribute.code)
            .map(|values| values.iter().map(|value| value.trim()).filter(|value| !value.is_empty()).collect())
            .unwrap_or_default();
        let mut fail = |message: String| {
            errors.entry(name.clone()).or_insert(message);
        };
        if typed.is_empty() {
            if attribute.required {
                fail(t1("Plotëso „{}“.", t(&attribute.name)));
            }
            continue;
        }
        let option = |code: &str| attribute.options.iter().find(|option| option.code == code);
        let value = match attribute.data_type.as_str() {
            "SELECT" if typed[0] == OTHER && attribute.allow_other => {
                let other = format!("{}_other", attribute.code);
                let text = values.get(&other).and_then(|values| values.first()).map(|text| text.trim()).unwrap_or_default();
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                let lower = text.to_lowercase();
                if text.is_empty() {
                    fail(t1("Shkruaj „{}“.", t(&attribute.name)));
                    continue;
                }
                if !plausible_name(&text) {
                    fail(t(NAME_PROBLEM).to_string());
                    continue;
                }
                // Typed as it is in the list ("bmw"): the listed one.
                let parent_option = attribute.parent_attribute_id.and_then(|parent| chosen.get(&parent).copied().flatten());
                let listed = attribute.options.iter().find(|option| {
                    option.label.to_lowercase() == lower && (option.parent_id.is_none() || option.parent_id == parent_option)
                });
                match listed {
                    Some(option) => {
                        chosen.insert(attribute.id, Some(option.id));
                        PropertyValue::Option(option.id)
                    }
                    None => {
                        chosen.insert(attribute.id, None);
                        PropertyValue::NewOption {
                            code: format::slug(&text).to_uppercase().replace('-', "_"),
                            label: text,
                            parent_attribute_id: attribute.parent_attribute_id,
                        }
                    }
                }
            }
            "SELECT" => match option(typed[0]) {
                Some(option) => {
                    let parent_chosen = attribute.parent_attribute_id.and_then(|parent| chosen.get(&parent));
                    if let (Some(parent), Some(parent_option)) = (parent_chosen, option.parent_id)
                        && *parent != Some(parent_option)
                    {
                        fail(t("Ky model nuk i përket markës së zgjedhur.").to_string());
                        continue;
                    }
                    chosen.insert(attribute.id, Some(option.id));
                    PropertyValue::Option(option.id)
                }
                None => {
                    fail(t("Zgjidh një nga mundësitë.").to_string());
                    continue;
                }
            },
            "MULTISELECT" => {
                let ids: Vec<_> = typed.iter().filter_map(|code| option(code)).map(|option| option.id).collect();
                if ids.is_empty() {
                    continue;
                }
                PropertyValue::Options(ids)
            }
            "NUMBER" => match parse_number(typed[0]) {
                Some(number) => {
                    let max = max_for(attribute);
                    let too_small = attribute.min.is_some_and(|min| number < min);
                    let too_big = max.is_some_and(|max| number > max);
                    if too_small || too_big {
                        let bound = |value: Decimal| value.normalize().to_string();
                        fail(match (attribute.min, max) {
                            (Some(min), Some(max)) => t2("Duhet të jetë nga {} deri në {}.", bound(min), bound(max)),
                            (Some(min), None) => t1("Duhet të jetë të paktën {}.", bound(min)),
                            (None, Some(max)) => t1("Duhet të jetë deri në {}.", bound(max)),
                            (None, None) => unreachable!(),
                        });
                        continue;
                    }
                    PropertyValue::Number(number)
                }
                None => {
                    fail(t("Shkruaj një numër.").to_string());
                    continue;
                }
            },
            "BOOLEAN" => PropertyValue::Boolean(true),
            _ => {
                let text = typed[0];
                if text.chars().count() > 200 {
                    fail(t("Deri në 200 shkronja.").to_string());
                    continue;
                }
                PropertyValue::Text(text.to_string())
            }
        };
        properties.push(ListingProperty { attribute_id: attribute.id, value });
    }
    check_year_pairs(attributes, &properties, errors);
    properties
}

/// A pair of years, "<x>_from_year" and "<x>_to_year" (the years a part fits), can't end before
/// it starts.
fn check_year_pairs(attributes: &[FormAttribute], properties: &[ListingProperty], errors: &mut HashMap<String, String>) {
    let number = |attribute: &FormAttribute| {
        properties.iter().find(|property| property.attribute_id == attribute.id).and_then(|property| match &property.value {
            PropertyValue::Number(number) => Some(*number),
            _ => None,
        })
    };
    for opening in attributes {
        let Some(prefix) = opening.code.strip_suffix("_from_year") else { continue };
        let closing_code = format!("{prefix}_to_year");
        let Some(closing) = attributes.iter().find(|attribute| attribute.code == closing_code) else { continue };
        if let (Some(from), Some(to)) = (number(opening), number(closing))
            && from > to
        {
            errors
                .entry(field_name(&closing.code))
                .or_insert(t1("Duhet të jetë {} ose më vonë.", from.normalize()));
        }
    }
}

/// The property fields alone, in their frame: the form reloads them when the category changes.
#[derive(Template)]
#[template(path = "listing_properties_frame.html")]
struct PropertiesFrame {
    fields: Vec<PropertyField>,
}

pub async fn fields_for_category(State(state): State<AppState>, RawQuery(query): RawQuery) -> Result<Response> {
    let params = Params::parse(query.as_deref());
    let categories = db::categories(&state.db).await?;
    let category = params
        .first("category")
        .and_then(|slug| categories.by_slug(slug))
        .filter(|category| !categories.has_children(category.id));
    let attributes = match category {
        Some(category) => db::form_attributes(&state.db, category.id).await?,
        None => Vec::new(),
    };
    render(PropertiesFrame { fields: fields(&attributes, &HashMap::new(), &HashMap::new()) })
}
