//! Stores (web/stores.rs): a business someone makes from their account (businesses, with them its
//! OWNER in business_members), which their listings can be posted as, with its own page
//! (/biznes/{slug}): its logo and cover, what it says of itself, where it is, when it is open, and
//! how to reach it. Its people (business_members): its owner, and those its owner and admins asked
//! in (business_invitations, 0035), each with a role.

use crate::{domain::Role, format::Contact};

use super::*;

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
    /// Whether the user may post its listings (Role::posts).
    pub fn posts(&self) -> bool {
        Role::parse(&self.role).is_some_and(Role::posts)
    }

    /// Whether the user may change the store itself (Role::manages).
    pub fn manages(&self) -> bool {
        Role::parse(&self.role).is_some_and(Role::manages)
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

/// The roles a store's people are given, the owner's aside (Role::GIVEN), as the database spells
/// them: who changes the store and its people (ADMIN), who posts its listings (EDITOR), and who
/// answers its messages alone (VIEWER).
pub const GIVEN_ROLES: [&str; 3] = Role::names(Role::GIVEN);

/// One of a store's people, as its page of people lists them.
#[derive(Debug, sqlx::FromRow)]
pub struct Member {
    pub user_id: Uuid,
    pub display_name: String,
    pub role: String,
    /// The contacts they sign in with.
    pub phone: Option<String>,
    pub email: Option<String>,
    pub since: OffsetDateTime,
}

/// A store's people: its owner first, then its admins, its editors and those who answer its
/// messages, each by name.
pub async fn members(db: &PgPool, store_id: Uuid) -> sqlx::Result<Vec<Member>> {
    sqlx::query_as(
        "SELECT u.id AS user_id, u.display_name, m.role, u.phone_e164::text AS phone, u.email::text AS email,
                m.created_at AS since
         FROM business_members m
         JOIN users u ON u.id = m.user_id
         WHERE m.business_id = $1
         ORDER BY array_position(ARRAY['OWNER', 'ADMIN', 'EDITOR', 'VIEWER']::varchar[], m.role), u.display_name, u.id",
    )
    .bind(store_id)
    .fetch_all(db)
    .await
}

/// The user's role in the store, if they are one of its people.
pub async fn role_in(db: &PgPool, store_id: Uuid, user_id: Uuid) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT role FROM business_members WHERE business_id = $1 AND user_id = $2")
        .bind(store_id)
        .bind(user_id)
        .fetch_optional(db)
        .await
}

/// Whether the user posts for the store (Role::posts), and so may change its listings
/// (manages_listing, 0035).
pub async fn posts_for(db: &PgPool, store_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    Ok(role_in(db, store_id, user_id).await?.as_deref().and_then(Role::parse).is_some_and(Role::posts))
}

/// An invitation into a store, with the store it is for.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub store_id: Uuid,
    pub store_slug: String,
    pub store_name: String,
    pub store_logo: Option<String>,
    /// Whom it asks in: the phone number or the e-mail address it waits for.
    pub phone: Option<String>,
    pub email: Option<String>,
    pub role: String,
    pub expires_at: OffsetDateTime,
}

impl Invitation {
    /// The contact it waits for.
    pub fn contact(&self) -> Contact {
        match (&self.phone, &self.email) {
            (Some(phone), _) => Contact::Phone(phone.clone()),
            (None, email) => Contact::Email(email.clone().unwrap_or_default()),
        }
    }
}

/// An invitation's columns, from business_invitations i and its store b.
const INVITATION: &str = "i.id, b.id AS store_id, b.slug AS store_slug, b.display_name AS store_name, b.logo_url AS store_logo,
                          i.phone_e164::text AS phone, i.email::text AS email, i.role, i.expires_at";

