//! The welcome page and the browse pages (a category, a search, or the map): listing grids with
//! filters, sorting and infinite scroll.

use askama::Template;
use axum::{
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::Response,
};

use super::{
    Result,
    filters::{Chip, Field, FieldKind, Filters, Params},
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

/// The icons of the welcome page's shortcuts under the search, one for each main category (an
/// SVG's inside, in the 24-unit box the site's line icons share). A main category added later
/// gets BOX until it has one of its own.
const SHORTCUTS: [(&str, &str); 11] = [
    // A car, from the side.
    ("automjete", r#"<path d="M5.5 11 7 7a2 2 0 0 1 1.9-1.4h6.2A2 2 0 0 1 17 7l1.5 4"/><path d="M5.2 16.5H4a1 1 0 0 1-1-1V13a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v2.5a1 1 0 0 1-1 1h-1.2"/><path d="M9.8 16.5h4.4"/><circle cx="7.5" cy="16.5" r="2.2"/><circle cx="16.5" cy="16.5" r="2.2"/>"#),
    // A house.
    ("prona", r#"<path d="M3.5 10.5 12 3.5l8.5 7"/><path d="M5.5 9v10.5a1 1 0 0 0 1 1H10v-5.5h4v5.5h3.5a1 1 0 0 0 1-1V9"/>"#),
    // A phone.
    ("elektronike", r#"<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M10.5 18h3"/>"#),
    // A sofa.
    ("shtepi-dhe-kopsht", r#"<path d="M5.5 11V8.5A2.5 2.5 0 0 1 8 6h8a2.5 2.5 0 0 1 2.5 2.5V11"/><path d="M3.5 17.5v-5a2 2 0 0 1 4 0V14h9v-1.5a2 2 0 0 1 4 0v5z"/><path d="M5.5 17.5V20M18.5 17.5V20"/>"#),
    // A briefcase.
    ("pune", r#"<rect x="3" y="7" width="18" height="13" rx="2.5"/><path d="M9 7V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V7"/><path d="M3 12.5h18"/>"#),
    // A hammer.
    ("sherbime", r#"<path d="m4.5 19.5 9-9"/><rect x="11" y="3.8" width="10" height="5.2" rx="1.3" transform="rotate(45 16 6.4)"/>"#),
    // A T-shirt.
    ("veshje-dhe-aksesore", r#"<path d="M8.5 3.5 3.5 6l-1.5 4.5 3.5 1.5v8.5h13V12l3.5-1.5-1.5-4.5-5-2.5a3.5 3.5 0 0 1-7 0z"/>"#),
    // A tennis ball.
    ("sport-dhe-hobi", r#"<circle cx="12" cy="12" r="8.5"/><path d="M5.8 6.2a8.5 8.5 0 0 1 0 11.6M18.2 6.2a8.5 8.5 0 0 0 0 11.6"/>"#),
    // A paw.
    ("kafshe-shtepiake", r#"<circle cx="8.5" cy="5.5" r="1.8"/><circle cx="15.5" cy="5.5" r="1.8"/><circle cx="4.8" cy="10.5" r="1.7"/><circle cx="19.2" cy="10.5" r="1.7"/><path d="M12 10.5c-2.6 0-5 3.2-5 5.8 0 1.6 1.2 2.7 2.6 2.7 1 0 1.6-.5 2.4-.5s1.4.5 2.4.5c1.4 0 2.6-1.1 2.6-2.7 0-2.6-2.4-5.8-5-5.8z"/>"#),
    // A factory.
    ("makineri-dhe-biznes", r#"<path d="M3 20.5V10l5 3v-3l5 3v-3l5 3V4.5h3v16z"/><path d="M7 17h2M11.5 17h2M16 17h2"/>"#),
    // A box: other things.
    ("te-tjera", BOX),
];

/// Other things, and a main category without an icon of its own yet.
const BOX: &str = r#"<path d="M3.5 7.5 12 3l8.5 4.5v9L12 21l-8.5-4.5z"/><path d="M3.5 7.5 12 12l8.5-4.5"/><path d="M12 12v9"/>"#;

/// A shortcut on the welcome page.
pub struct Shortcut {
    pub name: String,
    pub url: String,
    /// The icon's SVG inside, from SHORTCUTS.
    pub icon: &'static str,
}

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    shortcuts: Vec<Shortcut>,
    page: ListingPage,
}

/// The welcome page asks what the visitor is looking for, then shows the newest listings.
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

    let (rows, categories) =
        tokio::try_join!(db::listing_page(&state.db, &filter), db::categories(&state.db))?;
    // Every main category, in its order, with its icon.
    let shortcuts = categories
        .top_level()
        .map(|category| Shortcut {
            name: category.name.clone(),
            url: format!("/c/{}", category.slug),
            icon: SHORTCUTS.iter().find(|(slug, _)| *slug == category.slug).map_or(BOX, |(_, icon)| *icon),
        })
        .collect();
    render(HomePage { shortcuts, page: ListingPage::new(rows, Sort::Newest, next_url) })
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
    category_menu: Vec<MenuItem>,
    page: ListingPage,
    /// With the map open (harta=1), a dot for each result, up to db::MAP_POINTS.
    points: Vec<db::MapPoint>,
    more_points: bool,
}

/// A category in the category menu.
pub struct MenuEntry {
    pub name: String,
    pub url: String,
    pub current: bool,
}

/// A category with subcategories in the category menu, folded unless the page's category is it or
/// is under it.
pub struct MenuBranch {
    pub entry: MenuEntry,
    pub open: bool,
}

/// The category menu, line by line: a category with subcategories opens a branch, which an `End`
/// closes once they are listed; one without is a leaf. browse.html nests them as it goes.
pub enum MenuItem {
    Branch(MenuBranch),
    Leaf(MenuEntry),
    End,
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
    // The subcategories narrow the page, so their links keep its filters as well.
    let subcategories = category
        .map(|category| {
            categories
                .children(category.id)
                .map(|child| Link { name: child.name.clone(), url: filters.narrowed_url(child) })
                .collect()
        })
        .unwrap_or_default();
    // Every category, each followed by its subcategories; the links keep the words, the town
    // and the map pin, as the category links do, and those under the page's category its filters.
    let category_menu = match (&page_slug, category) {
        (Some(_), Some(current)) => {
            let all = MenuEntry { name: "Të gjitha njoftimet".to_string(), url: filters.all_categories_url(), current: false };
            let mut menu = vec![MenuItem::Leaf(all)];
            let narrower: Vec<uuid::Uuid> =
                categories.subtree_ids(current.id).into_iter().filter(|id| *id != current.id).collect();
            // Open: the page's category and those above it.
            let path: Vec<uuid::Uuid> = categories.ancestors(current).iter().map(|c| c.id).chain([current.id]).collect();
            // Each category followed by its subcategories, at any depth.
            fn add(categories: &db::Categories, category: &db::Category, depth: usize, current: &db::Category,
                   narrower: &[uuid::Uuid], path: &[uuid::Uuid], filters: &Filters, menu: &mut Vec<MenuItem>) {
                let entry = MenuEntry {
                    name: category.name.clone(),
                    url: match narrower.contains(&category.id) {
                        true => filters.narrowed_url(category),
                        false => filters.category_url(category),
                    },
                    current: category.id == current.id,
                };
                let children: Vec<&db::Category> = match depth < 5 {
                    true => categories.children(category.id).collect(),
                    false => Vec::new(),
                };
                if children.is_empty() {
                    menu.push(MenuItem::Leaf(entry));
                    return;
                }
                menu.push(MenuItem::Branch(MenuBranch { entry, open: path.contains(&category.id) }));
                for child in children {
                    add(categories, child, depth + 1, current, narrower, path, filters, menu);
                }
                menu.push(MenuItem::End);
            }
            for top in categories.top_level() {
                add(&categories, top, 0, current, &narrower, &path, &filters, &mut menu);
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
