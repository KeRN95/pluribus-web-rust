//! The language the API speaks (pluribus-core's i18n): the one each request asks for, and never a
//! cookie, which the app doesn't keep: `?lang=sq` or `?lang=en`, else the first language of
//! `Accept-Language` the site speaks, Albanian (by its code, or its Tosk's or Gheg's) or English,
//! else Albanian. What the API says is then said in it, the error envelope's messages among them
//! (error.rs), and every answer says in `Vary` that it differs with `Accept-Language`, and with
//! `Authorization`, which signs a request in (auth.rs).

use axum::{
    extract::Request,
    http::{HeaderMap, HeaderValue, header},
    middleware::Next,
    response::Response,
};
use pluribus_core::i18n::{self, Lang};

/// Answers each request in the language it asks for (`asked`), never offering English as the
/// pages do to a browser that reads no Albanian (`i18n::OFFER`), and says so in `Vary`.
pub async fn language(request: Request, next: Next) -> Response {
    let lang = asked(request.uri().query(), request.headers());
    let mut response = i18n::LANG.scope(lang, i18n::OFFER.scope(false, next.run(request))).await;
    vary(response.headers_mut());
    response
}

/// The language a request asks for: its query's `lang`, if the site speaks the one it names; else
/// the first of `Accept-Language`'s that the site speaks (`accepted`); else Albanian.
fn asked(query: Option<&str>, headers: &HeaderMap) -> Lang {
    let named = query
        .and_then(|query| query.split('&').find_map(|pair| pair.strip_prefix("lang=")))
        .and_then(Lang::from_code);
    named.or_else(|| accepted(headers)).unwrap_or_default()
}

/// The first language `Accept-Language` names, in its order, that the site speaks: by each tag's
/// primary subtag ("sq-AL" is sq), Albanian and its Tosk and Gheg (as the pages read it,
/// pluribus-web's language.rs) or English; "*", and a language refused (q=0), passed over.
fn accepted(headers: &HeaderMap) -> Option<Lang> {
    let languages = headers.get(header::ACCEPT_LANGUAGE)?.to_str().ok()?;
    languages.split(',').find_map(|language| {
        let mut parts = language.split(';').map(str::trim);
        let tag = parts.next().unwrap_or_default();
        if parts.any(|part| part.strip_prefix("q=").and_then(|q| q.parse::<f32>().ok()) == Some(0.0)) {
            return None;
        }
        match tag.split('-').next().unwrap_or_default().to_ascii_lowercase().as_str() {
            "sq" | "sqi" | "alb" | "als" | "aln" => Some(Lang::Sq),
            "en" => Some(Lang::En),
            _ => None,
        }
    })
}

/// `Accept-Language` and `Authorization` added to the answer's `Vary`, once each, beside what it
/// says already (compression's `Accept-Encoding`), as one header: a cache must keep apart the
/// answers in each language, and those to each signed-in user.
fn vary(headers: &mut HeaderMap) {
    let mut names: Vec<String> = headers
        .get_all(header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect();
    if names.iter().any(|name| name == "*") {
        return;
    }
    for name in ["Accept-Language", "Authorization"] {
        if !names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
            names.push(name.to_string());
        }
    }
    if let Ok(value) = HeaderValue::from_str(&names.join(", ")) {
        headers.insert(header::VARY, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_languages(languages: Option<&'static str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(languages) = languages {
            headers.insert(header::ACCEPT_LANGUAGE, HeaderValue::from_static(languages));
        }
        headers
    }

    #[test]
    fn the_query_names_the_language_then_accept_language_then_albanian() {
        let asked_by = |query: Option<&str>, languages| asked(query, &with_languages(languages));
        assert_eq!(asked_by(None, None), Lang::Sq, "nothing asked: Albanian");
        assert_eq!(asked_by(None, Some("en-GB,en;q=0.9")), Lang::En);
        assert_eq!(asked_by(None, Some("it-IT,it;q=0.9,en;q=0.8")), Lang::En, "the first the site speaks");
        assert_eq!(asked_by(None, Some("de-DE, sq-AL;q=0.5, en;q=0.4")), Lang::Sq, "in the header's order");
        assert_eq!(asked_by(None, Some("en;q=0, SQ")), Lang::Sq, "English refused");
        assert_eq!(asked_by(None, Some("als, en")), Lang::Sq, "Tosk");
        assert_eq!(asked_by(None, Some("de, *")), Lang::Sq, "none the site speaks: Albanian");
        assert_eq!(asked_by(Some("lang=sq"), Some("en")), Lang::Sq, "the query first");
        assert_eq!(asked_by(Some("after=x&lang=en"), None), Lang::En);
        assert_eq!(asked_by(Some("lang=fr"), Some("en")), Lang::En, "a language the site doesn't speak");
        assert_eq!(asked_by(Some("language=en"), None), Lang::Sq, "another parameter");
    }

    #[test]
    fn vary_names_the_language_and_the_token_once_beside_what_it_had() {
        let mut headers = HeaderMap::new();
        vary(&mut headers);
        assert_eq!(headers.get(header::VARY).unwrap(), "Accept-Language, Authorization");
        let mut headers = HeaderMap::new();
        headers.append(header::VARY, HeaderValue::from_static("accept-encoding"));
        headers.append(header::VARY, HeaderValue::from_static("accept-language"));
        vary(&mut headers);
        assert_eq!(headers.get_all(header::VARY).iter().collect::<Vec<_>>(), ["accept-encoding, accept-language, Authorization"]);
        let mut headers = HeaderMap::new();
        headers.insert(header::VARY, HeaderValue::from_static("*"));
        vary(&mut headers);
        assert_eq!(headers.get(header::VARY).unwrap(), "*");
    }
}
