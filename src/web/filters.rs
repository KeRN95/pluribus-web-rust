//! The filters of a browse page: read from the query string, turned into the listing query, and
//! shown back as the filter form, removable chips and the URL of the next page.
//!
//! Parameters: q, category (on /search and /harta; on a category page the path names it), city,
//! zone (one of the town's zones, Plazh in Durrës: in property only, whose listings can say one),
//! lat, lng and radius (a map pin, and how many km around it), type, condition (repeatable),
//! price_min, price_max, currency, sort and after, harta=1 (a category or search page with its
//! map's dots, which its map fetches when it first opens), numri=1 (only the count, as JSON:
//! browse.rs), plus one per filterable property of the category, named by the property's code:
//! option codes for SELECT and MULTISELECT properties (vehicle_fuel=DIESEL, repeatable),
//! <code>_min and <code>_max for NUMBER properties, and <code>=1 for BOOLEAN ones. Values that
//! don't exist are ignored, so an outdated or hand-edited URL still shows a page.
//!
//! Where to look is a place: a town (city), or a point on the map with a distance around it (lat,
//! lng, radius); one or the other, as the pills under the browse row offer them, and the town a
//! visitor picked stays theirs on the next pages (Params::remembering_town). In property, a zone
//! of the town narrows it further; it goes with its town.

use rust_decimal::{
    Decimal,
    prelude::{FromPrimitive, ToPrimitive},
};
use uuid::Uuid;

use super::{post::zoned, properties};
use crate::{
    db::{
        AttributeFilter, AttributeOption, Categories, Category, City, Cursor, FilterAttribute,
        ListingFilter, Near, PriceRange, Sort, TextSearch, Zone,
    },
    format,
    i18n::{self, Lang, t, t1},
};

/// The most characters of search words a page takes. Searches are a few words; each one widened
/// costs the database a lookup or two.
pub const SEARCH_CHARS: usize = 100;

const CONDITIONS: [&str; 4] = ["NEW", "LIKE_NEW", "USED", "FOR_PARTS"];
const CURRENCIES: [&str; 2] = ["EUR", "ALL"];

/// How far around the map pin to look, in whole kilometres, 1 to 50: in the map's form, what its
/// circle covers at the map's zoom (map_controller.js).
const MIN_RADIUS_KM: u32 = 1;
const MAX_RADIUS_KM: u32 = 50;
const DEFAULT_RADIUS_KM: u32 = 10;

/// An order's name, as its button, its menu and the results' heading say it.
fn sort_name(sort: Sort) -> &'static str {
    match sort {
        Sort::Relevance => t("Më të përshtatshmet"),
        Sort::Newest => t("Më të rejat"),
        Sort::PriceLow => t("Më të lirat"),
        Sort::PriceHigh => t("Më të shtrenjtat"),
        Sort::Nearest => t("Më afër"),
    }
}

/// The order as what it makes of the listings, for their heading on a category page ("Njoftimet
/// më të reja në Makina"; in English it comes first, "Newest listings in Cars", browse.html).
fn sort_adjective(sort: Sort) -> &'static str {
    match sort {
        Sort::Relevance => t("më të përshtatshme"),
        Sort::Newest => t("më të reja"),
        Sort::PriceLow => t("më të lira"),
        Sort::PriceHigh => t("më të shtrenjta"),
        Sort::Nearest => t("më afër"),
    }
}

/// The order a page opens in: nearest first with a map pin (`near`), the best match first with
/// words to search (`words`), the newest first otherwise.
fn default_sort(near: bool, words: bool) -> Sort {
    match (near, words) {
        (true, _) => Sort::Nearest,
        (false, true) => Sort::Relevance,
        (false, false) => Sort::Newest,
    }
}

/// The cookie that keeps the town a visitor picked last (place_controller.js), which a page whose
/// address names no place opens in (Params::remembering_town).
pub const TOWN_COOKIE: &str = "qyteti";

/// Properties the filters show at once wherever a category has them, besides those it requires or
/// shows on its cards (Field::primary): what buyers mostly narrow by. The rest wait under "Më shumë
/// filtra".
const PRIMARY: [&str; 10] = [
    "vehicle_body_type",
    "vehicle_transmission",
    "vehicle_fuel",
    "commercial_make",
    "moto_make",
    "machine_brand",
    "electronics_brand",
    "property_furnished",
    "job_contract",
    "job_place",
];

/// A category with at most this many filters shows them all; so does one that would leave at most
/// FEW_MORE under "Më shumë filtra".
const ALL_AT_ONCE: usize = 6;
const FEW_MORE: usize = 2;

