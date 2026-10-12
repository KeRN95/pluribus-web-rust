//! What the sitemaps list (web/seo.rs): the active listings by their numbers, and the businesses
//! with listings.

use super::*;

/// An active listing's address in a sitemap, and when it last changed.
pub struct SitemapListing {
    pub public_no: i64,
    pub slug: String,
    pub updated_at: OffsetDateTime,
}

/// The highest number an active listing has, for how many listing sitemaps there are.
pub async fn top_active_number(db: &PgPool) -> sqlx::Result<Option<i64>> {
    sqlx::query_scalar!("SELECT max(public_no) FROM listings WHERE status = 'ACTIVE'").fetch_one(db).await
}

/// The active listings numbered from `from` up to (not including) `to`, in their order.
pub async fn active_listings_numbered(db: &PgPool, from: i64, to: i64) -> sqlx::Result<Vec<SitemapListing>> {
    sqlx::query_as!(
        SitemapListing,
        "SELECT public_no, slug, updated_at
         FROM listings
         WHERE status = 'ACTIVE' AND public_no >= $1 AND public_no < $2
         ORDER BY public_no",
        from,
        to
    )
    .fetch_all(db)
    .await
}

/// The slugs of the businesses with an active listing, whose pages are worth finding.
pub async fn businesses_with_listings(db: &PgPool) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        "SELECT b.slug
         FROM businesses b
         WHERE EXISTS (SELECT 1 FROM listings l WHERE l.seller_business_id = b.id AND l.status = 'ACTIVE')
         ORDER BY b.slug"
    )
    .fetch_all(db)
    .await
}
