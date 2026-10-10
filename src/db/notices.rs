//! Notices of unread messages (src/notices.rs): who has a message they haven't read a while after
//! it came, and the notices sent (notifications, NEW_MESSAGE); whether a user wants them
//! (notification_preferences.message_alerts, on unless a row says otherwise).

use super::*;

/// Someone to tell about messages they haven't read: the latest of them, and how many there are.
#[derive(Debug)]
pub struct DueNotice {
    pub user_id: Uuid,
    pub conversation_id: Uuid,
    /// How to reach them: the e-mail address or phone number they sign in with.
    pub email: Option<String>,
    pub phone: Option<String>,
    /// Whether they let the site e-mail them (notification_preferences.email_enabled).
    pub email_ok: bool,
    /// Who wrote (a store's name, to its buyer), and about which listing (None once it is gone
    /// for good).
    pub writer: String,
    pub listing: Option<String>,
    /// The latest message they haven't read, and how many they haven't.
    pub body: String,
    pub unread: i64,
    /// The language their e-mail is in (users.preferred_language: "sq", "en").
    pub language: String,
}

/// Those who have messages from the other side unread at least `after` after they came (for one
/// of a store's people taking part, the buyer's; a colleague's answer is no news), in a
/// conversation they still see (messages.rs, `conversation`) written in over the last day, newer than the last notice about it, which went out
/// at least `gap` ago or never; who want such notices, and whose account is active. At most 100,
/// the oldest conversations first: a minute later the next ones.
pub async fn due_notices(db: &PgPool, after: std::time::Duration, gap: std::time::Duration) -> sqlx::Result<Vec<DueNotice>> {
    sqlx::query_as!(
        DueNotice,
        r#"SELECT p.user_id, p.conversation_id,
                  u.email::text AS email, u.phone_e164::text AS phone,
                  coalesce(np.email_enabled, true) AS "email_ok!",
                  CASE WHEN p.user_id = c.buyer_user_id AND b.display_name IS NOT NULL THEN b.display_name
                       ELSE writer.display_name END AS "writer!",
                  coalesce(l.generated_title, l.title) AS "listing?",
                  coalesce(latest.body, '') AS "body!",
                  unread.count AS "unread!",
                  u.preferred_language AS language
           FROM conversations c
           JOIN conversation_participants p ON p.conversation_id = c.id
           JOIN users u ON u.id = p.user_id AND u.status = 'ACTIVE'
           LEFT JOIN listings l ON l.id = c.listing_id
           LEFT JOIN businesses b ON b.id = c.seller_business_id
           LEFT JOIN notification_preferences np ON np.user_id = p.user_id
           LEFT JOIN LATERAL (
               SELECT max(n.created_at) AS at
               FROM notifications n
               WHERE n.user_id = p.user_id AND n.notification_type = 'NEW_MESSAGE'
                 AND n.payload ->> 'conversation' = c.id::text
           ) last ON true
           JOIN LATERAL (
               SELECT m.body, m.sender_user_id
               FROM messages m
               WHERE m.conversation_id = c.id
                 AND CASE WHEN p.user_id = c.buyer_user_id THEN m.sender_user_id <> c.buyer_user_id
                          ELSE m.sender_user_id = c.buyer_user_id END
                 AND m.deleted_at IS NULL
                 AND m.created_at > coalesce(p.last_read_at, '-infinity')
                 AND m.created_at > coalesce(last.at, '-infinity')
                 AND m.created_at <= now() - make_interval(secs => $1)
               ORDER BY m.created_at DESC
               LIMIT 1
           ) latest ON true
           JOIN users writer ON writer.id = latest.sender_user_id
           CROSS JOIN LATERAL (
               SELECT count(*) AS count
               FROM messages m
               WHERE m.conversation_id = c.id
                 AND CASE WHEN p.user_id = c.buyer_user_id THEN m.sender_user_id <> c.buyer_user_id
                          ELSE m.sender_user_id = c.buyer_user_id END
                 AND m.deleted_at IS NULL
                 AND m.created_at > coalesce(p.last_read_at, '-infinity')
           ) unread
           WHERE c.last_message_at > now() - interval '1 day'
             AND (p.user_id = c.buyer_user_id OR p.user_id = c.seller_user_id
                  OR EXISTS (SELECT 1 FROM business_members bm
                             WHERE bm.business_id = c.seller_business_id AND bm.user_id = p.user_id))
             AND p.archived_at IS NULL
             AND (p.muted_until IS NULL OR p.muted_until < now())
             AND (last.at IS NULL OR last.at < now() - make_interval(secs => $2))
             AND coalesce(np.message_alerts, true)
           ORDER BY c.last_message_at, p.user_id
           LIMIT 100"#,
        after.as_secs_f64(),
        gap.as_secs_f64(),
    )
    .fetch_all(db)
    .await
}

/// Records a notice sent to `user_id` about a conversation (notifications, NEW_MESSAGE): what it
/// said, and the conversation, which the next look for notices goes by.
pub async fn record_notice(db: &PgPool, user_id: Uuid, conversation_id: Uuid, title: &str, body: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO notifications (user_id, notification_type, title, body, payload)
         VALUES ($1, 'NEW_MESSAGE', $2, $3, jsonb_build_object('conversation', $4::text))",
        user_id,
        title,
        body,
        conversation_id.to_string(),
    )
    .execute(db)
    .await
    .map(|_| ())
}
