//! Pages of listing cards: the browse pages' filters, orders and pages, the rows under a listing,
//! the most seen, counts, and the map's points.

use std::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicU8, Ordering},
};

use sqlx::PgConnection;

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
    /// Whether its page is laid out as a car's or a property's (web/listings.rs, MAPPED), which
    /// frames its photos 4:3 on a phone, where the others' are 6:5: the card says so (data-deal),
    /// for the stand-in a tap shows while the page comes (application.js, standIn).
    #[sqlx(default)]
    pub deal: bool,
    /// A car's title as its seller wrote it, where the one the cards show was made from what the
    /// car is (generated_title): what its page heads itself with (listing.html), which the
    /// stand-in a tap shows heads itself with too (data-heading).
    #[sqlx(default)]
    pub heading: Option<String>,
    /// Its category's name and the one above it, if any, as its page's path says them
    /// (listing.html, .listing-path), which the stand-in a tap shows says too (data-here, data-up).
    #[sqlx(default)]
    pub category_name: String,
    #[sqlx(default)]
    pub category_parent: Option<String>,
    /// A property's area, as its page reads it (ListingDetail::area_m2), for whether its page
    /// shows what a square metre costs (ListingCard::compares).
    #[sqlx(default)]
    pub area_m2: Option<Decimal>,
    /// Its condition and when it was published, as its page says them in the line under its
    /// title (ListingCard::page_line).
    #[sqlx(default)]
    pub condition: Option<String>,
    #[sqlx(default)]
    pub published_at: Option<OffsetDateTime>,
    /// How many key facts its page has under its title, a car's or a property's (web/listings.rs,
    /// CAR_FACTS, PROPERTY_FACTS): those of them the listing gives, two to a line on a phone, which
    /// the stand-in a tap shows draws as many of (data-facts).
    #[sqlx(default)]
    pub facts: i64,
    /// What its page's description starts with, its lines as they are, one character past
    /// PAGE_DESCRIPTION so that a longer one shows it was cut, and what its page offers to reveal
    /// of the seller ("phone", "email", none: db::visible_contact): the stand-in a tap shows draws
    /// its page's description and contact bar from them (ListingCard::stand_in_description;
    /// data-description, data-reveal).
    #[sqlx(default)]
    pub page_description: String,
    #[sqlx(default)]
    pub reveal: Option<String>,
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
    /// Its status: every list shows active listings alone but /te-ruajturat, which keeps the
    /// reserved and sold ones a visitor kept, each saying so over its photo (status_label).
    #[sqlx(default)]
    pub status: String,
    /// How well it matches the search's words (ts_rank), in the best match first order alone
    /// (Sort::Relevance), whose next page starts after it.
    #[sqlx(default)]
    pub rank: Option<f32>,
    /// Whether a promotion features it now (featured_until, promotions.rs): the row over a
    /// category's or a search's results shows it.
    #[sqlx(default)]
    pub featured: bool,
    /// Whether a promotion highlights it now (highlighted_until): its card stands out.
    #[sqlx(default)]
    pub highlighted: bool,
    /// For a car up for sale at a price, what cars of its model and year ask, the median in
    /// euros (car_model_prices): a clear bargain says so on its card (web/car_prices.rs).
    #[sqlx(default)]
    pub model_median: Option<Decimal>,
}

/// How much of a description a card carries, in characters.
pub const DESCRIPTION_EXCERPT: usize = 400;

/// How much of its page's description a card carries for the stand-in a tap shows, its lines as
/// they are, in characters: more than a dozen lines' worth, which the page shows of it folded
/// (description.html), at any width and text size (a dozen lines are about 1 300 characters at the
/// widest column and the smallest text).
pub const PAGE_DESCRIPTION: usize = 1600;

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
    /// The best match for the search's words first (ts_rank: the title's words weigh most, the
    /// details' next, the description's least), the newest first among equals; only with words.
    /// Among the newest RANK_WINDOW matches, so that a word in a quarter of the listings costs
    /// no more than one in a few; the matches past them follow, newest first (relevance_page).
    Relevance,
    #[default]
    Newest,
    PriceLow,
    PriceHigh,
    /// Nearest to the map pin first; only with a pin.
    Nearest,
}

