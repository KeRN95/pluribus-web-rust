//! The filters of a browse page: read from the query string, turned into the listing query, and
//! shown back as the filter form, removable chips and the URL of the next page.
//!
//! Parameters: q, category (on /search and /harta; on a category page the path names it), city,
//! lat, lng and radius (a map pin, and how many km around it), type, condition (repeatable),
//! price_min, price_max, currency, sort and after, harta=1 (a category or search page with its
//! map's dots, which its map fetches when it first opens), plus one per filterable
//! property of the category, named by the property's code: option codes for SELECT and
//! MULTISELECT properties (vehicle_fuel=DIESEL, repeatable), <code>_min and <code>_max for
//! NUMBER properties, and <code>=1 for BOOLEAN ones. Values that don't exist are ignored, so an
//! outdated or hand-edited URL still shows a page.

use rust_decimal::{Decimal, prelude::ToPrimitive};
use uuid::Uuid;

use super::properties;
use crate::{
    db::{
        AttributeFilter, AttributeOption, Categories, Category, City, Cursor, FilterAttribute,
        ListingFilter, Near, PriceRange, Sort,
    },
    format,
};

const CONDITIONS: [&str; 4] = ["NEW", "LIKE_NEW", "USED", "FOR_PARTS"];
const CURRENCIES: [&str; 2] = ["EUR", "ALL"];

/// How far around the map pin to look, in whole kilometres: anything from 1 to 50 (the map's
/// slider, see map_radius.html).
const MIN_RADIUS_KM: u32 = 1;
const MAX_RADIUS_KM: u32 = 50;
const DEFAULT_RADIUS_KM: u32 = 10;

/// Where a map's pin stands until the visitor sets another point: the middle of Tirana (Skanderbeg
/// Square). On the map page it is the point the results are for; elsewhere a map only starts there.
const TIRANA: (f64, f64) = (41.3275, 19.8187);

/// Properties with at most this many options are offered as boxes, the colours too, longer lists
/// as a dropdown.
const CHECKBOX_LIMIT: usize = 8;

/// The price's slider (price_range_controller.js, which works the same out): PRICE_STEPS positions
/// on a logarithmic scale of PRICE_DECADES decades from 10 €, or a hundred times as many Lekë, the
/// rate listings' prices are compared at; either end at its edge is no limit.
const PRICE_STEPS: u32 = 1000;
const PRICE_DECADES: f64 = 5.0;

/// A property's boxes share their line with the next field when there are at most this many;
/// more take a line of their own (Ngjyra's seven), as do boxes with icons, however few
/// (Karroceria's). See Field::half.
const HALF_LIMIT: usize = 5;

/// The query string as name/value pairs, without empty values.
pub struct Params(Vec<(String, String)>);

impl Params {
    pub fn parse(query: Option<&str>) -> Self {
        let pairs = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
            .map(|(name, value)| (name.into_owned(), value.trim().to_string()))
            .filter(|(_, value)| !value.is_empty())
            .collect();
        Self(pairs)
    }

