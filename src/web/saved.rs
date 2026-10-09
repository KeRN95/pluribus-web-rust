//! Hearts: a signed-in user keeps listings with the heart over their photo (heart.html,
//! heart_controller.js) and finds them again on /te-ruajturat. Someone not signed in who taps one
//! goes to sign in first, and comes back to the page with the listing kept (after_sign_in, which
//! account.rs calls).

use askama::Template;
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    Result, cookie,
    account::{cookie_header, current_user, redirect_with_cookies},
    browse::{LISTING_IDS, ListingPage},
    not_found, render,
};
use crate::{AppState, db, format};

/// The most listings /te-ruajturat shows.
const SAVED_MOST: i64 = 200;

/// While someone signs in after tapping a heart: the listing's number, and the page they tapped
/// it on (after_sign_in).
const SAVE_COOKIE: &str = "ruaj";
pub(super) const BACK_COOKIE: &str = "kthehu";
/// How long they have to sign in, in seconds.
pub(super) const SIGN_IN_SECONDS: i64 = 30 * 60;

#[derive(Deserialize)]
pub struct SaveForm {
    /// "0" lets the listing go; anything else keeps it.
    #[serde(default)]
    ruaj: String,
}

/// POST /listing/{public_no}/ruaj: keeps the listing for the signed-in user, or with ruaj=0 lets it
/// go. heart_controller.js asks for JSON and gets 204, or 401 when nobody is signed in, and then
/// goes to sign in; a form without JavaScript goes back to its page, or to sign in. Not signed in,
/// the listing and the page wait in cookies for after_sign_in.
pub async fn save(
    State(state): State<AppState>,
    Path(public_no): Path<i64>,
    headers: HeaderMap,
    Form(form): Form<SaveForm>,
) -> Result<Response> {
    let script = accepts_json(&headers);
    let Some(user) = current_user(&state, &headers).await? else {
        let mut cookies = vec![cookie_header(&headers, SAVE_COOKIE, &public_no.to_string(), SIGN_IN_SECONDS)];
        if let Some(page) = referring_page(&headers) {
            cookies.push(cookie_header(&headers, BACK_COOKIE, &page, SIGN_IN_SECONDS));
        }
        if !script {
            return Ok(redirect_with_cookies("/llogaria", cookies));
        }
        let mut response = StatusCode::UNAUTHORIZED.into_response();
        for value in cookies {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
        return Ok(response);
    };
    if !db::set_saved(&state.db, user.id, public_no, form.ruaj != "0").await? {
        return not_found().await;
    }
    if script {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    Ok(Redirect::to(&referring_page(&headers).unwrap_or_else(|| "/".to_string())).into_response())
}

/// GET /te-ruajturat/numrat: for the hearts on a page and the header's (heart_controller.js), as
/// JSON: the numbers of the listings the visitor keeps, and how many of them dropped their price
/// since the visitor last saw /te-ruajturat (db::price_drops); none for someone not signed in.
/// `{"numbers": [1001, 1005], "price_drops": 1}`.
pub async fn numbers(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let (numbers, drops) = match current_user(&state, &headers).await? {
        Some(user) => tokio::try_join!(db::saved_numbers(&state.db, user.id), db::price_drops(&state.db, user.id))?,
        None => (Vec::new(), Vec::new()),
    };
    let body = serde_json::json!({ "numbers": numbers, "price_drops": drops.len() });
    Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(body)).into_response())
}

#[derive(Template)]
#[template(path = "saved.html")]
struct SavedPage {
    signed_in: bool,
    /// The listings kept whose price dropped since the visitor last looked, over them.
    drops: Vec<db::PriceDrop>,
    page: ListingPage,
}

impl SavedPage {
    /// Where the page says, once shown, that it showed its price drops: up to the latest of them.
    pub fn seen_url(&self) -> String {
        let latest = self.drops.iter().map(|drop| drop.notice).max().unwrap_or_default();
        format!("/te-ruajturat/pare?deri={latest}")
    }
}

impl db::PriceDrop {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// What it was and what it is, as the site writes prices: ("10 000 €", "9 500 €").
    pub fn prices(&self) -> (String, String) {
        (format::money(self.was, &self.currency), format::money(self.price, &self.currency))
    }
}

