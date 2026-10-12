//! Saved searches, as the phone app keeps them, by what the pages keep them by (pluribus-web's
//! searches.rs, /kerkimet-e-ruajtura): a signed-in user's searches, each a page and its filters,
//! with how many listings came since it was last opened (`GET /searches`); keeping one
//! (`POST /searches`), its page and filters written and named as the pages write and name them
//! (pluribus-services' searches.rs), so that the same search kept from the app or a page is one;
//! letting one go (`DELETE /searches/{id}`); its alerts on or off (`PUT /searches/{id}/alerts`);
//! and its listings, a page at a time, as its page finds them (`GET /searches/{id}/listings`), the
//! first page opening it as /kerkimet/{id} does: its new listings count from then on, and its
//! alerts are seen. Another user's search is not found, as on the pages. The shapes they answer in
//! are here, beside their handlers.

use anyhow::anyhow;
use axum::{
    Json,
    extract::{Path, RawQuery, State, rejection::PathRejection},
    http::{HeaderMap, StatusCode},
};
use pluribus_core::{
    domain::notices::NEW_MOST,
    i18n::{t, t1},
};
use pluribus_db::{self as db, Categories, City, Cursor, NewSearch, SavedSearch};
use pluribus_services::{
    AppState,
    filters::Params,
    searches::{CATEGORY_PAGES, is_search_page, replay},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody, Page,
    browse::{CardDto, Caller, CountDto, cards_of, origin, paged, unreadable},
};

/// A saved search (`GET /searches`, `POST /searches`).
#[derive(Debug, Serialize)]
pub struct SearchDto {
    pub id: Uuid,
    /// What it shows by, as the pages name it, in Albanian whatever the language asked for: its
    /// words, its page's category and what each filter's chip says, "„golf“ · Makina · Tiranë ·
    /// Naftë", or "Të gjitha njoftimet".
    pub name: String,
    /// Its page, /search or a category's (/c/makina), and its filters as a query string in the
    /// pages' vocabulary, which `/listings` takes too ("city=tirane&vehicle_fuel=DIESEL"; a
    /// category's page's category is its path's).
    pub path: String,
    pub query: String,
    /// Whether it tells of new listings: a notification, and an e-mail or a WhatsApp message where
    /// its saver has one, at most one in three hours.
    pub alerts: bool,
    /// How many listings came since it was last opened, or kept, up to 99, and whether more did
    /// ("99+"); null for one that no longer reads, its category gone.
    pub new_count: Option<CountDto>,
    /// When it was kept.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it was last opened (its listings' first page, here; /kerkimet/{id} on the pages); null
    /// until it is.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_seen_at: Option<OffsetDateTime>,
}

impl SearchDto {
    fn new(search: SavedSearch, new: Option<i64>) -> Self {
        Self {
            id: search.id,
            name: search.name,
            path: search.path,
            query: search.query,
            alerts: search.alerts_enabled,
            new_count: new.map(|count| CountDto { n: count.min(NEW_MOST), more: count > NEW_MOST }),
            created_at: search.created_at,
            last_seen_at: search.last_seen_at,
        }
    }
}

/// `GET /searches`: the user's saved searches, the last kept first, each with how many listings
/// came since it was last opened, as /kerkimet-e-ruajtura lists them.
pub async fn list(State(state): State<AppState>, ApiUser(user): ApiUser) -> Result<Json<Vec<SearchDto>>, ApiError> {
    let (searches, categories, cities) =
        tokio::try_join!(db::saved_searches(&state.db, user.id), db::categories(&state.db), db::cities(&state.db))?;
    let mut list = Vec::with_capacity(searches.len());
    for search in searches {
        let new = new_since_seen(&state.db, &categories, &cities, &search).await?;
        list.push(SearchDto::new(search, new));
    }
    Ok(Json(list))
}

/// How many listings came to the search's page since it was last opened, or kept
/// (`Replay::new_since`); none where its page no longer reads, its category gone.
async fn new_since_seen(db: &PgPool, categories: &Categories, cities: &[City], search: &SavedSearch) -> sqlx::Result<Option<i64>> {
    match replay(db, categories, cities, &search.path, &search.query).await? {
        Some(page) => page.new_since(db, search.seen_since).await.map(Some),
        None => Ok(None),
    }
}

/// What `POST /searches` takes: a page the pages' "Ruaj kërkimin" keeps, its path (/search, or a
/// category's, /c/makina) and its filters' query string, in the pages' vocabulary, as
/// `/listings` takes it ("city=tirane&vehicle_fuel=DIESEL"; a "?" before it is left out).
#[derive(Debug, Deserialize)]
pub struct NewSearchBody {
    pub path: String,
    #[serde(default)]
    pub query: String,
}

