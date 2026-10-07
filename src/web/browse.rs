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
    filters::{Chip, Field, FieldKind, Filters, Params},
    is_next_page_request, newest, not_found, render,
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
/// half the page's 1100px on a wide screen (app.css, .cards-big). cards_controller.js switches
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
    /// How many of the page's cards, the newest, the row of the newest listings shows.
    strip: usize,
    /// Where the row's stream of new listings starts: after the newest (none without listings).
    since: Option<String>,
    /// The listings seen the most this week ("Më të klikuarat"), none before any are.
    most_viewed: Option<ListingPage>,
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
    let filter = ListingFilter { after, ..Default::default() };
    let next_url = |cursor: &str| format!("/?after={cursor}");
    if is_next_page_request(&headers) {
        let rows = db::listing_page(&state.db, &filter).await?;
        return frame_response(after, &ListingPage::new(rows, Sort::Newest, next_url));
    }

    let (rows, categories, most_viewed) = tokio::try_join!(
        db::listing_page(&state.db, &filter),
        db::categories(&state.db),
        state.views.most_viewed(&state.db, None),
    )?;
    // Every main category, in its order, with its icon.
    let shortcuts = categories
        .top_level()
        .map(|category| Shortcut {
            name: category.name.clone(),
            url: format!("/c/{}", category.slug),
            icon: SHORTCUTS.iter().find(|(slug, _)| *slug == category.slug).map_or(BOX, |(_, icon)| *icon),
        })
        .collect();
    let page = ListingPage::new(rows, Sort::Newest, next_url);
    let strip = page.cards.len().min(newest::STRIP);
    let since = page.cards.first().map(ListingCard::newest_cursor);
    let most_viewed = (!most_viewed.is_empty()).then(|| ListingPage::related(most_viewed));
    render(HomePage { shortcuts, page, strip, since, most_viewed })
}

pub struct Link {
    pub name: String,
    pub url: String,
}

/// A subcategory under a category page's heading, a pill like the welcome page's main categories:
/// its icon, where it has one (SUBCATEGORY_ICONS), then its name.
pub struct Subcategory {
    pub name: String,
    pub url: String,
    pub icon: Option<&'static str>,
}

