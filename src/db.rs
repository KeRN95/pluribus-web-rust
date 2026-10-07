//! Database access. Fixed queries use sqlx's `query!` macros, which are checked against the real
//! schema at compile time. Listing pages use `QueryBuilder`, because their filters are optional.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, Postgres, QueryBuilder, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

/// Listings per page. Pages fetch one extra row to find out whether another page follows.
pub const PAGE_SIZE: i64 = 24;

/// Result counts stop here and show as "1 000+": counting every match of a broad search costs
/// more than the exact number is worth.
pub const COUNT_LIMIT: i64 = 1_000;

// ---------------------------------------------------------------- categories and places

#[derive(Debug, Clone)]
pub struct Category {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub default_currency: String,
    /// The kinds of listing the category takes: SELL, BUY, RENT, WANTED_RENT, SERVICE, JOB.
    pub listing_types: Vec<String>,
    /// Whether its listings have a condition (new, used...).
    pub uses_condition: bool,
}

/// The active categories in display order. There are few and most pages need several, so they
/// load in one query and the tree is walked in memory.
pub struct Categories(Vec<Category>);

impl Categories {
    pub fn top_level(&self) -> impl Iterator<Item = &Category> {
        self.0.iter().filter(|c| c.parent_id.is_none())
    }

    pub fn children(&self, id: Uuid) -> impl Iterator<Item = &Category> {
        self.0.iter().filter(move |c| c.parent_id == Some(id))
    }

    /// Whether the category only groups subcategories, so listings go into one of those instead.
    pub fn has_children(&self, id: Uuid) -> bool {
        self.children(id).next().is_some()
    }

    pub fn by_slug(&self, slug: &str) -> Option<&Category> {
        self.0.iter().find(|c| c.slug == slug)
    }

    /// The category and all its descendants, so browsing a parent includes its subcategories.
    pub fn subtree_ids(&self, id: Uuid) -> Vec<Uuid> {
        let mut ids = vec![id];
        let mut next = 0;
        while let Some(&parent) = ids.get(next) {
            for child in self.children(parent) {
                if !ids.contains(&child.id) {
                    ids.push(child.id);
                }
            }
            next += 1;
        }
        ids
    }

    /// The categories above this one, top level first.
    pub fn ancestors(&self, category: &Category) -> Vec<&Category> {
        let mut chain: Vec<&Category> = Vec::new();
        let mut parent_id = category.parent_id;
        while let Some(parent) = parent_id.and_then(|id| self.0.iter().find(|c| c.id == id)) {
            if chain.iter().any(|c| c.id == parent.id) {
                break;
            }
            chain.push(parent);
            parent_id = parent.parent_id;
        }
        chain.reverse();
        chain
    }
}

pub async fn categories(db: &PgPool) -> sqlx::Result<Categories> {
    let rows = sqlx::query_as!(
        Category,
        r#"SELECT id, parent_id, name, slug, default_currency::text AS "default_currency!",
                  listing_types, uses_condition
           FROM categories
           WHERE active
           ORDER BY sort_order, name"#
    )
    .fetch_all(db)
    .await?;
    Ok(Categories(rows))
}

/// A municipality: listings are placed in one (or somewhere inside one), so these are what the
/// city filter offers. Each is named after its main town.
#[derive(Debug, Clone)]
pub struct City {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub county: String,
    /// The town's point, where its listings stand on the map without one of their own, and where
    /// the map's town dropdown moves it.
    pub latitude: f64,
    pub longitude: f64,
}

/// Grouped by county, each county's main town first.
pub async fn cities(db: &PgPool) -> sqlx::Result<Vec<City>> {
    sqlx::query_as!(
        City,
        r#"SELECT m.id, m.name, m.slug, county.name AS county,
                m.latitude::float8 AS "latitude!", m.longitude::float8 AS "longitude!"
         FROM locations m
         JOIN locations county ON county.id = m.parent_id
         WHERE m.location_type = 'MUNICIPALITY' AND m.active
         ORDER BY county.name, m.slug <> county.slug, m.name"#
    )
    .fetch_all(db)
    .await
}

// ---------------------------------------------------------------- properties

/// A property a category can be filtered on.
#[derive(Debug)]
pub struct FilterAttribute {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    /// SELECT, MULTISELECT, NUMBER or BOOLEAN.
    pub data_type: String,
    pub unit: Option<String>,
    /// Set for dependent properties, like a car model, whose options each belong to an option
    /// of the parent property (the make).
    pub parent_attribute_id: Option<Uuid>,
    pub options: Json<Vec<AttributeOption>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttributeOption {
    pub id: Uuid,
    pub code: String,
    pub label: String,
    pub parent_id: Option<Uuid>,
}

/// The category's filterable properties, its own and inherited, in display order, each under the
/// category's name for it (category_attributes.label) where it has one. Looked up by slug, so a
/// page can load them alongside the categories.
pub async fn filter_attributes(
    db: &PgPool,
    category_slug: &str,
) -> sqlx::Result<Vec<FilterAttribute>> {
    sqlx::query_as!(
        FilterAttribute,
        r#"SELECT a.id, a.code, coalesce(eca.label, a.name) AS "name!", a.data_type, a.unit, a.parent_attribute_id,
                  coalesce(jsonb_agg(jsonb_build_object('id', o.id, 'code', o.value_code,
                                                         'label', o.label, 'parent_id', o.parent_option_id)
                                     ORDER BY o.sort_order, o.label)
                               FILTER (WHERE o.id IS NOT NULL),
                           '[]') AS "options!: Json<Vec<AttributeOption>>"
           FROM effective_category_attributes eca
           JOIN attributes a ON a.id = eca.attribute_id
           LEFT JOIN attribute_options o ON o.attribute_id = a.id AND o.active
           WHERE eca.category_id = (SELECT id FROM categories WHERE slug = $1 AND active)
             AND eca.filterable
             AND a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN')
           GROUP BY a.id, eca.sort_order, eca.label
           ORDER BY eca.sort_order, a.name"#,
        category_slug
    )
    .fetch_all(db)
    .await
}

/// A property as the listing form asks for it.
#[derive(Debug)]
pub struct FormAttribute {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    /// SELECT, MULTISELECT, NUMBER, BOOLEAN or TEXT.
    pub data_type: String,
    pub unit: Option<String>,
    pub parent_attribute_id: Option<Uuid>,
    pub required: bool,
    /// The seller may type an option the list lacks ("Tjetër"), like a car make.
    pub allow_other: bool,
    pub min: Option<Decimal>,
    pub max: Option<Decimal>,
    pub options: Json<Vec<AttributeOption>>,
}

/// The properties a listing in this category can have, its own and inherited, in form order, under
/// the category's name for them where it has one. Not the subcategory's kind (the old "type"
/// property): the subcategory sets that one.
pub async fn form_attributes(db: &PgPool, category_id: Uuid) -> sqlx::Result<Vec<FormAttribute>> {
    sqlx::query_as!(
        FormAttribute,
        r#"SELECT a.id, a.code, coalesce(eca.label, a.name) AS "name!", a.data_type, a.unit, a.parent_attribute_id,
                  eca.required AS "required!",
                  a.allow_other,
                  (a.validation_json ->> 'min')::numeric AS min, (a.validation_json ->> 'max')::numeric AS max,
                  coalesce(jsonb_agg(jsonb_build_object('id', o.id, 'code', o.value_code,
                                                         'label', o.label, 'parent_id', o.parent_option_id)
                                     ORDER BY o.sort_order, o.label)
                               FILTER (WHERE o.id IS NOT NULL),
                           '[]') AS "options!: Json<Vec<AttributeOption>>"
           FROM effective_category_attributes eca
           JOIN attributes a ON a.id = eca.attribute_id
           LEFT JOIN attribute_options o ON o.attribute_id = a.id AND o.active
           WHERE eca.category_id = $1
             AND a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN', 'TEXT')
             AND NOT EXISTS (SELECT 1 FROM categories k JOIN attribute_options ko ON ko.id = k.replaces_option_id
                             WHERE ko.attribute_id = a.id)
           GROUP BY a.id, eca.sort_order, eca.required, eca.label
           ORDER BY eca.sort_order, a.name"#,
        category_id
    )
    .fetch_all(db)
    .await
}

/// A listing's property values as the form holds them: option codes, numbers as typed, "1" for a
/// ticked box; several rows for a multi-select. An option the seller typed, not yet in the lists,
/// comes back as "Tjetër" ("__other__") with its text under "<code>_other".
pub async fn listing_property_values(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<(String, String)>> {
    let rows = sqlx::query!(
        r#"SELECT a.code AS "code!",
                  CASE WHEN o.id IS NOT NULL AND NOT o.active THEN '__other__'
                       ELSE coalesce(o.value_code, trim_scale(v.value_number)::text, v.value_text,
                                     CASE WHEN v.value_boolean THEN '1' END)
                  END AS value
           FROM listing_attribute_values v
           JOIN attributes a ON a.id = v.attribute_id
           LEFT JOIN attribute_options o ON o.id = v.value_option_id
           WHERE v.listing_id = $1
           UNION ALL
           SELECT a.code || '_other', o.label
           FROM listing_attribute_values v
           JOIN attributes a ON a.id = v.attribute_id
           JOIN attribute_options o ON o.id = v.value_option_id AND NOT o.active
           WHERE v.listing_id = $1
           UNION ALL
           SELECT a.code, o.value_code
           FROM listing_attribute_options lo
           JOIN attributes a ON a.id = lo.attribute_id
           JOIN attribute_options o ON o.id = lo.option_id
           WHERE lo.listing_id = $1"#,
        listing_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(|row| Some((row.code, row.value?))).collect())
}

// ---------------------------------------------------------------- listing pages

#[derive(Debug, sqlx::FromRow)]
pub struct ListingCard {
    pub id: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub listing_type: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    pub price_normalized: Option<Decimal>,
    pub location_name: Option<String>,
    /// The name of the municipality the listing is in.
    pub city: Option<String>,
    /// How far the listing is from the map pin, in metres, when the search has one.
    pub distance_m: Option<f64>,
    pub cover_url: Option<String>,
    /// The category's card properties in order, e.g. ["2016", "148 000 km", "Naftë"].
    pub details: Vec<String>,
    /// The first property the listing page shows that the card doesn't and the title doesn't
    /// mention, e.g. a phone's brand: what the card shows instead of the city when the search is
    /// in that city already. Only a chosen option: a bare number ("2003") says nothing on a card.
    pub spare_detail: Option<String>,
    pub sort_at: OffsetDateTime,
    /// Set when the search is limited to the listing's own city, which the card then leaves out.
    #[sqlx(skip)]
    pub city_known: bool,
}

/// The orders a listing page can be sorted in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Newest,
    PriceLow,
    PriceHigh,
    /// Nearest to the map pin first; only with a pin.
    Nearest,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Newest, Sort::PriceLow, Sort::PriceHigh, Sort::Nearest];

