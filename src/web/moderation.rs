//! The moderators' page, /moderim. The listings people reported (safety.rs), those the reports
//! took off the site first: a moderator keeps one, which goes back up, or takes it down for good.
//! And the makes, models and brands sellers typed in when a list lacked theirs ("Tjetër",
//! properties.rs): a moderator approves one, its spelling put right, and the lists and filters
//! offer it; or merges it into the listed one it is, spelt another way. The listings that wait
//! before they go up (db::review): approved, they go up; refused, their seller is told why. And
//! accounts: suspended, from a waiting or a reported listing, and restored. Only for the
//! moderators .env names (MODERATORS); anyone else finds nothing here.

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{Result, account::current_user, filters::Choice, not_found, properties, render, safety::reason_label};
use crate::{
    AppState,
    db::{self, Approval, Decision, MergeTarget, PendingOption, ReportedListing, SessionUser},
    format,
};

/// How alike a listed name must be to a typed one (pg_trgm's similarity) to come chosen for the
/// merge: "Mercedes" and Mercedes-Benz.
const LIKELY: f32 = 0.3;

const NO_TARGET: &str = "Zgjidh me cilin emër të listës ta bashkosh.";

/// A typed option waiting, with the listed ones it could be merged into.
pub struct PendingRow {
    pub option: PendingOption,
    pub targets: Vec<Choice>,
    pub error: Option<String>,
}

impl PendingRow {
    /// "1 njoftim", "3 njoftime".
    pub fn listings_text(&self) -> String {
        format::listing_count(self.option.listings, i64::MAX)
    }

    /// Whether a listed name comes chosen for the merge; if none does, the select asks for one.
    pub fn likely_chosen(&self) -> bool {
        self.targets.iter().any(|target| target.selected)
    }

    /// The listings with it: search finds them by its name.
    pub fn search_url(&self) -> String {
        let query: String = form_urlencoded::Serializer::new(String::new()).append_pair("q", &self.option.label).finish();
        format!("/search?{query}")
    }
}

/// Why a moderator refuses a waiting listing, by its code in the form, as its seller reads it.
pub const REFUSALS: [(&str, &str); 4] = [
    ("foto", "Fotot janë marrë nga një njoftim tjetër"),
    ("mashtrim", "Duket mashtrim"),
    ("ndaluar", "Shet diçka që nuk lejohet"),
    ("gabim", "Kategoria ose përshkrimi nuk përputhen me atë që shitet"),
];

/// Why a moderator suspends an account, by its code in the form.
pub const SUSPENSIONS: [(&str, &str); 4] = [
    ("mashtrim", "Mashtrim"),
    ("spam", "Spam"),
    ("ndaluar", "Shet gjëra të ndaluara"),
    ("tjeter", "Tjetër"),
];

/// A listing waiting for a moderator, as /moderim shows it.
impl db::PendingListing {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// Why it waits.
    pub fn why(&self) -> &'static str {
        match self.reason.as_str() {
            db::DUPLICATE_PHOTOS => "Foto të njëjta me njoftimin e një shitësi tjetër",
            _ => "Llogari e re, pa e konfirmuar numrin me kod",
        }
    }

    /// The other sellers' listings with one of its photos: each address, and its number ("nr. 1001").
    pub fn twins_shown(&self) -> Vec<(String, String)> {
        self.twins
            .iter()
            .map(|url| {
                let no = url.trim_start_matches("/listing/").split('-').next().unwrap_or_default();
                (url.clone(), format!("nr. {no}"))
            })
            .collect()
    }

    pub fn when(&self) -> String {
        format::date(self.created_at)
    }
}

/// A suspended account, as /moderim lists it.
impl db::SuspendedUser {
    /// The phone number or address it signs in with.
    pub fn contact(&self) -> String {
        match (&self.phone, &self.email) {
            (Some(phone), _) => format::phone(phone),
            (None, email) => email.clone().unwrap_or_default(),
        }
    }

    /// Why, and since when: "Mashtrim · 10.10.2026".
    pub fn why(&self) -> String {
        let reason = SUSPENSIONS.iter().find(|(code, _)| self.reason.as_deref() == Some(*code)).map_or("", |(_, label)| *label);
        match self.suspended_at {
            Some(at) if reason.is_empty() => format::date(at),
            Some(at) => format!("{reason} · {}", format::date(at)),
            None => reason.to_string(),
        }
    }
}

