//! Trust and safety (db/safety.rs): reporting a listing (/listing/{number}/raporto, report.html),
//! blocking the seller behind one and letting them be again, and the accounts a visitor blocked,
//! which the pages of listings leave out for them.

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    Result,
    account::{current_user, session_hash},
    not_found, render, saved,
};
use crate::{
    AppState,
    db::{self, ListingCard, Reported, ReportTarget},
    i18n::t,
};

/// A reason a listing can be reported for, as the form offers it.
pub struct Reason {
    pub code: &'static str,
    pub label: &'static str,
    /// What it covers, under its name.
    pub hint: &'static str,
}

/// The reasons, in db::REASONS's order; the first four take a listing off the site when enough
/// people give them (db::SERIOUS_REASONS). Their names and what they cover in Albanian: the form
/// says them in its language (report.html, crate::t), i18n/en/listing.txt has each; /moderim says
/// them as they are (reason_label).
pub const REASONS: [Reason; 7] = [
    Reason { code: "SCAM", label: "Mashtrim", hint: "Kërkon para përpara, ose çmimi duket shumë i mirë për të qenë i vërtetë." },
    Reason { code: "FAKE", label: "Njoftim i rremë", hint: "Sendi nuk ekziston, ose fotot janë marrë diku tjetër." },
    Reason { code: "PROHIBITED", label: "I ndaluar ose fyes", hint: "Diçka që nuk lejohet të shitet, ose përmbajtje fyese." },
    Reason { code: "SPAM", label: "Spam ose i përsëritur", hint: "I njëjti njoftim disa herë, ose reklamë." },
    Reason { code: "UNAVAILABLE", label: "Nuk është më në dispozicion", hint: "Është shitur ose është dhënë me qira." },
    Reason { code: "WRONG_DETAILS", label: "Të dhëna të gabuara", hint: "Kategoria, çmimi ose vendi nuk janë të sakta." },
    Reason { code: "OTHER", label: "Tjetër", hint: "Shkruaj më poshtë çfarë nuk shkon." },
];

/// A reason's name, by its code (/moderim).
pub fn reason_label(code: &str) -> &str {
    REASONS.iter().find(|reason| reason.code == code).map_or(code, |reason| reason.label)
}

/// The longest a report's explanation may be, in characters.
const DETAILS_MAX: usize = 1000;

/// Where the report page stands for the visitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportState {
    /// Not signed in: reports from accounts alone take a listing off the site, so that a stranger
    /// can't do it to someone else's.
    SignIn,
    Form,
    /// Reported, and waiting for the moderators.
    Done,
}

#[derive(Template)]
#[template(path = "report.html")]
struct ReportPage {
    public_no: i64,
    title: String,
    url: String,
    state: ReportState,
    reasons: &'static [Reason],
    /// The form as sent, when it comes back with what to put right.
    reason: String,
    details: String,
    error: Option<&'static str>,
    /// The visitor blocked the seller already: the thanks say so, in place of offering to.
    blocked: bool,
}

impl ReportPage {
    fn new(target: &ReportTarget, state: ReportState) -> Self {
        Self {
            public_no: target.public_no,
            title: target.title.clone(),
            url: format!("/listing/{}-{}", target.public_no, target.slug),
            state,
            reasons: &REASONS,
            reason: String::new(),
            details: String::new(),
            error: None,
            blocked: false,
        }
    }
}

/// GET /listing/{number}/raporto: what is wrong with the listing, for the moderators, from someone
/// signed in, whom it thanks once sent, offering to block the seller too (`block`), or saying that
/// they are blocked. Someone who isn't is asked to sign in first, and comes back here after
/// (saved::back_after_sign_in). Not for the listing's own seller, nor a listing that isn't on the
/// site, unless to thank whoever reported it.
pub async fn report_page(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    let Some(target) = db::report_target(&state.db, public_no).await? else {
        return not_found().await;
    };
    let user = current_user(&state, &headers).await?;
    if user.as_ref().is_some_and(|user| user.id == target.created_by_user_id) {
        return not_found().await;
    }
    let reported = match &user {
        Some(user) => db::has_reported(&state.db, public_no, user.id).await?,
        None => false,
    };
    if !reported && !target.reportable() {
        return not_found().await;
    }
    let page_state = match (&user, reported) {
        (None, _) => ReportState::SignIn,
        (Some(_), true) => ReportState::Done,
        (Some(_), false) => ReportState::Form,
    };
    let blocked =
        page_state == ReportState::Done && hidden_accounts(&state, &headers).await?.contains(&target.created_by_user_id);
    let mut response = render(ReportPage { blocked, ..ReportPage::new(&target, page_state) })?;
    if user.is_none() {
        let back = saved::back_after_sign_in(&headers, &format!("/listing/{public_no}/raporto"));
        response.headers_mut().append(header::SET_COOKIE, back);
    }
    Ok(response)
}

