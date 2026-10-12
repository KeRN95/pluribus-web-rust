//! The API's OpenAPI 3.1 document, the contract the phone app's client is generated from: every
//! route the router answers (lib.rs's `table`), each with its parameters, its body, every answer
//! it can give and who may call it, and every shape the API reads and answers in, made by utoipa
//! from the derives on the shapes (`ToSchema`, their doc comments the schemas' descriptions) and
//! the attributes on the handlers (`#[utoipa::path]`, each with a summary and a sentence of its
//! own); the kinds the API spells as text standing in for their types (`kinds`), and its failures
//! as responses of their own (`Unauthorized`...), each the envelope. Its paths are the routes'
//! under PREFIX, from the site's origin, which the client supplies (no `servers`). Served to anyone
//! at `GET /api/v1/openapi.json` (`document`), and committed as `openapi.json` at the repository's
//! root, pretty-printed with every object's keys sorted (`json`), so that a change to the API shows
//! in its diff: crates/api/tests/openapi.rs holds the file to what is served and the document to
//! the router, and writes the file anew with
//! `UPDATE_OPENAPI=1 cargo test -p pluribus-api --test openapi`.

use std::sync::OnceLock;

use axum::{
    http::{HeaderValue, header},
    response::IntoResponse,
};
use pluribus_core::i18n::{LANG, Lang};
use serde_json::Value;
use utoipa::{
    Modify, OpenApi, ToResponse, ToSchema,
    openapi::{
        ContentBuilder, Ref, RefOr, Response, ResponseBuilder,
        header::{Header, HeaderBuilder},
        schema::{ObjectBuilder, Type},
        security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    },
};

use crate::{
    ApiError, CardPage, CategoryNameDto, ErrorBody, ErrorEnvelope, HealthDto, MeDto, NotificationPage, account, browse,
    devices, listing, messages, notifications, own, reference, safety, saved, searches, signin, suggestions,
};

/// What the document says of the API as a whole (`info.description`).
const DESCRIPTION: &str = "The JSON API the Pluribus phone app calls. Its paths start at the \
site's origin (`https://pluribus.al/api/v1/...`), which the client supplies. Albanian is the \
default language: `Accept-Language` (the first of its languages the site speaks) or `?lang=en` \
picks English, `?lang=sq` Albanian, and every answer says in `Vary` that it differs with \
`Accept-Language` and `Authorization`. A route that needs someone signed in takes the bearer token \
`POST /api/v1/auth/verify` gives, as `Authorization: Bearer <token>` (the `bearer` scheme); one \
where signing in is optional answers the signed-in caller otherwise, and 401 for a token no good \
session has. Whatever fails is answered in one envelope, `{\"error\": {\"code\", \"message\", \
\"fields\", \"retry_after\"}}` (`ErrorEnvelope`): `code` for the app to act on, the same in every \
language; `message` for a person to read, in the language asked for; `fields`, on a validation \
error alone, each field the request named with what is wrong with it; `retry_after`, on a request \
that came too soon, the seconds to wait where they are known. Amounts are text, every digit kept; \
times are RFC 3339, with their offset.";