/// A reported listing, as /moderim shows it.
impl ReportedListing {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// Taken off the site by its reports, until a moderator decides.
    pub fn hidden(&self) -> bool {
        self.status == "SUSPENDED"
    }

    /// "1 person e raportoi", "3 veta e raportuan".
    pub fn reporters_text(&self) -> String {
        match self.reporters {
            1 => "1 person e raportoi".to_string(),
            reporters => format!("{reporters} veta e raportuan"),
        }
    }

    /// When it was last reported.
    pub fn when(&self) -> String {
        format::date(self.last_reported)
    }

    /// The reasons given, by their names, with how many gave each.
    pub fn reasons(&self) -> Vec<(&str, i64)> {
        self.reason_codes.iter().zip(&self.reason_counts).map(|(code, count)| (reason_label(code), *count)).collect()
    }
}

#[derive(Template)]
#[template(path = "moderation.html")]
struct ModerationPage {
    /// The listings that wait before they go up (db::review).
    pending: Vec<db::PendingListing>,
    /// The listings people reported, which wait for a decision.
    reported: Vec<ReportedListing>,
    /// The accounts suspended, to restore.
    suspended: Vec<db::SuspendedUser>,
    rows: Vec<PendingRow>,
    /// How many people reporting a listing take it off the site (db::HIDE_AFTER).
    hide_after: i64,
}

impl ModerationPage {
    /// The page as things stand, with `error` beside the typed option it is about.
    async fn now(state: &AppState, error: Option<(Uuid, String)>) -> Result<Self> {
        let (pending, reported, suspended) =
            tokio::try_join!(db::pending_listings(&state.db), db::reported_listings(&state.db), db::suspended_users(&state.db))?;
        Ok(Self { pending, reported, suspended, rows: rows(state, error).await?, hide_after: db::HIDE_AFTER })
    }
}

/// The signed-in user, if they are a moderator.
async fn moderator(state: &AppState, headers: &HeaderMap) -> Result<Option<SessionUser>> {
    Ok(current_user(state, headers).await?.filter(|user| state.moderators.include(user)))
}

/// What waits, each with its merge targets, the likeliest chosen; `error` goes with the row it is
/// about.
async fn rows(state: &AppState, error: Option<(Uuid, String)>) -> Result<Vec<PendingRow>> {
    let (options, targets) = tokio::try_join!(db::pending_options(&state.db), db::merge_targets(&state.db))?;
    let mut by_option: HashMap<Uuid, Vec<MergeTarget>> = HashMap::new();
    for target in targets {
        by_option.entry(target.pending_id).or_default().push(target);
    }
    Ok(options
        .into_iter()
        .map(|option| {
            let targets = by_option.remove(&option.id).unwrap_or_default();
            // The likeliest: a listed name the typed one starts with ("A3" for "A3 Sportback"),
            // or else the most alike, if alike enough.
            let typed = option.label.to_lowercase();
            let score = |target: &MergeTarget| match typed.starts_with(&format!("{} ", target.label.to_lowercase())) {
                true => 1.0,
                false => target.similarity,
            };
            let likeliest = targets
                .iter()
                .filter(|target| score(target) >= LIKELY)
                .max_by(|a, b| score(a).total_cmp(&score(b)))
                .map(|target| target.id);
            let targets = targets
                .into_iter()
                .map(|target| Choice { selected: Some(target.id) == likeliest, value: target.id.to_string(), label: target.label })
                .collect();
            let error = error.as_ref().filter(|(id, _)| *id == option.id).map(|(_, message)| message.clone());
            PendingRow { option, targets, error }
        })
        .collect())
}

pub async fn page(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if moderator(&state, &headers).await?.is_none() {
        return not_found().await;
    }
    render(ModerationPage::now(&state, None).await?)
}

#[derive(Deserialize)]
pub struct ApproveForm {
    #[serde(default)]
    label: String,
}

