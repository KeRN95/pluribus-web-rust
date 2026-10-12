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
//!
//! And how the filters show each property, which the pages' filter sheet and their row of
//! dropdowns go by, and the API's fields with them (pluribus-api's reference.rs), pluribus-web's
//! filters.rs's until C13: its kind (`FieldKind`, `Property::kind`: a dropdown, boxes, a range, a
//! box to tick, a year), the options it offers (`Property::offered`) and whether they are drawn
//! (`option_icon`, `Property::pictured`), which properties show at once and in what order
//! (`ParsedFilters::shown_at_once`, `sheet_order`), and the row of dropdowns after Filtrat
//! (`ParsedFilters::row`, `RowFilter`).

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

    /// Which of its properties the filters show at once rather than under "Më shumë filtra", in
    /// their order (`properties`): one the category requires or shows on its cards, one of PRIMARY,
    /// or the parent or dependent of one (a car's make with its models); every one in a category
    /// with few (ALL_AT_ONCE), or where few would wait (FEW_MORE). The pages' sheet (pluribus-web's
    /// `Field::primary`), and the order of the row's (`row`).
    pub fn shown_at_once(&self) -> Vec<bool> {
        let mut shown: Vec<bool> = self
            .properties
            .iter()
            .map(|property| {
                let attribute = property.attribute;
                attribute.required || attribute.show_in_card || PRIMARY.contains(&attribute.code.as_str())
            })
            .collect();
        for (at, property) in self.properties.iter().enumerate() {
            let parent = property.attribute.parent_attribute_id.and_then(|id| self.properties.iter().position(|p| p.attribute.id == id));
            if let Some(parent) = parent {
                let either = shown[at] || shown[parent];
                shown[at] = either;
                shown[parent] = either;
            }
        }
        let waiting = shown.iter().filter(|shown| !**shown).count();
        if shown.len() <= ALL_AT_ONCE || waiting <= FEW_MORE {
            shown.fill(true);
        }
        shown
    }

    /// Its properties in the order the filters' sheet lists them, as places in `properties`: those
    /// shown at once first (`shown_at_once`), each lot in the category's order.
    pub fn sheet_order(&self) -> Vec<usize> {
        let shown = self.shown_at_once();
        let mut order: Vec<usize> = (0..self.properties.len()).collect();
        order.sort_by_key(|at| !shown[*at]);
        order
    }

    /// Whether the kind of listing comes first among the filters: in Prona, where buying and
    /// renting are two searches. Elsewhere nearly everything is for sale, and it waits under "Më
    /// shumë filtra", or after the rest where nothing else does.
    pub fn type_first(&self) -> bool {
        self.category.is_some_and(|category| {
            category.slug == "prona" || self.categories.ancestors(category).iter().any(|above| above.slug == "prona")
        })
    }

    /// Whether the kind of listing is one of the filters: where the category takes more than one.
    pub fn types_offered(&self) -> bool {
        self.category.is_some_and(|category| category.listing_types.len() > 1)
    }

    /// Whether the condition is one of the filters: across every category, and in one whose
    /// listings say it, not property's or jobs'.
    pub fn conditions_offered(&self) -> bool {
        self.category.is_none_or(|category| category.uses_condition)
    }

    /// The results page's row of filter dropdowns after Filtrat, as many as the row has room for
    /// beside the chips of the `chips` filters on (ROW_FILTERS in all), of those buyers narrow by
    /// most that aren't on yet: on a search across every category the town, where none is set; in
    /// Prona the kind of listing; then the category's own filters in the sheet's order
    /// (`sheet_order`), its dropdowns, its boxes and its years, not a car's body types (their
    /// pictures want the sheet's room), nor a range in a unit (kilometres, square metres), nor a box
    /// to tick; then the condition, and elsewhere the kind of listing, where the category offers
    /// them. The price stands fourth, or before the last of fewer (after a lone one), as the canvas
    /// has them: "Marka, Modeli, Viti, Çmimi, Karburanti"; "Gjendja, Çmimi, Lloji i njoftimit";
    /// "Qyteti, Çmimi, Gjendja". The pages' row (pluribus-web's `Filters::drops`), and the API's
    /// fields' places in it with nothing on (pluribus-api's `FieldDto::row`).
    pub fn row(&self, chips: usize) -> Vec<RowFilter> {
        let mut offered = Vec::new();
        if self.category.is_none() && self.city.is_none() && self.near.is_none() {
            offered.push(RowFilter::City);
        }
        let type_open = self.listing_type.is_none() && self.types_offered();
        if type_open && self.type_first() {
            offered.push(RowFilter::Type);
        }
        for at in self.sheet_order() {
            let property = &self.properties[at];
            let fits = match property.kind(&self.properties) {
                FieldKind::Select => !property.offered(&self.properties).is_empty(),
                FieldKind::Checkboxes => !property.pictured(&self.properties) && !property.offered(&self.properties).is_empty(),
                FieldKind::Range => property.attribute.unit.as_deref().is_none_or(str::is_empty),
                FieldKind::Year => true,
                FieldKind::Toggle => false,
            };
            if fits && !property.active() {
                offered.push(RowFilter::Property(at));
            }
        }
        if self.conditions.is_empty() && self.conditions_offered() {
            offered.push(RowFilter::Condition);
        }
        if type_open && !self.type_first() {
            offered.push(RowFilter::Type);
        }
        let room = ROW_FILTERS.saturating_sub(chips);
        let price = self.price_min.is_none() && self.price_max.is_none() && room > 0;
        offered.truncate(if price { room - 1 } else { room });
        if price {
            let at = if offered.len() < 2 { offered.len() } else { (offered.len() - 1).min(3) };
            offered.insert(at, RowFilter::Price);
        }
        offered
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

// ------------------------------------------------------------ how the filters show them

/// How the filters show a property (`Property::kind`): a dropdown of its options, boxes to tick
/// among them, a range (a number's from and to), a box to tick for a yes, or one year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Select,
    Checkboxes,
    Range,
    Toggle,
    /// One year, the buyer's vehicle's, for a part's pair of years ("<x>_year", `year_param`).
    Year,
}

