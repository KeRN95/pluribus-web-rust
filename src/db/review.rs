//! The moderators' review (web/moderation.rs): listings that wait before they go up
//! (PENDING_REVIEW, listings.review_reason, migration 0037), approved or refused; the photos a new
//! listing shares with another seller's, the very same files (listing_media.sha256); and accounts
//! suspended, their listings and stores off the site until they are restored as they were.

use super::*;

/// The status history's reason for listings an account's suspension takes off the site.
const SUSPENDED_WITH_ACCOUNT: &str = "Llogaria u pezullua";

/// Whether any of these photos (by their files' SHA-256) is on a listing of another seller than
/// `seller` (a store, or a private seller), one that went up and isn't deleted: a copy waiting, or
/// refused before it went up, holds up nobody's own photo.
pub async fn photos_elsewhere(db: &PgPool, hashes: &[Vec<u8>], seller: Uuid) -> sqlx::Result<bool> {
    if hashes.is_empty() {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM listing_media m JOIN listings l ON l.id = m.listing_id
                        WHERE m.sha256 = ANY($1) AND l.published_at IS NOT NULL AND l.status <> 'DELETED'
                          AND coalesce(l.seller_business_id, l.seller_user_id) <> $2)",
    )
    .bind(hashes)
    .bind(seller)
    .fetch_one(db)
    .await
}

/// A listing waiting for a moderator, as /moderim shows it.
#[derive(Debug, sqlx::FromRow)]
pub struct PendingListing {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub photo: Option<String>,
    pub seller_name: String,
    pub seller_url: String,
    /// The account that posted it, which a moderator may suspend.
    pub created_by_user_id: Uuid,
    pub reason: String,
    pub created_at: OffsetDateTime,
    /// The other sellers' listings with one of its photos, by number and address.
    pub twins: Vec<String>,
}

/// The listings waiting for a moderator, the oldest first; at most 100.
pub async fn pending_listings(db: &PgPool) -> sqlx::Result<Vec<PendingListing>> {
    sqlx::query_as(
        "SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title,
                (SELECT coalesce(m.thumbnail_url, m.public_url) FROM listing_media m
                 WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                 ORDER BY m.is_cover DESC, m.sort_order LIMIT 1) AS photo,
                coalesce(b.display_name, u.display_name, '') AS seller_name,
                CASE WHEN b.id IS NOT NULL THEN '/biznes/' || b.slug ELSE '/perdorues/' || l.seller_user_id END AS seller_url,
                l.created_by_user_id, coalesce(l.review_reason, '') AS reason, l.created_at,
                coalesce((SELECT array_agg(DISTINCT '/listing/' || o.public_no || '-' || o.slug)
                          FROM listing_media m
                          JOIN listing_media om ON om.sha256 = m.sha256 AND om.listing_id <> m.listing_id
                          JOIN listings o ON o.id = om.listing_id AND o.published_at IS NOT NULL AND o.status <> 'DELETED'
                          WHERE m.listing_id = l.id AND m.sha256 IS NOT NULL
                            AND coalesce(o.seller_business_id, o.seller_user_id) <> coalesce(l.seller_business_id, l.seller_user_id)),
                         '{}') AS twins
         FROM listings l
         LEFT JOIN users u ON u.id = l.seller_user_id
         LEFT JOIN businesses b ON b.id = l.seller_business_id
         WHERE l.status = 'PENDING_REVIEW'
         ORDER BY l.created_at
         LIMIT 100",
    )
    .fetch_all(db)
    .await
}

/// How many listings wait for a moderator, for the link to /moderim.
pub async fn pending_listing_count(db: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar("SELECT count(*) FROM listings WHERE status = 'PENDING_REVIEW'").fetch_one(db).await
}

