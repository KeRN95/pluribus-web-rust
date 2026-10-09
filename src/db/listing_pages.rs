//! Pages of listing cards: the browse pages' filters, orders and pages, the rows under a listing,
//! the most seen, counts, and the map's points.

use super::*;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ListingCard {
    pub id: Uuid,
    /// The account that posted it (a business's, one of its people): the rows of cards leave out
    /// those of the accounts the visitor blocked (web/safety.rs).
    #[sqlx(default)]
    pub created_by_user_id: Uuid,
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
    /// The zone in it the listing says it is in (Blloku), if any.
    pub zone: Option<String>,
    /// How far the listing is from the map pin, in metres, when the search has one.
    pub distance_m: Option<f64>,
    /// The listing's photos, the cover first: their card-sized copies, and the full ones in the
    /// same order, as every browser and link preview shows them (a photo kept as WebP has a JPEG
    /// copy for them). A small card shows the first; a big one slides through them all.
    pub photos: Vec<String>,
    pub photos_full: Vec<String>,
    /// In the same order, the WebP copies of each photo kept as WebP (since migration 0025), for
    /// the browser to pick from; None for a photo kept before, as JPEG, or from elsewhere.
    #[sqlx(json)]
    pub photos_webp: Vec<Option<WebpCopies>>,
    /// The seller's name, and since when they are on the site, which a big card says over its
    /// photo.
    pub seller_name: String,
    pub seller_since: OffsetDateTime,
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
    /// How many times its page was seen this week, on the cards of "Më të klikuarat" alone
    /// (most_viewed), which show it over the photo.
    #[sqlx(default)]
    pub views: Option<i64>,
    /// The price it dropped from, when it did lately (price_was! in mod.rs): the card says by how
    /// much beside its price.
    #[sqlx(default)]
    pub price_was: Option<Decimal>,
    /// The start of its description, its spaces and lines run together, one character past
    /// DESCRIPTION_EXCERPT so that a longer one shows it was cut: the list's cards show it under
    /// the title (description_excerpt).
    #[sqlx(default)]
    pub description: String,
}

/// How much of a description a card carries, in characters.
pub const DESCRIPTION_EXCERPT: usize = 400;

/// A photo's WebP copies (media.rs), as a card has them: the card's (the small one's address is
/// it with its own ending, web/photos.rs), the middle one (none for a photo no bigger), the full
/// one and its size, and the tiny one as a data: URI.
#[derive(Debug, Clone, Deserialize)]
pub struct WebpCopies {
    pub card: String,
    pub middle: Option<String>,
    pub full: String,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub placeholder: Option<String>,
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
    /// The distance between the pin and the listing, in metres, straight through the earth (the
    /// cube's <->), which differs from the way over its surface by less than 15 cm at the slider's
    /// 50 km. The location indexes give listings nearest first by it (KNN): the listing's side
    /// comes first, written exactly as in idx_listings_active_earth and
    /// idx_listings_active_category_earth.
    fn push_distance(&self, query: &mut QueryBuilder<'_, Postgres>) {
        query
            .push("(ll_to_earth(l.latitude::float8, l.longitude::float8) <-> ll_to_earth(")
            .push_bind(self.latitude)
            .push(", ")
            .push_bind(self.longitude)
            .push("))");
    }
}