    pub fn from_param(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sort| sort.param() == value)
    }

    pub fn param(self) -> &'static str {
        match self {
            Sort::Newest => "newest",
            Sort::PriceLow => "price_asc",
            Sort::PriceHigh => "price_desc",
            Sort::Nearest => "nearest",
        }
    }

    /// Pushes the ORDER BY key. Listings without a price (price on request) come last in both
    /// price orders, so a missing price counts as infinitely high or low.
    fn push_key(self, query: &mut QueryBuilder<'_, Postgres>, near: Option<&Near>) {
        match (self, near) {
            (Sort::Nearest, Some(near)) => near.push_distance(query),
            (Sort::Newest | Sort::Nearest, _) => {
                query.push("l.sort_at");
            }
            (Sort::PriceLow, _) => {
                query.push("coalesce(l.price_normalized, 'Infinity')");
            }
            (Sort::PriceHigh, _) => {
                query.push("coalesce(l.price_normalized, '-Infinity')");
            }
        }
    }

    fn descending(self) -> bool {
        !matches!(self, Sort::PriceLow | Sort::Nearest)
    }
}

/// Where the next page starts: keyset pagination on the sort key, with the id breaking ties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cursor {
    Newest { sort_at: OffsetDateTime, id: Uuid },
    Price { price: Option<Decimal>, id: Uuid },
    Distance { meters: f64, id: Uuid },
}

impl Cursor {
    /// The position right after `card` in a page sorted by `sort`.
    pub fn after(sort: Sort, card: &ListingCard) -> Self {
        match (sort, card.distance_m) {
            (Sort::Nearest, Some(meters)) => Self::Distance { meters, id: card.id },
            (Sort::Newest | Sort::Nearest, _) => Self::Newest { sort_at: card.sort_at, id: card.id },
            (Sort::PriceLow | Sort::PriceHigh, _) => {
                Self::Price { price: card.price_normalized, id: card.id }
            }
        }
    }

    pub fn id(&self) -> Uuid {
        match self {
            Self::Newest { id, .. } | Self::Price { id, .. } | Self::Distance { id, .. } => *id,
        }
    }

    /// "<key>_<id>". The key is microseconds since 1970 (PostgreSQL's precision) for the newest
    /// first order, the price in euros, or "-" for none, for the price orders, and the distance
    /// in metres, in full, for the nearest first order.
    pub fn encode(&self) -> String {
        match self {
            Self::Newest { sort_at, id } => {
                format!("{}_{id}", sort_at.unix_timestamp_nanos() / 1_000)
            }
            Self::Price { price: Some(price), id } => format!("{price}_{id}"),
            Self::Price { price: None, id } => format!("-_{id}"),
            Self::Distance { meters, id } => format!("{meters}_{id}"),
        }
    }

    pub fn decode(sort: Sort, value: &str) -> Option<Self> {
        let (key, id) = value.split_once('_')?;
        let id = id.parse().ok()?;
        match sort {
            Sort::Newest => {
                let nanos = key.parse::<i128>().ok()?.checked_mul(1_000)?;
                let sort_at = OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
                Some(Self::Newest { sort_at, id })
            }
            Sort::PriceLow | Sort::PriceHigh => {
                let price = if key == "-" { None } else { Some(key.parse().ok()?) };
                Some(Self::Price { price, id })
            }
            Sort::Nearest => {
                let meters: f64 = key.parse().ok()?;
                meters.is_finite().then_some(Self::Distance { meters, id })
            }
        }
    }
}

/// A pin on the map and how far around it to look.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Near {
    pub latitude: f64,
    pub longitude: f64,
    pub radius_m: f64,
}

impl Near {
    /// The distance between the pin and the listing, in metres. The listing's side is written
    /// exactly as in the idx_listings_active_earth index, so "within" can use it.
    fn push_distance(&self, query: &mut QueryBuilder<'_, Postgres>) {
        query
            .push("earth_distance(ll_to_earth(")
            .push_bind(self.latitude)
            .push(", ")
            .push_bind(self.longitude)
            .push("), ll_to_earth(l.latitude::float8, l.longitude::float8))");
    }
}

#[derive(Debug, Default)]
pub struct ListingFilter {
    /// One seller's listings, for their profile.
    pub seller: Option<Seller>,
    pub category_ids: Option<Vec<Uuid>>,
    /// A municipality: listings placed in it or anywhere inside it.
    pub location_id: Option<Uuid>,
    /// A map pin: listings within its radius.
    pub near: Option<Near>,
    /// Words to search for, made by `text_search`.
    pub search: Option<TextSearch>,
    pub listing_type: Option<String>,
    /// Any of these conditions (NEW, LIKE_NEW, USED, FOR_PARTS).
    pub conditions: Vec<String>,
    pub price: Option<PriceRange>,
    pub attributes: Vec<AttributeFilter>,
    pub sort: Sort,
    pub after: Option<Cursor>,
}

/// A visitor's words as a search, which forgives typos and finds other forms of a word: "golff"
/// finds the Golfs, "makinat" the listings that say "makinë" (search_query, migration 0016).
#[derive(Debug, Clone)]
pub struct TextSearch {
    /// The tsquery listings are matched against, as text.
    query: String,
    /// The words searched in place of misspelt ones ("golf" for "golff"), in the order typed.
    pub corrections: Vec<String>,
}

/// The search for these words, or None without any. Pages work it out once, before the listing
/// queries, and those take it as a parameter: worked out inside them, it would be hidden from the
/// planner, and a search that finds nothing would walk the whole newest-first index.
pub async fn text_search(db: &PgPool, words: Option<&str>) -> sqlx::Result<Option<TextSearch>> {
    let Some(words) = words else {
        return Ok(None);
    };
    let row = sqlx::query!(
        r#"SELECT query::text AS "query!", corrections AS "corrections!" FROM search_query($1)"#,
        words,
    )
    .fetch_one(db)
    .await?;
    Ok(Some(TextSearch { query: row.query, corrections: row.corrections }))
}

/// Rebuilds the list of words listings use, which search takes typos' corrections and words'
/// other forms from, so that it follows the listings. Searches go on reading the old list
/// meanwhile.
pub async fn refresh_search_words(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query!("REFRESH MATERIALIZED VIEW CONCURRENTLY search_words").execute(db).await?;
    Ok(())
}