    pub fn first(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    fn all<'p>(&'p self, name: &'p str) -> impl Iterator<Item = &'p str> {
        self.0.iter().filter(move |(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

/// An option of a dropdown, checkbox group or radio group.
pub struct Choice {
    pub value: String,
    pub label: String,
    pub selected: bool,
}

pub struct ChoiceGroup {
    pub label: String,
    pub choices: Vec<Choice>,
}

/// A county's towns, for the map's dropdown, each with its point.
pub struct TownGroup<'a> {
    pub label: &'a str,
    pub towns: Vec<&'a City>,
}

/// A dependent property's options under the parent option they belong to: a make's models.
pub struct OptionGroup {
    pub label: String,
    /// The parent option's code, "BMW".
    pub parent: String,
    /// Under another parent than the one chosen: hidden, until the form's choice changes.
    pub hidden: bool,
    pub choices: Vec<Choice>,
}

/// A filter that is in use, shown above the results with a link that removes it.
pub struct Chip {
    pub label: String,
    pub url: String,
}

/// A property's filter in the form.
pub struct Field {
    pub name: String,
    pub label: String,
    pub kind: FieldKind,
    pub choices: Vec<Choice>,
    /// Options grouped under the parent option they belong to, e.g. models under their make.
    pub groups: Vec<OptionGroup>,
    /// The field this one depends on, the make for the model: narrow_controller.js shows only the
    /// groups under its choice, as soon as it changes.
    pub depends_on: Option<String>,
    pub min: String,
    pub max: String,
    pub unit: String,
    pub checked: bool,
    /// A small field, which takes half the form's width, beside the next small one: a dropdown, a
    /// range, a few boxes (Marka beside Modeli, Viti beside Kilometrazhi, Karburanti beside
    /// Kambio). The others take a line of their own, as does a small one left without another
    /// beside it (Opsionet, after the colours; Filters::fields).
    pub half: bool,
    /// Its boxes have icons over the names, a car's side over each body type (Karroceria): they
    /// are bigger, and take a line of their own on every screen.
    pub icons: bool,
}

impl Field {
    /// The icon over an option's name, where it has one (properties::option_icon).
    pub fn icon(&self, choice: &Choice) -> Option<&'static str> {
        properties::option_icon(&self.name, &choice.value)
    }
}

#[derive(PartialEq, Eq)]
pub enum FieldKind {
    Select,
    Checkboxes,
    Range,
    Toggle,
    /// One year, the buyer's vehicle's, for a part's pair of years (FITS_YEAR_LABEL).
    Year,
}

/// The filter a pair of years makes, "<x>_from_year" and "<x>_to_year" (a part's): the year of the
/// buyer's vehicle, which the listing's years must include. Its parameter is "<x>_year".
const FITS_YEAR_LABEL: &str = "Viti i automjetit";

/// A browse page's filters, checked against the categories, cities and properties that exist.
pub struct Filters<'a> {
    path: String,
    /// On /search the category is one of the filters; on a category page it is the page.
    category_is_filter: bool,
    categories: &'a Categories,
    cities: &'a [City],
    pub q: Option<String>,
    pub category: Option<&'a Category>,
    pub city: Option<&'a City>,
    /// The map pin, with its radius: the one in the address, or on the map page without one there,
    /// TIRANA.
    pub near: Option<Near>,
    /// Whether the pin came in the address (lat and lng). The map page's Tirana isn't written into
    /// links.
    pin_in_url: bool,
    /// The radius picked for the map, in kilometres; it stays picked without a pin.
    pub radius_km: u32,
    /// Whether the map's dots are asked for (harta=1): a category or search page's map fetches them
    /// when it first opens (map_points_url).
    pub map_points: bool,
    pub listing_type: Option<String>,
    pub conditions: Vec<&'static str>,
    pub price_min: Option<Decimal>,
    pub price_max: Option<Decimal>,
    pub currency: &'static str,
    properties: Vec<Property<'a>>,
    pub sort: Sort,
    pub after: Option<Cursor>,
}

/// One filterable property and what is chosen for it.
struct Property<'a> {
    attribute: &'a FilterAttribute,
    selected: Vec<&'a AttributeOption>,
    min: Option<Decimal>,
    max: Option<Decimal>,
    checked: bool,
    /// For "<x>_from_year", the "<x>_to_year" it makes a pair with: the two filter as one year.
    fits_to: Option<&'a FilterAttribute>,
    year: Option<Decimal>,
}

impl<'a> Filters<'a> {
    /// `path` is where the page lives (and the form submits to); `category` is the page's
    /// category, or on /search the one the category parameter names; `attributes` are its
    /// filterable properties.
    pub fn new(
        params: &Params,
        path: String,
        category: Option<&'a Category>,
        category_is_filter: bool,
        categories: &'a Categories,
        cities: &'a [City],
        attributes: &'a [FilterAttribute],
    ) -> Self {
        let offered_types = category.map(|c| c.listing_types.as_slice()).unwrap_or_default();
        let listing_type = params
            .first("type")
            .filter(|value| offered_types.len() > 1 && offered_types.iter().any(|t| t == value))
            .map(str::to_string);

        let conditions = if category.is_none_or(|c| c.uses_condition) {
            CONDITIONS.into_iter().filter(|c| params.all("condition").any(|v| v == *c)).collect()
        } else {
            Vec::new()
        };

        let (price_min, price_max) = ordered(
            params.first("price_min").and_then(parse_number),
            params.first("price_max").and_then(parse_number),
        );
        let default_currency = category.map_or("ALL", |c| c.default_currency.as_str());
        let currency = CURRENCIES
            .into_iter()
            .find(|c| params.first("currency") == Some(*c))
            .or_else(|| CURRENCIES.into_iter().find(|c| *c == default_currency))
            .unwrap_or("ALL");

        // The closing year of a pair filters along with its opening one, not on its own.
        let mut properties: Vec<Property> = attributes
            .iter()
            .filter(|attribute| !closes_year_pair(attribute, attributes))
            .map(|attribute| Property::read(attribute, attributes, params))
            .collect();
        for i in 0..properties.len() {
            let Some(parent_id) = properties[i].attribute.parent_attribute_id else { continue };
            let chosen_parents: Vec<Uuid> = properties
                .iter()
                .find(|p| p.attribute.id == parent_id)
                .map(|parent| parent.selected.iter().map(|o| o.id).collect())
                .unwrap_or_default();
            if chosen_parents.is_empty() {
                continue;
            }
            // A Golf filters nothing once the make is BMW.
            properties[i].selected.retain(|o| o.parent_id.is_some_and(|id| chosen_parents.contains(&id)));
        }

        let radius_km = params
            .first("radius")
            .and_then(|value| value.parse().ok())
            .filter(|radius| (MIN_RADIUS_KM..=MAX_RADIUS_KM).contains(radius))
            .unwrap_or(DEFAULT_RADIUS_KM);
        let pin = match (
            params.first("lat").and_then(|value| parse_coordinate(value, 90.0)),
            params.first("lng").and_then(|value| parse_coordinate(value, 180.0)),
        ) {
            (Some(latitude), Some(longitude)) => Some((latitude, longitude)),
            _ => None,
        };
        let near = pin
            .or((path == "/harta").then_some(TIRANA))
            .map(|(latitude, longitude)| Near { latitude, longitude, radius_m: f64::from(radius_km) * 1000.0 });

        // With a pin, nearest first unless another order is picked; without one, newest first.
        let default_sort = if near.is_some() { Sort::Nearest } else { Sort::Newest };
        let sort = params
            .first("sort")
            .and_then(Sort::from_param)
            .filter(|sort| *sort != Sort::Nearest || near.is_some())
            .unwrap_or(default_sort);
        let after = params.first("after").and_then(|value| Cursor::decode(sort, value));

        Self {
            path,
            category_is_filter,
            categories,
            cities,
            q: params.first("q").map(str::to_string),
            category,
            city: params.first("city").and_then(|slug| cities.iter().find(|c| c.slug == slug)),
            near,
            pin_in_url: pin.is_some(),
            radius_km,
            map_points: params.first("harta") == Some("1"),
            listing_type,
            conditions,
            price_min,
            price_max,
            currency,
            properties,
            sort,
            after,
        }
    }

