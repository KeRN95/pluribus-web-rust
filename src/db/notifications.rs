//! Every notification a user has had lately, of whatever kind, for /lajmerimet (web/notifications.rs):
//! prices dropped (price_drops.rs), saved searches with new listings (searches.rs), messages left
//! unread (notices.rs), the user's own listings expiring or expired (expiry.rs); and which of them
//! are unread. Each with its payload, what its writer kept of what it is about, from which
//! domain::notices says its title in either language.

use serde_json::Value;

use super::*;
use crate::domain::NotificationKind;

/// How far back /lajmerimet goes, in days, and how many notifications it shows at most.
pub const NOTIFICATIONS_DAYS: i32 = 90;
pub const NOTIFICATIONS_MOST: i64 = 100;

/// A notification, with what it is about while that is still there to open.
#[derive(Debug, sqlx::FromRow)]
pub struct Notification {
    /// A UUIDv7: the later ones sort after.
    pub id: Uuid,
    pub kind: String,
    /// Kept in Albanian, its first 180 characters (domain::notices::title says it from `payload`).
    pub title: String,
    pub body: Option<String>,
    /// What its writer kept of what it is about: each kind's fields, as domain::notices lists them.
    pub payload: Json<Value>,
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
        "SELECT n.id, n.notification_type AS kind, n.title, n.body, n.payload, n.created_at, n.read_at IS NOT NULL AS read,
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
/// and those about their own listings (NotificationKind::OWN_LISTING).
#[derive(Debug, Default, Clone, Copy, sqlx::FromRow)]
pub struct Unread {
    pub all: i64,
    pub own: i64,
}

pub async fn unread_notifications(db: &PgPool, user_id: Uuid) -> sqlx::Result<Unread> {
    let own = NotificationKind::names(NotificationKind::OWN_LISTING);
    sqlx::query_as(
        "SELECT count(*) AS \"all\", count(*) FILTER (WHERE notification_type = ANY($3)) AS own
         FROM notifications
         WHERE user_id = $1 AND read_at IS NULL AND created_at > now() - make_interval(days => $2)",
    )
    .bind(user_id)
    .bind(NOTIFICATIONS_DAYS)
    .bind(&own[..])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{Role, notices},
        format::Contact,
    };

    /// tests/fixtures/listings.sql's people: Ana and Besnik, and Dritan, who owns Auto Durrësi.
    const ANA: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);
    const BESNIK: Uuid = Uuid::from_u128(0x2222_2222_2222_4222_8222_2222_2222_2222);
    const DRITAN: Uuid = Uuid::from_u128(0x3333_3333_3333_4333_8333_3333_3333_3333);
    const AUTO_DURRESI: Uuid = Uuid::from_u128(0x4444_4444_4444_4444_8444_4444_4444_4444);

    /// Each writer keeps in a notification's payload what its title says: said from the payload
    /// (domain::notices), the title is the one it kept, whole (the column keeps its first 180
    /// characters), and an invitation's body too; read back as /lajmerimet reads them.
    #[sqlx::test(migrator = "crate::db::MIGRATOR", fixtures("../../tests/fixtures/listings.sql"))]
    async fn each_writer_keeps_what_its_notifications_title_says(pool: PgPool) -> sqlx::Result<()> {
        let golf: Uuid = sqlx::query_scalar("SELECT id FROM listings WHERE public_no = 1001").fetch_one(&pool).await?;
        // Besnik keeps Ana's Golf, whose price she lowers (posting.rs, update_listing): a price
        // drop for him.
        sqlx::query("INSERT INTO saved_listings (user_id, listing_id) VALUES ($1, $2)").bind(BESNIK).bind(golf).execute(&pool).await?;
        sqlx::query("UPDATE listing_price_history SET changed_at = changed_at - interval '1 hour' WHERE listing_id = $1")
            .bind(golf)
            .execute(&pool)
            .await?;
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE listings SET price_amount = 9000 WHERE id = $1").bind(golf).execute(&mut *tx).await?;
        notify_price_drop(&mut tx, golf).await?;
        tx.commit().await?;

        // He wrote to her about it ten minutes ago, and she hasn't read it: a notice (notices.rs).
        let conversation: Uuid = sqlx::query_scalar(
            "INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id, last_message_at)
             VALUES ($1, $2, $3, now() - interval '10 minutes') RETURNING id",
        )
        .bind(golf)
        .bind(BESNIK)
        .bind(ANA)
        .fetch_one(&pool)
        .await?;
        sqlx::query("INSERT INTO conversation_participants (conversation_id, user_id) VALUES ($1, $2), ($1, $3)")
            .bind(conversation)
            .bind(BESNIK)
            .bind(ANA)
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO messages (conversation_id, sender_user_id, body, created_at)
             VALUES ($1, $2, 'A është ende në shitje?', now() - interval '10 minutes')",
        )
        .bind(conversation)
        .bind(BESNIK)
        .execute(&pool)
        .await?;
        assert_eq!(crate::notices::send_due(&pool, "https://pluribus.example").await?, 1);

        // Her sofa's time is up, and her iPhone's two days off (expiry.rs).
        sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = 1007").execute(&pool).await?;
        sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = 1005").execute(&pool).await?;
        assert_eq!(expire_listings(&pool).await?.len(), 1);
        assert_eq!(warn_expiring(&pool, 3).await?.len(), 1);

        // A moderator features her bicycle (web/moderation.rs).
        assert!(matches!(grant_promotion(&pool, 1010, "FEATURED_7D", BESNIK).await?, Granted::Given(Some(_))));

        // His saved search has new listings: its alert, with the title web/searches.rs gives it.
        let name = "Makina në Tiranë";
        let search = NewSearch { name, category_id: None, words: None, path: "/c/makina", query: "city=tirane" };
        let search = save_search(&pool, BESNIK, &search).await?.expect("saved");
        let title = format!("{}: {name}", notices::new_listings(3));
        record_search_alert(&pool, BESNIK, search, &title, 3, &format!("https://pluribus.example/kerkimet/{search}")).await?;

        // Dritan's store asks him in to post its listings, in the words web/stores.rs gives it.
        let role = Role::Editor;
        let body = format!("Si „{}“: {}", role.label(), role.does());
        let contact = Contact::Phone("+355690000002".to_string());
        let invited = invite(&pool, AUTO_DURRESI, &contact, role.as_str(), DRITAN, "Auto Durrësi të fton në dyqan", &body).await?;
        assert!(matches!(invited, Invited::Sent(_)));

        let mut kinds = Vec::new();
        for user in [ANA, BESNIK] {
            for notification in notifications(&pool, user).await? {
                let kind = NotificationKind::parse(&notification.kind).expect("a kind the code writes");
                let said: String = notices::title(kind, &notification.payload).chars().take(180).collect();
                assert_eq!(said, notification.title, "{kind}: {}", notification.payload.0);
                let body = notices::body(kind, &notification.payload);
                match kind {
                    NotificationKind::StoreInvite => assert_eq!(body, notification.body),
                    _ => assert_eq!(body, None, "{kind}"),
                }
                kinds.push(kind);
            }
        }
        for kind in NotificationKind::ALL {
            assert_eq!(kinds.iter().filter(|written| **written == kind).count(), 1, "{kind}: {kinds:?}");
        }
        Ok(())
    }
}
