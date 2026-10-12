//! Price drops (price_was! in mod.rs): what a listing's price dropped from, for its page, and
//! telling those who keep a listing that its price dropped: a notification each (notifications,
//! PRICE_DROP), which /te-ruajturat shows them, and the header's heart with a dot until they have
//! seen it there.

use super::*;

/// The price the listing dropped from, if it did lately (price_was!), for its page.
pub async fn price_was(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Option<Decimal>> {
    sqlx::query_scalar::<_, Option<Decimal>>(concat!("SELECT ", price_was!(), " FROM listings l WHERE l.id = $1"))
        .bind(listing_id)
        .fetch_one(db)
        .await
}

/// After its seller changed the listing (posting.rs, update_listing), in the same transaction:
/// when the change lowered its price (price_was!), everyone who keeps it gets a notification, with
/// what it was and what it is, in place of one about it they haven't seen yet, whose higher "was"
/// it keeps, the price they last saw; not its own seller, nor those who turned price drop alerts
/// off (notification_preferences.price_drop_alerts). Its payload has the listing's title too,
/// which the notification's says (domain::notices).
pub(super) async fn notify_price_drop(tx: &mut sqlx::PgConnection, listing_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(concat!(
        "WITH dropped AS (
             SELECT l.id, l.public_no, coalesce(l.generated_title, l.title) AS title, l.price_amount,
                    l.currency::text AS currency, l.created_by_user_id, ",
        price_was!(),
        " AS was
             FROM listings l
             WHERE l.id = $1
               -- Its price changed now, in this transaction, not before.
               AND (SELECT max(p.changed_at) FROM listing_price_history p WHERE p.listing_id = l.id) >= now()
         ),
         keepers AS (
             SELECT k.user_id, d.id, d.public_no, d.title, d.price_amount, d.currency,
                    greatest(d.was, (SELECT max((n.payload->>'was')::numeric)
                                     FROM notifications n
                                     WHERE n.user_id = k.user_id AND n.notification_type = 'PRICE_DROP'
                                       AND n.read_at IS NULL AND n.payload->>'listing_id' = d.id::text)) AS was
             FROM dropped d
             JOIN saved_listings k ON k.listing_id = d.id
             LEFT JOIN notification_preferences np ON np.user_id = k.user_id
             WHERE d.was IS NOT NULL AND k.user_id IS DISTINCT FROM d.created_by_user_id
               AND coalesce(np.price_drop_alerts, true)
         ),
         replaced AS (
             DELETE FROM notifications n
             USING keepers k
             WHERE n.user_id = k.user_id AND n.notification_type = 'PRICE_DROP' AND n.read_at IS NULL
               AND n.payload->>'listing_id' = k.id::text
         )
         INSERT INTO notifications (user_id, notification_type, title, payload)
         SELECT k.user_id, 'PRICE_DROP', left('Çmimi ra: ' || k.title, 180),
                jsonb_build_object('listing_id', k.id, 'public_no', k.public_no, 'title', k.title, 'was', k.was,
                                   'price', k.price_amount, 'currency', k.currency)
         FROM keepers k"
    ))
    .bind(listing_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// A listing the user keeps whose price dropped, which they haven't seen yet on /te-ruajturat:
/// what it was and what it became.
#[derive(Debug, sqlx::FromRow)]
pub struct PriceDrop {
    /// The notification's id, a UUIDv7: the later ones sort after.
    pub notice: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub was: Decimal,
    pub price: Decimal,
    pub currency: String,
}

/// The price drops the user hasn't seen yet, the latest first: of listings still on the site, which
/// they still keep; at most 20.
pub async fn price_drops(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<PriceDrop>> {
    sqlx::query_as(
        "SELECT n.id AS notice, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title,
                (n.payload->>'was')::numeric AS was, (n.payload->>'price')::numeric AS price,
                n.payload->>'currency' AS currency
         FROM notifications n
         JOIN listings l ON l.id = (n.payload->>'listing_id')::uuid
         JOIN saved_listings k ON k.user_id = n.user_id AND k.listing_id = l.id
         WHERE n.user_id = $1 AND n.notification_type = 'PRICE_DROP' AND n.read_at IS NULL
           AND l.status = 'ACTIVE'
         ORDER BY n.created_at DESC
         LIMIT 20",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// The user saw their price drops on /te-ruajturat, up to the latest it showed (`up_to`, a
/// notification's id): not one that came after, as a copy of the page shown again from the
/// browser's cache wouldn't show it. The header's heart loses its dot.
pub async fn price_drops_seen(db: &PgPool, user_id: Uuid, up_to: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notifications SET read_at = now()
         WHERE user_id = $1 AND notification_type = 'PRICE_DROP' AND read_at IS NULL AND id <= $2",
    )
    .bind(user_id)
    .bind(up_to)
    .execute(db)
    .await?;
    Ok(())
}