/// Properties with at most this many options are offered as boxes, the colours too, longer lists
/// as a dropdown.
const CHECKBOX_LIMIT: usize = 8;

/// Properties the filters show at once wherever a category has them, besides those it requires or
/// shows on its cards (`ParsedFilters::shown_at_once`): what buyers mostly narrow by. The rest wait
/// under "Më shumë filtra".
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

/// How many filters the results page's row has after Filtrat at most, the chips of those on and
/// the dropdowns of those not on yet together (`ParsedFilters::row`), as the canvas has five.
pub const ROW_FILTERS: usize = 5;

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
/// inside of a 48-by-24 SVG drawn in strokes, BODY_TYPES), which the pages' filters show over its
/// name (pluribus-web's `Field::icon`); a property whose options have one keeps its boxes out of
/// the row (`Property::pictured`). pluribus-web's properties.rs's until C13.
pub fn option_icon(code: &str, option: &str) -> Option<&'static str> {
    match code {
        "vehicle_body_type" => BODY_TYPES.iter().find(|(of, _)| *of == option).map(|(_, icon)| *icon),
        _ => None,
    }
}

/// The options a property's filter offers (`Property::offered`).
pub struct Offered<'a> {
    /// Those under no other property's option: every one of a property that depends on none.
    pub top: Vec<&'a AttributeOption>,
    /// A dependent property's (a car's model's) under each option of its parent (a make) that has
    /// any, in the parent's order.
    pub groups: Vec<(&'a AttributeOption, Vec<&'a AttributeOption>)>,
}

impl Offered<'_> {
    /// Whether there is none.
    pub fn is_empty(&self) -> bool {
        self.top.is_empty() && self.groups.is_empty()
    }
}

/// One of the results page's row of filter dropdowns after Filtrat (`ParsedFilters::row`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowFilter {
    /// The town, by county.
    City,
    /// The kind of listing, one of those the category takes.
    Type,
    /// A property's filter: the one at this place in `ParsedFilters::properties`.
    Property(usize),
    /// The condition's boxes.
    Condition,
    /// The price.
    Price,
}

impl<'a> Property<'a> {
    /// The property among `all`, the query's (`ParsedFilters::properties`), whose options this
    /// one's belong under: a car model's make. None for most, and for one whose parent the
    /// category doesn't have.
    pub fn parent<'p>(&self, all: &'p [Property<'a>]) -> Option<&'p Property<'a>> {
        self.attribute.parent_attribute_id.and_then(|id| all.iter().find(|other| other.attribute.id == id))
    }

