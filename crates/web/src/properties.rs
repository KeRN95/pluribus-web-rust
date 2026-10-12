//! A category's properties in the listing form: a car's make, model, year and mileage, a flat's
//! rooms and area. The fields come from the database (README, "Categories and their properties"),
//! so a new property needs a migration, not code. What a seller gave for them is checked by
//! pluribus-services' properties.rs, which the JSON API checks by too (this file's until C9); here
//! its problems go under the form's fields (`check`).

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{RawQuery, State},
    response::Response,
};
use pluribus_services::{
    filters::Params,
    properties::{self, OTHER},
};

use super::{Result, filters::Choice, render};
use crate::{
    AppState,
    db::{self, FormAttribute, ListingProperty},
    i18n::t,
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

/// Checks the typed values against the category's properties (pluribus-services' properties::check);
/// problems go into `errors` under the field's name, in the page's language.
pub fn check(
    attributes: &[FormAttribute],
    values: &HashMap<String, Vec<String>>,
    errors: &mut HashMap<String, String>,
) -> Vec<ListingProperty> {
    let (properties, problems) = properties::check(attributes, values);
    for (code, message) in problems {
        errors.entry(field_name(&code)).or_insert(message);
    }
    properties
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