/// A moderator puts a waiting listing up: ACTIVE from now, for LISTING_DAYS, its reason gone; the
/// status history and moderation_actions say who. False when it no longer waits.
pub async fn approve_listing(db: &PgPool, public_no: i64, moderator: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    moderator_says(&mut tx, moderator, "Miratuar nga moderatorët").await?;
    let approved: Option<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE listings SET status = 'ACTIVE', review_reason = NULL, published_at = now(),
                             expires_at = now() + make_interval(days => $2), updated_at = now()
         WHERE public_no = $1 AND status = 'PENDING_REVIEW'
         RETURNING id, created_by_user_id",
    )
    .bind(public_no)
    .bind(LISTING_DAYS)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, owner)) = approved else {
        return Ok(false);
    };
    record(&mut tx, moderator, Some(id), owner, "APPROVE_LISTING", "review", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(true)
}

/// A moderator refuses a waiting listing: off the site for good (REJECTED), with the reason its
/// seller sees (listings.rejection_reason). False when it no longer waits.
pub async fn reject_listing(db: &PgPool, public_no: i64, moderator: Uuid, reason: &str) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    moderator_says(&mut tx, moderator, "Refuzuar nga moderatorët").await?;
    let refused: Option<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE listings SET status = 'REJECTED', review_reason = NULL, rejection_reason = $2, updated_at = now()
         WHERE public_no = $1 AND status = 'PENDING_REVIEW'
         RETURNING id, created_by_user_id",
    )
    .bind(public_no)
    .bind(reason)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, owner)) = refused else {
        return Ok(false);
    };
    record(&mut tx, moderator, Some(id), owner, "REJECT_LISTING", reason, serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(true)
}

/// The moderator and why, for the status history the listings' trigger writes.
async fn moderator_says(tx: &mut sqlx::Transaction<'_, Postgres>, moderator: Uuid, reason: &str) -> sqlx::Result<()> {
    sqlx::query("SELECT set_config('app.user_id', $1, true), set_config('app.status_reason', $2, true)")
        .bind(moderator.to_string())
        .bind(reason)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// What a moderator did (moderation_actions).
pub(super) async fn record(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    moderator: Uuid,
    listing: Option<Uuid>,
    target: Uuid,
    action: &str,
    reason: &str,
    metadata: serde_json::Value,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO moderation_actions (moderator_user_id, listing_id, target_user_id, action_type, reason, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(moderator)
    .bind(listing)
    .bind(target)
    .bind(action)
    .bind(reason)
    .bind(Json(metadata))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// The listings an account's suspension takes off the site: its own, and its stores', those up,
/// reserved or waiting for a moderator.
const ITS_LISTINGS: &str = "(l.seller_user_id = $1 OR l.seller_business_id IN (SELECT business_id FROM business_members
                                                                               WHERE user_id = $1 AND role = 'OWNER'))
                            AND l.status IN ('ACTIVE', 'RESERVED', 'PENDING_REVIEW')";

/// A moderator suspends an account: it can't sign in (its sessions end) nor post, and its listings
/// and the stores it owns leave the site; what each listing was is kept (moderation_actions), for
/// restoring it. False when it was suspended already, or is gone.
pub async fn suspend_user(db: &PgPool, user_id: Uuid, moderator: Uuid, reason: &str) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let suspended = sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE id = $1 AND status = 'ACTIVE'")
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if suspended == 0 {
        return Ok(false);
    }
    moderator_says(&mut tx, moderator, SUSPENDED_WITH_ACCOUNT).await?;
    let was: Vec<(Uuid, String)> = sqlx::query_as(&format!(
        "WITH hidden AS (SELECT l.id, l.status FROM listings l WHERE {ITS_LISTINGS} FOR UPDATE)
         UPDATE listings l SET status = 'SUSPENDED', updated_at = now() FROM hidden WHERE l.id = hidden.id
         RETURNING l.id, hidden.status"
    ))
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    let stores: Vec<Uuid> = sqlx::query_scalar(
        "UPDATE businesses SET status = 'SUSPENDED', updated_at = now()
         WHERE status = 'ACTIVE' AND id IN (SELECT business_id FROM business_members WHERE user_id = $1 AND role = 'OWNER')
         RETURNING id",
    )
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    let listings: Vec<serde_json::Value> = was.iter().map(|(id, status)| serde_json::json!([id.to_string(), status])).collect();
    let stores: Vec<String> = stores.iter().map(Uuid::to_string).collect();
    let metadata = serde_json::json!({ "listings": listings, "stores": stores });
    record(&mut tx, moderator, None, user_id, "SUSPEND_USER", reason, metadata).await?;
    tx.commit().await?;
    Ok(true)
}

