//! The moderators' page, /moderim: the makes, models and brands sellers typed in when a list
//! lacked theirs ("Tjetër", properties.rs). A moderator approves one, its spelling put right, and
//! the lists and filters offer it; or merges it into the listed one it is, spelt another way.
//! Only for the moderators .env names (MODERATORS); anyone else finds nothing here.

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

use super::{Result, account::current_user, filters::Choice, not_found, properties, render};
use crate::{
    AppState,
    db::{self, Approval, MergeTarget, PendingOption, SessionUser},
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

#[derive(Template)]
#[template(path = "moderation.html")]
struct ModerationPage {
    rows: Vec<PendingRow>,
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
    render(ModerationPage { rows: rows(&state, None).await? })
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
            let page = ModerationPage { rows: rows(&state, Some((id, message))).await? };
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
        let page = ModerationPage { rows: rows(&state, Some((id, NO_TARGET.to_string()))).await? };
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response());
    };
    if db::merge_option(&state.db, id, into).await? {
        tracing::info!(moderator = %user.id, option = %id, %into, "merged a typed option");
    }
    Ok(Redirect::to("/moderim").into_response())
}
