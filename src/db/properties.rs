//! Categories' properties: what the filters and the posting form offer, and what a listing says.

use super::*;

/// A property a category can be filtered on.
#[derive(Debug)]
pub struct FilterAttribute {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    /// SELECT, MULTISELECT, NUMBER or BOOLEAN.
    pub data_type: String,
    pub unit: Option<String>,
    /// Set for dependent properties, like a car model, whose options each belong to an option
    /// of the parent property (the make).
    pub parent_attribute_id: Option<Uuid>,
    pub options: Json<Vec<AttributeOption>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AttributeOption {
    pub id: Uuid,
    pub code: String,
    pub label: String,
    pub parent_id: Option<Uuid>,
}

/// The category's filterable properties, its own and inherited, in display order, each under the
/// category's name for it (category_attributes.label) where it has one, with the options it offers
/// (category_options: a phone's brands, not a camera's). Looked up by slug, so a page can load them
/// alongside the categories.
pub async fn filter_attributes(
    db: &PgPool,
    category_slug: &str,
) -> sqlx::Result<Vec<FilterAttribute>> {
    sqlx::query_as!(
        FilterAttribute,
        r#"SELECT a.id, a.code, coalesce(eca.label, a.name) AS "name!", a.data_type, a.unit, a.parent_attribute_id,
                  coalesce(jsonb_agg(jsonb_build_object('id', o.id, 'code', o.value_code,
                                                         'label', o.label, 'parent_id', o.parent_option_id)
                                     ORDER BY o.sort_order, o.label)
                               FILTER (WHERE o.id IS NOT NULL),
                           '[]') AS "options!: Json<Vec<AttributeOption>>"
           FROM effective_category_attributes eca
           JOIN attributes a ON a.id = eca.attribute_id
           LEFT JOIN LATERAL category_options(eca.category_id, a.id) o ON true
           WHERE eca.category_id = (SELECT id FROM categories WHERE slug = $1 AND active)
             AND eca.filterable
             AND a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN')
           GROUP BY a.id, eca.sort_order, eca.label
           ORDER BY eca.sort_order, a.name"#,
        category_slug
    )
    .fetch_all(db)
    .await
}

/// A property as the listing form asks for it.
#[derive(Debug)]
pub struct FormAttribute {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    /// SELECT, MULTISELECT, NUMBER, BOOLEAN or TEXT.
    pub data_type: String,
    pub unit: Option<String>,
    pub parent_attribute_id: Option<Uuid>,
    pub required: bool,
    /// The seller may type an option the list lacks ("Tjetër"), like a car make.
    pub allow_other: bool,
    pub min: Option<Decimal>,
    pub max: Option<Decimal>,
    pub options: Json<Vec<AttributeOption>>,
}

/// The properties a listing in this category can have, its own and inherited, in form order, under
/// the category's name for them where it has one, with the options it offers (category_options).
/// Not the subcategory's kind (the old "type" property): the subcategory sets that one.
pub async fn form_attributes(db: &PgPool, category_id: Uuid) -> sqlx::Result<Vec<FormAttribute>> {
    sqlx::query_as!(
        FormAttribute,
        r#"SELECT a.id, a.code, coalesce(eca.label, a.name) AS "name!", a.data_type, a.unit, a.parent_attribute_id,
                  eca.required AS "required!",
                  a.allow_other,
                  (a.validation_json ->> 'min')::numeric AS min, (a.validation_json ->> 'max')::numeric AS max,
                  coalesce(jsonb_agg(jsonb_build_object('id', o.id, 'code', o.value_code,
                                                         'label', o.label, 'parent_id', o.parent_option_id)
                                     ORDER BY o.sort_order, o.label)
                               FILTER (WHERE o.id IS NOT NULL),
                           '[]') AS "options!: Json<Vec<AttributeOption>>"
           FROM effective_category_attributes eca
           JOIN attributes a ON a.id = eca.attribute_id
           LEFT JOIN LATERAL category_options(eca.category_id, a.id) o ON true
           WHERE eca.category_id = $1
             AND a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN', 'TEXT')
             AND NOT EXISTS (SELECT 1 FROM categories k JOIN attribute_options ko ON ko.id = k.replaces_option_id
                             WHERE ko.attribute_id = a.id)
           GROUP BY a.id, eca.sort_order, eca.required, eca.label
           ORDER BY eca.sort_order, a.name"#,
        category_id
    )
    .fetch_all(db)
    .await
}

/// A property listings can have, with its options, for the AI that fills in the listing form
/// (web/ai_fill.rs): what form_attributes gives a category's form, for every category at once.
#[derive(Debug)]
pub struct CatalogueProperty {
    pub code: String,
    pub name: String,
    /// SELECT, MULTISELECT, NUMBER, BOOLEAN or TEXT.
    pub data_type: String,
    pub unit: Option<String>,
    /// The property each of this one's options belongs under an option of: a model's make.
    pub parent_code: Option<String>,
    pub min: Option<Decimal>,
    pub max: Option<Decimal>,
    /// Under their parent's options, in their order, where they have a parent.
    pub options: Json<Vec<CatalogueOption>>,
}

#[derive(Debug, Deserialize)]
pub struct CatalogueOption {
    pub code: String,
    pub label: String,
    /// The code of the parent property's option it belongs under: a model's make.
    pub parent: Option<String>,
}

pub async fn catalogue_properties(db: &PgPool) -> sqlx::Result<Vec<CatalogueProperty>> {
    sqlx::query_as!(
        CatalogueProperty,
        r#"SELECT a.code, a.name, a.data_type, a.unit, parent.code AS "parent_code?",
                  (a.validation_json ->> 'min')::numeric AS min, (a.validation_json ->> 'max')::numeric AS max,
                  coalesce(jsonb_agg(jsonb_build_object('code', o.value_code, 'label', o.label, 'parent', po.value_code)
                                     ORDER BY po.sort_order, po.label, o.sort_order, o.label)
                               FILTER (WHERE o.id IS NOT NULL),
                           '[]') AS "options!: Json<Vec<CatalogueOption>>"
           FROM attributes a
           LEFT JOIN attributes parent ON parent.id = a.parent_attribute_id
           LEFT JOIN attribute_options o ON o.attribute_id = a.id AND o.active
           LEFT JOIN attribute_options po ON po.id = o.parent_option_id
           WHERE a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN', 'TEXT')
           GROUP BY a.id, parent.code
           ORDER BY a.code"#
    )
    .fetch_all(db)
    .await
}

/// Each category's properties in the listing form, by code and in the form's order, as
/// form_attributes gives them: for the AI that fills in the form (web/ai_fill.rs).
pub async fn category_properties(db: &PgPool) -> sqlx::Result<HashMap<Uuid, Vec<String>>> {
    let rows = sqlx::query!(
        r#"SELECT eca.category_id AS "category_id!", array_agg(a.code ORDER BY eca.sort_order, a.name) AS "codes!"
           FROM effective_category_attributes eca
           JOIN attributes a ON a.id = eca.attribute_id
           WHERE a.data_type IN ('SELECT', 'MULTISELECT', 'NUMBER', 'BOOLEAN', 'TEXT')
             AND NOT EXISTS (SELECT 1 FROM categories k JOIN attribute_options ko ON ko.id = k.replaces_option_id
                             WHERE ko.attribute_id = a.id)
           GROUP BY eca.category_id"#
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|row| (row.category_id, row.codes)).collect())
}

