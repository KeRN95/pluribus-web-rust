//! Posting and editing listings, pausing and taking them down.

use std::fmt;

use super::*;
use pluribus_core::domain::{ListingType, ReviewReason, Status, listing::RecentPosts};

pub async fn recent_posts(db: &PgPool, user_id: Uuid, title: &str) -> sqlx::Result<RecentPosts> {
    sqlx::query_as!(
        RecentPosts,
        r#"SELECT count(*) AS "count!",
                  coalesce(bool_or(lower(title) = lower($2)), false) AS "same_title!"
           FROM listings
           WHERE created_by_user_id = $1 AND created_at > now() - interval '1 day' AND status <> 'DRAFT'"#,
        user_id,
        title,
    )
    .fetch_one(db)
    .await
}

pub struct NewListing<'a> {
    pub user_id: Uuid,
    pub category_id: Uuid,
    pub location_id: Uuid,
    pub title: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    pub listing_type: &'a str,
    pub condition: Option<&'a str>,
    pub price_amount: Option<Decimal>,
    pub currency: &'a str,
    pub price_type: &'a str,
    pub contact_name: Option<&'a str>,
    pub phone: Option<&'a str>,
    pub email: Option<&'a str>,
    /// The category's properties as the seller filled them in.
    pub properties: &'a [ListingProperty],
    /// Published at once, or kept as a draft until the seller confirms their contact.
    pub publish: bool,
    /// Why it waits for a moderator instead of going up (review.rs, NEW_ACCOUNT or
    /// DUPLICATE_PHOTOS), if it does; a draft keeps it for when its contact is confirmed.
    pub review: Option<ReviewReason>,
    /// The store it is posted as (stores.rs), one its poster may post for; None for their own.
    pub business_id: Option<Uuid>,
}

/// A property's value on a listing, as the form gives it.
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    Option(Uuid),
    /// An option the seller typed ("Tjetër"): added, inactive, unless one with its code exists.
    /// A model goes under the make chosen (or typed) for the parent property.
    NewOption { label: String, code: String, parent_attribute_id: Option<Uuid> },
    Options(Vec<Uuid>),
    Number(Decimal),
    Boolean(bool),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListingProperty {
    pub attribute_id: Uuid,
    pub value: PropertyValue,
}

/// Replaces the listing's property values with the form's, plus its subcategory's kind (the old
/// "type" property, migration 0008), which search still finds it by.
async fn save_properties(
    tx: &mut sqlx::PgConnection,
    listing_id: Uuid,
    listing: &NewListing<'_>,
) -> sqlx::Result<()> {
    sqlx::query!("DELETE FROM listing_attribute_values WHERE listing_id = $1", listing_id).execute(&mut *tx).await?;
    sqlx::query!("DELETE FROM listing_attribute_options WHERE listing_id = $1", listing_id).execute(&mut *tx).await?;
    sqlx::query!(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
         SELECT $1, a.id, a.data_type, o.id
         FROM categories c
         JOIN attribute_options o ON o.id = c.replaces_option_id
         JOIN attributes a ON a.id = o.attribute_id
         WHERE c.id = $2",
        listing_id,
        listing.category_id,
    )
    .execute(&mut *tx)
    .await?;
    // The option saved for each property so far: a typed model goes under the make's.
    let mut saved: HashMap<Uuid, Uuid> = HashMap::new();
    for property in listing.properties {
        let attribute = property.attribute_id;
        let option = match &property.value {
            PropertyValue::Option(option) => Some(*option),
            PropertyValue::NewOption { label, code, parent_attribute_id } => {
                let parent = parent_attribute_id.and_then(|parent| saved.get(&parent).copied());
                Some(
                    sqlx::query_scalar!(
                        r#"WITH code AS (
                               SELECT coalesce((SELECT value_code || '_' FROM attribute_options WHERE id = $2), '') || $3
                                      AS value_code
                           ), added AS (
                               INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, active,
                                                              added_by_user_id, sort_order)
                               SELECT $1, $2, value_code, $4, false, $5, 1000 FROM code
                               ON CONFLICT (attribute_id, value_code) DO NOTHING
                               RETURNING id
                           )
                           SELECT id AS "id!" FROM added
                           UNION ALL
                           SELECT o.id FROM attribute_options o, code
                           WHERE o.attribute_id = $1 AND o.value_code = code.value_code
                           LIMIT 1"#,
                        attribute,
                        parent,
                        code,
                        label,
                        listing.user_id,
                    )
                    .fetch_one(&mut *tx)
                    .await?,
                )
            }
            _ => None,
        };
        if let Some(option) = option {
            saved.insert(attribute, option);
            sqlx::query!(
                "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
                 VALUES ($1, $2, 'SELECT', $3)",
                listing_id,
                attribute,
                option,
            )
            .execute(&mut *tx)
            .await?;
            continue;
        }
        match &property.value {
            PropertyValue::Option(_) | PropertyValue::NewOption { .. } => {}
            PropertyValue::Options(options) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
                     SELECT $1, $2, unnest($3::uuid[])",
                    listing_id,
                    attribute,
                    options,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Number(number) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
                     VALUES ($1, $2, 'NUMBER', $3)",
                    listing_id,
                    attribute,
                    number,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Boolean(value) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_boolean)
                     VALUES ($1, $2, 'BOOLEAN', $3)",
                    listing_id,
                    attribute,
                    value,
                )
                .execute(&mut *tx)
                .await?;
            }
            PropertyValue::Text(text) => {
                sqlx::query!(
                    "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text)
                     VALUES ($1, $2, 'TEXT', $3)",
                    listing_id,
                    attribute,
                    text,
                )
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    Ok(())
}

