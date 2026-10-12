//! A saved search's page, made again from what the search keeps of it (`saved_searches.filters`:
//! the page's path and its filters' query string), as the pages make that page: what the pages
//! (pluribus-web's searches.rs: the counts on /kerkimet-e-ruajtura), the alerts and the JSON API
//! (pluribus-api's searches.rs: its saved searches, their counts and their listings) share, moved
//! out of pluribus-web's searches.rs in C7. `replay` reads the page at a path (a search across every
//! category, /search, or a category's page, /c/<slug>) with a query's filters, through the browse
//! parser (filters.rs), against the categories, the towns and the category's properties that exist,
//! with the search for its words (db::text_search), into a `Replay`: the listing query its page runs
//! (`Replay::listing_filter`), how many listings came to it since a moment (`Replay::new_since`),
//! and what a search keeps of it, its address and its name (`Replay::address`, `Replay::name`).
//! Those two are written here alone, from a page's filters (`address`, `name`), for the pages and
//! the API alike, so that a search kept from the app is the one its page keeps, and is named alike:
//! the pages' "Ruaj kërkimin" keeps a search by its `Replay` as the API does, and a page's save
//! button looks its search up by `address` (pluribus-web's `Filters::search_address`); until C12
//! the pages wrote and named theirs by their filters' own, which these copied word for word. A
//! name says what its page's chips say (pluribus-web's filters.rs words those): the API's tests
//! hold it to them, and the pages' save and the API's to each other (crates/api/tests/searches.rs).
//!
//! And the saved searches' alerts (`send_search_alerts`), pluribus-web's searches.rs's until D2,
//! which moved them here, beside `replay`, which they find a search's new listings by, for the jobs
//! to send, one server at a time (jobs.rs runs `tick_alerts` every ALERT_CHECK_EVERY): a search
//! with listings new since its last alert hears of them, at most once in ALERT_GAP, as a
//! notification (SAVED_SEARCH), which puts the dot on the heart that leads to the listings kept,
//! and by e-mail where its saver has an address, naming ALERT_LISTINGS of them, with the link that
//! stops that search's alerts alone, or by WhatsApp with its template (WHATSAPP_SEARCH_TEMPLATE);
//! the e-mail in its recipient's language, the notification's title and the template's texts in
//! Albanian.

use std::time::Duration;

use pluribus_core::{
    domain::notices::{NEW_MOST, new_listings},
    format::{self, Contact},
    i18n::{self, Lang, t, t1, t2},
};
use pluribus_db::{self as db, Categories, Category, City, FilterAttribute, ListingFilter, Sort, TextSearch};
use rust_decimal::Decimal;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    filters::{Params, ParsedFilters, plain},
    sender::{Alert, AlertTemplate, Senders},
};

/// A search across every category: one of the two pages a saved search can be of.
pub const SEARCH_PAGE: &str = "/search";

/// Where a category's page is, the other: /c/<slug>.
pub const CATEGORY_PAGES: &str = "/c/";

/// The most characters a search's name has (`saved_searches.name` holds 120): a longer one is cut
/// there, its last "…".
const NAME_CHARS: usize = 120;

/// What the filter of a part's vehicle's year is called, as its chip says it (pluribus-web's
/// filters.rs, FITS_YEAR_LABEL).
const FITS_YEAR_LABEL: &str = "Viti i automjetit";

/// At most one alert per search in this long, however many listings come.
pub const ALERT_GAP: Duration = Duration::from_secs(3 * 60 * 60);
/// How often the server looks for alerts due.
pub const ALERT_CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
/// How many of the new listings an alert's e-mail names.
const ALERT_LISTINGS: usize = 3;

/// Whether a saved search can be of the page at `path`: a search, or a category's page.
pub fn is_search_page(path: &str) -> bool {
    path == SEARCH_PAGE || path.starts_with(CATEGORY_PAGES)
}

/// A saved search's page made again (`replay`): its path, its filters as its query gives them, its
/// category, and the category's properties and the words' search they are read with; what the
/// pages and the API make of it.
pub struct Replay<'a> {
    /// The page: /search, or a category's (/c/makina).
    pub path: String,
    /// Its filters, as its query string gives them.
    pub params: Params,
    /// Its category: a category page's, or on /search the one its `category` names; none for a
    /// search across every category.
    pub category: Option<&'a Category>,
    /// Whether the category is one of the filters (/search), rather than the page (/c/<slug>).
    pub category_is_filter: bool,
    /// The category's filterable properties, which its filters are read against.
    pub attributes: Vec<FilterAttribute>,
    /// The search for its words (`q`), as db::text_search made it.
    pub search: Option<TextSearch>,
    categories: &'a Categories,
    cities: &'a [City],
}

