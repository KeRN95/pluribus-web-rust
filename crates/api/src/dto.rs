//! The shapes the API answers in, made from the database's rows and never the rows themselves, so
//! that a column added or renamed changes no answer unless one of these says so: a page of
//! anything, with where the next starts (`Page`), the health check's (`HealthDto`), the signed-in
//! user (`MeDto`), and a category named in both languages, which a listing's and the suggestions'
//! name (`CategoryNameDto`). Times are RFC 3339 (`time::serde::rfc3339`), with their offset. Each
//! is a schema of the OpenAPI document (`ToSchema`, openapi.rs), its doc comments the schema's
//! descriptions; a page there is one schema for each kind of item the API pages (`CardPage`,
//! `NotificationPage`).

use std::borrow::Cow;

use pluribus_core::i18n;
use pluribus_db::{AccountSettings, Category, SessionUser};
use serde::Serialize;
use time::OffsetDateTime;
use utoipa::{
    PartialSchema, ToSchema,
    openapi::{
        Ref, RefOr,
        schema::{ArrayBuilder, ObjectBuilder, Schema, SchemaType, Type},
    },
};
use uuid::Uuid;

use crate::{browse::CardDto, notifications::NotificationDto, openapi::kinds};

/// A page of a list, and where the next one starts: `next`, which the app sends back as `after`
/// (in the same order), or null on the last page.
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

impl<T: ToSchema> Page<T> {
    /// The page's schema in the OpenAPI document: its items each a reference to `T`'s own schema,
    /// where utoipa's derive on a generic type would copy `T`'s whole schema into the page's, and
    /// the client generated from it would have a second type for every item.
    fn schema() -> RefOr<Schema> {
        let items = ArrayBuilder::new().items(Ref::from_schema_name(T::name())).description(Some("The page's items, in the list's order"));
        let next = ObjectBuilder::new()
            .schema_type(SchemaType::from_iter([Type::String, Type::Null]))
            .description(Some("Where the next page starts, which the app sends back as `after` with the same query; null on the last page"));
        ObjectBuilder::new()
            .schema_type(Type::Object)
            .description(Some(format!("A page of a list of `{}`, and where the next one starts.", T::name())))
            .property("items", items)
            .required("items")
            .property("next", next)
            .into()
    }

    /// The schemas the page's refers to: its items'.
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((T::name().into_owned(), T::schema()));
        T::schemas(schemas);
    }
}

/// A page of listings' cards, `Page<CardDto>`, as the OpenAPI document names it.
pub struct CardPage;

impl PartialSchema for CardPage {
    fn schema() -> RefOr<Schema> {
        Page::<CardDto>::schema()
    }
}

impl ToSchema for CardPage {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("CardPage")
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        Page::<CardDto>::schemas(schemas);
    }
}

/// A page of notifications, `Page<NotificationDto>`, as the OpenAPI document names it.
pub struct NotificationPage;

impl PartialSchema for NotificationPage {
    fn schema() -> RefOr<Schema> {
        Page::<NotificationDto>::schema()
    }
}

impl ToSchema for NotificationPage {
    fn name() -> Cow<'static, str> {
        Cow::Borrowed("NotificationPage")
    }

    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        Page::<NotificationDto>::schemas(schemas);
    }
}

/// `GET /health`: the server and its database answer.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthDto {
    pub ok: bool,
}

/// The signed-in user (`GET /me`): who they are, from their session (`db::SessionUser`), and their
/// language and since when they have had an account, from the account (`db::account_settings`).
#[derive(Debug, Serialize, ToSchema)]
pub struct MeDto {
    pub id: Uuid,
    /// The name they show by, as they gave it ("Përdorues" until they give one).
    #[schema(examples("Ana"))]
    pub display_name: String,
    /// Their phone number, as +355691234567, if their account has one.
    #[schema(examples("+355691234567"))]
    pub phone: Option<String>,
    /// Their e-mail address, if their account has one.
    #[schema(examples("ana@example.com"))]
    pub email: Option<String>,
    /// The language their e-mails are in: "sq" or "en".
    #[schema(value_type = kinds::Language)]
    pub preferred_language: String,
    /// When they made their account.
    #[serde(with = "time::serde::rfc3339")]
    pub since: OffsetDateTime,
}

impl From<(&SessionUser, &AccountSettings)> for MeDto {
    fn from((user, account): (&SessionUser, &AccountSettings)) -> Self {
        Self {
            id: user.id,
            display_name: user.display_name.clone(),
            phone: user.phone.clone(),
            email: user.email.clone(),
            preferred_language: account.preferred_language.clone(),
            since: account.created_at,
        }
    }
}

/// A category by its slug, which `GET /listings` takes as `category`, and its name in Albanian and
/// as an English page says it: one of those above a listing's (`ListingDto::category`), or one
/// the suggestions lead to.
#[derive(Debug, PartialEq, Serialize, ToSchema)]
pub struct CategoryNameDto {
    #[schema(examples("automjete"))]
    pub slug: String,
    #[schema(examples("Automjete"))]
    pub name: String,
    #[schema(examples("Vehicles"))]
    pub name_en: String,
}

impl CategoryNameDto {
    /// The category of this slug and name (the database's), its English the catalogue's (i18n/en/),
    /// or the name itself where it has none, as an English page says it.
    pub fn named(slug: &str, name: &str) -> Self {
        Self { slug: slug.to_string(), name: name.to_string(), name_en: i18n::in_english(name).unwrap_or(name).to_string() }
    }
}

impl From<&Category> for CategoryNameDto {
    fn from(category: &Category) -> Self {
        Self::named(&category.slug, &category.name)
    }
}

#[cfg(test)]
mod tests {
    use utoipa::openapi::schema::Schema;

    use super::*;

    /// A page's schema has the page's members, `items` required and `next` nullable, as the
    /// derive has an `Option`, and its items refer to the item's schema.
    #[test]
    fn a_pages_schema_is_the_page() {
        let page = serde_json::to_value(Page { items: vec![1], next: None::<String> }).unwrap();
        let RefOr::T(Schema::Object(schema)) = CardPage::schema() else { panic!("a page's schema is an object") };
        let members: Vec<&String> = page.as_object().unwrap().keys().collect();
        assert_eq!(schema.properties.keys().collect::<Vec<_>>(), members);
        assert_eq!(schema.required, ["items"]);
        let items = serde_json::to_value(&schema.properties["items"]).unwrap();
        assert_eq!(items["items"]["$ref"], "#/components/schemas/CardDto");
        let mut schemas = Vec::new();
        NotificationPage::schemas(&mut schemas);
        assert!(schemas.iter().any(|(name, _)| name == "NotificationDto"));
    }
}