/// Who sells a listing: a private seller (a user) or a business.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seller {
    User(Uuid),
    Business(Uuid),
}

/// A seller's public profile: their name, since when they are on the site, and how many active
/// listings they have.
pub struct Profile {
    pub seller: Seller,
    pub name: String,
    pub since: OffsetDateTime,
    pub listings: i64,
}

pub async fn user_profile(db: &PgPool, id: Uuid) -> sqlx::Result<Option<Profile>> {
    let row = sqlx::query!(
        r#"SELECT u.display_name, u.created_at,
                  (SELECT count(*) FROM listings l WHERE l.seller_user_id = u.id AND l.status = 'ACTIVE') AS "listings!"
           FROM users u WHERE u.id = $1 AND u.status = 'ACTIVE'"#,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| Profile { seller: Seller::User(id), name: row.display_name, since: row.created_at, listings: row.listings }))
}

pub async fn business_profile(db: &PgPool, slug: &str) -> sqlx::Result<Option<Profile>> {
    let row = sqlx::query!(
        r#"SELECT b.id, b.display_name, b.created_at,
                  (SELECT count(*) FROM listings l WHERE l.seller_business_id = b.id AND l.status = 'ACTIVE') AS "listings!"
           FROM businesses b WHERE b.slug = $1 AND b.status = 'ACTIVE'"#,
        slug,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| Profile { seller: Seller::Business(row.id), name: row.display_name, since: row.created_at, listings: row.listings }))
}

#[derive(Debug, Clone)]
pub struct PriceRange {
    pub min: Option<Decimal>,
    pub max: Option<Decimal>,
    /// The currency of the bounds. They are converted to euros the way listing prices are, and
    /// compared with price_normalized.
    pub currency: String,
}

#[derive(Debug, Clone)]
pub enum AttributeFilter {
    /// A SELECT property set to any of these options.
    AnyOf { attribute_id: Uuid, option_ids: Vec<Uuid> },
    /// A MULTISELECT property that includes all of these options.
    AllOf { attribute_id: Uuid, option_ids: Vec<Uuid> },
    /// A NUMBER property within the bounds, inclusive.
    Range { attribute_id: Uuid, min: Option<Decimal>, max: Option<Decimal> },
    /// A BOOLEAN property that is true.
    IsTrue { attribute_id: Uuid },
    /// A pair of years a listing fits ("fits_from_year", "fits_to_year") that includes `year`. A
    /// listing that doesn't give one end, or either, may fit, so it stays.
    Fits { from_attribute_id: Uuid, to_attribute_id: Uuid, year: Decimal },
}

/// One page of active listings in the filter's order, with at most PAGE_SIZE + 1 rows.
pub async fn listing_page(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(
        "WITH card_attributes AS MATERIALIZED (
             SELECT category_id, attribute_id, sort_order
             FROM effective_category_attributes
             WHERE show_in_card
         ),
         spare_attributes AS MATERIALIZED (
             SELECT category_id, attribute_id, sort_order
             FROM effective_category_attributes
             WHERE show_in_details AND NOT show_in_card
         )
         SELECT l.id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.listing_type, l.price_amount,
                l.currency::text AS currency, l.price_type, l.price_normalized,
                loc.name AS location_name,
                (SELECT t.name FROM locations t
                 WHERE t.id = location_municipality(l.location_id)) AS city,
                m.thumbnail_url AS cover_url, l.sort_at,
                coalesce(
                    (SELECT array_agg(card.value ORDER BY card.sort_order)
                                FILTER (WHERE card.value IS NOT NULL)
                     FROM (SELECT ca.sort_order,
                                  coalesce(o.label, format_attribute_number(v.value_number, a.unit),
                                           v.value_text, CASE WHEN v.value_boolean THEN a.name END) AS value
                           FROM listing_attribute_values v
                           JOIN card_attributes ca
                             ON ca.category_id = l.category_id AND ca.attribute_id = v.attribute_id
                           JOIN attributes a ON a.id = v.attribute_id
                           LEFT JOIN attribute_options o ON o.id = v.value_option_id
                           WHERE v.listing_id = l.id) card),
                    '{}') AS details,
                (SELECT spare.value
                 FROM (SELECT sa.sort_order,
                              coalesce(o.label, format_attribute_number(v.value_number, a.unit),
                                       v.value_text, CASE WHEN v.value_boolean THEN a.name END) AS value
                       FROM listing_attribute_values v
                       JOIN spare_attributes sa
                         ON sa.category_id = l.category_id AND sa.attribute_id = v.attribute_id
                       JOIN attributes a ON a.id = v.attribute_id
                       LEFT JOIN attribute_options o ON o.id = v.value_option_id
                       WHERE v.listing_id = l.id AND v.value_option_id IS NOT NULL) spare
                 -- Not what the title says already (\"Volkswagen\" in \"Volkswagen Golf 7\").
                 WHERE spare.value IS NOT NULL
                   AND strpos(lower(coalesce(l.generated_title, l.title)), lower(spare.value)) = 0
                 ORDER BY spare.sort_order
                 LIMIT 1) AS spare_detail,
                ",
    );
    match &filter.near {
        Some(near) => near.push_distance(&mut query),
        None => {
            query.push("NULL::float8");
        }
    }
    query.push(
        " AS distance_m
         FROM listings l
         LEFT JOIN locations loc ON loc.id = l.location_id
         LEFT JOIN listing_media m ON m.listing_id = l.id AND m.is_cover AND m.status = 'READY'",
    );
    push_filters(&mut query, filter);

    let (sort, near) = (filter.sort, filter.near.as_ref());
    let (direction, comparison) = if sort.descending() { ("DESC", "<") } else { ("ASC", ">") };
    if let Some(after) = filter.after {
        query.push(" AND (");
        sort.push_key(&mut query, near);
        query.push(format_args!(", l.id) {comparison} ("));
        match after {
            Cursor::Newest { sort_at, id } => {
                query.push_bind(sort_at).push(", ").push_bind(id);
            }
            Cursor::Price { price, id } => {
                let missing = if sort.descending() { "-Infinity" } else { "Infinity" };
                query.push("coalesce(").push_bind(price);
                query.push(format_args!("::numeric, '{missing}'), ")).push_bind(id);
            }
            Cursor::Distance { meters, id } => {
                query.push_bind(meters).push(", ").push_bind(id);
            }
        }
        query.push(")");
    }
    query.push(" ORDER BY ");
    sort.push_key(&mut query, near);
    query.push(format_args!(" {direction}, l.id {direction} LIMIT "));
    query.push_bind(PAGE_SIZE + 1);
    query.build_query_as().fetch_all(db).await
}