    pub fn listing_filter(&self) -> ListingFilter {
        let has_price = self.price_min.is_some() || self.price_max.is_some();
        ListingFilter {
            category_ids: self.category.map(|c| self.categories.subtree_ids(c.id)),
            location_id: self.city.map(|c| c.id),
            near: self.near,
            query: self.q.clone(),
            listing_type: self.listing_type.clone(),
            conditions: self.conditions.iter().map(|c| c.to_string()).collect(),
            price: has_price.then(|| PriceRange {
                min: self.price_min,
                max: self.price_max,
                currency: self.currency.to_string(),
            }),
            attributes: self.properties.iter().filter_map(Property::filter).collect(),
            sort: self.sort,
            after: self.after,
            seller: None,
        }
    }

    /// The filters as query-string pairs in a fixed order, without the page cursor.
    fn pairs(&self) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let mut push = |name: &str, value: &str| pairs.push((name.to_string(), value.to_string()));
        if let Some(q) = &self.q {
            push("q", q);
        }
        if let (true, Some(category)) = (self.category_is_filter, self.category) {
            push("category", &category.slug);
        }
        if let Some(city) = self.city {
            push("city", &city.slug);
        }
        match self.near {
            Some(near) if self.pin_in_url => {
                // Five decimals is about a metre.
                push("lat", &format!("{:.5}", near.latitude));
                push("lng", &format!("{:.5}", near.longitude));
                push("radius", &self.radius_km.to_string());
            }
            // The map's starting pin isn't written down, but a distance picked for it is.
            Some(_) if self.radius_km != DEFAULT_RADIUS_KM => push("radius", &self.radius_km.to_string()),
            _ => {}
        }
        if let Some(listing_type) = &self.listing_type {
            push("type", listing_type);
        }
        for condition in &self.conditions {
            push("condition", condition);
        }
        if let Some(min) = self.price_min {
            push("price_min", &plain(min));
        }
        if let Some(max) = self.price_max {
            push("price_max", &plain(max));
        }
        if self.price_min.is_some() || self.price_max.is_some() {
            push("currency", self.currency);
        }
        for property in &self.properties {
            let code = &property.attribute.code;
            if let (Some(_), Some(year)) = (property.fits_to, property.year) {
                push(&year_param(property.attribute), &plain(year));
            }
            for option in &property.selected {
                push(code, &option.code);
            }
            if let Some(min) = property.min {
                push(&format!("{code}_min"), &plain(min));
            }
            if let Some(max) = property.max {
                push(&format!("{code}_max"), &plain(max));
            }
            if property.checked {
                push(code, "1");
            }
        }
        if self.sort != self.default_sort() {
            push("sort", self.sort.param());
        }
        pairs
    }

    /// Nearest first with a map pin, newest first without one.
    fn default_sort(&self) -> Sort {
        if self.near.is_some() { Sort::Nearest } else { Sort::Newest }
    }