/// How long a listing stays up once published, renewed ("Rifresko") or put back up, in days: it
/// expires then (expiry.rs), unless its seller renews it. A listing from before there was an
/// expiry has none, and stays up.
pub const LISTING_DAYS: i32 = 60;
/// A renewal brings a listing back to the top of "newest first" (`bumped_at`) once in this many
/// days at most, so that renewing is no way to stay on top.
pub const BUMP_DAYS: i32 = 7;

/// The photos a new listing or an edit brings, their files in the store already: put there for this
/// request, as the pages' form sends them with the listing (`Stored`, their rows as they go in); or
/// uploaded before by the user (`Uploads`, the API's, photo_uploads.rs), by their ids, in their
/// order, which move into the listing's photos keeping their ids (take_uploads).
#[derive(Clone, Copy)]
pub enum Photos<'a> {
    Stored(&'a [NewPhoto]),
    Uploads { ids: &'a [Uuid], user_id: Uuid },
}

/// Why a listing or an edit wasn't saved (`create_listing`, `update_listing`), nothing of it
/// written: the database failed (`Database`), or a photo it names by its upload (`Photos::Uploads`)
/// isn't there to take, another's, taken by a listing already, or forgotten (`UploadsGone`).
#[derive(Debug)]
pub enum NotSaved {
    Database(sqlx::Error),
    UploadsGone,
}

impl From<sqlx::Error> for NotSaved {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl fmt::Display for NotSaved {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Database(error) => error.fmt(f),
            Self::UploadsGone => f.write_str("a photo upload named isn't there to take"),
        }
    }
}

impl std::error::Error for NotSaved {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::UploadsGone => None,
        }
    }
}

/// Adds `photos` to the listing, after those it has, the first it gets its cover where it has none
/// (listing.rs's add_listing_photos, or photo_uploads.rs's take_uploads): inside the transaction
/// that saves it or its edit.
async fn add_photos(tx: &mut sqlx::PgConnection, listing_id: Uuid, photos: Photos<'_>) -> Result<(), NotSaved> {
    match photos {
        Photos::Stored(photos) => Ok(add_listing_photos(tx, listing_id, photos).await?),
        Photos::Uploads { ids, user_id } => match take_uploads(tx, listing_id, user_id, ids).await? {
            true => Ok(()),
            false => Err(NotSaved::UploadsGone),
        },
    }
}

/// Saves a listing with the seller's contact, published at once or as a draft (`publish`), when it
/// expires with it, its seller its poster or the store it is posted as (`business_id`), and returns
/// its id and number. It stands at `point`, its seller's own on the map, or else at its place's
/// (its town's, for a zone without one), with its properties and `photos` (their files already in
/// the store, put there for it or uploaded before): all in one transaction, so that a failure
/// leaves nothing of it, neither a listing without its point or photos nor their rows without it,
/// and an upload it names that isn't there to take fails it whole (`NotSaved::UploadsGone`).
pub async fn create_listing(
    db: &PgPool,
    listing: &NewListing<'_>,
    point: Option<(f64, f64)>,
    photos: Photos<'_>,
) -> Result<(Uuid, i64), NotSaved> {
    let mut tx = db.begin().await?;
    let row = sqlx::query!(
        "INSERT INTO listings (seller_user_id, seller_business_id, created_by_user_id, category_id, location_id, slug, title,
                               description, listing_type, condition, price_amount, currency, price_type,
                               price_normalized, latitude, longitude, status, published_at, expires_at, review_reason)
         SELECT CASE WHEN $14::uuid IS NULL THEN $1::uuid END, $14, $1, $2, loc.id, $4, $5, $6, $7, $8, $9, $10::text::currency_code, $11,
                price_in_eur($9, $10::text::currency_code),
                coalesce(loc.latitude, town.latitude), coalesce(loc.longitude, town.longitude),
                CASE WHEN NOT $12 THEN 'DRAFT' WHEN $15::text IS NULL THEN 'ACTIVE' ELSE 'PENDING_REVIEW' END,
                CASE WHEN $12 AND $15::text IS NULL THEN now() END,
                CASE WHEN $12 AND $15::text IS NULL THEN now() + make_interval(days => $13) END, $15
         FROM locations loc
         LEFT JOIN locations town ON town.id = location_municipality(loc.id)
         WHERE loc.id = $3
         RETURNING id, public_no",
        listing.user_id,
        listing.category_id,
        listing.location_id,
        listing.slug,
        listing.title,
        listing.description,
        listing.listing_type,
        listing.condition,
        listing.price_amount,
        listing.currency,
        listing.price_type,
        listing.publish,
        LISTING_DAYS,
        listing.business_id,
        listing.review.map(ReviewReason::as_str),
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO listing_contacts (listing_id, contact_name, phone_e164, email, show_phone, show_email)
         VALUES ($1, $2, $3::text::e164_phone, $4::text::citext, $3 IS NOT NULL, $4 IS NOT NULL)",
        row.id,
        listing.contact_name,
        listing.phone,
        listing.email,
    )
    .execute(&mut *tx)
    .await?;
    save_properties(&mut tx, row.id, listing).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = $1",
        row.id
    )
        .execute(&mut *tx)
        .await?;
    if let Some(point) = point {
        set_listing_point(&mut tx, row.id, point).await?;
    }
    add_photos(&mut tx, row.id, photos).await?;
    tx.commit().await?;
    Ok((row.id, row.public_no))
}

