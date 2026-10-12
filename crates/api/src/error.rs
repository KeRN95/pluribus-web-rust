//! Why a request failed, answered in one envelope whatever went wrong:
//! `{"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}}`, and for what
//! was sent wrong, each field's message under the request's own name for it,
//! `{"error": {"code": "validation", "message": "…", "fields": {"body": "…"}}}`; for a request that
//! came too soon, where that is known, in how many seconds to try again,
//! `{"error": {"code": "rate_limited", "message": "…", "retry_after": 42}}`. The code is for the
//! app to act on, the same in every language; the messages are for a person to read, in the
//! language the request asks for (lang.rs), from the catalogue (i18n/en/api.txt), each code's own,
//! or one of a refusal's own where it says more (`ApiError::said`: a sign-in code that expired, an
//! account suspended). An error's details stay in the log, never in the answer. The envelope is a
//! schema of the OpenAPI document (`ErrorEnvelope`, `ErrorBody`), and each status a response of
//! its own there (openapi.rs).

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use pluribus_core::t;
use serde::Serialize;
use utoipa::ToSchema;

use crate::openapi::kinds;

/// Why a request failed.
#[derive(Debug)]
pub enum ApiError {
    /// No bearer token where the route needs one, or one no good session has (wrong, expired, or
    /// its account closed or suspended): 401, with `WWW-Authenticate: Bearer`.
    Unauthorized,
    /// Who is asking is known, and may not: a suspended account whose sign-in code came back right
    /// (signin.rs): 403.
    Forbidden,
    /// Nothing there, or nothing the caller may see (what isn't theirs answers so, as on the
    /// pages): 404.
    NotFound,
    /// What was sent isn't right: each field, by the request's own name for it, with what is
    /// wrong with it, said in the request's language (the first, for a field with several): 422.
    Validation(Vec<(&'static str, String)>),
    /// Too many requests too quickly: 429.
    RateLimited,
    /// Anything else that ends a request, a database's error among them: logged in full,
    /// answered without its details: 500.
    Internal(anyhow::Error),
    /// One of the errors above, its code and status, said in words of its own rather than its
    /// code's (`said`), in the request's language: what a refusal says, a sign-in code that
    /// expired, an account suspended; and, for one that came too soon, in how many seconds to try
    /// again, where that is known (`retry_after`), in the envelope and in a `Retry-After` header.
    Said { error: Box<ApiError>, message: String, retry_after: Option<u64> },
}

/// What a failed request answers: its error, the envelope's one member.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// An error as the envelope gives it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    /// `unauthorized`, `forbidden`, `not_found`, `validation`, `rate_limited` or `internal`.
    #[schema(value_type = kinds::ErrorCode)]
    pub code: &'static str,
    /// What went wrong, in the request's language.
    #[schema(examples("Disa fusha nuk janë në rregull."))]
    pub message: String,
    /// A validation error's fields, each with its message; on other errors, none (left out).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<&'static str, String>>,
    /// For a request that came too soon, in how many seconds to try again, where that is known
    /// (`Retry-After` says it too); otherwise none (left out).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(examples(42))]
    pub retry_after: Option<u64>,
}

impl ApiError {
    /// Every code the envelope says, one for each kind of error above (`Said` says its error's),
    /// in their order: the OpenAPI document's `ErrorCode`.
    pub const CODES: [&'static str; 6] = ["unauthorized", "forbidden", "not_found", "validation", "rate_limited", "internal"];

    /// The envelope's code for the error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::Validation(_) => "validation",
            Self::RateLimited => "rate_limited",
            Self::Internal(_) => "internal",
            Self::Said { error, .. } => error.code(),
        }
    }