    /// This page's URL without the pairs `remove` matches.
    fn url_without(&self, remove: impl Fn(&str, &str) -> bool) -> String {
        url(&self.path, self.pairs().into_iter().filter(|(name, value)| !remove(name, value)))
    }

    pub fn next_url(&self, cursor: &str) -> String {
        let mut pairs = self.pairs();
        pairs.push(("after".to_string(), cursor.to_string()));
        url(&self.path, pairs)
    }

    /// Another category's page, keeping the search words, the city and the map pin.
    pub fn category_url(&self, category: &Category) -> String {
        let kept = self
            .pairs()
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "q" | "city" | "lat" | "lng" | "radius"));
        url(&format!("/c/{}", category.slug), kept)
    }

    /// A category under this page's, which narrows it: every filter stays, as it has this
    /// category's properties too (Pjesë dhe aksesorë with "Audi, 2006" opens GPS dhe navigatorë
    /// with them). One it doesn't take is ignored there, and gone from its links.
    pub fn narrowed_url(&self, category: &Category) -> String {
        let kept = self.pairs().into_iter().filter(|(name, _)| name != "category");
        url(&format!("/c/{}", category.slug), kept)
    }

    /// Every category, keeping what category_url keeps.
    pub fn all_categories_url(&self) -> String {
        let kept = self
            .pairs()
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "q" | "city" | "lat" | "lng" | "radius"));
        url("/search", kept)
    }

    /// The filters the map page's form has no fields for, which it carries along hidden.
    pub fn hidden_map_fields(&self) -> Vec<(String, String)> {
        self.pairs()
            .into_iter()
            .filter(|(name, _)| !matches!(name.as_str(), "lat" | "lng" | "radius" | "category" | "sort"))
            .collect()
    }

    /// This page with its map's dots, which the map fetches when it first opens: the page comes
    /// whole, and Turbo takes their frame out of it.
    pub fn map_points_url(&self) -> String {
        let mut pairs = self.pairs();
        pairs.push(("harta".to_string(), "1".to_string()));
        url(&self.path, pairs)
    }

    /// What the map's form on a category or search page carries along hidden: everything but the
    /// pin and its radius, which are its own fields. A picked order stays; otherwise a pin makes it
    /// nearest first.
    pub fn map_form_fields(&self) -> Vec<(String, String)> {
        self.pairs()
            .into_iter()
            .filter(|(name, _)| !matches!(name.as_str(), "lat" | "lng" | "radius"))
            .collect()
    }

    /// The same search everywhere, its map's point dropped and the rest kept: the map's "Të
    /// gjitha", and the point's chip.
    pub fn everywhere_url(&self) -> String {
        self.url_without(|name, _| matches!(name, "lat" | "lng" | "radius"))
    }

    /// The same search without any filters.
    pub fn clear_url(&self) -> String {
        self.url_without(|name, _| !matches!(name, "q" | "sort"))
    }

    pub fn chips(&self) -> Vec<Chip> {
        let mut chips = Vec::new();
        let mut chip = |label: String, url: String| chips.push(Chip { label, url });
        if let (true, Some(category)) = (self.category_is_filter, self.category) {
            // The properties belong to the category, so they go with it.
            let names: Vec<String> = self.properties.iter().flat_map(Property::param_names).collect();
            chip(
                category.name.clone(),
                self.url_without(|name, _| name == "category" || names.iter().any(|n| n == name)),
            );
        }
        if let Some(city) = self.city {
            chip(city.name.clone(), self.url_without(|name, _| name == "city"));
        }
        if let Some(label) = self.point_label() {
            chip(label, self.everywhere_url());
        }
        if let Some(listing_type) = &self.listing_type {
            chip(format::listing_type(listing_type).to_string(), self.url_without(|name, _| name == "type"));
        }
        for &condition in &self.conditions {
            chip(
                format::condition(condition).to_string(),
                self.url_without(|name, value| name == "condition" && value == condition),
            );
        }
        let symbol = format::currency_symbol(self.currency);
        if let Some(label) = range_label(self.price_min, self.price_max, Some(symbol), true) {
            chip(
                label,
                self.url_without(|name, _| matches!(name, "price_min" | "price_max" | "currency")),
            );
        }
        for property in &self.properties {
            let attribute = property.attribute;
            let code = attribute.code.as_str();
            if let (Some(_), Some(year)) = (property.fits_to, property.year) {
                let param = year_param(attribute);
                chip(format!("{FITS_YEAR_LABEL}: {year}"), self.url_without(|name, _| name == param));
            }
            for option in &property.selected {
                chip(
                    option.label.clone(),
                    self.url_without(|name, value| name == code && value == option.code),
                );
            }
            let unit = attribute.unit.as_deref();
            if let Some(range) = range_label(property.min, property.max, unit, unit.is_some()) {
                let (min, max) = (format!("{code}_min"), format!("{code}_max"));
                chip(
                    format!("{}: {range}", attribute.name),
                    self.url_without(|name, _| name == min || name == max),
                );
            }
            if property.checked {
                chip(attribute.name.clone(), self.url_without(|name, _| name == code));
            }
        }
        chips
    }

    /// The map's point, as its chip says it, "Brenda 10 km nga pika"; not on the map page, whose
    /// map is the pin.
    fn point_label(&self) -> Option<String> {
        (self.near.is_some() && self.path != "/harta").then(|| format!("Brenda {} km nga pika", self.radius_km))
    }

    /// What the filters' card says under "Filtrat": the filters on, as their chips say them, but
    /// the map's point, which is the map's button's beside it (its tooltip) and has its own chip;
    /// empty with none.
    pub fn summary(&self) -> String {
        let point = self.point_label();
        let labels: Vec<String> =
            self.chips().into_iter().map(|chip| chip.label).filter(|label| Some(label) != point.as_ref()).collect();
        labels.join(" · ")
    }

    // ------------------------------------------------------------ the form

    /// What the filter form carries along hidden, having no fields for it: the search words (the
    /// header's search has them), the town (a link may bring it; its chip drops it), the map pin
    /// and its radius.
    pub fn kept_fields(&self) -> Vec<(String, String)> {
        self.pairs()
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "q" | "city" | "lat" | "lng" | "radius"))
            .collect()
    }

    pub fn action(&self) -> &str {
        &self.path
    }

    pub fn q_value(&self) -> &str {
        self.q.as_deref().unwrap_or_default()
    }

    pub fn shows_category(&self) -> bool {
        self.category_is_filter
    }

    /// The towns the map's dropdown moves it onto, by county, each county's main town first.
    pub fn town_groups(&self) -> Vec<TownGroup<'a>> {
        let mut groups: Vec<TownGroup<'a>> = Vec::new();
        for city in self.cities {
            match groups.last_mut() {
                Some(group) if group.label == city.county => group.towns.push(city),
                _ => groups.push(TownGroup { label: &city.county, towns: vec![city] }),
            }
        }
        groups
    }

    pub fn category_choices(&self) -> Vec<Choice> {
        category_choices(self.categories, self.category.map(|c| c.slug.as_str()))
    }

    /// Empty unless the category takes more than one kind of listing.
    pub fn type_choices(&self) -> Vec<Choice> {
        let offered = self.category.map(|c| c.listing_types.as_slice()).unwrap_or_default();
        if offered.len() < 2 {
            return Vec::new();
        }
        offered
            .iter()
            .map(|code| Choice {
                value: code.clone(),
                label: format::listing_type(code).to_string(),
                selected: self.listing_type.as_ref() == Some(code),
            })
            .collect()
    }

    /// Empty for categories whose listings have no condition, like property or jobs.
    pub fn condition_choices(&self) -> Vec<Choice> {
        if !self.category.is_none_or(|c| c.uses_condition) {
            return Vec::new();
        }
        CONDITIONS
            .into_iter()
            .map(|code| Choice {
                value: code.to_string(),
                label: format::condition(code).to_string(),
                selected: self.conditions.contains(&code),
            })
            .collect()
    }

    pub fn currency_choices(&self) -> Vec<Choice> {
        CURRENCIES
            .into_iter()
            .map(|code| Choice {
                value: code.to_string(),
                label: format::currency_symbol(code).to_string(),
                selected: code == self.currency,
            })
            .collect()
    }

    pub fn price_min_value(&self) -> String {
        self.price_min.map(plain).unwrap_or_default()
    }

    pub fn price_max_value(&self) -> String {
        self.price_max.map(plain).unwrap_or_default()
    }

    /// Where the slider's lower end stands: at the left edge without a lowest price.
    pub fn price_from_position(&self) -> u32 {
        self.price_min.map_or(0, |min| price_position(min, self.currency).max(1))
    }

    /// Where its upper end stands: at the right edge without a highest price.
    pub fn price_to_position(&self) -> u32 {
        self.price_max.map_or(PRICE_STEPS, |max| price_position(max, self.currency).min(PRICE_STEPS - 1))
    }

    /// The prices beside the slider's name, as their chip says them, or "Çdo çmim".
    pub fn price_label(&self) -> String {
        range_label(self.price_min, self.price_max, Some(format::currency_symbol(self.currency)), true)
            .unwrap_or_else(|| "Çdo çmim".to_string())
    }

    /// What screen readers say for the slider's lower end: its price, or no limit.
    pub fn price_min_text(&self) -> String {
        self.price_text(self.price_min)
    }

    /// And for its upper end.
    pub fn price_max_text(&self) -> String {
        self.price_text(self.price_max)
    }

    fn price_text(&self, price: Option<Decimal>) -> String {
        match price {
            Some(price) => format!("{}\u{a0}{}", format::number(price), format::currency_symbol(self.currency)),
            None => "Pa kufi".to_string(),
        }
    }

    /// The default order's value is empty, so it stays out of URLs. Nearest first is only on
    /// offer with a map pin.
    pub fn sort_choices(&self) -> Vec<Choice> {
        let default = self.default_sort();
        Sort::ALL
            .into_iter()
            .filter(|sort| *sort != Sort::Nearest || self.near.is_some())
            .map(|sort| Choice {
                value: if sort == default { String::new() } else { sort.param().to_string() },
                label: match sort {
                    Sort::Newest => "Më të rejat",
                    Sort::PriceLow => "Më të lirat",
                    Sort::PriceHigh => "Më të shtrenjtat",
                    Sort::Nearest => "Më afër",
                }
                .to_string(),
                selected: sort == self.sort,
            })
            .collect()
    }

    /// Where the distance slider stands for the radius, from 0 to 100. The slider moves slowly
    /// near the pin and faster further out (km = 50 ^ (position / 100), so its middle is 7 km);
    /// map_controller.js turns a position back into kilometres.
    pub fn radius_position(&self) -> u32 {
        (f64::from(self.radius_km).ln() / f64::from(MAX_RADIUS_KM).ln() * 100.0).round() as u32
    }

    pub fn fields(&self) -> Vec<Field> {
        let mut fields: Vec<Field> = self.properties.iter().map(|property| property.field(&self.properties)).collect();
        // Small fields stand two to a line: one left over, alone, takes the whole line rather than
        // half of it beside nothing.
        for run in fields.chunk_by_mut(|one, next| one.half == next.half) {
            if run[0].half && run.len() % 2 == 1 {
                run[run.len() - 1].half = false;
            }
        }
        fields
    }
}