/// The oldest year a year's dropdown offers: a vehicle's or a machine's, and a building's.
const FIRST_YEAR: i32 = 1970;
const FIRST_BUILT_YEAR: i32 = 1950;

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

    /// The search words (q), the first SEARCH_CHARS characters of them.
    pub fn search_words(&self) -> Option<String> {
        self.first("q").map(|q| q.chars().take(SEARCH_CHARS).collect())
    }

    /// The town a visitor picked last (TOWN_COOKIE), for an address that names no place: the page
    /// opens in it, its pill and chip on, and the links from it name it.
    pub fn remembering_town(mut self, town: Option<String>) -> Self {
        let placeless = ["city", "lat", "lng"].into_iter().all(|name| self.first(name).is_none());
        if let Some(town) = town.filter(|town| placeless && !town.is_empty()) {
            self.0.push(("city".to_string(), town));
        }
        self
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
    /// The town's chip: removing it forgets the town too (TOWN_COOKIE, application.js).
    pub forgets_town: bool,
}

/// A county's towns in the map card's menu (Filters::town_menu).
pub struct TownMenuGroup<'a> {
    pub label: &'a str,
    pub towns: Vec<TownLink<'a>>,
}

/// A town in the map card's menu: the same page in it, and whether the page is.
pub struct TownLink<'a> {
    pub town: &'a City,
    pub url: String,
    pub selected: bool,
}

/// One of the price's quick choices, under its slider: about a quarter of the category's listings.
pub struct PriceChoice {
    pub label: String,
    pub min: String,
    pub max: String,
    pub currency: &'static str,
    pub selected: bool,
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
    /// range, a few boxes (Marka beside Modeli, Viti beside Dogana, Karburanti beside Motorri).
    /// The others take a line of their own, as does a small one left without another beside it (a
    /// motorcycle's Marka, before the eight boxes of its Lloji; Filters::fields).
    pub half: bool,
    /// Its boxes have icons over the names, a car's side over each body type (Karroceria): they
    /// are bigger, and take a line of their own on every screen.
    pub icons: bool,
    /// Shown at once rather than under "Më shumë filtra": a property the category requires or
    /// shows on its cards, one of PRIMARY, or the parent or dependent of one (a make and its
    /// models). Filters::fields decides.
    pub primary: bool,
    /// Set to something: the visitor filters by it.
    pub active: bool,
    /// For a range, the values its two dropdowns offer (range_steps), each marked if chosen; for a
    /// year (FieldKind::Year), its dropdown's, in min_choices. Empty for a range that is typed.
    pub min_choices: Vec<Choice>,
    pub max_choices: Vec<Choice>,
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
    /// One of the town's zones, in property (Plazh in Durrës): listings there rather than anywhere
    /// in the town.
    pub zone: Option<&'a Zone>,
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

        // With a pin, nearest first unless another order is picked; with words, the best match
        // first; with neither, newest first. Each of the first two only where it can be.
        let words = params.search_words();
        let default_sort = default_sort(near.is_some(), words.is_some());
        let sort = params
            .first("sort")
            .and_then(Sort::from_param)
            .filter(|sort| *sort != Sort::Nearest || near.is_some())
            .filter(|sort| *sort != Sort::Relevance || words.is_some())
            .unwrap_or(default_sort);
        let after = params.first("after").and_then(|value| Cursor::decode(sort, value));
        // A town or a point, not both: a point in the address leaves out a town with it.
        let city = params.first("city").filter(|_| pin.is_none()).and_then(|slug| cities.iter().find(|c| c.slug == slug));
        // A zone of that town, where the category's listings can say one; its name alone could be
        // another town's (Qendër is Tirana's, Durrës's and Vlora's).
        let zone = city
            .filter(|_| category.is_some_and(|category| zoned(categories, category)))
            .and_then(|city| params.first("zone").and_then(|slug| city.zones.iter().find(|zone| zone.slug == slug)));

