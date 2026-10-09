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
    icons, is_next_page_request,
    listings::Slide,
    named, newest, not_found, render, safety,
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
    /// How the cards show, as the visitor chose, or as the category shows them by default
    /// (cards_layout).
    pub layout: Cards,
    /// How many cards load their photo straight away, the first row's; the rest wait until they
    /// come near the screen (listing_card.html).
    pub eager: usize,
    /// What its cards' ids start with, "listing" ("listing-1001"), or for a row of cards beside a
    /// page's results (card_row.html) the row's own word (ROW_IDS), so that a listing in both is
    /// two cards, each with an id of its own.
    pub card_ids: &'static str,
    /// Whether its first photo is the page's first, which on a phone is the biggest thing on the
    /// first screen (what Lighthouse times as its largest contentful paint): the browser fetches
    /// it before the others (fetchpriority, listing_card.html).
    pub lead: bool,
}

/// What the cards of a page's results, or of a grid, start their ids with.
pub const LISTING_IDS: &str = "listing";

/// The rows of cards that slide sideways, by the word their cards' ids start with: the welcome
/// page's newest listings (whose cards also say where they stand in the order, for its stream:
/// newest_controller.js), the most seen this week, and a seller's other listings under one of theirs.
pub const NEWEST_IDS: &str = "newest";
pub const MOST_VIEWED_IDS: &str = "most-viewed";
pub const SELLER_IDS: &str = "seller";

/// How a browse page shows its listings (the view menu, cards_controller.js): small cards, as many
/// to a line as fit; big ones, one a line, every photo to slide through; or a list, a line each,
/// the photo at its left and the price, title, place and when beside it (page.css, .cards-big and
/// .cards-list). Pages other than the browse pages show small ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cards {
    #[default]
    Small,
    Big,
    List,
}

impl Cards {
    /// Each, in the order the view menu has them.
    pub const ALL: [Cards; 3] = [Cards::Small, Cards::Big, Cards::List];

    /// The cookie's value for it, and the name of its class on the page (cards-big).
    pub fn value(self) -> &'static str {
        match self {
            Cards::Small => "small",
            Cards::Big => "big",
            Cards::List => "list",
        }
    }

    fn from_value(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|cards| cards.value() == value)
    }

    /// Its name in the view menu, which its button says too.
    pub fn label(self) -> &'static str {
        match self {
            Cards::Small => "Kartela të vogla",
            Cards::Big => "Kartela të mëdha",
            Cards::List => "Listë",
        }
    }

    /// Its icon's shapes, in a 24×24 box, stroked.
    pub fn icon(self) -> &'static str {
        match self {
            Cards::Small => r#"<rect x="3.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="3.5" width="7" height="7" rx="1.5"/><rect x="3.5" y="13.5" width="7" height="7" rx="1.5"/><rect x="13.5" y="13.5" width="7" height="7" rx="1.5"/>"#,
            Cards::Big => r#"<rect x="4" y="3.5" width="16" height="11.5" rx="2.5"/><path d="M4.5 18.5h15M4.5 21h9"/>"#,
            Cards::List => r#"<rect x="3.5" y="4" width="6" height="6" rx="1.5"/><rect x="3.5" y="14" width="6" height="6" rx="1.5"/><path d="M13 5.5h7.5M13 8.5h4.5M13 15.5h7.5M13 18.5h4.5"/>"#,
        }
    }

    /// How wide a card's photo shows, for the browser to pick the copy it fetches (srcset in
    /// listing_card.html), as page.css lays the cards out in the page (.container: 1100px at most,
    /// 1rem in from the screen's sides, a rem 14px on a phone): a column of the grid, two on a
    /// phone, three from 724px, four from 960px, .75rem apart on a phone and 1rem wider; for a big
    /// card the line, no wider than 40rem, and half of it, less 1.5rem between them, from 960px; in
    /// the list, the photo at the line's left, 8rem on a phone and 11rem wider. In pixels: a sizes
    /// attribute's rem is always 16px. Just what the cards are, so that a phone's browser doesn't
    /// take a copy bigger than its screen needs: Safari takes the first copy as sharp as the screen
    /// at least, and on an iPhone took the middle one for every small card when this said half the
    /// screen. cards_controller.js switches between them as the view menu switches the cards.
    pub fn sizes(self) -> &'static str {
        match self {
            Cards::Small => "(min-width: 1100px) 255px, (min-width: 960px) calc((100vw - 80px) / 4), (min-width: 724px) calc((100vw - 64px) / 3), (min-width: 720px) calc((100vw - 48px) / 2), calc((100vw - 38px) / 2)",
            Cards::Big => "(min-width: 1100px) 522px, (min-width: 960px) calc((100vw - 56px) / 2), (min-width: 720px) 640px, (min-width: 588px) 560px, calc(100vw - 28px)",
            Cards::List => "(min-width: 720px) 176px, 112px",
        }
    }
}