/// How many active listings match the filter, counting at most COUNT_LIMIT + 1.
pub async fn listing_count(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<i64> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT count(*) FROM (SELECT 1 FROM listings l");
    push_filters(&mut query, filter);
    query.push(" LIMIT ").push_bind(COUNT_LIMIT + 1).push(") matches");
    query.build_query_scalar().fetch_one(db).await
}

/// The most dots the map shows at once.
pub const MAP_POINTS: i64 = 500;

/// A listing's dot on the map, with what its popup shows.
#[derive(Debug, sqlx::FromRow)]
pub struct MapPoint {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub listing_type: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// The dots for every match that has a point, not only the page of cards: in the list's order,
/// at most MAP_POINTS + 1 of them, the extra one telling that some are left out.
pub async fn map_points(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Vec<MapPoint>> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.listing_type, l.price_amount,
                l.currency::text AS currency, l.price_type,
                l.latitude::float8 AS latitude, l.longitude::float8 AS longitude
         FROM listings l",
    );
    push_filters(&mut query, filter);
    query.push(" AND l.latitude IS NOT NULL ORDER BY ");
    filter.sort.push_key(&mut query, filter.near.as_ref());
    let direction = if filter.sort.descending() { "DESC" } else { "ASC" };
    query.push(format_args!(" {direction}, l.id {direction} LIMIT "));
    query.push_bind(MAP_POINTS + 1);
    query.build_query_as().fetch_all(db).await
}

/// The WHERE clause shared by a page of listings, their count and their dots on the map.
fn push_filters(query: &mut QueryBuilder<'_, Postgres>, filter: &ListingFilter) {
    query.push(" WHERE l.status = 'ACTIVE'");
    match filter.seller {
        Some(Seller::User(id)) => {
            query.push(" AND l.seller_user_id = ").push_bind(id);
        }
        Some(Seller::Business(id)) => {
            query.push(" AND l.seller_business_id = ").push_bind(id);
        }
        None => {}
    }
    if let Some(ids) = &filter.category_ids {
        query.push(" AND l.category_id = ANY(").push_bind(ids.clone()).push(")");
    }
    if let Some(location_id) = filter.location_id {
        query
            .push(
                " AND l.location_id IN (
                     WITH RECURSIVE area AS (
                         SELECT ",
            )
            .push_bind(location_id)
            .push(
                "::uuid AS id
                         UNION ALL
                         SELECT loc.id FROM locations loc JOIN area ON loc.parent_id = area.id
                     )
                     SELECT id FROM area)",
            );
    }
    if let Some(near) = &filter.near {
        // earth_box() finds the candidates in the index; earth_distance() keeps those truly
        // within the radius (the box's corners reach further).
        query
            .push(" AND l.latitude IS NOT NULL AND earth_box(ll_to_earth(")
            .push_bind(near.latitude)
            .push(", ")
            .push_bind(near.longitude)
            .push("), ")
            .push_bind(near.radius_m)
            .push(") @> ll_to_earth(l.latitude::float8, l.longitude::float8) AND ");
        near.push_distance(query);
        query.push(" <= ").push_bind(near.radius_m);
    }
    if let Some(search) = &filter.search {
        query.push(" AND l.search_vector @@ ").push_bind(search.query.clone()).push("::tsquery");
    }
    if let Some(listing_type) = &filter.listing_type {
        query.push(" AND l.listing_type = ").push_bind(listing_type.clone());
    }
    if !filter.conditions.is_empty() {
        query.push(" AND l.condition = ANY(").push_bind(filter.conditions.clone()).push(")");
    }
    if let Some(price) = &filter.price {
        for (bound, comparison) in [(price.min, ">="), (price.max, "<=")] {
            if let Some(bound) = bound {
                query.push(format_args!(" AND l.price_normalized {comparison} price_in_eur("));
                query.push_bind(bound).push(", ").push_bind(price.currency.clone()).push(")");
            }
        }
    }
    for attribute in &filter.attributes {
        match attribute {
            AttributeFilter::AnyOf { attribute_id, option_ids } => {
                query
                    .push(
                        " AND EXISTS (SELECT 1 FROM listing_attribute_values v
                                      WHERE v.listing_id = l.id AND v.attribute_id = ",
                    )
                    .push_bind(*attribute_id)
                    .push(" AND v.value_option_id = ANY(")
                    .push_bind(option_ids.clone())
                    .push("))");
            }
            AttributeFilter::AllOf { attribute_id, option_ids } => {
                query
                    .push(
                        " AND (SELECT count(*) FROM listing_attribute_options lo
                               WHERE lo.listing_id = l.id AND lo.attribute_id = ",
                    )
                    .push_bind(*attribute_id)
                    .push(" AND lo.option_id = ANY(")
                    .push_bind(option_ids.clone())
                    .push(")) = ")
                    .push_bind(option_ids.len() as i64);
            }
            AttributeFilter::Range { attribute_id, min, max } => {
                query
                    .push(
                        " AND EXISTS (SELECT 1 FROM listing_attribute_values v
                                      WHERE v.listing_id = l.id AND v.attribute_id = ",
                    )
                    .push_bind(*attribute_id);
                if let Some(min) = min {
                    query.push(" AND v.value_number >= ").push_bind(*min);
                }
                if let Some(max) = max {
                    query.push(" AND v.value_number <= ").push_bind(*max);
                }
                query.push(")");
            }
            AttributeFilter::IsTrue { attribute_id } => {
                query
                    .push(
                        " AND EXISTS (SELECT 1 FROM listing_attribute_values v
                                      WHERE v.listing_id = l.id AND v.attribute_id = ",
                    )
                    .push_bind(*attribute_id)
                    .push(" AND v.value_boolean)");
            }
            AttributeFilter::Fits { from_attribute_id, to_attribute_id, year } => {
                query
                    .push(
                        " AND NOT EXISTS (SELECT 1 FROM listing_attribute_values v
                                          WHERE v.listing_id = l.id AND v.attribute_id = ",
                    )
                    .push_bind(*from_attribute_id)
                    .push(" AND v.value_number > ")
                    .push_bind(*year)
                    .push(
                        ") AND NOT EXISTS (SELECT 1 FROM listing_attribute_values v
                                           WHERE v.listing_id = l.id AND v.attribute_id = ",
                    )
                    .push_bind(*to_attribute_id)
                    .push(" AND v.value_number < ")
                    .push_bind(*year)
                    .push(")");
            }
        }
    }
}

// ---------------------------------------------------------------- one listing

#[derive(Debug)]
pub struct ListingDetail {
    pub id: Uuid,
    pub public_no: i64,
    pub slug: String,
    /// What the page calls it: a car's title made from what it is (generated_title), or the
    /// seller's.
    pub title: String,
    /// The seller's own title, when the page shows a made one: it heads the description.
    pub heading: Option<String>,
    pub description: String,
    pub status: String,
    pub listing_type: String,
    pub condition: Option<String>,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    pub category_id: Uuid,
    pub category_name: String,
    pub category_slug: String,
    pub location_name: Option<String>,
    pub published_at: Option<OffsetDateTime>,
    /// The seller: a business's name, or the private seller's, with their profile's address.
    pub seller_name: String,
    pub seller_url: String,
    pub seller_since: OffsetDateTime,
    pub seller_listings: i64,
}

#[derive(Debug)]
pub struct AttributeValue {
    pub code: String,
    pub label: String,
    /// Its value as the page shows it; a multi-select's options, each on its own.
    pub shown: Vec<String>,
    pub multi: bool,
}

#[derive(Debug)]
pub struct Photo {
    pub url: String,
    /// Its card-sized copy, which the listing page blurs to fill the frame around the photo.
    pub card_url: String,
    pub width_px: Option<i32>,
    pub height_px: Option<i32>,
}

/// Sold and reserved listings stay viewable, so shared links keep working.
pub async fn listing_by_public_no(
    db: &PgPool,
    public_no: i64,
) -> sqlx::Result<Option<ListingDetail>> {
    sqlx::query_as!(
        ListingDetail,
        r#"SELECT l.id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS "title!",
                  CASE WHEN l.generated_title IS NOT NULL THEN l.title END AS heading,
                  l.description, l.status, l.listing_type,
                  l.condition, l.price_amount, l.currency::text AS "currency!", l.price_type,
                  c.id AS category_id, c.name AS category_name, c.slug AS category_slug,
                  loc.name AS "location_name?", l.published_at,
                  coalesce(b.display_name, u.display_name) AS "seller_name!",
                  CASE WHEN b.id IS NOT NULL THEN '/biznes/' || b.slug ELSE '/perdorues/' || u.id END AS "seller_url!",
                  coalesce(b.created_at, u.created_at) AS "seller_since!",
                  (SELECT count(*) FROM listings o
                   WHERE o.status = 'ACTIVE'
                     AND (o.seller_business_id = l.seller_business_id OR o.seller_user_id = l.seller_user_id)) AS "seller_listings!"
           FROM listings l
           JOIN categories c ON c.id = l.category_id
           LEFT JOIN locations loc ON loc.id = l.location_id
           LEFT JOIN users u ON u.id = l.seller_user_id
           LEFT JOIN businesses b ON b.id = l.seller_business_id
           WHERE l.public_no = $1 AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD')"#,
        public_no
    )
    .fetch_optional(db)
    .await
}

/// Property labels and display values, single values and multi-select lists alike, in the order
/// the listing's category defines. Only properties the category shows in details are included.
pub async fn listing_attributes(
    db: &PgPool,
    listing_id: Uuid,
    category_id: Uuid,
) -> sqlx::Result<Vec<AttributeValue>> {
    sqlx::query_as!(
        AttributeValue,
        r#"SELECT code AS "code!", label AS "label!", shown AS "shown!", multi AS "multi!"
           FROM (
               SELECT a.code, coalesce(eca.label, a.name) AS label,
                      ARRAY[coalesce(o.label, v.value_text, format_attribute_number(v.value_number, a.unit),
                                     CASE v.value_boolean WHEN true THEN 'Po' WHEN false THEN 'Jo' END,
                                     to_char(v.value_date, 'DD.MM.YYYY'))] AS shown,
                      false AS multi,
                      eca.sort_order
               FROM listing_attribute_values v
               JOIN attributes a ON a.id = v.attribute_id
               JOIN effective_category_attributes eca
                 ON eca.category_id = $2 AND eca.attribute_id = v.attribute_id AND eca.show_in_details
               LEFT JOIN attribute_options o ON o.id = v.value_option_id
               WHERE v.listing_id = $1
               UNION ALL
               SELECT a.code, coalesce(eca.label, a.name), array_agg(o.label ORDER BY o.sort_order), true,
                      eca.sort_order
               FROM listing_attribute_options lo
               JOIN attributes a ON a.id = lo.attribute_id
               JOIN effective_category_attributes eca
                 ON eca.category_id = $2 AND eca.attribute_id = lo.attribute_id AND eca.show_in_details
               JOIN attribute_options o ON o.id = lo.option_id
               WHERE lo.listing_id = $1
               GROUP BY a.id, a.code, a.name, eca.label, eca.sort_order
           ) attrs
           ORDER BY sort_order, label"#,
        listing_id,
        category_id
    )
    .fetch_all(db)
    .await
}

