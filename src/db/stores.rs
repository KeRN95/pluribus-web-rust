//! Stores (web/stores.rs): a business someone makes from their account (businesses, with them its
//! OWNER in business_members), which their listings can be posted as, with its own page
//! (/biznes/{slug}): its logo and cover, what it says of itself, where it is, when it is open, and
//! how to reach it.

use super::*;

/// The roles that may post a store's listings, and those that may change the store itself.
pub const POSTING_ROLES: [&str; 3] = ["OWNER", "ADMIN", "EDITOR"];
pub const MANAGING_ROLES: [&str; 2] = ["OWNER", "ADMIN"];

/// What a store's form fills in, checked (web/stores.rs).
#[derive(Debug, Default)]
pub struct StoreFields {
    pub name: String,
    pub description: Option<String>,
    /// Its public number, as +355…, and whether it takes WhatsApp.
    pub phone: Option<String>,
    pub whatsapp: bool,
    pub email: Option<String>,
    pub website: Option<String>,
    /// Its town (a municipality, locations).
    pub location_id: Option<Uuid>,
    pub address: Option<String>,
    pub opening_hours: Option<String>,
    /// Its NIPT, which only its form shows.
    pub registration_no: Option<String>,
}

/// A picture a store's form sent: its file in the photo store, and the address pages give it.
#[derive(Debug, Clone)]
pub struct StoreImage {
    pub key: String,
    pub url: String,
}

/// What becomes of a store's logo or cover as its form is saved.
#[derive(Debug, Clone)]
pub enum ImageChange {
    Keep,
    Replace(StoreImage),
    Remove,
}

/// A store as its form and its page show it.
#[derive(Debug, sqlx::FromRow)]
pub struct Store {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub description: Option<String>,
    pub public_phone: Option<String>,
    pub whatsapp: bool,
    pub public_email: Option<String>,
    pub website_url: Option<String>,
    pub location_id: Option<Uuid>,
    /// Its town's name.
    pub town: Option<String>,
    pub address: Option<String>,
    pub opening_hours: Option<String>,
    pub registration_no: Option<String>,
    pub logo_url: Option<String>,
    pub cover_url: Option<String>,
    pub created_at: OffsetDateTime,
}

/// The store at this address, while it is up.
pub async fn store_by_slug(db: &PgPool, slug: &str) -> sqlx::Result<Option<Store>> {
    sqlx::query_as(
        "SELECT b.id, b.slug, b.display_name, b.description, b.public_phone::text AS public_phone, b.whatsapp,
                b.public_email::text AS public_email, b.website_url, b.location_id, loc.name AS town, b.address,
                b.opening_hours, b.registration_no, b.logo_url, b.cover_url, b.created_at
         FROM businesses b
         LEFT JOIN locations loc ON loc.id = b.location_id
         WHERE b.slug = $1 AND b.status = 'ACTIVE'",
    )
    .bind(slug)
    .fetch_optional(db)
    .await
}

/// A store the user is one of the people of, with their role in it (OWNER, ADMIN, EDITOR, VIEWER).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MyStore {
    pub id: Uuid,
    pub slug: String,
    pub display_name: String,
    pub role: String,
    pub logo_url: Option<String>,
    pub public_phone: Option<String>,
}

impl MyStore {
    /// Whether the user may post its listings.
    pub fn posts(&self) -> bool {
        POSTING_ROLES.contains(&self.role.as_str())
    }

    /// Whether the user may change the store itself.
    pub fn manages(&self) -> bool {
        MANAGING_ROLES.contains(&self.role.as_str())
    }
}

