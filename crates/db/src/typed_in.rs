//! What sellers type in where a list lacks theirs ("Tjetër"), and how moderators approve or merge
//! it.

use super::*;

/// A make, model or brand a seller typed in ("Tjetër") that waits for a moderator (/moderim).
pub struct PendingOption {
    pub id: Uuid,
    /// The property's own name: "Marka", "Modeli".
    pub attribute: String,
    pub label: String,
    /// A model's make.
    pub parent_label: Option<String>,
    /// The make waits too, and goes first.
    pub parent_pending: bool,
    pub added_by: Option<String>,
    /// The listings that have it, deleted ones aside.
    pub listings: i64,
}

/// What sellers typed in and nobody has approved yet: makes before models.
pub async fn pending_options(db: &PgPool) -> sqlx::Result<Vec<PendingOption>> {
    sqlx::query_as!(
        PendingOption,
        r#"SELECT o.id, a.name AS attribute, o.label, parent.label AS "parent_label?",
                  coalesce(NOT parent.active, false) AS "parent_pending!", u.display_name AS "added_by?",
                  (SELECT count(*) FROM listing_attribute_values v JOIN listings l ON l.id = v.listing_id
                   WHERE v.value_option_id = o.id AND l.status <> 'DELETED') AS "listings!"
           FROM attribute_options o
           JOIN attributes a ON a.id = o.attribute_id
           LEFT JOIN attribute_options parent ON parent.id = o.parent_option_id
           LEFT JOIN users u ON u.id = o.added_by_user_id
           WHERE NOT o.active AND o.added_by_user_id IS NOT NULL
           ORDER BY o.parent_option_id IS NOT NULL, a.name, lower(parent.label), lower(o.label)"#
    )
    .fetch_all(db)
    .await
}

/// How many wait, for the link to /moderim.
pub async fn pending_option_count(db: &PgPool) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM attribute_options WHERE NOT active AND added_by_user_id IS NOT NULL"#
    )
    .fetch_one(db)
    .await
}

/// A listed option a waiting one can be merged into: the same property, under the same make.
pub struct MergeTarget {
    pub pending_id: Uuid,
    pub id: Uuid,
    pub label: String,
    /// How alike the two names are, 0 to 1 (pg_trgm): "Mercedes" and Mercedes-Benz are.
    pub similarity: f32,
}

pub async fn merge_targets(db: &PgPool) -> sqlx::Result<Vec<MergeTarget>> {
    sqlx::query_as!(
        MergeTarget,
        r#"SELECT o.id AS pending_id, t.id, t.label, similarity(lower(t.label), lower(o.label)) AS "similarity!"
           FROM attribute_options o
           JOIN attribute_options t
             ON t.attribute_id = o.attribute_id AND t.active AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
           WHERE NOT o.active AND o.added_by_user_id IS NOT NULL
           ORDER BY o.id, t.sort_order, t.label"#
    )
    .fetch_all(db)
    .await
}

pub enum Approval {
    Approved,
    /// A listed option of the same property and make has that name already: merge, rather.
    Taken,
    /// It doesn't wait (any more), or its make still does.
    NotPending,
}

/// Approves a typed option under `label`, its spelling put right: the lists and filters offer it
/// from now on, after the listed names that come before it alphabetically.
pub async fn approve_option(db: &PgPool, id: Uuid, label: &str) -> sqlx::Result<Approval> {
    let mut tx = db.begin().await?;
    let taken = sqlx::query_scalar!(
        r#"SELECT EXISTS (
               SELECT 1 FROM attribute_options o
               JOIN attribute_options t
                 ON t.attribute_id = o.attribute_id AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
                AND t.active AND t.id <> o.id AND lower(t.label) = lower($2)
               WHERE o.id = $1
           ) AS "taken!""#,
        id,
        label,
    )
    .fetch_one(&mut *tx)
    .await?;
    if taken {
        return Ok(Approval::Taken);
    }
    let approved = sqlx::query_scalar!(
        "UPDATE attribute_options o
         SET active = true, label = $2::text,
             sort_order = coalesce((SELECT max(s.sort_order) FROM attribute_options s
                                    WHERE s.attribute_id = o.attribute_id AND s.active
                                      AND s.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
                                      AND lower(s.label) < lower($2::text)), 0) + 1
         WHERE o.id = $1 AND NOT o.active AND o.added_by_user_id IS NOT NULL
           AND (o.parent_option_id IS NULL
                OR EXISTS (SELECT 1 FROM attribute_options p WHERE p.id = o.parent_option_id AND p.active))
         RETURNING o.id",
        id,
        label,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if approved.is_none() {
        return Ok(Approval::NotPending);
    }
    // A category that offers only some of the property's options (category_options: a phone's
    // brands) offers this one too where it was typed in, after the others but before "Tjetër".
    sqlx::query!(
        "INSERT INTO category_attribute_options (category_id, option_id, sort_order)
         SELECT DISTINCT l.category_id, o.id,
                coalesce((SELECT max(s.sort_order) FROM category_attribute_options s
                          JOIN attribute_options so ON so.id = s.option_id
                          WHERE s.category_id = l.category_id AND so.attribute_id = o.attribute_id
                            AND so.value_code <> 'OTHER'), 0) + 1
         FROM attribute_options o
         JOIN listing_attribute_values v ON v.value_option_id = o.id
         JOIN listings l ON l.id = v.listing_id
         WHERE o.id = $1
           AND EXISTS (SELECT 1 FROM category_attribute_options s
                       JOIN attribute_options so ON so.id = s.option_id
                       WHERE s.category_id = l.category_id AND so.attribute_id = o.attribute_id)
         ON CONFLICT DO NOTHING",
        id,
    )
    .execute(&mut *tx)
    .await?;
    // Search reads the names from search_extra (listing_search_extra), which has the old spelling,
    // and a car's title from generated_title.
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id IN (SELECT listing_id FROM listing_attribute_values WHERE value_option_id = $1)",
        id,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Approval::Approved)
}

/// Merges a typed option into a listed one of the same property and make, the one the seller
/// meant: its listings get the listed one, a typed make's typed models move under it, and the
/// typed one is gone. False when either isn't what it should be.
pub async fn merge_option(db: &PgPool, id: Uuid, into: Uuid) -> sqlx::Result<bool> {
    let mut tx = db.begin().await?;
    let pending = sqlx::query_scalar!(
        "SELECT o.id FROM attribute_options o
         JOIN attribute_options t
           ON t.id = $2 AND t.active AND t.attribute_id = o.attribute_id
          AND t.parent_option_id IS NOT DISTINCT FROM o.parent_option_id
         WHERE o.id = $1 AND NOT o.active AND o.added_by_user_id IS NOT NULL
         FOR UPDATE OF o",
        id,
        into,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if pending.is_none() {
        return Ok(false);
    }
    let listings = sqlx::query_scalar!(
        "UPDATE listing_attribute_values SET value_option_id = $2 WHERE value_option_id = $1 RETURNING listing_id",
        id,
        into,
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!("UPDATE attribute_options SET parent_option_id = $2 WHERE parent_option_id = $1", id, into)
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM attribute_options WHERE id = $1", id).execute(&mut *tx).await?;
    sqlx::query!(
        "UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
         WHERE id = ANY($1)",
        &listings,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}