/// The page at `path` (/search, /c/<slug>) with the filters of `query`, as the pages make it but
/// for the town a visitor's cookie remembers, which a saved search has written in where it had one;
/// None for a path that is neither, or a category that doesn't exist (the page's, or the one
/// /search's `category` names).
pub async fn replay<'a>(
    db: &PgPool,
    categories: &'a Categories,
    cities: &'a [City],
    path: &str,
    query: &str,
) -> sqlx::Result<Option<Replay<'a>>> {
    let page_slug = match path {
        SEARCH_PAGE => None,
        _ => match path.strip_prefix(CATEGORY_PAGES) {
            Some(slug) => Some(slug),
            None => return Ok(None),
        },
    };
    let params = Params::parse(Some(query));
    let slug = page_slug.or(params.first("category"));
    let category = match slug {
        Some(slug) => match categories.by_slug(slug) {
            Some(category) => Some(category),
            None => return Ok(None),
        },
        None => None,
    };
    let words = params.search_words();
    let (attributes, search) = tokio::try_join!(
        async {
            match category {
                Some(category) => db::filter_attributes(db, &category.slug).await,
                None => Ok(Vec::new()),
            }
        },
        db::text_search(db, words.as_deref()),
    )?;
    Ok(Some(Replay {
        path: path.to_string(),
        params,
        category,
        category_is_filter: page_slug.is_none(),
        attributes,
        search,
        categories,
        cities,
    }))
}

impl Replay<'_> {
    /// Its filters, as the parser reads them (filters.rs), with no pin but one in its query.
    pub fn filters(&self) -> ParsedFilters<'_> {
        ParsedFilters::new(&self.params, self.category, self.categories, self.cities, &self.attributes, None)
    }

    /// The listing query its page runs, newest first, or the best match first with words, or the
    /// nearest first with a pin in its query, as its page opens (`ParsedFilters::listing_filter`).
    pub fn listing_filter(&self) -> ListingFilter {
        self.filters().listing_filter(self.search.clone())
    }

    /// Its category's id, which a saved search keeps beside its address.
    pub fn category_id(&self) -> Option<Uuid> {
        self.category.map(|category| category.id)
    }

    /// Its search words (`q`), which a saved search keeps beside its address.
    pub fn words(&self) -> Option<String> {
        self.params.search_words()
    }

    /// How many listings came to it since `since` (put up, or brought up again, after it), counted
    /// up to NEW_MOST and one past it, which says "99+" (domain::notices::new_listings): a saved
    /// search's new listings since it was last opened, as /kerkimet-e-ruajtura counts them.
    pub async fn new_since(&self, db: &PgPool, since: OffsetDateTime) -> sqlx::Result<i64> {
        let mut filter = self.listing_filter();
        filter.since = Some(since);
        db::listing_count_up_to(db, &filter, NEW_MOST).await
    }

    /// What a saved search keeps of its page (`address`): its path, and its filters as a query
    /// string.
    pub fn address(&self) -> (String, String) {
        address(&self.path, &self.filters(), self.category_is_filter)
    }

    /// The name a saved search of its page shows by (`name`).
    pub fn name(&self) -> String {
        name(&self.filters(), self.category_is_filter)
    }
}

/// What a saved search keeps of the page at `path` with `filters` (its category one of them on
/// /search, `category_is_filter`, rather than the page): its path, and its filters as a query
/// string in the pages' fixed order (`ParsedFilters::query_pairs`), without what doesn't exist or
/// isn't on, and without its order, which the page sets when it is opened: the words, the category
/// of a search, the town and its zone, or the pin and its radius, the kind of listing, the
/// conditions, the price and its currency, then each property's (a part's vehicle's year, the
/// options chosen, a number's bounds, a box ticked). The same filters asked for in another order,
/// or with a page's cursor, write the same address, so that a search is kept once. The pages and
/// the API keep a search by it (`Replay::address`), and a page's save button looks its search up by
/// it (pluribus-web's `Filters::search_address`).
pub fn address(path: &str, filters: &ParsedFilters<'_>, category_is_filter: bool) -> (String, String) {
    let pairs = filters.query_pairs(category_is_filter);
    (path.to_string(), form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs).finish())
}

