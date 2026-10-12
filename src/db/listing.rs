//! One listing: its page, its photos and properties, and the contact it reveals.

use super::*;
use crate::format;

#[derive(Debug)]
pub struct ListingDetail {
    pub id: Uuid,
    pub public_no: i64,
    pub slug: String,
    /// What the page calls it: a car's title made from what it is (generated_title), or the
    /// seller's.
    pub title: String,
    /// The seller's own title, when the page shows a made one: it heads the description.
    pub heading: Option<String>,
    pub description: String,
    pub status: String,
    pub listing_type: String,
    pub condition: Option<String>,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    /// A property's area in square metres (property_area), for its price per square metre.
    pub area_m2: Option<Decimal>,
    pub category_id: Uuid,
    pub category_name: String,
    pub category_slug: String,
    /// Its place: the town, or the zone and the town ("Blloku, Tiranë").
    pub location_name: Option<String>,
    /// The zone in the town it is in, for a property that says one.
    pub zone: Option<String>,
    /// Its point on the map: its place's, the town's for a zone without one (posting.rs).
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// What a square metre costs in its zone on average, in euros, among the zone's sales in its
    /// category (zone_area_prices), and how many sales that is; None outside a zone, or where too
    /// few listings make an average.
    pub zone_average_eur: Option<Decimal>,
    pub zone_listings: Option<i64>,
    /// What a unit of its currency is worth in euros (exchange_rates): the zone's average is in
    /// euros.
    pub eur_rate: Option<Decimal>,
    pub published_at: Option<OffsetDateTime>,
    /// When it expires, if it does (posting.rs, LISTING_DAYS).
    pub expires_at: Option<OffsetDateTime>,
    /// The seller: a business's name, or the private seller's.
    pub seller_name: String,
    /// Whose page theirs is: a store's, by its slug, or else the private seller's account (a
    /// listing has the one or the other).
    pub seller_business_slug: Option<String>,
    pub seller_user_id: Option<Uuid>,
    /// Their page's address, made from those two once the row is read (format::seller_path): the
    /// query leaves it empty.
    pub seller_url: String,
    /// A store's logo (stores.rs), which its card shows in place of the initial.
    pub seller_logo: Option<String>,
    pub seller_since: OffsetDateTime,
    pub seller_listings: i64,
    /// The account that posted it: its seller, or one of a business's people.
    pub created_by_user_id: Uuid,
    /// The store it is posted as, whose people who post may change it (manages_listing).
    pub seller_business_id: Option<Uuid>,
    /// Why the moderators refused it, which its seller is told.
    pub rejection_reason: Option<String>,
    /// How quickly the seller usually answers: the middle time, in seconds, from a buyer's first
    /// message to their first answer over the last 90 days (seller_response_times), once they had
    /// three conversations and answered half of them at least.
    pub seller_answers: Option<f64>,
    /// When the seller was last on the site: the private seller, or the latest of a store's
    /// people.
    pub seller_seen: Option<OffsetDateTime>,
    /// Whether the number (or address) it shows is one the account that posted it signed in with,
    /// proved theirs by the code (user_verifications).
    pub contact_verified: bool,
}

#[derive(Debug)]
pub struct AttributeValue {
    pub code: String,
    pub label: String,
    /// Its value as the page shows it; a multi-select's options, each on its own.
    pub shown: Vec<String>,
    pub multi: bool,
    /// One of the category's key facts: shown on its cards too (show_in_card), and its icon lit
    /// among the listing's details, in a small round pill of its own.
    pub key: bool,
}

#[derive(Debug)]
pub struct Photo {
    /// The full photo as every browser and link preview shows it: a photo kept as WebP (since
    /// migration 0025) has a JPEG copy for them; others are a JPEG already, or from elsewhere.
    pub url: String,
    /// Its card-sized copy, which the listing page blurs to fill the frame around a photo kept
    /// before WebP.
    pub card_url: String,
    /// A photo kept as WebP: the full copy and the middle one (none for a photo no bigger), which
    /// the browser picks between with the card's and the small one, whose address is the card's
    /// with its own ending (srcset, web/photos.rs), and the tiny one as a data: URI, which the page
    /// blurs behind it.
    pub webp_url: Option<String>,
    pub middle_url: Option<String>,
    pub placeholder: Option<String>,
    pub width_px: Option<i32>,
    pub height_px: Option<i32>,
}

