//! Listings that expire (posting.rs, LISTING_DAYS): those whose time is up leave the site
//! (EXPIRED), and their sellers hear of it; those a few days from it, their sellers hear first
//! (expiry.rs). Each telling is a notification (EXPIRED, EXPIRING) to the account that posted it.

use super::*;

/// A listing whose seller hears that it expired or is about to, with how to reach them.
#[derive(Debug, sqlx::FromRow)]
pub struct ExpiryNotice {
    pub listing_id: Uuid,
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
/// saying why), each with a notification to its seller (EXPIRED); returns them.
pub async fn expire_listings(db: &PgPool) -> sqlx::Result<Vec<ExpiryNotice>> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.status_reason', 'Skadoi', true)").execute(&mut *tx).await?;
    let expired = sqlx::query_as(
        "WITH gone AS (
             UPDATE listings l SET status = 'EXPIRED', updated_at = now()
             WHERE l.status IN ('ACTIVE', 'RESERVED') AND l.expires_at < now()
             RETURNING l.id, l.created_by_user_id, l.public_no, coalesce(l.generated_title, l.title) AS title, l.expires_at
         ),
         told AS (
             INSERT INTO notifications (user_id, notification_type, title, payload)
             SELECT g.created_by_user_id, 'EXPIRED', left('Njoftimi skadoi: ' || g.title, 180),
                    jsonb_build_object('listing_id', g.id, 'public_no', g.public_no)
             FROM gone g
         )
         SELECT g.id AS listing_id, g.created_by_user_id AS user_id, g.public_no, g.title, g.expires_at,
                u.email::text AS email, u.phone_e164::text AS phone, coalesce(np.email_enabled, true) AS email_ok
         FROM gone g
         JOIN users u ON u.id = g.created_by_user_id
         LEFT JOIN notification_preferences np ON np.user_id = u.id",
    )
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(expired)
}

/// The listings up or reserved that expire within `days`, whose sellers haven't heard of this
/// expiry yet (a renewal is a new one): a notification to each seller (EXPIRING, with when), and
/// the listings; at most 500 a time.
pub async fn warn_expiring(db: &PgPool, days: i32) -> sqlx::Result<Vec<ExpiryNotice>> {
    sqlx::query_as(
        "WITH due AS (
             SELECT l.id, l.created_by_user_id, l.public_no, coalesce(l.generated_title, l.title) AS title, l.expires_at
             FROM listings l
             WHERE l.status IN ('ACTIVE', 'RESERVED') AND l.expires_at IS NOT NULL
               AND l.expires_at < now() + make_interval(days => $1)
               AND NOT EXISTS (
                   SELECT 1 FROM notifications n
                   WHERE n.user_id = l.created_by_user_id AND n.notification_type = 'EXPIRING'
                     AND n.payload ->> 'listing_id' = l.id::text
                     AND (n.payload ->> 'expires_at')::timestamptz = l.expires_at)
             ORDER BY l.expires_at
             LIMIT 500
         ),
         told AS (
             INSERT INTO notifications (user_id, notification_type, title, payload)
             SELECT d.created_by_user_id, 'EXPIRING', left('Njoftimi skadon së shpejti: ' || d.title, 180),
                    jsonb_build_object('listing_id', d.id, 'public_no', d.public_no, 'expires_at', d.expires_at)
             FROM due d
         )
         SELECT d.id AS listing_id, d.created_by_user_id AS user_id, d.public_no, d.title, d.expires_at,
                u.email::text AS email, u.phone_e164::text AS phone, coalesce(np.email_enabled, true) AS email_ok
         FROM due d
         JOIN users u ON u.id = d.created_by_user_id
         LEFT JOIN notification_preferences np ON np.user_id = u.id",
    )
    .bind(days)
    .fetch_all(db)
    .await
}