impl<'a> Property<'a> {
    /// `all` is the category's other filterable properties: a year pair's closing year is there.
    fn read(attribute: &'a FilterAttribute, all: &'a [FilterAttribute], params: &Params) -> Self {
        let code = attribute.code.as_str();
        let (mut selected, mut min, mut max, mut checked) = (Vec::new(), None, None, false);
        let fits_to = year_pair_end(attribute, all);
        if fits_to.is_some() {
            let year = params.first(&year_param(attribute)).and_then(parse_year);
            return Self { attribute, selected, min, max, checked, fits_to, year };
        }
        match attribute.data_type.as_str() {
            "SELECT" | "MULTISELECT" => {
                selected = attribute.options.iter().filter(|o| params.all(code).any(|value| value == o.code)).collect();
            }
            "NUMBER" => {
                (min, max) = ordered(
                    params.first(&format!("{code}_min")).and_then(parse_number),
                    params.first(&format!("{code}_max")).and_then(parse_number),
                );
            }
            "BOOLEAN" => checked = params.first(code) == Some("1"),
            _ => {}
        }
        Self { attribute, selected, min, max, checked, fits_to, year: None }
    }

    fn filter(&self) -> Option<AttributeFilter> {
        let attribute_id = self.attribute.id;
        if let Some(to) = self.fits_to {
            return self.year.map(|year| AttributeFilter::Fits { from_attribute_id: attribute_id, to_attribute_id: to.id, year });
        }
        let option_ids = || self.selected.iter().map(|o| o.id).collect();
        match self.attribute.data_type.as_str() {
            "SELECT" if !self.selected.is_empty() => {
                Some(AttributeFilter::AnyOf { attribute_id, option_ids: option_ids() })
            }
            "MULTISELECT" if !self.selected.is_empty() => {
                Some(AttributeFilter::AllOf { attribute_id, option_ids: option_ids() })
            }
            "NUMBER" if self.min.is_some() || self.max.is_some() => {
                Some(AttributeFilter::Range { attribute_id, min: self.min, max: self.max })
            }
            "BOOLEAN" if self.checked => Some(AttributeFilter::IsTrue { attribute_id }),
            _ => None,
        }
    }