/// The document: the routes (`Routes`) under PREFIX, every shape and failure, the `bearer` scheme
/// (`Bearer`), no licence (`NoLicence`), and a tag for each part of the API, each a module of this
/// crate.
#[derive(OpenApi)]
#[openapi(
    info(title = "Pluribus API", version = env!("CARGO_PKG_VERSION"), description = DESCRIPTION),
    nest((path = crate::PREFIX, api = Routes)),
    components(
        schemas(
            ErrorEnvelope, ErrorBody, HealthDto, MeDto, CardPage, NotificationPage, CategoryNameDto,
            kinds::Status, kinds::ListingType, kinds::Condition, kinds::PriceType, kinds::DataType,
            kinds::NotificationKind, kinds::CarVerdict, kinds::ZoneVerdict, kinds::Activity, kinds::ErrorCode, kinds::Client,
            kinds::Platform, kinds::Channel, kinds::Language, kinds::Sort, kinds::Currency,
            account::RenameRequest, account::NoticesDto, account::LanguageRequest, account::ContactRequest,
            account::ContactCodeRequest, account::SessionDto, account::EndedDto, account::CloseRequest,
            devices::DeviceRequest,
            reference::CategoryDto, reference::CityDto, reference::ZoneDto, reference::FieldsFor, reference::FieldDto,
            reference::OptionDto, reference::ValidationDto, reference::Bound,
            browse::CardDto, browse::DetailDto, browse::PriceDto, browse::PhotoDto, browse::SellerKind,
            browse::ListingsDto, browse::CountDto, browse::HomeDto, browse::MainCategoryDto,
            suggestions::SuggestionsDto, suggestions::OptionSuggestionDto, suggestions::FilterDto,
            suggestions::PlaceSuggestionDto, suggestions::PlaceKind,
            listing::ListingDto, listing::ListingCategoryDto, listing::ListingPriceDto, listing::DropDto, listing::CarPriceDto,
            listing::AreaPriceDto, listing::LoanDto, listing::AttributeDto, listing::OptionDto,
            listing::ListingPhotoDto, listing::CopiesDto, listing::PlaceDto, listing::PointDto, listing::SellerDto,
            listing::ContactDto, listing::ContactKind, listing::Action, listing::ContactRevealedDto,
            own::UploadDto, own::PhotosForm, own::ListingInputDto, own::ListingEditDto, own::PointInput,
            own::MyListingDto, own::OwnPriceDto, own::PromotionDto, own::ZoneWordsDto, own::ZoneDto,
            own::PriceHintDto,
            notifications::NotificationDto, notifications::SeenBody, notifications::CountsDto,
            signin::CodeRequest, signin::CodeSentDto, signin::VerifyRequest, signin::SignedInDto,
            saved::SavedDto, saved::PriceDropDto, saved::GoneDto, saved::SeenBody,
            searches::SearchDto, searches::NewSearchBody, searches::AlertsBody,
            safety::ReasonDto, safety::ReportBody,
            messages::ListingHeadDto, messages::OtherDto, messages::OtherKind, messages::StoreDto,
            messages::InboxDto, messages::ConversationDto, messages::WhyNotDto, messages::MessageDto,
            messages::SentDto, messages::MessageBody, messages::ReadBody,
        ),
        responses(Unauthorized, Forbidden, NotFound, Validation, RateLimited, Internal),
    ),
    modifiers(&Bearer, &NoLicence),
    tags(
        (name = "meta", description = "The API itself: whether it answers, and this document."),
        (name = "account", description = "The signed-in user, and their account's settings as \"Cilësimet\" changes them."),
        (name = "devices", description = "The phone app's devices, which push notifications go to."),
        (name = "reference", description = "The reference data the app loads first: the categories, the towns, a category's properties; each name in both languages, each answer kept five minutes."),
        (name = "browse", description = "The listings a query finds, in the pages' own vocabulary, the front page's rows, and what a search box suggests."),
        (name = "listing", description = "One listing as its page shows it, the listings like it, its page seen, and its seller's number revealed."),
        (name = "own", description = "A seller's own listings: photos uploaded, listings posted and edited, the seller's buttons, and the posting form's helpers."),
        (name = "notifications", description = "The notifications, and what the app's badges count."),
        (name = "signin", description = "Signing in by a code sent to a phone number or an e-mail address, and signing out."),
        (name = "saved", description = "The listings a user keeps, and the drops of their prices."),
        (name = "searches", description = "Saved searches: their alerts, and their listings."),
        (name = "safety", description = "Reporting a listing, and blocking its seller."),
        (name = "messages", description = "Messages between buyers and sellers."),
    ),
)]
struct ApiDoc;

