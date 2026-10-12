//! The listings a signed-in user keeps, as the phone app keeps them, by what the pages keep them by
//! (pluribus-web's saved.rs: the hearts, /te-ruajturat): a listing kept or let go
//! (`PUT` and `DELETE /listings/{no}/saved`, db::set_saved: only one up can be kept), the listings
//! kept with the price drops not seen yet over them and those no longer on the site under them
//! (`GET /saved`), and the drops seen once shown (`POST /saved/seen`, as the page's beacon says it
//! showed them). The shapes they answer in are here, beside their handlers.

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, State, rejection::PathRejection},
    http::{HeaderMap, StatusCode},
};
use pluribus_core::domain::Status;
use pluribus_db::{self as db, GoneListing, PriceDrop};
use pluribus_services::AppState;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody,
    browse::{CardDto, cards_of, origin},
    openapi::{Internal, NotFound, Unauthorized, Validation, kinds},
};

/// The most listings `/saved` gives, the last kept first, as /te-ruajturat shows (pluribus-web's
/// SAVED_MOST).
const SAVED_MOST: i64 = 200;

/// `GET /saved`: what the user keeps, as /te-ruajturat shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct SavedDto {
    /// The listings kept that anyone can still open, the last kept first: those up, and those
    /// reserved or sold since, which their `status` says.
    pub listings: Vec<CardDto>,
    /// The price drops of listings kept that the user hasn't seen yet, the latest first: what the
    /// page shows over the listings, until `POST /saved/seen`.
    pub price_drops: Vec<PriceDropDto>,
    /// The listings kept that no one can open any more, the last kept first, each to let go
    /// (`DELETE /listings/{no}/saved`).
    pub gone: Vec<GoneDto>,
}

/// A listing kept whose price dropped, from what to what (`db::PriceDrop`).
#[derive(Debug, Serialize, ToSchema)]
pub struct PriceDropDto {
    /// The drop's own id, which `POST /saved/seen` takes as `up_to`: the first's, the latest.
    pub id: Uuid,
    /// The listing's number, which its card among `listings` has too.
    #[schema(examples(1001))]
    pub no: i64,
    pub title: String,
    /// What it was when the user last saw it, and what it is, as text ("32000", "29000").
    #[schema(examples("32000"))]
    pub from: Decimal,
    #[schema(examples("29000"))]
    pub to: Decimal,
    /// EUR or ALL.
    #[schema(examples("EUR"))]
    pub currency: String,
    /// When it dropped.
    #[serde(with = "time::serde::rfc3339")]
    pub at: OffsetDateTime,
}

impl From<&PriceDrop> for PriceDropDto {
    fn from(drop: &PriceDrop) -> Self {
        Self {
            id: drop.notice,
            no: drop.public_no,
            title: drop.title.clone(),
            from: drop.was.normalize(),
            to: drop.price.normalize(),
            currency: drop.currency.clone(),
            at: drop.at,
        }
    }
}

/// A listing kept that no one can open any more (`db::GoneListing`): its seller took it down for a
/// while or for good, it expired, or the reports or the moderators took it off.
#[derive(Debug, Serialize, ToSchema)]
pub struct GoneDto {
    #[schema(examples(1001))]
    pub no: i64,
    pub title: String,
    /// Where it stands, as the database spells it: PAUSED, DELETED, EXPIRED, SUSPENDED or REJECTED.
    #[schema(value_type = kinds::Status)]
    pub status: Status,
    /// Why it isn't on the site, as /te-ruajturat says it (`Status::gone_label`), in the language
    /// asked for: "Çaktivizuar nga shitësi", "U fshi", "Skadoi", "Hequr nga faqja".
    #[schema(examples("Çaktivizuar nga shitësi"))]
    pub why: &'static str,
}

impl TryFrom<&GoneListing> for GoneDto {
    type Error = anyhow::Error;

