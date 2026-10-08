//! The welcome page and the browse pages (a category, a search, or the map): listing grids with
//! filters, sorting and infinite scroll.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use askama::Template;
use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, HeaderValue, header},
    response::{IntoResponse, Redirect, Response},
};
use rust_decimal::Decimal;
use uuid::Uuid;

use super::{
    Result, cookie,
    filters::{Chip, Field, FieldKind, Filters, Params, PriceChoice, TOWN_COOKIE},
    icons, is_next_page_request, named, newest, not_found, render,
    safety,
    seo::{self, PageMeta},
};
use crate::{
    AppState,
    db::{self, Cursor, ListingCard, ListingFilter, Sort},
    format, views,
};

/// One page of listing cards, plus where the next page starts.
pub struct ListingPage {
    pub cards: Vec<ListingCard>,
    pub next: Option<NextPage>,
    /// Big cards, one a line, every photo to slide through, as the visitor chose, or as the
    /// category shows them by default (big_cards).
    pub big: bool,
    /// How many cards load their photo straight away, the first row's; the rest wait until they
    /// come near the screen (listing_card.html).
    pub eager: usize,
}

/// How wide a card's photo shows, for the browser to pick the copy it fetches (srcset in
/// listing_card.html): a column of the grid, or for a big card the line, no wider than 40rem, and
/// half the page's 1100px on a wide screen (page.css, .cards-big). cards_controller.js switches
/// between them as the view button switches the cards.
const SMALL_CARD_SIZES: &str = "(min-width: 720px) 260px, 50vw";
const BIG_CARD_SIZES: &str = "(min-width: 960px) 535px, (min-width: 720px) 40rem, 100vw";

/// The cookie the view button sets (cards_controller.js): "big" or "small".
const CARDS_COOKIE: &str = "cards";

/// The main categories whose pages, and those of the categories under them, show big cards until
/// the visitor chooses small ones there: the photos say the most about a home. The choice made on
/// them is kept apart ("cards-prona"), so that one made on other pages leaves them as they are.
const BIG_CARDS_IN: [&str; 1] = ["prona"];

/// The view button's cookie on a page of `category`, and whether its cards are big without one.
fn cards_cookie(categories: &db::Categories, category: Option<&db::Category>) -> (String, bool) {
    let main = category.map(|category| categories.ancestors(category).first().copied().unwrap_or(category));
    match main.filter(|main| BIG_CARDS_IN.contains(&main.slug.as_str())) {
        Some(main) => (format!("{CARDS_COOKIE}-{}", main.slug), true),
        None => (CARDS_COOKIE.to_string(), false),
    }
}

/// Whether the page shows big cards: as the visitor chose with `cookie`, or else as it does by
/// default.
fn big_cards(headers: &HeaderMap, cookie_name: &str, by_default: bool) -> bool {
    match cookie(headers, cookie_name).as_deref() {
        Some("big") => true,
        Some("small") => false,
        _ => by_default,
    }
}

/// A lazily loaded `<turbo-frame>` that fetches the next page when it scrolls into view.
pub struct NextPage {
    pub frame_id: String,
    pub url: String,
}

impl ListingPage {
    /// `rows` holds up to PAGE_SIZE + 1 listings; the extra one only shows that more exist.
    fn new(mut rows: Vec<ListingCard>, sort: Sort, next_url: impl Fn(&str) -> String) -> Self {
        let has_more = rows.len() > db::PAGE_SIZE as usize;
        rows.truncate(db::PAGE_SIZE as usize);
        let next = has_more.then(|| rows.last()).flatten().map(|last| {
            let cursor = Cursor::after(sort, last);
            NextPage { frame_id: frame_id(&cursor), url: next_url(&cursor.encode()) }
        });
        Self { cards: rows, next, big: false, eager: 4 }
    }

    /// The cards under a listing's page, the seller's other listings and those like it
    /// (db::seller_listings, db::related_listings): small, none of them loaded before they come
    /// near the screen, as they come after the listing.
    pub fn related(cards: Vec<ListingCard>) -> Self {
        Self { cards, next: None, big: false, eager: 0 }
    }

    fn big(mut self, big: bool) -> Self {
        self.big = big;
        self
    }