#[derive(Debug, Default)]
pub struct ListingFilter {
    /// One seller's listings, for their profile.
    pub seller: Option<Seller>,
    /// The accounts the visitor blocked, whose listings are left out (web/safety.rs).
    pub hidden_accounts: Vec<Uuid>,
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
/// finds the Golfs, "makinat" the listings that say "makinë" (search_query, migration 0016). A
/// word that names a kind of listing finds the listings of that kind, as well as those that say
/// it: "golf në shitje" the Golfs for sale, which seldom say so (migration 0024).
#[derive(Debug, Clone)]
pub struct TextSearch {
    /// The tsquery listings are matched against, as text; None where the words only named a kind
    /// of listing ("në shitje").
    query: Option<String>,
    /// The words searched in place of misspelt ones ("golf" for "golff"), in the order typed.
    pub corrections: Vec<String>,
    /// The kinds of listing the words named (SELL for "shitje"), and those words with their forms,
    /// as a tsquery: a listing found is of one of the kinds, or matches it.
    kinds: Vec<String>,
    kind_query: Option<String>,
}

/// The search for these words, or None without any. Pages work it out once, before the listing
/// queries, and those take it as a parameter: worked out inside them, it would be hidden from the
/// planner, and a search that finds nothing would walk the whole newest-first index.
pub async fn text_search(db: &PgPool, words: Option<&str>) -> sqlx::Result<Option<TextSearch>> {
    let Some(words) = words else {
        return Ok(None);
    };
    let row = sqlx::query!(
        r#"SELECT query::text AS "query?", corrections AS "corrections!", kinds AS "kinds!",
                  kind_query::text AS "kind_query?"
           FROM search_query($1)"#,
        words,
    )
    .fetch_one(db)
    .await?;
    Ok(Some(TextSearch { query: row.query, corrections: row.corrections, kinds: row.kinds, kind_query: row.kind_query }))
}

/// Rebuilds the list of words listings use, which search takes typos' corrections and words'
/// other forms from, so that it follows the listings. Searches go on reading the old list
/// meanwhile.
pub async fn refresh_search_words(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query!("REFRESH MATERIALIZED VIEW CONCURRENTLY search_words").execute(db).await?;
    Ok(())
}

/// Rebuilds what a square metre costs in each zone, by category (zone_area_prices), which a
/// property's page compares its own price per square metre with, so that it follows the listings.
/// Pages go on reading the old figures meanwhile.
pub async fn refresh_zone_prices(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query!("REFRESH MATERIALIZED VIEW CONCURRENTLY zone_area_prices").execute(db).await?;
    Ok(())
}

/// What a word being typed may become: the words listings use that start with `start`, the most
/// used first and one of each stem ("apart" gives "apartament", not "apartamente" too). `start`
/// is folded (format::fold) and letters only.
pub async fn words_starting_with(db: &PgPool, start: &str, limit: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        r#"SELECT word AS "word!"
           FROM (SELECT DISTINCT ON (stem) word, listings
                 FROM search_words
                 WHERE word LIKE $1 || '%'
                 ORDER BY stem, listings DESC, word) words
           ORDER BY listings DESC, word
           LIMIT $2"#,
        start,
        limit,
    )
    .fetch_all(db)
    .await
}

/// The common word a folded word most likely misspells, when it seems misspelt ("golff": "golf").
pub async fn spelling(db: &PgPool, word: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar!(r#"SELECT search_correction($1) AS "word""#, word).fetch_one(db).await
}

/// A make, model or brand, from the lists sellers can add to (attributes.allow_other), that a
/// search names.
#[derive(Debug)]
pub struct NamedOption {
    /// "Volkswagen Golf": a model with its make.
    pub name: String,
    /// The category most of the active listings with it are in.
    pub category_slug: String,
    pub category_name: String,
    /// The filters that find them there, as query-string pairs: vehicle_make=VOLKSWAGEN and
    /// vehicle_model=VOLKSWAGEN_GOLF.
    pub filters: Vec<(String, String)>,
}

/// The makes, models and brands whose names have a word starting with each of `starts` (folded),
/// that active listings have, the most used first.
pub async fn named_options(db: &PgPool, starts: &[String], limit: i64) -> sqlx::Result<Vec<NamedOption>> {
    let rows = sqlx::query!(
        r#"WITH named AS MATERIALIZED (
               SELECT o.id, o.attribute_id, concat_ws(' ', p.label, o.label) AS name,
                      CASE WHEN p.id IS NULL THEN jsonb_build_array(jsonb_build_array(a.code, o.value_code))
                           ELSE jsonb_build_array(jsonb_build_array(pa.code, p.value_code),
                                                  jsonb_build_array(a.code, o.value_code))
                      END AS filters
               FROM attribute_options o
               JOIN attributes a ON a.id = o.attribute_id AND a.allow_other
               LEFT JOIN attribute_options p ON p.id = o.parent_option_id
               LEFT JOIN attributes pa ON pa.id = p.attribute_id
               WHERE o.active
                 AND NOT EXISTS (SELECT 1 FROM unnest($1::text[]) AS s (start)
                                 WHERE lower(immutable_unaccent(concat_ws(' ', p.label, o.label)))
                                       !~ ('(^|[^a-z0-9])' || s.start))
           )
           SELECT named.name AS "name!", c.slug AS "category_slug!", c.name AS "category_name!",
                  named.filters AS "filters!: Json<Vec<(String, String)>>"
           FROM named
           CROSS JOIN LATERAL (
               SELECT l.category_id, count(*) AS listings
               FROM listing_attribute_values v
               JOIN listings l ON l.id = v.listing_id AND l.status = 'ACTIVE'
               WHERE v.attribute_id = named.attribute_id AND v.value_option_id = named.id
               GROUP BY l.category_id
               ORDER BY listings DESC, l.category_id
               LIMIT 1
           ) used
           JOIN categories c ON c.id = used.category_id
           ORDER BY used.listings DESC, named.name
           LIMIT $2"#,
        starts,
        limit,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| NamedOption {
            name: row.name,
            category_slug: row.category_slug,
            category_name: row.category_name,
            filters: row.filters.0,
        })
        .collect())
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
    /// What of theirs is verified (user_verifications): PHONE, the number they signed in with,
    /// or EMAIL; a business's public number when one of its people signed in with it.
    pub verified: Option<String>,
}

