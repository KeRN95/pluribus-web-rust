//! Signing a request in: a bearer token (`Authorization: Bearer <token>`), as the phone app sends
//! it, which names a session of the same table the pages' cookie does (`sessions`, each token kept
//! as its SHA-256, `pluribus_services::hash`, as the pages keep theirs), so that "Cilësimet" counts
//! the app among the devices signed in, and signing out everywhere else ends it too. The token's
//! session must be good and its account active (`db::session_user`, which also moves when the
//! session was last used). A route that needs someone signed in takes `ApiUser`, and answers 401
//! without one; a route anyone may call, that answers the signed-in otherwise, `OptionalApiUser`.
//! The pages' cookie signs no API request in.

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};
use pluribus_db::{self as db, SessionUser};
use pluribus_services::AppState;

use crate::ApiError;

/// The signed-in user, for a route that needs one: 401 (`ApiError::Unauthorized`) without a
/// bearer token, or with one no good session has (wrong, expired, or its account closed or
/// suspended).
pub struct ApiUser(pub SessionUser);

/// The signed-in user, if any, for a route anyone may call: none without a bearer token (or with
/// another kind of `Authorization`); 401 for a bearer token no good session has, so that the app
/// hears its session ended, rather than going on as nobody.
pub struct OptionalApiUser(pub Option<SessionUser>);

impl FromRequestParts<AppState> for ApiUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let token = bearer(&parts.headers).ok_or(ApiError::Unauthorized)?;
        signed_in(state, token).await.map(Self)
    }
}

impl FromRequestParts<AppState> for OptionalApiUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        match bearer(&parts.headers) {
            Some(token) => signed_in(state, token).await.map(|user| Self(Some(user))),
            None => Ok(Self(None)),
        }
    }
}

/// The bearer token the request carries: what follows the scheme in `Authorization`, when the
/// scheme is Bearer (in any case, as RFC 7235 has schemes).
fn bearer(headers: &HeaderMap) -> Option<&str> {
    let (scheme, token) = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim().split_once(' ')?;
    let token = token.trim();
    (scheme.eq_ignore_ascii_case("bearer") && !token.is_empty()).then_some(token)
}

/// The user whose good session the token names, else 401.
async fn signed_in(state: &AppState, token: &str) -> Result<SessionUser, ApiError> {
    db::session_user(&state.db, &pluribus_services::hash(token)).await?.ok_or(ApiError::Unauthorized)
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn the_token_is_what_follows_bearer_in_any_case() {
        let token = |authorization: Option<&'static str>| {
            let mut headers = HeaderMap::new();
            if let Some(authorization) = authorization {
                headers.insert(header::AUTHORIZATION, HeaderValue::from_static(authorization));
            }
            bearer(&headers).map(str::to_string)
        };
        assert_eq!(token(Some("Bearer abc123")).as_deref(), Some("abc123"));
        assert_eq!(token(Some("bearer  abc123 ")).as_deref(), Some("abc123"));
        assert_eq!(token(Some("BEARER abc123")).as_deref(), Some("abc123"));
        assert_eq!(token(Some("Basic YWxhZGRpbjpvcGVuc2VzYW1l")), None, "another scheme");
        assert_eq!(token(Some("Bearer")), None);
        assert_eq!(token(Some("Bearer ")), None);
        assert_eq!(token(Some("abc123")), None);
        assert_eq!(token(None), None);
    }
}