pub async fn listing_photos(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<Photo>> {
    sqlx::query_as!(
        Photo,
        r#"SELECT coalesce(public_url, thumbnail_url) AS "url!", coalesce(thumbnail_url, public_url) AS "card_url!",
                  width_px, height_px
           FROM listing_media
           WHERE listing_id = $1 AND status = 'READY' AND media_type = 'IMAGE'
             AND coalesce(public_url, thumbnail_url) IS NOT NULL
           ORDER BY is_cover DESC, sort_order"#,
        listing_id
    )
    .fetch_all(db)
    .await
}

/// A photo stored for a listing (media.rs): its key in the store, where the site serves it and its
/// card-sized copy, and what the full one weighs.
pub struct NewPhoto {
    pub storage_key: String,
    pub url: String,
    pub card_url: String,
    pub size_bytes: i64,
    pub width: i32,
    pub height: i32,
    pub sha256: Vec<u8>,
}

/// Adds photos after the listing's others, in order; the first photo a listing gets is its cover.
pub async fn add_listing_photos(db: &PgPool, listing_id: Uuid, photos: &[NewPhoto]) -> sqlx::Result<()> {
    if photos.is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    let last = sqlx::query!(
        r#"SELECT coalesce(max(sort_order), -1) AS "last!", coalesce(bool_or(is_cover), false) AS "has_cover!"
           FROM listing_media WHERE listing_id = $1"#,
        listing_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    for (i, photo) in photos.iter().enumerate() {
        sqlx::query!(
            "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url,
                                        mime_type, size_bytes, width_px, height_px, sha256, sort_order, is_cover)
             VALUES ($1, 'IMAGE', 'READY', $2, $3, $4, 'image/jpeg', $5, $6, $7, $8, $9, $10)",
            listing_id,
            photo.storage_key,
            photo.url,
            photo.card_url,
            photo.size_bytes,
            photo.width,
            photo.height,
            photo.sha256,
            last.last + 1 + i as i32,
            !last.has_cover && i == 0,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// A photo on the edit form, which its seller can remove.
pub struct OwnPhoto {
    pub id: Uuid,
    pub card_url: String,
}

/// The listing's photos, the cover first.
pub async fn own_listing_photos(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<OwnPhoto>> {
    sqlx::query_as!(
        OwnPhoto,
        r#"SELECT id, coalesce(thumbnail_url, public_url) AS "card_url!"
           FROM listing_media
           WHERE listing_id = $1 AND media_type = 'IMAGE' AND coalesce(thumbnail_url, public_url) IS NOT NULL
           ORDER BY is_cover DESC, sort_order"#,
        listing_id,
    )
    .fetch_all(db)
    .await
}

/// Removes some of the listing's photos; returns their keys in the store, to delete the files.
/// When the cover goes, the first photo left takes its place.
pub async fn remove_listing_photos(db: &PgPool, listing_id: Uuid, ids: &[Uuid]) -> sqlx::Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = db.begin().await?;
    let keys = sqlx::query_scalar!(
        "DELETE FROM listing_media WHERE listing_id = $1 AND id = ANY($2) RETURNING storage_key",
        listing_id,
        ids,
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE listing_media SET is_cover = true
         WHERE id = (SELECT id FROM listing_media WHERE listing_id = $1 ORDER BY sort_order LIMIT 1)
           AND NOT EXISTS (SELECT 1 FROM listing_media WHERE listing_id = $1 AND is_cover)",
        listing_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(keys)
}

/// How a buyer reaches the seller: the phone number, or, for sellers who signed up with an e-mail
/// address, that address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SellerContact {
    Phone(String),
    Email(String),
}

/// What the listing's page offers to reveal, by the same rule as `reveal_contact`, so the button
/// never shows where nothing can be revealed (a sold listing, say). The value itself stays out
/// of the page until the visitor asks for it.
pub async fn visible_contact(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Option<SellerContact>> {
    let row = sqlx::query!(
        r#"SELECT CASE WHEN c.show_phone THEN c.phone_e164::text END AS phone,
                  CASE WHEN c.show_email THEN c.email::text END AS email
           FROM listings l
           JOIN listing_contacts c ON c.listing_id = l.id
           WHERE l.id = $1 AND l.status IN ('ACTIVE', 'RESERVED')"#,
        listing_id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|row| match (row.phone, row.email) {
        (Some(phone), _) => Some(SellerContact::Phone(phone)),
        (None, Some(email)) => Some(SellerContact::Email(email)),
        (None, None) => None,
    }))
}

/// The listing's phone number, or its e-mail address if it has no number to show. Each reveal
/// is counted in today's statistics.
pub async fn reveal_contact(db: &PgPool, public_no: i64) -> sqlx::Result<Option<SellerContact>> {
    let Some(id) = sqlx::query_scalar!("SELECT id FROM listings WHERE public_no = $1", public_no)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let Some(contact) = visible_contact(db, id).await? else {
        return Ok(None);
    };
    // Fine at launch volumes; buffer these counters once traffic grows (README, "Analytics").
    sqlx::query!(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, phone_reveals)
         VALUES ($1, current_date, 1)
         ON CONFLICT (listing_id, stat_date)
         DO UPDATE SET phone_reveals = listing_daily_stats.phone_reveals + 1",
        id
    )
    .execute(db)
    .await?;

    Ok(Some(contact))
}

// ---------------------------------------------------------------- signing in

/// How many sign-in codes a contact and a network (client IP) asked for in the last hour, and how
/// many seconds ago the contact's latest one went out.
pub struct CodeRequests {
    pub for_contact: i64,
    pub from_ip: i64,
    pub seconds_since_last: Option<f64>,
}

pub async fn code_requests(db: &PgPool, contact: &str, ip: Option<&str>) -> sqlx::Result<CodeRequests> {
    sqlx::query_as!(
        CodeRequests,
        r#"SELECT
               (SELECT count(*) FROM login_codes
                WHERE contact = $1 AND created_at > now() - interval '1 hour') AS "for_contact!",
               (SELECT count(*) FROM login_codes
                WHERE client_ip = $2::text::inet AND created_at > now() - interval '1 hour') AS "from_ip!",
               (SELECT extract(epoch FROM now() - max(created_at))::float8 FROM login_codes
                WHERE contact = $1) AS seconds_since_last"#,
        contact,
        ip,
    )
    .fetch_one(db)
    .await
}

/// What a sign-in code carries besides the contact: a listing to publish once it is confirmed
/// (posted before signing in), and a name for a new account.
#[derive(Debug, Default, Clone)]
pub struct CodePurpose {
    pub listing_id: Option<Uuid>,
    pub display_name: Option<String>,
}

pub async fn create_login_code(
    db: &PgPool,
    contact: &str,
    code_hash: &[u8],
    ip: Option<&str>,
    minutes: i32,
    purpose: &CodePurpose,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar!(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at, listing_id, display_name)
         VALUES ($1, $2, $3::text::inet, now() + make_interval(mins => $4), $5, $6)
         RETURNING id",
        contact,
        code_hash,
        ip,
        minutes,
        purpose.listing_id,
        purpose.display_name,
    )
    .fetch_one(db)
    .await
}

/// A code that never went out (the provider refused it): gone, so it neither waits nor counts.
pub async fn delete_login_code(db: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM login_codes WHERE id = $1", id).execute(db).await.map(|_| ())
}

/// A code waiting to be typed in: who it went to, and whether it publishes a listing, if it can
/// still be used.
pub struct PendingCode {
    pub contact: String,
    pub publishes: bool,
}

pub async fn pending_login_code(db: &PgPool, id: Uuid, max_attempts: i16) -> sqlx::Result<Option<PendingCode>> {
    sqlx::query_as!(
        PendingCode,
        r#"SELECT contact, listing_id IS NOT NULL AS "publishes!" FROM login_codes
           WHERE id = $1 AND used_at IS NULL AND expires_at > now() AND attempts < $2"#,
        id,
        max_attempts,
    )
    .fetch_optional(db)
    .await
}

pub enum CodeCheck {
    /// Right: the code is used up, and the contact it went to is proven.
    Signed { contact: String, purpose: CodePurpose },
    /// Wrong, with this many tries left.
    Wrong(i16),
    /// Expired, used, or tried too often: a new code is needed.
    Gone,
}

/// Checks a typed code. The row is locked, so two guesses at once can't both count as the last.
pub async fn check_login_code(db: &PgPool, id: Uuid, code_hash: &[u8], max_attempts: i16) -> sqlx::Result<CodeCheck> {
    let mut tx = db.begin().await?;
    let Some(row) = sqlx::query!(
        "SELECT contact, code_hash, attempts, listing_id, display_name FROM login_codes
         WHERE id = $1 AND used_at IS NULL AND expires_at > now()
         FOR UPDATE",
        id,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(CodeCheck::Gone);
    };
    if row.attempts >= max_attempts {
        return Ok(CodeCheck::Gone);
    }
    let check = if row.code_hash == code_hash {
        sqlx::query!("UPDATE login_codes SET used_at = now() WHERE id = $1", id).execute(&mut *tx).await?;
        CodeCheck::Signed {
            contact: row.contact,
            purpose: CodePurpose { listing_id: row.listing_id, display_name: row.display_name },
        }
    } else {
        sqlx::query!("UPDATE login_codes SET attempts = attempts + 1 WHERE id = $1", id).execute(&mut *tx).await?;
        match max_attempts - row.attempts - 1 {
            0 => CodeCheck::Gone,
            left => CodeCheck::Wrong(left),
        }
    };
    tx.commit().await?;
    Ok(check)
}

/// The account that owns a phone number or e-mail address.
pub struct Account {
    pub id: Uuid,
    /// PENDING (created, contact not yet confirmed), ACTIVE or SUSPENDED.
    pub status: String,
}

/// The account for this phone number or e-mail address, created if there is none: PENDING, with
/// this name, until its contact is confirmed (`confirm_account`).
pub async fn account_for_contact(
    db: &PgPool,
    phone: Option<&str>,
    email: Option<&str>,
    name: Option<&str>,
) -> sqlx::Result<Account> {
    if let Some(account) = account_by_contact(db, phone, email).await? {
        return Ok(account);
    }
    let subject = match (phone, email) {
        (Some(phone), _) => format!("phone:{phone}"),
        (None, Some(email)) => format!("email:{email}"),
        (None, None) => unreachable!("an account always has a phone number or an e-mail address"),
    };
    // Two first sign-ins at once: one insert wins, and both find its row.
    sqlx::query!(
        "INSERT INTO users (auth_subject, phone_e164, email, display_name, status)
         VALUES ($1, $2::text::e164_phone, $3::text::citext, coalesce($4, 'Përdorues'), 'PENDING')
         ON CONFLICT DO NOTHING",
        subject,
        phone,
        email,
        name,
    )
    .execute(db)
    .await?;
    account_by_contact(db, phone, email).await?.ok_or(sqlx::Error::RowNotFound)
}

/// The contact is confirmed: a new account becomes active, and takes the name given when it was
/// created if it has none yet.
pub async fn confirm_account(db: &PgPool, id: Uuid, name: Option<&str>) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE users
         SET status = CASE WHEN status = 'PENDING' THEN 'ACTIVE' ELSE status END,
             display_name = CASE WHEN display_name = 'Përdorues' THEN coalesce($2, display_name) ELSE display_name END
         WHERE id = $1",
        id,
        name,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// The account this phone number or e-mail address has, if any.
pub async fn account_by_contact(db: &PgPool, phone: Option<&str>, email: Option<&str>) -> sqlx::Result<Option<Account>> {
    sqlx::query_as!(
        Account,
        "SELECT id, status FROM users WHERE (phone_e164 = $1 OR email = $2::text::citext) AND status <> 'DELETED'",
        phone,
        email,
    )
    .fetch_optional(db)
    .await
}

/// The account a listing posted without signing in goes under: the one its phone number or e-mail
/// address has, or a new one, active at once (posting needs no code), under the name typed or else
/// "Përdorues".
pub async fn poster_account(db: &PgPool, phone: Option<&str>, email: Option<&str>, name: Option<&str>) -> sqlx::Result<Account> {
    let subject = match (phone, email) {
        (Some(phone), _) => format!("phone:{phone}"),
        (None, Some(email)) => format!("email:{email}"),
        (None, None) => unreachable!("an account always has a phone number or an e-mail address"),
    };
    // Two first posts at once: one insert makes it, and the other finds it.
    let created = sqlx::query_as!(
        Account,
        "INSERT INTO users (auth_subject, phone_e164, email, display_name, status)
         VALUES ($1, $2::text::e164_phone, $3::text::citext, coalesce($4, 'Përdorues'), 'ACTIVE')
         ON CONFLICT DO NOTHING
         RETURNING id, status",
        subject,
        phone,
        email,
        name,
    )
    .fetch_optional(db)
    .await?;
    match created {
        Some(account) => Ok(account),
        None => account_by_contact(db, phone, email).await?.ok_or(sqlx::Error::RowNotFound),
    }
}

/// Publishes a listing posted before signing in, once its seller confirmed their contact;
/// returns its address.
pub async fn publish_draft(db: &PgPool, listing_id: Uuid, user_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!(
        r#"UPDATE listings SET status = 'ACTIVE', published_at = now()
           WHERE id = $1 AND created_by_user_id = $2 AND status = 'DRAFT'
           RETURNING '/listing/' || public_no || '-' || slug AS "url!""#,
        listing_id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

pub async fn create_session(db: &PgPool, token_hash: &[u8], user_id: Uuid, days: i32) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES ($1, $2, now() + make_interval(days => $3))",
        token_hash,
        user_id,
        days,
    )
    .execute(db)
    .await
    .map(|_| ())
}

/// The signed-in user, with what they signed in with.
pub struct SessionUser {
    pub id: Uuid,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub display_name: String,
}

/// The user a session token belongs to, unless it expired or the account is closed or suspended.
pub async fn session_user(db: &PgPool, token_hash: &[u8]) -> sqlx::Result<Option<SessionUser>> {
    sqlx::query_as!(
        SessionUser,
        r#"SELECT u.id, u.phone_e164::text AS phone, u.email::text AS email, u.display_name
           FROM sessions s
           JOIN users u ON u.id = s.user_id
           WHERE s.token_hash = $1 AND s.expires_at > now() AND u.status = 'ACTIVE'"#,
        token_hash,
    )
    .fetch_optional(db)
    .await
}

pub async fn delete_session(db: &PgPool, token_hash: &[u8]) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM sessions WHERE token_hash = $1", token_hash).execute(db).await.map(|_| ())
}

// ---------------------------------------------------------------- posting

/// How many listings the user posted in the last day, and whether one of them has this title.
pub struct RecentPosts {
    pub count: i64,
    pub same_title: bool,
}

pub async fn recent_posts(db: &PgPool, user_id: Uuid, title: &str) -> sqlx::Result<RecentPosts> {
    sqlx::query_as!(
        RecentPosts,
        r#"SELECT count(*) AS "count!",
                  coalesce(bool_or(lower(title) = lower($2)), false) AS "same_title!"
           FROM listings
           WHERE created_by_user_id = $1 AND created_at > now() - interval '1 day'"#,
        user_id,
        title,
    )
    .fetch_one(db)
    .await
}

pub struct NewListing<'a> {
    pub user_id: Uuid,
    pub category_id: Uuid,
    pub location_id: Uuid,
    pub title: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub listing_type: &'a str,
    pub condition: Option<&'a str>,
    pub price_amount: Option<Decimal>,
    pub currency: &'a str,
    pub price_type: &'a str,
    pub contact_name: Option<&'a str>,
    pub phone: Option<&'a str>,
    pub email: Option<&'a str>,
    /// The category's properties as the seller filled them in.
    pub properties: &'a [ListingProperty],
    /// Published at once, or kept as a draft until the seller confirms their contact.
    pub publish: bool,
}

