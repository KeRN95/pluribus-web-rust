//! Messages between a listing's buyers and its seller: a buyer starts a conversation about a
//! listing, and the two write in it (conversations, conversation_participants and messages, from
//! the first migration). The seller's side is the private seller, or for a business's listing
//! whoever posted it. Times are Albania's (Europe/Tirane), which the database knows.

use time::PrimitiveDateTime;

use super::*;

/// The most messages a conversation's page shows: its latest.
const MESSAGES_SHOWN: i64 = 300;

/// A listing as a buyer would write to its seller about it.
#[derive(Debug)]
pub struct MessageTarget {
    pub listing_id: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    pub listing_type: String,
    /// Its first photo's card-sized copy, for the page's head.
    pub photo: Option<String>,
    pub seller_user_id: Option<Uuid>,
    pub seller_business_id: Option<Uuid>,
    /// Who answers for the seller: the private seller, or whoever posted a business's listing.
    pub seller_side_id: Uuid,
    /// The business's name, or the seller's.
    pub seller_name: String,
    /// Whether the listing takes messages: it is up (active or reserved), and they aren't turned
    /// off for it (listing_contacts.allow_messages).
    pub open: bool,
    /// The conversation the buyer has about it already, if any.
    pub conversation_id: Option<Uuid>,
}

pub async fn message_target(db: &PgPool, public_no: i64, buyer_id: Uuid) -> sqlx::Result<Option<MessageTarget>> {
    sqlx::query_as!(
        MessageTarget,
        r#"SELECT l.id AS listing_id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS "title!",
                  l.price_amount, l.currency::text AS "currency!", l.price_type, l.listing_type,
                  (SELECT coalesce(m.thumbnail_url, m.public_url)
                   FROM listing_media m
                   WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                   ORDER BY m.is_cover DESC, m.sort_order
                   LIMIT 1) AS photo,
                  l.seller_user_id, l.seller_business_id,
                  coalesce(l.seller_user_id, l.created_by_user_id) AS "seller_side_id!",
                  coalesce(b.display_name, u.display_name) AS "seller_name!",
                  (l.status IN ('ACTIVE', 'RESERVED') AND coalesce(c.allow_messages, true)) AS "open!",
                  (SELECT v.id FROM conversations v WHERE v.listing_id = l.id AND v.buyer_user_id = $2) AS conversation_id
           FROM listings l
           JOIN users u ON u.id = coalesce(l.seller_user_id, l.created_by_user_id)
           LEFT JOIN businesses b ON b.id = l.seller_business_id
           LEFT JOIN listing_contacts c ON c.listing_id = l.id
           WHERE l.public_no = $1 AND l.status NOT IN ('DRAFT', 'DELETED')"#,
        public_no,
        buyer_id,
    )
    .fetch_optional(db)
    .await
}

/// What an account sent lately, for the limits on sending (web/messages.rs): messages in the last
/// minute and day, and conversations it started (sellers it wrote to) in the last hour and day.
#[derive(Debug)]
pub struct SentLately {
    pub last_minute: i64,
    pub last_day: i64,
    pub started_last_hour: i64,
    pub started_last_day: i64,
}

