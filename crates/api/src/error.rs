//! Why a request failed, answered in one envelope whatever went wrong:
//! `{"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}}`, and for what
//! was sent wrong, each field's message under the request's own name for it,
//! `{"error": {"code": "validation", "message": "…", "fields": {"body": "…"}}}`. The code is for the
//! app to act on, the same in every language; the messages are for a person to read, in the
//! language the request asks for (lang.rs), from the catalogue (i18n/en/api.txt). An error's
//! details stay in the log, never in the answer.

use std::collections::BTreeMap;

use axum::{
    Json,
    extract::rejection::JsonRejection,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use pluribus_core::t;
use serde::Serialize;

/// Why a request failed.
#[derive(Debug)]
pub enum ApiError {
    /// No bearer token where the route needs one, or one no good session has (wrong, expired, or
    /// its account closed or suspended): 401, with `WWW-Authenticate: Bearer`.
    Unauthorized,
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
}

/// What a failed request answers: its error, the envelope's one member.
#[derive(Debug, Serialize)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

/// An error as the envelope gives it.
#[derive(Debug, Serialize)]
pub struct ErrorBody {
    /// `unauthorized`, `not_found`, `validation`, `rate_limited` or `internal`.
    pub code: &'static str,
    /// What went wrong, in the request's language.
    pub message: String,
    /// A validation error's fields, each with its message; on other errors, none (left out).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<BTreeMap<&'static str, String>>,
}

impl ApiError {
    /// The envelope's code for the error.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Unauthorized => "unauthorized",
            Self::NotFound => "not_found",
            Self::Validation(_) => "validation",
            Self::RateLimited => "rate_limited",
            Self::Internal(_) => "internal",
        }
    }

    /// The HTTP status the error answers with.
    pub fn status(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// The envelope, its message in the language now.
    pub fn envelope(&self) -> ErrorEnvelope {
        let message = match self {
            Self::Unauthorized => t("Hyr në llogari për të vazhduar."),
            Self::NotFound => t("Ajo që kërkove nuk u gjet."),
            Self::Validation(_) => t("Disa fusha nuk janë në rregull."),
            Self::RateLimited => t("Shumë kërkesa njëra pas tjetrës. Provo përsëri pas pak."),
            Self::Internal(_) => t("Diçka shkoi keq. Provo përsëri."),
        };
        let fields = match self {
            Self::Validation(fields) => {
                let mut first = BTreeMap::new();
                for (field, message) in fields {
                    first.entry(*field).or_insert_with(|| message.clone());
                }
                Some(first)
            }
            _ => None,
        };
        ErrorEnvelope { error: ErrorBody { code: self.code(), message: message.to_string(), fields } }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(error) = &self {
            tracing::error!(error = ?error, "API request failed");
        }
        let mut response = (self.status(), Json(self.envelope())).into_response();
        if matches!(self, Self::Unauthorized) {
            // RFC 6750: a 401 names the scheme that would sign the request in.
            response.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
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
            (ApiError::NotFound, "not_found", 404),
            (ApiError::Validation(vec![("title", "a".into()), ("price", "b".into()), ("title", "c".into())]), "validation", 422),
            (ApiError::RateLimited, "rate_limited", 429),
            (ApiError::Internal(anyhow::anyhow!("the database went away at 10.0.0.5")), "internal", 500),
        ];
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
            // An error's details stay in the log.
            assert!(!envelope.error.message.contains("10.0.0.5"));
        }
    }
}