/// A property's value on a listing, as the form gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Option(Uuid),
    /// An option the seller typed ("Tjetër"): added, inactive, unless one with its code exists.
    /// A model goes under the make chosen (or typed) for the parent property.
    NewOption { label: String, code: String, parent_attribute_id: Option<Uuid> },
    Options(Vec<Uuid>),
    Number(Decimal),
    Boolean(bool),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListingProperty {
    pub attribute_id: Uuid,
    pub value: PropertyValue,
}

/// Replaces the listing's property values with the form's, plus its subcategory's kind (the old
/// "type" property, migration 0008), which search still finds it by.
async fn save_properties(
    tx: &mut sqlx::PgConnection,
    listing_id: Uuid,
    listing: &NewListing<'_>,
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM listing_attribute_values WHERE listing_id = $1", listing_id).execute(&mut *tx).await?;
    sqlx::query!("DELETE FROM listing_attribute_options WHERE listing_id = $1", listing_id).execute(&mut *tx).await?;
    sqlx::query!(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
         SELECT $1, a.id, a.data_type, o.id
         FROM categories c
         JOIN attribute_options o ON o.id = c.replaces_option_id
         JOIN attributes a ON a.id = o.attribute_id
         WHERE c.id = $2",
        listing_id,
        listing.category_id,
    )
    .execute(&mut *tx)
    .await?;
    // The option saved for each property so far: a typed model goes under the make's.
    let mut saved: HashMap<Uuid, Uuid> = HashMap::new();
    for property in listing.properties {
        let attribute = property.attribute_id;
        let option = match &property.value {
            PropertyValue::Option(option) => Some(*option),
            PropertyValue::NewOption { label, code, parent_attribute_id } => {
                let parent = parent_attribute_id.and_then(|parent| saved.get(&parent).copied());
                Some(
                    sqlx::query_scalar!(
                        r#"WITH code AS (
                               SELECT coalesce((SELECT value_code || '_' FROM attribute_options WHERE id = $2), '') || $3
                                      AS value_code
                           ), added AS (
                               INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, active,
                                                              added_by_user_id, sort_order)
                               SELECT $1, $2, value_code, $4, false, $5, 1000 FROM code
                               ON CONFLICT (attribute_id, value_code) DO NOTHING
                               RETURNING id
                           )
                           SELECT id AS "id!" FROM added
                           UNION ALL
                           SELECT o.id FROM attribute_options o, code
                           WHERE o.attribute_id = $1 AND o.value_code = code.value_code
                           LIMIT 1"#,
                        attribute,
                        parent,
                        code,
                        label,
                        listing.user_id,
                    )
                    .fetch_one(&mut *tx)
                    .await?,
                )
            }
            _ => None,
        };
        if let Some(option) = option {
            saved.insert(attribute, option);
            sqlx::query!(
                "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
                 VALUES ($1, $2, 'SELECT', $3)",
                listing_id,
                attribute,
                option,
            )
            .execute(&mut *tx)
            .await?;
            continue;
        }
        match &property.value {
            PropertyValue::Option(_) | PropertyValue::NewOption { .. } => {}
            PropertyValue::Options(options) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
                     SELECT $1, $2, unnest($3::uuid[])",
                    listing_id,
                    attribute,
                    options,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Number(number) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
                     VALUES ($1, $2, 'NUMBER', $3)",
                    listing_id,
                    attribute,
                    number,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Boolean(value) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_boolean)
                     VALUES ($1, $2, 'BOOLEAN', $3)",
                    listing_id,
                    attribute,
                    value,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Text(text) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text)
                     VALUES ($1, $2, 'TEXT', $3)",
                    listing_id,
                    attribute,
                    text,
                )
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    Ok(())
}