/// The stores the user is one of the people of, those they own first, then by name.
pub async fn my_stores(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<MyStore>> {
    sqlx::query_as(
        "SELECT b.id, b.slug, b.display_name, m.role, b.logo_url, b.public_phone::text AS public_phone
         FROM business_members m
         JOIN businesses b ON b.id = m.business_id
         WHERE m.user_id = $1 AND b.status = 'ACTIVE'
         ORDER BY m.role = 'OWNER' DESC, b.display_name",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// How many stores someone may own: enough for a few branches, not a flood of shop windows.
pub const STORES_MOST: i64 = 5;

/// How many stores the user owns.
pub async fn stores_owned(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM business_members m JOIN businesses b ON b.id = m.business_id
         WHERE m.user_id = $1 AND m.role = 'OWNER' AND b.status <> 'DELETED'",
    )
    .bind(user_id)
    .fetch_one(db)
    .await
}

/// Makes a store, the user its owner, at an address made from its name (format::slug), with a
/// number after it while another store has that one; returns its address.
pub async fn create_store(
    db: &PgPool,
    user_id: Uuid,
    fields: &StoreFields,
    logo: Option<&StoreImage>,
    cover: Option<&StoreImage>,
) -> sqlx::Result<String> {
    let base = match crate::format::slug(&fields.name) {
        slug if slug.is_empty() => "dyqan".to_string(),
        slug => slug.chars().take(150).collect::<String>().trim_end_matches('-').to_string(),
    };
    let mut tx = db.begin().await?;
    let mut created = None;
    for n in 1..=50 {
        let slug = if n == 1 { base.clone() } else { format!("{base}-{n}") };
        created = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO businesses (legal_name, display_name, slug, registration_no, description, public_phone, public_email,
                                     website_url, whatsapp, location_id, address, opening_hours, logo_url, logo_key, cover_url, cover_key)
             VALUES ($1, $1, $2, $3, $4, $5::text::e164_phone, $6::text::citext, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (slug) DO NOTHING
             RETURNING id",
        )
        .bind(&fields.name)
        .bind(&slug)
        .bind(&fields.registration_no)
        .bind(&fields.description)
        .bind(&fields.phone)
        .bind(&fields.email)
        .bind(&fields.website)
        .bind(fields.whatsapp)
        .bind(fields.location_id)
        .bind(&fields.address)
        .bind(&fields.opening_hours)
        .bind(logo.map(|image| &image.url))
        .bind(logo.map(|image| &image.key))
        .bind(cover.map(|image| &image.url))
        .bind(cover.map(|image| &image.key))
        .fetch_optional(&mut *tx)
        .await?
        .map(|id| (id, slug));
        if created.is_some() {
            break;
        }
    }
    let Some((id, slug)) = created else {
        return Err(sqlx::Error::RowNotFound);
    };
    sqlx::query("INSERT INTO business_members (business_id, user_id, role) VALUES ($1, $2, 'OWNER')")
        .bind(id)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(slug)
}

/// Saves a store's form, its logo and cover kept, replaced or taken off, and its listings that
/// showed its old public number the new one; returns the files replaced or taken off, which the
/// caller takes out of the photo store. Its address stays, so that its links do.
pub async fn update_store(
    db: &PgPool,
    store_id: Uuid,
    fields: &StoreFields,
    logo: &ImageChange,
    cover: &ImageChange,
) -> sqlx::Result<Vec<String>> {
    let mut tx = db.begin().await?;
    let (old_phone, old_logo, old_cover): (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT public_phone::text, logo_key, cover_key FROM businesses WHERE id = $1 FOR UPDATE")
            .bind(store_id)
            .fetch_one(&mut *tx)
            .await?;
    let image = |change: &ImageChange, old: Option<String>| match change {
        ImageChange::Keep => (old.clone(), None, Vec::new(), false),
        ImageChange::Replace(image) => (Some(image.key.clone()), Some(image.url.clone()), old.into_iter().collect(), true),
        ImageChange::Remove => (None, None, old.into_iter().collect(), true),
    };
    let (logo_key, logo_url, mut gone, logo_changed) = image(logo, old_logo);
    let (cover_key, cover_url, cover_gone, cover_changed) = image(cover, old_cover);
    gone.extend(cover_gone);
    sqlx::query(
        "UPDATE businesses
         SET display_name = $2, description = $3, public_phone = $4::text::e164_phone, public_email = $5::text::citext,
             website_url = $6, whatsapp = $7, location_id = $8, address = $9, opening_hours = $10, registration_no = $11,
             logo_key = CASE WHEN $12 THEN $13 ELSE logo_key END, logo_url = CASE WHEN $12 THEN $14 ELSE logo_url END,
             cover_key = CASE WHEN $15 THEN $16 ELSE cover_key END, cover_url = CASE WHEN $15 THEN $17 ELSE cover_url END,
             updated_at = now()
         WHERE id = $1",
    )
    .bind(store_id)
    .bind(&fields.name)
    .bind(&fields.description)
    .bind(&fields.phone)
    .bind(&fields.email)
    .bind(&fields.website)
    .bind(fields.whatsapp)
    .bind(fields.location_id)
    .bind(&fields.address)
    .bind(&fields.opening_hours)
    .bind(&fields.registration_no)
    .bind(logo_changed)
    .bind(&logo_key)
    .bind(&logo_url)
    .bind(cover_changed)
    .bind(&cover_key)
    .bind(&cover_url)
    .execute(&mut *tx)
    .await?;
    if let (Some(old), Some(new)) = (old_phone, &fields.phone)
        && &old != new
    {
        sqlx::query(
            "UPDATE listing_contacts c SET phone_e164 = $3::text::e164_phone
             FROM listings l WHERE l.id = c.listing_id AND l.seller_business_id = $1 AND c.phone_e164 = $2::text::e164_phone",
        )
        .bind(store_id)
        .bind(old)
        .bind(new)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(gone)
}
