//! What a seller gave for a category's properties (db::form_attributes: a car's make, model, year
//! and mileage, a flat's rooms and area), checked against them as the pages' posting form, the AI's
//! fill of it (pluribus-web's properties.rs and ai_fill.rs) and the JSON API's listings
//! (pluribus-api's own.rs) check it alike: `check`, which gives back the values to save
//! (db::ListingProperty) and each property's problem, by its code, in the language now. The
//! values come as the form sends them, by the property's code, several for a multi-select, a
//! select's "Tjetër" (`OTHER`) with the name typed under `<code>_other`. pluribus-web's
//! properties.rs's until C9, which lays the form's fields out still.

use std::collections::HashMap;

use pluribus_core::{
    format,
    i18n::{t, t1, t2},
};
use pluribus_db::{FormAttribute, ListingProperty, PropertyValue};
use rust_decimal::Decimal;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::filters::parse_number;

/// The value a select sends for "Tjetër": the seller types the name the list lacks, under the
/// property's code with `_other` after it ("vehicle_make_other").
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

/// What is wrong with what was given for a category's properties: each property's first problem,
/// by its code ("vehicle_year"), said in the language now, in the properties' order.
pub type PropertyErrors = Vec<(String, String)>;

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

/// Checks the values given, by property code, against the category's properties: what to save, and
/// each property's problem in the language now (a required one left out, an option the list lacks,
/// a model of another make than the one chosen, a number out of its bounds, a name typed for
/// "Tjetër" that can't be one, a text over 200 characters, a pair of years the wrong way round).
pub fn check(attributes: &[FormAttribute], values: &HashMap<String, Vec<String>>) -> (Vec<ListingProperty>, PropertyErrors) {
    let mut properties = Vec::new();
    let mut errors = PropertyErrors::new();
    // The options chosen so far, by property, None for a typed one: a model must belong to the
    // chosen make.
    let mut chosen: HashMap<Uuid, Option<Uuid>> = HashMap::new();
    for attribute in attributes {
        let typed: Vec<&str> = values
            .get(&attribute.code)
            .map(|values| values.iter().map(|value| value.trim()).filter(|value| !value.is_empty()).collect())
            .unwrap_or_default();
        let mut fail = |message: String| errors.push((attribute.code.clone(), message));
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
    check_year_pairs(attributes, &properties, &mut errors);
    (properties, errors)
}

/// A pair of years, "<x>_from_year" and "<x>_to_year" (the years a part fits), can't end before
/// it starts: the later one's problem, unless it has one already.
fn check_year_pairs(attributes: &[FormAttribute], properties: &[ListingProperty], errors: &mut PropertyErrors) {
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
            && !errors.iter().any(|(code, _)| *code == closing.code)
        {
            errors.push((closing.code.clone(), t1("Duhet të jetë {} ose më vonë.", from.normalize())));
        }
    }
}

#[cfg(test)]
mod tests {
    use pluribus_db::AttributeOption;
    use sqlx::types::Json;

    use super::*;

    /// A property of the form's, numbered `n`.
    fn attribute(n: u128, code: &str, data_type: &str, options: Vec<AttributeOption>) -> FormAttribute {
        FormAttribute {
            id: Uuid::from_u128(n),
            code: code.to_string(),
            name: code.to_string(),
            data_type: data_type.to_string(),
            unit: None,
            parent_attribute_id: None,
            required: false,
            allow_other: false,
            min: None,
            max: None,
            options: Json(options),
        }
    }

    fn option(n: u128, code: &str, label: &str, parent: Option<u128>) -> AttributeOption {
        AttributeOption { id: Uuid::from_u128(n), code: code.to_string(), label: label.to_string(), parent_id: parent.map(Uuid::from_u128) }
    }

    /// A car's make and model, its year and its options, and a part's years.
    fn car() -> Vec<FormAttribute> {
        let make = FormAttribute {
            required: true,
            allow_other: true,
            ..attribute(1, "vehicle_make", "SELECT", vec![option(11, "VOLKSWAGEN", "Volkswagen", None), option(12, "BMW", "BMW", None)])
        };
        let model = FormAttribute {
            parent_attribute_id: Some(make.id),
            allow_other: true,
            ..attribute(2, "vehicle_model", "SELECT", vec![option(21, "VOLKSWAGEN_GOLF", "Golf", Some(11)), option(22, "BMW_X5", "X5", Some(12))])
        };
        let year = FormAttribute { min: Some(Decimal::from(1950)), ..attribute(3, "vehicle_year", "NUMBER", Vec::new()) };
        let features = attribute(4, "vehicle_features", "MULTISELECT", vec![option(41, "ABS", "ABS", None), option(42, "NAVIGATION", "Navigacion", None)]);
        let from = attribute(5, "fits_from_year", "NUMBER", Vec::new());
        let to = attribute(6, "fits_to_year", "NUMBER", Vec::new());
        vec![make, model, year, features, from, to]
    }