/// How wide a card shows in a row of five or more that slides sideways (.card-row in page.css), as
/// Cards::sizes says it for the grid: 2.25 cards across the page on a phone, 3.25 from 724px, 4.25
/// from 960px, the gaps between them as the grid's.
const ROW_SIZES: &str = "(min-width: 1100px) 236px, (min-width: 960px) calc((100vw - 96px) / 4.25), (min-width: 724px) calc((100vw - 80px) / 3.25), (min-width: 720px) calc((100vw - 64px) / 2.25), calc((100vw - 49px) / 2.25)";

/// How wide a big card's photo needs to be when it is wider than tall: the card's frame is 4:3
/// (page.css, .cards-big), so it cuts a wide photo at its sides and scales it by its height, three
/// quarters of the frame's width, which the srcset's widths give (the photo's shorter side,
/// photos::Measure); a tall photo it scales by its width (Cards::sizes).
const BIG_LANDSCAPE_SIZES: &str = "(min-width: 1100px) 392px, (min-width: 960px) calc((100vw - 56px) * 0.375), (min-width: 720px) 480px, (min-width: 588px) 420px, calc((100vw - 28px) * 0.75)";

/// The cookie the view menu sets (cards_controller.js): "small", "big" or "list" (Cards::value).
const CARDS_COOKIE: &str = "cards";

/// The cookie the button beside the view menu sets (most_viewed_controller.js): "hidden" while the
/// visitor keeps "Më të klikuarat" folded away on the categories' pages, none while it shows.
const MOST_VIEWED_COOKIE: &str = "most-viewed";

/// The main categories whose pages, and those of the categories under them, show big cards until
/// the visitor chooses others there: the photos say the most about a home. The choice made on
/// them is kept apart ("cards-prona"), so that one made on other pages leaves them as they are.
const BIG_CARDS_IN: [&str; 1] = ["prona"];

/// The view menu's cookie on a page of `category`, and how its cards show without one.
fn cards_cookie(categories: &db::Categories, category: Option<&db::Category>) -> (String, Cards) {
    let main = category.map(|category| categories.main_of(category));
    match main.filter(|main| BIG_CARDS_IN.contains(&main.slug.as_str())) {
        Some(main) => (format!("{CARDS_COOKIE}-{}", main.slug), Cards::Big),
        None => (CARDS_COOKIE.to_string(), Cards::Small),
    }
}