/// A listing's property values as the form holds them: option codes, numbers as typed, "1" for a
/// ticked box; several rows for a multi-select. An option the seller typed, not yet in the lists,
/// comes back as "Tjetër" ("__other__") with its text under "<code>_other".
pub async fn listing_property_values(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Vec<(String, String)>> {
    let rows = sqlx::query!(
        r#"SELECT a.code AS "code!",
                  CASE WHEN o.id IS NOT NULL AND NOT o.active THEN '__other__'
                       ELSE coalesce(o.value_code, trim_scale(v.value_number)::text, v.value_text,
                                     CASE WHEN v.value_boolean THEN '1' END)
                  END AS value
           FROM listing_attribute_values v
           JOIN attributes a ON a.id = v.attribute_id
           LEFT JOIN attribute_options o ON o.id = v.value_option_id
           WHERE v.listing_id = $1
           UNION ALL
           SELECT a.code || '_other', o.label
           FROM listing_attribute_values v
           JOIN attributes a ON a.id = v.attribute_id
           JOIN attribute_options o ON o.id = v.value_option_id AND NOT o.active
           WHERE v.listing_id = $1
           UNION ALL
           SELECT a.code, o.value_code
           FROM listing_attribute_options lo
           JOIN attributes a ON a.id = lo.attribute_id
           JOIN attribute_options o ON o.id = lo.option_id
           WHERE lo.listing_id = $1"#,
        listing_id
    )
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().filter_map(|row| Some((row.code, row.value?))).collect())
}