pub async fn user_profile(db: &PgPool, id: Uuid) -> sqlx::Result<Option<Profile>> {
    let row = sqlx::query!(
        r#"SELECT u.display_name, u.created_at,
                  (SELECT count(*) FROM listings l WHERE l.seller_user_id = u.id AND l.status = 'ACTIVE') AS "listings!",
                  (SELECT v.verification_type FROM user_verifications v
                   WHERE v.user_id = u.id AND v.status = 'VERIFIED'
                     AND ((v.verification_type = 'PHONE' AND v.verified_value = u.phone_e164::text)
                          OR (v.verification_type = 'EMAIL' AND lower(v.verified_value) = lower(u.email::text)))
                   ORDER BY v.verification_type = 'PHONE' DESC
                   LIMIT 1) AS verified
           FROM users u WHERE u.id = $1 AND u.status = 'ACTIVE'"#,
        id,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| Profile {
        seller: Seller::User(id),
        name: row.display_name,
        since: row.created_at,
        listings: row.listings,
        verified: row.verified,
    }))
}

pub async fn business_profile(db: &PgPool, slug: &str) -> sqlx::Result<Option<Profile>> {
    let row = sqlx::query!(
        r#"SELECT b.id, b.display_name, b.created_at,
                  (SELECT count(*) FROM listings l WHERE l.seller_business_id = b.id AND l.status = 'ACTIVE') AS "listings!",
                  (SELECT v.verification_type FROM business_members m
                   JOIN user_verifications v ON v.user_id = m.user_id
                   WHERE m.business_id = b.id AND v.status = 'VERIFIED' AND v.verification_type = 'PHONE'
                     AND v.verified_value = b.public_phone::text
                   LIMIT 1) AS verified
           FROM businesses b WHERE b.slug = $1 AND b.status = 'ACTIVE'"#,
        slug,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| Profile {
        seller: Seller::Business(row.id),
        name: row.display_name,
        since: row.created_at,
        listings: row.listings,
        verified: row.verified,
    }))
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

