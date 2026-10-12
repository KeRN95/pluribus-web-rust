//! The reference data the app loads before anything else, for anyone: the categories as their tree
//! (`GET /categories`), the towns listings go into with their zones (`GET /cities`), and a
//! category's properties, as its filters offer them or as its posting form asks for them
//! (`GET /categories/{slug}/fields?for=filters|form`). Each comes from the very function the pages
//! load it with (db::categories, db::cities, db::filter_attributes, db::form_attributes), so that
//! the app offers what the site does; each name in both languages at once, the Albanian the
//! database keeps and the English an English page says for it (`english`), so that the app changes
//! language without asking again; and each answer may be kept five minutes (`Cached`), for it
//! changes only with a migration. The shapes they answer in are here, beside their handlers.

use std::fmt::Display;

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use pluribus_core::{
    domain::{DataType, ListingType},
    i18n, t,
};
use pluribus_db::{self as db, AttributeOption, Categories, Category, City, FilterAttribute, FormAttribute, Zone};
use pluribus_services::AppState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ApiError;

/// How long the app, and any cache on the way, may keep these answers: five minutes. They change
/// only with a migration (a category, a property or a town added), and nothing in them is anyone's
/// own or in one language alone.
pub const CACHE_CONTROL: &str = "public, max-age=300";

/// An answer that may be kept a while (`CACHE_CONTROL`), in JSON as any other. A failure isn't:
/// it answers in the envelope alone.
pub struct Cached<T>(pub T);

impl<T: Serialize> IntoResponse for Cached<T> {
    fn into_response(self) -> Response {
        ([(header::CACHE_CONTROL, HeaderValue::from_static(CACHE_CONTROL))], Json(self.0)).into_response()
    }
}

/// What an English page says for one of the database's own words (a category's name, a
/// property's, an option's, a unit), whatever language the request asks for: the catalogue's
/// English (i18n/en/data.txt), or the word itself where it has none ("Volkswagen", "km"), as `t`
/// says it. Not a category's `name_i18n`, which keeps the English of its name from before it was
/// one word (migration 0042), for search, nor an option's `label_i18n`: the pages read neither,
/// and the two often say otherwise ("Home & Garden" for Shtëpi, where a page says "Home").
fn english(text: &str) -> String {
    i18n::in_english(text).unwrap_or(text).to_string()
}

/// A category, with those under it (`GET /categories`).
#[derive(Debug, Serialize)]
pub struct CategoryDto {
    /// What names it in an address and a filter: "makina" (/c/makina, `category=makina`).
    pub slug: String,
    /// Its name in Albanian, one word where it can be, as the pages show it: "Makina".
    pub name: String,
    /// Its name as an English page shows it: "Cars".
    pub name_en: String,
    /// The category it is under; none for a main one.
    pub parent_slug: Option<String>,
    /// Whether listings go into it: not into one that only groups others ("Automjete"), whose
    /// listings go into one of its `children`.
    pub allow_listings: bool,
    /// The currency its posting form starts with: "EUR" for cars and property, "ALL" for most.
    pub default_currency: String,
    /// The kinds of listing it takes, of SELL, BUY, RENT, WANTED_RENT, SERVICE and JOB: what its
    /// posting form offers, and its filters where there are several.
    pub listing_types: Vec<ListingType>,
    /// Whether its listings say what condition the thing is in (new, used...).
    pub uses_condition: bool,
    /// The categories under it, in their order, each likewise.
    pub children: Vec<CategoryDto>,
}

impl CategoryDto {
    /// `category`, under `parent`, with every category under it.
    fn of(categories: &Categories, category: &Category, parent: Option<&Category>) -> Result<Self, ApiError> {
        let listing_types = category
            .listing_types
            .iter()
            .map(|kind| kind.parse().map_err(anyhow::Error::from))
            .collect::<Result<Vec<ListingType>, _>>()?;
        let children = categories
            .children(category.id)
            .map(|child| Self::of(categories, child, Some(category)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            slug: category.slug.clone(),
            name: category.name.clone(),
            name_en: english(&category.name),
            parent_slug: parent.map(|parent| parent.slug.clone()),
            allow_listings: !categories.has_children(category.id),
            default_currency: category.default_currency.clone(),
            listing_types,
            uses_condition: category.uses_condition,
            children,
        })
    }
}