/// Subcategories' icons, drawn as the main categories' are (SHORTCUTS): in strokes, in a 24-unit
/// box. A subcategory without one shows its name alone.
const SUBCATEGORY_ICONS: [(&str, &str); 66] = [
    // A car from the front, where Automjete has one from the side.
    ("makina", r#"<path d="M5 11l1.5-4a2 2 0 0 1 1.9-1.3h7.2a2 2 0 0 1 1.9 1.3L19 11"/><rect x="3" y="11" width="18" height="6.5" rx="2"/><path d="M6.5 17.5V20M17.5 17.5V20"/><circle cx="7.5" cy="14.2" r=".9"/><circle cx="16.5" cy="14.2" r=".9"/>"#),
    // A motorbike: its wheels, seat and tank, the engine under them, the fork and handlebar.
    ("motocikleta", r#"<circle cx="5.5" cy="16.5" r="3"/><circle cx="18.5" cy="16.5" r="3"/><path d="M3.5 11.5h5.5l1.8-2h3.2"/><path d="M8 11.5l2.5 5h3l2.5-4.5"/><path d="M15.5 9.5 18.5 16.5"/><path d="M14.5 9.5l-1-2.5h2.5"/>"#),
    // A delivery truck.
    ("automjete-tregtare", r#"<path d="M14 16.5V6.5a1 1 0 0 0-1-1H3.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h1.3"/><path d="M14 9h3.8l3.7 4v2.5a1 1 0 0 1-1 1h-1.3"/><path d="M9.2 16.5h5.6"/><circle cx="7" cy="16.8" r="2.2"/><circle cx="17" cy="16.8" r="2.2"/>"#),
    // A wrench.
    ("pjese-dhe-aksesore", r#"<path d="M15.2 4.3a4.6 4.6 0 0 0-5.4 6l-6.2 6.2a2 2 0 1 0 2.9 2.9l6.2-6.2a4.6 4.6 0 0 0 6-5.4l-2.9 2.9-2.6-.4-.4-2.6z"/>"#),
    // Its kinds. A nut.
    ("pjese-kembimi", r#"<path d="M12 3 19.8 7.5v9L12 21l-7.8-4.5v-9z"/><circle cx="12" cy="12" r="3.5"/>"#),
    // A tyre on its rim, five spokes.
    ("goma-dhe-disqe", r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="5"/><path d="M12 10.2V7M13.7 11.4l3.1-.9M13.1 13.5l1.8 2.5M10.9 13.5 9.1 16M10.3 11.4l-3.1-.9"/>"#),
    // A loudspeaker.
    ("audio-dhe-video", r#"<rect x="5.5" y="2.5" width="13" height="19" rx="2.5"/><circle cx="12" cy="14.5" r="3.5"/><circle cx="12" cy="7" r="1.2"/>"#),
    // A navigation arrow.
    ("gps-dhe-navigatore", r#"<path d="M12 3.5 19 20l-7-3.8L5 20z"/>"#),
    // A racing flag.
    ("styling-dhe-pajisje-sportive", r#"<path d="M5 21V3.5"/><path d="M5 4h12.5l-2.5 4.5 2.5 4.5H5"/>"#),
    // A mat with its grooves.
    ("tapete-makine", r#"<rect x="5.5" y="3" width="13" height="18" rx="3"/><path d="M9 7v10M12 7v10M15 7v10"/>"#),
    // A trailer: its box, wheel and drawbar.
    ("rimorkio", r#"<path d="M2.5 15.5v-7a1 1 0 0 1 1-1h11a1 1 0 0 1 1 1v7z"/><path d="M15.5 13.5h6"/><circle cx="9" cy="17.5" r="2"/>"#),
    // A lightning bolt.
    ("mbushes-per-makina-elektrike", r#"<path d="M13 2.5 5.5 13h6l-1 8.5L18.5 11h-6z"/>"#),
    // A box, as for the main categories without an icon of their own.
    ("pjese-te-tjera", BOX),
    // Prona's.
    // Two blocks of flats.
    ("apartamente", r#"<path d="M3.5 21V8.5l6-3V21"/><path d="M9.5 21V3.5h11V21"/><path d="M2.5 21h19"/><path d="M13 7.5h1M17 7.5h1M13 11.5h1M17 11.5h1M13 15.5h1M17 15.5h1M6 12h1M6 16h1"/>"#),
    // A bed.
    ("garsoniera", r#"<path d="M2.5 19V6"/><path d="M21.5 19v-5H2.5"/><path d="M2.5 14v-3.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1V14"/><path d="M8.5 11h11a2 2 0 0 1 2 2"/>"#),
    // A house with a chimney and a door.
    ("vila-dhe-shtepi", r#"<path d="M3 11 12 4l9 7"/><path d="M5 9.5V20h14V9.5"/><path d="M15.5 6.5V4h2v4"/><rect x="9.5" y="14" width="5" height="6"/>"#),
    // A shop front under its awning.
    ("dyqane", r#"<path d="M3.5 9.5 5 4h14l1.5 5.5"/><path d="M3.5 9.5a2.8 2.8 0 0 0 5.7 0 2.8 2.8 0 0 0 5.6 0 2.8 2.8 0 0 0 5.7 0"/><path d="M5 12v8.5h14V12"/><path d="M10 20.5v-5h4v5"/>"#),
    // An office tower.
    ("zyra", r#"<rect x="5" y="2.5" width="14" height="19" rx="1"/><path d="M9 6.5h2M13 6.5h2M9 10.5h2M13 10.5h2M9 14.5h2M13 14.5h2"/><path d="M10.5 21.5v-3h3v3"/>"#),
    // A plot of land, a flag in it.
    ("truall", r#"<path d="M2.5 17.5 7 11h10l4.5 6.5z"/><path d="M12 11V4.5"/><path d="M12 4.5h4.5l-1.2 1.75 1.2 1.75H12"/>"#),
    // A garage and its door.
    ("garazhe", r#"<path d="M3 20.5V9l9-5.5L21 9v11.5"/><path d="M7 20.5v-8h10v8"/><path d="M7 15h10M7 17.8h10"/>"#),
    // Elektronikë's.
    // A phone, its camera at the top.
    ("telefona", r#"<rect x="6.5" y="2.5" width="11" height="19" rx="2.5"/><path d="M10.5 18h3"/><circle cx="12" cy="5.5" r=".6"/>"#),
    // A laptop.
    ("laptope", r#"<rect x="4.5" y="4.5" width="15" height="10.5" rx="1.5"/><path d="M2.5 19.5h19"/><path d="M4.5 15 2.5 19.5M19.5 15l2 4.5"/>"#),
    // A screen on its stand.
    ("kompjutere", r#"<rect x="3" y="4" width="18" height="12" rx="1.5"/><path d="M9 20h6M12 16v4"/>"#),
    // A tablet.
    ("tableta", r#"<rect x="4.5" y="2.5" width="15" height="19" rx="2.5"/><path d="M11 18h2"/>"#),
    // A television and its aerial.
    ("televizore", r#"<rect x="2.5" y="7" width="19" height="13" rx="2"/><path d="m8 3 4 4 4-4"/>"#),
    // A game controller.
    ("konsola-lojerash", r#"<path d="M6.5 7.5h11a4 4 0 0 1 4 4.4l-.6 4.8a2.5 2.5 0 0 1-4.3 1.4L14 15.5h-4l-2.6 2.6a2.5 2.5 0 0 1-4.3-1.4l-.6-4.8a4 4 0 0 1 4-4.4z"/><path d="M7.5 10v4M5.5 12h4"/><circle cx="16" cy="11" r=".6"/><circle cx="17.5" cy="13" r=".6"/>"#),
    // A camera.
    ("aparate-fotografike", r#"<path d="M3 8.5a2 2 0 0 1 2-2h2.5L9 4.5h6l1.5 2H19a2 2 0 0 1 2 2V18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><circle cx="12" cy="13" r="3.5"/>"#),
    // Headphones.
    ("audio-dhe-kufje", r#"<path d="M4 14v-2a8 8 0 0 1 16 0v2"/><rect x="2.5" y="13.5" width="4.5" height="7" rx="1.5"/><rect x="17" y="13.5" width="4.5" height="7" rx="1.5"/>"#),
    // Shtëpi dhe kopsht's.
    // A chair.
    ("mobilje", r#"<path d="M7 11V5a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v6"/><path d="M5.5 11h13v4h-13z"/><path d="M7 15v6M17 15v6"/>"#),
    // A washing machine.
    ("elektroshtepiake", r#"<rect x="4" y="2.5" width="16" height="19" rx="2"/><circle cx="12" cy="13.5" r="4.5"/><path d="M7.5 6h2"/><circle cx="16" cy="6" r=".6"/>"#),
    // A sprout, two leaves.
    ("kopsht", r#"<path d="M12 21v-9"/><path d="M12 12c0-3.5-2.5-6-6.5-6 0 3.5 2.5 6 6.5 6z"/><path d="M12 14c0-3.5 2.5-6 6.5-6 0 3.5-2.5 6-6.5 6z"/><path d="M7.5 21h9"/>"#),
    // A table lamp.
    ("dekor", r#"<path d="M8.5 3.5h7l2.5 7H6z"/><path d="M12 10.5v8.5"/><path d="M8.5 20.5h7"/>"#),
    // Punë's.
    // A fork and a knife.
    ("pune-turizem", r#"<path d="M5.5 3v6a2.5 2.5 0 0 0 5 0V3M8 3v18"/><path d="M17.5 21V3c-2 1-3.5 4-3.5 8h3.5"/>"#),
    // A hard hat.
    ("pune-ndertim", r#"<path d="M2.5 18h19"/><path d="M5 18v-2.5a7 7 0 0 1 14 0V18"/><path d="M9.5 9.3V6.5h5v2.8"/><path d="M12 6.5V13"/>"#),
    // A medical cross.
    ("pune-shendetesi", r#"<path d="M9.5 3.5h5v6h6v5h-6v6h-5v-6h-6v-5h6z"/>"#),
    // Code brackets.
    ("pune-it", r#"<path d="m8 7-5 5 5 5M16 7l5 5-5 5M14 4.5l-4 15"/>"#),
    // A price tag.
    ("pune-shitje", r#"<path d="M3.5 12.5V4.5a1 1 0 0 1 1-1h8l8.5 8.5a1.5 1.5 0 0 1 0 2.1l-6.9 6.9a1.5 1.5 0 0 1-2.1 0z"/><circle cx="8" cy="8" r="1.4"/>"#),
    // A steering wheel.
    ("pune-transport", r#"<circle cx="12" cy="12" r="8.5"/><circle cx="12" cy="12" r="2"/><path d="M3.8 10.5c3.5-1 12.9-1 16.4 0M10.2 13.5 7 19.5M13.8 13.5l3.2 6"/>"#),
    // A headset and its microphone.
    ("pune-call-center", r#"<path d="M4 13v-1a8 8 0 0 1 16 0v1"/><rect x="2.5" y="12.5" width="4" height="6" rx="1.5"/><rect x="17.5" y="12.5" width="4" height="6" rx="1.5"/><path d="M19.5 18.5a3 3 0 0 1-3 3H13"/>"#),
    // Shërbime's.
    // A screwdriver.
    ("riparime", r#"<rect x="13.5" y="2.6" width="4.5" height="8.5" rx="1.5" transform="rotate(45 15.75 6.85)"/><path d="m12.6 11.4-8 8"/>"#),
    // A bus from the front.
    ("transport-dhe-transferime", r#"<rect x="4" y="3.5" width="16" height="14" rx="2.5"/><path d="M4 11h16"/><path d="M7 17.5V20M17 17.5V20"/><circle cx="8" cy="14.3" r=".7"/><circle cx="16" cy="14.3" r=".7"/>"#),
    // A broom.
    ("pastrim", r#"<path d="M14 3.5 10.5 12"/><path d="M6.5 12h8l2 8.5h-12z"/><path d="M9 16.5l-.5 4M12 16.5v4M15 16.5l.5 4"/>"#),
    // An open book.
    ("mesime", r#"<path d="M12 6.5c-2-1.5-5-2-8.5-1.5V19c3.5-.5 6.5 0 8.5 1.5 2-1.5 5-2 8.5-1.5V5c-3.5-.5-6.5 0-8.5 1.5z"/><path d="M12 6.5v14"/>"#),
    // A video camera.
    ("foto-dhe-video", r#"<rect x="2.5" y="6.5" width="13" height="11" rx="2"/><path d="m15.5 10.5 6-3.5v10l-6-3.5"/>"#),
    // A paint roller.
    ("ndertim-dhe-lyerje", r#"<rect x="3.5" y="3.5" width="14" height="5" rx="1.5"/><path d="M17.5 6h2a1 1 0 0 1 1 1v3a1 1 0 0 1-1 1h-7.5v3"/><rect x="10.5" y="14" width="3" height="7" rx="1"/>"#),
    // Veshje dhe aksesorë's.
    // A jacket, open at the collar.
    ("xhaketa-dhe-pallto", r#"<path d="M8.5 3.5 4 6l-.5 14.5H9V12M15.5 3.5 20 6l.5 14.5H15V12"/><path d="M8.5 3.5 12 8l3.5-4.5"/><path d="M12 8v12.5"/>"#),
    // A trainer.
    ("kepuce", r#"<path d="M3 18.5h18"/><path d="M3 18.5v-8a1 1 0 0 1 1-1h2.5a1 1 0 0 1 1 .8c.4 1.5 1.6 2.2 3 2.2h1l6.5 2a3 3 0 0 1 2 2.8v1.2"/><path d="m11.5 12.5-1.5-2.5M14.5 13.4l-1.3-2.2"/>"#),
    // A dress.
    ("fustane", r#"<path d="M9 3h6l-1 4.5 4.5 13.5h-13L10 7.5z"/><path d="M10 7.5h4"/>"#),
    // A handbag.
    ("canta", r#"<path d="M5 8.5h14l-1.2 11.5a1 1 0 0 1-1 .9H7.2a1 1 0 0 1-1-.9z"/><path d="M8.5 8.5V7a3.5 3.5 0 0 1 7 0v1.5"/>"#),
    // A wristwatch.
    ("ora", r#"<circle cx="12" cy="12" r="5.5"/><path d="M12 9.5V12l1.5 1.5"/><path d="M9 7l.7-4h4.6l.7 4M9 17l.7 4h4.6l.7-4"/>"#),
    // Sunglasses.
    ("aksesore", r#"<circle cx="6.5" cy="14" r="3.5"/><circle cx="17.5" cy="14" r="3.5"/><path d="M10 14c1.3-1 2.7-1 4 0"/><path d="M3 14 4.5 8M21 14l-1.5-6"/>"#),
    // Sport dhe hobi's.
    // A bicycle.
    ("bicikleta", r#"<circle cx="5.5" cy="16" r="3.5"/><circle cx="18.5" cy="16" r="3.5"/><path d="M5.5 16 9 10.5h5.5l4 5.5"/><path d="M14.5 10.5 12.8 6.5h2.7"/><path d="M9 10.5 7.8 8.5H5"/>"#),
    // A dumbbell.
    ("fitnes", r#"<path d="M6.5 6.5v11M17.5 6.5v11M3.5 9v6M20.5 9v6M6.5 12h11"/>"#),
    // Two notes.
    ("instrumente-muzikore", r#"<path d="M9 18V5.5l11-2V16"/><circle cx="6.5" cy="18" r="2.5"/><circle cx="17.5" cy="16" r="2.5"/>"#),
    // A tent.
    ("kamping-dhe-peshkim", r#"<path d="M2.5 20h19"/><path d="M4 20 12 5l8 15"/><path d="M9 20l3-6 3 6"/>"#),
    // A snowflake.
    ("sporte-dimerore", r#"<path d="M12 2.5v19M3.8 7.25l16.4 9.5M3.8 16.75l16.4-9.5"/><path d="m9.5 4 2.5 2 2.5-2M9.5 20l2.5-2 2.5 2"/>"#),
    // A box, as for other things.
    ("sport-te-tjera", BOX),
    // Kafshë shtëpiake's.
    // A dog's head, its ears down.
    ("qen", r#"<path d="M8 4.5c-2.5 0-4.5 2-4.5 5 0 1.5 1 2 2 1.5L7 10v5.5a5 5 0 0 0 10 0V10l1.5 1c1 .5 2 0 2-1.5 0-3-2-5-4.5-5z"/><circle cx="10" cy="12" r=".6"/><circle cx="14" cy="12" r=".6"/><path d="M11 16h2l-1 1z"/>"#),
    // A cat's head, its ears up.
    ("mace", r#"<path d="M5 4l3.5 4h7L19 4v9a7 7 0 0 1-14 0z"/><circle cx="9.5" cy="12.5" r=".6"/><circle cx="14.5" cy="12.5" r=".6"/><path d="M11 15.5h2l-1 1z"/>"#),
    // A bird.
    ("zogj", r#"<path d="M3.5 13.5c3 0 5-1.5 6-4a5 5 0 0 1 9.5 2l2.5-1-1.5 3a7 7 0 0 1-7 6.5H8z"/><circle cx="15" cy="10" r=".6"/>"#),
    // A fish.
    ("peshq", r#"<path d="M2.5 12c3-4.5 9-6 14-2l3.5-3v10l-3.5-3c-5 4-11 2.5-14-2z"/><circle cx="7.5" cy="11.3" r=".6"/>"#),
    // A bone.
    ("aksesore-per-kafshe", r#"<path d="M17.4 4.6a2.4 2.4 0 0 0-4.2 2.1L6.7 13.2a2.4 2.4 0 1 0-2.1 4.2 2.4 2.4 0 1 0 4.2 2.1l6.5-6.5a2.4 2.4 0 1 0 2.1-4.2 2.4 2.4 0 1 0 0-4.2z"/>"#),
    // Makineri dhe biznes's.
    // An excavator, its arm and bucket.
    ("makineri-ndertimi", r#"<path d="M2.5 19.5h11"/><rect x="2.5" y="12" width="9" height="5" rx="1"/><path d="M5 12V9h4l1 3"/><path d="M11.5 13.5l5-6 4 3.5-1 2.5"/>"#),
    // A tractor.
    ("makineri-bujqesore", r#"<circle cx="7" cy="16" r="4"/><circle cx="18" cy="17.5" r="2.5"/><path d="M11 16h4.5M3.5 13V7.5h6l1.5 5.5"/><path d="M15.5 13V9.5h3.5l1.5 5.5"/>"#),
    // A solar panel on its stand.
    ("energji-dhe-gjeneratore", r#"<path d="M4.5 4.5h15l2 10h-19z"/><path d="M3.5 9.5h17M9.5 4.5l-1 10M14.5 4.5l1 10"/><path d="M12 14.5v5M8 19.5h8"/>"#),
    // A cog.
    ("makineri-industriale", r#"<path d="M18.4 10.6L20.9 10.7L20.9 13.3L18.4 13.4L17.5 15.5L19.2 17.4L17.4 19.2L15.5 17.5L13.4 18.4L13.3 20.9L10.7 20.9L10.6 18.4L8.5 17.5L6.6 19.2L4.8 17.4L6.5 15.5L5.6 13.4L3.1 13.3L3.1 10.7L5.6 10.6L6.5 8.5L4.8 6.6L6.6 4.8L8.5 6.5L10.6 5.6L10.7 3.1L13.3 3.1L13.4 5.6L15.5 6.5L17.4 4.8L19.2 6.6L17.5 8.5z"/><circle cx="12" cy="12" r="2.5"/>"#),
];

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
    count: String,
    /// The words searched in place of misspelt ones, quoted ("„golf“"), or empty.
    corrections: String,
    filters: &'a Filters<'a>,
    fields: Vec<Field>,
    chips: Vec<Chip>,
    clear_url: String,
    breadcrumbs: Vec<Link>,
    subcategories: Vec<Subcategory>,
    /// On a search across every category, the categories its listings are in, the most first.
    found_in: Vec<FoundIn>,
    /// On a category page, the menu the heading opens to go to another category.
    category_menu: Vec<MenuItem>,
    /// The cookie the view button sets on this page (cards_cookie).
    cards_cookie: String,
    /// In a category whose page shows its own most seen (views::has_own_most_viewed), with no
    /// filter on, its listings seen the most this week ("Më të klikuarat"), none before any are.
    most_viewed: Option<ListingPage>,
    page: ListingPage,
    /// With the map's dots asked for (harta=1), a dot for each result, up to db::MAP_POINTS.
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
    let words = params.search_words();
    let (categories, cities, attributes, search) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        async {
            match category_slug {
                Some(slug) => db::filter_attributes(&state.db, slug).await,
                None => Ok(Vec::new()),
            }
        },
        db::text_search(&state.db, words.as_deref()),
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
    // The words searched in place of misspelt ones, which the heading's line names.
    let corrections = search.as_ref().map(|search| format::quoted_list(&search.corrections)).unwrap_or_default();
    let filter = filters.listing_filter(search);
    let next_url = |cursor: &str| filters.next_url(cursor);

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
    let (mut rows, count, mut points, found_in, most_viewed) = tokio::try_join!(
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
    )?;
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
                .map(|child| Subcategory {
                    name: child.name.clone(),
                    url: filters.narrowed_url(child),
                    icon: SUBCATEGORY_ICONS.iter().find(|(slug, _)| *slug == child.slug).map(|(_, icon)| *icon),
                })
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
        corrections,
        fields: filters.fields(),
        chips: filters.chips(),
        clear_url: filters.clear_url(),
        filters: &filters,
        breadcrumbs,
        subcategories,
        found_in,
        category_menu,
        cards_cookie,
        most_viewed,
        page,
        points,
        more_points,
    })
}