/// Sold and reserved listings stay viewable, so shared links keep working; those the reports or
/// the moderators took off the site, those their seller took down for a while (PAUSED) and those
/// that expired come too, for their seller and the moderators alone (web/listings.rs).
pub async fn listing_by_public_no(
    db: &PgPool,
    public_no: i64,
) -> sqlx::Result<Option<ListingDetail>> {
    let listing = sqlx::query_as!(
        ListingDetail,
        r#"SELECT l.id, l.public_no, l.slug, coalesce(l.generated_title, l.title) AS "title!",
                  CASE WHEN l.generated_title IS NOT NULL THEN l.title END AS heading,
                  l.description, l.status, l.listing_type,
                  l.condition, l.price_amount, l.currency::text AS "currency!", l.price_type,
                  (SELECT v.value_number FROM listing_attribute_values v
                   JOIN attributes a ON a.id = v.attribute_id
                   WHERE v.listing_id = l.id AND a.code = 'property_area') AS "area_m2?",
                  c.id AS category_id, c.name AS category_name, c.slug AS category_slug,
                  CASE WHEN loc.location_type = 'AREA'
                       THEN loc.name || ', ' || (SELECT town.name FROM locations town WHERE town.id = location_municipality(loc.id))
                       ELSE loc.name END AS "location_name?",
                  CASE WHEN loc.location_type = 'AREA' THEN loc.name END AS "zone?",
                  l.latitude::float8 AS "latitude?", l.longitude::float8 AS "longitude?",
                  zone_price.average_eur AS "zone_average_eur?", zone_price.listings AS "zone_listings?",
                  (SELECT r.eur_rate FROM exchange_rates r WHERE r.currency = l.currency) AS "eur_rate?",
                  l.published_at, l.expires_at,
                  coalesce(b.display_name, u.display_name) AS "seller_name!",
                  b.slug AS "seller_business_slug?", l.seller_user_id, '' AS "seller_url!",
                  b.logo_url AS "seller_logo?",
                  coalesce(b.created_at, u.created_at) AS "seller_since!",
                  (SELECT count(*) FROM listings o
                   WHERE o.status = 'ACTIVE'
                     AND (o.seller_business_id = l.seller_business_id OR o.seller_user_id = l.seller_user_id)) AS "seller_listings!",
                  l.created_by_user_id, l.seller_business_id, l.rejection_reason,
                  (SELECT r.median_seconds FROM seller_response_times r
                   WHERE r.seller_id = coalesce(l.seller_business_id, l.seller_user_id) AND r.conversations >= 3 AND r.answered * 2 >= r.conversations) AS "seller_answers?",
                  CASE WHEN l.seller_business_id IS NULL THEN u.last_seen_at
                       ELSE (SELECT max(mu.last_seen_at) FROM business_members bm JOIN users mu ON mu.id = bm.user_id
                             WHERE bm.business_id = l.seller_business_id) END AS "seller_seen?",
                  coalesce((SELECT CASE WHEN c.show_phone AND c.phone_e164 IS NOT NULL
                                        THEN EXISTS (SELECT 1 FROM user_verifications v
                                                     WHERE v.user_id = l.created_by_user_id AND v.status = 'VERIFIED'
                                                       AND v.verification_type = 'PHONE' AND v.verified_value = c.phone_e164::text)
                                        WHEN c.show_email AND c.email IS NOT NULL
                                        THEN EXISTS (SELECT 1 FROM user_verifications v
                                                     WHERE v.user_id = l.created_by_user_id AND v.status = 'VERIFIED'
                                                       AND v.verification_type = 'EMAIL' AND lower(v.verified_value) = lower(c.email::text))
                                        ELSE false END
                            FROM listing_contacts c WHERE c.listing_id = l.id), false) AS "contact_verified!"
           FROM listings l
           JOIN categories c ON c.id = l.category_id
           LEFT JOIN locations loc ON loc.id = l.location_id
           LEFT JOIN zone_area_prices zone_price ON zone_price.zone_id = l.location_id AND zone_price.category_id = l.category_id
           LEFT JOIN users u ON u.id = l.seller_user_id
           LEFT JOIN businesses b ON b.id = l.seller_business_id
           WHERE l.public_no = $1
             AND l.status IN ('ACTIVE', 'RESERVED', 'SOLD', 'PAUSED', 'EXPIRED', 'SUSPENDED', 'REJECTED', 'PENDING_REVIEW')"#,
        public_no
    )
    .fetch_optional(db)
    .await?;
    Ok(listing.map(|mut listing| {
        listing.seller_url = format::seller_path(listing.seller_business_slug.as_deref(), listing.seller_user_id);
        listing
    }))
}

