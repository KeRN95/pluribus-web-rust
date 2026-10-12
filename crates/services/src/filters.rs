//! A browse query's filters: read from the query string, checked against the categories, towns and
//! properties that exist, and turned into the listing query (`ParsedFilters::listing_filter`). The
//! pages read them so (pluribus-web's filters.rs, which shows them back as the filter form, the
//! chips that remove them and the next page's address) and so does the JSON API's `/listings`
//! (pluribus-api's browse.rs), with one meaning: what a query asks for is the same listings
//! wherever it is asked. Saved searches keep their queries in this vocabulary (pluribus-web's
//! searches.rs), and the phone app sends it, so its names and what they mean stay as they are. The
//! filters read are written back as a query string's pairs in one fixed order
//! (`ParsedFilters::query_pairs`), which the pages' links and a saved search's address
//! (searches.rs) are made of.
//!
//! Parameters: q, category (on /search and /harta, and the API's /listings; on a category page the
//! path names it), city, zone (one of the town's zones, Plazh in Durrës: in property only, whose
//! listings can say one), lat, lng and radius (a map pin, and how many km around it), type,
//! condition (repeatable), price_min, price_max, currency, sort and after, plus one per filterable
//! property of the category, named by the property's code: option codes for SELECT and
//! MULTISELECT properties (vehicle_fuel=DIESEL, repeatable), <code>_min and <code>_max for NUMBER
//! properties, <code>=1 for BOOLEAN ones, and for a pair of years (a part's <x>_from_year and
//! <x>_to_year) the buyer's vehicle's year, <x>_year. Values that don't exist are ignored, so an
//! outdated or hand-edited query still reads. The pages' own parameters (harta, numri) are theirs,
//! and the API's (sipas, count) its own.
//!
//! Where to look is a place: a town (city), or a point on the map with a distance around it (lat,
//! lng, radius); one or the other. In property, a zone of the town narrows it further; it goes
//! with its town. What the caller knows of the visitor besides the query it gives the parser: the
//! town they picked last, which a query naming no place opens in (`Params::remembering_town`; the
//! pages keep it in a cookie), and where to look without a pin in the query (`default_pin`: the
//! pages' map page's Tirana). The parser knows nothing of cookies or pages.

use pluribus_core::domain::{Condition, listing::zoned};
use pluribus_db::{
    AttributeFilter, AttributeOption, Categories, Category, City, Cursor, FilterAttribute, ListingFilter, Near,
    PriceRange, Sort, TextSearch, Zone,
};
use rust_decimal::Decimal;
use uuid::Uuid;

/// The most characters of search words a query takes. Searches are a few words; each one widened
/// costs the database a lookup or two.
pub const SEARCH_CHARS: usize = 100;

/// The conditions a query can ask for, as it names them.
pub const CONDITIONS: [&str; 4] = Condition::NAMES;

/// The currencies a price range is in: euros, or lekë, the listings' prices compared at the rate.
pub const CURRENCIES: [&str; 2] = ["EUR", "ALL"];

/// How far around the map pin to look, in whole kilometres, 1 to 50: in the map's form, what its
/// circle covers at the map's zoom (map_controller.js).
pub const MIN_RADIUS_KM: u32 = 1;
pub const MAX_RADIUS_KM: u32 = 50;
pub const DEFAULT_RADIUS_KM: u32 = 10;

/// The order a query opens in: nearest first with a map pin (`near`), the best match first with
/// words to search (`words`), the newest first otherwise.
pub fn default_sort(near: bool, words: bool) -> Sort {
    match (near, words) {
        (true, _) => Sort::Nearest,
        (false, true) => Sort::Relevance,
        (false, false) => Sort::Newest,
    }
}

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

    /// Every value of a parameter given more than once (condition, a MULTISELECT's options), in
    /// their order.
    pub fn all<'p>(&'p self, name: &'p str) -> impl Iterator<Item = &'p str> {
        self.0.iter().filter(move |(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// The search words (q), the first SEARCH_CHARS characters of them.
    pub fn search_words(&self) -> Option<String> {
        self.first("q").map(|q| q.chars().take(SEARCH_CHARS).collect())
    }

    /// The town the caller remembers for the visitor, the one they picked last, for a query that
    /// names no place: it reads as if the query named it (the pages: their town cookie,
    /// pluribus-web's TOWN_COOKIE, the page opening in it, its pill and chip on, and the links
    /// from it naming it).
    pub fn remembering_town(mut self, town: Option<String>) -> Self {
        let placeless = ["city", "lat", "lng"].into_iter().all(|name| self.first(name).is_none());
        if let Some(town) = town.filter(|town| placeless && !town.is_empty()) {
            self.0.push(("city".to_string(), town));
        }
        self
    }
}

/// A query's filters, checked against the categories, towns and properties that exist: what the
/// listing query is made of (`listing_filter`), and what a page shows back (pluribus-web's
/// `Filters`, whose fields these read as).
pub struct ParsedFilters<'a> {
    /// The categories and the towns the query was read against.
    pub categories: &'a Categories,
    pub cities: &'a [City],
    pub q: Option<String>,
    pub category: Option<&'a Category>,
    pub city: Option<&'a City>,
    /// One of the town's zones, in property (Plazh in Durrës): listings there rather than anywhere
    /// in the town.
    pub zone: Option<&'a Zone>,
    /// The map pin, with its radius: the one in the query, or without one there, `default_pin`.
    pub near: Option<Near>,
    /// Whether the pin came in the query (lat and lng). A default pin (the map page's Tirana) isn't
    /// written into the pages' links.
    pub pin_in_url: bool,
    /// The radius picked for the map, in kilometres; it stays picked without a pin.
    pub radius_km: u32,
    pub listing_type: Option<String>,
    pub conditions: Vec<&'static str>,
    pub price_min: Option<Decimal>,
    pub price_max: Option<Decimal>,
    pub currency: &'static str,
    /// The category's filterable properties, each with what the query chose for it, but the
    /// closing year of a pair, which filters with its opening one.
    pub properties: Vec<Property<'a>>,
    pub sort: Sort,
    pub after: Option<Cursor>,
}

/// One filterable property and what the query chose for it.
pub struct Property<'a> {
    pub attribute: &'a FilterAttribute,
    pub selected: Vec<&'a AttributeOption>,
    pub min: Option<Decimal>,
    pub max: Option<Decimal>,
    pub checked: bool,
    /// For "<x>_from_year", the "<x>_to_year" it makes a pair with: the two filter as one year.
    pub fits_to: Option<&'a FilterAttribute>,
    pub year: Option<Decimal>,
}

