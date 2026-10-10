//! Promotions (promotion_products, listing_promotions; migration 0040): what a listing can be
//! promoted with, and the promotions moderators give (web/moderation.rs) until there are
//! payments. Each is a row of listing_promotions, and what it does is copied onto the listing,
//! which every card reads: FEATURED into featured_until, for the row of them over a category's or
//! a search's results (featured_cards); HIGHLIGHT into highlighted_until, a card that stands out
//! wherever it shows; BUMP into bumped_at, back to the top of "newest first" at once, spent there
//! and then; CATEGORY_TOP into bumped_at too, and again every day while it lasts
//! (tick_promotions). One given again before it ends adds its time to what is left. None of it
//! changes what the listing says, so none of it moves updated_at, which the sitemaps give search
//! engines (app.keep_updated_at, migration 0032).

use super::{expiry::TOLD, review::record, *};

/// The kinds of promotion the site knows what to do with (promotion_products.promotion_type).
const KINDS: &str = "('FEATURED', 'HIGHLIGHT', 'BUMP', 'CATEGORY_TOP')";

/// A promotion that can be given: one on sale (promotion_products.active) of a kind the site
/// does something with.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PromotionProduct {
    pub code: String,
    pub name: String,
    pub promotion_type: String,
    /// How long it lasts; none for a bump, spent at once.
    pub duration_hours: Option<i32>,
    pub price_amount: Decimal,
    pub currency: String,
}

/// The promotions on sale, the cheapest first.
pub async fn promotion_products(db: &PgPool) -> sqlx::Result<Vec<PromotionProduct>> {
    sqlx::query_as(&format!(
        "SELECT code, name, promotion_type, duration_hours, price_amount, currency::text AS currency
         FROM promotion_products WHERE active AND promotion_type IN {KINDS}
         ORDER BY price_amount, code"
    ))
    .fetch_all(db)
    .await
}

/// A promotion on now, with its listing.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct OnPromotion {
    pub id: Uuid,
    pub listing_id: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    /// The promotion's name ("I veçuar për 7 ditë").
    pub name: String,
    pub promotion_type: String,
    pub ends_at: OffsetDateTime,
}

const ON_PROMOTION: &str = "SELECT p.id, l.id AS listing_id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS title,
                                   pr.name, pr.promotion_type, p.ends_at
                            FROM listing_promotions p
                            JOIN promotion_products pr ON pr.id = p.promotion_product_id
                            JOIN listings l ON l.id = p.listing_id
                            WHERE p.status = 'ACTIVE' AND p.ends_at > now()";

/// Every promotion on now, those ending soonest first (/moderim).
pub async fn promotions_on(db: &PgPool) -> sqlx::Result<Vec<OnPromotion>> {
    sqlx::query_as(&format!("{ON_PROMOTION} ORDER BY p.ends_at, p.id")).fetch_all(db).await
}

/// The promotions on now of the listings with these numbers, those ending soonest first: their
/// seller's pages say so (the listing's own, "Njoftimet e mia").
pub async fn listing_promotions(db: &PgPool, numbers: &[i64]) -> sqlx::Result<Vec<OnPromotion>> {
    if numbers.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as(&format!("{ON_PROMOTION} AND l.public_no = ANY($1) ORDER BY p.ends_at, p.id"))
        .bind(numbers)
        .fetch_all(db)
        .await
}

/// What came of a moderator's giving a promotion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granted {
    /// Given, lasting until then; a bump, spent at once, has no end.
    Given(Option<OffsetDateTime>),
    /// No listing up or reserved has that number.
    NoListing,
    /// No promotion on sale has that code.
    NoProduct,
}