/// `GET /categories`: the whole tree, each main category in the pages' order with those under it,
/// as the welcome page and the posting form list them (db::categories, the active ones).
pub async fn categories(State(state): State<AppState>) -> Result<Cached<Vec<CategoryDto>>, ApiError> {
    let categories = db::categories(&state.db).await?;
    let tree = categories.top_level().map(|main| CategoryDto::of(&categories, main, None)).collect::<Result<_, _>>()?;
    Ok(Cached(tree))
}

/// A town listings go into (`GET /cities`): a municipality, named after its main town. A place's
/// name is the same in English.
#[derive(Debug, Serialize)]
pub struct CityDto {
    pub id: Uuid,
    /// "Tiranë".
    pub name: String,
    /// What names it in a filter: "tirane" (`city=tirane`).
    pub slug: String,
    /// Its county, which the filters list the towns under: "Tiranë".
    pub county: String,
    /// Its point, where its listings stand on the map without one of their own.
    pub latitude: f64,
    pub longitude: f64,
    /// The parts of the town people name, which a property listing can say it is in (Blloku), by
    /// name; none for most towns.
    pub zones: Vec<ZoneDto>,
}

/// A zone of a town (`CityDto::zones`).
#[derive(Debug, Serialize)]
pub struct ZoneDto {
    pub id: Uuid,
    /// "Blloku".
    pub name: String,
    /// What names it in a filter, beside its town: "blloku" (`city=tirane&zone=blloku`).
    pub slug: String,
}

impl From<City> for CityDto {
    fn from(city: City) -> Self {
        Self {
            id: city.id,
            name: city.name,
            slug: city.slug,
            county: city.county,
            latitude: city.latitude,
            longitude: city.longitude,
            zones: city.zones.into_iter().map(ZoneDto::from).collect(),
        }
    }
}

impl From<Zone> for ZoneDto {
    fn from(zone: Zone) -> Self {
        Self { id: zone.id, name: zone.name, slug: zone.slug }
    }
}

/// `GET /cities`: the towns, by county, each county's main town first, as the filters list them
/// (db::cities).
pub async fn cities(State(state): State<AppState>) -> Result<Cached<Vec<CityDto>>, ApiError> {
    let cities = db::cities(&state.db).await?;
    Ok(Cached(cities.into_iter().map(CityDto::from).collect()))
}

/// Which of a category's properties `GET /categories/{slug}/fields` gives (`?for=`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldsFor {
    /// Those its filters offer.
    Filters,
    /// Those its posting form asks for.
    Form,
}

/// The query of `GET /categories/{slug}/fields`.
#[derive(Debug, Deserialize)]
pub struct FieldsQuery {
    #[serde(rename = "for")]
    pub fields_for: Option<FieldsFor>,
}

