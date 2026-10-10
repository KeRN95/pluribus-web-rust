//! Listings that expire (posting.rs, LISTING_DAYS): those whose time is up leave the site
//! (EXPIRED), and their sellers hear of it; those a few days from it, their sellers hear first
//! (expiry.rs). Each telling is a notification (EXPIRED, EXPIRING) to the account that posted it,
//! or for a store's listing that someone who left posted, to the store's owner (TOLD).

use super::*;

/// Listings posted without signing in under a contact an account had, whose code nobody typed
/// (web/post.rs): a day on, taken down for good (DELETED), their contact and their photos' rows
/// with them, so that someone else's number typed into the form leaves nothing behind; returns
/// the photos' files, which the caller takes out of the store.
pub async fn forget_drafts(db: &PgPool) -> sqlx::Result<Vec<String>> {
    let mut tx = db.begin().await?;
    let old = "l.status = 'DRAFT' AND l.created_at < now() - interval '1 day'";
    let files: Vec<String> =
        sqlx::query_scalar(&format!("DELETE FROM listing_media m USING listings l WHERE l.id = m.listing_id AND {old} RETURNING m.storage_key"))
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query(&format!("DELETE FROM listing_contacts c USING listings l WHERE l.id = c.listing_id AND {old}")).execute(&mut *tx).await?;
    sqlx::query(&format!("UPDATE listings l SET status = 'DELETED' WHERE {old}")).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(files)
}

/// Who hears of a listing's expiry, `l` the listing: whoever posted it; for a store's listing,
/// while they still post for the store (manages_listing, 0035) and their account is active,
/// and else the store's owner, who may renew it as well. NULL for a store without an owner.
const TOLD: &str = "CASE WHEN l.seller_business_id IS NULL
                            OR manages_listing(l.seller_business_id, l.created_by_user_id, l.created_by_user_id)
                               AND EXISTS (SELECT 1 FROM users p WHERE p.id = l.created_by_user_id AND p.status = 'ACTIVE')
                          THEN l.created_by_user_id
                          ELSE (SELECT m.user_id FROM business_members m
                                WHERE m.business_id = l.seller_business_id AND m.role = 'OWNER')
                     END";

/// A listing whose seller hears that it expired or is about to, with how to reach them.
#[derive(Debug, sqlx::FromRow)]
pub struct ExpiryNotice {
    pub listing_id: Uuid,
    /// Who hears of it (TOLD): whoever posted it, or a store's owner.
    pub user_id: Uuid,
    pub public_no: i64,
    pub title: String,
    pub expires_at: OffsetDateTime,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Whether its seller lets the site e-mail them (notification_preferences.email_enabled).
    pub email_ok: bool,
}

/// Takes the listings up or reserved whose time is up off the site (EXPIRED, the status history
/// saying why), each with a notification to its seller (EXPIRED, to whom TOLD says); returns
/// those with someone to tell.
pub async fn expire_listings(db: &PgPool) -> sqlx::Result<Vec<ExpiryNotice>> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.status_reason', 'Skadoi', true)").execute(&mut *tx).await?;
    let expired = sqlx::query_as(&format!(
        "WITH gone AS (
             UPDATE listings l SET status = 'EXPIRED', updated_at = now()
             WHERE l.status IN ('ACTIVE', 'RESERVED') AND l.expires_at < now()
             RETURNING l.id, {TOLD} AS user_id, l.public_no, coalesce(l.generated_title, l.title) AS title, l.expires_at
         ),
         told AS (
             INSERT INTO notifications (user_id, notification_type, title, payload)
             SELECT g.user_id, 'EXPIRED', left('Njoftimi skadoi: ' || g.title, 180),
                    jsonb_build_object('listing_id', g.id, 'public_no', g.public_no)
             FROM gone g
             WHERE g.user_id IS NOT NULL
         )
         SELECT g.id AS listing_id, g.user_id, g.public_no, g.title, g.expires_at,
                u.email::text AS email, u.phone_e164::text AS phone, coalesce(np.email_enabled, true) AS email_ok
         FROM gone g
         JOIN users u ON u.id = g.user_id
         LEFT JOIN notification_preferences np ON np.user_id = u.id"
    ))
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(expired)
}

/// The listings up or reserved that expire within `days`, whose sellers haven't heard of this
/// expiry yet (a renewal is a new one; a store's owner, told once its poster has left, hears of
/// it even if the poster did): a notification to each seller (EXPIRING, with when; to whom TOLD
/// says), and the listings; at most 500 a time.
pub async fn warn_expiring(db: &PgPool, days: i32) -> sqlx::Result<Vec<ExpiryNotice>> {
    sqlx::query_as(&format!(
        "WITH due AS (
             SELECT l.id, t.user_id, l.public_no, coalesce(l.generated_title, l.title) AS title, l.expires_at
             FROM listings l
             CROSS JOIN LATERAL (SELECT {TOLD} AS user_id) t
             WHERE l.status IN ('ACTIVE', 'RESERVED') AND l.expires_at IS NOT NULL
               AND l.expires_at < now() + make_interval(days => $1)
               AND t.user_id IS NOT NULL
               AND NOT EXISTS (
                   SELECT 1 FROM notifications n
                   WHERE n.user_id = t.user_id AND n.notification_type = 'EXPIRING'
                     AND n.payload ->> 'listing_id' = l.id::text
                     AND (n.payload ->> 'expires_at')::timestamptz = l.expires_at)
             ORDER BY l.expires_at
             LIMIT 500
         ),
         told AS (
             INSERT INTO notifications (user_id, notification_type, title, payload)
             SELECT d.user_id, 'EXPIRING', left('Njoftimi skadon së shpejti: ' || d.title, 180),
                    jsonb_build_object('listing_id', d.id, 'public_no', d.public_no, 'expires_at', d.expires_at)
             FROM due d
         )
         SELECT d.id AS listing_id, d.user_id, d.public_no, d.title, d.expires_at,
                u.email::text AS email, u.phone_e164::text AS phone, coalesce(np.email_enabled, true) AS email_ok
         FROM due d
         JOIN users u ON u.id = d.user_id
         LEFT JOIN notification_preferences np ON np.user_id = u.id"
    ))
    .bind(days)
    .fetch_all(db)
    .await
}