/// One of the user's own listings, as the edit form shows it. Only listings they created, and only
/// while they are up (active or reserved) or taken down by them for a while (paused), can be edited.
pub struct OwnListing {
    pub id: Uuid,
    pub public_no: i64,
    pub slug: String,
    pub category: String,
    pub listing_type: String,
    pub condition: Option<String>,
    pub title: String,
    pub description: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    /// The municipality's slug: the form picks towns, and a listing may sit in a smaller place.
    pub city: Option<String>,
    /// The zone's slug, when it sits in one (Blloku in Tiranë).
    pub zone: Option<String>,
    pub contact_name: Option<String>,
}

pub async fn own_listing(db: &PgPool, public_no: i64, user_id: Uuid) -> sqlx::Result<Option<OwnListing>> {
    sqlx::query_as!(
        OwnListing,
        r#"SELECT l.id, l.public_no, l.slug, c.slug AS category, l.listing_type, l.condition, l.title, l.description,
                  l.price_amount, l.currency::text AS "currency!", l.price_type,
                  (SELECT m.slug FROM locations m WHERE m.id = location_municipality(l.location_id)) AS city,
                  (SELECT z.slug FROM locations z WHERE z.id = l.location_id AND z.location_type = 'AREA') AS zone,
                  lc.contact_name
           FROM listings l
           JOIN categories c ON c.id = l.category_id
           LEFT JOIN listing_contacts lc ON lc.listing_id = l.id
           WHERE l.public_no = $1 AND manages_listing(l.seller_business_id, l.created_by_user_id, $2)
             AND l.status IN ('ACTIVE', 'RESERVED', 'PAUSED')"#,
        public_no,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Saves an edit, if the listing is the user's (or a store's they post for, manages_listing) and
/// still up, or taken down for a while (PAUSED, which an edit leaves as it is); None when it isn't,
/// and nothing changes. The number it shows stays, and its place in "newest first". It moves to
/// `point`, its seller's own on the map; without one it keeps its point unless it moves to another
/// town. Its photos `removed` go (only its own), those `added` (their files already in the store,
/// put there for it or uploaded before) follow the rest, and all of them take the `order` its form
/// sent, if any, each by its id or its key in the store (order_listing_photos; an upload by its id,
/// which it keeps). All in one transaction, so that a failure leaves the listing as it was, an
/// upload that isn't there to take too (`NotSaved::UploadsGone`); it returns the store's keys of
/// the photos removed, whose files can go once it has gone through. Those it told of a lower
/// price, by a notification each, `update_listing_with_drops` returns as well.
pub async fn update_listing(
    db: &PgPool,
    public_no: i64,
    listing: &NewListing<'_>,
    point: Option<(f64, f64)>,
    removed: &[Uuid],
    added: Photos<'_>,
    order: &[String],
) -> Result<Option<Vec<String>>, NotSaved> {
    let updated = update_listing_with_drops(db, public_no, listing, point, removed, added, order).await?;
    Ok(updated.map(|updated| updated.gone))
}

/// What an edit saved (`update_listing_with_drops`) leaves to do once its transaction has gone
/// through: the files of the photos it took out, by their keys in the store, which can go; and
/// those it told that the listing's price dropped (notify_price_drop), whose phones can hear of it
/// too (pluribus-services' push.rs, `after_price_drops`).
#[derive(Debug)]
pub struct ListingUpdated {
    pub gone: Vec<String>,
    pub drops: Vec<PriceDropTold>,
}

/// `update_listing`, which returns what it leaves to do whole (`ListingUpdated`): the photos'
/// files that can go, and those told that the price dropped.
pub async fn update_listing_with_drops(
    db: &PgPool,
    public_no: i64,
    listing: &NewListing<'_>,
    point: Option<(f64, f64)>,
    removed: &[Uuid],
    added: Photos<'_>,
    order: &[String],
) -> Result<Option<ListingUpdated>, NotSaved> {
    let mut tx = db.begin().await?;
    let Some(id) = sqlx::query_scalar!(
        "UPDATE listings l
         SET category_id = $3, slug = $5, title = $6, description = $7, listing_type = $8, condition = $9,
             price_amount = $10, currency = $11::text::currency_code, price_type = $12,
             price_normalized = price_in_eur($10, $11::text::currency_code),
             latitude = CASE WHEN l.location_id = $4 THEN l.latitude ELSE coalesce(loc.latitude, town.latitude) END,
             longitude = CASE WHEN l.location_id = $4 THEN l.longitude ELSE coalesce(loc.longitude, town.longitude) END,
             location_id = $4
         FROM locations loc
         LEFT JOIN locations town ON town.id = location_municipality(loc.id)
         WHERE loc.id = $4 AND l.public_no = $1 AND manages_listing(l.seller_business_id, l.created_by_user_id, $2)
           AND l.status IN ('ACTIVE', 'RESERVED', 'PAUSED')
         RETURNING l.id",
        public_no,
        listing.user_id,
        listing.category_id,
        listing.location_id,
        listing.slug,
        listing.title,
        listing.description,
        listing.listing_type,
        listing.condition,
        listing.price_amount,
        listing.currency,
        listing.price_type,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(None);
    };
    sqlx::query!("UPDATE listing_contacts SET contact_name = $2 WHERE listing_id = $1", id, listing.contact_name)
        .execute(&mut *tx)
        .await?;
    save_properties(&mut tx, id, listing).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = $1",
        id
    )
        .execute(&mut *tx)
        .await?;
    let drops = notify_price_drop(&mut tx, id).await?;
    if let Some(point) = point {
        set_listing_point(&mut tx, id, point).await?;
    }
    let gone = remove_listing_photos(&mut tx, id, removed).await?;
    add_photos(&mut tx, id, added).await?;
    if !order.is_empty() {
        order_listing_photos(&mut tx, id, order).await?;
    }
    tx.commit().await?;
    Ok(Some(ListingUpdated { gone, drops }))
}