    pub fn sizes(&self) -> &'static str {
        if self.big { BIG_CARD_SIZES } else { SMALL_CARD_SIZES }
    }

    pub fn small_sizes(&self) -> &'static str {
        SMALL_CARD_SIZES
    }

    pub fn big_sizes(&self) -> &'static str {
        BIG_CARD_SIZES
    }
}

/// Each frame is named after the listing it follows, which is unique on the page.
fn frame_id(cursor: &Cursor) -> String {
    format!("page-{}", cursor.id())
}

/// The response to an infinite-scroll request: the cards inside the frame Turbo asked for.
#[derive(Template)]
#[template(path = "listing_frame.html")]
struct ListingFrame<'a> {
    frame_id: String,
    page: &'a ListingPage,
}

fn frame_response(after: Option<Cursor>, page: &ListingPage) -> Result<Response> {
    let frame_id = after.as_ref().map(frame_id).unwrap_or_default();
    render(ListingFrame { frame_id, page })
}

/// A shortcut on the welcome page.
pub struct Shortcut {
    pub name: String,
    pub url: String,
    /// The icon's SVG inside (icons::main_category).
    pub icon: &'static str,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    shortcuts: Vec<Shortcut>,
    page: ListingPage,
    /// How many of the page's cards, the newest, the row of the newest listings shows.
    strip: usize,
    /// Where the row's stream of new listings starts: after the newest (none without listings).
    since: Option<String>,
    /// The listings seen the most this week ("Më të klikuarat"), none before any are.
    most_viewed: Option<ListingPage>,
    meta: PageMeta,
}

/// The welcome page asks what the visitor is looking for, then shows the newest listings in a row
/// that follows new ones (web/newest.rs), the most seen this week in another (db::most_viewed),
/// then all of them, newest first.
pub async fn home(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    let params = Params::parse(query.as_deref());
    let after = params.first("after").and_then(|value| Cursor::decode(Sort::Newest, value));
    let hidden = safety::hidden_accounts(&state, &headers).await?;
    let filter = ListingFilter { after, hidden_accounts: hidden.clone(), ..Default::default() };
    let next_url = |cursor: &str| format!("/?after={cursor}");
    if is_next_page_request(&headers) {
        let rows = db::listing_page(&state.db, &filter).await?;
        return frame_response(after, &ListingPage::new(rows, Sort::Newest, next_url));
    }

    let (rows, categories, mut most_viewed) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::categories(&state.db),
        state.views.most_viewed(&state.db, None),
    )?;
    safety::without_hidden(&mut most_viewed, &hidden);
    // Every main category, in its order, with its icon.
    let shortcuts = categories
        .top_level()
        .map(|category| Shortcut {
            name: category.name.clone(),
            url: format!("/c/{}", category.slug),
            icon: icons::main_category(&category.slug),
        })
        .collect();
    let page = ListingPage::new(rows, Sort::Newest, next_url);
    let strip = page.cards.len().min(newest::STRIP);
    let since = page.cards.first().map(ListingCard::newest_cursor);
    let most_viewed = (!most_viewed.is_empty()).then(|| ListingPage::related(most_viewed));
    let origin = seo::origin(&headers);
    let meta = PageMeta::new(&origin, "Pluribus: njoftime në gjithë Shqipërinë", seo::SITE_DESCRIPTION, format!("{origin}/"));
    render(HomePage { shortcuts, page, strip, since, most_viewed, meta })
}

/// A category's icon, as its pill has it (icons.rs): a main category's own, a subcategory's where
/// it has one.
fn category_icon(category: &db::Category) -> Option<&'static str> {
    match category.parent_id {
        None => Some(icons::main_category(&category.slug)),
        Some(_) => icons::subcategory(&category.slug),
    }
}

/// A category as a pill like the welcome page's main categories: its icon, where it has one, then
/// its name. A category page's subcategories, the category above it, and those beside it.
pub struct Subcategory {
    pub name: String,
    pub url: String,
    pub icon: Option<&'static str>,
}

/// The most categories a search across every category names, where its listings are.
const FOUND_IN: i64 = 6;

