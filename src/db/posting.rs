//! Posting and editing listings, pausing and taking them down.

use super::*;

/// How many listings the user posted in the last day, and whether one of them has this title.
pub struct RecentPosts {
    pub count: i64,
    pub same_title: bool,
}

pub async fn recent_posts(db: &PgPool, user_id: Uuid, title: &str) -> sqlx::Result<RecentPosts> {
    sqlx::query_as!(
        RecentPosts,
        r#"SELECT count(*) AS "count!",
                  coalesce(bool_or(lower(title) = lower($2)), false) AS "same_title!"
           FROM listings
           WHERE created_by_user_id = $1 AND created_at > now() - interval '1 day'"#,
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

/// Saves a listing at its place's point on the map (its town's, for a zone without one), with the
/// seller's contact, published at once or as a draft (`publish`), and returns its id and number.
pub async fn create_listing(db: &PgPool, listing: &NewListing<'_>) -> sqlx::Result<(Uuid, i64)> {
    let mut tx = db.begin().await?;
    let row = sqlx::query!(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title,
                               description, listing_type, condition, price_amount, currency, price_type,
                               price_normalized, latitude, longitude, status, published_at)
         SELECT $1, $1, $2, loc.id, $4, $5, $6, $7, $8, $9, $10::text::currency_code, $11,
                price_in_eur($9, $10::text::currency_code),
                coalesce(loc.latitude, town.latitude), coalesce(loc.longitude, town.longitude),
                CASE WHEN $12 THEN 'ACTIVE' ELSE 'DRAFT' END, CASE WHEN $12 THEN now() END
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
    tx.commit().await?;
    Ok((row.id, row.public_no))
}

/// One of the user's own listings, as the edit form shows it. Only listings they created, and only
/// while they are up (active or reserved), can be edited.
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
           WHERE l.public_no = $1 AND l.created_by_user_id = $2 AND l.status IN ('ACTIVE', 'RESERVED')"#,
        public_no,
        user_id,
    )
    .fetch_optional(db)
    .await
}

/// Saves an edit, if the listing is the user's and still up; returns whether it did. A listing
/// keeps its point on the map unless it moves to another town, and its place in "newest first".
pub async fn update_listing(db: &PgPool, public_no: i64, listing: &NewListing<'_>) -> sqlx::Result<bool> {
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
         WHERE loc.id = $4 AND l.public_no = $1 AND l.created_by_user_id = $2 AND l.status IN ('ACTIVE', 'RESERVED')
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
        return Ok(false);
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
    notify_price_drop(&mut tx, id).await?;
    tx.commit().await?;
    Ok(true)
}

/// What a seller can do to their own listing, besides editing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListingAction {
    /// Take it down for a while (PAUSED): nobody else sees it until it is put back.
    Pause,
    /// Put a paused listing back up.
    Resume,
    /// Take it down for good (DELETED).
    Delete,
}

impl ListingAction {
    /// The statuses the action applies to, and the one it leaves.
    fn statuses(self) -> (&'static [&'static str], &'static str) {
        match self {
            Self::Pause => (&["ACTIVE", "RESERVED"], "PAUSED"),
            Self::Resume => (&["PAUSED"], "ACTIVE"),
            Self::Delete => (&["DRAFT", "ACTIVE", "RESERVED", "SOLD", "EXPIRED", "PAUSED", "SUSPENDED", "REJECTED"], "DELETED"),
        }
    }
}

/// Pauses, resumes or deletes one of the user's listings. False when the listing isn't theirs, or
/// isn't in a state the action applies to (a deleted one, or pausing one already paused). The
/// status history records the seller as the one who changed it.
pub async fn change_own_listing(db: &PgPool, public_no: i64, user_id: Uuid, action: ListingAction) -> sqlx::Result<bool> {
    let (from, to) = action.statuses();
    let from: Vec<String> = from.iter().map(|status| status.to_string()).collect();
    let mut tx = db.begin().await?;
    sqlx::query!("SELECT set_config('app.user_id', $1, true)", user_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
    let changed = sqlx::query!(
        "UPDATE listings SET status = $3, updated_at = now()
         WHERE public_no = $1 AND created_by_user_id = $2 AND status::text = ANY($4)",
        public_no,
        user_id,
        to,
        &from,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    tx.commit().await?;
    Ok(changed)
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

/// The user's listings, newest first, deleted ones left out, each with what it drew.
pub async fn my_listings(db: &PgPool, user_id: Uuid) -> sqlx::Result<Vec<MyListing>> {
    let rows = sqlx::query!(
        r#"SELECT l.public_no, l.slug, coalesce(l.generated_title, l.title) AS "title!", l.status, l.listing_type,
                  l.price_amount, l.currency::text AS "currency!", l.price_type,
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
           WHERE l.created_by_user_id = $1 AND l.status <> 'DELETED'
           ORDER BY l.created_at DESC, l.public_no DESC"#,
        user_id,
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
