//! Trust and safety, as the phone app does it, by what the pages do it by (pluribus-web's
//! safety.rs, db's safety.rs): the reasons a listing can be reported for (`GET /report/reasons`);
//! reporting one (`POST /listings/{no}/report`), signed in, as the pages take reports from accounts
//! alone, so that a stranger can't take someone else's listing off the site: the third person to
//! report it for a serious reason (db::HIDE_AFTER, db::SERIOUS_REASONS) takes it off the site until
//! a moderator decides; and blocking a seller, or letting them be again
//! (`PUT` and `DELETE /sellers/{user_id}/blocked`), whose listings then leave every list of the
//! user's (browse.rs, searches.rs, by db::blocked_for_session) and who can no longer write to them.
//! The shapes they answer in are here, beside their handlers.

use axum::{
    extract::{Path, State, rejection::PathRejection},
    http::StatusCode,
};
use pluribus_core::i18n::{self, t};
use pluribus_db::{self as db, Reported};
use pluribus_services::AppState;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ApiError, ApiUser, JsonBody, reference::Cached};

/// The reasons a listing can be reported for, as the pages' report form offers them
/// (pluribus-web's safety.rs, REASONS): each code db::REASONS has, in its order, with its name and
/// what it covers, in Albanian, whose English the catalogue has (i18n/en/listing.txt).
const REASONS: [(&str, &str, &str); 7] = [
    ("SCAM", "Mashtrim", "Kërkon para përpara, ose çmimi duket shumë i mirë për të qenë i vërtetë."),
    ("FAKE", "Njoftim i rremë", "Sendi nuk ekziston, ose fotot janë marrë diku tjetër."),
    ("PROHIBITED", "I ndaluar ose fyes", "Diçka që nuk lejohet të shitet, ose përmbajtje fyese."),
    ("SPAM", "Spam ose i përsëritur", "I njëjti njoftim disa herë, ose reklamë."),
    ("UNAVAILABLE", "Nuk është më në dispozicion", "Është shitur ose është dhënë me qira."),
    ("WRONG_DETAILS", "Të dhëna të gabuara", "Kategoria, çmimi ose vendi nuk janë të sakta."),
    ("OTHER", "Tjetër", "Shkruaj më poshtë çfarë nuk shkon."),
];

/// The longest a report's note may be, in characters, as the pages' form takes it.
const NOTE_MOST: usize = 1000;

/// A reason a listing can be reported for (`GET /report/reasons`).
#[derive(Debug, Serialize)]
pub struct ReasonDto {
    /// What a report says: SCAM, FAKE, PROHIBITED, SPAM, UNAVAILABLE, WRONG_DETAILS or OTHER.
    pub code: &'static str,
    /// Its name, in Albanian and as an English page says it: "Mashtrim", "Scam".
    pub label: &'static str,
    pub label_en: &'static str,
    /// What it covers, which the form says under its name, likewise.
    pub hint: &'static str,
    pub hint_en: &'static str,
    /// Whether it counts towards taking the listing off the site: three people reporting it for one
    /// of these do (SCAM, FAKE, PROHIBITED, SPAM); the others are for a moderator to look at.
    pub serious: bool,
}

/// `GET /report/reasons`: the reasons, in the form's order, each in both languages, as the
/// reference data are, so that the app changes language without asking again; kept five minutes
/// (`Cached`), for they change only with the code.
pub async fn reasons() -> Cached<Vec<ReasonDto>> {
    let english = |text: &'static str| i18n::in_english(text).unwrap_or(text);
    Cached(
        REASONS
            .into_iter()
            .map(|(code, label, hint)| ReasonDto {
                code,
                label,
                label_en: english(label),
                hint,
                hint_en: english(hint),
                serious: db::SERIOUS_REASONS.contains(&code),
            })
            .collect(),
    )
}

/// What `POST /listings/{no}/report` takes: one of the reasons' codes, and what the reporter adds,
/// which OTHER needs.
#[derive(Debug, Deserialize)]
pub struct ReportBody {
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub note: Option<String>,
}

/// `POST /listings/{no}/report`: reports the listing for the moderators (db::report_listing), once a
/// person until a moderator decides, however often; the report that makes three people giving a
/// serious reason takes it off the site until a moderator decides. As the pages' form checks it: a
/// reason that isn't one is a validation error on `reason`, OTHER without a note or a note over
/// NOTE_MOST characters one on `note`; past db::REPORTS_PER_DAY reports a day, `rate_limited`,
/// said as the page says it. A listing that isn't on the site, or is the reporter's own, is not
/// found.
pub async fn report(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    no: Result<Path<i64>, PathRejection>,
    JsonBody(body): JsonBody<ReportBody>,
) -> Result<StatusCode, ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    if db::report_target(&state.db, no).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    let note = body.note.as_deref().map(str::trim).filter(|note| !note.is_empty());
    let mut wrong = Vec::new();
    if !db::REASONS.contains(&body.reason.as_str()) {
        wrong.push(("reason", t("Zgjidh çfarë nuk shkon me njoftimin.").to_string()));
    }
    if body.reason == "OTHER" && note.is_none() {
        wrong.push(("note", t("Shkruaj shkurt çfarë nuk shkon me njoftimin.").to_string()));
    }
    if note.is_some_and(|note| note.chars().count() > NOTE_MOST) {
        wrong.push(("note", t("Shpjegimi mund të ketë deri në 1 000 shkronja.").to_string()));
    }
    if !wrong.is_empty() {
        return Err(ApiError::Validation(wrong));
    }
    if db::reports_today(&state.db, user.id).await? >= db::REPORTS_PER_DAY {
        return Err(ApiError::RateLimited.said(t("Ke dërguar shumë raportime sot. Provo përsëri nesër.")));
    }
    match db::report_listing(&state.db, no, user.id, &body.reason, note).await? {
        Reported::NotReportable => Err(ApiError::NotFound),
        Reported::Taken { hidden } => {
            if hidden {
                tracing::info!(listing = no, "reports took a listing off the site");
            }
            Ok(StatusCode::NO_CONTENT)
        }
        Reported::Already => Ok(StatusCode::NO_CONTENT),
    }
}

/// `PUT /sellers/{user_id}/blocked`: blocks the account for the user, as a listing's report page
/// and a conversation block it (db::set_user_blocked): its listings leave the user's lists, and
/// neither can write to the other. The account is the one behind a listing, the person who posted
/// it, as the pages block it: for a store's listing, the one of its people who posted it, not the
/// store. One that isn't on the site, or the user's own, is not found.
pub async fn block(State(state): State<AppState>, ApiUser(user): ApiUser, id: Result<Path<Uuid>, PathRejection>) -> Result<StatusCode, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    if id == user.id || db::user_profile(&state.db, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    db::set_user_blocked(&state.db, user.id, id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /sellers/{user_id}/blocked`: lets the account be again, as "Cilësimet" does
/// (db::unblock), whichever way it was blocked; one not blocked stays so.
pub async fn unblock(State(state): State<AppState>, ApiUser(user): ApiUser, id: Result<Path<Uuid>, PathRejection>) -> Result<StatusCode, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    db::unblock(&state.db, user.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reasons are the database's, in its order, and each name and what it covers is the
    /// pages' own (the catalogue has their English, as it has the form's).
    #[test]
    fn the_reasons_are_the_databases_with_the_forms_words() {
        assert_eq!(REASONS.map(|(code, _, _)| code), db::REASONS);
        for (code, label, hint) in REASONS {
            assert!(i18n::in_english(label).is_some(), "{code}: {label}");
            assert!(i18n::in_english(hint).is_some(), "{code}: {hint}");
        }
    }
}