/// Saves a listing at its town's point on the map, with the seller's contact, published at once
/// or as a draft (`publish`), and returns its id and number.
pub async fn create_listing(db: &PgPool, listing: &NewListing<'_>) -> sqlx::Result<(Uuid, i64)> {
    let mut tx = db.begin().await?;
    let row = sqlx::query!(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title,
                               description, listing_type, condition, price_amount, currency, price_type,
                               price_normalized, latitude, longitude, status, published_at)
         SELECT $1, $1, $2, loc.id, $4, $5, $6, $7, $8, $9, $10::text::currency_code, $11,
                price_in_eur($9, $10::text::currency_code), loc.latitude, loc.longitude,
                CASE WHEN $12 THEN 'ACTIVE' ELSE 'DRAFT' END, CASE WHEN $12 THEN now() END
         FROM locations loc
         WHERE loc.id = $3
         RETURNING id, public_no",
        listing.user_id,
        listing.category_id,
        listing.location_id,
        listing.slug,
        listing.title,
        listing.description,
        listing.listing_type,
        listing.condition,
        listing.price_amount,
        listing.currency,
        listing.price_type,
        listing.publish,
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO listing_contacts (listing_id, contact_name, phone_e164, email, show_phone, show_email)
         VALUES ($1, $2, $3::text::e164_phone, $4::text::citext, $3 IS NOT NULL, $4 IS NOT NULL)",
        row.id,
        listing.contact_name,
        listing.phone,
        listing.email,
    )
    .execute(&mut *tx)
    .await?;
    save_properties(&mut tx, row.id, listing).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = $1",
        row.id
    )
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((row.id, row.public_no))
}

/// One of the user's own listings, as the edit form shows it. Only listings they created, and only
/// while they are up (active or reserved), can be edited.
pub struct OwnListing {
    pub id: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub category: String,
    pub listing_type: String,
    pub condition: Option<String>,
    pub title: String,
    pub description: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    /// The municipality's slug: the form picks towns, and a listing may sit in a smaller place.
    pub city: Option<String>,
    pub contact_name: Option<String>,
}

pub async fn own_listing(db: &PgPool, public_no: i64, user_id: Uuid) -> sqlx::Result<Option<OwnListing>> {
    sqlx::query_as!(
        OwnListing,
        r#"SELECT l.id, l.public_no, l.slug, c.slug AS category, l.listing_type, l.condition, l.title, l.description,
                  l.price_amount, l.currency::text AS "currency!", l.price_type,
                  (SELECT m.slug FROM locations m WHERE m.id = location_municipality(l.location_id)) AS city,
                  lc.contact_name
           FROM listings l
           JOIN categories c ON c.id = l.category_id
           LEFT JOIN listing_contacts lc ON lc.listing_id = l.id
           WHERE l.public_no = $1 AND l.created_by_user_id = $2 AND l.status IN ('ACTIVE', 'RESERVED')"#,
        public_no,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Saves an edit, if the listing is the user's and still up; returns whether it did. A listing
/// keeps its point on the map unless it moves to another town, and its place in "newest first".
pub async fn update_listing(db: &PgPool, public_no: i64, listing: &NewListing<'_>) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let Some(id) = sqlx::query_scalar!(
        "UPDATE listings l
         SET category_id = $3, slug = $5, title = $6, description = $7, listing_type = $8, condition = $9,
             price_amount = $10, currency = $11::text::currency_code, price_type = $12,
             price_normalized = price_in_eur($10, $11::text::currency_code),
             latitude = CASE WHEN l.location_id = $4 THEN l.latitude ELSE loc.latitude END,
             longitude = CASE WHEN l.location_id = $4 THEN l.longitude ELSE loc.longitude END,
             location_id = $4
         FROM locations loc
         WHERE loc.id = $4 AND l.public_no = $1 AND l.created_by_user_id = $2 AND l.status IN ('ACTIVE', 'RESERVED')
         RETURNING l.id",
        public_no,
        listing.user_id,
        listing.category_id,
        listing.location_id,
        listing.slug,
        listing.title,
        listing.description,
        listing.listing_type,
        listing.condition,
        listing.price_amount,
        listing.currency,
        listing.price_type,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query!("UPDATE listing_contacts SET contact_name = $2 WHERE listing_id = $1", id, listing.contact_name)
        .execute(&mut *tx)
        .await?;
    save_properties(&mut tx, id, listing).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = $1",
        id
    )
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// Whether the user created this listing, so its page offers them the edit button.
pub async fn is_own_listing(db: &PgPool, listing_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM listings WHERE id = $1 AND created_by_user_id = $2) AS "own!""#,
        listing_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// What a seller can do to their own listing, besides editing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingAction {
    /// Take it down for a while (PAUSED): nobody else sees it until it is put back.
    Pause,
    /// Put a paused listing back up.
    Resume,
    /// Take it down for good (DELETED).
    Delete,
}

impl ListingAction {
    /// The statuses the action applies to, and the one it leaves.
    fn statuses(self) -> (&'static [&'static str], &'static str) {
        match self {
            Self::Pause => (&["ACTIVE", "RESERVED"], "PAUSED"),
            Self::Resume => (&["PAUSED"], "ACTIVE"),
            Self::Delete => (&["DRAFT", "ACTIVE", "RESERVED", "SOLD", "EXPIRED", "PAUSED"], "DELETED"),
        }
    }
}

