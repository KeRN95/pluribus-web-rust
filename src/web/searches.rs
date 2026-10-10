//! Saved searches: a category or search page, with its filters, kept by a signed-in visitor with
//! "Ruaj kërkimin" (browse.html), to open again from /kerkimet-e-ruajtura, which says how many
//! listings came since it was last opened; and their alerts, every ALERT_CHECK_EVERY: a search
//! with listings new since its last alert hears of them, at most once in ALERT_GAP, as a
//! notification (SAVED_SEARCH), which puts the dot on the heart that leads to the listings kept
//! (heart_controller.js), and by e-mail where its saver has an address. Each search keeps its page
//! and its filters as the page itself writes them (Filters::search_address), so that the same
//! search saved twice is one, and its listings are found as its page finds them (replay).

use std::time::Duration;

use askama::Template;
use axum::{
    Form,
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    Result,
    account::{current_user, redirect_with_cookies},
    filters::{Filters, Params},
    not_found, render, saved,
};
use crate::{
    AppState,
    db::{self, Categories, City, ListingFilter, NewSearch, SavedSearch, Sort},
    format::Contact,
    i18n::{self, Lang, t1, t2},
    sender,
};

/// At most one alert per search in this long, however many listings come.
pub const ALERT_GAP: Duration = Duration::from_secs(3 * 60 * 60);
/// How often the server looks for alerts due.
pub const ALERT_CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
/// New listings are counted up to this many; past it, "99+".
const NEW_MOST: i64 = 99;
/// How many of the new listings an alert's e-mail names.
const ALERT_LISTINGS: usize = 3;

/// A search page as its path and query make it, as browse.rs makes it (the town a cookie
/// remembers aside, which a saved search has written in): what a saved search keeps of it, and
/// its listing query.
struct Replayed {
    path: String,
    query: String,
    name: String,
    category_id: Option<Uuid>,
    words: Option<String>,
    filter: ListingFilter,
}

/// The page at `path` (/search, /c/<slug>) with the filters of `query`; None for a path that is
/// neither or a category that doesn't exist.
async fn replay(db: &PgPool, categories: &Categories, cities: &[City], path: &str, query: &str) -> sqlx::Result<Option<Replayed>> {
    let page_slug = match path {
        "/search" => None,
        _ => match path.strip_prefix("/c/") {
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
    let attributes = match slug {
        Some(slug) => db::filter_attributes(db, slug).await?,
        None => Vec::new(),
    };
    let words = params.search_words();
    let search = db::text_search(db, words.as_deref()).await?;
    let filters = Filters::new(&params, path.to_string(), category, page_slug.is_none(), categories, cities, &attributes);
    let (path, query) = filters.search_address();
    Ok(Some(Replayed {
        path,
        query,
        name: filters.search_name(),
        category_id: category.map(|category| category.id),
        words: filters.q.clone(),
        filter: filters.listing_filter(search),
    }))
}

/// A search's address, its path and its query.
fn address(path: &str, query: &str) -> String {
    match query.is_empty() {
        true => path.to_string(),
        false => format!("{path}?{query}"),
    }
}

/// "1 njoftim i ri", "12 njoftime të reja", "99+ njoftime të reja", in the language now: a page's,
/// or an alert's e-mail's, its recipient's; Albanian for the notification it keeps and for its
/// WhatsApp template.
pub fn new_listings(count: i64) -> String {
    match count {
        1 => i18n::t("1 njoftim i ri").to_string(),
        count if count > NEW_MOST => t1("{}+ njoftime të reja", NEW_MOST),
        count => t1("{} njoftime të reja", count),
    }
}

#[derive(Deserialize)]
pub struct SaveForm {
    /// The page's address, its path and its query (Filters::search_address).
    adresa: String,
}

/// POST /kerkimet/ruaj: keeps the search at `adresa` for the signed-in visitor, as its own page
/// writes it, and goes back to it; someone not signed in signs in first and comes back to it.
pub async fn save(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<SaveForm>) -> Result<Response> {
    let (path, query) = form.adresa.split_once('?').unwrap_or((form.adresa.as_str(), ""));
    if path != "/search" && !path.starts_with("/c/") {
        return not_found().await;
    }
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(redirect_with_cookies("/llogaria", vec![saved::back_after_sign_in(&headers, &form.adresa)]));
    };
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let Some(page) = replay(&state.db, &categories, &cities, path, query).await? else {
        return not_found().await;
    };
    let search = NewSearch {
        name: &page.name,
        category_id: page.category_id,
        words: page.words.as_deref(),
        path: &page.path,
        query: &page.query,
    };
    if db::save_search(&state.db, user.id, &search).await?.is_none() {
        // As many as one keeps: their page says so.
        return Ok(Redirect::to("/kerkimet-e-ruajtura?plot=1").into_response());
    }
    Ok(Redirect::to(&address(&page.path, &page.query)).into_response())
}

/// POST /kerkimet/{id}/fshi: lets the search go, from its page ("Kërkimi u ruajt", tapped again)
/// or from /kerkimet-e-ruajtura, which it goes back to.
pub async fn delete(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let (Some(user), Ok(id)) = (current_user(&state, &headers).await?, id.parse::<Uuid>()) else {
        return not_found().await;
    };
    if !db::delete_search(&state.db, user.id, id).await? {
        return not_found().await;
    }
    let back = saved::referring_page(&headers).unwrap_or_else(|| "/kerkimet-e-ruajtura".to_string());
    Ok(Redirect::to(&back).into_response())
}

#[derive(Deserialize)]
pub struct AlertsForm {
    /// "1" turns the search's alerts on; anything else, or nothing, off.
    #[serde(default)]
    lajmero: String,
}

/// POST /kerkimet/{id}/lajmerime: turns the search's alerts on (lajmero=1) or off, back on
/// /kerkimet-e-ruajtura.
pub async fn alerts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<AlertsForm>,
) -> Result<Response> {
    let (Some(user), Ok(id)) = (current_user(&state, &headers).await?, id.parse::<Uuid>()) else {
        return not_found().await;
    };
    if !db::set_search_alerts(&state.db, user.id, id, form.lajmero == "1").await? {
        return not_found().await;
    }
    Ok(Redirect::to("/kerkimet-e-ruajtura").into_response())
}