/// What a seller can do to their own listing, besides editing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingAction {
    /// Take it down for a while (PAUSED): nobody else sees it until it is put back.
    Pause,
    /// Put a paused or sold listing back up.
    Resume,
    /// Say it is sold, or let out (SOLD, with when): its page stays, saying so, with no way to
    /// reach the seller about it, and it leaves the lists.
    Sell,
    /// Say someone is taking it (RESERVED): it stays up, saying so.
    Reserve,
    /// Take the reservation off: up as before.
    Unreserve,
    /// Take it down for good (DELETED).
    Delete,
}

/// The kinds of listing that can be sold or reserved (ListingType::SELLABLE), as the database
/// spells them, for web/listings.rs, which still compares a listing's kind with these.
pub const SELLABLE_KINDS: [&str; 2] = ListingType::names(ListingType::SELLABLE);

impl ListingAction {
    /// Whether it applies to a listing in `status`, by the rules "Njoftimet e mia" offers its
    /// buttons by (Status's): pausing one that can be paused, putting back up one that can be,
    /// selling one up (of a kind that can be sold: `kinds`), reserving one active, taking the
    /// reservation off one reserved, and deleting any not deleted yet.
    fn applies_to(self, status: Status) -> bool {
        match self {
            Self::Pause => status.pausable(),
            Self::Resume => status.resumable(),
            Self::Sell => status.up(),
            Self::Reserve => status == Status::Active,
            Self::Unreserve => status == Status::Reserved,
            Self::Delete => status != Status::Deleted,
        }
    }

    /// The status it leaves.
    fn leaves(self) -> Status {
        match self {
            Self::Pause => Status::Paused,
            Self::Resume | Self::Unreserve => Status::Active,
            Self::Sell => Status::Sold,
            Self::Reserve => Status::Reserved,
            Self::Delete => Status::Deleted,
        }
    }

    /// The kinds of listing it applies to: selling and reserving only to those that can be
    /// handed over (ListingType::SELLABLE); None for every kind.
    fn kinds(self) -> Option<&'static [ListingType]> {
        match self {
            Self::Sell | Self::Reserve | Self::Unreserve => Some(&ListingType::SELLABLE),
            Self::Pause | Self::Resume | Self::Delete => None,
        }
    }
}