/// What a card shows (ListingCard) of each listing a query reads as `l`, but for its distance_m,
/// which the query says before FROM: the WITH clause and select list shared by a page of cards and
/// a listing's related ones.
pub(super) const CARD_COLUMNS: &str = concat!("WITH card_attributes AS MATERIALIZED (
             SELECT category_id, attribute_id, sort_order
             FROM effective_category_attributes
             WHERE show_in_card
         ),
         spare_attributes AS MATERIALIZED (
             SELECT category_id, attribute_id, sort_order
             FROM effective_category_attributes
             WHERE show_in_details AND NOT show_in_card
         )
         SELECT l.id, l.created_by_user_id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.listing_type, l.price_amount,
                l.currency::text AS currency, l.price_type, l.price_normalized,
                loc.name AS location_name,
                CASE WHEN loc.location_type = 'AREA' THEN loc.name END AS zone,
                (SELECT t.name FROM locations t
                 WHERE t.id = location_municipality(l.location_id)) AS city,
                coalesce((SELECT array_agg(coalesce(m.thumbnail_url, m.public_url) ORDER BY m.is_cover DESC, m.sort_order)
                          FROM listing_media m
                          WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                            AND coalesce(m.public_url, m.thumbnail_url) IS NOT NULL), '{}') AS photos,
                coalesce((SELECT array_agg(coalesce(m.jpeg_url, m.public_url, m.thumbnail_url) ORDER BY m.is_cover DESC, m.sort_order)
                          FROM listing_media m
                          WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                            AND coalesce(m.public_url, m.thumbnail_url) IS NOT NULL), '{}') AS photos_full,
                coalesce((SELECT json_agg(CASE WHEN m.mime_type = 'image/webp' THEN
                                     json_build_object('card', coalesce(m.thumbnail_url, m.public_url), 'middle', m.middle_url,
                                                       'full', m.public_url, 'width', m.width_px, 'height', m.height_px,
                                                       'placeholder', m.placeholder)
                                 END ORDER BY m.is_cover DESC, m.sort_order)
                          FROM listing_media m
                          WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                            AND coalesce(m.public_url, m.thumbnail_url) IS NOT NULL), '[]') AS photos_webp,
                coalesce((SELECT b.display_name FROM businesses b WHERE b.id = l.seller_business_id),
                         (SELECT u.display_name FROM users u WHERE u.id = l.seller_user_id), '') AS seller_name,
                coalesce((SELECT b.created_at FROM businesses b WHERE b.id = l.seller_business_id),
                         (SELECT u.created_at FROM users u WHERE u.id = l.seller_user_id), l.created_at) AS seller_since,
                l.sort_at,
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
                left(btrim(regexp_replace(coalesce(l.description, ''), '\\s+', ' ', 'g')), 401) AS description,
                ", price_was!(), " AS price_was,
                ");

/// One page of active listings in the filter's order, with at most PAGE_SIZE + 1 rows.
pub async fn listing_page(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    match &filter.near {
        Some(near) => near.push_distance(&mut query),
        None => {
            query.push("NULL::float8");
        }
    }
    query.push(" AS distance_m FROM ");
    push_ordered(&mut query, filter, filter.after, false, PAGE_SIZE + 1);
    query.push(" LEFT JOIN locations loc ON loc.id = l.location_id");
    push_order(&mut query, filter.sort, filter.near.as_ref());
    query.push(" LIMIT ").push_bind(PAGE_SIZE + 1);
    query.build_query_as().fetch_all(db).await
}