/// One of a category's properties, as its filters offer it or as its posting form asks for it
/// (`GET /categories/{slug}/fields`): a car's make, a flat's rooms.
#[derive(Debug, Serialize)]
pub struct FieldDto {
    /// What names it, and its value in a filter or a listing: "vehicle_make"
    /// (`vehicle_make=VOLKSWAGEN`, `vehicle_year_min=2010`).
    pub code: String,
    /// Its name in Albanian, the category's own name for it where it has one: "Marka".
    pub label: String,
    /// Its name as an English page shows it: "Brand".
    pub label_en: String,
    /// What its values are: TEXT, NUMBER, BOOLEAN, SELECT or MULTISELECT.
    pub data_type: DataType,
    /// A number's unit, as the pages write it ("km", "inç"); none for most.
    pub unit: Option<String>,
    /// The unit as an English page writes it ("km", "in").
    pub unit_en: Option<String>,
    /// Whether the category asks every listing for it.
    pub required: bool,
    /// Whether the seller may give an option the list lacks ("Tjetër"), as for a car's make and
    /// model: in the posting form; never in the filters.
    pub allow_other: bool,
    /// The property among the category's whose options this one's each belong under: a car's
    /// model, under its make ("vehicle_make"); none for most.
    pub parent_code: Option<String>,
    /// Its options, a SELECT's or a MULTISELECT's, in their order; none for the others.
    pub options: Vec<OptionDto>,
    /// The bounds the posting form holds a number to, as the property keeps them
    /// (`attributes.validation_json`, whose `min` and `max` the form reads): `{"min": 1900}`,
    /// `{"min": 25, "max": 10000}`, `{}` for none; null for the filters, which hold to none.
    pub validation: Option<ValidationDto>,
    /// Its place among the category's properties, from 0, in the category's order, which the
    /// answer is in too.
    pub order: usize,
}

/// One of a property's options (`FieldDto::options`).
#[derive(Debug, Serialize)]
pub struct OptionDto {
    /// What names it as the property's value, in a filter or a listing: "VOLKSWAGEN".
    pub code: String,
    /// Its label in Albanian: "Naftë".
    pub label: String,
    /// Its label as an English page shows it: "Diesel".
    pub label_en: String,
    /// For a property that depends on another (`FieldDto::parent_code`), the code of the other's
    /// option it belongs under: a Golf's make, "VOLKSWAGEN"; none for the others.
    pub parent_option: Option<String>,
}

/// The bounds the posting form holds a number to (`FieldDto::validation`), each where the property
/// has one.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct ValidationDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<Bound>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<Bound>,
}

/// A bound as validation_json writes it: a whole number as one (1900), any other with its
/// decimals (0.5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Bound {
    Whole(i64),
    Fraction(f64),
}

impl Bound {
    /// The bound a number says, from its digits without the zeros its scale adds ("1900", "0.5").
    fn of(number: impl Display) -> Option<Self> {
        let digits = number.to_string();
        digits.parse().map(Self::Whole).ok().or_else(|| digits.parse().map(Self::Fraction).ok())
    }
}

/// What the filters' properties (`FilterAttribute`) and the form's (`FormAttribute`) say alike,
/// which a field is made from, with what the form's alone say: whether the seller may give an
/// option of their own, and the bounds.
struct Property<'a> {
    id: Uuid,
    code: &'a str,
    name: &'a str,
    data_type: &'a str,
    unit: Option<&'a str>,
    parent_attribute_id: Option<Uuid>,
    required: bool,
    options: &'a [AttributeOption],
    allow_other: bool,
    validation: Option<ValidationDto>,
}

impl<'a> From<&'a FilterAttribute> for Property<'a> {
    fn from(attribute: &'a FilterAttribute) -> Self {
        Self {
            id: attribute.id,
            code: &attribute.code,
            name: &attribute.name,
            data_type: &attribute.data_type,
            unit: attribute.unit.as_deref(),
            parent_attribute_id: attribute.parent_attribute_id,
            required: attribute.required,
            options: attribute.options.as_slice(),
            allow_other: false,
            validation: None,
        }
    }
}

impl<'a> From<&'a FormAttribute> for Property<'a> {
    fn from(attribute: &'a FormAttribute) -> Self {
        Self {
            id: attribute.id,
            code: &attribute.code,
            name: &attribute.name,
            data_type: &attribute.data_type,
            unit: attribute.unit.as_deref(),
            parent_attribute_id: attribute.parent_attribute_id,
            required: attribute.required,
            options: attribute.options.as_slice(),
            allow_other: attribute.allow_other,
            validation: Some(ValidationDto {
                min: attribute.min.and_then(|min| Bound::of(min.normalize())),
                max: attribute.max.and_then(|max| Bound::of(max.normalize())),
            }),
        }
    }
}