pub async fn sent_lately(db: &PgPool, user_id: Uuid) -> sqlx::Result<SentLately> {
    sqlx::query_as!(
        SentLately,
        r#"SELECT count(*) FILTER (WHERE m.created_at > now() - interval '1 minute') AS "last_minute!",
                  count(*) AS "last_day!",
                  (SELECT count(*) FROM conversations c
                   WHERE c.buyer_user_id = $1 AND c.created_at > now() - interval '1 hour') AS "started_last_hour!",
                  (SELECT count(*) FROM conversations c
                   WHERE c.buyer_user_id = $1 AND c.created_at > now() - interval '1 day') AS "started_last_day!"
           FROM messages m
           WHERE m.sender_user_id = $1 AND m.created_at > now() - interval '1 day'"#,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// Whether `user` blocked `other`, and whether `other` blocked `user`.
pub async fn blocks_between(db: &PgPool, user: Uuid, other: Uuid) -> sqlx::Result<(bool, bool)> {
    let row = sqlx::query!(
        r#"SELECT EXISTS (SELECT 1 FROM user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2) AS "blocked!",
                  EXISTS (SELECT 1 FROM user_blocks WHERE blocker_user_id = $2 AND blocked_user_id = $1) AS "blocked_by!""#,
        user,
        other,
    )
    .fetch_one(db)
    .await?;
    Ok((row.blocked, row.blocked_by))
}

/// Blocks `other` for `user`, or with `on` false lets them write again.
pub async fn set_blocked(db: &PgPool, user: Uuid, other: Uuid, on: bool) -> sqlx::Result<()> {
    if on {
        sqlx::query!(
            "INSERT INTO user_blocks (blocker_user_id, blocked_user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            user,
            other,
        )
        .execute(db)
        .await?;
    } else {
        sqlx::query!("DELETE FROM user_blocks WHERE blocker_user_id = $1 AND blocked_user_id = $2", user, other)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Starts the buyer's conversation about the listing with its first message, both sides taking
/// part, or adds the message to the one they have (one per buyer and listing); returns its id.
pub async fn start_conversation(db: &PgPool, target: &MessageTarget, buyer_id: Uuid, body: &str) -> sqlx::Result<Uuid> {
    let mut tx = db.begin().await?;
    let id = sqlx::query_scalar!(
        "INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id, seller_business_id)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (listing_id, buyer_user_id) DO UPDATE SET updated_at = now()
         RETURNING id",
        target.listing_id,
        buyer_id,
        target.seller_user_id,
        target.seller_business_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO conversation_participants (conversation_id, user_id) VALUES ($1, $2), ($1, $3)
         ON CONFLICT DO NOTHING",
        id,
        buyer_id,
        target.seller_side_id,
    )
    .execute(&mut *tx)
    .await?;
    write(&mut tx, id, buyer_id, body).await?;
    tx.commit().await?;
    Ok(id)
}

/// Writes a message in a conversation its sender takes part in.
pub async fn send_message(db: &PgPool, conversation_id: Uuid, sender_id: Uuid, body: &str) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    write(&mut tx, conversation_id, sender_id, body).await?;
    tx.commit().await
}

/// The message, the conversation moved to the top of both inboxes, and everything in it read by
/// its sender, who just wrote in it.
async fn write(tx: &mut sqlx::Transaction<'_, Postgres>, conversation_id: Uuid, sender_id: Uuid, body: &str) -> sqlx::Result<()> {
    let sent_at = sqlx::query_scalar!(
        "INSERT INTO messages (conversation_id, sender_user_id, body) VALUES ($1, $2, $3) RETURNING created_at",
        conversation_id,
        sender_id,
        body,
    )
    .fetch_one(&mut **tx)
    .await?;
    sqlx::query!("UPDATE conversations SET last_message_at = $2 WHERE id = $1", conversation_id, sent_at)
        .execute(&mut **tx)
        .await?;
    sqlx::query!(
        "UPDATE conversation_participants SET last_read_at = $3 WHERE conversation_id = $1 AND user_id = $2",
        conversation_id,
        sender_id,
        sent_at,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// A conversation as one of its two sides sees it.
#[derive(Debug)]
pub struct Conversation {
    pub id: Uuid,
    /// The listing it is about; None once it is gone for good.
    pub public_no: Option<i64>,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub listing_status: Option<String>,
    pub price_amount: Option<Decimal>,
    pub currency: Option<String>,
    pub price_type: Option<String>,
    pub listing_type: Option<String>,
    pub photo: Option<String>,
    /// The other side: the seller, as the buyer sees them (a business's name), or the buyer.
    pub other_id: Uuid,
    pub other_name: String,
    /// Whether the other side has written in it yet, and how many messages the user sent since
    /// they last did: the limits until an answer comes (web/messages.rs).
    pub answered: bool,
    pub in_a_row: i64,
    pub blocked: bool,
    pub blocked_by: bool,
}

/// The conversation, for someone who takes part in it; None for anyone else.
pub async fn conversation(db: &PgPool, id: Uuid, user_id: Uuid) -> sqlx::Result<Option<Conversation>> {
    sqlx::query_as!(
        Conversation,
        r#"SELECT c.id, l.public_no AS "public_no?", l.slug AS "slug?",
                  coalesce(l.generated_title, l.title) AS title, l.status AS "listing_status?",
                  l.price_amount AS "price_amount?", l.currency::text AS currency, l.price_type AS "price_type?",
                  l.listing_type AS "listing_type?",
                  (SELECT coalesce(m.thumbnail_url, m.public_url)
                   FROM listing_media m
                   WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                   ORDER BY m.is_cover DESC, m.sort_order
                   LIMIT 1) AS photo,
                  other.user_id AS "other_id!",
                  CASE WHEN c.buyer_user_id = $2 THEN coalesce(b.display_name, u.display_name)
                       ELSE u.display_name END AS "other_name!",
                  EXISTS (SELECT 1 FROM messages m
                          WHERE m.conversation_id = c.id AND m.sender_user_id = other.user_id) AS "answered!",
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = c.id AND m.sender_user_id = $2
                     AND m.created_at > coalesce((SELECT max(o.created_at) FROM messages o
                                                  WHERE o.conversation_id = c.id AND o.sender_user_id = other.user_id),
                                                 '-infinity')) AS "in_a_row!",
                  EXISTS (SELECT 1 FROM user_blocks k
                          WHERE k.blocker_user_id = $2 AND k.blocked_user_id = other.user_id) AS "blocked!",
                  EXISTS (SELECT 1 FROM user_blocks k
                          WHERE k.blocker_user_id = other.user_id AND k.blocked_user_id = $2) AS "blocked_by!"
           FROM conversations c
           JOIN conversation_participants me ON me.conversation_id = c.id AND me.user_id = $2
           JOIN LATERAL (SELECT p.user_id FROM conversation_participants p
                         WHERE p.conversation_id = c.id AND p.user_id <> $2
                         ORDER BY p.joined_at
                         LIMIT 1) other ON true
           JOIN users u ON u.id = other.user_id
           LEFT JOIN listings l ON l.id = c.listing_id
           LEFT JOIN businesses b ON b.id = c.seller_business_id
           WHERE c.id = $1"#,
        id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Whether the user takes part in the conversation.
pub async fn takes_part(db: &PgPool, conversation_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM conversation_participants
                          WHERE conversation_id = $1 AND user_id = $2) AS "takes_part!""#,
        conversation_id,
        user_id,
    )
    .fetch_one(db)
    .await
}

/// A message as a conversation's page shows it.
#[derive(Debug)]
pub struct Message {
    pub id: Uuid,
    /// Sent by the user reading it.
    pub mine: bool,
    pub body: String,
    /// When it was sent, in Albania's time, and how many days before today that was there.
    pub sent: PrimitiveDateTime,
    pub days_ago: i32,
}

/// A conversation's messages, the oldest first: its latest MESSAGES_SHOWN, or those after the
/// message `after` (ids grow with time, uuid_v7).
pub async fn messages(db: &PgPool, conversation_id: Uuid, user_id: Uuid, after: Option<Uuid>) -> sqlx::Result<Vec<Message>> {
    let mut messages = sqlx::query_as!(
        Message,
        r#"SELECT m.id, coalesce(m.sender_user_id = $2, false) AS "mine!", coalesce(m.body, '') AS "body!",
                  (m.created_at AT TIME ZONE 'Europe/Tirane') AS "sent!",
                  ((now() AT TIME ZONE 'Europe/Tirane')::date - (m.created_at AT TIME ZONE 'Europe/Tirane')::date) AS "days_ago!"
           FROM messages m
           WHERE m.conversation_id = $1 AND m.deleted_at IS NULL AND m.message_type = 'TEXT'
             AND ($3::uuid IS NULL OR m.id > $3)
           ORDER BY m.id DESC
           LIMIT $4"#,
        conversation_id,
        user_id,
        after,
        MESSAGES_SHOWN,
    )
    .fetch_all(db)
    .await?;
    messages.reverse();
    Ok(messages)
}

/// The user has read the conversation up to the message `upto`, which their page showed.
pub async fn mark_read(db: &PgPool, conversation_id: Uuid, user_id: Uuid, upto: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE conversation_participants p
         SET last_read_at = m.created_at
         FROM messages m
         WHERE m.id = $3 AND m.conversation_id = $1 AND p.conversation_id = $1 AND p.user_id = $2
           AND (p.last_read_at IS NULL OR p.last_read_at < m.created_at)",
        conversation_id,
        user_id,
        upto,
    )
    .execute(db)
    .await?;
    Ok(())
}

/// A conversation in the inbox.
#[derive(Debug)]
pub struct InboxEntry {
    pub id: Uuid,
    pub listing_title: Option<String>,
    pub photo: Option<String>,
    pub other_name: String,
    /// Its latest message: what it says, whether the user sent it, and when, as Message has it.
    pub last_body: Option<String>,
    pub last_mine: bool,
    pub last_sent: Option<PrimitiveDateTime>,
    pub days_ago: Option<i32>,
    /// Messages from the other side the user hasn't read.
    pub unread: i64,
}

/// The user's conversations, the latest written in first.
pub async fn inbox(db: &PgPool, user_id: Uuid, limit: i64) -> sqlx::Result<Vec<InboxEntry>> {
    sqlx::query_as!(
        InboxEntry,
        r#"SELECT c.id, coalesce(l.generated_title, l.title) AS listing_title,
                  (SELECT coalesce(m.thumbnail_url, m.public_url)
                   FROM listing_media m
                   WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                   ORDER BY m.is_cover DESC, m.sort_order
                   LIMIT 1) AS photo,
                  CASE WHEN c.buyer_user_id = $1 THEN coalesce(b.display_name, u.display_name)
                       ELSE u.display_name END AS "other_name!",
                  last.body AS "last_body?", coalesce(last.sender_user_id = $1, false) AS "last_mine!",
                  (last.created_at AT TIME ZONE 'Europe/Tirane') AS "last_sent?",
                  ((now() AT TIME ZONE 'Europe/Tirane')::date - (last.created_at AT TIME ZONE 'Europe/Tirane')::date) AS "days_ago?",
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = c.id AND m.sender_user_id <> $1
                     AND m.created_at > coalesce(me.last_read_at, '-infinity')) AS "unread!"
           FROM conversation_participants me
           JOIN conversations c ON c.id = me.conversation_id
           JOIN LATERAL (SELECT p.user_id FROM conversation_participants p
                         WHERE p.conversation_id = c.id AND p.user_id <> $1
                         ORDER BY p.joined_at
                         LIMIT 1) other ON true
           JOIN users u ON u.id = other.user_id
           LEFT JOIN listings l ON l.id = c.listing_id
           LEFT JOIN businesses b ON b.id = c.seller_business_id
           LEFT JOIN LATERAL (SELECT m.body, m.sender_user_id, m.created_at FROM messages m
                              WHERE m.conversation_id = c.id AND m.deleted_at IS NULL
                              ORDER BY m.id DESC
                              LIMIT 1) last ON true
           WHERE me.user_id = $1 AND me.archived_at IS NULL AND c.last_message_at IS NOT NULL
           ORDER BY c.last_message_at DESC
           LIMIT $2"#,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// How many of the user's conversations have messages from the other side they haven't read.
pub async fn unread_conversations(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "unread!"
           FROM conversation_participants me
           WHERE me.user_id = $1 AND me.archived_at IS NULL
             AND EXISTS (SELECT 1 FROM messages m
                         WHERE m.conversation_id = me.conversation_id AND m.sender_user_id <> $1
                           AND m.created_at > coalesce(me.last_read_at, '-infinity'))"#,
        user_id,
    )
    .fetch_one(db)
    .await
}