/// Every route, as its handler documents it, its path under PREFIX: in lib.rs's order.
#[derive(OpenApi)]
#[openapi(paths(
    crate::health, document, crate::me, account::rename, account::close, account::notices, account::set_notices,
    account::language, account::add_contact, account::verify_contact, account::remove_contact, account::sessions,
    account::end_other_sessions, devices::register, devices::forget, reference::categories, reference::cities,
    reference::fields, browse::listings, own::create, browse::home, suggestions::suggestions, listing::show, own::update,
    own::delete,
    listing::related, listing::seen, listing::contact, own::upload, own::mine, own::pause, own::resume, own::sell,
    own::reserve, own::unreserve, own::renew, own::zone, own::price_hint, notifications::list, notifications::seen,
    notifications::counts, signin::code, signin::verify, signin::sign_out, saved::keep, saved::let_go, saved::saved,
    saved::seen, searches::list, searches::save, searches::delete, searches::alerts, searches::listings,
    safety::reasons, safety::report, safety::block, safety::unblock, messages::inbox, messages::conversation,
    messages::start, messages::reply, messages::read, messages::block, messages::unblock,
))]
struct Routes;

/// The `bearer` scheme, which every route that signs a request in names (`security(("bearer" =
/// []))`, beside `()` where signing in is optional).
struct Bearer;

impl Modify for Bearer {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let description = "The token `POST /api/v1/auth/verify` gives, given once, as \
            `Authorization: Bearer <token>`: a session of the same table as the pages' cookie, a \
            year long on a phone, which `POST /api/v1/auth/sign-out` ends, as signing out of the \
            other devices does.";
        let scheme = HttpBuilder::new().scheme(HttpAuthScheme::Bearer).description(Some(description)).build();
        openapi.components.get_or_insert_with(Default::default).add_security_scheme("bearer", SecurityScheme::Http(scheme));
    }
}

/// No licence: utoipa writes the package's (Cargo.toml's `license`), and with none, an empty one.
struct NoLicence;

impl Modify for NoLicence {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        openapi.info.license = None;
    }
}

/// `GET /openapi.json`: this document (`json`), for anyone, kept five minutes, as the reference
/// data are (`reference::CACHE_CONTROL`).
#[utoipa::path(
    get,
    path = "/openapi.json",
    tag = "meta",
    operation_id = "getOpenApi",
    summary = "This document",
    description = "The API's OpenAPI 3.1 document, from which the app's client is generated, its keys sorted as in the repository's `openapi.json`.",
    responses(
        (
            status = 200,
            description = "The document",
            content_type = "application/json",
            body = Object,
            headers(("Cache-Control" = String, description = "`public, max-age=300`: it may be kept five minutes")),
        ),
    ),
)]
pub async fn document() -> impl IntoResponse {
    let headers = [
        (header::CONTENT_TYPE, HeaderValue::from_static("application/json")),
        (header::CACHE_CONTROL, HeaderValue::from_static(reference::CACHE_CONTROL)),
    ];
    (headers, json())
}

/// The document as JSON, as `GET /openapi.json` serves it and the repository keeps it: pretty
/// (two spaces), every object's keys sorted, and a line's end after it. Made once.
pub fn json() -> &'static str {
    static JSON: OnceLock<String> = OnceLock::new();
    JSON.get_or_init(|| {
        let document = serde_json::to_value(ApiDoc::openapi()).expect("the OpenAPI document is JSON");
        let mut json = serde_json::to_string_pretty(&sorted(document)).expect("JSON writes out");
        json.push('\n');
        json
    })
}