impl FieldDto {
    /// The fields of a category's properties, in their order. A property that depends on another
    /// does so where the other is among them, each of its options under one of the other's (an
    /// option under one the category doesn't offer left out); where the other isn't, it depends
    /// on none: as the filters and the form show them.
    fn all(properties: &[Property<'_>]) -> Result<Vec<Self>, ApiError> {
        properties
            .iter()
            .enumerate()
            .map(|(order, property)| {
                let parent = property.parent_attribute_id.and_then(|id| properties.iter().find(|other| other.id == id));
                let options = property
                    .options
                    .iter()
                    .filter_map(|option| {
                        let parent_option = match (parent, option.parent_id) {
                            (Some(parent), Some(id)) => Some(parent.options.iter().find(|of| of.id == id)?.code.clone()),
                            _ => None,
                        };
                        Some(OptionDto {
                            code: option.code.clone(),
                            label: option.label.clone(),
                            label_en: english(&option.label),
                            parent_option,
                        })
                    })
                    .collect();
                Ok(Self {
                    code: property.code.to_string(),
                    label: property.name.to_string(),
                    label_en: english(property.name),
                    data_type: property.data_type.parse().map_err(anyhow::Error::from)?,
                    unit: property.unit.map(str::to_string),
                    unit_en: property.unit.map(english),
                    required: property.required,
                    allow_other: property.allow_other,
                    parent_code: parent.map(|parent| parent.code.to_string()),
                    options,
                    validation: property.validation,
                    order,
                })
            })
            .collect()
    }
}

/// `GET /categories/{slug}/fields?for=filters|form`: a category's properties, its own and those it
/// takes from the categories above it, in its order: as its filters offer them (`for=filters`,
/// db::filter_attributes, as its page and a search in it have them), or as its posting form asks
/// for them (`for=form`, db::form_attributes, as the form's properties frame has them: none for a
/// category that only groups others, whose listings go into one under it). A `for` that is
/// neither, or none, is a validation error on `for`; a category the site hasn't, or not any more,
/// is not found.
pub async fn fields(
    State(state): State<AppState>,
    slug: Result<Path<String>, PathRejection>,
    query: Result<Query<FieldsQuery>, QueryRejection>,
) -> Result<Cached<Vec<FieldDto>>, ApiError> {
    let Ok(Query(FieldsQuery { fields_for: Some(fields_for) })) = query else {
        return Err(ApiError::Validation(vec![("for", t("Duhet të jetë „filters“ ose „form“.").to_string())]));
    };
    // A slug that isn't text (its percent-encoding no UTF-8) names no category either.
    let Path(slug) = slug.map_err(|_| ApiError::NotFound)?;
    let categories = db::categories(&state.db).await?;
    let category = categories.by_slug(&slug).ok_or(ApiError::NotFound)?;
    let fields = match fields_for {
        FieldsFor::Filters => {
            let attributes = db::filter_attributes(&state.db, &category.slug).await?;
            FieldDto::all(&attributes.iter().map(Property::from).collect::<Vec<_>>())?
        }
        FieldsFor::Form if categories.has_children(category.id) => Vec::new(),
        FieldsFor::Form => {
            let attributes = db::form_attributes(&state.db, category.id).await?;
            FieldDto::all(&attributes.iter().map(Property::from).collect::<Vec<_>>())?
        }
    };
    Ok(Cached(fields))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_is_written_as_validation_json_writes_it() {
        let written = |digits: &str| Bound::of(digits).map(|bound| serde_json::to_string(&bound).unwrap());
        assert_eq!(written("1900").as_deref(), Some("1900"));
        assert_eq!(written("-3").as_deref(), Some("-3"));
        assert_eq!(written("0.5").as_deref(), Some("0.5"));
        assert_eq!(written("none"), None);
        // Each bound where there is one, and none written where there isn't.
        let bounds = ValidationDto { min: Bound::of("0"), max: None };
        assert_eq!(serde_json::to_string(&bounds).unwrap(), r#"{"min":0}"#);
        assert_eq!(serde_json::to_string(&ValidationDto::default()).unwrap(), "{}");
    }
}