/// A moderator restores a suspended account: it signs in and posts again, and what its suspension
/// took off the site comes back as it was, those listings still where it left them. False when it
/// isn't suspended.
pub async fn restore_user(db: &PgPool, user_id: Uuid, moderator: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let restored = sqlx::query("UPDATE users SET status = 'ACTIVE' WHERE id = $1 AND status = 'SUSPENDED'")
        .bind(user_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if restored == 0 {
        return Ok(false);
    }
    let metadata: Option<Json<serde_json::Value>> = sqlx::query_scalar(
        "SELECT metadata FROM moderation_actions WHERE target_user_id = $1 AND action_type = 'SUSPEND_USER'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let metadata = metadata.map(|Json(metadata)| metadata).unwrap_or_default();
    moderator_says(&mut tx, moderator, "Llogaria u rikthye").await?;
    for listing in metadata["listings"].as_array().into_iter().flatten() {
        let (Some(id), Some(status)) = (listing[0].as_str().and_then(|id| id.parse::<Uuid>().ok()), listing[1].as_str()) else {
            continue;
        };
        sqlx::query("UPDATE listings SET status = $2, updated_at = now() WHERE id = $1 AND status = 'SUSPENDED'")
            .bind(id)
            .bind(status)
            .execute(&mut *tx)
            .await?;
    }
    for store in metadata["stores"].as_array().into_iter().flatten() {
        if let Some(id) = store.as_str().and_then(|id| id.parse::<Uuid>().ok()) {
            sqlx::query("UPDATE businesses SET status = 'ACTIVE', updated_at = now() WHERE id = $1 AND status = 'SUSPENDED'")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }
    record(&mut tx, moderator, None, user_id, "RESTORE_USER", "", serde_json::json!({})).await?;
    tx.commit().await?;
    Ok(true)
}

/// Whether a listing's seller is suspended, the account it is posted under or the store it is
/// posted as: a moderator's suspension takes their listings off the site with it (suspend_user),
/// which its page says, rather than that it was reported.
pub async fn seller_suspended(db: &PgPool, public_no: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM listings l
                        LEFT JOIN users u ON u.id = l.seller_user_id
                        LEFT JOIN businesses b ON b.id = l.seller_business_id
                        WHERE l.public_no = $1 AND (u.status = 'SUSPENDED' OR b.status = 'SUSPENDED'))",
    )
    .bind(public_no)
    .fetch_one(db)
    .await
}

/// A suspended account, as /moderim lists them.
#[derive(Debug, sqlx::FromRow)]
pub struct SuspendedUser {
    pub id: Uuid,
    pub display_name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    /// Why, and when, as the moderator who suspended it said.
    pub reason: Option<String>,
    pub suspended_at: Option<OffsetDateTime>,
}

/// The suspended accounts, the latest suspended first; at most 100.
pub async fn suspended_users(db: &PgPool) -> sqlx::Result<Vec<SuspendedUser>> {
    sqlx::query_as(
        "SELECT u.id, u.display_name, u.phone_e164::text AS phone, u.email::text AS email, a.reason, a.created_at AS suspended_at
         FROM users u
         LEFT JOIN LATERAL (SELECT m.reason, m.created_at FROM moderation_actions m
                            WHERE m.target_user_id = u.id AND m.action_type = 'SUSPEND_USER'
                            ORDER BY m.created_at DESC LIMIT 1) a ON true
         WHERE u.status = 'SUSPENDED'
         ORDER BY a.created_at DESC NULLS LAST
         LIMIT 100",
    )
    .fetch_all(db)
    .await
}