/// `value` with every object's keys in order, however its maps keep them.
fn sorted(value: Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut members: Vec<(String, Value)> = object.into_iter().collect();
            members.sort_by(|(one, _), (other, _)| one.cmp(other));
            Value::Object(members.into_iter().map(|(key, value)| (key, sorted(value))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
        value => value,
    }
}

/// The text of a doc comment's lines (`#[doc = "..."]`, each starting with a space), as one line.
fn prose(lines: &[&str]) -> String {
    lines.iter().map(|line| line.trim()).collect::<Vec<_>>().join(" ")
}

/// Declares the failures a route can answer with, each a response of the document's own, which
/// the routes name by its type (`(status = 401, response = Unauthorized)`): the envelope
/// (`ErrorEnvelope`), described by the type's doc comment, its example the error's envelope in
/// Albanian, the language nothing asked for, and with the header it sends, where it sends one.
macro_rules! failures {
    ($($(#[doc = $doc:literal])+ $name:ident = $error:expr, $header:expr;)+) => {
        $(
            $(#[doc = $doc])+
            pub struct $name;

            impl<'r> ToResponse<'r> for $name {
                fn response() -> (&'r str, RefOr<Response>) {
                    (stringify!($name), failure(&[$($doc),+], $error, $header))
                }
            }
        )+
    };
}

failures! {
    /// No bearer token where the route needs one, or one no good session has (wrong, expired, or
    /// its account closed or suspended): sign in again. `WWW-Authenticate: Bearer` says how.
    Unauthorized = ApiError::Unauthorized, Some(("WWW-Authenticate", "`Bearer`: the scheme that would sign the request in"));
    /// Who is asking is known, and may not: a suspended account whose sign-in code came back right.
    Forbidden = ApiError::Forbidden, None;
    /// Nothing there, or nothing the caller may see: what isn't theirs answers so.
    NotFound = ApiError::NotFound, None;
    /// What was sent isn't right: `fields` names each field the request named with what is wrong
    /// with it (`body` for a body that isn't JSON, wasn't sent as JSON or lacks a field); a refusal
    /// that is no field's says so in `message` alone.
    Validation = ApiError::Validation(vec![("body", "Trupi i kërkesës nuk është JSON i vlefshëm.".to_string())]), None;
    /// Too many requests too quickly: `retry_after` and `Retry-After` say in how many seconds to
    /// try again, where that is known.
    RateLimited = ApiError::RateLimited.retry_after(42), Some(("Retry-After", "The seconds to wait, where they are known, as `retry_after` says them"));
    /// Anything else that ended the request, a database's error among them: logged in full,
    /// answered without its details. Try again.
    Internal = ApiError::Internal(anyhow::anyhow!("an error")), None;
}

/// A failure's response: the envelope, with `error`'s as the example (in Albanian), described by
/// `doc`, and with `header`, its name and what it says, where it sends one.
fn failure(doc: &[&str], error: ApiError, header: Option<(&str, &str)>) -> RefOr<Response> {
    let example = LANG.sync_scope(Lang::Sq, || serde_json::to_value(error.envelope())).expect("the envelope is JSON");
    let content =
        ContentBuilder::new().schema(Some(Ref::from_schema_name(ErrorEnvelope::name()))).example(Some(example)).build();
    let mut response = ResponseBuilder::new().description(prose(doc)).content("application/json", content);
    if let Some((name, says)) = header {
        let header: Header = HeaderBuilder::new()
            .schema(Some(ObjectBuilder::new().schema_type(Type::String)))
            .description(Some(says))
            .build();
        response = response.header(name, header);
    }
    response.build().into()
}

pub mod kinds {
    //! The kinds the API spells as text, each a schema of its own in the document, named as what
    //! it stands for, its values listed (`enum`): pluribus-core's kinds, as the database spells
    //! them (their `NAMES`); the verdicts on a price; and the API's own words for a few things. A
    //! shape's field that holds one names it in place of its type (`#[schema(value_type =
    //! kinds::Status)]`), its type being another crate's, or a word.

    use std::borrow::Cow;

    use pluribus_core::{
        domain::{
            self,
            listing::{CarVerdict as Car, ZoneVerdict as Zone},
        },
        format::Activity as Active,
        i18n::Lang,
    };
    use pluribus_db as db;
    use pluribus_services::{filters, sender, sign_in};
    use serde::Serialize;
    use serde_json::Value;
    use utoipa::{
        PartialSchema, ToSchema,
        openapi::{
            RefOr,
            schema::{ObjectBuilder, Schema, Type},
        },
    };

    use super::prose;
    use crate::{ApiError, devices, signin};

    /// Every verdict on a car's price, in their order (a test holds the list to the type).
    pub(super) const CAR_VERDICTS: [Car; 3] = [Car::Good, Car::Near, Car::Above];

    /// Every verdict on a property's price per square metre, in their order.
    pub(super) const ZONE_VERDICTS: [Zone; 4] = [Zone::Below, Zone::Same, Zone::Near, Zone::Above];

    /// Every way of saying when someone was last on the site, in their order.
    pub(super) const ACTIVITIES: [Active; 4] = [Active::Today, Active::Week, Active::Month, Active::Longer];

    /// Every kind of session, in their order.
    pub(super) const CLIENTS: [sign_in::Client; 3] = [sign_in::Client::Web, sign_in::Client::Ios, sign_in::Client::Android];

    /// Every way a code goes, in their order.
    pub(super) const CHANNELS: [sender::Channel; 3] = [sender::Channel::WhatsApp, sender::Channel::Email, sender::Channel::Log];

    /// Every language the site speaks, in their order.
    pub(super) const LANGUAGES: [Lang; 2] = [Lang::Sq, Lang::En];

    /// Each of `kinds` as serde spells it, which the API's answers do.
    fn spelt<K: Serialize>(kinds: &[K]) -> Vec<Value> {
        kinds.iter().map(|kind| serde_json::to_value(kind).expect("a kind is spelt as text")).collect()
    }

    /// A text that is one of `values`, described by a doc comment's `lines`.
    fn one_of<V: Into<Value>>(lines: &[&str], values: impl IntoIterator<Item = V>) -> RefOr<Schema> {
        ObjectBuilder::new().schema_type(Type::String).description(Some(prose(lines))).enum_values(Some(values)).into()
    }

    /// Declares kinds: each a unit type named as the kind, whose schema is a text that is one of
    /// its values, its doc comment the schema's description.
    macro_rules! kinds {
        ($($(#[doc = $doc:literal])+ $kind:ident = $values:expr;)+) => {
            $(
                $(#[doc = $doc])+
                pub struct $kind;

                impl PartialSchema for $kind {
                    fn schema() -> RefOr<Schema> {
                        one_of(&[$($doc),+], $values)
                    }
                }

                impl ToSchema for $kind {
                    fn name() -> Cow<'static, str> {
                        Cow::Borrowed(stringify!($kind))
                    }
                }
            )+
        };
    }

    kinds! {
        /// Where a listing stands, as the database spells it: DRAFT (waiting for its seller's
        /// code), PENDING_REVIEW (waiting for a moderator), ACTIVE (up), PAUSED (down for a while,
        /// by its seller), RESERVED (up, and someone is taking it), SOLD (sold or let out, its page
        /// staying), EXPIRED (its time ran out), REJECTED (refused or taken off by a moderator),
        /// SUSPENDED (off the site for its reports, or with its seller's account), DELETED.
        Status = domain::Status::NAMES;
        /// What a listing offers or asks for: SELL (on sale), BUY (someone wants to buy), RENT (for
        /// rent, its price a month's), WANTED_RENT (someone wants to rent), SERVICE, or JOB (its
        /// price a month's pay).
        ListingType = domain::ListingType::NAMES;
        /// What state a thing is in, in the categories that ask: NEW, LIKE_NEW, USED or FOR_PARTS.
        Condition = domain::Condition::NAMES;
        /// How a listing's price is meant: FIXED, NEGOTIABLE, FROM (the price it starts from),
        /// FREE, or ON_REQUEST (no price given, agreed with the seller).
        PriceType = domain::PriceType::NAMES;
        /// What a property's values are: TEXT, NUMBER, BOOLEAN, DATE, SELECT (one of its options)
        /// or MULTISELECT (any of them).
        DataType = domain::DataType::NAMES;
        /// What a notification is about: PRICE_DROP (a listing kept dropped its price),
        /// SAVED_SEARCH (a saved search has new listings), NEW_MESSAGE (a message waits unread),
        /// EXPIRING and EXPIRED (one of the user's listings), PROMOTED (a moderator promoted one of
        /// them), or STORE_INVITE (the user is asked into a store).
        NotificationKind = domain::NotificationKind::NAMES;
        /// How a car's price compares with what cars of its model and year ask: good (15 % or more
        /// under their median), near (within 15 % of it), or above (15 % or more over it).
        CarVerdict = spelt(&CAR_VERDICTS);
        /// How a property's price per square metre compares with its zone's average: below (by
        /// 200 € or more), same (the same, as both show rounded), near (within 200 €), or above (by
        /// 200 € or more).
        ZoneVerdict = spelt(&ZONE_VERDICTS);
        /// When someone was last on the site, by Albania's clock: today, within the week (from
        /// yesterday to six days ago), within the month (from a week to thirty days ago), or
        /// longer ago.
        Activity = spelt(&ACTIVITIES);
        /// Why a request failed, the same in every language, for the app to act on: unauthorized
        /// (401), forbidden (403), not_found (404), validation (422), rate_limited (429) or
        /// internal (500).
        ErrorCode = ApiError::CODES;
        /// What a session is for: web (the pages), or the phone app on ios or android.
        Client = CLIENTS.map(sign_in::Client::as_str);
        /// The phone app's platform, which its push notifications go through: ios or android.
        Platform = devices::PLATFORMS.map(|(platform, _)| platform);
        /// How a code went: whatsapp, email, or log, where its channel isn't set up (the server's
        /// log has it, as in development).
        Channel = CHANNELS.map(signin::channel);
        /// A language the site speaks: sq (Albanian) or en (English).
        Language = LANGUAGES.map(Lang::code);
        /// The orders a list of listings comes in: relevance (the best match for the words first;
        /// only with words), newest, price_asc, price_desc, or nearest (to the pin first; only with
        /// one).
        Sort = db::Sort::ALL.map(db::Sort::param);
        /// A currency a price is asked or filtered in: EUR or ALL (lekë).
        Currency = filters::CURRENCIES;
    }
}

#[cfg(test)]
mod tests {
    use pluribus_core::{
        domain::listing::{CarVerdict, ZoneVerdict},
        format::Activity as Active,
    };
    use pluribus_services::{sender::Channel, sign_in::Client};

    use super::kinds::*;

    /// The lists of a kind's values that aren't the kind's own (its `NAMES`) hold every one of
    /// them: each match below names them all, so that one added to its type fails to compile here
    /// until its list has it too, at the place its arm says.
    #[test]
    fn the_lists_of_a_kinds_values_hold_every_one() {
        let car = |verdict| match verdict {
            CarVerdict::Good => 0,
            CarVerdict::Near => 1,
            CarVerdict::Above => 2,
        };
        assert_eq!(CAR_VERDICTS.map(car), [0, 1, 2]);
        let zone = |verdict| match verdict {
            ZoneVerdict::Below => 0,
            ZoneVerdict::Same => 1,
            ZoneVerdict::Near => 2,
            ZoneVerdict::Above => 3,
        };
        assert_eq!(ZONE_VERDICTS.map(zone), [0, 1, 2, 3]);
        let active = |active| match active {
            Active::Today => 0,
            Active::Week => 1,
            Active::Month => 2,
            Active::Longer => 3,
        };
        assert_eq!(ACTIVITIES.map(active), [0, 1, 2, 3]);
        let client = |client| match client {
            Client::Web => 0,
            Client::Ios => 1,
            Client::Android => 2,
        };
        assert_eq!(CLIENTS.map(client), [0, 1, 2]);
        let channel = |channel| match channel {
            Channel::WhatsApp => 0,
            Channel::Email => 1,
            Channel::Log => 2,
        };
        assert_eq!(CHANNELS.map(channel), [0, 1, 2]);
        let language = |language| match language {
            pluribus_core::i18n::Lang::Sq => 0,
            pluribus_core::i18n::Lang::En => 1,
        };
        assert_eq!(LANGUAGES.map(language), [0, 1]);
    }
}