    fn param_names(&self) -> Vec<String> {
        let code = &self.attribute.code;
        if self.fits_to.is_some() {
            return vec![year_param(self.attribute)];
        }
        vec![code.clone(), format!("{code}_min"), format!("{code}_max")]
    }

    fn field(&self, all: &[Property]) -> Field {
        let attribute = self.attribute;
        let choice = |option: &AttributeOption| Choice {
            value: option.code.clone(),
            label: option.label.clone(),
            selected: self.selected.iter().any(|s| s.id == option.id),
        };
        let mut field = Field {
            name: attribute.code.clone(),
            label: attribute.name.clone(),
            kind: FieldKind::Select,
            choices: Vec::new(),
            groups: Vec::new(),
            depends_on: None,
            min: self.min.map(plain).unwrap_or_default(),
            max: self.max.map(plain).unwrap_or_default(),
            unit: attribute.unit.clone().unwrap_or_default(),
            checked: self.checked,
            half: true,
            icons: false,
        };
        if self.fits_to.is_some() {
            field.name = year_param(attribute);
            field.label = FITS_YEAR_LABEL.to_string();
            field.kind = FieldKind::Year;
            field.min = self.year.map(plain).unwrap_or_default();
            return field;
        }
        match attribute.data_type.as_str() {
            "NUMBER" => field.kind = FieldKind::Range,
            "BOOLEAN" => field.kind = FieldKind::Toggle,
            data_type => {
                let parent = attribute
                    .parent_attribute_id
                    .and_then(|id| all.iter().find(|p| p.attribute.id == id));
                field.choices = attribute
                    .options
                    .iter()
                    .filter(|o| parent.is_none() || o.parent_id.is_none())
                    .map(choice)
                    .collect();
                // Every make's models, so that picking another make in the form offers its own
                // straight away; those of the makes not chosen start hidden.
                if let Some(parent) = parent {
                    field.depends_on = Some(parent.attribute.code.clone());
                    for parent_option in parent.attribute.options.iter() {
                        let choices: Vec<Choice> = attribute
                            .options
                            .iter()
                            .filter(|o| o.parent_id == Some(parent_option.id))
                            .map(choice)
                            .collect();
                        if !choices.is_empty() {
                            field.groups.push(OptionGroup {
                                label: parent_option.label.clone(),
                                parent: parent_option.code.clone(),
                                hidden: !parent.selected.is_empty()
                                    && !parent.selected.iter().any(|chosen| chosen.id == parent_option.id),
                                choices,
                            });
                        }
                    }
                }
                let few = parent.is_none() && attribute.options.len() <= CHECKBOX_LIMIT;
                if data_type == "MULTISELECT" || few {
                    field.kind = FieldKind::Checkboxes;
                    field.choices.extend(field.groups.drain(..).flat_map(|group| group.choices));
                    field.icons = field.choices.iter().any(|choice| field.icon(choice).is_some());
                    field.half = field.choices.len() <= HALF_LIMIT && !field.icons;
                }
            }
        }
        field
    }
}

