//! The listings a signed-in visitor keeps (the hearts).

use super::*;
use crate::domain::Status;

/// Keeps the listing numbered `public_no` for the user (`keep`), or lets it go. Only an active
/// listing can be kept; one kept already stays as it was. False when there is no such listing to
/// keep or let go.
pub async fn set_saved(db: &PgPool, user_id: Uuid, public_no: i64, keep: bool) -> sqlx::Result<bool> {
    let listing: Option<(Uuid, String)> = sqlx::query_as("SELECT id, status::text FROM listings WHERE public_no = $1")
        .bind(public_no)
        .fetch_optional(db)
        .await?;
    let Some((listing_id, status)) = listing else {
        return Ok(false);
    };
    if keep {
        if Status::parse(&status) != Some(Status::Active) {
            return Ok(false);
        }
        sqlx::query("INSERT INTO saved_listings (user_id, listing_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(listing_id)
            .execute(db)
            .await?;
    } else {
        sqlx::query("DELETE FROM saved_listings WHERE user_id = $1 AND listing_id = $2")
            .bind(user_id)
            .bind(listing_id)
            .execute(db)
            .await?;
    }
    Ok(true)
}

/// The numbers of the listings the user keeps.
pub async fn saved_numbers(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<i64>> {
    sqlx::query_scalar(
        "SELECT l.public_no FROM saved_listings s JOIN listings l ON l.id = s.listing_id WHERE s.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// The cards of the listings the user keeps that anyone can still open, the last kept first: the
/// active ones, and those reserved or sold since, which their cards say (ListingCard::status). At
/// most `limit`.
pub async fn saved_cards(db: &PgPool, user_id: Uuid, limit: i64) -> sqlx::Result<Vec<ListingCard>> {
    let mut query = QueryBuilder::<Postgres>::new(CARD_COLUMNS);
    query.push(
        "NULL::float8 AS distance_m
         FROM (SELECT l.*, s.saved_at
               FROM saved_listings s
               JOIN listings l ON l.id = s.listing_id
               WHERE s.user_id = ",
    );
    query.push_bind(user_id);
    query.push(" AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD') ORDER BY s.saved_at DESC, l.id DESC LIMIT ");
    query.push_bind(limit);
    query.push(") l LEFT JOIN locations loc ON loc.id = l.location_id ORDER BY l.saved_at DESC, l.id DESC");
    query.build_query_as().fetch_all(db).await
}

/// A listing the user keeps that no one can open any more: its seller took it down for a while
/// or for good, or the reports or the moderators did. /te-ruajturat names it, to be let go.
#[derive(Debug, sqlx::FromRow)]
pub struct GoneListing {
    pub public_no: i64,
    pub title: String,
    pub status: String,
}

/// The listings the user keeps that are no longer on the site, the last kept first.
pub async fn saved_gone(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<GoneListing>> {
    sqlx::query_as(
        "SELECT l.public_no, coalesce(l.generated_title, l.title) AS title, l.status::text AS status
         FROM saved_listings s
         JOIN listings l ON l.id = s.listing_id
         WHERE s.user_id = $1 AND l.status NOT IN ('ACTIVE', 'RESERVED', 'SOLD')
         ORDER BY s.saved_at DESC, l.id DESC
         LIMIT 50",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}