/// A category a search across every category found listings in, which narrows the search to it:
/// its name, and how many it found there, unless too many were found to count.
pub struct FoundIn {
    pub name: String,
    pub url: String,
    pub count: String,
}

#[derive(Template)]
#[template(path = "browse.html")]
struct BrowsePage<'a> {
    heading: String,
    /// On a category page, the listings' own heading over them, under "Më të klikuarat" where it
    /// is: the order, the category and the kind of listing when one is chosen ("Njoftimet më të
    /// reja në Makina në shitje"). Elsewhere the order alone heads them, for screen readers.
    listings_heading: Option<String>,
    count: String,
    /// The words searched in place of misspelt ones, quoted ("„golf“"), or empty.
    corrections: String,
    filters: &'a Filters<'a>,
    fields: Vec<Field>,
    chips: Vec<Chip>,
    clear_url: String,
    /// On a category page, the category above it, the row's first pill: a way up.
    parent: Option<Subcategory>,
    /// On a category page, its own icon, on its pill.
    heading_icon: Option<&'static str>,
    subcategories: Vec<Subcategory>,
    /// On a search across every category, the categories its listings are in, the most first.
    found_in: Vec<FoundIn>,
    /// On a category page, the other categories at its level, which its own pill shows as pills
    /// after it: those beside it; for a main category, the other main ones, after "Të gjitha
    /// njoftimet". None on a search, whose heading is just a heading.
    level: Option<Vec<Subcategory>>,
    /// The cookie the view button sets on this page (cards_cookie).
    cards_cookie: String,
    /// In a category whose page shows its own most seen (views::has_own_most_viewed), with no
    /// filter on, its listings seen the most this week ("Më të klikuarat"), none before any are.
    most_viewed: Option<ListingPage>,
    page: ListingPage,
    /// With the map's dots asked for (harta=1), a dot for each result, up to db::MAP_POINTS.
    points: Vec<db::MapPoint>,
    meta: PageMeta,
    /// The count stopped at db::COUNT_LIMIT: the page asks for one up to db::PRECISE_COUNT_LIMIT
    /// once it shows (filters_controller.js, numri=1).
    capped: bool,
    /// What the filters' card's button says it shows: "Shfaq 24 njoftime", or "Asnjë njoftim".
    show_label: String,
    /// The price's quick choices, from the category's prices (filters.rs, price_choices).
    price_choices: Vec<PriceChoice>,
    /// Some of the filters wait under "Më shumë filtra" (Field::primary), and whether it opens
    /// with the page: one of them is on, or the kind of listing waiting there is.
    has_more: bool,
    more_open: bool,
}

/// How long a category's price quarters, its price's quick choices, are kept before they are
/// worked out again: every listing priced counts (db::price_quarters), and they move slowly.
const PRICE_QUARTERS_HOURS: u64 = 6;

/// The price quarters by database (each test has one of its own), category and currency, with
/// when they were worked out.
type KeptQuarters = HashMap<(String, Uuid, String), (Instant, Option<[Decimal; 3]>)>;

fn kept_quarters() -> &'static Mutex<KeptQuarters> {
    static KEPT: OnceLock<Mutex<KeptQuarters>> = OnceLock::new();
    KEPT.get_or_init(Default::default)
}

/// The category's price quarters in its usual currency, as kept, or worked out again once
/// PRICE_QUARTERS_HOURS have gone.
async fn price_quarters(
    state: &AppState,
    categories: &db::Categories,
    category: &db::Category,
) -> sqlx::Result<Option<[Decimal; 3]>> {
    let database = state.db.connect_options().get_database().unwrap_or_default().to_string();
    let key = (database, category.id, category.default_currency.clone());
    let fresh = Duration::from_secs(PRICE_QUARTERS_HOURS * 3600);
    if let Some((at, quarters)) = kept_quarters().lock().unwrap().get(&key) {
        if at.elapsed() < fresh {
            return Ok(*quarters);
        }
    }
    let quarters = db::price_quarters(&state.db, &categories.subtree_ids(category.id), &category.default_currency).await?;
    kept_quarters().lock().unwrap().insert(key, (Instant::now(), quarters));
    Ok(quarters)
}