/// Pauses, resumes, sells, reserves or deletes one of the user's listings, or of a store's they post
/// for (manages_listing). False when the listing isn't theirs, isn't in a state the action applies to (a deleted one, or pausing one already
/// paused), or isn't of a kind it applies to (selling a wanted ad). Selling sets when it was sold,
/// as the schema asks of a sold listing, and putting one back up clears it and gives it its
/// LISTING_DAYS again. The status history records the seller as the one who changed it.
pub async fn change_own_listing(db: &PgPool, public_no: i64, user_id: Uuid, action: ListingAction) -> sqlx::Result<bool> {
    let from: Vec<String> = Status::ALL.into_iter().filter(|status| action.applies_to(*status)).map(|status| status.to_string()).collect();
    let to = action.leaves().as_str();
    let kinds: Option<Vec<String>> = action.kinds().map(|kinds| kinds.iter().map(|kind| kind.to_string()).collect());
    let mut tx = db.begin().await?;
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", user_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    let changed = sqlx::query!(
        "UPDATE listings
         SET status = $3::text, updated_at = now(),
             sold_at = CASE WHEN $3::text = 'SOLD' THEN now() WHEN $3::text = 'ACTIVE' THEN NULL ELSE sold_at END,
             expires_at = CASE WHEN $3::text = 'ACTIVE' AND status IN ('PAUSED', 'SOLD')
                               THEN now() + make_interval(days => $6) ELSE expires_at END
         WHERE public_no = $1 AND manages_listing(seller_business_id, created_by_user_id, $2) AND status::text = ANY($4)
           AND ($5::text[] IS NULL OR listing_type::text = ANY($5))",
        public_no,
        user_id,
        to,
        &from,
        kinds.as_deref(),
        LISTING_DAYS,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    tx.commit().await?;
    Ok(changed)
}

/// "Rifresko": one of the user's listings (or of a store's they post for), up, reserved or expired, stays up for LISTING_DAYS from
/// now, an expired one back up, and comes back to the top of "newest first" if it was published
/// or last brought up (`sort_at`) BUMP_DAYS ago or more. Its notifications that it expires or
/// expired are read, and the account button loses its dot for them. False if it isn't theirs, or
/// isn't in a state to renew (paused, sold, deleted, taken off the site).
pub async fn renew_listing(db: &PgPool, public_no: i64, user_id: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", user_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    let Some(id) = sqlx::query_scalar!(
        "UPDATE listings
         SET status = CASE WHEN status = 'EXPIRED' THEN 'ACTIVE' ELSE status END,
             expires_at = now() + make_interval(days => $3),
             bumped_at = CASE WHEN sort_at < now() - make_interval(days => $4) THEN now() ELSE bumped_at END,
             updated_at = now()
         WHERE public_no = $1 AND manages_listing(seller_business_id, created_by_user_id, $2)
           AND status IN ('ACTIVE', 'RESERVED', 'EXPIRED')
         RETURNING id",
        public_no,
        user_id,
        LISTING_DAYS,
        BUMP_DAYS,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        return Ok(false);
    };
    sqlx::query!(
        "UPDATE notifications SET read_at = now()
         WHERE user_id = $1 AND notification_type IN ('EXPIRING', 'EXPIRED') AND read_at IS NULL
           AND payload ->> 'listing_id' = $2::uuid::text",
        user_id,
        id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// A row on "Njoftimet e mia".
pub struct MyListing {
    pub public_no: i64,
    pub slug: String,
    pub title: String,
    pub status: String,
    pub listing_type: String,
    pub price_amount: Option<Decimal>,
    pub currency: String,
    pub price_type: String,
    /// When it expires, if it does (LISTING_DAYS).
    pub expires_at: Option<OffsetDateTime>,
    /// When it went up; none for one that hasn't (waiting for a moderator, or for its code).
    pub published_at: Option<OffsetDateTime>,
    /// Why the moderators took it off, for its seller to read, where they did (REJECTED).
    pub rejection_reason: Option<String>,
    pub stats: ListingStats,
}

/// What listings drew, since they were posted and over the last seven days (`_week`): how many
/// times their pages were seen (views.rs, which writes the last minute's a minute later), their
/// seller's number or address was shown ("Shfaq numrin"), and how many keep them (saved_listings;
/// in the week, those kept in it).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListingStats {
    pub views: i64,
    pub views_week: i64,
    pub contacts: i64,
    pub contacts_week: i64,
    pub saves: i64,
    pub saves_week: i64,
}

impl std::iter::Sum for ListingStats {
    fn sum<I: Iterator<Item = Self>>(stats: I) -> Self {
        stats.fold(Self::default(), |all, one| Self {
            views: all.views + one.views,
            views_week: all.views_week + one.views_week,
            contacts: all.contacts + one.contacts,
            contacts_week: all.contacts_week + one.contacts_week,
            saves: all.saves + one.saves,
            saves_week: all.saves_week + one.saves_week,
        })
    }
}