#[derive(Deserialize)]
pub struct ReportForm {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    details: String,
}

/// POST /listing/{number}/raporto: keeps the report (db::report_listing), which may take the
/// listing off the site, and goes back to the page, which thanks the visitor.
pub async fn report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Form(form): Form<ReportForm>,
) -> Result<Response> {
    let back = Redirect::to(&format!("/listing/{public_no}/raporto")).into_response();
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(back);
    };
    let Some(target) = db::report_target(&state.db, public_no).await? else {
        return not_found().await;
    };
    let details = form.details.trim();
    let error = if !db::REASONS.contains(&form.reason.as_str()) {
        Some(t("Zgjidh çfarë nuk shkon me njoftimin."))
    } else if form.reason == "OTHER" && details.is_empty() {
        Some(t("Shkruaj shkurt çfarë nuk shkon me njoftimin."))
    } else if details.chars().count() > DETAILS_MAX {
        Some(t("Shpjegimi mund të ketë deri në 1 000 shkronja."))
    } else if db::reports_today(&state.db, user.id).await? >= db::REPORTS_PER_DAY {
        Some(t("Ke dërguar shumë raportime sot. Provo përsëri nesër."))
    } else {
        None
    };
    if let Some(error) = error {
        let page = ReportPage {
            reason: form.reason.clone(),
            details: form.details.clone(),
            error: Some(error),
            ..ReportPage::new(&target, ReportState::Form)
        };
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response());
    }
    let details = (!details.is_empty()).then_some(details);
    match db::report_listing(&state.db, public_no, user.id, &form.reason, details).await? {
        Reported::NotReportable => not_found().await,
        Reported::Taken { hidden } => {
            if hidden {
                tracing::info!(listing = public_no, "reports took a listing off the site");
            }
            Ok(back)
        }
        Reported::Already => Ok(back),
    }
}

#[derive(Deserialize)]
pub struct BlockForm {
    #[serde(default)]
    blloko: String,
}

/// POST /listing/{number}/blloko: blocks the seller behind the listing for the signed-in user
/// (blloko=1), or lets them be again (blloko=0), from the report page's thanks, which it goes back
/// to, and which say so then.
pub async fn block(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Form(form): Form<BlockForm>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    if !db::set_blocked(&state.db, user.id, public_no, form.blloko != "0").await? {
        return not_found().await;
    }
    Ok(Redirect::to(&format!("/listing/{public_no}/raporto")).into_response())
}

/// POST /llogaria/zhblloko/{id}: lets someone the user blocked be again, from "Cilësimet",
/// whichever page they were blocked on.
pub async fn unblock(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<Uuid>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    db::unblock(&state.db, user.id, id).await?;
    Ok(Redirect::to("/llogaria/cilesimet?u=zhbllokuar").into_response())
}

/// The accounts the visitor blocked, whose listings the pages leave out for them; none, without
/// asking the database, for someone not signed in. A database result, to go with the pages' other
/// queries (tokio::try_join!).
pub async fn hidden_accounts(state: &AppState, headers: &HeaderMap) -> sqlx::Result<Vec<Uuid>> {
    match session_hash(headers) {
        Some(hash) => db::blocked_for_session(&state.db, &hash).await,
        None => Ok(Vec::new()),
    }
}

/// Leaves out of a row of cards those of the accounts the visitor blocked.
pub fn without_hidden(cards: &mut Vec<ListingCard>, hidden: &[Uuid]) {
    if !hidden.is_empty() {
        cards.retain(|card| !hidden.contains(&card.created_by_user_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_form_offers_every_reason_the_database_knows() {
        assert_eq!(REASONS.map(|reason| reason.code), db::REASONS);
    }
}