impl<'a> ParsedFilters<'a> {
    /// `category` is the query's category (a category page's, or the one the category parameter
    /// names), `attributes` its filterable properties; `default_pin`, where to look around without
    /// a pin in the query (the pages' map page's, Tirana; none elsewhere).
    pub fn new(
        params: &Params,
        category: Option<&'a Category>,
        categories: &'a Categories,
        cities: &'a [City],
        attributes: &'a [FilterAttribute],
        default_pin: Option<(f64, f64)>,
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
            .or(default_pin)
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
        // A town or a point, not both: a point in the query leaves out a town with it.
        let city = params.first("city").filter(|_| pin.is_none()).and_then(|slug| cities.iter().find(|c| c.slug == slug));
        // A zone of that town, where the category's listings can say one; its name alone could be
        // another town's (Qendër is Tirana's, Durrës's and Vlora's).
        let zone = city
            .filter(|_| category.is_some_and(|category| zoned(categories, category)))
            .and_then(|city| params.first("zone").and_then(|slug| city.zones.iter().find(|zone| zone.slug == slug)));

        Self {
            categories,
            cities,
            q: params.search_words(),
            category,
            city,
            zone,
            near,
            pin_in_url: pin.is_some(),
            radius_km,
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
            // The caller adds the accounts the visitor blocked (db::blocked_for_session).
            hidden_accounts: Vec::new(),
            exclude: Vec::new(),
            since: None,
            matches: Default::default(),
        }
    }

    /// The order the query opens in, which its address leaves out: nearest first with a map pin,
    /// the best match first with words, newest first otherwise.
    pub fn default_sort(&self) -> Sort {
        default_sort(self.near.is_some(), self.q.is_some())
    }

    /// The filters as a query string's pairs, in the pages' fixed order, without the order and a
    /// page's cursor: the words, the category where it is one of the filters (`category_is_filter`:
    /// on /search, not on a category's page, whose path names it), the town and its zone, or the
    /// pin and its radius (a default pin isn't written down, but a distance picked for it is), the
    /// kind of listing, the conditions, the price and its currency, then each property's (a part's
    /// vehicle's year, the options chosen, a number's bounds, a box ticked); nothing that doesn't
    /// exist or isn't on, so that the same filters asked for in another order write the same
    /// pairs. What the pages' links are made of (pluribus-web's `Filters`, which adds the order
    /// where it isn't the one they open in; until C12 written there), and a saved search's address
    /// (searches.rs, `address`).
    pub fn query_pairs(&self, category_is_filter: bool) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let mut push = |name: &str, value: &str| pairs.push((name.to_string(), value.to_string()));
        if let Some(q) = &self.q {
            push("q", q);
        }
        if let (true, Some(category)) = (category_is_filter, self.category) {
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
        pairs
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

    /// The query's names for it: its code, "<code>_min" and "<code>_max"; a pair of years' one
    /// year, "<x>_year" (year_param).
    pub fn param_names(&self) -> Vec<String> {
        let code = &self.attribute.code;
        if self.fits_to.is_some() {
            return vec![year_param(self.attribute)];
        }
        vec![code.clone(), format!("{code}_min"), format!("{code}_max")]
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
pub fn year_param(opening: &FilterAttribute) -> String {
    format!("{}_year", opening.code.strip_suffix("_from_year").unwrap_or(&opening.code))
}

/// A year a vehicle could have been made in; anything else is ignored.
fn parse_year(value: &str) -> Option<Decimal> {
    let year: i32 = value.trim().parse().ok()?;
    (1900..=2100).contains(&year).then(|| Decimal::from(year))
}

/// A number as people type it, in Albanian's way or English's: "5000", "5 000"; "5.000" or
/// "5,000", a mark before the last three digits (or one of several alike: "1.000.000") grouping
/// thousands; "12,5" or "12.5", one before one, two or four digits (or after a lone "0") marking
/// decimals, as the later of a dot and a comma does ("1.234,56", "1,234.56"). The queries' own
/// numbers (`plain`) read back as they were written. The filters read their numbers so, and the
/// pages' posting form, its properties and the AI's answers too (pluribus-web).
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

/// A number as a query writes it, and a form's field: "5000", "12.5".
pub fn plain(value: Decimal) -> String {
    value.normalize().to_string()
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

    #[test]
    fn a_query_opens_nearest_first_with_a_pin_best_first_with_words_else_newest() {
        assert_eq!(default_sort(true, true), Sort::Nearest);
        assert_eq!(default_sort(false, true), Sort::Relevance);
        assert_eq!(default_sort(false, false), Sort::Newest);
    }
}