/// Each listing's first photo's card-sized copy, by its number, for "Njoftimet e mia"'s rows; none
/// for a listing without one.
pub async fn first_photos(db: &PgPool, numbers: &[i64]) -> sqlx::Result<HashMap<i64, String>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT l.public_no, coalesce(m.thumbnail_url, m.public_url)
         FROM listings l
         JOIN LATERAL (
             SELECT m.thumbnail_url, m.public_url
             FROM listing_media m
             WHERE m.listing_id = l.id AND m.status = 'READY' AND m.media_type = 'IMAGE'
               AND coalesce(m.thumbnail_url, m.public_url) IS NOT NULL
             ORDER BY m.is_cover DESC, m.sort_order
             LIMIT 1
         ) m ON true
         WHERE l.public_no = ANY($1)",
    )
    .bind(numbers)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// The user's own listings, or with `store` that store's, whoever of its people posted them (the
/// caller checks the user posts for it); newest first, deleted ones left out, each with what it
/// drew: "Njoftimet e mia", and the API's `GET /me/listings`.
pub async fn my_listings(db: &PgPool, user_id: Uuid, store: Option<Uuid>) -> sqlx::Result<Vec<MyListing>> {
    let rows = sqlx::query!(
        r#"SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS "title!", l.status, l.listing_type,
                  l.price_amount, l.currency::text AS "currency!", l.price_type, l.expires_at, l.published_at,
                  l.rejection_reason,
                  coalesce(seen.views, 0)::bigint AS "views!", coalesce(seen.views_week, 0)::bigint AS "views_week!",
                  coalesce(seen.contacts, 0)::bigint AS "contacts!",
                  coalesce(seen.contacts_week, 0)::bigint AS "contacts_week!",
                  kept.saves AS "saves!", kept.saves_week AS "saves_week!"
           FROM listings l
           LEFT JOIN LATERAL (
               SELECT sum(s.detail_views) AS views,
                      sum(s.detail_views) FILTER (WHERE s.stat_date > current_date - 7) AS views_week,
                      sum(s.phone_reveals) AS contacts,
                      sum(s.phone_reveals) FILTER (WHERE s.stat_date > current_date - 7) AS contacts_week
               FROM listing_daily_stats s
               WHERE s.listing_id = l.id
           ) seen ON true
           CROSS JOIN LATERAL (
               SELECT count(*) AS saves, count(*) FILTER (WHERE k.saved_at > now() - interval '7 days') AS saves_week
               FROM saved_listings k
               WHERE k.listing_id = l.id
           ) kept
           WHERE CASE WHEN $2::uuid IS NULL THEN l.created_by_user_id = $1 AND l.seller_business_id IS NULL
                      ELSE l.seller_business_id = $2 END
             AND l.status <> 'DELETED'
           ORDER BY l.created_at DESC, l.public_no DESC"#,
        user_id,
        store,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| MyListing {
            public_no: row.public_no,
            slug: row.slug,
            title: row.title,
            status: row.status,
            listing_type: row.listing_type,
            price_amount: row.price_amount,
            currency: row.currency,
            price_type: row.price_type,
            expires_at: row.expires_at,
            published_at: row.published_at,
            rejection_reason: row.rejection_reason,
            stats: ListingStats {
                views: row.views,
                views_week: row.views_week,
                contacts: row.contacts,
                contacts_week: row.contacts_week,
                saves: row.saves,
                saves_week: row.saves_week,
            },
        })
        .collect())
}

