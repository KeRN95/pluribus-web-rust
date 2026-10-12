//! Messages between a listing's buyers and its seller: a buyer starts a conversation about a
//! listing, and the two sides write in it (conversations, conversation_participants and messages,
//! from the first migration). The seller's side is the private seller, or for a store's listing
//! every one of its people (business_members), whose inbox it shares: the one who posted the
//! listing takes part from the start, and the others as they write or read in it; who left the
//! store sees it no more. A message is the other side's by who sent it, the buyer or anyone on
//! the seller's side, so that a colleague's answer is "ours". Times are timestamps, as the
//! database keeps them (timestamptz, OffsetDateTime), which the pages put on Albania's clock
//! (web/messages.rs).

use super::*;
use pluribus_core::domain::messaging::SentLately;

/// The most messages a conversation's page shows: its latest.
const MESSAGES_SHOWN: i64 = 300;

/// Rebuilds how quickly each seller answers (seller_response_times, migration 0036), which their
/// listings' card and their profile say; pages go on reading the old figures meanwhile.
pub async fn refresh_response_times(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query!("REFRESH MATERIALIZED VIEW CONCURRENTLY seller_response_times").execute(db).await?;
    Ok(())
}

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
    /// Who answers for the seller first: the private seller, or for a store's listing whoever
    /// posted it while they are one of its people, else its owner.
    pub seller_side_id: Uuid,
    /// The business's name, or the seller's.
    pub seller_name: String,
    /// Whether the one asking is on the seller's side themself: the seller, or one of the store's
    /// people, who don't write to themselves.
    pub own: bool,
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
                  CASE WHEN l.seller_business_id IS NULL THEN l.seller_user_id
                       WHEN EXISTS (SELECT 1 FROM business_members m
                                    WHERE m.business_id = l.seller_business_id AND m.user_id = l.created_by_user_id)
                       THEN l.created_by_user_id
                       ELSE coalesce((SELECT m.user_id FROM business_members m
                                      WHERE m.business_id = l.seller_business_id AND m.role = 'OWNER'), l.created_by_user_id)
                  END AS "seller_side_id!",
                  coalesce(b.display_name, u.display_name) AS "seller_name!",
                  (coalesce(l.seller_user_id = $2, false)
                   OR EXISTS (SELECT 1 FROM business_members m WHERE m.business_id = l.seller_business_id AND m.user_id = $2)) AS "own!",
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

/// What an account sent lately, for the limits on sending (domain::messaging::refuse): messages in
/// the last minute and day, and conversations it started (sellers it wrote to) in the last hour
/// and day.
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

/// Whether `buyer` blocked the listing's seller's side, and whether that side blocked them: its
/// seller (`seller_side_id`), or for a store's listing any one of its people.
pub async fn blocks_with_seller(db: &PgPool, buyer: Uuid, target: &MessageTarget) -> sqlx::Result<(bool, bool)> {
    let row = sqlx::query!(
        r#"WITH side AS (SELECT $2::uuid AS user_id
                         UNION SELECT m.user_id FROM business_members m WHERE m.business_id = $3)
           SELECT EXISTS (SELECT 1 FROM user_blocks k JOIN side ON side.user_id = k.blocked_user_id
                          WHERE k.blocker_user_id = $1) AS "blocked!",
                  EXISTS (SELECT 1 FROM user_blocks k JOIN side ON side.user_id = k.blocker_user_id
                          WHERE k.blocked_user_id = $1) AS "blocked_by!""#,
        buyer,
        target.seller_side_id,
        target.seller_business_id,
    )
    .fetch_one(db)
    .await?;
    Ok((row.blocked, row.blocked_by))
}