/// Property labels and display values, single values and multi-select lists alike, in the order
/// the listing's category defines; a box ticked shows its name ("Me garanci"). Only properties the
/// category shows in details are included.
pub async fn listing_attributes(
    db: &PgPool,
    listing_id: Uuid,
    category_id: Uuid,
) -> sqlx::Result<Vec<AttributeValue>> {
    sqlx::query_as!(
        AttributeValue,
        r#"SELECT code AS "code!", label AS "label!", shown AS "shown!", multi AS "multi!", key AS "key!"
           FROM (
               SELECT a.code, coalesce(eca.label, a.name) AS label,
                      ARRAY[coalesce(o.label, v.value_text, format_attribute_number(v.value_number, a.unit),
                                     CASE v.value_boolean WHEN true THEN coalesce(eca.label, a.name) WHEN false THEN 'Jo' END,
                                     to_char(v.value_date, 'DD.MM.YYYY'))] AS shown,
                      false AS multi, eca.show_in_card AS key,
                      eca.sort_order
               FROM listing_attribute_values v
               JOIN attributes a ON a.id = v.attribute_id
               JOIN effective_category_attributes eca
                 ON eca.category_id = $2 AND eca.attribute_id = v.attribute_id AND eca.show_in_details
               LEFT JOIN attribute_options o ON o.id = v.value_option_id
               WHERE v.listing_id = $1
               UNION ALL
               SELECT a.code, coalesce(eca.label, a.name), array_agg(o.label ORDER BY o.sort_order), true,
                      eca.show_in_card, eca.sort_order
               FROM listing_attribute_options lo
               JOIN attributes a ON a.id = lo.attribute_id
               JOIN effective_category_attributes eca
                 ON eca.category_id = $2 AND eca.attribute_id = lo.attribute_id AND eca.show_in_details
               JOIN attribute_options o ON o.id = lo.option_id
               WHERE lo.listing_id = $1
               GROUP BY a.id, a.code, a.name, eca.label, eca.show_in_card, eca.sort_order
           ) attrs
           ORDER BY sort_order, label"#,
        listing_id,
        category_id
    )
    .fetch_all(db)
    .await
}

