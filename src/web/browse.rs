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
    Result,
    account::current_user,
    cookie,
    filters::{Chip, Drop, DropKind, Field, FieldKind, Filters, Params, PriceChoice, TOWN_COOKIE},
    icons, is_next_page_request,
    listings::Slide,
    named, not_found, render, safety,
    seo::{self, PageMeta},
};
use crate::{
    AppState,
    db::{self, Cursor, ListingCard, ListingFilter, Sort},
    format,
    i18n::{t, t1, t2, t3},
    views,
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

/// The row of cards that slides sideways, by the word its cards' ids start with: the most seen
/// this week.
pub const MOST_VIEWED_IDS: &str = "most-viewed";
/// The row of those a promotion features (db::featured_cards).
pub const FEATURED_IDS: &str = "featured";
/// The welcome page's row of the listings its visitor saw lately (SEEN_COOKIE).
pub const SEEN_IDS: &str = "seen";

/// The cookie that keeps the listings a visitor saw lately, the latest first, their numbers
/// joined by dots: application.js puts a listing first as its page shows (not one Turbo only
/// fetched ahead, nor the seller's own), and the welcome page shows them in a row (home).
pub const SEEN_COOKIE: &str = "shikuar";
/// How many it keeps (application.js keeps to it too).
pub const SEEN_MOST: usize = 12;

/// The listings the visitor saw lately, by SEEN_COOKIE: their numbers, the latest first, each
/// once, SEEN_MOST at most; what isn't a number left out.
fn seen_numbers(headers: &HeaderMap) -> Vec<i64> {
    let mut numbers = Vec::new();
    for number in cookie(headers, SEEN_COOKIE).unwrap_or_default().split('.').filter_map(|part| part.parse::<i64>().ok()) {
        if number > 0 && !numbers.contains(&number) {
            numbers.push(number);
        }
        if numbers.len() == SEEN_MOST {
            break;
        }
    }
    numbers
}

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
            Cards::Small => t("Kartela të vogla"),
            Cards::Big => t("Kartela të mëdha"),
            Cards::List => t("Listë"),
        }
    }

    /// Its icon (icons.rs, Phosphor's).
    pub fn icon(self) -> &'static str {
        match self {
            Cards::Small => icons::VIEW_GRID,
            Cards::Big => icons::VIEW_BIG,
            Cards::List => icons::VIEW_LIST,
        }
    }

    /// How wide a card's photo shows, for the browser to pick the copy it fetches (srcset in
    /// listing_card.html), as page.css lays the cards out in the page (.container: 1100px at most,
    /// 1rem in from the screen's sides, a rem 14px on a phone): a column of the grid, two on a
    /// phone, three from 724px, four from 960px, .75rem apart on a phone and 1rem wider; for a big
    /// card the line, no wider than 40rem, and half of it, less 1.5rem between them, from 960px; in
    /// the list, the photo at the line's left, 240px wide and 116px on a phone. In pixels: a sizes
    /// attribute's rem is always 16px. Just what the cards are, so that a phone's browser doesn't
    /// take a copy bigger than its screen needs: Safari takes the first copy as sharp as the screen
    /// at least, and on an iPhone took the middle one for every small card when this said half the
    /// screen. cards_controller.js switches between them as the view menu switches the cards.
    pub fn sizes(self) -> &'static str {
        match self {
            Cards::Small => "(min-width: 1100px) 255px, (min-width: 960px) calc((100vw - 80px) / 4), (min-width: 724px) calc((100vw - 64px) / 3), (min-width: 720px) calc((100vw - 48px) / 2), calc((100vw - 38px) / 2)",
            Cards::Big => "(min-width: 1100px) 522px, (min-width: 960px) calc((100vw - 56px) / 2), (min-width: 720px) 640px, (min-width: 588px) 560px, calc(100vw - 28px)",
            Cards::List => "(min-width: 720px) 240px, 116px",
        }
    }
}