/// How the page shows its cards: as the visitor chose with `cookie`, or else as it does by
/// default.
fn cards_layout(headers: &HeaderMap, cookie_name: &str, by_default: Cards) -> Cards {
    cookie(headers, cookie_name).as_deref().and_then(Cards::from_value).unwrap_or(by_default)
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
        Self { cards: rows, next, layout: Cards::Small, eager: 4, card_ids: LISTING_IDS, lead: false }
    }

    /// The listings like one, under its page (db::related_listings): small, none of them loaded
    /// before they come near the screen, as they come after the listing.
    pub fn related(cards: Vec<ListingCard>) -> Self {
        Self { cards, next: None, layout: Cards::Small, eager: 0, card_ids: LISTING_IDS, lead: false }
    }

    /// A row of small cards that slides sideways (card_row.html), its cards' ids starting with
    /// `card_ids` (NEWEST_IDS...): the first `eager` load their photo straight away.
    pub fn row(cards: Vec<ListingCard>, card_ids: &'static str, eager: usize) -> Self {
        Self { cards, next: None, layout: Cards::Small, eager, card_ids, lead: false }
    }

    /// The page's first cards, whose first photo the browser fetches first (lead).
    fn leading(mut self) -> Self {
        self.lead = true;
        self
    }

    /// The welcome page's row of the newest listings, whose cards say where they stand in the
    /// order (listing_card.html's data-cursor), and which follows new ones (newest_controller.js).
    pub fn is_newest(&self) -> bool {
        self.card_ids == NEWEST_IDS
    }

    fn layout(mut self, layout: Cards) -> Self {
        self.layout = layout;
        self
    }

    /// How wide its cards' photos show (Cards::sizes): in a row of five cards or more (card_row.html),
    /// as the newest listings' is, which takes them from its stream one at a time (newest.rs), a
    /// quarter of the next card showing at its edge, narrower than a column of the grid
    /// (ROW_SIZES); a shorter row's are as wide as the grid's.
    pub fn sizes(&self) -> &'static str {
        if self.full_row() { ROW_SIZES } else { self.layout.sizes() }
    }

    /// How wide one of its cards' photos needs to be: as wide as it shows (`sizes`), or, in a big
    /// card, a photo wider than tall three quarters of that (BIG_LANDSCAPE_SIZES).
    pub fn sizes_of(&self, slide: &Slide) -> &'static str {
        if self.layout == Cards::Big && slide.landscape && !self.full_row() { BIG_LANDSCAPE_SIZES } else { self.sizes() }
    }

    fn full_row(&self) -> bool {
        self.card_ids == NEWEST_IDS || (self.card_ids != LISTING_IDS && self.cards.len() >= 5)
    }

    /// The page's class for how its cards show: none for small ones.
    pub fn cards_class(&self) -> String {
        match self.layout {
            Cards::Small => String::new(),
            layout => format!(" cards-{}", layout.value()),
        }
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

/// The main categories the welcome page puts last, after its search's button, in this order.
const LAST_ON_WELCOME: [&str; 4] = ["automjete", "prona", "elektronike", "veshje-dhe-aksesore"];

/// The main category the welcome page's button that posts a listing follows, among those before
/// its search's button: after them all without it.
const PLUS_AFTER_ON_WELCOME: &str = "sport-dhe-hobi";

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    /// Every main category but LAST_ON_WELCOME's, in their order, up to PLUS_AFTER_ON_WELCOME's,
    /// before the button that posts a listing.
    before_plus: Vec<Shortcut>,
    /// The others of them, after that button and before the search's.
    before_search: Vec<Shortcut>,
    /// LAST_ON_WELCOME's, after the search's button.
    last: Vec<Shortcut>,
    page: ListingPage,
    /// The row of the newest listings ("Sapo u shtuan"): the page's first newest::STRIP.
    newest: ListingPage,
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
    // Every main category, in its order, with its icon; the button that posts a listing after Sport
    // / hobi, and Automjete, Prona, Elektronikë and Veshje / aksesorë after the search.
    let shortcut = |category: &db::Category| Shortcut {
        name: category.name.clone(),
        url: format!("/c/{}", category.slug),
        icon: icons::main_category(&category.slug),
    };
    let first: Vec<&db::Category> =
        categories.top_level().filter(|category| !LAST_ON_WELCOME.contains(&category.slug.as_str())).collect();
    let plus_at = first.iter().position(|category| category.slug == PLUS_AFTER_ON_WELCOME).map_or(first.len(), |at| at + 1);
    let (before_plus, before_search) = first.split_at(plus_at);
    let before_plus = before_plus.iter().copied().map(shortcut).collect();
    let before_search = before_search.iter().copied().map(shortcut).collect();
    let last = LAST_ON_WELCOME.iter().filter_map(|slug| categories.by_slug(slug)).map(shortcut).collect();
    let page = ListingPage::new(rows, Sort::Newest, next_url);
    // Both rows load their first two photos straight away: they are on the first screen. The first
    // row's first photo comes before anything else (ListingPage::lead): "Më të klikuarat", or
    // without it the newest.
    let most_viewed = (!most_viewed.is_empty()).then(|| ListingPage::row(most_viewed, MOST_VIEWED_IDS, 2).leading());
    let newest = ListingPage::row(page.cards.iter().take(newest::STRIP).cloned().collect(), NEWEST_IDS, 2);
    let newest = if most_viewed.is_none() { newest.leading() } else { newest };
    let since = page.cards.first().map(ListingCard::newest_cursor);
    let origin = seo::origin(&headers);
    let meta = PageMeta::new(&origin, "Pluribus: njoftime në gjithë Shqipërinë", seo::SITE_DESCRIPTION, format!("{origin}/"));
    render(HomePage { before_plus, before_search, last, page, newest, since, most_viewed, meta })
}