/// Blocks `other` for `user`, or with `on` false lets them write again.
pub async fn set_user_blocked(db: &PgPool, user: Uuid, other: Uuid, on: bool) -> sqlx::Result<()> {
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

/// In a conversation, `user` blocks the other side (`other`: the buyer, or for the buyer the
/// seller who answered first), or with `on` false lets them write again: lifting every block
/// between the buyer and the seller's side, so that a store's people lift one another's.
pub async fn set_conversation_blocked(db: &PgPool, conversation_id: Uuid, user: Uuid, other: Uuid, on: bool) -> sqlx::Result<()> {
    if on {
        return set_user_blocked(db, user, other, true).await;
    }
    sqlx::query!(
        "DELETE FROM user_blocks k
         USING conversations c, conversation_participants p
         WHERE c.id = $1 AND p.conversation_id = c.id AND p.user_id <> c.buyer_user_id
           AND ((k.blocker_user_id = c.buyer_user_id AND k.blocked_user_id = p.user_id)
                OR (k.blocker_user_id = p.user_id AND k.blocked_user_id = c.buyer_user_id))",
        conversation_id,
    )
    .execute(db)
    .await?;
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

/// Writes a message in a conversation its sender may see (`conversation`).
pub async fn send_message(db: &PgPool, conversation_id: Uuid, sender_id: Uuid, body: &str) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    write(&mut tx, conversation_id, sender_id, body).await?;
    tx.commit().await
}

/// The message, the conversation moved to the top of both inboxes, and everything in it read by
/// its sender, who just wrote in it: one of a store's people takes part from their first.
async fn write(tx: &mut sqlx::Transaction<'_, Postgres>, conversation_id: Uuid, sender_id: Uuid, body: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO conversation_participants (conversation_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        conversation_id,
        sender_id,
    )
    .execute(&mut **tx)
    .await?;
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
    /// The other side: the seller, as the buyer sees them (a business's name), or the buyer; by
    /// their account, the seller who answered first.
    pub other_id: Uuid,
    pub other_name: String,
    /// For one of a store's people, the store whose conversation it is.
    pub store: Option<String>,
    /// Whether the other side has written in it yet, and how many messages the user's side sent
    /// since they last did: the limits until an answer comes (web/messages.rs).
    pub answered: bool,
    pub in_a_row: i64,
    /// Whether the user's side blocked the other, and the other side the user's: on a store's
    /// side, anyone of its people who took part.
    pub blocked: bool,
    pub blocked_by: bool,
}

/// The conversation, for whoever may see it: its buyer, its private seller, or one of the store's
/// people whose listing it is about (who left the store sees it no more); None for anyone else.
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
                  CASE WHEN c.buyer_user_id = $2 THEN seller.user_id ELSE c.buyer_user_id END AS "other_id!",
                  CASE WHEN c.buyer_user_id = $2 THEN coalesce(b.display_name, su.display_name)
                       ELSE bu.display_name END AS "other_name!",
                  CASE WHEN c.buyer_user_id <> $2 THEN b.display_name END AS store,
                  EXISTS (SELECT 1 FROM messages m
                          WHERE m.conversation_id = c.id
                            AND CASE WHEN c.buyer_user_id = $2 THEN m.sender_user_id <> c.buyer_user_id
                                     ELSE m.sender_user_id = c.buyer_user_id END) AS "answered!",
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = c.id
                     AND CASE WHEN c.buyer_user_id = $2 THEN m.sender_user_id = c.buyer_user_id
                              ELSE m.sender_user_id <> c.buyer_user_id END
                     AND m.created_at > coalesce((SELECT max(o.created_at) FROM messages o
                                                  WHERE o.conversation_id = c.id
                                                    AND CASE WHEN c.buyer_user_id = $2 THEN o.sender_user_id <> c.buyer_user_id
                                                             ELSE o.sender_user_id = c.buyer_user_id END),
                                                 '-infinity')) AS "in_a_row!",
                  CASE WHEN c.buyer_user_id = $2 THEN blocks.by_buyer ELSE blocks.by_seller END AS "blocked!",
                  CASE WHEN c.buyer_user_id = $2 THEN blocks.by_seller ELSE blocks.by_buyer END AS "blocked_by!"
           FROM conversations c
           JOIN users bu ON bu.id = c.buyer_user_id
           JOIN LATERAL (SELECT p.user_id FROM conversation_participants p
                         WHERE p.conversation_id = c.id AND p.user_id <> c.buyer_user_id
                         ORDER BY p.joined_at
                         LIMIT 1) seller ON true
           JOIN users su ON su.id = seller.user_id
           CROSS JOIN LATERAL (
               SELECT EXISTS (SELECT 1 FROM user_blocks k
                              JOIN conversation_participants p ON p.conversation_id = c.id AND p.user_id = k.blocked_user_id
                              WHERE k.blocker_user_id = c.buyer_user_id) AS by_buyer,
                      EXISTS (SELECT 1 FROM user_blocks k
                              JOIN conversation_participants p ON p.conversation_id = c.id AND p.user_id = k.blocker_user_id
                              WHERE k.blocked_user_id = c.buyer_user_id) AS by_seller
           ) blocks
           LEFT JOIN listings l ON l.id = c.listing_id
           LEFT JOIN businesses b ON b.id = c.seller_business_id
           WHERE c.id = $1
             AND (c.buyer_user_id = $2 OR c.seller_user_id = $2
                  OR EXISTS (SELECT 1 FROM business_members bm WHERE bm.business_id = c.seller_business_id AND bm.user_id = $2))"#,
        id,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Whether the user may see the conversation, as `conversation` has it.