/// The invitation, while it is open and its store up.
pub async fn invitation(db: &PgPool, id: Uuid) -> sqlx::Result<Option<Invitation>> {
    sqlx::query_as(&format!(
        "SELECT {INVITATION} FROM business_invitations i JOIN businesses b ON b.id = i.business_id
         WHERE i.id = $1 AND i.expires_at > now() AND b.status = 'ACTIVE'"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

/// A store's open invitations, the latest first.
pub async fn store_invitations(db: &PgPool, store_id: Uuid) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as(&format!(
        "SELECT {INVITATION} FROM business_invitations i JOIN businesses b ON b.id = i.business_id
         WHERE i.business_id = $1 AND i.expires_at > now()
         ORDER BY i.created_at DESC"
    ))
    .bind(store_id)
    .fetch_all(db)
    .await
}

/// The open invitations for the user's phone number or e-mail address, from stores up that they
/// aren't one of the people of yet, the latest first.
pub async fn invitations_for(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<Invitation>> {
    sqlx::query_as(&format!(
        "SELECT {INVITATION} FROM users u
         JOIN business_invitations i ON i.phone_e164 = u.phone_e164 OR i.email = u.email
         JOIN businesses b ON b.id = i.business_id
         WHERE u.id = $1 AND i.expires_at > now() AND b.status = 'ACTIVE'
           AND NOT EXISTS (SELECT 1 FROM business_members m WHERE m.business_id = b.id AND m.user_id = u.id)
         ORDER BY i.created_at DESC"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// What became of asking someone into a store.
#[derive(Debug, PartialEq, Eq)]
pub enum Invited {
    /// The invitation: a new one, or the one the contact had, with the new role and its two weeks
    /// again.
    Sent(Uuid),
    /// An account with that contact is one of the store's people already.
    Member,
}

/// Asks the contact into the store with the role, `by` one of its owner or admins; an account that
/// signs in with that contact has a notification of it (STORE_INVITE) saying `title` and `body`.
pub async fn invite(db: &PgPool, store_id: Uuid, contact: &Contact, role: &str, by: Uuid, title: &str, body: &str) -> sqlx::Result<Invited> {
    let (phone, email) = match contact {
        Contact::Phone(phone) => (Some(phone.as_str()), None),
        Contact::Email(email) => (None, Some(email.as_str())),
    };
    let mut tx = db.begin().await?;
    let member: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM business_members m JOIN users u ON u.id = m.user_id
                        WHERE m.business_id = $1 AND (u.phone_e164 = $2::text::e164_phone OR u.email = $3::text::citext))",
    )
    .bind(store_id)
    .bind(phone)
    .bind(email)
    .fetch_one(&mut *tx)
    .await?;
    if member {
        return Ok(Invited::Member);
    }
    // One per store and contact (0035's unique indexes): asked again, it is the same one.
    let target = if phone.is_some() { "(business_id, phone_e164) WHERE phone_e164 IS NOT NULL" } else { "(business_id, email) WHERE email IS NOT NULL" };
    let id: Uuid = sqlx::query_scalar(&format!(
        "INSERT INTO business_invitations (business_id, phone_e164, email, role, invited_by_user_id)
         VALUES ($1, $2::text::e164_phone, $3::text::citext, $4, $5)
         ON CONFLICT {target}
         DO UPDATE SET role = excluded.role, invited_by_user_id = excluded.invited_by_user_id,
                       created_at = now(), expires_at = excluded.expires_at
         RETURNING id"
    ))
    .bind(store_id)
    .bind(phone)
    .bind(email)
    .bind(role)
    .bind(by)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO notifications (user_id, notification_type, title, body, payload)
         SELECT u.id, 'STORE_INVITE', $3, $4, jsonb_build_object('invitation', $5::uuid::text)
         FROM users u
         WHERE u.status = 'ACTIVE' AND (u.phone_e164 = $1::text::e164_phone OR u.email = $2::text::citext)",
    )
    .bind(phone)
    .bind(email)
    .bind(title)
    .bind(body)
    .bind(id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Invited::Sent(id))
}

/// The user takes the invitation, if it is open and for their phone number or e-mail address: they
/// become one of the store's people with its role (or keep the role they have), the invitation
/// and its notifications done with. Returns the store's address; None if it wasn't theirs to take.
pub async fn accept_invitation(db: &PgPool, id: Uuid, user_id: Uuid) -> sqlx::Result<Option<String>> {
    let mut tx = db.begin().await?;
    let taken: Option<(Uuid, String, String)> = sqlx::query_as(
        "DELETE FROM business_invitations i
         USING users u, businesses b
         WHERE i.id = $1 AND u.id = $2 AND b.id = i.business_id AND b.status = 'ACTIVE' AND i.expires_at > now()
           AND (i.phone_e164 = u.phone_e164 OR i.email = u.email)
         RETURNING i.business_id, i.role, b.slug",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((store_id, role, slug)) = taken else {
        return Ok(None);
    };
    sqlx::query("INSERT INTO business_members (business_id, user_id, role) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(store_id)
        .bind(user_id)
        .bind(role)
        .execute(&mut *tx)
        .await?;
    read_invitation_notices(&mut tx, id, user_id).await?;
    tx.commit().await?;
    Ok(Some(slug))
}

/// The user turns the invitation down, if it is for their phone number or e-mail address; returns
/// whether it was.
pub async fn decline_invitation(db: &PgPool, id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let declined = sqlx::query(
        "DELETE FROM business_invitations i USING users u
         WHERE i.id = $1 AND u.id = $2 AND (i.phone_e164 = u.phone_e164 OR i.email = u.email)",
    )
    .bind(id)
    .bind(user_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    read_invitation_notices(&mut tx, id, user_id).await?;
    tx.commit().await?;
    Ok(declined)
}

/// The notifications of an invitation the user took or turned down, read.
async fn read_invitation_notices(tx: &mut sqlx::Transaction<'_, Postgres>, id: Uuid, user_id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE notifications SET read_at = now()
         WHERE user_id = $1 AND notification_type = 'STORE_INVITE' AND read_at IS NULL AND payload ->> 'invitation' = $2::uuid::text",
    )
    .bind(user_id)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Withdraws one of the store's invitations; returns whether it had it.
pub async fn cancel_invitation(db: &PgPool, store_id: Uuid, id: Uuid) -> sqlx::Result<bool> {
    let cancelled = sqlx::query("DELETE FROM business_invitations WHERE id = $2 AND business_id = $1")
        .bind(store_id)
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(cancelled == 1)
}

/// Gives one of the store's people, not its owner, another of GIVEN_ROLES; returns whether it did.
pub async fn set_member_role(db: &PgPool, store_id: Uuid, user_id: Uuid, role: &str) -> sqlx::Result<bool> {
    let changed = sqlx::query("UPDATE business_members SET role = $3 WHERE business_id = $1 AND user_id = $2 AND role <> 'OWNER'")
        .bind(store_id)
        .bind(user_id)
        .bind(role)
        .execute(db)
        .await?
        .rows_affected();
    Ok(changed == 1)
}

/// Takes one of the store's people, not its owner, out of it (they leave, or are let go): the
/// listings they posted for it stay its own, and theirs to change no more (manages_listing).
pub async fn remove_member(db: &PgPool, store_id: Uuid, user_id: Uuid) -> sqlx::Result<bool> {
    let removed = sqlx::query("DELETE FROM business_members WHERE business_id = $1 AND user_id = $2 AND role <> 'OWNER'")
        .bind(store_id)
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(removed == 1)
}
