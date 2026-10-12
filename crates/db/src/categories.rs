//! The categories and places (towns, their zones), as the pages and the forms list them: read
//! into the domain's types (pluribus-core's domain::categories), which are re-exported here, so
//! each is `db::...` as before.

use super::*;

pub use pluribus_core::domain::{Categories, Category, City, Zone};

pub async fn categories(db: &PgPool) -> sqlx::Result<Categories> {
    let rows = sqlx::query_as!(
        Category,
        r#"SELECT id, parent_id, name, slug, default_currency::text AS "default_currency!",
                  listing_types, uses_condition
           FROM categories
           WHERE active
           ORDER BY sort_order, name"#
    )
    .fetch_all(db)
    .await?;
    Ok(Categories::new(rows))
}

/// Grouped by county, each county's main town first.
pub async fn cities(db: &PgPool) -> sqlx::Result<Vec<City>> {
    let rows = sqlx::query!(
        r#"SELECT m.id, m.name, m.slug, county.name AS county,
                m.latitude::float8 AS "latitude!", m.longitude::float8 AS "longitude!",
                coalesce((SELECT json_agg(json_build_object('id', z.id, 'name', z.name, 'slug', z.slug) ORDER BY z.name)
                          FROM locations z
                          WHERE z.parent_id = m.id AND z.location_type = 'AREA' AND z.active), '[]') AS "zones!: Json<Vec<Zone>>"
         FROM locations m
         JOIN locations county ON county.id = m.parent_id
         WHERE m.location_type = 'MUNICIPALITY' AND m.active
         ORDER BY county.name, m.slug <> county.slug, m.name"#
    )
    .fetch_all(db)
    .await?;
    // The zones come as JSON (json_agg); a City keeps them as they are.
    Ok(rows
        .into_iter()
        .map(|row| City {
            id: row.id,
            name: row.name,
            slug: row.slug,
            county: row.county,
            latitude: row.latitude,
            longitude: row.longitude,
            zones: row.zones.0,
        })
        .collect())
}
