//! The shapes the API answers in, made from the database's rows and never the rows themselves, so
//! that a column added or renamed changes no answer unless one of these says so: a page of
//! anything, with where the next starts (`Page`), the health check's (`HealthDto`), and the
//! signed-in user (`MeDto`). Times are RFC 3339 (`time::serde::rfc3339`), with their offset.

use pluribus_db::{AccountSettings, SessionUser};
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// A page of a list, and where the next one starts: `next`, which the app sends back as `after`
/// (in the same order), or null on the last page.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

/// `GET /health`: the server and its database answer.
#[derive(Debug, Serialize)]
pub struct HealthDto {
    pub ok: bool,
}

/// The signed-in user (`GET /me`): who they are, from their session (`db::SessionUser`), and their
/// language and since when they have had an account, from the account (`db::account_settings`).
#[derive(Debug, Serialize)]
pub struct MeDto {
    pub id: Uuid,
    /// The name they show by, as they gave it ("Përdorues" until they give one).
    pub display_name: String,
    /// Their phone number, as +355691234567, if their account has one.
    pub phone: Option<String>,
    /// Their e-mail address, if their account has one.
    pub email: Option<String>,
    /// The language their e-mails are in: "sq" or "en".
    pub preferred_language: String,
    /// When they made their account.
    #[serde(with = "time::serde::rfc3339")]
    pub since: OffsetDateTime,
}

impl From<(&SessionUser, &AccountSettings)> for MeDto {
    fn from((user, account): (&SessionUser, &AccountSettings)) -> Self {
        Self {
            id: user.id,
            display_name: user.display_name.clone(),
            phone: user.phone.clone(),
            email: user.email.clone(),
            preferred_language: account.preferred_language.clone(),
            since: account.created_at,
        }
    }
}