/// GET /kerkimet/{id}: opens the search, from /kerkimet-e-ruajtura or an alert: from now on its
/// new listings count from now, and its alerts are seen. Someone not signed in signs in first.
pub async fn open(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(redirect_with_cookies("/llogaria", vec![saved::back_after_sign_in(&headers, &format!("/kerkimet/{id}"))]));
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return not_found().await;
    };
    let Some((path, query)) = db::open_search(&state.db, user.id, id).await? else {
        return not_found().await;
    };
    Ok(Redirect::to(&address(&path, &query)).into_response())
}

/// One saved search on /kerkimet-e-ruajtura, with how many listings came since it was last opened
/// (None if its category is gone).
pub struct SearchRow {
    pub search: SavedSearch,
    pub new: Option<i64>,
}

impl SearchRow {
    /// How many listings came, as new_listings says it, in the page's language.
    pub fn new_text(&self) -> Option<String> {
        self.new.filter(|count| *count > 0).map(new_listings)
    }
}

#[derive(Template)]
#[template(path = "saved_searches.html")]
struct SavedSearchesPage {
    signed_in: bool,
    rows: Vec<SearchRow>,
    /// Alerts not seen yet: once shown, the page says so (data-seen-url, application.js) and the
    /// heart loses its dot.
    unseen: i64,
    /// Back from saving one more than SEARCHES_MOST.
    full: bool,
}

/// GET /kerkimet-e-ruajtura: the visitor's saved searches, the last saved first, each with how
/// many listings came since it was last opened, its alerts on or off, and a way to let it go; for
/// someone not signed in, the way to sign in.
pub async fn page(State(state): State<AppState>, headers: HeaderMap, RawQuery(query): RawQuery) -> Result<Response> {
    let full = query.as_deref() == Some("plot=1");
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SavedSearchesPage { signed_in: false, rows: Vec::new(), unseen: 0, full });
    };
    let (searches, categories, cities, unseen) = tokio::try_join!(
        db::saved_searches(&state.db, user.id),
        db::categories(&state.db),
        db::cities(&state.db),
        db::unseen_search_alerts(&state.db, user.id),
    )?;
    let mut rows = Vec::with_capacity(searches.len());
    for search in searches {
        let new = match replay(&state.db, &categories, &cities, &search.path, &search.query).await? {
            Some(mut page) => {
                page.filter.since = Some(search.seen_since);
                Some(db::listing_count_up_to(&state.db, &page.filter, NEW_MOST).await?)
            }
            None => None,
        };
        rows.push(SearchRow { search, new });
    }
    render(SavedSearchesPage { signed_in: true, rows, unseen, full })
}

/// POST /kerkimet/pare: /kerkimet-e-ruajtura was shown (application.js, the beacon), not just
/// fetched ahead under the pointer: the alerts of the searches it lists are seen.
pub async fn seen(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(user) = current_user(&state, &headers).await? {
        db::search_alerts_seen(&state.db, user.id).await?;
    }
    Ok(axum::http::StatusCode::NO_CONTENT.into_response())
}

/// Sends the alerts due now; returns how many went out. Their links start with `origin`
/// (web::site_origin), and lead through /kerkimet/{id}, which opens the search.
pub async fn send_search_alerts(db: &PgPool, origin: &str) -> sqlx::Result<usize> {
    let due = db::due_search_alerts(db, ALERT_GAP).await?;
    if due.is_empty() {
        return Ok(0);
    }
    let (categories, cities) = tokio::try_join!(db::categories(db), db::cities(db))?;
    let mut sent = 0;
    for search in due {
        let Some(mut page) = replay(db, &categories, &cities, &search.path, &search.query).await? else {
            continue;
        };
        page.filter.since = Some(search.since);
        page.filter.sort = Sort::Newest;
        let count = db::listing_count_up_to(db, &page.filter, NEW_MOST).await?;
        if count == 0 {
            continue;
        }
        let newest = db::listing_page(db, &page.filter).await?;
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
                body.push_str(&format!("    {} · {}\n", card.title, card.price_display()));
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
        let alert = sender::Alert {
            subject,
            body,
            whatsapp: sender::AlertTemplate::Search { new, name: search.name.clone(), id: search.id.to_string() },
            unsubscribe,
        };
        match sender::senders().send_alert(&contact, &alert).await {
            Ok(channel) => tracing::debug!(?channel, to = %contact.masked(), "saved search alert sent"),
            Err(error) => tracing::warn!(%error, to = %contact.masked(), "saved search alert not sent"),
        }
    }
    Ok(sent)
}

/// Looks for the alerts due every ALERT_CHECK_EVERY, from now on, in the background.
pub fn start_alerts(db: PgPool) {
    tokio::spawn(async move {
        let origin = super::site_origin();
        let mut every = tokio::time::interval(ALERT_CHECK_EVERY);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            if let Err(error) = send_search_alerts(&db, &origin).await {
                tracing::warn!(%error, "could not send the saved searches' alerts");
            }
        }
    });
}