    /// How the filters show it: a pair of years as one year; a number as a range; a yes or no as a
    /// box to tick; a multi-select's options, and a choice's few (CHECKBOX_LIMIT at most) where it
    /// depends on no other property, as boxes; any other choice's as a dropdown (a make's, a
    /// model's under their makes).
    pub fn kind(&self, all: &[Property<'a>]) -> FieldKind {
        if self.fits_to.is_some() {
            return FieldKind::Year;
        }
        match self.attribute.data_type.as_str() {
            "NUMBER" => FieldKind::Range,
            "BOOLEAN" => FieldKind::Toggle,
            data_type => {
                let few = self.parent(all).is_none() && self.attribute.options.len() <= CHECKBOX_LIMIT;
                if data_type == "MULTISELECT" || few { FieldKind::Checkboxes } else { FieldKind::Select }
            }
        }
    }

    /// Whether the query sets it: an option chosen, a bound, a box ticked, a year.
    pub fn active(&self) -> bool {
        !self.selected.is_empty() || self.min.is_some() || self.max.is_some() || self.checked || self.year.is_some()
    }

    /// The options its filter offers (`Offered`): of a property that depends on none, every one;
    /// of one that depends on another among `all` (a model, on its make), those under none of the
    /// other's options, and the rest under each of them, in the other's order; one under an option
    /// the other hasn't, nowhere.
    pub fn offered(&self, all: &[Property<'a>]) -> Offered<'a> {
        let attribute: &'a FilterAttribute = self.attribute;
        let parent = self.parent(all);
        let top = attribute.options.iter().filter(|option| parent.is_none() || option.parent_id.is_none()).collect();
        let groups = match parent {
            Some(parent) => {
                let parent: &'a FilterAttribute = parent.attribute;
                parent
                    .options
                    .iter()
                    .map(|of| (of, attribute.options.iter().filter(|option| option.parent_id == Some(of.id)).collect::<Vec<_>>()))
                    .filter(|(_, options)| !options.is_empty())
                    .collect()
            }
            None => Vec::new(),
        };
        Offered { top, groups }
    }

    /// Whether any of the options it offers has an icon over its name (`option_icon`: a car's body
    /// types), whose boxes want room of their own.
    pub fn pictured(&self, all: &[Property<'a>]) -> bool {
        let offered = self.offered(all);
        let mut options = offered.top.iter().chain(offered.groups.iter().flat_map(|(_, options)| options));
        options.any(|option| option_icon(&self.attribute.code, &option.code).is_some())
    }
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

    /// A category of these kinds of listing, under `parent`, whose listings say their condition or
    /// not.
    fn category(id: u128, parent: Option<u128>, slug: &str, kinds: &[&str], uses_condition: bool) -> Category {
        Category {
            id: Uuid::from_u128(id),
            parent_id: parent.map(Uuid::from_u128),
            name: slug.to_string(),
            slug: slug.to_string(),
            default_currency: "EUR".to_string(),
            listing_types: kinds.iter().map(|kind| kind.to_string()).collect(),
            uses_condition,
        }
    }

    /// A filterable property, its options named as given, each under one of `parent`'s in turn.
    fn property(id: u128, code: &str, data_type: &str, unit: Option<&str>, at_once: bool, options: &[&str], parent: Option<&FilterAttribute>) -> FilterAttribute {
        let options = options.iter().enumerate().map(|(at, code)| AttributeOption {
            id: Uuid::from_u128(id * 100 + at as u128),
            code: code.to_string(),
            label: code.to_string(),
            parent_id: parent.map(|parent| parent.options[at % parent.options.len()].id),
        });
        FilterAttribute {
            id: Uuid::from_u128(id),
            code: code.to_string(),
            name: code.to_string(),
            data_type: data_type.to_string(),
            unit: unit.map(str::to_string),
            parent_attribute_id: parent.map(|parent| parent.id),
            required: at_once,
            show_in_card: false,
            options: sqlx::types::Json(options.collect()),
        }
    }

    /// The row the filters of `query` in `category` (by its slug) make beside `chips` chips, each by
    /// its property's code or by what it is.
    fn row(query: &str, category: Option<&str>, categories: &Categories, attributes: &[FilterAttribute], chips: usize) -> Vec<String> {
        let category = category.map(|slug| categories.by_slug(slug).unwrap());
        let filters = ParsedFilters::new(&Params::parse(Some(query)), category, categories, &[], attributes, None);
        filters
            .row(chips)
            .into_iter()
            .map(|filter| match filter {
                RowFilter::Property(at) => filters.properties[at].attribute.code.clone(),
                other => format!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn the_row_offers_the_filters_buyers_use_most_the_price_fourth() {
        let categories = Categories::new(vec![
            category(1, None, "automjete", &["SELL", "BUY", "RENT"], true),
            category(2, Some(1), "makina", &["SELL", "BUY", "RENT"], true),
            category(3, None, "prona", &["SELL", "RENT", "WANTED_RENT"], false),
            category(4, Some(3), "apartamente", &["SELL", "RENT", "WANTED_RENT"], false),
        ]);
        let makes: Vec<String> = (0..12).map(|n| format!("MAKE_{n}")).collect();
        let make = property(10, "vehicle_make", "SELECT", None, true, &makes.iter().map(String::as_str).collect::<Vec<_>>(), None);
        let model = property(11, "vehicle_model", "SELECT", None, true, &["GOLF", "POLO", "A4"], Some(&make));
        let cars = [
            make,
            model,
            // Drawn over their names: the sheet's, never the row's.
            property(12, "vehicle_body_type", "SELECT", None, false, &["SEDAN", "SUV"], None),
            property(13, "vehicle_year", "NUMBER", None, true, &[], None),
            property(14, "vehicle_customs", "SELECT", None, false, &["PAID", "UNPAID"], None),
            // A range in a unit, and a box to tick: the sheet's.
            property(15, "vehicle_mileage", "NUMBER", Some("km"), true, &[], None),
            property(16, "vehicle_fuel", "SELECT", None, false, &["DIESEL", "PETROL", "HYBRID"], None),
            property(17, "vehicle_engine", "NUMBER", Some("cm³"), false, &[], None),
            property(18, "vehicle_emission", "SELECT", None, false, &["EURO5", "EURO6"], None),
            property(19, "vehicle_no_accident", "BOOLEAN", None, false, &[], None),
        ];
        // The make, its models and the year first, those shown at once, then the fuel, one of
        // PRIMARY; the price fourth; the rest, the customs, the condition and the kind, no room.
        let filters = ParsedFilters::new(&Params::parse(None), categories.by_slug("makina"), &categories, &[], &cars, None);
        let codes = |order: Vec<usize>| order.into_iter().map(|at| filters.properties[at].attribute.code.as_str()).collect::<Vec<_>>();
        assert_eq!(codes(filters.sheet_order())[..6], ["vehicle_make", "vehicle_model", "vehicle_body_type", "vehicle_year", "vehicle_mileage", "vehicle_fuel"]);
        assert_eq!(row("", Some("makina"), &categories, &cars, 0), ["vehicle_make", "vehicle_model", "vehicle_year", "Price", "vehicle_fuel"]);
        // A filter on is a chip in place of its dropdown, the row five at most with the chips.
        assert_eq!(row("vehicle_fuel=DIESEL", Some("makina"), &categories, &cars, 1), ["vehicle_make", "vehicle_model", "Price", "vehicle_year"]);
        assert_eq!(row("price_max=9000", Some("makina"), &categories, &cars, 1), ["vehicle_make", "vehicle_model", "vehicle_year", "vehicle_fuel"]);
        assert!(row("", Some("makina"), &categories, &cars, 5).is_empty());
        // With nothing of its own, a main category has the condition, the price and the kind; a
        // search across every category starts with the town.
        assert_eq!(row("", Some("automjete"), &categories, &[], 0), ["Condition", "Price", "Type"]);
        assert_eq!(row("q=golf", None, &categories, &[], 0), ["City", "Price", "Condition"]);
        // In property the kind comes first, the price after a lone other.
        let flats = [
            property(20, "property_area", "NUMBER", Some("m²"), true, &[], None),
            property(21, "property_features", "MULTISELECT", None, false, &["BALCONY", "LIFT"], None),
        ];
        assert_eq!(row("", Some("apartamente"), &categories, &flats, 0), ["Type", "Price", "property_features"]);
        let flats = ParsedFilters::new(&Params::parse(None), categories.by_slug("apartamente"), &categories, &[], &flats, None);
        assert_eq!(flats.shown_at_once(), [true, true], "two, all at once");
        assert_eq!(flats.properties[1].kind(&flats.properties), FieldKind::Checkboxes);
        assert!(!filters.properties[1].offered(&filters.properties).groups.is_empty(), "the models under their makes");
        assert!(filters.properties[2].pictured(&filters.properties) && !filters.properties[0].pictured(&filters.properties));
    }
}