/// GET /te-ruajturat: the listings the signed-in user keeps, the last kept first, as the browse
/// pages show listings, under those whose price dropped since they last looked; for someone not
/// signed in, the way to sign in.
pub async fn page(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let (signed_in, drops, cards) = match current_user(&state, &headers).await? {
        Some(user) => {
            let (drops, cards) =
                tokio::try_join!(db::price_drops(&state.db, user.id), db::saved_cards(&state.db, user.id, SAVED_MOST))?;
            (true, drops, cards)
        }
        None => (false, Vec::new(), Vec::new()),
    };
    render(SavedPage { signed_in, drops, page: ListingPage { cards, next: None, layout: Default::default(), eager: 4, card_ids: LISTING_IDS } })
}

#[derive(Deserialize)]
pub struct Seen {
    /// The latest price drop the page showed, by its notification's id.
    deri: Uuid,
}

/// POST /te-ruajturat/pare?deri=…: /te-ruajturat was shown with its price drops (application.js,
/// the beacon), not just fetched ahead under the pointer: they are seen, up to the latest it
/// showed, and the header's heart loses its dot.
pub async fn seen(State(state): State<AppState>, headers: HeaderMap, Query(Seen { deri }): Query<Seen>) -> Result<Response> {
    if let Some(user) = current_user(&state, &headers).await? {
        db::price_drops_seen(&state.db, user.id, deri).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// For a page someone must sign in to use (reporting a listing, safety.rs): the cookie that brings
/// them back to it after signing in (after_sign_in).
pub fn back_after_sign_in(headers: &HeaderMap, page: &str) -> HeaderValue {
    cookie_header(headers, BACK_COOKIE, page, SIGN_IN_SECONDS)
}

/// After signing in (account.rs, check_code): the listing whose heart was tapped before is kept for
/// the user, and the page it was tapped on is where they go back to. The cookies that held them go.
pub async fn after_sign_in(
    state: &AppState,
    headers: &HeaderMap,
    user_id: Uuid,
) -> Result<(Option<String>, Vec<HeaderValue>)> {
    let number = cookie(headers, SAVE_COOKIE).and_then(|value| value.parse::<i64>().ok());
    let back = cookie(headers, BACK_COOKIE).filter(|page| is_local(page));
    if number.is_none() && back.is_none() {
        return Ok((None, Vec::new()));
    }
    if let Some(number) = number {
        db::set_saved(&state.db, user_id, number, true).await?;
    }
    let done = vec![cookie_header(headers, SAVE_COOKIE, "", 0), cookie_header(headers, BACK_COOKIE, "", 0)];
    Ok((back, done))
}

fn accepts_json(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("application/json"))
}

/// The page a request came from (its Referer), when it is one of the site's own: its path and
/// query, or the path alone when the query can't go in a cookie as it is.
fn referring_page(headers: &HeaderMap) -> Option<String> {
    let referer = headers.get(header::REFERER)?.to_str().ok()?;
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let page = referer.split_once("://")?.1.strip_prefix(host)?;
    let page = page.split('#').next().unwrap_or_default();
    let path = page.split('?').next().unwrap_or_default();
    [page, path].into_iter().find(|candidate| is_local(candidate)).map(str::to_string)
}

/// A path on this site, which is safe to go to and goes in a cookie as it is.
fn is_local(page: &str) -> bool {
    page.starts_with('/')
        && !page.starts_with("//")
        && page.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"/?=&%+-_.~".contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn came_from(referer: &str) -> Option<String> {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("pluribus.example"));
        headers.insert(header::REFERER, HeaderValue::from_str(referer).unwrap());
        referring_page(&headers)
    }

    #[test]
    fn only_the_sites_own_pages_are_gone_back_to() {
        assert_eq!(came_from("https://pluribus.example/").as_deref(), Some("/"));
        assert_eq!(
            came_from("https://pluribus.example/c/automjete?vehicle_fuel=DIESEL&price_max=12000#top").as_deref(),
            Some("/c/automjete?vehicle_fuel=DIESEL&price_max=12000")
        );
        // A query that can't go in a cookie as it is: the path alone.
        assert_eq!(came_from("https://pluribus.example/search?q=a;b").as_deref(), Some("/search"));
        assert_eq!(came_from("https://elsewhere.example/c/automjete"), None);
        assert_eq!(came_from("https://pluribus.example.evil/x"), None);
        assert!(!is_local("//elsewhere.example/x"));
        assert!(!is_local("https://elsewhere.example/"));
    }
}
