//! The categories and places (towns, their zones), as the pages and the forms list them.

use super::*;

#[derive(Debug, Clone)]
pub struct Category {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub default_currency: String,
    /// The kinds of listing the category takes: SELL, BUY, RENT, WANTED_RENT, SERVICE, JOB.
    pub listing_types: Vec<String>,
    /// Whether its listings have a condition (new, used...).
    pub uses_condition: bool,
}

/// The active categories in display order. There are few and most pages need several, so they
/// load in one query and the tree is walked in memory.
pub struct Categories(Vec<Category>);

impl Categories {
    /// Every category, in their order.
    pub fn iter(&self) -> impl Iterator<Item = &Category> {
        self.0.iter()
    }

    pub fn top_level(&self) -> impl Iterator<Item = &Category> {
        self.0.iter().filter(|c| c.parent_id.is_none())
    }

    pub fn children(&self, id: Uuid) -> impl Iterator<Item = &Category> {
        self.0.iter().filter(move |c| c.parent_id == Some(id))
    }

    /// Whether the category only groups subcategories, so listings go into one of those instead.
    pub fn has_children(&self, id: Uuid) -> bool {
        self.children(id).next().is_some()
    }

    pub fn by_slug(&self, slug: &str) -> Option<&Category> {
        self.0.iter().find(|c| c.slug == slug)
    }

    pub fn by_id(&self, id: Uuid) -> Option<&Category> {
        self.0.iter().find(|c| c.id == id)
    }

    /// The category and all its descendants, so browsing a parent includes its subcategories.
    pub fn subtree_ids(&self, id: Uuid) -> Vec<Uuid> {
        let mut ids = vec![id];
        let mut next = 0;
        while let Some(&parent) = ids.get(next) {
            for child in self.children(parent) {
                if !ids.contains(&child.id) {
                    ids.push(child.id);
                }
            }
            next += 1;
        }
        ids
    }

    /// The main category it is under, or itself when it is one: the part of the site it is in.
    pub fn main_of<'a>(&'a self, category: &'a Category) -> &'a Category {
        self.ancestors(category).first().copied().unwrap_or(category)
    }

    /// The categories above this one, top level first.
    pub fn ancestors(&self, category: &Category) -> Vec<&Category> {
        let mut chain: Vec<&Category> = Vec::new();
        let mut parent_id = category.parent_id;
        while let Some(parent) = parent_id.and_then(|id| self.0.iter().find(|c| c.id == id)) {
            if chain.iter().any(|c| c.id == parent.id) {
                break;
            }
            chain.push(parent);
            parent_id = parent.parent_id;
        }
        chain.reverse();
        chain
    }
}

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
    Ok(Categories(rows))
}

/// A municipality: listings are placed in one (or somewhere inside one), so these are what the
/// city filter offers. Each is named after its main town.
#[derive(Debug, Clone)]
pub struct City {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub county: String,
    /// The town's point, where its listings stand on the map without one of their own, and where
    /// the map's town dropdown moves it.
    pub latitude: f64,
    pub longitude: f64,
    /// The parts of the town people name (Blloku), which a property listing can say it is in;
    /// none for most towns (migration 0019).
    pub zones: Json<Vec<Zone>>,
}

/// A zone in a town, a place under its municipality (location_type 'AREA').
#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

/// Grouped by county, each county's main town first.
pub async fn cities(db: &PgPool) -> sqlx::Result<Vec<City>> {
    sqlx::query_as!(
        City,
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
    .await
}