    /// Its status is one the database's CHECK allows, which the domain knows (kinds.rs): one it
    /// didn't would be an error, as on the cards (browse.rs).
    fn try_from(gone: &GoneListing) -> anyhow::Result<Self> {
        let status: Status = gone.status.parse().with_context(|| format!("listing {}: {:?}", gone.public_no, gone.status))?;
        Ok(Self { no: gone.public_no, title: gone.title.clone(), status, why: status.gone_label() })
    }
}

/// `PUT /listings/{no}/saved`: keeps the listing for the user, as its heart does; once, however
/// often. Only a listing up can be kept: one that isn't, or none by that number, is not found.
#[utoipa::path(
    put,
    path = "/listings/{no}/saved",
    tag = "saved",
    operation_id = "keepListing",
    summary = "Keep a listing",
    description = "Keeps the listing for the user, as its heart does, once however often; only a listing up can be kept.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Kept"),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn keep(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    set(&state, user.id, no, true).await
}

/// `DELETE /listings/{no}/saved`: lets the listing go, whatever it became; not found for a number
/// no listing has.
#[utoipa::path(
    delete,
    path = "/listings/{no}/saved",
    tag = "saved",
    operation_id = "letGoListing",
    summary = "Let a kept listing go",
    description = "Lets the listing go, whatever it became.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Let go"),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn let_go(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    set(&state, user.id, no, false).await
}

/// Keeps the listing numbered `no` for the user, or lets it go (db::set_saved): 204, or 404 for a
/// number that isn't one (any other path, "abc") or no listing's to keep or let go.
async fn set(state: &AppState, user: Uuid, no: Result<Path<i64>, PathRejection>, keep: bool) -> Result<StatusCode, ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    match db::set_saved(&state.db, user, no, keep).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(ApiError::NotFound),
    }
}

/// `GET /saved`: the listings the user keeps, the price drops they haven't seen, and those no
/// longer on the site, as /te-ruajturat shows them (db::saved_cards, db::price_drops,
/// db::saved_gone).
#[utoipa::path(
    get,
    path = "/saved",
    tag = "saved",
    operation_id = "getSaved",
    summary = "The listings kept",
    description = "The listings the user keeps, the price drops among them they haven't seen, and those no longer on the site, as /te-ruajturat shows them.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "What the user keeps", body = SavedDto),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn saved(State(state): State<AppState>, ApiUser(user): ApiUser, headers: HeaderMap) -> Result<Json<SavedDto>, ApiError> {
    let (cards, drops, gone) = tokio::try_join!(
        db::saved_cards(&state.db, user.id, SAVED_MOST),
        db::price_drops(&state.db, user.id),
        db::saved_gone(&state.db, user.id),
    )?;
    Ok(Json(SavedDto {
        listings: cards_of(&cards, &origin(&state.config, &headers))?,
        price_drops: drops.iter().map(PriceDropDto::from).collect(),
        gone: gone.iter().map(GoneDto::try_from).collect::<anyhow::Result<_>>()?,
    }))
}

/// What `POST /saved/seen` takes; in the OpenAPI document `SavedSeenBody`, notifications.rs having
/// a `SeenBody` of its own.
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = SavedSeenBody)]
pub struct SeenBody {
    /// The latest price drop shown, by its id (`PriceDropDto::id`).
    pub up_to: Uuid,
}

/// `POST /saved/seen`: the price drops were shown, up to the latest of them (`up_to`): they are
/// seen, and leave `/saved`; one that came after stays, as the page's beacon has it
/// (db::price_drops_seen).
#[utoipa::path(
    post,
    path = "/saved/seen",
    tag = "saved",
    operation_id = "markPriceDropsSeen",
    summary = "The price drops were shown",
    description = "The price drops were shown, up to the latest of them: they are seen and leave `/saved`, one that came after staying.",
    request_body = SeenBody,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Seen"),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn seen(State(state): State<AppState>, ApiUser(user): ApiUser, JsonBody(body): JsonBody<SeenBody>) -> Result<StatusCode, ApiError> {
    db::price_drops_seen(&state.db, user.id, body.up_to).await?;
    Ok(StatusCode::NO_CONTENT)
}