pub async fn listing_photos(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<Photo>> {
    sqlx::query_as!(
        Photo,
        r#"SELECT coalesce(jpeg_url, public_url, thumbnail_url) AS "url!", coalesce(thumbnail_url, public_url) AS "card_url!",
                  CASE WHEN mime_type = 'image/webp' THEN public_url END AS "webp_url?", middle_url, placeholder,
                  width_px, height_px
           FROM listing_media
           WHERE listing_id = $1 AND status = 'READY' AND media_type = 'IMAGE'
             AND coalesce(public_url, thumbnail_url) IS NOT NULL
           ORDER BY is_cover DESC, sort_order"#,
        listing_id
    )
    .fetch_all(db)
    .await
}

/// A photo stored for a listing (media.rs): its key in the store, the addresses of its copies (the
/// full WebP, the middle one if any, the card's, the JPEG), its tiny copy, and what the full one
/// weighs.
pub struct NewPhoto {
    pub storage_key: String,
    pub url: String,
    pub middle_url: Option<String>,
    pub card_url: String,
    pub jpeg_url: String,
    pub placeholder: String,
    pub size_bytes: i64,
    pub width: i32,
    pub height: i32,
    pub sha256: Vec<u8>,
}

/// Adds photos after the listing's others, in order; the first photo a listing gets is its cover.
pub async fn add_listing_photos(db: &PgPool, listing_id: Uuid, photos: &[NewPhoto]) -> sqlx::Result<()> {
    if photos.is_empty() {
        return Ok(());
    }
    let mut tx = db.begin().await?;
    let last = sqlx::query!(
        r#"SELECT coalesce(max(sort_order), -1) AS "last!", coalesce(bool_or(is_cover), false) AS "has_cover!"
           FROM listing_media WHERE listing_id = $1"#,
        listing_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    for (i, photo) in photos.iter().enumerate() {
        sqlx::query!(
            "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, middle_url,
                                        thumbnail_url, jpeg_url, placeholder, mime_type, size_bytes, width_px,
                                        height_px, sha256, sort_order, is_cover)
             VALUES ($1, 'IMAGE', 'READY', $2, $3, $4, $5, $6, $7, 'image/webp', $8, $9, $10, $11, $12, $13)",
            listing_id,
            photo.storage_key,
            photo.url,
            photo.middle_url,
            photo.card_url,
            photo.jpeg_url,
            photo.placeholder,
            photo.size_bytes,
            photo.width,
            photo.height,
            photo.sha256,
            last.last + 1 + i as i32,
            !last.has_cover && i == 0,
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Puts the listing's photos in the order its edit form sent: each by its id, or a photo just
/// added by its key in the store (it has no id the form knew); the first is the cover. Photos
/// the order leaves out follow, as they were. The cover changes in two steps, the old one let go
/// first, as only one may be (ux_listing_media_one_cover).
pub async fn order_listing_photos(db: &PgPool, listing_id: Uuid, order: &[String]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "WITH wanted AS (SELECT token, place FROM unnest($2::text[]) WITH ORDINALITY AS o(token, place)),
              ranked AS (
                  SELECT m.id, row_number() OVER (ORDER BY coalesce(w.place, 1000000), m.is_cover DESC, m.sort_order, m.id) - 1 AS place
                  FROM listing_media m LEFT JOIN wanted w ON w.token = m.id::text OR w.token = m.storage_key
                  WHERE m.listing_id = $1
              )
         UPDATE listing_media m SET sort_order = r.place FROM ranked r WHERE m.id = r.id",
    )
    .bind(listing_id)
    .bind(order)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE listing_media SET is_cover = false WHERE listing_id = $1 AND is_cover AND sort_order <> 0")
        .bind(listing_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE listing_media SET is_cover = true WHERE listing_id = $1 AND sort_order = 0 AND NOT is_cover")
        .bind(listing_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}

/// Puts the listing at a point of its seller's own on the map, more exactly than its town's (the
/// form's map, map_pin_controller.js; post.rs rounds it to about a hundred metres).
pub async fn set_listing_point(db: &PgPool, listing_id: Uuid, latitude: f64, longitude: f64) -> sqlx::Result<()> {
    sqlx::query("UPDATE listings SET latitude = $2::numeric(9, 6), longitude = $3::numeric(9, 6) WHERE id = $1")
        .bind(listing_id)
        .bind(latitude)
        .bind(longitude)
        .execute(db)
        .await?;
    Ok(())
}

/// Where the listing's photos are kept, the full copies, the cover first: at most `limit`, for
/// the AI to look at on an edit (web/ai_fill.rs).
pub async fn listing_photo_keys(db: &PgPool, listing_id: Uuid, limit: i64) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar(
        "SELECT storage_key FROM listing_media
         WHERE listing_id = $1 AND media_type = 'IMAGE' AND status = 'READY' AND storage_key IS NOT NULL
         ORDER BY is_cover DESC, sort_order LIMIT $2",
    )
    .bind(listing_id)
    .bind(limit)
    .fetch_all(db)
    .await
}

/// The listing's point on the map: its own, or its town's.
pub async fn listing_point(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Option<(f64, f64)>> {
    sqlx::query_as("SELECT latitude::float8, longitude::float8 FROM listings WHERE id = $1 AND latitude IS NOT NULL")
        .bind(listing_id)
        .fetch_optional(db)
        .await
}

/// A photo on the edit form, which its seller can remove.
pub struct OwnPhoto {
    pub id: Uuid,
    pub card_url: String,
}

/// The listing's photos, the cover first.
pub async fn own_listing_photos(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<OwnPhoto>> {
    sqlx::query_as!(
        OwnPhoto,
        r#"SELECT id, coalesce(thumbnail_url, public_url) AS "card_url!"
           FROM listing_media
           WHERE listing_id = $1 AND media_type = 'IMAGE' AND coalesce(thumbnail_url, public_url) IS NOT NULL
           ORDER BY is_cover DESC, sort_order"#,
        listing_id,
    )
    .fetch_all(db)
    .await
}

/// Removes some of the listing's photos; returns their keys in the store, to delete the files.
/// When the cover goes, the first photo left takes its place.
pub async fn remove_listing_photos(db: &PgPool, listing_id: Uuid, ids: &[Uuid]) -> sqlx::Result<Vec<String>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut tx = db.begin().await?;
    let keys = sqlx::query_scalar!(
        "DELETE FROM listing_media WHERE listing_id = $1 AND id = ANY($2) RETURNING storage_key",
        listing_id,
        ids,
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE listing_media SET is_cover = true
         WHERE id = (SELECT id FROM listing_media WHERE listing_id = $1 ORDER BY sort_order LIMIT 1)
           AND NOT EXISTS (SELECT 1 FROM listing_media WHERE listing_id = $1 AND is_cover)",
        listing_id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(keys)
}

/// How a buyer reaches the seller: the phone number, or, for sellers who signed up with an e-mail
/// address, that address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SellerContact {
    Phone(String),
    Email(String),
}

/// What the listing's page offers to reveal, by the same rule as `reveal_contact`, so the button
/// never shows where nothing can be revealed (a sold listing, say). The value itself stays out
/// of the page until the visitor asks for it.
pub async fn visible_contact(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Option<SellerContact>> {
    let row = sqlx::query!(
        r#"SELECT CASE WHEN c.show_phone THEN c.phone_e164::text END AS phone,
                  CASE WHEN c.show_email THEN c.email::text END AS email
           FROM listings l
           JOIN listing_contacts c ON c.listing_id = l.id
           WHERE l.id = $1 AND l.status IN ('ACTIVE', 'RESERVED')"#,
        listing_id
    )
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|row| match (row.phone, row.email) {
        (Some(phone), _) => Some(SellerContact::Phone(phone)),
        (None, Some(email)) => Some(SellerContact::Email(email)),
        (None, None) => None,
    }))
}

/// What a reveal shows: how to reach the seller, and the listing's title and slug, which the
/// message a WhatsApp button starts names (web/listings.rs).
pub struct Revealed {
    pub contact: SellerContact,
    pub title: String,
    pub slug: String,
}

/// The listing's phone number, or its e-mail address if it has no number to show. Each reveal
/// is counted in today's statistics.
pub async fn reveal_contact(db: &PgPool, public_no: i64) -> sqlx::Result<Option<Revealed>> {
    let Some(listing) = sqlx::query!(
        r#"SELECT id, coalesce(generated_title, title) AS "title!", slug FROM listings WHERE public_no = $1"#,
        public_no
    )
    .fetch_optional(db)
    .await?
    else {
        return Ok(None);
    };
    let id = listing.id;
    let Some(contact) = visible_contact(db, id).await? else {
        return Ok(None);
    };
    // Fine at launch volumes; buffer these counters once traffic grows (README, "Analytics").
    sqlx::query!(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, phone_reveals)
         VALUES ($1, current_date, 1)
         ON CONFLICT (listing_id, stat_date)
         DO UPDATE SET phone_reveals = listing_daily_stats.phone_reveals + 1",
        id
    )
    .execute(db)
    .await?;

    Ok(Some(Revealed { contact, title: listing.title, slug: listing.slug }))
}

/// A listing's neighbour on its category's page, the one before it or the one after it: its
/// number and its slug, which make its address.
#[derive(Debug, sqlx::FromRow)]
pub struct Neighbour {
    pub public_no: i64,
    pub slug: String,
}

/// The listings either side of one on its category's page, for the round buttons at the right end
/// of its path (listing.html): the one before it and the one after it, in that page's own order,
/// newest first (sort_at, then id, as listing_pages.rs orders the page, which the category's index
/// serves), among the active listings in its category but for those of the accounts the visitor
/// hides (safety.rs). None where the list ends; a listing no longer on the list itself still has
/// those either side of where it was.
pub async fn listing_neighbours(
    db: &PgPool,
    listing_id: Uuid,
    category_id: Uuid,
    hidden: &[Uuid],
) -> sqlx::Result<(Option<Neighbour>, Option<Neighbour>)> {
    const PREVIOUS: &str = "SELECT l.public_no, l.slug FROM listings l
         WHERE l.category_id = $1 AND l.status = 'ACTIVE' AND l.created_by_user_id <> ALL($3)
           AND (l.sort_at, l.id) > (SELECT o.sort_at, o.id FROM listings o WHERE o.id = $2)
         ORDER BY l.sort_at, l.id LIMIT 1";
    const NEXT: &str = "SELECT l.public_no, l.slug FROM listings l
         WHERE l.category_id = $1 AND l.status = 'ACTIVE' AND l.created_by_user_id <> ALL($3)
           AND (l.sort_at, l.id) < (SELECT o.sort_at, o.id FROM listings o WHERE o.id = $2)
         ORDER BY l.sort_at DESC, l.id DESC LIMIT 1";
    let step = |sql: &'static str| {
        sqlx::query_as::<_, Neighbour>(sql).bind(category_id).bind(listing_id).bind(hidden.to_vec()).fetch_optional(db)
    };
    tokio::try_join!(step(PREVIOUS), step(NEXT))
}