/// `POST /searches`: keeps the search, its page made again as the pages make it
/// (pluribus-services' `replay`): its address and its name written as the page writes them, so that
/// the same filters asked for in another order are the one search, the one the page's own "Ruaj
/// kërkimin" keeps; once, however often (db::save_search). Answers the search. A path that is
/// neither a search nor a category's page is a validation error on `path`, as is a category that
/// doesn't exist (on `query` for /search's `category`), and one more than the most a user keeps
/// (db::SEARCHES_MOST) one on `searches`, its message the page's.
pub async fn save(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<NewSearchBody>,
) -> Result<Json<SearchDto>, ApiError> {
    let query = body.query.strip_prefix('?').unwrap_or(&body.query);
    if !is_search_page(&body.path) {
        let why = t("Ruhet vetëm faqja e një kategorie (/c/…) ose një kërkim (/search).");
        return Err(ApiError::Validation(vec![("path", why.to_string())]));
    }
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let Some(page) = replay(&state.db, &categories, &cities, &body.path, query).await? else {
        let field = if body.path.starts_with(CATEGORY_PAGES) { "path" } else { "query" };
        return Err(ApiError::Validation(vec![(field, t("Kjo kategori nuk ekziston.").to_string())]));
    };
    let (path, query) = page.address();
    let (name, words) = (page.name(), page.words());
    let new = NewSearch { name: &name, category_id: page.category_id(), words: words.as_deref(), path: &path, query: &query };
    let Some(id) = db::save_search(&state.db, user.id, &new).await? else {
        // No field of the body is wrong: the page's words say why, as the envelope's message too.
        let why = t1("Ke ruajtur {} kërkime, sa mund të mbash: fshi një për të ruajtur një tjetër.", db::SEARCHES_MOST);
        return Err(ApiError::Validation(vec![("searches", why.clone())]).said(why));
    };
    let search = db::saved_searches(&state.db, user.id)
        .await?
        .into_iter()
        .find(|search| search.id == id)
        .ok_or_else(|| anyhow!("saved search {id} not among its user's"))?;
    let new = page.new_since(&state.db, search.seen_since).await?;
    Ok(Json(SearchDto::new(search, Some(new))))
}

/// `DELETE /searches/{id}`: lets the search go; another's, or none, is not found.
pub async fn delete(State(state): State<AppState>, ApiUser(user): ApiUser, id: Result<Path<Uuid>, PathRejection>) -> Result<StatusCode, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    match db::delete_search(&state.db, user.id, id).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(ApiError::NotFound),
    }
}

/// What `PUT /searches/{id}/alerts` takes: whether the search tells of new listings.
#[derive(Debug, Deserialize)]
pub struct AlertsBody {
    pub on: bool,
}

/// `PUT /searches/{id}/alerts`: turns the search's alerts on or off (`on`); another's, or none, is
/// not found.
pub async fn alerts(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    id: Result<Path<Uuid>, PathRejection>,
    JsonBody(body): JsonBody<AlertsBody>,
) -> Result<StatusCode, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    match db::set_search_alerts(&state.db, user.id, id, body.on).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(ApiError::NotFound),
    }
}

/// `GET /searches/{id}/listings?after=`: a page of the search's listings, as its page finds them,
/// in the order it opens in (newest first, or the best match first with words, or the nearest
/// first with a pin), the accounts the user blocked left out; `next`, sent back as `after`, the
/// page after it. The first page opens the search, as /kerkimet/{id} does (db::open_search): its
/// new listings count from now on, and its alerts are seen; the pages after it only read it, so
/// that what came meanwhile, at the top, still counts as new. Another's search, or one whose
/// category is gone, is not found; an `after` that doesn't read, a validation error.
pub async fn listings(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    id: Result<Path<Uuid>, PathRejection>,
    RawQuery(raw): RawQuery,
) -> Result<Json<Page<CardDto>>, ApiError> {
    let Caller { user, hidden } = caller;
    let user = user.ok_or(ApiError::Unauthorized)?;
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    let params = Params::parse(raw.as_deref());
    let after = params.first("after");
    let kept = match after {
        None => db::open_search(&state.db, user.id, id).await?,
        Some(_) => db::saved_searches(&state.db, user.id)
            .await?
            .into_iter()
            .find(|search| search.id == id)
            .map(|search| (search.path, search.query)),
    };
    let (path, query) = kept.ok_or(ApiError::NotFound)?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let page = replay(&state.db, &categories, &cities, &path, &query).await?.ok_or(ApiError::NotFound)?;
    let mut filter = page.listing_filter();
    filter.hidden_accounts = hidden;
    if let Some(after) = after {
        filter.after = Some(Cursor::decode(filter.sort, after).ok_or_else(|| unreadable("after"))?);
    }
    let (cards, next) = paged(db::listing_page(&state.db, &filter).await?, filter.sort);
    Ok(Json(Page { items: cards_of(&cards, &origin(&state.config, &headers))?, next }))
}