/// Pauses, resumes or deletes one of the user's listings. False when the listing isn't theirs, or
/// isn't in a state the action applies to (a deleted one, or pausing one already paused). The
/// status history records the seller as the one who changed it.
pub async fn change_own_listing(db: &PgPool, public_no: i64, user_id: Uuid, action: ListingAction) -> sqlx::Result<bool> {
    let (from, to) = action.statuses();
    let from: Vec<String> = from.iter().map(|status| status.to_string()).collect();
    let mut tx = db.begin().await?;
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", user_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    let changed = sqlx::query!(
        "UPDATE listings SET status = $3, updated_at = now()
         WHERE public_no = $1 AND created_by_user_id = $2 AND status::text = ANY($4)",
        public_no,
        user_id,
        to,
        &from,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    tx.commit().await?;
    Ok(changed)
}

/// A row on "Njoftimet e mia".
pub struct MyListing {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub listing_type: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
}

/// The user's listings, newest first, deleted ones left out.
pub async fn my_listings(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<MyListing>> {
    sqlx::query_as!(
        MyListing,
        r#"SELECT public_no, slug, coalesce(generated_title, title) AS "title!", status, listing_type, price_amount,
                  currency::text AS "currency!", price_type
           FROM listings
           WHERE created_by_user_id = $1 AND status <> 'DELETED'
           ORDER BY created_at DESC, public_no DESC"#,
        user_id,
    )
    .fetch_all(db)
    .await
}

pub async fn ping(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query("SELECT 1").execute(db).await.map(|_| ())
}

// ---------------------------------------------------------------- what sellers typed in

/// A make, model or brand a seller typed in ("Tjetër") that waits for a moderator (/moderim).
pub struct PendingOption {
    pub id: Uuid,
    /// The property's own name: "Marka", "Modeli".
    pub attribute: String,
    pub label: String,
    /// A model's make.
    pub parent_label: Option<String>,
    /// The make waits too, and goes first.
    pub parent_pending: bool,
    pub added_by: Option<String>,
    /// The listings that have it, deleted ones aside.
    pub listings: i64,
}

/// What sellers typed in and nobody has approved yet: makes before models.
pub async fn pending_options(db: &PgPool) -> sqlx::Result<Vec<PendingOption>> {
    sqlx::query_as!(
        PendingOption,
        r#"SELECT o.id, a.name AS attribute, o.label, parent.label AS "parent_label?",
                  coalesce(NOT parent.active, false) AS "parent_pending!", u.display_name AS "added_by?",
                  (SELECT count(*) FROM listing_attribute_values v JOIN listings l ON l.id = v.listing_id
                   WHERE v.value_option_id = o.id AND l.status <> 'DELETED') AS "listings!"
           FROM attribute_options o
           JOIN attributes a ON a.id = o.attribute_id
           LEFT JOIN attribute_options parent ON parent.id = o.parent_option_id
           LEFT JOIN users u ON u.id = o.added_by_user_id
           WHERE NOT o.active AND o.added_by_user_id IS NOT NULL
           ORDER BY o.parent_option_id IS NOT NULL, a.name, lower(parent.label), lower(o.label)"#
    )
    .fetch_all(db)
    .await
}

/// How many wait, for the link to /moderim.
pub async fn pending_option_count(db: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM attribute_options WHERE NOT active AND added_by_user_id IS NOT NULL"#
    )
    .fetch_one(db)
    .await
}

/// A listed option a waiting one can be merged into: the same property, under the same make.
pub struct MergeTarget {
    pub pending_id: Uuid,
    pub id: Uuid,
    pub label: String,
    /// How alike the two names are, 0 to 1 (pg_trgm): "Mercedes" and Mercedes-Benz are.
    pub similarity: f32,
}

pub async fn merge_targets(db: &PgPool) -> sqlx::Result<Vec<MergeTarget>> {
    sqlx::query_as!(
        MergeTarget,
        r#"SELECT o.id AS pending_id, t.id, t.label, similarity(lower(t.label), lower(o.label)) AS "similarity!"
           FROM attribute_options o
           JOIN attribute_options t
             ON t.attribute_id = o.attribute_id AND t.active AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
           WHERE NOT o.active AND o.added_by_user_id IS NOT NULL
           ORDER BY o.id, t.sort_order, t.label"#
    )
    .fetch_all(db)
    .await
}

pub enum Approval {
    Approved,
    /// A listed option of the same property and make has that name already: merge, rather.
    Taken,
    /// It doesn't wait (any more), or its make still does.
    NotPending,
}

/// Approves a typed option under `label`, its spelling put right: the lists and filters offer it
/// from now on, after the listed names that come before it alphabetically.
pub async fn approve_option(db: &PgPool, id: Uuid, label: &str) -> sqlx::Result<Approval> {
    let mut tx = db.begin().await?;
    let taken = sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM attribute_options o
               JOIN attribute_options t
                 ON t.attribute_id = o.attribute_id AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
                AND t.active AND t.id <> o.id AND lower(t.label) = lower($2)
               WHERE o.id = $1
           ) AS "taken!""#,
        id,
        label,
    )
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Ok(Approval::Taken);
    }
    let approved = sqlx::query_scalar!(
        "UPDATE attribute_options o
         SET active = true, label = $2::text,
             sort_order = coalesce((SELECT max(s.sort_order) FROM attribute_options s
                                    WHERE s.attribute_id = o.attribute_id AND s.active
                                      AND s.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
                                      AND lower(s.label) < lower($2::text)), 0) + 1
         WHERE o.id = $1 AND NOT o.active AND o.added_by_user_id IS NOT NULL
           AND (o.parent_option_id IS NULL
                OR EXISTS (SELECT 1 FROM attribute_options p WHERE p.id = o.parent_option_id AND p.active))
         RETURNING o.id",
        id,
        label,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if approved.is_none() {
        return Ok(Approval::NotPending);
    }
    // Search reads the names from search_extra (listing_search_extra), which has the old spelling,
    // and a car's title from generated_title.
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id IN (SELECT listing_id FROM listing_attribute_values WHERE value_option_id = $1)",
        id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Approval::Approved)
}

/// Merges a typed option into a listed one of the same property and make, the one the seller
/// meant: its listings get the listed one, a typed make's typed models move under it, and the
/// typed one is gone. False when either isn't what it should be.
pub async fn merge_option(db: &PgPool, id: Uuid, into: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let pending = sqlx::query_scalar!(
        "SELECT o.id FROM attribute_options o
         JOIN attribute_options t
           ON t.id = $2 AND t.active AND t.attribute_id = o.attribute_id
          AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
         WHERE o.id = $1 AND NOT o.active AND o.added_by_user_id IS NOT NULL
         FOR UPDATE OF o",
        id,
        into,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if pending.is_none() {
        return Ok(false);
    }
    let listings = sqlx::query_scalar!(
        "UPDATE listing_attribute_values SET value_option_id = $2 WHERE value_option_id = $1 RETURNING listing_id",
        id,
        into,
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!("UPDATE attribute_options SET parent_option_id = $2 WHERE parent_option_id = $1", id, into)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM attribute_options WHERE id = $1", id).execute(&mut *tx).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = ANY($1)",
        &listings,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_survive_a_round_trip() {
        let id = Uuid::from_u128(0x0199_a1b2_c3d4_7e5f_8a6b_7c8d_9e0f_1a2b);
        let newest = Cursor::Newest {
            sort_at: OffsetDateTime::from_unix_timestamp_nanos(1_790_000_000_123_456_000).unwrap(),
            id,
        };
        assert_eq!(Cursor::decode(Sort::Newest, &newest.encode()), Some(newest));

        let priced = Cursor::Price { price: Some(Decimal::new(1_250_050, 2)), id };
        assert_eq!(priced.encode(), format!("12500.50_{id}"));
        assert_eq!(Cursor::decode(Sort::PriceLow, &priced.encode()), Some(priced));

        let unpriced = Cursor::Price { price: None, id };
        assert_eq!(Cursor::decode(Sort::PriceHigh, &unpriced.encode()), Some(unpriced));

        assert_eq!(Cursor::decode(Sort::Newest, "garbage"), None);
        assert_eq!(Cursor::decode(Sort::PriceLow, &format!("abc_{id}")), None);

        // Distances keep every digit, so the next page starts exactly after the last listing.
        let near = Cursor::Distance { meters: 2_345.678_901_234_567, id };
        assert_eq!(Cursor::decode(Sort::Nearest, &near.encode()), Some(near));
        assert_eq!(Cursor::decode(Sort::Nearest, &format!("inf_{id}")), None);
    }
}