impl Sort {
    pub const ALL: [Sort; 5] = [Sort::Relevance, Sort::Newest, Sort::PriceLow, Sort::PriceHigh, Sort::Nearest];

    pub fn from_param(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|sort| sort.param() == value)
    }

    pub fn param(self) -> &'static str {
        match self {
            Sort::Relevance => "relevance",
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
            // The rank, then the newest: push_order and push_ordered write both.
            (Sort::Relevance, _) => {
                query.push("l.rank");
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
    Relevance { rank: f32, sort_at: OffsetDateTime, id: Uuid },
    /// The best match first, past its ranked window: the matches older than it, newest first.
    RelevanceRest { sort_at: OffsetDateTime, id: Uuid },
    Newest { sort_at: OffsetDateTime, id: Uuid },
    Price { price: Option<Decimal>, id: Uuid },
    Distance { meters: f64, id: Uuid },
}

impl Cursor {
    /// The position right after `card` in a page sorted by `sort`.
    pub fn after(sort: Sort, card: &ListingCard) -> Self {
        match (sort, card.distance_m) {
            (Sort::Nearest, Some(meters)) => Self::Distance { meters, id: card.id },
            (Sort::Relevance, _) => match card.rank {
                Some(rank) => Self::Relevance { rank, sort_at: card.sort_at, id: card.id },
                None => Self::RelevanceRest { sort_at: card.sort_at, id: card.id },
            },
            (Sort::Newest | Sort::Nearest, _) => Self::Newest { sort_at: card.sort_at, id: card.id },
            (Sort::PriceLow | Sort::PriceHigh, _) => {
                Self::Price { price: card.price_normalized, id: card.id }
            }
        }
    }

    pub fn id(&self) -> Uuid {
        match self {
            Self::Relevance { id, .. }
            | Self::RelevanceRest { id, .. }
            | Self::Newest { id, .. }
            | Self::Price { id, .. }
            | Self::Distance { id, .. } => *id,
        }
    }

    /// "<key>_<id>". The key is microseconds since 1970 (PostgreSQL's precision) for the newest
    /// first order, the price in euros, or "-" for none, for the price orders, and the distance
    /// in metres, in full, for the nearest first order; for the best match first, the rank (as
    /// short as it reads back the same) and the microseconds, "<rank>_<key>_<id>", and past its
    /// window "past_<key>_<id>".
    pub fn encode(&self) -> String {
        match self {
            Self::Relevance { rank, sort_at, id } => {
                format!("{rank}_{}_{id}", sort_at.unix_timestamp_nanos() / 1_000)
            }
            Self::RelevanceRest { sort_at, id } => {
                format!("past_{}_{id}", sort_at.unix_timestamp_nanos() / 1_000)
            }
            Self::Newest { sort_at, id } => {
                format!("{}_{id}", sort_at.unix_timestamp_nanos() / 1_000)
            }
            Self::Price { price: Some(price), id } => format!("{price}_{id}"),
            Self::Price { price: None, id } => format!("-_{id}"),
            Self::Distance { meters, id } => format!("{meters}_{id}"),
        }
    }

    pub fn decode(sort: Sort, value: &str) -> Option<Self> {
        if sort == Sort::Relevance {
            if let Some(past) = value.strip_prefix("past_") {
                let Some(Self::Newest { sort_at, id }) = Self::decode(Sort::Newest, past) else { return None };
                return Some(Self::RelevanceRest { sort_at, id });
            }
            let mut parts = value.splitn(3, '_');
            let rank: f32 = parts.next()?.parse().ok()?;
            let nanos = parts.next()?.parse::<i128>().ok()?.checked_mul(1_000)?;
            let sort_at = OffsetDateTime::from_unix_timestamp_nanos(nanos).ok()?;
            let id = parts.next()?.parse().ok()?;
            return rank.is_finite().then_some(Self::Relevance { rank, sort_at, id });
        }
        let (key, id) = value.split_once('_')?;
        let id = id.parse().ok()?;
        match sort {
            Sort::Relevance => None,
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
    /// Listings left out, by their ids: those the welcome page recommended, which the newest after
    /// them don't show again (web/browse.rs, recommended_page).
    pub exclude: Vec<Uuid>,
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
    /// Only listings put up or brought up after this moment (`sort_at`): a saved search's new
    /// ones (web/searches.rs).
    pub since: Option<OffsetDateTime>,
    /// What its queries have found out about its words' matches (planned).
    pub matches: Matches,
}

/// What a filter's queries have found out about its words' matches, which its later queries go by
/// (planned): nothing yet, or whether they are few (fewer than FEW) or many.
#[derive(Debug, Default)]
pub struct Matches(AtomicU8);

impl Matches {
    const FEW: u8 = 1;
    const MANY: u8 = 2;

    /// Few, or many, once a query has found out; None before.
    pub fn few(&self) -> Option<bool> {
        match self.0.load(Ordering::Relaxed) {
            Self::FEW => Some(true),
            Self::MANY => Some(false),
            _ => None,
        }
    }

    pub fn set(&self, few: bool) {
        self.0.store(if few { Self::FEW } else { Self::MANY }, Ordering::Relaxed);
    }
}

/// How long a query of a search with words runs as Postgres plans it before planned steps in:
/// ample for the plans that suit their matches, a few milliseconds.
const FIRST_TRY: &str = "50ms";

/// How many matches are few: read through the words' index, they are quicker to sort than the
/// newest first order is to walk for them (a page wants 25, the ranked window 240).
const FEW: i64 = 5_000;

/// Bitmap scans alone, no parallel workers: a filter's matches read through the words' index (and
/// any other that narrows them), sorted when an order is wanted.
const THROUGH_INDEX: &str = "SELECT set_config('enable_indexscan', 'off', true), set_config('enable_seqscan', 'off', true),
                                    set_config('max_parallel_workers_per_gather', '0', true)";

/// Index scans alone: the order walked, its matches taken as they come.
const IN_ORDER: &str = "SELECT set_config('enable_bitmapscan', 'off', true), set_config('enable_seqscan', 'off', true)";

/// How a query reads a filter's matches: the first in an order (a page, the ranked window and its
/// edge, the map's dots, the newest for their towns), or any of them (a count, the categories).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reads {
    InOrder,
    Any,
}

/// A query running on the connection it was given, for `planned`.
type Run<'c, T> = Pin<Box<dyn Future<Output = sqlx::Result<T>> + Send + 'c>>;

/// Runs one of a filter's queries, which `run` builds and runs on the connection it is given (twice
/// at most). Without words, as Postgres plans it. With words, Postgres takes each word for as
/// common among a category's listings, or alongside another word, as it is among all of them, and
/// errs both ways. For words that seldom meet ("apartament" among the cars) it walks the newest
/// first order (or the whole table, for a count) expecting a match soon, and reads every listing:
/// 2.9 s for that page's ranked window against a million listings. For words that meet often
/// ("gjendje shumë mirë", 51 549 matches) it reads every match and sorts them, 92 ms where the
/// walk takes 4. Counting the matches first would tell, but a word in a quarter of the listings
/// would pay for it on every page (29 ms for "makina"). So the query runs as planned for
/// FIRST_TRY; past it, the matches are counted through the words' index up to FEW (Matches, for
/// the filter's later queries), and the query runs again: through that index for few matches, which
/// has their categories in it too (idx_listings_active_words), and for many, in order (a count,
/// which the walk would take a long way for, through the index all the same).
async fn planned<T>(
    db: &PgPool,
    filter: &ListingFilter,
    reads: Reads,
    run: impl for<'c> Fn(&'c mut PgConnection) -> Run<'c, T>,
) -> sqlx::Result<T> {
    let words = filter.search.as_ref().is_some_and(|search| search.query.is_some() || search.kind_query.is_some());
    if !words {
        return run(&mut *db.acquire().await?).await;
    }
    let few = match filter.matches.few() {
        Some(few) => few,
        None => {
            let mut tx = db.begin().await?;
            sqlx::query("SELECT set_config('statement_timeout', $1, true)").bind(FIRST_TRY).execute(&mut *tx).await?;
            match run(&mut tx).await {
                // Cancelled for running too long (query_canceled).
                Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("57014") => {}
                // Read only: the transaction ends as it is dropped.
                done => return done,
            }
            drop(tx);
            match filter.matches.few() {
                Some(few) => few,
                None => {
                    let few = count_through_index(db, filter).await? < FEW;
                    filter.matches.set(few);
                    few
                }
            }
        }
    };
    let mut tx = db.begin().await?;
    let plan = if few || reads == Reads::Any { THROUGH_INDEX } else { IN_ORDER };
    sqlx::query(plan).execute(&mut *tx).await?;
    run(&mut tx).await
}

/// The filter's matches counted up to FEW through the words' index alone: as dear as they are many,
/// and at most what FEW of them cost (a few milliseconds for a few, 29 for "makina").
async fn count_through_index(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<i64> {
    let mut tx = db.begin().await?;
    sqlx::query(THROUGH_INDEX).execute(&mut *tx).await?;
    let mut query = QueryBuilder::<Postgres>::new("SELECT count(*) FROM (SELECT 1 FROM listings l");
    push_filters(&mut query, filter);
    query.push(" LIMIT ").push_bind(FEW).push(") matches");
    query.build_query_scalar().fetch_one(&mut *tx).await
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
    /// How quickly they usually answer, and when they were last on the site, as a listing's
    /// seller card has them (db::ListingDetail).
    pub answers: Option<f64>,
    pub seen: Option<OffsetDateTime>,
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
                   LIMIT 1) AS verified,
                  (SELECT r.median_seconds FROM seller_response_times r
                   WHERE r.seller_id = u.id AND r.conversations >= 3 AND r.answered * 2 >= r.conversations) AS "seller_answers?",
                  u.last_seen_at
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
        answers: row.seller_answers,
        seen: row.last_seen_at,
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
                   LIMIT 1) AS verified,
                  (SELECT r.median_seconds FROM seller_response_times r
                   WHERE r.seller_id = b.id AND r.conversations >= 3 AND r.answered * 2 >= r.conversations) AS "seller_answers?",
                  (SELECT max(mu.last_seen_at) FROM business_members bm JOIN users mu ON mu.id = bm.user_id
                   WHERE bm.business_id = b.id) AS seen
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
        answers: row.seller_answers,
        seen: row.seen,
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
pub(super) const CARD_COLUMNS: &str = concat!("WITH RECURSIVE deal_categories AS (
             -- A car's and a property's categories and those under them (web/listings.rs, MAPPED),
             -- the cars' marked.
             SELECT id, slug = 'makina' AS car FROM categories WHERE slug IN ('prona', 'makina')
             UNION ALL
             SELECT c.id, d.car FROM categories c JOIN deal_categories d ON c.parent_id = d.id
         ),
         card_attributes AS MATERIALIZED (
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
                l.category_id IN (SELECT id FROM deal_categories) AS deal,
                CASE WHEN l.generated_title IS NOT NULL AND l.category_id IN (SELECT id FROM deal_categories WHERE car)
                     THEN l.title END AS heading,
                (SELECT c.name FROM categories c WHERE c.id = l.category_id) AS category_name,
                (SELECT p.name FROM categories c JOIN categories p ON p.id = c.parent_id WHERE c.id = l.category_id) AS category_parent,
                (SELECT v.value_number FROM listing_attribute_values v JOIN attributes a ON a.id = v.attribute_id
                 WHERE v.listing_id = l.id AND a.code = 'property_area') AS area_m2,
                l.condition, l.published_at,
                -- What its page's description starts with, its lines as they are, one character past
                -- PAGE_DESCRIPTION, and what its page offers to reveal of the seller, the phone or
                -- else the e-mail while it is up, as visible_contact reads it: for the stand-in a
                -- tap shows (ListingCard::stand_in_description, reveal).
                left(coalesce(l.description, ''), 1601) AS page_description,
                (SELECT CASE WHEN c.show_phone AND c.phone_e164 IS NOT NULL THEN 'phone'
                             WHEN c.show_email AND c.email IS NOT NULL THEN 'email' END
                 FROM listing_contacts c
                 WHERE c.listing_id = l.id AND l.status IN ('ACTIVE', 'RESERVED')) AS reveal,
                -- The key facts a car's page or a property's shows (web/listings.rs, CAR_FACTS and
                -- PROPERTY_FACTS, whose codes these are): those the listing gives of its category's details.
                (SELECT count(DISTINCT a.code) FROM listing_attribute_values v
                 JOIN attributes a ON a.id = v.attribute_id
                 JOIN effective_category_attributes eca
                   ON eca.category_id = l.category_id AND eca.attribute_id = v.attribute_id AND eca.show_in_details
                 WHERE v.listing_id = l.id
                   AND a.code IN ('vehicle_year', 'vehicle_mileage', 'vehicle_transmission', 'vehicle_fuel',
                                  'property_rooms', 'property_area', 'property_floor', 'property_furnished')) AS facts,
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
                l.status,
                ", price_was!(), " AS price_was,
                coalesce(l.featured_until > now(), false) AS featured,
                coalesce(l.highlighted_until > now(), false) AS highlighted,
                -- A car's (Makina's, its model and year the properties 0002 fixes the ids of):
                -- what cars of its model and year ask (car_model_prices).
                (SELECT p.median_eur
                 FROM listing_attribute_values m
                 JOIN listing_attribute_values y
                   ON y.listing_id = m.listing_id AND y.attribute_id = '20000000-0000-0000-0000-000000000003'
                 JOIN car_model_prices p ON p.model_id = m.value_option_id AND p.year = y.value_number::int
                 WHERE m.listing_id = l.id AND m.attribute_id = '20000000-0000-0000-0000-000000000002'
                   AND l.category_id = '10000000-0000-0000-0000-000000000101' AND l.listing_type = 'SELL'
                   AND l.price_type IN ('FIXED', 'NEGOTIABLE') AND l.condition IS DISTINCT FROM 'FOR_PARTS') AS model_median,
                ");

/// One page of active listings in the filter's order, with at most PAGE_SIZE + 1 rows.
pub async fn listing_page(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Vec<ListingCard>> {
    match filter.sort {
        Sort::Relevance => relevance_page(db, filter).await,
        sort => ordered_page(db, filter, sort, filter.after, PAGE_SIZE + 1).await,
    }
}

/// The first `limit` matches in this order after the cursor.
async fn ordered_page(
    db: &PgPool,
    filter: &ListingFilter,
    sort: Sort,
    after: Option<Cursor>,
    limit: i64,
) -> sqlx::Result<Vec<ListingCard>> {
    planned(db, filter, Reads::InOrder, |conn| {
        let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
        query.push("NULL::real AS rank, ");
        match &filter.near {
            Some(near) => near.push_distance(&mut query),
            None => {
                query.push("NULL::float8");
            }
        }
        query.push(" AS distance_m FROM ");
        push_ordered(&mut query, filter, sort, after, false, limit);
        query.push(" LEFT JOIN locations loc ON loc.id = l.location_id");
        push_order(&mut query, sort, filter.near.as_ref());
        query.push(" LIMIT ").push_bind(limit);
        Box::pin(async move { query.build_query_as().fetch_all(conn).await })
    })
    .await
}

/// A page of the best match first: the ranked window's matches after the cursor, and once they
/// run out, the matches older than the window, newest first (their cards have no rank, and the
/// page after them starts past the window, Cursor::RelevanceRest). A window that holds every
/// match has nothing past it.
async fn relevance_page(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Vec<ListingCard>> {
    let mut rows = Vec::new();
    let past = match filter.after {
        Some(Cursor::RelevanceRest { sort_at, id }) => Cursor::Newest { sort_at, id },
        after => {
            rows = ranked_page(db, filter, after).await?;
            if rows.len() as i64 > PAGE_SIZE {
                return Ok(rows);
            }
            match window_edge(db, filter).await? {
                Some(edge) => edge,
                None => return Ok(rows),
            }
        }
    };
    let left = PAGE_SIZE + 1 - rows.len() as i64;
    rows.extend(ordered_page(db, filter, Sort::Newest, Some(past), left).await?);
    Ok(rows)
}

/// The ranked window's matches after the cursor, the best first: PAGE_SIZE + 1 at most.
async fn ranked_page(db: &PgPool, filter: &ListingFilter, after: Option<Cursor>) -> sqlx::Result<Vec<ListingCard>> {
    planned(db, filter, Reads::InOrder, |conn| {
        let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
        query.push("l.rank AS rank, NULL::float8 AS distance_m FROM ");
        push_ranked(&mut query, filter, after, PAGE_SIZE + 1);
        query.push(" LEFT JOIN locations loc ON loc.id = l.location_id");
        push_order(&mut query, Sort::Relevance, None);
        query.push(" LIMIT ").push_bind(PAGE_SIZE + 1);
        Box::pin(async move { query.build_query_as().fetch_all(conn).await })
    })
    .await
}

/// The oldest match in the ranked window, where the matches past it start: the RANK_WINDOW-th
/// newest, as a newest first cursor; None when the window holds every match.
async fn window_edge(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<Option<Cursor>> {
    let edge: Option<(OffsetDateTime, Uuid)> = planned(db, filter, Reads::InOrder, |conn| {
        let mut query = QueryBuilder::<Postgres>::new("SELECT l.sort_at, l.id FROM listings l");
        push_conditions(&mut query, filter, false);
        query.push(" ORDER BY l.sort_at DESC, l.id DESC OFFSET ").push_bind(RANK_WINDOW - 1).push(" LIMIT 1");
        Box::pin(async move { query.build_query_as().fetch_optional(conn).await })
    })
    .await?;
    Ok(edge.map(|(sort_at, id)| Cursor::Newest { sort_at, id }))
}

/// The listings with these numbers still on the site, up, reserved or sold (whose cards say so),
/// in the order given: the welcome page's row of those its visitor saw lately (browse.rs,
/// SEEN_COOKIE).
pub async fn cards_by_numbers(db: &PgPool, numbers: &[i64]) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push("NULL::real AS rank, NULL::float8 AS distance_m FROM (SELECT l.* FROM listings l WHERE l.public_no = ANY(");
    query.push_bind(numbers.to_vec()).push(") AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD')) l");
    query.push(" LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY array_position(");
    query.push_bind(numbers.to_vec()).push("::bigint[], l.public_no)");
    query.build_query_as().fetch_all(db).await
}

/// The active listings with these ids, in the order given: the welcome page's recommended ones,
/// ranked by their ids alone (recommended_ids), a page of them at a time (web/browse.rs).
pub async fn cards_by_ids(db: &PgPool, ids: &[Uuid]) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push("NULL::real AS rank, NULL::float8 AS distance_m FROM (SELECT l.* FROM listings l WHERE l.id = ANY(");
    query.push_bind(ids.to_vec()).push(") AND l.status = 'ACTIVE') l");
    query.push(" LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY array_position(");
    query.push_bind(ids.to_vec()).push("::uuid[], l.id)");
    query.build_query_as().fetch_all(db).await
}

/// How many listings the row over a category's or a search's results shows at most, of those a
/// promotion features (featured_cards).
pub const FEATURED: i64 = 8;

/// The listings a promotion features now (featured_until, promotions.rs) among the filter's
/// matches, at most `limit`, in another order each time, so that each has its turn at the
/// row's start: the row over a category's or a search's results (browse.rs). Few listings are
/// featured, and their partial index (idx_listings_active_featured) finds them, whatever else
/// the filter asks.
pub async fn featured_cards(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push("NULL::real AS rank, ");
    match &filter.near {
        Some(near) => near.push_distance(&mut query),
        None => {
            query.push("NULL::float8");
        }
    }
    query.push(" AS distance_m FROM (SELECT l.* FROM listings l");
    push_filters(&mut query, filter);
    query.push(" AND l.featured_until > now() ORDER BY random() LIMIT ").push_bind(limit).push(") l");
    query.push(" LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY random()");
    query.build_query_as().fetch_all(db).await
}

/// How many listings "Më të klikuarat" is read with, and over how many days their views count: it
/// shows the first eight (web/browse.rs, MOST_VIEWED_SHOWN), the others standing in for those of
/// accounts the visitor blocked.
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

/// How many related listings a listing's page shows under it at a time: the next as many load as
/// the last come into view (listings::related).
pub const RELATED: i64 = 8;

/// Listings like this one, for under its page: from other sellers, at most two from any one, the
/// most alike first, `limit` of them from the `offset`th (the page's next frames ask from where
/// it left off; the ranking holds still enough between two requests, its newness part moving by
/// the day). As the big sites do it, on a small scale: a few hundred candidates come
/// cheaply off the category's indexes (the closest in price either way, the nearest, the newest,
/// and those sharing a narrow choice, such as the model), and a score ranks them: the choices
/// shared (a model, picked under its make, three; any other, a make, a fuel, a colour, a half, so
/// that many small likenesses don't outweigh the price), the price (three at the same, none at
/// half or double), the place (two at the same, none 50 km away) and how new it is (one today,
/// none two months on).
pub async fn related_listings(db: &PgPool, listing_id: Uuid, limit: i64, offset: i64) -> sqlx::Result<Vec<ListingCard>> {
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
    query.push(" OFFSET ");
    query.push_bind(offset);
    query.push(") l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.related_score DESC, l.id");
    query.build_query_as().fetch_all(db).await
}

/// How many active listings match the filter, counting at most COUNT_LIMIT + 1.
pub async fn listing_count(db: &PgPool, filter: &ListingFilter) -> sqlx::Result<i64> {
    listing_count_up_to(db, filter, COUNT_LIMIT).await
}

/// How many active listings match the filter, counting at most `limit` + 1.
pub async fn listing_count_up_to(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<i64> {
    planned(db, filter, Reads::Any, |conn| {
        let mut query = QueryBuilder::<Postgres>::new("SELECT count(*) FROM (SELECT 1 FROM listings l");
        push_filters(&mut query, filter);
        query.push(" LIMIT ").push_bind(limit + 1).push(") matches");
        Box::pin(async move { query.build_query_scalar().fetch_one(conn).await })
    })
    .await
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
    planned(db, filter, Reads::Any, |conn| {
        let mut query =
            QueryBuilder::<Postgres>::new("SELECT category_id, count(*) FROM (SELECT l.category_id FROM listings l");
        push_filters(&mut query, filter);
        query.push(" LIMIT ").push_bind(COUNT_LIMIT + 1);
        query.push(") matches GROUP BY category_id ORDER BY count(*) DESC, category_id LIMIT ").push_bind(limit);
        Box::pin(async move { query.build_query_as().fetch_all(conn).await })
    })
    .await
}

/// The towns (municipalities) a search finds most in, a zone's listings counting in its town,
/// counted among its newest COUNT_LIMIT + 1 matches, for the search box's suggestions ("apartamente
/// në" → Tiranë, Durrës, Vlorë). The newest, not the first found as listing_categories counts:
/// those come in the order rows lie in the table, and listings rewritten together, such as those
/// moved into their zones, lie together at its end.
pub async fn listing_towns(db: &PgPool, filter: &ListingFilter, limit: i64) -> sqlx::Result<Vec<(Uuid, i64)>> {
    planned(db, filter, Reads::InOrder, |conn| {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT CASE WHEN loc.location_type = 'AREA' THEN loc.parent_id ELSE loc.id END AS town, count(*)
             FROM (SELECT l.location_id FROM listings l",
        );
        push_filters(&mut query, filter);
        query.push(" ORDER BY l.sort_at DESC, l.id DESC LIMIT ").push_bind(COUNT_LIMIT + 1);
        query.push(") matches JOIN locations loc ON loc.id = matches.location_id GROUP BY 1 ORDER BY count(*) DESC, 1 LIMIT ");
        query.push_bind(limit);
        Box::pin(async move { query.build_query_as().fetch_all(conn).await })
    })
    .await
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
    // Its dots come in the list's order, but the best match first's: newest first there, as
    // its matches past the window come (relevance_page).
    let sort = if filter.sort == Sort::Relevance { Sort::Newest } else { filter.sort };
    planned(db, filter, Reads::InOrder, |conn| {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title, l.listing_type, l.price_amount,
                    l.currency::text AS currency, l.price_type,
                    l.latitude::float8 AS latitude, l.longitude::float8 AS longitude
             FROM ",
        );
        push_ordered(&mut query, filter, sort, None, true, MAP_POINTS + 1);
        push_order(&mut query, sort, filter.near.as_ref());
        query.push(" LIMIT ").push_bind(MAP_POINTS + 1);
        Box::pin(async move { query.build_query_as().fetch_all(conn).await })
    })
    .await
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
/// The order is `sort`, the filter's or, for the best match first's pages, the newest first
/// (relevance_page, whose window push_ranked reads).
fn push_ordered(
    query: &mut QueryBuilder<'_, Postgres>,
    filter: &ListingFilter,
    sort: Sort,
    after: Option<Cursor>,
    located: bool,
    limit: i64,
) {
    let by_category = filter
        .category_ids
        .as_ref()
        .filter(|_| matches!((sort, &filter.near), (Sort::PriceLow | Sort::PriceHigh, _) | (Sort::Nearest, Some(_))));
    if let Some(ids) = by_category {
        query.push("unnest(").push_bind(ids.clone()).push("::uuid[]) AS category(id) CROSS JOIN LATERAL ");
    }
    query.push("(SELECT l.* FROM listings l");
    push_conditions(query, filter, by_category.is_some());
    if located {
        query.push(" AND l.latitude IS NOT NULL");
    }
    if let Some(after) = after {
        push_after(query, sort, filter.near.as_ref(), after);
    }
    match (sort, filter.near.as_ref()) {
        // By the distance alone, which a location index gives nearest first (KNN) only when it
        // is the whole order: every listing as far as the last one comes too, and the id orders
        // those outside (push_order), many sharing their town's point.
        (Sort::Nearest, Some(near)) => {
            query.push(" ORDER BY ");
            near.push_distance(query);
            query.push(" FETCH FIRST (").push_bind(limit).push(") ROWS WITH TIES) l");
        }
        _ => {
            push_order(query, sort, filter.near.as_ref());
            query.push(" LIMIT ").push_bind(limit).push(") l");
        }
    }
}

/// How many of a search's matches, the newest, the best match first ranks; the rest follow them
/// newest first. Ranking every match costs as many heap reads as there are matches: against a
/// million listings a page of "makina" (252 000 matches) took 210 ms where newest first took
/// 5. The window is read as newest first reads its page, off idx_listings_active_sort, for as
/// long as the planner thinks walking it cheaper than reading every match: at 1 000 it judged
/// otherwise for "gjendje të mirë" (70 634 matches, 14 126 expected) and took 134 ms, at 240
/// it walks, 3 ms. Ten pages ranked; a search that finds fewer has them all ranked.
pub const RANK_WINDOW: i64 = 10 * PAGE_SIZE;

/// The best match first's window, as `l`: the newest RANK_WINDOW matches, each with its rank
/// (`rank`, ts_rank against the words; 0 for all where the words left nothing to search), the
/// first `limit` of them after the cursor by the rank, then the newest. The window's own LIMIT
/// keeps the ranking above it, to its rows alone.
fn push_ranked(query: &mut QueryBuilder<'_, Postgres>, filter: &ListingFilter, after: Option<Cursor>, limit: i64) {
    query.push("(SELECT w.* FROM (SELECT l.*, ");
    match filter.search.as_ref().and_then(|search| search.query.as_ref()) {
        Some(words) => {
            query.push("ts_rank(l.search_vector, ").push_bind(words.clone()).push("::tsquery)");
        }
        None => {
            query.push("0::real");
        }
    }
    query.push(" AS rank FROM (SELECT l.* FROM listings l");
    push_conditions(query, filter, false);
    query.push(" ORDER BY l.sort_at DESC, l.id DESC LIMIT ").push_bind(RANK_WINDOW).push(") l) w");
    if let Some(Cursor::Relevance { rank, sort_at, id }) = after {
        query.push(" WHERE (w.rank, w.sort_at, w.id) < (").push_bind(rank).push("::real, ");
        query.push_bind(sort_at).push(", ").push_bind(id).push(")");
    }
    query.push(" ORDER BY w.rank DESC, w.sort_at DESC, w.id DESC LIMIT ").push_bind(limit).push(") l");
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
        // The best match first's pages compare their own (relevance_page).
        Cursor::Relevance { .. } | Cursor::RelevanceRest { .. } => {
            query.push("NULL, NULL");
        }
    }
    query.push(")");
}

/// The sort's order, the id breaking ties; for the best match first, the rank, then the newest.
fn push_order(query: &mut QueryBuilder<'_, Postgres>, sort: Sort, near: Option<&Near>) {
    if sort == Sort::Relevance {
        query.push(" ORDER BY l.rank DESC, l.sort_at DESC, l.id DESC");
        return;
    }
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
    if !filter.exclude.is_empty() {
        query.push(" AND l.id <> ALL(").push_bind(filter.exclude.clone()).push(")");
    }
    if let Some(since) = filter.since {
        query.push(" AND l.sort_at > ").push_bind(since);
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