pub async fn may_see(db: &PgPool, conversation_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        r#"SELECT EXISTS (SELECT 1 FROM conversations c
                          WHERE c.id = $1
                            AND (c.buyer_user_id = $2 OR c.seller_user_id = $2
                                 OR EXISTS (SELECT 1 FROM business_members bm
                                            WHERE bm.business_id = c.seller_business_id AND bm.user_id = $2))) AS "may_see!""#,
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
    /// Sent by the user reading it's side: by them, or by one of the store's people they are one of.
    pub mine: bool,
    /// For one of a store's people, the colleague who sent it, when it wasn't them.
    pub author: Option<String>,
    pub body: String,
    /// When it was sent.
    pub created_at: OffsetDateTime,
}

/// A conversation's messages, the oldest first: its latest MESSAGES_SHOWN, or those after the
/// message `after` (ids grow with time, uuid_v7).
pub async fn messages(db: &PgPool, conversation_id: Uuid, user_id: Uuid, after: Option<Uuid>) -> sqlx::Result<Vec<Message>> {
    let mut messages = sqlx::query_as!(
        Message,
        r#"SELECT m.id,
                  coalesce(CASE WHEN c.buyer_user_id = $2 THEN m.sender_user_id = c.buyer_user_id
                                ELSE m.sender_user_id <> c.buyer_user_id END, false) AS "mine!",
                  CASE WHEN c.buyer_user_id <> $2 AND m.sender_user_id <> c.buyer_user_id AND m.sender_user_id <> $2
                       THEN u.display_name END AS "author?",
                  coalesce(m.body, '') AS "body!",
                  m.created_at
           FROM messages m
           JOIN conversations c ON c.id = m.conversation_id
           LEFT JOIN users u ON u.id = m.sender_user_id
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

/// The user has read the conversation up to the message `upto`, which their page showed: one of a
/// store's people takes part from then, if they didn't yet. With nothing from the other side left
/// unread, the notices they had of it (notices.rs, NEW_MESSAGE) are read too, on /lajmerimet.
pub async fn mark_read(db: &PgPool, conversation_id: Uuid, user_id: Uuid, upto: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO conversation_participants AS p (conversation_id, user_id, last_read_at)
         SELECT $1, $2, m.created_at FROM messages m WHERE m.id = $3 AND m.conversation_id = $1
         ON CONFLICT (conversation_id, user_id) DO UPDATE SET last_read_at = excluded.last_read_at
         WHERE p.last_read_at IS NULL OR p.last_read_at < excluded.last_read_at",
        conversation_id,
        user_id,
        upto,
    )
    .execute(db)
    .await?;
    sqlx::query!(
        "UPDATE notifications n SET read_at = now()
         WHERE n.user_id = $2 AND n.notification_type = 'NEW_MESSAGE' AND n.read_at IS NULL
           AND n.payload ->> 'conversation' = $1::uuid::text
           AND NOT EXISTS (SELECT 1 FROM messages m
                           JOIN conversations c ON c.id = m.conversation_id
                           JOIN conversation_participants p ON p.conversation_id = m.conversation_id AND p.user_id = $2
                           WHERE m.conversation_id = $1
                             AND CASE WHEN c.buyer_user_id = $2 THEN m.sender_user_id <> c.buyer_user_id
                                      ELSE m.sender_user_id = c.buyer_user_id END
                             AND m.created_at > coalesce(p.last_read_at, '-infinity'))",
        conversation_id,
        user_id,
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
    /// For one of a store's people, the store whose conversation it is.
    pub store: Option<String>,
    /// Its latest message: what it says, whether the user sent it (or which colleague did, on a
    /// store's side), and when it was sent (that message's created_at, as Message has it).
    pub last_body: Option<String>,
    pub last_mine: bool,
    pub last_author: Option<String>,
    pub last_message_at: Option<OffsetDateTime>,
    /// Messages from the other side the user hasn't read: for one of a store's people, since they
    /// last read it, or until they do since they joined the store.
    pub unread: i64,
}

/// The user's conversations, as `conversation` has who may see them, the latest written in first.
pub async fn inbox(db: &PgPool, user_id: Uuid, limit: i64) -> sqlx::Result<Vec<InboxEntry>> {
    sqlx::query_as!(
        InboxEntry,
        r#"SELECT c.id, coalesce(l.generated_title, l.title) AS listing_title,
                  (SELECT coalesce(m.thumbnail_url, m.public_url)
                   FROM listing_media m
                   WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
                   ORDER BY m.is_cover DESC, m.sort_order
                   LIMIT 1) AS photo,
                  CASE WHEN c.buyer_user_id = $1 THEN coalesce(b.display_name, su.display_name)
                       ELSE bu.display_name END AS "other_name!",
                  CASE WHEN c.buyer_user_id <> $1 THEN b.display_name END AS store,
                  last.body AS "last_body?", coalesce(last.sender_user_id = $1, false) AS "last_mine!",
                  CASE WHEN c.buyer_user_id <> $1 AND last.sender_user_id <> c.buyer_user_id AND last.sender_user_id <> $1
                       THEN last.author END AS "last_author?",
                  last.created_at AS "last_message_at?",
                  (SELECT count(*) FROM messages m
                   WHERE m.conversation_id = c.id
                     AND CASE WHEN c.buyer_user_id = $1 THEN m.sender_user_id <> c.buyer_user_id
                              ELSE m.sender_user_id = c.buyer_user_id END
                     AND m.created_at > coalesce(me.last_read_at, joined.at, '-infinity')) AS "unread!"
           FROM conversations c
           JOIN users bu ON bu.id = c.buyer_user_id
           JOIN LATERAL (SELECT p.user_id FROM conversation_participants p
                         WHERE p.conversation_id = c.id AND p.user_id <> c.buyer_user_id
                         ORDER BY p.joined_at
                         LIMIT 1) seller ON true
           JOIN users su ON su.id = seller.user_id
           LEFT JOIN conversation_participants me ON me.conversation_id = c.id AND me.user_id = $1
           LEFT JOIN LATERAL (SELECT bm.created_at AS at FROM business_members bm
                              WHERE bm.business_id = c.seller_business_id AND bm.user_id = $1) joined ON true
           LEFT JOIN listings l ON l.id = c.listing_id
           LEFT JOIN businesses b ON b.id = c.seller_business_id
           LEFT JOIN LATERAL (SELECT m.body, m.sender_user_id, m.created_at, a.display_name AS author
                              FROM messages m
                              LEFT JOIN users a ON a.id = m.sender_user_id
                              WHERE m.conversation_id = c.id AND m.deleted_at IS NULL
                              ORDER BY m.id DESC
                              LIMIT 1) last ON true
           WHERE (c.buyer_user_id = $1 OR c.seller_user_id = $1
                  OR c.seller_business_id = ANY (ARRAY(SELECT bm.business_id FROM business_members bm WHERE bm.user_id = $1)))
             AND me.archived_at IS NULL AND c.last_message_at IS NOT NULL
           ORDER BY c.last_message_at DESC
           LIMIT $2"#,
        user_id,
        limit,
    )
    .fetch_all(db)
    .await
}

/// How many of the user's conversations have messages from the other side they haven't read, as
/// the inbox counts them.
pub async fn unread_conversations(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "unread!"
           FROM conversations c
           LEFT JOIN conversation_participants me ON me.conversation_id = c.id AND me.user_id = $1
           LEFT JOIN LATERAL (SELECT bm.created_at AS at FROM business_members bm
                              WHERE bm.business_id = c.seller_business_id AND bm.user_id = $1) joined ON true
           WHERE (c.buyer_user_id = $1 OR c.seller_user_id = $1
                  OR c.seller_business_id = ANY (ARRAY(SELECT bm.business_id FROM business_members bm WHERE bm.user_id = $1)))
             AND me.archived_at IS NULL
             AND EXISTS (SELECT 1 FROM messages m
                         WHERE m.conversation_id = c.id
                           AND CASE WHEN c.buyer_user_id = $1 THEN m.sender_user_id <> c.buyer_user_id
                                    ELSE m.sender_user_id = c.buyer_user_id END
                           AND m.created_at > coalesce(me.last_read_at, joined.at, '-infinity'))"#,
        user_id,
    )
    .fetch_one(db)
    .await
}