/// Where the newest active listings stand in the newest-first order (sort_at, then the id), the
/// newest first: what the welcome page's row of the newest listings compares with what it saw
/// before, to find those posted since (web/newest.rs).
pub async fn newest_listing_keys(db: &PgPool, limit: i64) -> sqlx::Result<Vec<(OffsetDateTime, Uuid)>> {
    sqlx::query_as(
        "SELECT sort_at, id FROM listings WHERE status = 'ACTIVE' ORDER BY sort_at DESC, id DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(db)
    .await
}

/// The cards of those of these listings that are active, the oldest first.
pub async fn cards_by_id(db: &PgPool, ids: &[Uuid]) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push(
        "NULL::float8 AS distance_m
         FROM (SELECT l.* FROM listings l WHERE l.status = 'ACTIVE' AND l.id = ANY(",
    );
    query.push_bind(ids.to_vec());
    query.push(")) l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.sort_at, l.id");
    query.build_query_as().fetch_all(db).await
}


/// How many of the seller's other listings a listing's page shows, over the related ones.
pub const FROM_SELLER: i64 = 4;

/// The seller's other active listings, newest first as their profile lists them, for under a
/// listing's page: at most `limit`.
pub async fn seller_listings(db: &PgPool, listing_id: Uuid, limit: i64) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push(
        "NULL::float8 AS distance_m
         FROM (
             SELECT o.*
             FROM listings t
             JOIN listings o ON o.seller_user_id = t.seller_user_id OR o.seller_business_id = t.seller_business_id
             WHERE t.id = ",
    );
    query.push_bind(listing_id);
    query.push(
        " AND o.status = 'ACTIVE' AND o.id <> t.id
             ORDER BY o.sort_at DESC, o.id DESC
             LIMIT ",
    );
    query.push_bind(limit);
    query.push(") l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.sort_at DESC, l.id DESC");
    query.build_query_as().fetch_all(db).await
}

/// How many listings the welcome page's "Më të klikuarat" shows, and over how many days their
/// views count.
pub const MOST_VIEWED: i64 = 12;
pub const MOST_VIEWED_DAYS: i32 = 7;

/// The active listings whose pages were seen the most over the last `days` days, today's counting
/// (views.rs), the most first: at most `limit`; in these categories alone, when `categories` names
/// them. Listings no longer on the site make room for the next ones, from four times as many.
pub async fn most_viewed(
    db: &PgPool,
    categories: Option<&[Uuid]>,
    days: i32,
    limit: i64,
) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push(
        "l.views::bigint AS views, NULL::float8 AS distance_m
         FROM (
             SELECT l.*, top.views
             FROM (SELECT s.listing_id, sum(s.detail_views) AS views
                   FROM listing_daily_stats s",
    );
    if let Some(categories) = categories {
        query
            .push(" JOIN listings c ON c.id = s.listing_id AND c.category_id = ANY(")
            .push_bind(categories.to_vec())
            .push(")");
    }
    query.push(" WHERE s.stat_date > current_date - ");
    query.push_bind(days);
    query.push(
        " AND s.detail_views > 0
                   GROUP BY s.listing_id
                   ORDER BY views DESC
                   LIMIT ",
    );
    query.push_bind(limit * 4);
    query.push(
        ") top
             JOIN listings l ON l.id = top.listing_id
             WHERE l.status = 'ACTIVE'
             ORDER BY top.views DESC, l.id DESC
             LIMIT ",
    );
    query.push_bind(limit);
    query.push(") l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.views DESC, l.id DESC");
    query.build_query_as().fetch_all(db).await
}

/// How many times the listing's page was seen, every day's views added up (views.rs); those
/// gathered but not yet written aside.
pub async fn listing_views(db: &PgPool, listing_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT coalesce(sum(detail_views), 0)::bigint FROM listing_daily_stats WHERE listing_id = $1")
        .bind(listing_id)
        .fetch_one(db)
        .await
}

/// Adds views to today's statistics: the listings' numbers, and how many times each was seen
/// (views.rs, which gathers them). Numbers no listing has are left out.
pub async fn add_views(db: &PgPool, numbers: &[i64], views: &[i64]) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views)
         SELECT l.id, current_date, v.views
         FROM unnest($1::bigint[], $2::bigint[]) AS v (public_no, views)
         JOIN listings l ON l.public_no = v.public_no
         ON CONFLICT (listing_id, stat_date)
         DO UPDATE SET detail_views = listing_daily_stats.detail_views + EXCLUDED.detail_views",
    )
    .bind(numbers)
    .bind(views)
    .execute(db)
    .await?;
    Ok(())
}

/// How many related listings a listing's page shows under it.
pub const RELATED: i64 = 8;