/// A moderator gives the listing numbered `public_no` the promotion `code`: what it does is done
/// at once, its seller hears of it (a notification, PROMOTED, to whoever keeps the listing: TOLD,
/// db/expiry.rs), and the moderators' record keeps who gave it.
pub async fn grant_promotion(db: &PgPool, public_no: i64, code: &str, moderator: Uuid) -> sqlx::Result<Granted> {
    let mut tx = db.begin().await?;
    keep_updated_at(&mut tx).await?;
    let product: Option<(Uuid, String, String, Option<i32>)> = sqlx::query_as(&format!(
        "SELECT id, name, promotion_type, duration_hours FROM promotion_products
         WHERE code = $1 AND active AND promotion_type IN {KINDS}"
    ))
    .bind(code)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((product_id, name, kind, hours)) = product else {
        return Ok(Granted::NoProduct);
    };
    let listing: Option<(Uuid, String, Uuid, Option<Uuid>)> = sqlx::query_as(&format!(
        "SELECT l.id, coalesce(l.generated_title, l.title), l.created_by_user_id, {TOLD}
         FROM listings l WHERE l.public_no = $1 AND l.status IN ('ACTIVE', 'RESERVED') FOR UPDATE"
    ))
    .bind(public_no)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((listing_id, title, owner, keeper)) = listing else {
        return Ok(Granted::NoListing);
    };
    // Until when: its hours on top of what is left of one of its kind on the listing.
    let ends_at: Option<OffsetDateTime> = match (kind.as_str(), hours) {
        ("BUMP", _) | (_, None) => None,
        (kind, Some(hours)) => {
            let left = match kind {
                "FEATURED" => "SELECT featured_until FROM listings WHERE id = $1",
                "HIGHLIGHT" => "SELECT highlighted_until FROM listings WHERE id = $1",
                _ => {
                    "SELECT max(p.ends_at) FROM listing_promotions p
                     JOIN promotion_products pr ON pr.id = p.promotion_product_id
                     WHERE p.listing_id = $1 AND p.status = 'ACTIVE' AND pr.promotion_type = 'CATEGORY_TOP'"
                }
            };
            Some(
                sqlx::query_scalar(&format!("SELECT greatest(now(), coalesce(({left}), now())) + make_interval(hours => $2)"))
                    .bind(listing_id)
                    .bind(hours)
                    .fetch_one(&mut *tx)
                    .await?,
            )
        }
    };
    match (kind.as_str(), ends_at) {
        ("FEATURED", Some(until)) => {
            sqlx::query("UPDATE listings SET featured_until = $2 WHERE id = $1").bind(listing_id).bind(until).execute(&mut *tx).await?;
        }
        ("HIGHLIGHT", Some(until)) => {
            sqlx::query("UPDATE listings SET highlighted_until = $2 WHERE id = $1").bind(listing_id).bind(until).execute(&mut *tx).await?;
        }
        // A bump, or a week at the top, which tick_promotions brings back up every day after.
        _ => {
            sqlx::query("UPDATE listings SET bumped_at = now() WHERE id = $1").bind(listing_id).execute(&mut *tx).await?;
        }
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO listing_promotions (listing_id, promotion_product_id, starts_at, ends_at, status)
         VALUES ($1, $2, now(), $3, CASE WHEN $3::timestamptz IS NULL THEN 'EXPIRED' ELSE 'ACTIVE' END)
         RETURNING id",
    )
    .bind(listing_id)
    .bind(product_id)
    .bind(ends_at)
    .fetch_one(&mut *tx)
    .await?;
    if let Some(keeper) = keeper {
        sqlx::query(
            "INSERT INTO notifications (user_id, notification_type, title, payload)
             VALUES ($1, 'PROMOTED', left($2 || ': ' || $3, 180), jsonb_build_object('listing_id', $4::uuid, 'public_no', $5::bigint))",
        )
        .bind(keeper)
        .bind(&name)
        .bind(&title)
        .bind(listing_id)
        .bind(public_no)
        .execute(&mut *tx)
        .await?;
    }
    record(&mut tx, moderator, Some(listing_id), owner, "GRANT_PROMOTION", &name, serde_json::json!({ "promotion": id.to_string(), "product": code }))
        .await?;
    tx.commit().await?;
    Ok(Granted::Given(ends_at))
}

/// A moderator ends a promotion before its time: the listing loses what it gave, keeping what
/// another of its kind on it still gives. False when it wasn't on.
pub async fn end_promotion(db: &PgPool, id: Uuid, moderator: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    keep_updated_at(&mut tx).await?;
    let ended: Option<(Uuid, Uuid, String, String)> = sqlx::query_as(
        "UPDATE listing_promotions p SET status = 'CANCELLED'
         FROM promotion_products pr, listings l
         WHERE p.id = $1 AND p.status = 'ACTIVE' AND pr.id = p.promotion_product_id AND l.id = p.listing_id
         RETURNING p.listing_id, l.created_by_user_id, pr.promotion_type, pr.code",
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((listing_id, owner, kind, code)) = ended else {
        return Ok(false);
    };
    let column = match kind.as_str() {
        "FEATURED" => Some("featured_until"),
        "HIGHLIGHT" => Some("highlighted_until"),
        _ => None,
    };
    if let Some(column) = column {
        sqlx::query(&format!(
            "UPDATE listings SET {column} = (SELECT max(p.ends_at) FROM listing_promotions p
                                             JOIN promotion_products pr ON pr.id = p.promotion_product_id
                                             WHERE p.listing_id = $1 AND p.status = 'ACTIVE' AND p.ends_at > now()
                                               AND pr.promotion_type = $2)
             WHERE id = $1"
        ))
        .bind(listing_id)
        .bind(&kind)
        .execute(&mut *tx)
        .await?;
    }
    record(&mut tx, moderator, Some(listing_id), owner, "END_PROMOTION", "", serde_json::json!({ "promotion": id.to_string(), "product": code }))
        .await?;
    tx.commit().await?;
    Ok(true)
}

/// The promotions whose time is up end (EXPIRED), and a listing at the top of its category for a
/// while goes back to the top of "newest first" once a day (CATEGORY_TOP), from the promotions
/// on, never a scan of the listings. Returns how many of each.
pub async fn tick_promotions(db: &PgPool) -> sqlx::Result<(u64, u64)> {
    let mut tx = db.begin().await?;
    keep_updated_at(&mut tx).await?;
    let ended = sqlx::query("UPDATE listing_promotions SET status = 'EXPIRED' WHERE status = 'ACTIVE' AND ends_at <= now()")
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let topped = sqlx::query(
        "UPDATE listings l SET bumped_at = now()
         FROM listing_promotions p JOIN promotion_products pr ON pr.id = p.promotion_product_id
         WHERE l.id = p.listing_id AND p.status = 'ACTIVE' AND p.ends_at > now() AND pr.promotion_type = 'CATEGORY_TOP'
           AND l.status = 'ACTIVE' AND l.sort_at < now() - interval '1 day'",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    tx.commit().await?;
    Ok((ended, topped))
}

/// What the transaction changes on the listings leaves updated_at as it was (migration 0032).
async fn keep_updated_at(tx: &mut sqlx::Transaction<'_, Postgres>) -> sqlx::Result<()> {
    sqlx::query("SELECT set_config('app.keep_updated_at', 'on', true)").execute(&mut **tx).await?;
    Ok(())
}