/// How wide a card shows in a row that slides sideways (.card-row in page.css), as Cards::sizes says
/// it for the grid: 236px, and 180px on a phone, as the design system has its shelves, however
/// wide the screen and however many cards.
const ROW_SIZES: &str = "(min-width: 720px) 236px, 180px";

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
    /// before they come near the screen, as they come after the listing; `next` fetches the
    /// ones after these as the last come into view (listings::related).
    pub fn related(cards: Vec<ListingCard>, next: Option<NextPage>) -> Self {
        Self { cards, next, layout: Cards::Small, eager: 0, card_ids: LISTING_IDS, lead: false }
    }

    /// A row of small cards that slides sideways (card_row.html), its cards' ids starting with
    /// `card_ids` (SEEN_IDS, FEATURED_IDS...): the first `eager` load their photo straight away.
    pub fn row(cards: Vec<ListingCard>, card_ids: &'static str, eager: usize) -> Self {
        Self { cards, next: None, layout: Cards::Small, eager, card_ids, lead: false }
    }

    /// The first two of these as big cards (card_row.html's card_pair; .big-cards in page.css),
    /// two to a line from 960px up and one over the other under it, their photos as wide as the
    /// big cards' grid has them: "Më të klikuarat", as the user asked on 2026-10-11 (a row of small
    /// cards that slid sideways before). The first `eager` load their photo straight away.
    pub fn pair(mut cards: Vec<ListingCard>, card_ids: &'static str, eager: usize) -> Self {
        cards.truncate(2);
        Self { cards, next: None, layout: Cards::Big, eager, card_ids, lead: false }
    }

    /// The page's first cards, whose first photo the browser fetches first (lead).
    fn leading(mut self) -> Self {
        self.lead = true;
        self
    }

    fn layout(mut self, layout: Cards) -> Self {
        self.layout = layout;
        self
    }

    /// How wide its cards' photos show (Cards::sizes): in a row that slides sideways (card_row.html),
    /// the row's cards' width (ROW_SIZES); in the grid, as wide as its layout has them.
    pub fn sizes(&self) -> &'static str {
        if self.in_row() { ROW_SIZES } else { self.layout.sizes() }
    }

    /// How wide one of its cards' photos needs to be: as wide as it shows (`sizes`), or, in a big
    /// card, a photo wider than tall three quarters of that (BIG_LANDSCAPE_SIZES).
    pub fn sizes_of(&self, slide: &Slide) -> &'static str {
        if self.layout == Cards::Big && slide.landscape && !self.in_row() { BIG_LANDSCAPE_SIZES } else { self.sizes() }
    }

    fn in_row(&self) -> bool {
        self.card_ids != LISTING_IDS && self.layout == Cards::Small
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
    frame(after.as_ref().map(frame_id).unwrap_or_default(), page)
}

/// The cards of `page` inside the frame `frame_id`, for any page's infinite scroll (the browse
/// pages', a listing's related ones').
pub fn frame(frame_id: String, page: &ListingPage) -> Result<Response> {
    render(ListingFrame { frame_id, page })
}

/// The main categories in the welcome page's order (MAIN_ON_WELCOME's first, then the others as
/// the database sorts them), by their slugs and names, for the menu on every page (base.html),
/// whose frame has no database to ask: the categories come with the migrations, and a test keeps
/// these theirs (the_menu_leads_everywhere, tests/web.rs).
const MENU_CATEGORIES: [(&str, &str); 11] = [
    ("automjete", "Automjete"),
    ("prona", "Prona"),
    ("elektronike", "Elektronikë"),
    ("veshje-dhe-aksesore", "Veshje / aksesorë"),
    ("shtepi-dhe-kopsht", "Shtëpi / kopsht"),
    ("pune", "Punë"),
    ("sherbime", "Shërbime"),
    ("sport-dhe-hobi", "Sport / hobi"),
    ("kafshe-shtepiake", "Kafshë shtëpiake"),
    ("makineri-dhe-biznes", "Makineri / biznes"),
    ("te-tjera", "Të tjera"),
];

/// The menu's categories (MENU_CATEGORIES), each a shortcut as the welcome page's are: its name,
/// its page and its icon.
pub fn menu_categories() -> Vec<Shortcut> {
    MENU_CATEGORIES
        .iter()
        .map(|(slug, name)| Shortcut { name: (*name).to_string(), url: format!("/c/{slug}"), icon: icons::main_category(slug) })
        .collect()
}

/// A shortcut on the welcome page.
pub struct Shortcut {
    pub name: String,
    pub url: String,
    /// The icon's SVG inside (icons::main_category).
    pub icon: &'static str,
}

/// The main categories the welcome page leads with, in this order: a line of their own after its
/// search's button, in bigger pills than the others'.
const MAIN_ON_WELCOME: [&str; 4] = ["automjete", "prona", "elektronike", "veshje-dhe-aksesore"];