/// Listings like this one, for under its page: from other sellers, at most two from any one, the
/// most alike first. As the big sites do it, on a small scale: a few hundred candidates come
/// cheaply off the category's indexes (the closest in price either way, the nearest, the newest,
/// and those sharing a narrow choice, such as the model), and a score ranks them: the choices
/// shared (a model, picked under its make, three; any other, a make, a fuel, a colour, a half, so
/// that many small likenesses don't outweigh the price), the price (three at the same, none at
/// half or double), the place (two at the same, none 50 km away) and how new it is (one today,
/// none two months on).
pub async fn related_listings(db: &PgPool, listing_id: Uuid, limit: i64) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push(
        "NULL::float8 AS distance_m
         FROM (
             WITH target AS MATERIALIZED (
                 SELECT id, category_id, price_normalized, coalesce(seller_business_id, seller_user_id) AS seller,
                        CASE WHEN latitude IS NOT NULL THEN ll_to_earth(latitude::float8, longitude::float8) END AS place
                 FROM listings
                 WHERE id = ",
    );
    query.push_bind(listing_id);
    query.push(
        "),
             choices AS MATERIALIZED (
                 SELECT v.attribute_id, v.value_option_id, o.parent_option_id IS NOT NULL AS narrow
                 FROM target t
                 JOIN listing_attribute_values v ON v.listing_id = t.id
                 JOIN attribute_options o ON o.id = v.value_option_id
             ),
             candidates AS (
                 SELECT c.id FROM target t CROSS JOIN LATERAL (
                     SELECT l.id FROM listings l
                     WHERE l.status = 'ACTIVE' AND l.category_id = t.category_id
                       AND coalesce(l.price_normalized, 'Infinity') <= coalesce(t.price_normalized, 'Infinity')
                     ORDER BY coalesce(l.price_normalized, 'Infinity') DESC, l.id DESC
                     LIMIT 40) c
                 UNION
                 SELECT c.id FROM target t CROSS JOIN LATERAL (
                     SELECT l.id FROM listings l
                     WHERE l.status = 'ACTIVE' AND l.category_id = t.category_id
                       AND coalesce(l.price_normalized, 'Infinity') >= coalesce(t.price_normalized, 'Infinity')
                     ORDER BY coalesce(l.price_normalized, 'Infinity'), l.id
                     LIMIT 40) c
                 UNION
                 SELECT c.id FROM target t CROSS JOIN LATERAL (
                     SELECT l.id FROM listings l
                     WHERE t.place IS NOT NULL AND l.status = 'ACTIVE' AND l.category_id = t.category_id
                       AND l.latitude IS NOT NULL
                     ORDER BY ll_to_earth(l.latitude::float8, l.longitude::float8) <-> t.place
                     LIMIT 40) c
                 UNION
                 SELECT c.id FROM target t CROSS JOIN LATERAL (
                     SELECT l.id FROM listings l
                     WHERE l.status = 'ACTIVE' AND l.category_id = t.category_id
                     ORDER BY l.sort_at DESC, l.id DESC
                     LIMIT 40) c
                 UNION
                 -- The newest 40 with its narrowest options (a car's model): read from the option's
                 -- index, newest first, each listing looked up by its id until 40 are up in the
                 -- category. As a join, the planner, who can't know the category ahead, took a
                 -- big one for an average one and read all of it to match (100 ms for the cars).
                 SELECT c.listing_id FROM target t JOIN choices ch ON ch.narrow CROSS JOIN LATERAL (
                     SELECT v.listing_id FROM listing_attribute_values v
                     WHERE v.attribute_id = ch.attribute_id AND v.value_option_id = ch.value_option_id
                       AND (SELECT l.status = 'ACTIVE' AND l.category_id = t.category_id
                            FROM listings l WHERE l.id = v.listing_id)
                     ORDER BY v.listing_id DESC
                     LIMIT 40) c
             ),
             scored AS (
                 SELECT l.*, coalesce(l.seller_business_id, l.seller_user_id) AS related_seller,
                        (SELECT coalesce(sum(CASE WHEN ch.narrow THEN 3 ELSE 0.5 END), 0)
                         FROM listing_attribute_values v
                         JOIN choices ch ON ch.attribute_id = v.attribute_id AND ch.value_option_id = v.value_option_id
                         WHERE v.listing_id = l.id)
                        + 3 * CASE WHEN l.price_normalized > 0 AND t.price_normalized > 0
                                   THEN greatest(0, 1 - abs(ln(l.price_normalized / t.price_normalized)) / ln(2))
                                   ELSE 0 END
                        + 2 * CASE WHEN l.latitude IS NOT NULL AND t.place IS NOT NULL
                                   THEN greatest(0, 1 - (ll_to_earth(l.latitude::float8, l.longitude::float8) <-> t.place) / 50000)
                                   ELSE 0 END
                        + greatest(0, 1 - extract(epoch FROM now() - l.sort_at) / 5184000) AS related_score
                 FROM candidates c
                 JOIN listings l ON l.id = c.id
                 CROSS JOIN target t
                 WHERE l.id <> t.id AND coalesce(l.seller_business_id, l.seller_user_id) IS DISTINCT FROM t.seller
             )
             SELECT * FROM (
                 SELECT s.*, row_number() OVER (PARTITION BY s.related_seller ORDER BY s.related_score DESC, s.id) AS seller_rank
                 FROM scored s
             ) ranked
             WHERE seller_rank <= 2
             ORDER BY related_score DESC, id
             LIMIT ",
    );
    query.push_bind(limit);
    query.push(") l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.related_score DESC, l.id");
    query.build_query_as().fetch_all(db).await
}

/// How many active listings match the filter, counting at most COUNT_LIMIT + 1.
pub async fn listing_count(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<i64> {
    listing_count_up_to(db, filter, COUNT_LIMIT).await
}

/// How many active listings match the filter, counting at most `limit` + 1.
pub async fn listing_count_up_to(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<i64> {
    let mut query = QueryBuilder::<Postgres>::new("SELECT count(*) FROM (SELECT 1 FROM listings l");
    push_filters(&mut query, filter);
    query.push(" LIMIT ").push_bind(limit + 1).push(") matches");
    query.build_query_scalar().fetch_one(db).await
}

/// The prices that split the active listings of these categories priced in `currency` into four
/// as many each (the lowest quarter, the next...), for the price's quick choices (filters.rs,
/// price_choices); None with too few of them to say. Every such listing counts, so it takes a
/// moment in a big category: browse.rs keeps the answer for hours.
pub async fn price_quarters(db: &PgPool, category_ids: &[Uuid], currency: &str) -> sqlx::Result<Option<[Decimal; 3]>> {
    let row = sqlx::query!(
        r#"SELECT percentile_disc(0.25) WITHIN GROUP (ORDER BY price_amount) AS lower,
                  percentile_disc(0.5) WITHIN GROUP (ORDER BY price_amount) AS middle,
                  percentile_disc(0.75) WITHIN GROUP (ORDER BY price_amount) AS upper,
                  count(*) AS "priced!"
           FROM listings
           WHERE status = 'ACTIVE' AND category_id = ANY($1) AND currency::text = $2
             AND price_type IN ('FIXED', 'NEGOTIABLE') AND price_amount > 0"#,
        category_ids,
        currency,
    )
    .fetch_one(db)
    .await?;
    Ok(match (row.lower, row.middle, row.upper) {
        (Some(lower), Some(middle), Some(upper)) if row.priced >= 20 => Some([lower, middle, upper]),
        _ => None,
    })
}

/// The categories the listings matching the filter are in, with how many each, the most first:
/// at most `limit` of them, counted as listing_count counts, among at most COUNT_LIMIT + 1
/// listings.
pub async fn listing_categories(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<Vec<(Uuid, i64)>> {
    let mut query =
        QueryBuilder::<Postgres>::new("SELECT category_id, count(*) FROM (SELECT l.category_id FROM listings l");
    push_filters(&mut query, filter);
    query.push(" LIMIT ").push_bind(COUNT_LIMIT + 1);
    query.push(") matches GROUP BY category_id ORDER BY count(*) DESC, category_id LIMIT ").push_bind(limit);
    query.build_query_as().fetch_all(db).await
}

/// The towns (municipalities) a search finds most in, a zone's listings counting in its town,
/// counted among its newest COUNT_LIMIT + 1 matches, for the search box's suggestions ("apartamente
/// në" → Tiranë, Durrës, Vlorë). The newest, not the first found as listing_categories counts:
/// those come in the order rows lie in the table, and listings rewritten together, such as those
/// moved into their zones, lie together at its end.
pub async fn listing_towns(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<Vec<(Uuid, i64)>> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT CASE WHEN loc.location_type = 'AREA' THEN loc.parent_id ELSE loc.id END AS town, count(*)
         FROM (SELECT l.location_id FROM listings l",
    );
    push_filters(&mut query, filter);
    query.push(" ORDER BY l.sort_at DESC, l.id DESC LIMIT ").push_bind(COUNT_LIMIT + 1);
    query.push(") matches JOIN locations loc ON loc.id = matches.location_id GROUP BY 1 ORDER BY count(*) DESC, 1 LIMIT ");
    query.push_bind(limit);
    query.build_query_as().fetch_all(db).await
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
         FROM ",
    );
    push_ordered(&mut query, filter, None, true, MAP_POINTS + 1);
    push_order(&mut query, filter.sort, filter.near.as_ref());
    query.push(" LIMIT ").push_bind(MAP_POINTS + 1);
    query.build_query_as().fetch_all(db).await
}

/// The filter's matches in its order, the first `limit` (after the cursor, if one is given), as
/// `l`: what a page of cards and the map's dots are read from; `located` keeps only listings with
/// a point. By price or distance within categories, each category's own index gives its first
/// `limit` in order (idx_listings_active_category_cheapest, _dearest, _earth), in a LATERAL scan
/// per category, and those merge: `category_id = ANY(...)` can't use an index for the order, and
/// an index of every category's listings would wade through the others' whenever they come
/// first, the cheapest of all being neither cars nor flats. The newest first come from one scan,
/// as ever: every category's newest are in one index (idx_listings_active_sort), the categories
/// are big enough for it, and filtered, the one scan can share its work among Postgres's workers.
fn push_ordered(
    query: &mut QueryBuilder<'_, Postgres>,
    filter: &ListingFilter,
    after: Option<Cursor>,
    located: bool,
    limit: i64,
) {
    let by_category = filter.category_ids.as_ref().filter(|_| {
        matches!((filter.sort, &filter.near), (Sort::PriceLow | Sort::PriceHigh, _) | (Sort::Nearest, Some(_)))
    });
    if let Some(ids) = by_category {
        query.push("unnest(").push_bind(ids.clone()).push("::uuid[]) AS category(id) CROSS JOIN LATERAL ");
    }
    query.push("(SELECT l.* FROM listings l");
    push_conditions(query, filter, by_category.is_some());
    if located {
        query.push(" AND l.latitude IS NOT NULL");
    }
    if let Some(after) = after {
        push_after(query, filter.sort, filter.near.as_ref(), after);
    }
    match (filter.sort, filter.near.as_ref()) {
        // By the distance alone, which a location index gives nearest first (KNN) only when it
        // is the whole order: every listing as far as the last one comes too, and the id orders
        // those outside (push_order), many sharing their town's point.
        (Sort::Nearest, Some(near)) => {
            query.push(" ORDER BY ");
            near.push_distance(query);
            query.push(" FETCH FIRST (").push_bind(limit).push(") ROWS WITH TIES) l");
        }
        _ => {
            push_order(query, filter.sort, filter.near.as_ref());
            query.push(" LIMIT ").push_bind(limit).push(") l");
        }
    }
}

/// Keyset pagination: only what comes after the cursor in the sort's order.
fn push_after(query: &mut QueryBuilder<'_, Postgres>, sort: Sort, near: Option<&Near>, after: Cursor) {
    let comparison = if sort.descending() { "<" } else { ">" };
    query.push(" AND (");
    sort.push_key(query, near);
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

/// The sort's order, the id breaking ties.
fn push_order(query: &mut QueryBuilder<'_, Postgres>, sort: Sort, near: Option<&Near>) {
    let direction = if sort.descending() { "DESC" } else { "ASC" };
    query.push(" ORDER BY ");
    sort.push_key(query, near);
    query.push(format_args!(" {direction}, l.id {direction}"));
}

/// The WHERE clause shared by a page of listings, their count and their dots on the map.
fn push_filters(query: &mut QueryBuilder<'_, Postgres>, filter: &ListingFilter) {
    push_conditions(query, filter, false);
}

/// push_filters, the categories either listed or, `per_category`, the one a LATERAL scan is on
/// (push_ordered).
fn push_conditions(query: &mut QueryBuilder<'_, Postgres>, filter: &ListingFilter, per_category: bool) {
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
    if !filter.hidden_accounts.is_empty() {
        query.push(" AND l.created_by_user_id <> ALL(").push_bind(filter.hidden_accounts.clone()).push(")");
    }
    if let Some(ids) = &filter.category_ids {
        if per_category {
            query.push(" AND l.category_id = category.id");
        } else {
            query.push(" AND l.category_id = ANY(").push_bind(ids.clone()).push(")");
        }
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
        // earth_box() finds the candidates in the index; the distance keeps those truly within the
        // radius (the box's corners reach further).
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
        if let Some(words) = &search.query {
            query.push(" AND l.search_vector @@ ").push_bind(words.clone()).push("::tsquery");
        }
        if let Some(kind_words) = &search.kind_query {
            query
                .push(" AND (l.listing_type = ANY(")
                .push_bind(search.kinds.clone())
                .push(") OR l.search_vector @@ ")
                .push_bind(kind_words.clone())
                .push("::tsquery)");
        }
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