        Self {
            path,
            category_is_filter,
            categories,
            cities,
            q: params.search_words(),
            category,
            city,
            zone,
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

    /// The listing query for these filters, with `search`, the search for the words (q) that
    /// db::text_search made.
    pub fn listing_filter(&self, search: Option<TextSearch>) -> ListingFilter {
        let has_price = self.price_min.is_some() || self.price_max.is_some();
        ListingFilter {
            category_ids: self.category.map(|c| self.categories.subtree_ids(c.id)),
            location_id: self.zone.map(|zone| zone.id).or(self.city.map(|c| c.id)),
            near: self.near,
            search,
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
            // The page adds the accounts the visitor blocked (safety.rs).
            hidden_accounts: Vec::new(),
            since: None,
            matches: Default::default(),
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
        if let Some(zone) = self.zone {
            push("zone", &zone.slug);
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

    /// Nearest first with a map pin, the best match first with words, newest first otherwise.
    fn default_sort(&self) -> Sort {
        default_sort(self.near.is_some(), self.q.is_some())
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

    /// What a saved search keeps of this page (searches.rs): its path, and its filters as a query
    /// string in their fixed order (pairs), the town a cookie remembered among them, without the
    /// order, which its page sets when it is opened.
    pub fn search_address(&self) -> (String, String) {
        let pairs = self.pairs().into_iter().filter(|(name, _)| name != "sort");
        (self.path.clone(), form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish())
    }

    /// The name a saved search shows by: its words, its page's category and what each filter's
    /// chip says, "„golf“ · Makina · Tiranë · Naftë", or "Të gjitha njoftimet" for none. It is
    /// kept in the database (searches.rs), so it is in Albanian whatever the page's language: the
    /// chips as an Albanian page words them.
    pub fn search_name(&self) -> String {
        i18n::LANG.sync_scope(Lang::Sq, || {
            let mut parts = Vec::new();
            if let Some(q) = &self.q {
                parts.push(format!("„{q}“"));
            }
            if let (false, Some(category)) = (self.category_is_filter, self.category) {
                parts.push(category.name.clone());
            }
            parts.extend(self.chips().into_iter().map(|chip| chip.label));
            if parts.is_empty() {
                return "Të gjitha njoftimet".to_string();
            }
            let name = parts.join(" · ");
            match name.char_indices().nth(119) {
                Some((cut, _)) => format!("{}…", name[..cut].trim_end()),
                None => name,
            }
        })
    }

    /// Another category's page, keeping the search words, the city and the map pin; the zone too,
    /// where that category's listings can say one.
    pub fn category_url(&self, category: &Category) -> String {
        let zone_kept = zoned(self.categories, category);
        let kept = self.pairs().into_iter().filter(|(name, _)| {
            matches!(name.as_str(), "q" | "city" | "lat" | "lng" | "radius") || (name == "zone" && zone_kept)
        });
        url(&format!("/c/{}", category.slug), kept)
    }

    /// A category under this page's, which narrows it: every filter stays, as it has this
    /// category's properties too (Pjesë / aksesorë with "Audi, 2006" opens GPS / navigatorë
    /// with them). One it doesn't take is ignored there, and gone from its links.
    pub fn narrowed_url(&self, category: &Category) -> String {
        let kept = self.pairs().into_iter().filter(|(name, _)| name != "category");
        url(&format!("/c/{}", category.slug), kept)
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
    /// What the search box in the browse row carries along hidden: the page as it is, its filters,
    /// the map's point and the order, so that its words narrow it.
    pub fn search_form_fields(&self) -> Vec<(String, String)> {
        self.pairs().into_iter().filter(|(name, _)| !matches!(name.as_str(), "q" | "after")).collect()
    }

    /// Everything but the place: the map's point, which its own fields carry, replaces a town and
    /// its zone.
    pub fn map_form_fields(&self) -> Vec<(String, String)> {
        self.pairs()
            .into_iter()
            .filter(|(name, _)| !matches!(name.as_str(), "city" | "zone" | "lat" | "lng" | "radius"))
            .collect()
    }

    /// The same search everywhere, its map's point dropped and the rest kept: the map's "Të
    /// gjitha", and the point's chip.
    pub fn everywhere_url(&self) -> String {
        self.url_without(|name, _| matches!(name, "lat" | "lng" | "radius"))
    }

    /// The same search without its filters: the words, the order and the place stay, as the place
    /// has pills of its own; the zone with the category it is for, a category page's.
    pub fn clear_url(&self) -> String {
        self.url_without(|name, _| match name {
            "q" | "sort" | "city" | "lat" | "lng" | "radius" => false,
            "zone" => self.category_is_filter,
            _ => true,
        })
    }

    /// The filters on, each a chip that removes it: the place first (the town, or the map's point),
    /// then the rest.
    pub fn chips(&self) -> Vec<Chip> {
        let mut chips = self.place_chips();
        chips.extend(self.filter_chips());
        chips
    }

    /// The town's chip takes its zone with it; the zone's leaves the town.
    fn place_chips(&self) -> Vec<Chip> {
        let mut chips = Vec::new();
        if let Some(city) = self.city {
            let url = self.url_without(|name, _| name == "city" || name == "zone");
            chips.push(Chip { label: city.name.clone(), url, forgets_town: true });
        }
        if let Some(zone) = self.zone {
            chips.push(Chip { label: zone.name.clone(), url: self.town_url(), forgets_town: false });
        }
        if let Some(label) = self.point_label() {
            chips.push(Chip { label, url: self.everywhere_url(), forgets_town: false });
        }
        chips
    }

    /// The chips of what the filters' card sets, the place aside.
    fn filter_chips(&self) -> Vec<Chip> {
        let mut chips = Vec::new();
        let mut chip = |label: String, url: String| chips.push(Chip { label, url, forgets_town: false });
        if let (true, Some(category)) = (self.category_is_filter, self.category) {
            // The properties belong to the category, so they go with it, as does a property's zone.
            let names: Vec<String> = self.properties.iter().flat_map(Property::param_names).collect();
            chip(
                t(&category.name).to_string(),
                self.url_without(|name, _| matches!(name, "category" | "zone") || names.iter().any(|n| n == name)),
            );
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
                chip(format!("{}: {year}", t(FITS_YEAR_LABEL)), self.url_without(|name, _| name == param));
            }
            for option in &property.selected {
                chip(
                    t(&option.label).to_string(),
                    self.url_without(|name, value| name == code && value == option.code),
                );
            }
            let unit = attribute.unit.as_deref().map(t);
            if let Some(range) = range_label(property.min, property.max, unit, unit.is_some()) {
                let (min, max) = (format!("{code}_min"), format!("{code}_max"));
                chip(
                    format!("{}: {range}", t(&attribute.name)),
                    self.url_without(|name, _| name == min || name == max),
                );
            }
            if property.checked {
                chip(t(&attribute.name).to_string(), self.url_without(|name, _| name == code));
            }
        }
        chips
    }

    /// The map's point, as its chip says it, "Brenda 10 km nga pika"; not on the map page, whose
    /// map is the pin.
    fn point_label(&self) -> Option<String> {
        (self.near.is_some() && self.path != "/harta").then(|| t1("Brenda {} km nga pika", self.radius_km))
    }

    /// What the filters' button says to screen readers and as its tooltip, "Filtrat: Naftë · deri
    /// 5 000 €": the filters on, as their chips say them, but the place, which the pills and the
    /// map's button say; empty with none.
    pub fn summary(&self) -> String {
        self.filter_chips().into_iter().map(|chip| chip.label).collect::<Vec<_>>().join(" · ")
    }

    /// How many filters are on, the place aside: the number on the filters' button.
    pub fn filter_count(&self) -> usize {
        self.filter_chips().len()
    }

    // ------------------------------------------------------------ the place

    /// The towns of the map card's menu, by county, each the same page in it (place_url), the one
    /// the page is in marked; all of Albania, before them, is placeless_url.
    pub fn town_menu(&self) -> Vec<TownMenuGroup<'a>> {
        self.town_groups()
            .into_iter()
            .map(|group| TownMenuGroup {
                label: group.label,
                towns: group
                    .towns
                    .into_iter()
                    .map(|town| TownLink {
                        url: self.place_url(Some(town)),
                        selected: self.city.is_some_and(|city| city.id == town.id),
                        town,
                    })
                    .collect(),
            })
            .collect()
    }

    /// No place: all of Albania.
    pub fn everywhere(&self) -> bool {
        self.city.is_none() && !self.pin_in_url
    }

    /// The same page without a place: "Gjithë Shqipëria", first in the map card's menu.
    pub fn placeless_url(&self) -> String {
        self.place_url(None)
    }

    /// The same page in the whole town, without its zone: the zone's chip, and what a zone with
    /// nothing in it offers.
    pub fn town_url(&self) -> String {
        self.url_without(|name, _| name == "zone")
    }

    /// The same page in `town`, or without a place: the town, or no town, replaces the map's
    /// point, and a town replaces the town and its zone.
    fn place_url(&self, town: Option<&City>) -> String {
        let mut pairs: Vec<(String, String)> = self
            .pairs()
            .into_iter()
            .filter(|(name, _)| !matches!(name.as_str(), "city" | "zone" | "lat" | "lng" | "radius"))
            .collect();
        if let Some(town) = town {
            // Where the town goes in the fixed order: after the words and the category.
            let at = pairs.iter().position(|(name, _)| !matches!(name.as_str(), "q" | "category")).unwrap_or(pairs.len());
            pairs.insert(at, ("city".to_string(), town.slug.clone()));
        }
        url(&self.path, pairs)
    }

    /// The town picked, for the cookie that keeps it (place_controller.js); empty with none.
    pub fn town_slug(&self) -> &str {
        self.city.map_or("", |city| city.slug.as_str())
    }

    /// The towns by county, for the filters' Qyteti, the one picked selected.
    pub fn city_groups(&self) -> Vec<ChoiceGroup> {
        city_groups(self.cities, self.city.map(|city| city.slug.as_str()))
    }

    /// Whether the filters have Zona: in property, whose listings can say their zone.
    pub fn zoned(&self) -> bool {
        self.category.is_some_and(|category| zoned(self.categories, category))
    }

    /// Whether Zona shows: the town picked has zones (Tirana, Durrës, Vlorë).
    pub fn zone_shown(&self) -> bool {
        self.city.is_some_and(|city| !city.zones.is_empty())
    }

    /// Every town's zones, under their town, for Zona: those of the town picked show, the others'
    /// once Qyteti picks theirs (narrow_controller.js, filters_controller.js), the zone picked
    /// selected.
    pub fn zone_groups(&self) -> Vec<OptionGroup> {
        self.cities
            .iter()
            .filter(|city| !city.zones.is_empty())
            .map(|city| OptionGroup {
                label: city.name.clone(),
                parent: city.slug.clone(),
                hidden: self.city.is_none_or(|picked| picked.id != city.id),
                choices: city
                    .zones
                    .iter()
                    .map(|zone| Choice {
                        value: zone.slug.clone(),
                        label: zone.name.clone(),
                        selected: self.zone.is_some_and(|picked| picked.id == zone.id),
                    })
                    .collect(),
            })
            .collect()
    }

    /// Whether the map's point is on: its circle shows, and the results are those within it.
    pub fn has_pin(&self) -> bool {
        self.pin_in_url
    }

    /// The map's point, when it is on.
    pub fn pin(&self) -> Option<Near> {
        self.near.filter(|_| self.pin_in_url)
    }

    /// What the map's button says: the town, or the map's point as its nearest town and the
    /// distance ("Tiranë · 10 km"), or "Harta" with neither.
    pub fn map_label(&self) -> String {
        if let Some(city) = self.city {
            return city.name.clone();
        }
        match self.near.filter(|_| self.pin_in_url) {
            Some(near) => match self.nearest_town(near.latitude, near.longitude) {
                Some(town) => format!("{} · {} km", town.name, self.radius_km),
                None => format!("{} km", self.radius_km),
            },
            None => t("Harta").to_string(),
        }
    }

    /// The town whose point is nearest this one (a flat projection does at Albania's size).
    fn nearest_town(&self, latitude: f64, longitude: f64) -> Option<&'a City> {
        let shrink = latitude.to_radians().cos();
        let distance = |city: &City| ((city.longitude - longitude) * shrink).powi(2) + (city.latitude - latitude).powi(2);
        self.cities.iter().min_by(|one, other| distance(one).total_cmp(&distance(other)))
    }

    /// Where the map starts: on its point, else on the town picked; None, on Tirana (map_controller.js).
    pub fn map_start(&self) -> Option<(f64, f64)> {
        match (self.near, self.city) {
            (Some(near), _) => Some((near.latitude, near.longitude)),
            (None, Some(city)) => Some((city.latitude, city.longitude)),
            (None, None) => None,
        }
    }

    /// With the map's point and less than the widest distance, the same search further out (25
    /// km, or 50 from 25), and how far: what a page without results offers.
    pub fn wider_url(&self) -> Option<(String, u32)> {
        let wider = match self.radius_km {
            radius if radius < 25 => 25,
            radius if radius < MAX_RADIUS_KM => MAX_RADIUS_KM,
            _ => return None,
        };
        self.pin_in_url.then(|| {
            let pairs = self.pairs().into_iter().map(|(name, value)| match name.as_str() {
                "radius" => (name, wider.to_string()),
                _ => (name, value),
            });
            (url(&self.path, pairs), wider)
        })
    }

    // ------------------------------------------------------------ the form

    /// What the filter form carries along hidden, having no fields for it: the search words (the
    /// row's search has them), the map pin and its radius (the map's). Its Qyteti is the town.
    pub fn kept_fields(&self) -> Vec<(String, String)> {
        self.pairs()
            .into_iter()
            .filter(|(name, _)| matches!(name.as_str(), "q" | "lat" | "lng" | "radius"))
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

    /// Whether the kind of listing comes first among the filters: in Prona, where buying and
    /// renting are two searches. Elsewhere nearly everything is for sale, and it waits under "Më
    /// shumë filtra", or after the rest where nothing else does.
    pub fn type_first(&self) -> bool {
        self.category.is_some_and(|category| {
            category.slug == "prona" || self.categories.ancestors(category).iter().any(|above| above.slug == "prona")
        })
    }

    /// Whether the kind of listing is chosen, which opens "Më shumë filtra" where it waits.
    pub fn type_chosen(&self) -> bool {
        self.listing_type.is_some()
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
            .unwrap_or_else(|| t("Çdo çmim").to_string())
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
            None => t("Pa kufi").to_string(),
        }
    }

    /// The price's quick choices: four stretches between the round prices nearest the category's
    /// `quarters` (db::price_quarters), each holding about a quarter of its listings: "deri 3 000
    /// €", "3 000 – 6 000 €", "6 000 – 12 000 €", "nga 12 000 €", in its usual currency. None
    /// where rounding makes two of them one.
    pub fn price_choices(&self, quarters: [Decimal; 3]) -> Vec<PriceChoice> {
        let Some(category) = self.category else { return Vec::new() };
        let currency = CURRENCIES.into_iter().find(|code| *code == category.default_currency).unwrap_or("ALL");
        let [low, middle, high] = quarters.map(round_price);
        if !(low < middle && middle < high) {
            return Vec::new();
        }
        let symbol = format::currency_symbol(currency);
        [(None, Some(low)), (Some(low), Some(middle)), (Some(middle), Some(high)), (Some(high), None)]
            .into_iter()
            .map(|(min, max)| PriceChoice {
                label: range_label(min, max, Some(symbol), true).unwrap_or_default(),
                min: min.map(plain).unwrap_or_default(),
                max: max.map(plain).unwrap_or_default(),
                currency,
                selected: self.price_min == min && self.price_max == max && self.currency == currency,
            })
            .collect()
    }

    /// The default order's value is empty, so it stays out of URLs. Nearest first is only on
    /// offer with a map pin, the best match first with words.
    pub fn sort_choices(&self) -> Vec<Choice> {
        let default = self.default_sort();
        Sort::ALL
            .into_iter()
            .filter(|sort| *sort != Sort::Nearest || self.near.is_some())
            .filter(|sort| *sort != Sort::Relevance || self.q.is_some())
            .map(|sort| Choice {
                value: if sort == default { String::new() } else { sort.param().to_string() },
                label: sort_name(sort).to_string(),
                selected: sort == self.sort,
            })
            .collect()
    }

    /// The order the listings come in, by name: what the order's button says, and the results'
    /// heading.
    pub fn sort_label(&self) -> &'static str {
        sort_name(self.sort)
    }

    /// The order as what it makes of the listings, for their heading on a category page
    /// ("Njoftimet më të reja në Makina", browse.rs).
    pub fn sort_adjective(&self) -> &'static str {
        sort_adjective(self.sort)
    }

    /// The category's filters, those shown at once first (Field::primary), each in its order: a
    /// property the category requires or shows on its cards, one of PRIMARY, or the parent or
    /// dependent of one, a car's make with its models. All at once in a category with few, or
    /// where few would wait under "Më shumë filtra".
    pub fn fields(&self) -> Vec<Field> {
        let mut fields: Vec<Field> = self.properties.iter().map(|property| property.field(&self.properties)).collect();
        for (field, property) in fields.iter_mut().zip(&self.properties) {
            let attribute = property.attribute;
            field.primary = attribute.required || attribute.show_in_card || PRIMARY.contains(&attribute.code.as_str());
        }
        for (i, property) in self.properties.iter().enumerate() {
            let parent = property.attribute.parent_attribute_id.and_then(|id| self.properties.iter().position(|p| p.attribute.id == id));
            if let Some(parent) = parent {
                let either = fields[i].primary || fields[parent].primary;
                fields[i].primary = either;
                fields[parent].primary = either;
            }
        }
        let waiting = fields.iter().filter(|field| !field.primary).count();
        if fields.len() <= ALL_AT_ONCE || waiting <= FEW_MORE {
            for field in &mut fields {
                field.primary = true;
            }
        }
        fields.sort_by_key(|field| !field.primary);
        // Small fields stand two to a line, among those of their kind: one left over, alone, takes
        // the whole line rather than half of it beside nothing.
        for run in fields.chunk_by_mut(|one, next| one.half == next.half && one.primary == next.primary) {
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

    /// Its field in the form, its name, unit and options in the page's language.
    fn field(&self, all: &[Property]) -> Field {
        let attribute = self.attribute;
        let choice = |option: &AttributeOption| Choice {
            value: option.code.clone(),
            label: t(&option.label).to_string(),
            selected: self.selected.iter().any(|s| s.id == option.id),
        };
        let mut field = Field {
            name: attribute.code.clone(),
            label: t(&attribute.name).to_string(),
            kind: FieldKind::Select,
            choices: Vec::new(),
            groups: Vec::new(),
            depends_on: None,
            min: self.min.map(plain).unwrap_or_default(),
            max: self.max.map(plain).unwrap_or_default(),
            unit: attribute.unit.as_deref().map(t).unwrap_or_default().to_string(),
            checked: self.checked,
            half: true,
            icons: false,
            primary: true,
            active: !self.selected.is_empty() || self.min.is_some() || self.max.is_some() || self.checked || self.year.is_some(),
            min_choices: Vec::new(),
            max_choices: Vec::new(),
        };
        if self.fits_to.is_some() {
            field.name = year_param(attribute);
            field.label = t(FITS_YEAR_LABEL).to_string();
            field.kind = FieldKind::Year;
            field.min = self.year.map(plain).unwrap_or_default();
            if let Some(steps) = range_steps(&attribute.code, attribute.unit.as_deref()) {
                field.min_choices = step_choices(&steps, self.year);
            }
            return field;
        }
        match attribute.data_type.as_str() {
            "NUMBER" => {
                field.kind = FieldKind::Range;
                // Picked from dropdowns where the unit has steps, two of them a line's width.
                if let Some(steps) = range_steps(&attribute.code, attribute.unit.as_deref()) {
                    field.min_choices = step_choices(&steps, self.min);
                    field.max_choices = step_choices(&steps, self.max);
                    field.half = false;
                }
            }
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
                                label: t(&parent_option.label).to_string(),
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

/// The values a range's dropdowns offer, in the order they are offered, and whether they are years
/// (shown as they are, the newest first) rather than amounts (grouped, the smallest first).
struct Steps {
    values: Vec<i64>,
    years: bool,
}

/// A range's dropdown values by its unit, or a year's: a car's kilometres, a flat's square
/// metres, an engine's cm³, a screen's inches; years from the next back to FIRST_YEAR
/// (FIRST_BUILT_YEAR for a building). None for a range that is typed: a floor, a count, hours.
fn range_steps(code: &str, unit: Option<&str>) -> Option<Steps> {
    if code.ends_with("year") || code.ends_with("year_built") || code.ends_with("from_year") {
        let first = if code == "property_year_built" { FIRST_BUILT_YEAR } else { FIRST_YEAR };
        let next = time::OffsetDateTime::now_utc().year() + 1;
        return Some(Steps { values: (first..=next).rev().map(i64::from).collect(), years: true });
    }
    let values: &[i64] = match (code, unit) {
        ("rim_size", _) => &[13, 14, 15, 16, 17, 18, 19, 20, 21, 22],
        (_, Some("km")) => &[
            5_000, 10_000, 20_000, 30_000, 40_000, 50_000, 75_000, 100_000, 125_000, 150_000, 175_000, 200_000, 250_000,
            300_000,
        ],
        (_, Some("m²")) => &[20, 30, 40, 50, 60, 70, 80, 90, 100, 120, 150, 200, 250, 300, 500, 1_000, 2_000, 5_000],
        (_, Some("cm³")) => &[50, 125, 250, 500, 750, 1_000, 1_200, 1_400, 1_600, 1_800, 2_000, 2_500, 3_000, 4_000, 5_000],
        (_, Some("HP")) => &[50, 75, 100, 125, 150, 175, 200, 250, 300, 400, 500],
        (_, Some("inç")) => &[5, 6, 7, 8, 10, 11, 13, 14, 15, 16, 17, 24, 27, 32, 43, 50, 55, 65, 75, 85],
        _ => return None,
    };
    Some(Steps { values: values.to_vec(), years: false })
}

/// A range's dropdown, `current` marked: offered in its place among the steps where it isn't one
/// of them (an older link's).
fn step_choices(steps: &Steps, current: Option<Decimal>) -> Vec<Choice> {
    let mut values: Vec<Decimal> = steps.values.iter().map(|value| Decimal::from(*value)).collect();
    if let Some(current) = current.filter(|current| !values.contains(current)) {
        let at = values
            .iter()
            .position(|value| if steps.years { *value < current } else { *value > current })
            .unwrap_or(values.len());
        values.insert(at, current);
    }
    values
        .into_iter()
        .map(|value| Choice {
            value: plain(value),
            label: if steps.years { plain(value) } else { format::number(value) },
            selected: current == Some(value),
        })
        .collect()
}

/// A price as a round one near it, for the price's quick choices: two figures from 10, 12, 15, 20,
/// 25, 30, 40, 50, 60, 70, 80, the nearest by ratio (2 870 € is 3 000 €, 450 € is 500 €).
fn round_price(price: Decimal) -> Decimal {
    const ROUND: [f64; 12] = [10.0, 12.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 100.0];
    let value = price.to_f64().unwrap_or(1.0).max(1.0);
    let scale = 10f64.powf(value.log10().floor() - 1.0);
    let off = |round: &f64| (value / scale / round).ln().abs();
    let round = ROUND.iter().min_by(|one, other| off(one).total_cmp(&off(other))).copied().unwrap_or(10.0);
    Decimal::from_f64((round * scale).round()).unwrap_or(price).normalize()
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

/// Every category, for a dropdown: subcategories follow their parent, indented; each name in the
/// page's language.
pub fn category_choices(categories: &Categories, selected: Option<&str>) -> Vec<Choice> {
    fn add(categories: &Categories, category: &Category, depth: usize, selected: Option<&str>, out: &mut Vec<Choice>) {
        out.push(Choice {
            value: category.slug.clone(),
            label: format!("{}{}", "\u{a0}\u{a0}\u{a0}".repeat(depth), t(&category.name)),
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

pub fn url(path: &str, pairs: impl IntoIterator<Item = (String, String)>) -> String {
    let query = form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish();
    if query.is_empty() { path.to_string() } else { format!("{path}?{query}") }
}

/// A number as people type it, in Albanian's way or English's: "5000", "5 000"; "5.000" or
/// "5,000", a mark before the last three digits (or one of several alike: "1.000.000") grouping
/// thousands; "12,5" or "12.5", one before one, two or four digits (or after a lone "0") marking
/// decimals, as the later of a dot and a comma does ("1.234,56", "1,234.56"). The URLs' own
/// numbers (`plain`) read back as they were written.
pub fn parse_number(text: &str) -> Option<Decimal> {
    let kept: String = text.chars().filter(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-')).collect();
    let decimal = match (kept.rfind('.'), kept.rfind(',')) {
        (Some(dot), Some(comma)) => Some(dot.max(comma)),
        (Some(at), None) | (None, Some(at)) => {
            let mark = kept.as_bytes()[at];
            let once = kept.bytes().filter(|byte| *byte == mark).count() == 1;
            let whole = kept[..at].trim_start_matches('-');
            let thousands = kept.len() - at - 1 == 3 && !whole.is_empty() && whole != "0";
            (once && !thousands).then_some(at)
        }
        (None, None) => None,
    };
    let number: String = kept
        .char_indices()
        .filter_map(|(at, c)| match c {
            '.' | ',' if Some(at) == decimal => Some('.'),
            '.' | ',' => None,
            c => Some(c),
        })
        .collect();
    let value: Decimal = number.parse().ok()?;
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
        (Some(min), None) => t1("nga {}", with_unit(min)),
        (None, Some(max)) => t1("deri {}", with_unit(max)),
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
        // English's way too, and a URL's own: a point marks decimals, a comma groups thousands.
        assert_eq!(parse_number("12.5"), Some(Decimal::new(125, 1)));
        assert_eq!(parse_number("9,500"), Some(Decimal::from(9500)));
        assert_eq!(parse_number("1.000.000"), Some(Decimal::from(1_000_000)));
        assert_eq!(parse_number("1,000,000"), Some(Decimal::from(1_000_000)));
        assert_eq!(parse_number("1.234,56"), Some(Decimal::new(123_456, 2)));
        assert_eq!(parse_number("1,234.56"), Some(Decimal::new(123_456, 2)));
        assert_eq!(parse_number("0,125"), Some(Decimal::new(125, 3)));
        assert_eq!(parse_number("6.1 inç"), Some(Decimal::new(61, 1)));
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
    fn rounds_prices_for_the_quick_choices() {
        let round = |price: i64| round_price(Decimal::from(price)).to_string();
        assert_eq!(round(2870), "3000");
        assert_eq!(round(450), "500");
        assert_eq!(round(11_400), "12000");
        assert_eq!(round(64_000), "60000");
        assert_eq!(round(9), "10");
    }

    #[test]
    fn ranges_offer_steps_by_their_unit() {
        let km = range_steps("vehicle_mileage", Some("km")).unwrap();
        assert!(!km.years && km.values.contains(&150_000));
        let years = range_steps("vehicle_year", None).unwrap();
        assert!(years.years && years.values.first() > years.values.last(), "the newest first");
        assert!(range_steps("property_floor", None).is_none(), "a floor is typed");
        // A value that isn't a step is offered in its place, chosen.
        let choices = step_choices(&km, Some(Decimal::from(123_456)));
        let at = choices.iter().position(|choice| choice.selected).unwrap();
        assert_eq!((choices[at - 1].value.as_str(), choices[at].value.as_str()), ("100000", "123456"));
        assert_eq!(choices[at].label, "123\u{a0}456");
    }

    #[test]
    fn a_remembered_town_counts_where_no_place_is_named() {
        let town = || Some("durres".to_string());
        assert_eq!(Params::parse(Some("vehicle_fuel=DIESEL")).remembering_town(town()).first("city"), Some("durres"));
        assert_eq!(Params::parse(Some("city=vlore")).remembering_town(town()).first("city"), Some("vlore"));
        assert_eq!(Params::parse(Some("lat=41.3&lng=19.8")).remembering_town(town()).first("city"), None);
        assert_eq!(Params::parse(None).remembering_town(Some(String::new())).first("city"), None);
    }

    #[test]
    fn drops_empty_parameters() {
        let params = Params::parse(Some("q=&city=durres&vehicle_fuel=DIESEL&vehicle_fuel=HYBRID&sort="));
        assert_eq!(params.first("q"), None);
        assert_eq!(params.first("city"), Some("durres"));
        assert_eq!(params.all("vehicle_fuel").collect::<Vec<_>>(), ["DIESEL", "HYBRID"]);
        assert_eq!(params.first("sort"), None);
    }

    #[test]
    fn takes_the_first_hundred_characters_of_the_search_words() {
        let params = Params::parse(Some(&format!("q={}", "golf ".repeat(30))));
        assert_eq!(params.search_words().map(|words| words.chars().count()), Some(SEARCH_CHARS));
        assert_eq!(Params::parse(Some("q=shtëpi")).search_words().as_deref(), Some("shtëpi"));
        assert_eq!(Params::parse(Some("q=")).search_words(), None);
    }
}