#[derive(Template)]
#[template(path = "home.html")]
struct HomePage {
    /// MAIN_ON_WELCOME's, the first line, after the search's button.
    main: Vec<Shortcut>,
    /// Every other main category, in their order, after them.
    others: Vec<Shortcut>,
    page: ListingPage,
    /// The listings the visitor saw lately ("Shikuar së fundmi", SEEN_COOKIE), still on the site;
    /// none for one who saw none.
    seen: Option<ListingPage>,
    /// The listings seen the most this week ("Më të klikuarat"), none before any are.
    most_viewed: Option<ListingPage>,
    meta: PageMeta,
}

/// The welcome page asks what the visitor is looking for, then shows the listings they saw lately
/// in a row (SEEN_COOKIE) and the most seen this week in another (db::most_viewed), then all of
/// them, newest first.
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

    let seen = seen_numbers(&headers);
    let (rows, categories, mut most_viewed, mut seen) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::categories(&state.db),
        state.views.most_viewed(&state.db, None),
        async {
            match seen.is_empty() {
                true => Ok(Vec::new()),
                false => db::cards_by_numbers(&state.db, &seen).await,
            }
        },
    )?;
    safety::without_hidden(&mut most_viewed, &hidden);
    safety::without_hidden(&mut seen, &hidden);
    // Every main category with its icon: Automjete, Prona, Elektronikë and Veshje / aksesorë
    // first, on a line of their own after the search's button, then the others in their order.
    let shortcut = |category: &db::Category| Shortcut {
        name: category.name.clone(),
        url: format!("/c/{}", category.slug),
        icon: icons::main_category(&category.slug),
    };
    let main = MAIN_ON_WELCOME.iter().filter_map(|slug| categories.by_slug(slug)).map(shortcut).collect();
    let others =
        categories.top_level().filter(|category| !MAIN_ON_WELCOME.contains(&category.slug.as_str())).map(shortcut).collect();
    let page = ListingPage::new(rows, Sort::Newest, next_url);
    // The first row loads its first two photos straight away: it is on the first screen, its first
    // photo before anything else (ListingPage::lead), the listings seen lately's where there are
    // some, else "Më të klikuarat"'s; without either, the listings' first does.
    let seen = (!seen.is_empty()).then(|| ListingPage::row(seen, SEEN_IDS, 2).leading());
    let most_viewed = (!most_viewed.is_empty()).then(|| {
        let pair = ListingPage::pair(most_viewed, MOST_VIEWED_IDS, 2);
        if seen.is_none() { pair.leading() } else { pair }
    });
    let page = if seen.is_none() && most_viewed.is_none() { page.leading() } else { page };
    let origin = seo::origin(&headers);
    // The title is the shared link's alone (Open Graph), which crawlers ask for in Albanian.
    let meta = PageMeta::new(&origin, "Pluribus: njoftime në gjithë Shqipërinë", t(seo::SITE_DESCRIPTION), format!("{origin}/"));
    render(HomePage { main, others, page, seen, most_viewed, meta })
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
    /// Its icon, where it has one (category_icon).
    pub icon: Option<&'static str>,
}

#[derive(Template)]
#[template(path = "browse.html")]
struct BrowsePage<'a> {
    heading: String,
    count: String,
    /// The words searched in place of misspelt ones, quoted ("„golf“"), or empty.
    corrections: String,
    filters: &'a Filters<'a>,
    fields: Vec<Field>,
    chips: Vec<Chip>,
    /// The row's filter dropdowns, after the chips (Filters::drops).
    drops: Vec<Drop>,
    clear_url: String,
    /// On a category page, the category above it, its breadcrumb's way up (the welcome page over a
    /// main category).
    parent: Option<Subcategory>,
    subcategories: Vec<Subcategory>,
    /// On a search across every category, the categories its listings are in, the most first.
    found_in: Vec<FoundIn>,
    /// A category page, which says where it is in a breadcrumb over its heading; a search's heading
    /// is just a heading.
    category_page: bool,
    /// The cookie the view button sets on this page (cards_cookie).
    cards_cookie: String,
    /// The listings a promotion features among the page's matches ("Të veçuara"), a row over
    /// the rest; none when none is.
    featured: Option<ListingPage>,
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
    /// "Ruaj kërkimin" (searches.rs): the page's address as a saved search keeps it, its filters
    /// written as the page writes them, and the saved search it is, for a signed-in visitor who
    /// keeps it.
    save_address: String,
    saved_search: Option<Uuid>,
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
    if count == 0 { t("Asnjë njoftim").to_string() } else { t1("Shfaq {}", count_text) }
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
    /// How quickly they answer and when they were last on the site (format::seller_activity).
    activity: Option<String>,
    /// A store's page (stores.rs): its cover, logo and what it says of itself, where it is, when it
    /// is open and how to reach it; and, for its owner and admins, the way to change it.
    store: Option<db::Store>,
    edit_url: Option<String>,
}