pub async fn ping(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query("SELECT 1").execute(db).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pluribus_core::domain::{Condition, PriceType};

    /// Who posts, and where: an account, and the cars' category, Tiranë and a car's year, which
    /// the migrations put in every database.
    struct Setting {
        user: Uuid,
        cars: Uuid,
        tirane: Uuid,
        year: Uuid,
    }

    async fn setting(db: &PgPool) -> sqlx::Result<Setting> {
        let one = |sql: &'static str| sqlx::query_scalar::<_, Uuid>(sql).fetch_one(db);
        Ok(Setting {
            user: one("INSERT INTO users (auth_subject, display_name) VALUES ('posting-tests', 'Ana') RETURNING id").await?,
            cars: one("SELECT id FROM categories WHERE slug = 'makina'").await?,
            tirane: one("SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'").await?,
            year: one("SELECT id FROM attributes WHERE code = 'vehicle_year'").await?,
        })
    }

    /// A Golf for sale, at `place`, with its year.
    fn golf<'a>(setting: &Setting, place: Uuid, properties: &'a [ListingProperty]) -> NewListing<'a> {
        NewListing {
            user_id: setting.user,
            category_id: setting.cars,
            location_id: place,
            title: "Volkswagen Golf 7, 2016",
            slug: "volkswagen-golf-7-2016",
            description: "148 000 km, kambio manuale, naftë.",
            listing_type: ListingType::Sell.as_str(),
            condition: Some(Condition::Used.as_str()),
            price_amount: Some(Decimal::from(9500)),
            currency: "EUR",
            price_type: PriceType::Fixed.as_str(),
            contact_name: Some("Ana"),
            phone: Some("+355690000001"),
            email: None,
            properties,
            publish: true,
            review: None,
            business_id: None,
        }
    }

    /// A photo as the store keeps it (pluribus-services' photos.rs); one `width` 0 wide is one the
    /// database refuses (listing_media's CHECK).
    fn photo(name: &str, width: i32) -> NewPhoto {
        NewPhoto {
            storage_key: format!("photos/{name}.webp"),
            url: format!("/media/photos/{name}.webp"),
            middle_url: None,
            card_url: format!("/media/photos/{name}-card.webp"),
            jpeg_url: format!("/media/photos/{name}.jpg"),
            placeholder: "data:image/webp;base64,".to_string(),
            size_bytes: 1000,
            width,
            height: 600,
            sha256: vec![name.as_bytes()[0]; 32],
        }
    }

    /// The rows a listing makes, in every table it fills (its status's and price's history by
    /// triggers).
    async fn rows(db: &PgPool) -> sqlx::Result<Vec<(String, i64)>> {
        let tables = [
            "listings",
            "listing_contacts",
            "listing_attribute_values",
            "listing_attribute_options",
            "listing_media",
            "listing_status_history",
            "listing_price_history",
        ];
        let mut rows = Vec::new();
        for table in tables {
            let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(db).await?;
            rows.push((table.to_string(), count));
        }
        Ok(rows)
    }

    async fn nothing_left(db: &PgPool) -> sqlx::Result<()> {
        let left: Vec<(String, i64)> = rows(db).await?.into_iter().filter(|(_, count)| *count > 0).collect();
        assert!(left.is_empty(), "rows left behind: {left:?}");
        Ok(())
    }

    /// A listing saves whole or not at all: one the database refuses, at its first statement or
    /// at its last photo, leaves nothing of it, neither the listing, its contact, its properties,
    /// its photos nor its history; the same listing with photos it takes goes in with all of them
    /// and its point.
    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_listing_saves_whole_or_not_at_all(db: PgPool) -> Result<(), NotSaved> {
        let setting = setting(&db).await?;
        let properties = [ListingProperty { attribute_id: setting.year, value: PropertyValue::Number(Decimal::from(2016)) }];
        let point = Some((41.331, 19.816));

        // A place that isn't one.
        let nowhere = golf(&setting, Uuid::nil(), &properties);
        assert!(create_listing(&db, &nowhere, point, Photos::Stored(&[photo("a", 800)])).await.is_err());
        nothing_left(&db).await?;

        // A photo the database refuses, after the listing, its contact, its properties, its point
        // and the first photo went in.
        let golf = golf(&setting, setting.tirane, &properties);
        let error = create_listing(&db, &golf, point, Photos::Stored(&[photo("a", 800), photo("b", 0)])).await.unwrap_err();
        assert!(error.to_string().contains("listing_media"), "{error}");
        nothing_left(&db).await?;

        // With photos it takes, all of it.
        let (id, _) = create_listing(&db, &golf, point, Photos::Stored(&[photo("a", 800), photo("b", 640)])).await?;
        let saved: (f64, f64, String) = sqlx::query_as(
            "SELECT l.latitude::float8, l.longitude::float8,
                    (SELECT m.storage_key FROM listing_media m WHERE m.listing_id = l.id AND m.is_cover)
             FROM listings l WHERE l.id = $1",
        )
        .bind(id)
        .fetch_one(&db)
        .await?;
        assert_eq!(saved, (41.331, 19.816, "photos/a.webp".to_string()));
        let counts: Vec<i64> = rows(&db).await?.into_iter().map(|(_, count)| count).collect();
        assert_eq!((counts[0], counts[1], counts[4]), (1, 1, 2));
        assert!(counts[2] >= 1, "its year: {counts:?}");
        Ok(())
    }

    /// An edit saves whole or not at all: one whose new photo the database refuses leaves the
    /// listing as it was, its title, its point and the photo it took out; the same edit with a
    /// photo it takes changes all of them, and gives back the key of the photo taken out, for its
    /// files to go. Someone else's edit changes nothing.
    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn an_edit_saves_whole_or_not_at_all(db: PgPool) -> Result<(), NotSaved> {
        let setting = setting(&db).await?;
        let properties = [ListingProperty { attribute_id: setting.year, value: PropertyValue::Number(Decimal::from(2016)) }];
        let golf = golf(&setting, setting.tirane, &properties);
        let (id, public_no) = create_listing(&db, &golf, Some((41.331, 19.816)), Photos::Stored(&[photo("a", 800), photo("b", 640)])).await?;
        let first: Uuid = sqlx::query_scalar("SELECT id FROM listing_media WHERE storage_key = 'photos/a.webp'").fetch_one(&db).await?;
        let edited = NewListing { title: "Volkswagen Golf 7, 2016, i rregulluar", ..golf };
        let before = media_of(&db, id).await?;

        let refused = update_listing(&db, public_no, &edited, Some((41.35, 19.85)), &[first], Photos::Stored(&[photo("c", 0)]), &[]).await;
        assert!(refused.unwrap_err().to_string().contains("listing_media"));
        assert_eq!(media_of(&db, id).await?, before);

        let stranger = NewListing { user_id: Uuid::from_u128(9), ..edited };
        assert_eq!(update_listing(&db, public_no, &stranger, None, &[first], Photos::Stored(&[]), &[]).await?, None);
        assert_eq!(media_of(&db, id).await?, before);

        // In the order the form sent: the new photo first, by its key.
        let order = ["photos/c.webp".to_string()];
        let gone = update_listing(&db, public_no, &edited, Some((41.35, 19.85)), &[first], Photos::Stored(&[photo("c", 640)]), &order).await?;
        assert_eq!(gone, Some(vec!["photos/a.webp".to_string()]));
        let after = media_of(&db, id).await?;
        assert_eq!(after.0, edited.title);
        assert_eq!((after.1, after.2), (41.35, 19.85));
        assert_eq!(after.3, ["photos/c.webp", "photos/b.webp"]);
        Ok(())
    }

    /// A listing's title, its point and its photos' keys in their order.
    async fn media_of(db: &PgPool, id: Uuid) -> sqlx::Result<(String, f64, f64, Vec<String>)> {
        sqlx::query_as(
            "SELECT l.title, l.latitude::float8, l.longitude::float8,
                    array(SELECT m.storage_key FROM listing_media m WHERE m.listing_id = l.id ORDER BY m.sort_order)
             FROM listings l WHERE l.id = $1",
        )
        .bind(id)
        .fetch_one(db)
        .await
    }

    /// A listing takes the photos its seller uploaded before (photo_uploads), in the order named,
    /// each keeping its id, the first its cover, and they wait no more; one naming an upload that
    /// isn't theirs to take leaves nothing of it, and the uploads waiting as they were. An edit takes
    /// them likewise, put in its order by their ids.
    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn a_listing_takes_its_uploads_whole_or_not_at_all(db: PgPool) -> Result<(), NotSaved> {
        let setting = setting(&db).await?;
        let properties = [ListingProperty { attribute_id: setting.year, value: PropertyValue::Number(Decimal::from(2016)) }];
        let golf = golf(&setting, setting.tirane, &properties);
        let ids = add_photo_uploads(&db, setting.user, &[photo("a", 800), photo("b", 640)]).await?;
        let waiting = || sqlx::query_scalar::<_, i64>("SELECT count(*) FROM photo_uploads").fetch_one(&db);

        // Another's upload, or none at all, among them.
        let other: Uuid = sqlx::query_scalar("INSERT INTO users (auth_subject, display_name) VALUES ('posting-tests-2', 'Besa') RETURNING id")
            .fetch_one(&db)
            .await?;
        let theirs = add_photo_uploads(&db, other, &[photo("z", 800)]).await?;
        for wrong in [theirs[0], Uuid::nil()] {
            let named = [ids[0], wrong];
            let refused = create_listing(&db, &golf, None, Photos::Uploads { ids: &named, user_id: setting.user }).await;
            assert!(matches!(refused, Err(NotSaved::UploadsGone)), "{refused:?}");
            nothing_left(&db).await?;
            assert_eq!(waiting().await?, 3);
        }

        // Its own, the second first, each named once however often.
        let named = [ids[1], ids[0], ids[1]];
        let (id, public_no) = create_listing(&db, &golf, None, Photos::Uploads { ids: &named, user_id: setting.user }).await?;
        let photos: Vec<(Uuid, String, bool)> =
            sqlx::query_as("SELECT id, storage_key, is_cover FROM listing_media WHERE listing_id = $1 ORDER BY sort_order")
                .bind(id)
                .fetch_all(&db)
                .await?;
        assert_eq!(photos, [(ids[1], "photos/b.webp".to_string(), true), (ids[0], "photos/a.webp".to_string(), false)]);
        assert_eq!(waiting().await?, 1, "the other's alone");

        // An edit: one out, a new upload in, first by its id.
        let new = add_photo_uploads(&db, setting.user, &[photo("c", 800)]).await?;
        let order = [new[0].to_string(), ids[0].to_string()];
        let added = Photos::Uploads { ids: &new, user_id: setting.user };
        let gone = update_listing(&db, public_no, &golf, None, &[ids[1]], added, &order).await?;
        assert_eq!(gone, Some(vec!["photos/b.webp".to_string()]));
        assert_eq!(media_of(&db, id).await?.3, ["photos/c.webp", "photos/a.webp"]);
        let cover: Uuid = sqlx::query_scalar("SELECT id FROM listing_media WHERE listing_id = $1 AND is_cover").bind(id).fetch_one(&db).await?;
        assert_eq!(cover, new[0]);
        Ok(())
    }
}