pub async fn approve(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<ApproveForm>,
) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    let label = form.label.split_whitespace().collect::<Vec<_>>().join(" ");
    let problem = if !properties::plausible_name(&label) {
        Some(properties::NAME_PROBLEM.to_string())
    } else {
        match db::approve_option(&state.db, id, &label).await? {
            Approval::Approved => {
                tracing::info!(moderator = %user.id, option = %id, label, "approved a typed option");
                None
            }
            Approval::Taken => Some(format!("„{label}“ është tashmë në listë: bashkoje me të.")),
            // Seen to by someone else meanwhile, or its make waits: the page shows how things are.
            Approval::NotPending => None,
        }
    };
    match problem {
        None => Ok(Redirect::to("/moderim").into_response()),
        Some(message) => {
            let page = ModerationPage::now(&state, Some((id, message))).await?;
            Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
        }
    }
}

#[derive(Deserialize)]
pub struct MergeForm {
    #[serde(default)]
    into: String,
}

pub async fn merge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Form(form): Form<MergeForm>,
) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    let Ok(into) = form.into.parse::<Uuid>() else {
        let page = ModerationPage::now(&state, Some((id, NO_TARGET.to_string()))).await?;
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response());
    };
    if db::merge_option(&state.db, id, into).await? {
        tracing::info!(moderator = %user.id, option = %id, %into, "merged a typed option");
    }
    Ok(Redirect::to("/moderim").into_response())
}

/// POST /moderim/njoftim/{number}/mbaje: nothing wrong with the reported listing. Its reports are
/// dismissed, and if they took it off the site, it goes back up as it was.
pub async fn keep(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    decide(&state, &headers, public_no, Decision::Keep).await
}

/// POST /moderim/njoftim/{number}/hiq: the reported listing breaks the rules, and comes off the
/// site for good; its seller sees it on "Njoftimet e mia" as taken down by the moderators.
pub async fn remove(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    decide(&state, &headers, public_no, Decision::Remove).await
}

/// POST /moderim/njoftim/{number}/mirato: a waiting listing goes up.
pub async fn approve_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    if db::approve_listing(&state.db, public_no, user.id).await? {
        tracing::info!(moderator = %user.id, listing = public_no, "approved a waiting listing");
    }
    Ok(Redirect::to("/moderim").into_response())
}

#[derive(Deserialize)]
pub struct ReasonForm {
    #[serde(default)]
    arsye: String,
}

/// POST /moderim/njoftim/{number}/refuzo: a waiting listing comes off the site for good, its
/// seller told why (one of REFUSALS).
pub async fn reject_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Form(form): Form<ReasonForm>,
) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    let Some((_, reason)) = REFUSALS.iter().find(|(code, _)| *code == form.arsye) else {
        return Ok(Redirect::to("/moderim").into_response());
    };
    if db::reject_listing(&state.db, public_no, user.id, reason).await? {
        tracing::info!(moderator = %user.id, listing = public_no, reason, "refused a waiting listing");
    }
    Ok(Redirect::to("/moderim").into_response())
}

/// POST /moderim/perdorues/{id}/pezullo: suspends the account (one of SUSPENSIONS for why), its
/// listings and stores off the site; never the moderator's own.
pub async fn suspend_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<ReasonForm>,
) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    let (Ok(id), Some((code, _))) = (id.parse::<Uuid>(), SUSPENSIONS.iter().find(|(code, _)| *code == form.arsye)) else {
        return Ok(Redirect::to("/moderim").into_response());
    };
    if id != user.id && db::suspend_user(&state.db, id, user.id, code).await? {
        tracing::info!(moderator = %user.id, account = %id, reason = code, "suspended an account");
    }
    Ok(Redirect::to("/moderim").into_response())
}

/// POST /moderim/perdorues/{id}/rikthe: restores a suspended account, and what its suspension
/// took off the site.
pub async fn restore_user(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let Some(user) = moderator(&state, &headers).await? else {
        return not_found().await;
    };
    if let Ok(id) = id.parse::<Uuid>()
        && db::restore_user(&state.db, id, user.id).await?
    {
        tracing::info!(moderator = %user.id, account = %id, "restored an account");
    }
    Ok(Redirect::to("/moderim").into_response())
}

async fn decide(state: &AppState, headers: &HeaderMap, public_no: i64, decision: Decision) -> Result<Response> {
    let Some(user) = moderator(state, headers).await? else {
        return not_found().await;
    };
    // Decided by someone else meanwhile: the page shows how things are.
    if db::decide(&state.db, public_no, user.id, decision).await? {
        tracing::info!(moderator = %user.id, listing = public_no, ?decision, "decided on a reported listing");
    }
    Ok(Redirect::to("/moderim").into_response())
}