/// The name a saved search of a page with `filters` shows by (its category one of them on
/// /search, `category_is_filter`, rather than the page): its words, its page's category, then what
/// each filter's chip says (pluribus-web's `Filters::chips`), the place first (the town, its zone,
/// or the pin's "Brenda 10 km nga pika"), then the category of a search, the kind of listing, the
/// conditions, the price, and each property's ("„golf“ · Makina · Tiranë · Naftë · deri
/// 10 000 €"); "Të gjitha njoftimet" for none; cut at NAME_CHARS. It is kept in the database, so it
/// is in Albanian whatever the language asked for, as an Albanian page's chips word it. The pages
/// and the API name a search by it (`Replay::name`); the pages' own did until C12
/// (`Filters::search_name`, from the page's chips).
pub fn name(filters: &ParsedFilters<'_>, category_is_filter: bool) -> String {
    i18n::LANG.sync_scope(Lang::Sq, || {
        let mut parts = Vec::new();
        if let Some(q) = &filters.q {
            parts.push(format!("„{q}“"));
        }
        if let (false, Some(category)) = (category_is_filter, filters.category) {
            parts.push(category.name.clone());
        }
        // The place's chips: the town, its zone, the pin.
        if let Some(city) = filters.city {
            parts.push(city.name.clone());
        }
        if let Some(zone) = filters.zone {
            parts.push(zone.name.clone());
        }
        if filters.near.is_some() {
            parts.push(t1("Brenda {} km nga pika", filters.radius_km));
        }
        // The filters' chips.
        if let (true, Some(category)) = (category_is_filter, filters.category) {
            parts.push(t(&category.name).to_string());
        }
        if let Some(listing_type) = &filters.listing_type {
            parts.push(format::listing_type(listing_type).to_string());
        }
        for &condition in &filters.conditions {
            parts.push(format::condition(condition).to_string());
        }
        let symbol = format::currency_symbol(filters.currency);
        if let Some(label) = range_label(filters.price_min, filters.price_max, Some(symbol), true) {
            parts.push(label);
        }
        for property in &filters.properties {
            let attribute = property.attribute;
            if let (Some(_), Some(year)) = (property.fits_to, property.year) {
                parts.push(format!("{}: {year}", t(FITS_YEAR_LABEL)));
            }
            for option in &property.selected {
                parts.push(t(&option.label).to_string());
            }
            let unit = attribute.unit.as_deref().map(t);
            if let Some(range) = range_label(property.min, property.max, unit, unit.is_some()) {
                parts.push(format!("{}: {range}", t(&attribute.name)));
            }
            if property.checked {
                parts.push(t(&attribute.name).to_string());
            }
        }
        if parts.is_empty() {
            return "Të gjitha njoftimet".to_string();
        }
        let name = parts.join(" · ");
        match name.char_indices().nth(NAME_CHARS - 1) {
            Some((cut, _)) => format!("{}…", name[..cut].trim_end()),
            None => name,
        }
    })
}

/// A range as its chip says it (pluribus-web's filters.rs, range_label): "5 000 – 10 000 €",
/// "nga 2015", "deri 150 000 km"; years and the like not grouped.
fn range_label(min: Option<Decimal>, max: Option<Decimal>, unit: Option<&str>, grouped: bool) -> Option<String> {
    let number = |value: Decimal| if grouped { format::number(value) } else { plain(value) };
    let with_unit = |value: Decimal| match unit {
        Some(unit) => format!("{}\u{a0}{unit}", number(value)),
        None => number(value),
    };
    Some(match (min, max) {
        (Some(min), Some(max)) => format!("{} – {}", number(min), with_unit(max)),
        (Some(min), None) => t1("nga {}", with_unit(min)),
        (None, Some(max)) => t1("deri {}", with_unit(max)),
        (None, None) => return None,
    })
}