impl ProfilePage {
    /// A store's logo, in place of the initial.
    fn logo(&self) -> Option<&str> {
        self.store.as_ref()?.logo_url.as_deref()
    }

    /// The store's description, a paragraph for each block of lines.
    fn store_paragraphs(&self) -> Vec<&str> {
        self.store
            .as_ref()
            .and_then(|store| store.description.as_deref())
            .map(|text| text.split("\n\n").map(str::trim).filter(|block| !block.is_empty()).collect())
            .unwrap_or_default()
    }

    /// The store's public number as people write it, and as a WhatsApp link where it takes it.
    fn store_phone(&self) -> Option<String> {
        self.store.as_ref()?.public_phone.as_deref().map(format::phone)
    }

    fn store_whatsapp(&self) -> Option<String> {
        let store = self.store.as_ref().filter(|store| store.whatsapp)?;
        Some(format!("https://wa.me/{}", store.public_phone.as_deref()?.trim_start_matches('+')))
    }

    /// The website without its scheme, as the page writes it ("www.dyqani.al").
    fn store_site(&self) -> Option<&str> {
        let url = self.store.as_ref()?.website_url.as_deref()?;
        Some(url.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/'))
    }
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
    profile_page(&state, &headers, query, profile, format!("/perdorues/{id}"), None).await
}

pub async fn business_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(slug): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response> {
    let (Some(profile), store) = tokio::try_join!(db::business_profile(&state.db, &slug), db::store_by_slug(&state.db, &slug))? else {
        return not_found().await;
    };
    profile_page(&state, &headers, query, profile, format!("/biznes/{slug}"), store).await
}

async fn profile_page(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<String>,
    profile: db::Profile,
    path: String,
    store: Option<db::Store>,
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
        t3("{} në Pluribus që nga {}: {}.", &profile.name, &since, &count),
        seo::canonical(&origin, &path, query.as_deref()),
    );
    if let Some((card, photo)) = page.cards.iter().find_map(|card| Some((card, card.photos_full.first()?))) {
        meta = meta.with_image(&origin, photo, card.title.clone());
    }
    meta.noindex = words.is_some();
    // A store's people who may change it have the way to, on its page.
    let edit_url = match &store {
        Some(store) => match current_user(state, headers).await? {
            Some(user) => db::my_stores(&state.db, user.id)
                .await?
                .iter()
                .any(|mine| mine.id == store.id && mine.manages())
                .then(|| format!("/dyqani/{}/ndrysho", store.slug)),
            None => None,
        },
        None => None,
    };
    if let Some(store) = &store
        && let Some(cover) = store.cover_url.as_deref().or(store.logo_url.as_deref())
    {
        meta = meta.with_image(&origin, cover, store.display_name.clone());
    }
    render(ProfilePage {
        store,
        edit_url,
        meta,
        path,
        query: words,
        found,
        initial: format::initial(&profile.name),
        since,
        count,
        business: matches!(profile.seller, db::Seller::Business(_)),
        verified: verified_label(profile.verified.as_deref()),
        activity: format::seller_activity(profile.answers, profile.seen, time::OffsetDateTime::now_utc()),
        name: profile.name,
        page,
    })
}