/// For "<x>_from_year", the category's "<x>_to_year": the pair filters as one year.
fn year_pair_end<'a>(attribute: &FilterAttribute, all: &'a [FilterAttribute]) -> Option<&'a FilterAttribute> {
    let prefix = attribute.code.strip_suffix("_from_year")?;
    let closing = format!("{prefix}_to_year");
    all.iter().find(|other| other.code == closing)
}

/// "<x>_to_year", when the category has its "<x>_from_year".
fn closes_year_pair(attribute: &FilterAttribute, all: &[FilterAttribute]) -> bool {
    attribute
        .code
        .strip_suffix("_to_year")
        .is_some_and(|prefix| all.iter().any(|other| other.code == format!("{prefix}_from_year")))
}

/// The pair's parameter, "<x>_year": "fits_year".
fn year_param(opening: &FilterAttribute) -> String {
    format!("{}_year", opening.code.strip_suffix("_from_year").unwrap_or(&opening.code))
}

/// A year a vehicle could have been made in; anything else is ignored.
fn parse_year(value: &str) -> Option<Decimal> {
    let year: i32 = value.trim().parse().ok()?;
    (1900..=2100).contains(&year).then(|| Decimal::from(year))
}

/// The cities by county, for a dropdown.
pub fn city_groups(cities: &[City], selected: Option<&str>) -> Vec<ChoiceGroup> {
    let mut groups: Vec<ChoiceGroup> = Vec::new();
    for city in cities {
        let choice = Choice {
            value: city.slug.clone(),
            label: city.name.clone(),
            selected: selected == Some(city.slug.as_str()),
        };
        match groups.last_mut() {
            Some(group) if group.label == city.county => group.choices.push(choice),
            _ => groups.push(ChoiceGroup { label: city.county.clone(), choices: vec![choice] }),
        }
    }
    groups
}

