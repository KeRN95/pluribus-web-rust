//! Which language the pages speak (i18n.rs): the visitor's choice, kept a year in a cookie
//! (LANG_COOKIE) by the switch at every page's foot and in "Cilësimet"; Albanian until they choose.
//! A signed-in user's choice is kept on their account too (users.preferred_language), for what
//! reaches them with the site closed: their e-mails. A visitor who hasn't chosen, whose browser
//! reads no Albanian (Accept-Language: a foreigner's, or the phone of someone abroad), is offered
//! English at the top of the page, in English, until they choose either.

use axum::{
    Form,
    extract::{Request, State},
    http::{HeaderMap, header},
    middleware::Next,
    response::Response,
};
use serde::Deserialize;

use super::{
    Result,
    account::{cookie_header, current_user, redirect_with_cookies},
    cookie,
};
use crate::{
    AppState, db,
    i18n::{self, Lang},
};

/// The cookie that keeps the language chosen: "sq" or "en".
pub const LANG_COOKIE: &str = "gjuha";

/// Answers every request in the language its cookie names (i18n::LANG), Albanian without one;
/// without one, to a browser that reads no Albanian, the page offers English (i18n::OFFER).
pub async fn language(request: Request, next: Next) -> Response {
    let chosen = cookie(request.headers(), LANG_COOKIE).and_then(|code| Lang::from_code(&code));
    let offer = chosen.is_none() && reads_no_albanian(request.headers());
    i18n::LANG.scope(chosen.unwrap_or_default(), i18n::OFFER.scope(offer, next.run(request))).await
}

/// Whether the browser names the languages it reads (Accept-Language), and Albanian isn't one
/// of them; a search engine names none.
fn reads_no_albanian(headers: &HeaderMap) -> bool {
    let Some(languages) = headers.get(header::ACCEPT_LANGUAGE).and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let mut named = false;
    for language in languages.split(',') {
        let mut parts = language.split(';').map(str::trim);
        let tag = parts.next().unwrap_or_default();
        let refused = parts.any(|part| part.strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()) == Some(0.0));
        if tag.is_empty() || tag == "*" || refused {
            continue;
        }
        named = true;
        let primary = tag.split('-').next().unwrap_or_default().to_ascii_lowercase();
        // Albanian, and its Tosk and Gheg.
        if matches!(primary.as_str(), "sq" | "sqi" | "alb" | "als" | "aln") {
            return false;
        }
    }
    named
}

#[derive(Deserialize)]
pub struct Choice {
    #[serde(default)]
    gjuha: String,
}

/// POST /gjuha: the language chosen, kept a year, and on the account of someone signed in; back to
/// the page it was chosen on (the form's Referer, this site's), else the welcome page.
pub async fn choose(State(state): State<AppState>, headers: HeaderMap, Form(choice): Form<Choice>) -> Result<Response> {
    let lang = Lang::from_code(&choice.gjuha).unwrap_or_default();
    if let Some(user) = current_user(&state, &headers).await? {
        db::set_preferred_language(&state.db, user.id, lang.code()).await?;
    }
    let back = came_from(&headers).unwrap_or_else(|| "/".to_string());
    Ok(redirect_with_cookies(&back, vec![cookie_header(&headers, LANG_COOKIE, lang.code(), 365 * 24 * 60 * 60)]))
}

/// The path (and query) of the page a form was sent from, when it is this site's.
fn came_from(headers: &HeaderMap) -> Option<String> {
    let referer = headers.get(header::REFERER)?.to_str().ok()?;
    let host = headers.get(header::HOST)?.to_str().ok()?;
    let rest = referer.split_once("://")?.1;
    let path = rest.strip_prefix(host)?;
    (path.starts_with('/') && !path.starts_with("//")).then(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn a_choice_goes_back_to_this_sites_own_page_alone() {
        let headers = |referer: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, HeaderValue::from_static("pluribus.al"));
            headers.insert(header::REFERER, HeaderValue::from_str(referer).unwrap());
            headers
        };
        assert_eq!(came_from(&headers("https://pluribus.al/c/makina?q=golf")).as_deref(), Some("/c/makina?q=golf"));
        assert_eq!(came_from(&headers("https://another.example/c/makina")), None);
        assert_eq!(came_from(&headers("https://pluribus.al.another.example/")), None);
        assert_eq!(came_from(&headers("https://pluribus.al//another.example/")), None);
    }

    #[test]
    fn english_is_offered_to_a_browser_that_reads_no_albanian() {
        let reads = |languages: Option<&'static str>| {
            let mut headers = HeaderMap::new();
            if let Some(languages) = languages {
                headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static(languages));
            }
            !reads_no_albanian(&headers)
        };
        assert!(!reads(Some("en-GB,en;q=0.9")));
        assert!(!reads(Some("it-IT,it;q=0.9,en;q=0.8")));
        assert!(!reads(Some("en, sq;q=0")), "Albanian refused");
        assert!(reads(Some("sq-AL,sq;q=0.9,en;q=0.8")));
        assert!(reads(Some("en-US,en;q=0.9,SQ;q=0.5")));
        assert!(reads(None), "a search engine");
        assert!(reads(Some("*")));
    }
}
