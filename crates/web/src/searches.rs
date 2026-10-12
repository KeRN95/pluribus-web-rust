//! Saved searches: a category or search page, with its filters, kept by a signed-in visitor with
//! "Ruaj kërkimin" (browse.html), to open again from /kerkimet-e-ruajtura, which says how many
//! listings came since it was last opened; and their alerts, every ALERT_CHECK_EVERY: a search
//! with listings new since its last alert hears of them, at most once in ALERT_GAP, as a
//! notification (SAVED_SEARCH), which puts the dot on the heart that leads to the listings kept
//! (heart_controller.js), and by e-mail where its saver has an address. Each search keeps its page
//! and its filters as the page itself writes them (Filters::search_address), so that the same
//! search saved twice is one, and its listings are found as its page finds them: its page made
//! again from them (pluribus-services' searches.rs, `replay`, which the JSON API's saved searches
//! share; this file's until C7).

use std::time::Duration;

use askama::Template;
use axum::{
    Form,
    extract::{Path, RawQuery, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use pluribus_services::searches::{is_search_page, replay};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    Result,
    account::{current_user, redirect_with_cookies},
    filters::Filters,
    listings::ListingCardExt,
    not_found, render, saved,
};
use crate::{
    AppState,
    db::{self, NewSearch, SavedSearch, Sort},
    domain::notices::{NEW_MOST, new_listings},
    format::Contact,
    i18n::{self, Lang, t1, t2},
    sender,
};

/// At most one alert per search in this long, however many listings come.
pub const ALERT_GAP: Duration = Duration::from_secs(3 * 60 * 60);
/// How often the server looks for alerts due.
pub const ALERT_CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
/// How many of the new listings an alert's e-mail names.
const ALERT_LISTINGS: usize = 3;

/// A search's address, its path and its query.
fn address(path: &str, query: &str) -> String {
    match query.is_empty() {
        true => path.to_string(),
        false => format!("{path}?{query}"),
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
    if !is_search_page(path) {
        return not_found().await;
    }
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(redirect_with_cookies("/llogaria", vec![saved::back_after_sign_in(&headers, &form.adresa)]));
    };
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let Some(page) = replay(&state.db, &categories, &cities, path, query).await? else {
        return not_found().await;
    };
    // Its address and its name as the page's own filters write them, which its save button looks
    // for it by (browse.rs); the API's, from the same page made again, say the same.
    let filters = Filters::new(
        &page.params,
        page.path.clone(),
        page.category,
        page.category_is_filter,
        &categories,
        &cities,
        &page.attributes,
    );
    let (path, query) = filters.search_address();
    let name = filters.search_name();
    let search = NewSearch {
        name: &name,
        category_id: page.category_id(),
        words: filters.q.as_deref(),
        path: &path,
        query: &query,
    };
    if db::save_search(&state.db, user.id, &search).await?.is_none() {
        // As many as one keeps: their page says so.
        return Ok(Redirect::to("/kerkimet-e-ruajtura?plot=1").into_response());
    }
    Ok(Redirect::to(&address(&path, &query)).into_response())
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
            Some(page) => Some(page.new_since(&state.db, search.seen_since).await?),
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

/// Sends the alerts due now by `senders`; returns how many went out. Their links start with
/// `origin` (`Config::site_origin`), and lead through /kerkimet/{id}, which opens the search.
pub async fn send_search_alerts(db: &PgPool, senders: &sender::Senders, origin: &str) -> sqlx::Result<usize> {
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
        match senders.send_alert(&contact, &alert).await {
            Ok(channel) => tracing::debug!(?channel, to = %contact.masked(), "saved search alert sent"),
            Err(error) => tracing::warn!(%error, to = %contact.masked(), "saved search alert not sent"),
        }
    }
    Ok(sent)
}

/// Looks for the alerts due every ALERT_CHECK_EVERY, from now on, in the background, and sends
/// them by `senders` with links that start with `origin` (the state's, `web::state`).
pub fn start_alerts(db: PgPool, senders: std::sync::Arc<sender::Senders>, origin: String) {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(ALERT_CHECK_EVERY);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            if let Err(error) = send_search_alerts(&db, &senders, &origin).await {
                tracing::warn!(%error, "could not send the saved searches' alerts");
            }
        }
    });
}