/// Every category, for a dropdown: subcategories follow their parent, indented.
pub fn category_choices(categories: &Categories, selected: Option<&str>) -> Vec<Choice> {
    fn add(categories: &Categories, category: &Category, depth: usize, selected: Option<&str>, out: &mut Vec<Choice>) {
        out.push(Choice {
            value: category.slug.clone(),
            label: format!("{}{}", "\u{a0}\u{a0}\u{a0}".repeat(depth), category.name),
            selected: selected == Some(category.slug.as_str()),
        });
        if depth < 5 {
            for child in categories.children(category.id) {
                add(categories, child, depth + 1, selected, out);
            }
        }
    }
    let mut choices = Vec::new();
    for category in categories.top_level() {
        add(categories, category, 0, selected, &mut choices);
    }
    choices
}

fn url(path: &str, pairs: impl IntoIterator<Item = (String, String)>) -> String {
    let query = form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish();
    if query.is_empty() { path.to_string() } else { format!("{path}?{query}") }
}

/// A number as people type it: "5000", "5 000", "5.000" or "12,5". Dots and spaces group
/// thousands and a comma marks decimals, as in Albanian.
pub fn parse_number(text: &str) -> Option<Decimal> {
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, ',' | '-'))
        .map(|c| if c == ',' { '.' } else { c })
        .collect();
    let value: Decimal = cleaned.parse().ok()?;
    (value.abs() < Decimal::from(1_000_000_000_000_i64)).then_some(value)
}

/// A latitude (limit 90) or longitude (limit 180) in degrees.
fn parse_coordinate(text: &str, limit: f64) -> Option<f64> {
    let value: f64 = text.parse().ok()?;
    (value.is_finite() && value.abs() <= limit).then_some(value)
}

/// Bounds typed the wrong way round are swapped.
fn ordered(min: Option<Decimal>, max: Option<Decimal>) -> (Option<Decimal>, Option<Decimal>) {
    match (min, max) {
        (Some(min), Some(max)) if min > max => (Some(max), Some(min)),
        bounds => bounds,
    }
}

/// A number for a form field or a URL: "5000", "12.5".
fn plain(value: Decimal) -> String {
    value.normalize().to_string()
}

/// A price's place on the slider, 0 to PRICE_STEPS (see PRICE_STEPS).
fn price_position(price: Decimal, currency: &str) -> u32 {
    let base = if currency == "ALL" { 1000.0 } else { 10.0 };
    let price = price.to_f64().unwrap_or(0.0).max(base);
    let position = (price / base).log10() / PRICE_DECADES * f64::from(PRICE_STEPS);
    position.round().clamp(0.0, f64::from(PRICE_STEPS)) as u32
}

/// "5 000 – 10 000 €", "nga 2015", "deri 150 000 km". Years and the like are not grouped.
fn range_label(min: Option<Decimal>, max: Option<Decimal>, unit: Option<&str>, grouped: bool) -> Option<String> {
    let number = |value: Decimal| if grouped { format::number(value) } else { plain(value) };
    let with_unit = |value: Decimal| match unit {
        Some(unit) => format!("{}\u{a0}{unit}", number(value)),
        None => number(value),
    };
    Some(match (min, max) {
        (Some(min), Some(max)) => format!("{} – {}", number(min), with_unit(max)),
        (Some(min), None) => format!("nga {}", with_unit(min)),
        (None, Some(max)) => format!("deri {}", with_unit(max)),
        (None, None) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_numbers_the_way_people_type_them() {
        assert_eq!(parse_number("5000"), Some(Decimal::from(5000)));
        assert_eq!(parse_number("5 000"), Some(Decimal::from(5000)));
        assert_eq!(parse_number("5.000 €"), Some(Decimal::from(5000)));
        assert_eq!(parse_number("12,5"), Some(Decimal::new(125, 1)));
        assert_eq!(parse_number("-1"), Some(Decimal::from(-1)));
        assert_eq!(parse_number("abc"), None);
        assert_eq!(parse_number("99999999999999999999"), None);
    }

    #[test]
    fn labels_ranges() {
        let n = |v: i64| Some(Decimal::from(v));
        assert_eq!(range_label(n(5000), n(10000), Some("€"), true).unwrap(), "5\u{a0}000 – 10\u{a0}000\u{a0}€");
        assert_eq!(range_label(n(2015), None, None, false).unwrap(), "nga 2015");
        assert_eq!(range_label(None, n(150000), Some("km"), true).unwrap(), "deri 150\u{a0}000\u{a0}km");
        assert_eq!(range_label(None, None, None, false), None);
    }

    #[test]
    fn drops_empty_parameters() {
        let params = Params::parse(Some("q=&city=durres&vehicle_fuel=DIESEL&vehicle_fuel=HYBRID&sort="));
        assert_eq!(params.first("q"), None);
        assert_eq!(params.first("city"), Some("durres"));
        assert_eq!(params.all("vehicle_fuel").collect::<Vec<_>>(), ["DIESEL", "HYBRID"]);
        assert_eq!(params.first("sort"), None);
    }
}