    /// The HTTP status the error answers with.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Said { error, .. } => error.status(),
        }
    }

    /// The error said in `message`, already in the request's language, rather than in its code's
    /// words: a sign-in code's `ApiError::NotFound` saying that it expired.
    pub fn said(self, message: impl Into<String>) -> Self {
        match self {
            Self::Said { error, retry_after, .. } => Self::Said { error, message: message.into(), retry_after },
            error => Self::Said { error: Box::new(error), message: message.into(), retry_after: None },
        }
    }

    /// The error with in how many seconds to try again: `ApiError::RateLimited.retry_after(42)`.
    pub fn retry_after(self, seconds: u64) -> Self {
        match self {
            Self::Said { error, message, .. } => Self::Said { error, message, retry_after: Some(seconds) },
            error => {
                let message = error.message();
                Self::Said { error: Box::new(error), message, retry_after: Some(seconds) }
            }
        }
    }

    /// The error itself, under the words it is said in.
    fn plain(&self) -> &Self {
        match self {
            Self::Said { error, .. } => error.plain(),
            error => error,
        }
    }

    /// What the error says, in the language now: its own words, or its code's.
    fn message(&self) -> String {
        let message = match self {
            Self::Unauthorized => t("Hyr në llogari për të vazhduar."),
            Self::Forbidden => t("Nuk të lejohet ta bësh këtë."),
            Self::NotFound => t("Ajo që kërkove nuk u gjet."),
            Self::Validation(_) => t("Disa fusha nuk janë në rregull."),
            Self::RateLimited => t("Shumë kërkesa njëra pas tjetrës. Provo përsëri pas pak."),
            Self::Internal(_) => t("Diçka shkoi keq. Provo përsëri."),
            Self::Said { message, .. } => message.as_str(),
        };
        message.to_string()
    }

    /// The envelope, its message in the language now.
    pub fn envelope(&self) -> ErrorEnvelope {
        let fields = match self.plain() {
            Self::Validation(fields) => {
                let mut first = BTreeMap::new();
                for (field, message) in fields {
                    first.entry(*field).or_insert_with(|| message.clone());
                }
                Some(first)
            }
            _ => None,
        };
        let retry_after = match self {
            Self::Said { retry_after, .. } => *retry_after,
            _ => None,
        };
        ErrorEnvelope { error: ErrorBody { code: self.code(), message: self.message(), fields, retry_after } }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(error) = self.plain() {
            tracing::error!(error = ?error, "API request failed");
        }
        let envelope = self.envelope();
        let retry_after = envelope.error.retry_after;
        let mut response = (self.status(), Json(envelope)).into_response();
        if matches!(self.plain(), Self::Unauthorized) {
            // RFC 6750: a 401 names the scheme that would sign the request in.
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        }
        if let Some(seconds) = retry_after {
            // RFC 9110: when to try again, as the envelope says it, for clients that read headers.
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

/// A JSON body axum couldn't read (`JsonBody`): a validation error on `body`, which says why.
impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        tracing::debug!(%rejection, "a request's JSON body was refused");
        let why = match rejection {
            JsonRejection::JsonSyntaxError(_) => t("Trupi i kërkesës nuk është JSON i vlefshëm."),
            JsonRejection::MissingJsonContentType(_) => {
                t("Trupi i kërkesës duhet të dërgohet si JSON, me Content-Type: application/json.")
            }
            JsonRejection::JsonDataError(_) => {
                t("Trupi i kërkesës nuk ka atë që duhet: i mungon një fushë, ose një fushë nuk është e llojit të duhur.")
            }
            // Its bytes couldn't be read: cut off, or over the size requests may be.
            _ => t("Trupi i kërkesës nuk u lexua dot."),
        };
        Self::Validation(vec![("body", why.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_error_has_its_code_and_status_and_only_validation_its_fields() {
        let errors = [
            (ApiError::Unauthorized, "unauthorized", 401),
            (ApiError::Forbidden, "forbidden", 403),
            (ApiError::NotFound, "not_found", 404),
            (ApiError::Validation(vec![("title", "a".into()), ("price", "b".into()), ("title", "c".into())]), "validation", 422),
            (ApiError::RateLimited, "rate_limited", 429),
            (ApiError::Internal(anyhow::anyhow!("the database went away at 10.0.0.5")), "internal", 500),
        ];
        // The codes the document lists (`ErrorCode`), one for each kind of error, in its order.
        assert_eq!(errors.each_ref().map(|(error, ..)| error.code()), ApiError::CODES);
        for (error, code, status) in errors {
            let envelope = error.envelope();
            assert_eq!((envelope.error.code, error.status().as_u16()), (code, status));
            match &error {
                ApiError::Validation(_) => {
                    let fields = envelope.error.fields.expect("a validation error's fields");
                    // A field's first message.
                    assert_eq!(fields.into_iter().collect::<Vec<_>>(), [("price", "b".to_string()), ("title", "a".to_string())]);
                }
                _ => assert!(envelope.error.fields.is_none(), "{code}"),
            }
            assert_eq!(envelope.error.retry_after, None, "{code}");
            // An error's details stay in the log.
            assert!(!envelope.error.message.contains("10.0.0.5"));
        }
    }

    #[test]
    fn an_error_said_in_its_own_words_keeps_its_code_status_and_fields_and_may_say_when_to_try_again() {
        let gone = ApiError::NotFound.said("Kodi skadoi.");
        let envelope = gone.envelope();
        assert_eq!((envelope.error.code, gone.status().as_u16(), envelope.error.message.as_str()), ("not_found", 404, "Kodi skadoi."));
        assert_eq!((envelope.error.fields, envelope.error.retry_after), (None, None));

        let wrong = ApiError::Validation(vec![("code", "Gabim.".into())]).said("Gabim.");
        let envelope = wrong.envelope();
        assert_eq!((envelope.error.code, envelope.error.message.as_str()), ("validation", "Gabim."));
        assert_eq!(envelope.error.fields.unwrap().get("code").map(String::as_str), Some("Gabim."));

        // Either way round: the words and the wait kept both.
        for soon in [ApiError::RateLimited.said("Prit.").retry_after(42), ApiError::RateLimited.retry_after(42).said("Prit.")] {
            let envelope = soon.envelope();
            assert_eq!((envelope.error.code, soon.status().as_u16()), ("rate_limited", 429));
            assert_eq!((envelope.error.message.as_str(), envelope.error.retry_after), ("Prit.", Some(42)));
            let response = soon.into_response();
            assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "42");
        }
        // A wait alone keeps the code's own words.
        let envelope = ApiError::RateLimited.retry_after(5).envelope();
        assert_eq!(envelope.error.message, "Shumë kërkesa njëra pas tjetrës. Provo përsëri pas pak.");

        // A 401 in words of its own still names the scheme; a 403 doesn't.
        let response = ApiError::Unauthorized.said("Hyr.").into_response();
        assert_eq!(response.headers().get(header::WWW_AUTHENTICATE).unwrap(), "Bearer");
        let response = ApiError::Forbidden.said("Kjo llogari është pezulluar.").into_response();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(response.headers().get(header::WWW_AUTHENTICATE).is_none());
        assert!(response.headers().get(header::RETRY_AFTER).is_none());
    }
}