/// What the filters' card's button says it shows, for a count as format::listing_count says it.
fn show_label(count: i64, count_text: &str) -> String {
    if count == 0 { "Asnjë njoftim".to_string() } else { format!("Shfaq {count_text}") }
}

/// A seller's profile: a private seller's (/perdorues/{id}) or a business's (/biznes/{slug}),
/// with their active listings, newest first, and more as the page scrolls.
#[derive(Template)]
#[template(path = "profile.html")]
struct ProfilePage {
    name: String,
    initial: String,
    since: String,
    count: String,
    business: bool,
    /// The profile's address, which its search form sends to.
    path: String,
    /// Searching this seller's listings: the words, and how many match.
    query: Option<String>,
    found: String,
    page: ListingPage,
    meta: PageMeta,
    /// "Numër i verifikuar" when the number they signed in with is theirs, or the address.
    verified: Option<&'static str>,
}

pub async fn user_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    let Ok(id) = id.parse::<uuid::Uuid>() else {
        return not_found().await;
    };
    let Some(profile) = db::user_profile(&state.db, id).await? else {
        return not_found().await;
    };
    profile_page(&state, &headers, query, profile, format!("/perdorues/{id}")).await
}

pub async fn business_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    let Some(profile) = db::business_profile(&state.db, &slug).await? else {
        return not_found().await;
    };
    profile_page(&state, &headers, query, profile, format!("/biznes/{slug}")).await
}

async fn profile_page(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<String>,
    profile: db::Profile,
    path: String,
) -> Result<Response> {
    let params = Params::parse(query.as_deref());
    let after = params.first("after").and_then(|value| Cursor::decode(Sort::Newest, value));
    // A search within this seller's listings, kept as the list scrolls on.
    let words = params.search_words();
    let search = db::text_search(&state.db, words.as_deref()).await?;
    let filter = ListingFilter { seller: Some(profile.seller), search, after, ..Default::default() };
    let next_url = |cursor: &str| match &words {
        Some(q) => format!("{path}?{}", form_urlencoded::Serializer::new(String::new()).append_pair("q", q).append_pair("after", cursor).finish()),
        None => format!("{path}?after={cursor}"),
    };
    let rows = db::listing_page(&state.db, &filter).await?;
    let page = ListingPage::new(rows, Sort::Newest, next_url);
    if is_next_page_request(headers) {
        return frame_response(after, &page);
    }
    let found = match &words {
        Some(_) => format::listing_count(db::listing_count(&state.db, &filter).await?, db::COUNT_LIMIT),
        None => String::new(),
    };
    // What the page says about itself (seo.rs); a search among the seller's listings is not for
    // search engines to list.
    let origin = seo::origin(headers);
    let since = format::month_year(profile.since);
    let count = format::listing_count(profile.listings, i64::MAX);
    let mut meta = PageMeta::new(
        &origin,
        profile.name.clone(),
        format!("{} në Pluribus që nga {since}: {count}.", profile.name),
        seo::canonical(&origin, &path, query.as_deref()),
    );
    if let Some((card, photo)) = page.cards.iter().find_map(|card| Some((card, card.photos_full.first()?))) {
        meta = meta.with_image(&origin, photo, card.title.clone());
    }
    meta.noindex = words.is_some();
    render(ProfilePage {
        meta,
        path,
        query: words,
        found,
        initial: format::initial(&profile.name),
        since,
        count,
        business: matches!(profile.seller, db::Seller::Business(_)),
        verified: verified_label(profile.verified.as_deref()),
        name: profile.name,
        page,
    })
}

/// What a seller's page and listings say of the contact they proved theirs by signing in with it.
pub fn verified_label(kind: Option<&str>) -> Option<&'static str> {
    match kind? {
        "EMAIL" => Some("Email i verifikuar"),
        _ => Some("Numër i verifikuar"),
    }
}

/// The map page: drop a pin, pick a distance, and see the listings around it.
#[derive(Template)]
#[template(path = "map.html")]
struct MapPage<'a> {
    count: String,
    filters: &'a Filters<'a>,
    chips: Vec<Chip>,
    page: ListingPage,
    points: Vec<db::MapPoint>,
    /// Whether more results have a point than the map shows.
    more_points: bool,
}

