//! Every notification a user has had lately, of whatever kind, for /lajmerimet (web/notifications.rs):
//! prices dropped (price_drops.rs), saved searches with new listings (searches.rs), messages left
//! unread (notices.rs), the user's own listings expiring or expired (expiry.rs); and which of them
//! are unread.

use super::*;

/// How far back /lajmerimet goes, in days, and how many notifications it shows at most.
pub const NOTIFICATIONS_DAYS: i32 = 90;
pub const NOTIFICATIONS_MOST: i64 = 100;

/// The kinds about the user's own listings, which the account button's dot stands for: the others
/// have the heart's dot (prices dropped, saved searches) and the conversations' count (messages).
pub const OWN_LISTING_KINDS: [&str; 3] = ["EXPIRING", "EXPIRED", "PROMOTED"];

/// A notification, with what it is about while that is still there to open.
#[derive(Debug, sqlx::FromRow)]
pub struct Notification {
    /// A UUIDv7: the later ones sort after.
    pub id: Uuid,
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
    pub created_at: OffsetDateTime,
    pub read: bool,
    /// The listing it is about, while the user may open its page: up or sold, or, for their own,
    /// anything but deleted.
    pub public_no: Option<i64>,
    pub slug: Option<String>,
    /// The saved search it is about, while the user keeps it.
    pub search: Option<Uuid>,
    /// The conversation it is about.
    pub conversation: Option<Uuid>,
    /// The invitation into a store it is about, while it is open.
    pub invitation: Option<Uuid>,
}

/// The user's notifications of the last NOTIFICATIONS_DAYS, the latest first; NOTIFICATIONS_MOST
/// at most.
pub async fn notifications(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Notification>> {
    sqlx::query_as(
        "SELECT n.id, n.notification_type AS kind, n.title, n.body, n.created_at, n.read_at IS NOT NULL AS read,
                l.public_no, l.slug, s.id AS search, c.id AS conversation, i.id AS invitation
         FROM notifications n
         LEFT JOIN listings l
                ON l.id = CASE WHEN n.payload ? 'listing_id' THEN (n.payload ->> 'listing_id')::uuid END
               AND (l.status IN ('ACTIVE', 'RESERVED', 'SOLD') OR (l.created_by_user_id = n.user_id AND l.status <> 'DELETED'))
         LEFT JOIN saved_searches s
                ON s.id = CASE WHEN n.payload ? 'search' THEN (n.payload ->> 'search')::uuid END AND s.user_id = n.user_id
         LEFT JOIN conversations c
                ON c.id = CASE WHEN n.payload ? 'conversation' THEN (n.payload ->> 'conversation')::uuid END
         LEFT JOIN business_invitations i
                ON i.id = CASE WHEN n.payload ? 'invitation' THEN (n.payload ->> 'invitation')::uuid END AND i.expires_at > now()
         WHERE n.user_id = $1 AND n.created_at > now() - make_interval(days => $2)
         ORDER BY n.created_at DESC, n.id DESC
         LIMIT $3",
    )
    .bind(user_id)
    .bind(NOTIFICATIONS_DAYS)
    .bind(NOTIFICATIONS_MOST)
    .fetch_all(db)
    .await
}

/// How many of the user's notifications of the last NOTIFICATIONS_DAYS are unread: all of them,
/// and those about their own listings (OWN_LISTING_KINDS).
#[derive(Debug, Default, Clone, Copy, sqlx::FromRow)]
pub struct Unread {
    pub all: i64,
    pub own: i64,
}

pub async fn unread_notifications(db: &PgPool, user_id: Uuid) -> sqlx::Result<Unread> {
    sqlx::query_as(
        "SELECT count(*) AS \"all\", count(*) FILTER (WHERE notification_type = ANY($3)) AS own
         FROM notifications
         WHERE user_id = $1 AND read_at IS NULL AND created_at > now() - make_interval(days => $2)",
    )
    .bind(user_id)
    .bind(NOTIFICATIONS_DAYS)
    .bind(&OWN_LISTING_KINDS[..])
    .fetch_one(db)
    .await
}

/// The user saw their notifications on /lajmerimet, up to the latest it showed (`up_to`, a
/// notification's id): not one that came after, as a copy of the page shown again from the
/// browser's cache wouldn't show it.
pub async fn notifications_seen(db: &PgPool, user_id: Uuid, up_to: Uuid) -> sqlx::Result<()> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL AND id <= $2")
        .bind(user_id)
        .bind(up_to)
        .execute(db)
        .await?;
    Ok(())
}
