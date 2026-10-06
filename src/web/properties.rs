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

impl PropertyField {
    /// What the field says inside it, as its placeholder or a list's empty choice: the label, with
    /// the unit, and an asterisk when it is required ("Kilometrazhi (km)", "Marka *").
    pub fn placeholder(&self) -> String {
        let mut text = self.label.clone();
        if let Some(unit) = &self.unit {
            text.push_str(&format!(" ({unit})"));
        }
        if self.required {
            text.push_str(" *");
        }
        text
    }
}

/// The value a select sends for "Tjetër".
pub const OTHER: &str = "__other__";

/// What's wrong with a name typed for "Tjetër" that isn't plausible_name.
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

/// The form's fields for these properties, holding what was typed (or what the listing has).
pub fn fields(
    attributes: &[FormAttribute],
    values: &HashMap<String, Vec<String>>,
    errors: &HashMap<String, String>,
) -> Vec<PropertyField> {
    attributes
        .iter()
        .map(|attribute| {
            let name = field_name(&attribute.code);
            let typed = values.get(&attribute.code).map(Vec::as_slice).unwrap_or_default();
            let chosen = |code: &str| typed.iter().any(|value| value == code);
            let choice = |option: &db::AttributeOption| Choice {
                value: option.code.clone(),
                label: option.label.clone(),
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
                            label: parent_option.label.clone(),
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
                label: attribute.name.clone(),
                unit: attribute.unit.clone(),
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
        .collect()
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
/// the field's name.
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
                fail(format!("Plotëso \u{201e}{}\u{201c}.", attribute.name));
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
                    fail(format!("Shkruaj \u{201e}{}\u{201c}.", attribute.name));
                    continue;
                }
                if !plausible_name(&text) {
                    fail(NAME_PROBLEM.to_string());
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
                    if let (Some(parent), Some(parent_option)) = (parent_chosen, option.parent_id) {
                        if *parent != Some(parent_option) {
                            fail("Ky model nuk i përket markës së zgjedhur.".to_string());
                            continue;
                        }
                    }
                    chosen.insert(attribute.id, Some(option.id));
                    PropertyValue::Option(option.id)
                }
                None => {
                    fail("Zgjidh një nga mundësitë.".to_string());
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
                            (Some(min), Some(max)) => format!("Duhet të jetë nga {} deri në {}.", bound(min), bound(max)),
                            (Some(min), None) => format!("Duhet të jetë të paktën {}.", bound(min)),
                            (None, Some(max)) => format!("Duhet të jetë deri në {}.", bound(max)),
                            (None, None) => unreachable!(),
                        });
                        continue;
                    }
                    PropertyValue::Number(number)
                }
                None => {
                    fail("Shkruaj një numër.".to_string());
                    continue;
                }
            },
            "BOOLEAN" => PropertyValue::Boolean(true),
            _ => {
                let text = typed[0];
                if text.chars().count() > 200 {
                    fail("Deri në 200 shkronja.".to_string());
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
        if let (Some(from), Some(to)) = (number(opening), number(closing)) {
            if from > to {
                errors
                    .entry(field_name(&closing.code))
                    .or_insert(format!("Duhet të jetë {} ose më vonë.", from.normalize()));
            }
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