/// What a seller's page and listings say of the contact they proved theirs by signing in with it.
pub fn verified_label(kind: Option<&str>) -> Option<&'static str> {
    match kind? {
        "EMAIL" => Some(t("Email i verifikuar")),
        _ => Some(t("Numër i verifikuar")),
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
    let (mut rows, count, mut points, found_in, mut most_viewed, quarters, featured) = tokio::try_join!(
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
        async {
            // The listings a promotion features among the matches, the filter's own and its
            // words': a row of their own over the results, on the grid.
            match view {
                View::Grid => db::featured_cards(&state.db, &filter, db::FEATURED).await,
                _ => Ok(Vec::new()),
            }
        },
    )?;
    safety::without_hidden(&mut most_viewed, &filter.hidden_accounts);
    // "Më të klikuarat" comes before the results, on the first screen: its first two photos load
    // straight away, the first before anything else (ListingPage::lead); without it, or with it
    // folded away (MOST_VIEWED_COOKIE), where its photos wait until it is shown again, the
    // results' first does, but on the map page, whose map comes first.
    // "Të veçuara", when a promotion features any of the matches, comes first of all.
    let featured = (!featured.is_empty()).then(|| ListingPage::row(featured, FEATURED_IDS, 2).leading());
    let most_viewed_shown = cookie(&headers, MOST_VIEWED_COOKIE).as_deref() != Some("hidden");
    let most_viewed = (!most_viewed.is_empty()).then(|| {
        let pair = ListingPage::pair(most_viewed, MOST_VIEWED_IDS, if most_viewed_shown { 2 } else { 0 });
        if most_viewed_shown && featured.is_none() { pair.leading() } else { pair }
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
                icon: category_icon(category),
            })
        })
        .collect();
    let capped = count > db::COUNT_LIMIT;
    let count_number = count;
    let count = format::short_listing_count(count, db::COUNT_LIMIT);
    mark_city(&mut rows);
    let page = ListingPage::new(rows, filter.sort, next_url).layout(layout);
    let leads = featured.is_none() && (most_viewed.is_none() || !most_viewed_shown);
    let page = if leads && view != View::Map { page.leading() } else { page };
    let more_points = points.len() as i64 > db::MAP_POINTS;
    points.truncate(db::MAP_POINTS as usize);
    if view == View::Map {
        let chips = filters.chips();
        return render(MapPage { count, filters: &filters, chips, page, points, more_points });
    }

    let heading = match (&filters.q, category) {
        (Some(q), Some(category)) => t2("„{}“ në {}", q, t(&category.name)),
        (Some(q), None) => t1("Rezultatet për „{}“", q),
        (None, Some(category)) => t(&category.name).to_string(),
        (None, None) => match filters.city {
            // Where a plate code leads: all of a town's listings.
            Some(city) => t1("Njoftimet në {}", &city.name),
            None => t("Të gjitha njoftimet").to_string(),
        },
    };
    // The category above this one, the breadcrumb's way up, its links keeping the words, the town
    // and the map pin, as the category links do.
    let parent = match (&page_slug, category) {
        (Some(_), Some(current)) => current
            .parent_id
            .and_then(|id| categories.by_id(id))
            .map(|above| Subcategory { name: above.name.clone(), url: filters.category_url(above), icon: category_icon(above) }),
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
    let category_page = page_slug.is_some() && category.is_some();
    // What the page says about itself (seo.rs), its first listing's photo for the preview; a
    // search's results are not for search engines to list, a category's page is, with its filters.
    let origin = seo::origin(&headers);
    let description = match (category, &filters.q) {
        (Some(_), None) => t2("{}: {} në Pluribus, me foto, çmime dhe vendndodhje.", &heading, &count),
        _ => t2("{}: {} në Pluribus.", &heading, &count),
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
    let chips = filters.chips();
    let drops = filters.drops(&fields, chips.len());
    let has_more = fields.iter().any(|field| !field.primary);
    let type_waits = !filters.type_first() && filters.type_chosen();
    let more_open = fields.iter().any(|field| !field.primary && field.active) || (has_more && type_waits);
    let price_choices = quarters.map(|quarters| filters.price_choices(quarters)).unwrap_or_default();
    // "Ruaj kërkimin": the page as a saved search keeps it, and whether the visitor keeps it.
    let (search_path, search_query) = filters.search_address();
    let save_address = if search_query.is_empty() { search_path.clone() } else { format!("{search_path}?{search_query}") };
    let saved_search = match current_user(&state, &headers).await? {
        Some(user) => db::saved_search_id(&state.db, user.id, &search_path, &search_query).await?,
        None => None,
    };
    render(BrowsePage {
        save_address,
        saved_search,
        meta,
        show_label: show_label(count_number, &count),
        heading,
        count,
        capped,
        corrections,
        price_choices,
        has_more,
        more_open,
        fields,
        chips,
        drops,
        clear_url: filters.clear_url(),
        filters: &filters,
        parent,
        subcategories,
        found_in,
        category_page,
        cards_cookie,
        featured,
        most_viewed,
        most_viewed_shown,
        page,
        points,
    })
}