/// Sends the alerts due now by `senders`; returns how many went out. Their links start with
/// `origin` (`Config::site_origin`), and lead through /kerkimet/{id}, which opens the search.
pub async fn send_search_alerts(db: &PgPool, senders: &Senders, origin: &str) -> sqlx::Result<usize> {
    let due = db::due_search_alerts(db, ALERT_GAP).await?;
    if due.is_empty() {
        return Ok(0);
    }
    let (categories, cities) = tokio::try_join!(db::categories(db), db::cities(db))?;
    let mut sent = 0;
    for search in due {
        let Some(page) = replay(db, &categories, &cities, &search.path, &search.query).await? else {
            continue;
        };
        // One filter for the count and the newest, which the second's plan takes from the first's.
        let mut filter = page.listing_filter();
        filter.since = Some(search.since);
        filter.sort = Sort::Newest;
        let count = db::listing_count_up_to(db, &filter, NEW_MOST).await?;
        if count == 0 {
            continue;
        }
        let newest = db::listing_page(db, &filter).await?;
        let url = format!("{origin}/kerkimet/{}", search.id);
        // The notification keeps its title in Albanian (web/notifications.rs words it for its
        // reader), as the WhatsApp template's texts are; the e-mail is in its recipient's language.
        let title = i18n::LANG.sync_scope(Lang::Sq, || format!("{}: {}", new_listings(count), search.name));
        db::record_search_alert(db, search.user_id, search.id, &title, count, &url).await?;
        sent += 1;
        let contact = match (search.email, search.phone) {
            (Some(email), _) if search.email_ok => Contact::Email(email),
            (_, Some(phone)) => Contact::Phone(phone),
            _ => continue,
        };
        // An e-mail carries the link that stops this search's alerts (web/unsubscribe.rs).
        let unsubscribe = match &contact {
            Contact::Email(_) => Some(format!("{origin}/ndalo/{}/kerkimi-{}", db::notice_token(db, search.user_id).await?, search.id)),
            Contact::Phone(_) => None,
        };
        let (subject, body) = i18n::LANG.sync_scope(Lang::from_code(&search.language).unwrap_or_default(), || {
            let mut body = format!("{}\n\n", t2("Te kërkimi yt „{}“ në Pluribus ka {}:", &search.name, new_listings(count)));
            for card in newest.iter().take(ALERT_LISTINGS) {
                // Its price in full, its month or "(i diskutueshëm)" after it (pluribus-web's
                // `ListingCardExt::price_display` said it so, for these alone, until D2).
                let price = format::price(card.price_amount, &card.currency, &card.price_type, &card.listing_type);
                body.push_str(&format!("    {} · {}\n", card.title, price));
            }
            body.push_str(&format!(
                "\n{}\n\n{}\n",
                t1("Shihi të gjitha: {}", &url),
                t1(
                    "Për një kërkim nuk të vjen më shumë se një email në tri orë. Për t'i ndalur, hap „Kërkimet e ruajtura“: {}",
                    format!("{origin}/kerkimet-e-ruajtura")
                ),
            ));
            if let Some(link) = &unsubscribe {
                body.push_str(&format!("{}\n", t1("Për të ndalur vetëm këtë, me një klik: {}", link)));
            }
            (format!("{}: {}", new_listings(count), search.name), body)
        });
        let new = i18n::LANG.sync_scope(Lang::Sq, || new_listings(count));
        let alert = Alert {
            subject,
            body,
            whatsapp: AlertTemplate::Search { new, name: search.name.clone(), id: search.id.to_string() },
            unsubscribe,
        };
        match senders.send_alert(&contact, &alert).await {
            Ok(channel) => tracing::debug!(?channel, to = %contact.masked(), "saved search alert sent"),
            Err(error) => tracing::warn!(%error, to = %contact.masked(), "saved search alert not sent"),
        }
    }
    Ok(sent)
}

/// One look for the alerts due, sent by `senders` with links that start with `origin` (the
/// state's, `Config::site_origin`), a failure in the log: what the jobs run every
/// ALERT_CHECK_EVERY, as the server starts and from then on, in the process that holds the
/// alerts' lock (jobs.rs, `start_all`; pluribus-web's `start_alerts`, from its `state`, until D2).
pub async fn tick_alerts(db: &PgPool, senders: &Senders, origin: &str) {
    if let Err(error) = send_search_alerts(db, senders, origin).await {
        tracing::warn!(%error, "could not send the saved searches' alerts");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_search_is_of_a_search_or_a_categorys_page() {
        assert!(is_search_page("/search"));
        assert!(is_search_page("/c/makina"));
        assert!(!is_search_page("/mesazhet"));
        assert!(!is_search_page("/searching"));
        assert!(!is_search_page("/"));
    }

    #[test]
    fn a_range_says_its_bounds_as_a_chip_does() {
        let n = |value: i64| Some(Decimal::from(value));
        assert_eq!(range_label(n(5000), n(10000), Some("€"), true).unwrap(), "5\u{a0}000 – 10\u{a0}000\u{a0}€");
        assert_eq!(range_label(n(2015), None, None, false).unwrap(), "nga 2015");
        assert_eq!(range_label(None, n(150_000), Some("km"), true).unwrap(), "deri 150\u{a0}000\u{a0}km");
        assert_eq!(range_label(None, None, None, false), None);
    }
}
