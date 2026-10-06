//! The welcome page and the browse pages (a category, a search, or the map): listing grids with
//! filters, sorting and infinite scroll.

use askama::Template;
use axum::{
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::Response,
};

use super::{
    Result, cookie,
    filters::{self, Chip, Choice, ChoiceGroup, Field, FieldKind, Filters, Params},
    is_next_page_request, not_found, render,
};
use crate::{
    AppState,
    db::{self, Cursor, ListingCard, ListingFilter, Sort},
    format,
};

/// One page of listing cards, plus where the next page starts.
pub struct ListingPage {
    pub cards: Vec<ListingCard>,
    pub next: Option<NextPage>,
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
        Self { cards: rows, next }
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

pub struct Tile {
    pub name: String,
    pub url: String,
    pub count: String,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    cities: Vec<ChoiceGroup>,
    categories: Vec<Choice>,
    tiles: Vec<Tile>,
    page: ListingPage,
}

/// The welcome page asks what the visitor is looking for and where, then shows the newest
/// listings.
pub async fn home(
    State(state): State<AppState>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    let params = Params::parse(query.as_deref());
    let after = params.first("after").and_then(|value| Cursor::decode(Sort::Newest, value));
    let filter = ListingFilter { after, ..Default::default() };
    let next_url = |cursor: &str| format!("/?after={cursor}");
    if is_next_page_request(&headers) {
        let rows = db::listing_page(&state.db, &filter).await?;
        return frame_response(after, &ListingPage::new(rows, Sort::Newest, next_url));
    }

    let (rows, categories, cities, counts) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::categories(&state.db),
        db::cities(&state.db),
        db::listing_counts(&state.db),
    )?;
    // The city picked last time (see city_controller.js), if it still exists.
    let city = cookie(&headers, "city").filter(|slug| cities.iter().any(|c| &c.slug == slug));
    let tiles = categories
        .top_level()
        .map(|category| Tile {
            name: category.name.clone(),
            url: match &city {
                Some(city) => format!("/c/{}?city={city}", category.slug),
                None => format!("/c/{}", category.slug),
            },
            count: format::listing_count(categories.listing_count(category.id, &counts), i64::MAX),
        })
        .collect();
    render(HomePage {
        cities: filters::city_groups(&cities, city.as_deref()),
        categories: filters::category_choices(&categories, None),
        tiles,
        page: ListingPage::new(rows, Sort::Newest, next_url),
    })
}

pub struct Link {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "browse.html")]
struct BrowsePage<'a> {
    heading: String,
    count: String,
    filters: &'a Filters<'a>,
    fields: Vec<Field>,
    chips: Vec<Chip>,
    clear_url: String,
    breadcrumbs: Vec<Link>,
    subcategories: Vec<Link>,
    /// On a category page, the menu the heading opens to go to another category.
    category_menu: Vec<MenuEntry>,
    page: ListingPage,
    /// With the map open (harta=1), a dot for each result, up to db::MAP_POINTS.
    points: Vec<db::MapPoint>,
    more_points: bool,
}

/// A line in the category menu: a top-level category, or one of its subcategories under it.
pub struct MenuEntry {
    pub name: String,
    pub url: String,
    pub top: bool,
    pub current: bool,
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
    let words = params.first("q").map(|q| q.trim().chars().take(100).collect::<String>()).filter(|q| !q.is_empty());
    let filter = ListingFilter { seller: Some(profile.seller), query: words.clone(), after, ..Default::default() };
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
    render(ProfilePage {
        path,
        query: words,
        found,
        initial: format::initial(&profile.name),
        since: format::month_year(profile.since),
        count: format::listing_count(profile.listings, i64::MAX),
        business: matches!(profile.seller, db::Seller::Business(_)),
        name: profile.name,
        page,
    })
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
    let params = Params::parse(query.as_deref());
    let category_slug = page_slug.as_deref().or(params.first("category"));
    let (categories, cities, attributes) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        async {
            match category_slug {
                Some(slug) => db::filter_attributes(&state.db, slug).await,
                None => Ok(Vec::new()),
            }
        },
    )?;
    let category = category_slug.and_then(|slug| categories.by_slug(slug));
    let path = match (&page_slug, category, view) {
        (Some(_), None, _) => return not_found().await,
        (Some(slug), Some(_), _) => format!("/c/{slug}"),
        (None, _, View::Grid) => "/search".to_string(),
        (None, _, View::Map) => "/harta".to_string(),
    };
    let filters = Filters::new(
        &params,
        path,
        category,
        page_slug.is_none(),
        &categories,
        &cities,
        &attributes,
    );
    let filter = filters.listing_filter();
    let next_url = |cursor: &str| filters.next_url(cursor);

    // In a search limited to a city, the cards in that city leave it out (ListingCard::city_label).
    let mark_city = |rows: &mut Vec<ListingCard>| {
        if let Some(city) = filters.city {
            for card in rows.iter_mut() {
                card.city_known = card.city_label() == Some(city.name.as_str());
            }
        }
    };

    if is_next_page_request(&headers) {
        let mut rows = db::listing_page(&state.db, &filter).await?;
        mark_city(&mut rows);
        return frame_response(filter.after, &ListingPage::new(rows, filter.sort, next_url));
    }

    let (mut rows, count, mut points) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::listing_count(&state.db, &filter),
        async {
            if view == View::Map || filters.map_open {
                db::map_points(&state.db, &filter).await
            } else {
                Ok(Vec::new())
            }
        },
    )?;
    let count = format::listing_count(count, db::COUNT_LIMIT);
    mark_city(&mut rows);
    let page = ListingPage::new(rows, filter.sort, next_url);
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
    let link = |category: &db::Category| Link {
        name: category.name.clone(),
        url: filters.category_url(category),
    };
    let breadcrumbs = match (&page_slug, category) {
        (Some(_), Some(category)) => categories.ancestors(category).into_iter().map(link).collect(),
        _ => Vec::new(),
    };
    let subcategories = category
        .map(|category| categories.children(category.id).map(link).collect())
        .unwrap_or_default();
    // Every category, each followed by its subcategories; the links keep the words, the town
    // and the map pin, as the category links do.
    let category_menu = match (&page_slug, category) {
        (Some(_), Some(current)) => {
            let entry = |category: &db::Category, top: bool| MenuEntry {
                name: category.name.clone(),
                url: filters.category_url(category),
                top,
                current: category.id == current.id,
            };
            let mut menu = vec![MenuEntry { name: "Të gjitha njoftimet".to_string(), url: filters.all_categories_url(), top: true, current: false }];
            for top in categories.top_level() {
                menu.push(entry(top, true));
                menu.extend(categories.children(top.id).map(|child| entry(child, false)));
            }
            menu
        }
        _ => Vec::new(),
    };
    render(BrowsePage {
        heading,
        count,
        fields: filters.fields(),
        chips: filters.chips(),
        clear_url: filters.clear_url(),
        filters: &filters,
        breadcrumbs,
        subcategories,
        category_menu,
        page,
        points,
        more_points,
    })
}