/// How a browse page shows its listings: as a grid with filters, or beside a map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Grid,
    Map,
}

/// /c/{slug}: a category and its subcategories.
pub async fn category(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    browse(state, headers, query, Some(slug), View::Grid).await
}

/// /search: every category, or the one the category parameter names.
pub async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    browse(state, headers, query, None, View::Grid).await
}

/// /harta: the listings around a pin on the map (lat, lng and radius), nearest first.
pub async fn map(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    browse(state, headers, query, None, View::Map).await
}

async fn browse(
    state: AppState,
    headers: HeaderMap,
    query: Option<String>,
    page_slug: Option<String>,
    view: View,
) -> Result<Response> {
    // A category or search page whose address names no place opens in the town the visitor
    // picked last (filters.rs, TOWN_COOKIE); the map page has its own point.
    let town = (view == View::Grid).then(|| cookie(&headers, TOWN_COOKIE)).flatten();
    let params = Params::parse(query.as_deref()).remembering_town(town);
    let category_slug = page_slug.as_deref().or(params.first("category"));
    let words = params.search_words();
    let (categories, cities, attributes, search, hidden) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        async {
            match category_slug {
                Some(slug) => db::filter_attributes(&state.db, slug).await,
                None => Ok(Vec::new()),
            }
        },
        db::text_search(&state.db, words.as_deref()),
        safety::hidden_accounts(&state, &headers),
    )?;
    let category = category_slug.and_then(|slug| categories.by_slug(slug));
    let path = match (&page_slug, category, view) {
        (Some(_), None, _) => return not_found().await,
        (Some(slug), Some(_), _) => format!("/c/{slug}"),
        (None, _, View::Grid) => "/search".to_string(),
        (None, _, View::Map) => "/harta".to_string(),
    };
    // Words that name a category, a kind of listing or a town go to the page with those filters
    // on, the rest of them searched there (named.rs). Not the next page or the count, which a
    // page asks for that did so already, nor the map, whose pin says where.
    if view == View::Grid && !is_next_page_request(&headers) && params.first("numri").is_none() {
        if let Some(named) = words.as_deref().and_then(|words| named::read(words, category, &categories, &cities)) {
            return Ok(Redirect::to(&named::url_for(&path, query.as_deref(), &named)).into_response());
        }
    }
    let canonical_path = path.clone();
    let filters = Filters::new(
        &params,
        path,
        category,
        page_slug.is_none(),
        &categories,
        &cities,
        &attributes,
    );
    // The words searched in place of misspelt ones, which the heading's line names.
    let corrections = search.as_ref().map(|search| format::quoted_list(&search.corrections)).unwrap_or_default();
    let mut filter = filters.listing_filter(search);
    filter.hidden_accounts = hidden;
    let next_url = |cursor: &str| filters.next_url(cursor);

    // Only the count, further than the page counts (db::PRECISE_COUNT_LIMIT), which the page asks
    // for once it shows when its own stopped at db::COUNT_LIMIT (filters_controller.js):
    // {"count": 3412, "text": "3 412 njoftime", "show": "Shfaq 3 412 njoftime"}.
    if params.first("numri") == Some("1") {
        let count = db::listing_count_up_to(&state.db, &filter, db::PRECISE_COUNT_LIMIT).await?;
        let text = format::listing_count(count, db::PRECISE_COUNT_LIMIT);
        let body = serde_json::json!({ "count": count, "show": show_label(count, &text), "text": text });
        return Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(body)).into_response());
    }

    // In a search limited to a city, the cards in that city leave it out (ListingCard::city_label).
    let mark_city = |rows: &mut Vec<ListingCard>| {
        if let Some(city) = filters.city {
            for card in rows.iter_mut() {
                card.city_known = card.city_label() == Some(city.name.as_str());
            }
        }
    };

    // Big cards only in the grid: the map page's list stays small, beside the map.
    let (cards_cookie, big_by_default) = cards_cookie(&categories, category);
    let big = view == View::Grid && big_cards(&headers, &cards_cookie, big_by_default);

    if is_next_page_request(&headers) {
        let mut rows = db::listing_page(&state.db, &filter).await?;
        mark_city(&mut rows);
        return frame_response(filter.after, &ListingPage::new(rows, filter.sort, next_url).big(big));
    }

    // Every main category's page, and those of a few big subcategories, show their own most seen,
    // as the welcome page does for every category: the listings seen the most this week in the
    // category and those under it (views::has_own_most_viewed); with no words and no filter on,
    // as they take no notice of them, and would only be in the way of the listings asked for.
    let most_viewed_in = match (&page_slug, category, view) {
        (Some(_), Some(category), View::Grid)
            if views::has_own_most_viewed(category) && filters.q.is_none() && filters.chips().is_empty() =>
        {
            Some((category.id, categories.subtree_ids(category.id)))
        }
        _ => None,
    };
    let (mut rows, count, mut points, found_in, mut most_viewed, quarters) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::listing_count(&state.db, &filter),
        async {
            if view == View::Map || filters.map_points {
                db::map_points(&state.db, &filter).await
            } else {
                Ok(Vec::new())
            }
        },
        async {
            // A search across every category: the categories its listings are in.
            match (view, category, &filter.search) {
                (View::Grid, None, Some(_)) => db::listing_categories(&state.db, &filter, FOUND_IN).await,
                _ => Ok(Vec::new()),
            }
        },
        async {
            match &most_viewed_in {
                Some((id, ids)) => state.views.most_viewed(&state.db, Some((*id, ids))).await,
                None => Ok(Vec::new()),
            }
        },
        async {
            // The price's quick choices, from the category's prices: on its grid only.
            match (view, category) {
                (View::Grid, Some(category)) => price_quarters(&state, &categories, category).await,
                _ => Ok(None),
            }
        },
    )?;
    safety::without_hidden(&mut most_viewed, &filter.hidden_accounts);
    let most_viewed = (!most_viewed.is_empty()).then(|| ListingPage::related(most_viewed));
    // Counted among at most COUNT_LIMIT + 1 listings, as the count is: past that, numbers would
    // mislead, and the names go alone.
    let counted = count <= db::COUNT_LIMIT;
    let found_in = found_in
        .into_iter()
        .filter_map(|(id, listings)| {
            let category = categories.by_id(id)?;
            Some(FoundIn {
                name: category.name.clone(),
                url: filters.category_url(category),
                count: if counted { listings.to_string() } else { String::new() },
            })
        })
        .collect();
    let capped = count > db::COUNT_LIMIT;
    let count_number = count;
    let count = format::listing_count(count, db::COUNT_LIMIT);
    mark_city(&mut rows);
    let page = ListingPage::new(rows, filter.sort, next_url).big(big);
    let more_points = points.len() as i64 > db::MAP_POINTS;
    points.truncate(db::MAP_POINTS as usize);
    if view == View::Map {
        let chips = filters.chips();
        return render(MapPage { count, filters: &filters, chips, page, points, more_points });
    }

    let heading = match (&filters.q, category) {
        (Some(q), Some(category)) => format!("„{q}“ në {}", category.name),
        (Some(q), None) => format!("Rezultatet për „{q}“"),
        (None, Some(category)) => category.name.clone(),
        (None, None) => match filters.city {
            // Where a plate code leads: all of a town's listings.
            Some(city) => format!("Njoftimet në {}", city.name),
            None => "Të gjitha njoftimet".to_string(),
        },
    };
    // The category above this one, the row's first pill: a way up; and this one's own icon.
    let parent = match (&page_slug, category) {
        (Some(_), Some(current)) => current.parent_id.and_then(|id| categories.by_id(id)).map(|above| Subcategory {
            name: above.name.clone(),
            url: filters.category_url(above),
            icon: category_icon(above),
        }),
        _ => None,
    };
    let heading_icon = match (&page_slug, category) {
        (Some(_), Some(current)) => category_icon(current),
        _ => None,
    };
    // The listings' own heading on a category page: the order as what it makes of them, the
    // category and, when one is chosen, the kind of listing, in small letters after the name
    // ("Njoftimet më të reja në Makina në shitje", "Njoftimet më të lira në Prona me qira").
    let listings_heading = match (&page_slug, category) {
        (Some(_), Some(current)) => {
            let mut heading = format!("Njoftimet {} në {}", filters.sort_adjective(), current.name);
            if let Some(kind) = &filters.listing_type {
                heading.push(' ');
                heading.push_str(&format::listing_type(kind).to_lowercase());
            }
            Some(heading)
        }
        _ => None,
    };
    // The subcategories narrow the page, so their links keep its filters as well.
    let subcategories: Vec<Subcategory> = category
        .map(|category| {
            categories
                .children(category.id)
                .map(|child| Subcategory {
                    name: child.name.clone(),
                    url: filters.narrowed_url(child),
                    icon: icons::subcategory(&child.slug),
                })
                .collect()
        })
        .unwrap_or_default();
    // The other categories at this one's level, which its pill shows as pills after it: those
    // beside it, not itself; for a main category, the other main ones, after "Të gjitha
    // njoftimet". The links keep the words, the town and the map pin, as the category links do.
    let level: Option<Vec<Subcategory>> = match (&page_slug, category) {
        (Some(_), Some(current)) => {
            let others = |category: &&db::Category| category.id != current.id;
            let pill = |category: &db::Category| Subcategory {
                name: category.name.clone(),
                url: filters.category_url(category),
                icon: category_icon(category),
            };
            Some(match current.parent_id {
                Some(parent) => categories.children(parent).filter(others).map(pill).collect(),
                None => {
                    let all = Subcategory { name: "Të gjitha njoftimet".to_string(), url: filters.all_categories_url(), icon: None };
                    std::iter::once(all).chain(categories.top_level().filter(others).map(pill)).collect()
                }
            })
        }
        _ => None,
    };
    // What the page says about itself (seo.rs), its first listing's photo for the preview; a
    // search's results are not for search engines to list, a category's page is, with its filters.
    let origin = seo::origin(&headers);
    let description = match (category, &filters.q) {
        (Some(_), None) => format!("{heading}: {count} në Pluribus, me foto, çmime dhe vendndodhje."),
        _ => format!("{heading}: {count} në Pluribus."),
    };
    let mut meta = PageMeta::new(&origin, heading.clone(), description, seo::canonical(&origin, &canonical_path, query.as_deref()));
    if let Some((card, photo)) = page.cards.iter().find_map(|card| Some((card, card.photos_full.first()?))) {
        meta = meta.with_image(&origin, photo, card.title.clone());
    }
    meta.noindex = page_slug.is_none() || filters.q.is_some();
    // Where a category page stands, for the trail search engines show under a result (schema.org's
    // BreadcrumbList): the welcome page, the categories above, and the page's own.
    if let (Some(_), Some(current)) = (&page_slug, category) {
        let trail = categories.ancestors(current).into_iter().chain([current]);
        let items: Vec<serde_json::Value> = std::iter::once(("Pluribus".to_string(), format!("{origin}/")))
            .chain(trail.map(|category| (category.name.clone(), format!("{origin}/c/{}", category.slug))))
            .enumerate()
            .map(|(at, (name, item))| serde_json::json!({"@type": "ListItem", "position": at + 1, "name": name, "item": item}))
            .collect();
        meta.json_ld = Some(seo::json_ld(&serde_json::json!({
            "@context": "https://schema.org",
            "@type": "BreadcrumbList",
            "itemListElement": items,
        })));
    }
    let fields = filters.fields();
    let has_more = fields.iter().any(|field| !field.primary);
    let type_waits = !filters.type_first() && filters.type_chosen();
    let more_open = fields.iter().any(|field| !field.primary && field.active) || (has_more && type_waits);
    let price_choices = quarters.map(|quarters| filters.price_choices(quarters)).unwrap_or_default();
    render(BrowsePage {
        meta,
        show_label: show_label(count_number, &count),
        heading,
        listings_heading,
        count,
        capped,
        corrections,
        price_choices,
        has_more,
        more_open,
        fields,
        chips: filters.chips(),
        clear_url: filters.clear_url(),
        filters: &filters,
        parent,
        heading_icon,
        subcategories,
        found_in,
        level,
        cards_cookie,
        most_viewed,
        page,
        points,
    })
}