    fn given(pairs: &[(&str, &str)]) -> HashMap<String, Vec<String>> {
        let mut values: HashMap<String, Vec<String>> = HashMap::new();
        for (code, value) in pairs {
            values.entry(code.to_string()).or_default().push(value.to_string());
        }
        values
    }

    #[test]
    fn what_a_seller_gave_is_checked_against_the_categorys_properties() {
        let car = car();
        let (properties, errors) = check(
            &car,
            &given(&[("vehicle_make", "VOLKSWAGEN"), ("vehicle_model", "VOLKSWAGEN_GOLF"), ("vehicle_year", "2016"), ("vehicle_features", "ABS"), ("vehicle_features", "NAVIGATION"), ("vehicle_features", "WINGS")]),
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            properties.iter().map(|property| property.value.clone()).collect::<Vec<_>>(),
            [
                PropertyValue::Option(Uuid::from_u128(11)),
                PropertyValue::Option(Uuid::from_u128(21)),
                PropertyValue::Number(Decimal::from(2016)),
                PropertyValue::Options(vec![Uuid::from_u128(41), Uuid::from_u128(42)]),
            ]
        );

        // Each property's first problem, by its code: the make left out, a year over next year's,
        // and a pair of years the wrong way round, on the later one.
        let next = OffsetDateTime::now_utc().year() + 1;
        let (_, errors) = check(&car, &given(&[("vehicle_year", "3000"), ("fits_from_year", "2012"), ("fits_to_year", "2010")]));
        assert_eq!(
            errors,
            [
                ("vehicle_make".to_string(), "Plotëso „vehicle_make“.".to_string()),
                ("vehicle_year".to_string(), format!("Duhet të jetë nga 1950 deri në {next}.")),
                ("fits_to_year".to_string(), "Duhet të jetë 2012 ose më vonë.".to_string()),
            ]
        );

        // A model of another make, an option the list lacks, and a number that isn't one.
        let (_, errors) = check(&car, &given(&[("vehicle_make", "BMW"), ("vehicle_model", "VOLKSWAGEN_GOLF"), ("vehicle_year", "dikur")]));
        assert_eq!(errors[0], ("vehicle_model".to_string(), "Ky model nuk i përket markës së zgjedhur.".to_string()));
        assert_eq!(errors[1], ("vehicle_year".to_string(), "Shkruaj një numër.".to_string()));
        let (_, errors) = check(&car, &given(&[("vehicle_make", "TESLA")]));
        assert_eq!(errors, [("vehicle_make".to_string(), "Zgjidh një nga mundësitë.".to_string())]);

        // "Tjetër": a make typed as the list has it is the listed one; one it lacks is new, its model
        // under it; one that can't be a name is refused.
        let (properties, errors) = check(&car, &given(&[("vehicle_make", OTHER), ("vehicle_make_other", " bmw ")]));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(properties[0].value, PropertyValue::Option(Uuid::from_u128(12)));
        let (properties, _) = check(
            &car,
            &given(&[("vehicle_make", OTHER), ("vehicle_make_other", "Zastava"), ("vehicle_model", OTHER), ("vehicle_model_other", "Yugo  45")]),
        );
        assert_eq!(
            properties.iter().map(|property| property.value.clone()).collect::<Vec<_>>(),
            [
                PropertyValue::NewOption { label: "Zastava".to_string(), code: "ZASTAVA".to_string(), parent_attribute_id: None },
                PropertyValue::NewOption { label: "Yugo 45".to_string(), code: "YUGO_45".to_string(), parent_attribute_id: Some(Uuid::from_u128(1)) },
            ]
        );
        let (_, errors) = check(&car, &given(&[("vehicle_make", OTHER), ("vehicle_make_other", "www.example.com")]));
        assert_eq!(errors, [("vehicle_make".to_string(), NAME_PROBLEM.to_string())]);
    }
}