/// A category's icon, as its pill has it (icons.rs): a main category's own, a subcategory's where
/// it has one.
pub fn category_icon(category: &db::Category) -> Option<&'static str> {
    match category.parent_id {
        None => Some(icons::main_category(&category.slug)),
        Some(_) => icons::subcategory(&category.slug),
    }
}

/// A category as a pill like the welcome page's main categories: its icon, where it has one, then
/// its name. A category page's subcategories, the category above it, and those beside it; a
/// listing's page has its category and the one above it.
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
    /// On a category page, where the listings' own heading over them says they are, after their
    /// order: the category and the kind of listing when one is chosen ("në Makina në shitje", in
    /// "Njoftimet më të reja në Makina në shitje"). Elsewhere the order alone heads them, for
    /// screen readers.
    listings_place: Option<String>,
    count: String,
    /// The words searched in place of misspelt ones, quoted ("„golf“"), or empty.
    corrections: String,
    filters: &'a Filters<'a>,
    fields: Vec<Field>,
    chips: Vec<Chip>,
    clear_url: String,
    /// On a category page, the category above it, the row's first pill: a way up.
    parent: Option<Subcategory>,
    /// On a category page three levels down, the category above its parent, which the round button
    /// before the pills leads to by its icon (the welcome page's house otherwise).
    above_parent: Option<Subcategory>,
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
    /// Whether "Më të klikuarat" shows, or the visitor folded it away (MOST_VIEWED_COOKIE); its
    /// button in the row says which.
    most_viewed_shown: bool,
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
    /// The main category the page is in, its category's own or the one above it, which the page's
    /// body names (base.html, data-section): Elektronikë's pages are in Claude's blue and Shtëpi /
    /// kopsht's in its olive where the rest of the site is coral (base.css).
    section: Option<String>,
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
    if let Some((at, quarters)) = kept_quarters().lock().unwrap().get(&key)
        && at.elapsed() < fresh
    {
        return Ok(*quarters);
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
    /// The main category the page is in (BrowsePage::section).
    section: Option<String>,
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
    let section = category.map(|category| categories.main_of(category).slug.clone());
    let path = match (&page_slug, category, view) {
        (Some(_), None, _) => return not_found().await,
        (Some(slug), Some(_), _) => format!("/c/{slug}"),
        (None, _, View::Grid) => "/search".to_string(),
        (None, _, View::Map) => "/harta".to_string(),
    };
    // Words that name a category, a kind of listing, a town or a property's zone go to the page
    // with those filters on, the rest of them searched there (named.rs). Not the next page or the
    // count, which a page asks for that did so already, nor the map, whose pin says where.
    if view == View::Grid && !is_next_page_request(&headers) && params.first("numri").is_none() {
        let town = params.first("city").and_then(|slug| cities.iter().find(|city| city.slug == slug));
        if let Some(named) = words.as_deref().and_then(|words| named::read(words, category, town, &categories, &cities)) {
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
    // {"count": 3412, "text": "3k njoftime", "show": "Shfaq 3k njoftime"}.
    if params.first("numri") == Some("1") {
        let count = db::listing_count_up_to(&state.db, &filter, db::PRECISE_COUNT_LIMIT).await?;
        let text = format::short_listing_count(count, db::PRECISE_COUNT_LIMIT);
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

    // Big cards and the list only in the grid: the map page's list stays small, beside the map.
    let (cards_cookie, default_layout) = cards_cookie(&categories, category);
    let layout = if view == View::Grid { cards_layout(&headers, &cards_cookie, default_layout) } else { Cards::Small };

    if is_next_page_request(&headers) {
        let mut rows = db::listing_page(&state.db, &filter).await?;
        mark_city(&mut rows);
        return frame_response(filter.after, &ListingPage::new(rows, filter.sort, next_url).layout(layout));
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
    // "Më të klikuarat" comes before the results, on the first screen: its first two photos load
    // straight away, the first before anything else (ListingPage::lead); without it, or with it
    // folded away (MOST_VIEWED_COOKIE), where its photos wait until it is shown again, the
    // results' first does, but on the map page, whose map comes first.
    let most_viewed_shown = cookie(&headers, MOST_VIEWED_COOKIE).as_deref() != Some("hidden");
    let most_viewed = (!most_viewed.is_empty()).then(|| {
        let row = ListingPage::row(most_viewed, MOST_VIEWED_IDS, if most_viewed_shown { 2 } else { 0 });
        if most_viewed_shown { row.leading() } else { row }
    });
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
    let count = format::short_listing_count(count, db::COUNT_LIMIT);
    mark_city(&mut rows);
    let page = ListingPage::new(rows, filter.sort, next_url).layout(layout);
    let leads = most_viewed.is_none() || !most_viewed_shown;
    let page = if leads && view != View::Map { page.leading() } else { page };
    let more_points = points.len() as i64 > db::MAP_POINTS;
    points.truncate(db::MAP_POINTS as usize);
    if view == View::Map {
        let chips = filters.chips();
        return render(MapPage { count, filters: &filters, chips, page, points, more_points, section });
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
    // The category above this one, the row's first pill: a way up; the one above that, which the
    // round button before the pills leads to; and this one's own icon.
    let way_up = |of: &db::Category| {
        of.parent_id.and_then(|id| categories.by_id(id)).map(|above| {
            (above, Subcategory { name: above.name.clone(), url: filters.category_url(above), icon: category_icon(above) })
        })
    };
    let (parent, above_parent) = match (&page_slug, category.and_then(way_up)) {
        (Some(_), Some((above, link))) => (Some(link), way_up(above).map(|(_, link)| link)),
        _ => (None, None),
    };
    let heading_icon = match (&page_slug, category) {
        (Some(_), Some(current)) => category_icon(current),
        _ => None,
    };
    // Where the listings' own heading on a category page says they are, after their order (the
    // template's, which opens the orders): the category and, when one is chosen, the kind of
    // listing, in small letters ("në Makina në shitje", "në Prona me qira").
    let listings_place = match (&page_slug, category) {
        (Some(_), Some(current)) => {
            let mut place = format!("në {}", current.name);
            if let Some(kind) = &filters.listing_type {
                place.push(' ');
                place.push_str(&format::listing_type(kind).to_lowercase());
            }
            Some(place)
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
        listings_place,
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
        above_parent,
        heading_icon,
        subcategories,
        found_in,
        level,
        cards_cookie,
        most_viewed,
        most_viewed_shown,
        page,
        points,
        section,
    })
}
