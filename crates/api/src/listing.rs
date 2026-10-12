//! One listing, as the phone app shows it: what its page shows, as data, said in the language the
//! request asks for where it says something (`GET /listings/{no}`, `ListingDto`), made by the
//! rules and the figures the page is made by (domain::Status, domain::listing, pluribus-services'
//! listings.rs, which C5 moved out of the pages for both); the listings like it, a page at a time,
//! as its page's frames go on (`GET /listings/{no}/related`); its page seen, for the view counts
//! (`POST /listings/{no}/seen`); and its seller's number or address revealed, counted as the page's
//! button counts it (`POST /listings/{no}/contact`). A listing off the site is its seller's and the
//! moderators' alone, as on the pages: for anyone else it isn't found.

use std::str::FromStr;

use anyhow::Context;
use axum::{
    Json,
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
};
use pluribus_core::{
    domain::{
        Condition, ListingType, PriceType, Status, UnknownKind,
        listing::{self as rules, CarVerdict, LOAN_DOWN_PERCENT, MAPPED, ZoneVerdict},
    },
    format::{self, Activity},
    i18n,
};
use pluribus_db::{self as db, AttributeValue, ListingDetail, Photo, SellerContact, SessionUser};
use pluribus_services::{
    AppState,
    filters::Params,
    listings::{CarPrices, owns, shown, visitor},
    media::{CARD_AVIF, CARD_COPY, MINI_AVIF, MINI_COPY, SMALL_AVIF, SMALL_COPY},
    photos::card_copy,
};
use rust_decimal::Decimal;
use serde::Serialize;
use time::{OffsetDateTime, macros::format_description};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, CardPage, CategoryNameDto, OptionalApiUser, Page,
    browse::{self, CardDto, Caller, SellerKind},
    openapi::{Internal, NotFound, Unauthorized, Validation, kinds},
    signin::ClientIp,
};

/// `GET /listings/{no}`: a listing as its page shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListingDto {
    /// Its number, which its page and these routes go by.
    #[schema(examples(1001))]
    pub no: i64,
    #[schema(examples("volkswagen-golf-1-6-2016"))]
    pub slug: String,
    /// What its page calls it: a car's made from what the car is ("Volkswagen Golf 1.6, 2016"),
    /// another's as its seller wrote it.
    #[schema(examples("Volkswagen Golf 1.6, 2016"))]
    pub title: String,
    /// A car's title as its seller wrote it, where `title` was made: what its page heads its
    /// description with; none for the others.
    pub heading: Option<String>,
    /// Its category, and the way up from it, as its page's breadcrumb goes.
    #[schema(required = false)]
    pub category: ListingCategoryDto,
    /// Whether its page is laid out as a car's or a property's (domain::listing::MAPPED): its
    /// photos a mosaic, its key facts in a row, where it is on a map (`place.point`).
    pub deal: bool,
    /// What it offers or asks for: SELL, BUY, RENT, WANTED_RENT, SERVICE or JOB.
    #[schema(value_type = kinds::ListingType)]
    pub kind: ListingType,
    /// What state the thing is in, in the categories that ask: NEW, LIKE_NEW, USED or FOR_PARTS.
    #[schema(value_type = Option<kinds::Condition>)]
    pub condition: Option<Condition>,
    /// Where it stands: ACTIVE, RESERVED or SOLD for anyone; PAUSED, EXPIRED, PENDING_REVIEW,
    /// SUSPENDED or REJECTED for its seller and the moderators, who alone see it then.
    #[schema(value_type = kinds::Status)]
    pub status: Status,
    /// What its page says over it to those who see it while nobody else does, or nobody yet
    /// (domain::Status::page_note), in the language asked for: that it waits for a moderator,
    /// that it is off the site with its seller's account or for its reports, that the moderators
    /// took it off (to its seller with their reason), that its seller paused it or that it
    /// expired (to its seller with the button that puts it back); none for one on the site.
    pub status_note: Option<String>,
    pub price: ListingPriceDto,
    /// For a car up for sale at a price, what cars of its model and year ask, as its page's bubble
    /// says; none where too few do, or for anything else.
    pub car_price: Option<CarPriceDto>,
    /// For a property on sale at a price of its own with an area, what a square metre of it costs,
    /// against its zone's average where its zone has one.
    pub area_price: Option<AreaPriceDto>,
    /// For something on sale at a price of its own worth borrowing for, what the page's loan
    /// calculator starts from; the years and the interest are the app's to ask.
    pub loan: Option<LoanDto>,
    /// As its seller wrote it, its lines as they are.
    pub description: String,
    /// Its properties, in its category's order, those its page lists (a car's make and model, its
    /// key facts, its options, all of them).
    pub attributes: Vec<AttributeDto>,
    /// Its photos, the cover first.
    pub photos: Vec<ListingPhotoDto>,
    pub place: PlaceDto,
    pub seller: SellerDto,
    pub contact: ContactDto,
    /// Whether the caller is its seller (a private seller's own, or a store's people who post for
    /// it), whose page offers them their buttons (`actions`).
    pub own: bool,
    /// What its seller can do to it, by where it stands, as its page's buttons and "Njoftimet e
    /// mia"'s go by it (domain::Status): any of edit, pause, resume, sell, reserve, unreserve,
    /// renew and delete; none for anyone else.
    pub actions: Vec<Action>,
    /// When it went up, and when it expires if it does.
    #[serde(with = "time::serde::rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    /// How many times its page was seen, every day's views added up, as its page says under its
    /// heart; none before the first.
    pub views: Option<i64>,
}

/// A listing's category (`ListingDto::category`): its slug, which `GET /listings` takes as
/// `category`, its name in Albanian and as an English page says it, what its page is titled by,
/// and those above it, the main one first, as its page's breadcrumb goes up through them.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListingCategoryDto {
    #[schema(examples("makina"))]
    pub slug: String,
    #[schema(examples("Makina"))]
    pub name: String,
    #[schema(examples("Cars"))]
    pub name_en: String,
    /// The categories above it, the main one first (Automjete over Makina); none for a main one.
    pub path: Vec<CategoryNameDto>,
}

/// A listing's price, as its page says it.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListingPriceDto {
    /// The amount, as text, every digit it has; none for a price on request, nor for a free one.
    #[schema(examples("9500"))]
    pub amount: Option<Decimal>,
    /// EUR or ALL.
    #[schema(examples("EUR"))]
    pub currency: String,
    /// How the price is meant: FIXED, NEGOTIABLE, FROM, FREE or ON_REQUEST.
    #[serde(rename = "type")]
    #[schema(value_type = kinds::PriceType)]
    pub kind: PriceType,
    /// What it was, when it dropped lately (by a hundredth or more, within the last 30 days).
    #[schema(examples("10200"))]
    pub was: Option<Decimal>,
    /// Whether the amount is a month's: a rent's, or a job's pay.
    pub per_month: bool,
    /// The drop, as the pill beside its page's price says it.
    pub drop: Option<DropDto>,
    /// What a unit of its currency is worth in euros, for the app to say it in the other currency.
    #[schema(examples("1"))]
    pub eur_rate: Option<Decimal>,
}

/// A price lowered lately (domain::listing::price_drop): from what, to what, and by how many
/// percent, whole, 1 at least ("−7%"). The page doesn't know when; the 30 days before it
/// changed are what `from` is the lowest of.
#[derive(Debug, Serialize, ToSchema)]
pub struct DropDto {
    #[schema(examples("10200"))]
    pub from: Decimal,
    #[schema(examples("9500"))]
    pub to: Decimal,
    #[schema(examples(7))]
    pub percent: i64,
}

/// A car's price against what cars of its model and year ask (two years either side), as its
/// page's bubble says it (domain::listing::car_verdict, pluribus-services' listings::CarPrices).
#[derive(Debug, Serialize, ToSchema)]
pub struct CarPriceDto {
    /// The median, in the listing's currency, rounded as its page rounds it (to the hundred euros,
    /// the ten thousand lekë).
    #[schema(examples("10300"))]
    pub median: Decimal,
    /// How its price compares: "good" 15 % or more under the median, "near" within 15 %, "above"
    /// 15 % or more over it; null under half of it, which says only that the price is likely wrong
    /// (its page then shows no bubble).
    #[schema(value_type = Option<kinds::CarVerdict>)]
    pub verdict: Option<CarVerdict>,
    /// Its price as a share of the median, both in euros, in whole percent, the half rounded up
    /// (domain::listing::car_percent): what the verdict weighs, unrounded; 93 for a price 7 % under
    /// it, 120 for one 20 % over.
    #[schema(examples(93))]
    pub percent: Option<i64>,
    /// What such cars ask, in a sentence, in the language asked for: "Volkswagen Golf 2014–2018 në
    /// shitje: shumica kërkojnë 9 500–11 400 €, mesatarja 10 300 €."
    pub text: String,
}

/// What a square metre of a property on sale costs (domain::listing::area_price), against its
/// zone's average (domain::listing::zone_price).
#[derive(Debug, Serialize, ToSchema)]
pub struct AreaPriceDto {
    /// Its price over its area, in its currency, to the whole euro or lek.
    #[schema(examples("1450"))]
    pub per_m2: Decimal,
    /// What one costs in its zone on average, among the zone's sales in its category, in its
    /// currency, rounded likewise; none outside a zone with an average, or for a starting price.
    #[schema(examples("1600"))]
    pub zone_per_m2: Option<Decimal>,
    /// "below" the average by 200 € or more, "same", "near" within 200 € or "above" it by 200 €
    /// or more; none without an average.
    #[schema(value_type = Option<kinds::ZoneVerdict>)]
    pub verdict: Option<ZoneVerdict>,
}

/// What the page's loan calculator starts from.
#[derive(Debug, Serialize, ToSchema)]
pub struct LoanDto {
    /// The price it borrows on.
    pub amount: Decimal,
    /// The down payment it takes for given, a percentage of the price, as Albania's banks ask for
    /// a used car (domain::listing::LOAN_DOWN_PERCENT); the loan is the rest.
    pub down_percent: u32,
}

/// One of a listing's properties, as data, and as its page words it in the language asked for.
#[derive(Debug, Serialize, ToSchema)]
pub struct AttributeDto {
    /// The property's code: "vehicle_mileage".
    pub code: String,
    /// Its name as the listing's category gives it ("Kilometrazhi"), and as an English page says it
    /// ("Mileage").
    pub label: String,
    pub label_en: String,
    /// Its value as its page shows it, in the language asked for ("148 000 km", "Naftë", "Jo",
    /// "04.10.2026"; pluribus-services' listings::shown): one, or a multi-select's each.
    pub shown: Vec<String>,
    /// Whether it takes several values (a car's options, a flat's amenities), each an option.
    pub multi: bool,
    /// One of the category's key facts, which its cards show too.
    pub key: bool,
    /// A number, as text, every digit it has, with its unit as the database keeps it and as an
    /// English page says it after this number ("muaj", "months"; "month" after a 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<Decimal>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_en: Option<String>,
    /// A box: ticked or not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    /// A date, as ISO 8601 has it: "2026-10-04".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// A text someone typed, as they typed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The options chosen: a choice's one, a multi-select's each, in their order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionDto>,
}

/// An option chosen: its code, as the filters and the form name it, and its label, in Albanian and
/// as an English page says it (the catalogue's, not the database's own label_i18n); in the OpenAPI
/// document `AttributeOptionDto`, reference.rs having an `OptionDto` of its own.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = AttributeOptionDto)]
pub struct OptionDto {
    #[schema(examples("DIESEL"))]
    pub code: String,
    #[schema(examples("Naftë"))]
    pub label: String,
    #[schema(examples("Diesel"))]
    pub label_en: String,
}

/// One of a listing's photos, each copy's address absolute, as the pages name them.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListingPhotoDto {
    /// Its id, by which an edit orders and removes it.
    pub id: Uuid,
    /// Whether it is the cover, the cards' photo: the first.
    pub cover: bool,
    pub copies: CopiesDto,
    /// The whole photo's size in pixels, which every copy has the shape of; none where it isn't
    /// known.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// A tiny blurred copy as a data: URI, to show while one comes; none for a photo kept before
    /// WebP.
    pub placeholder: Option<String>,
}

/// A photo's copies (pluribus-services' media::COPIES), as the pages offer them to a browser.
#[derive(Debug, Serialize, ToSchema)]
pub struct CopiesDto {
    /// The cards' copy, 600 px on its shorter side, as WebP with the query of the rules it is made
    /// by; for a photo kept before WebP, its card-sized JPEG.
    pub card: String,
    /// The small and the mini copies, 400 and 300 px on the shorter side, as WebP; none for a
    /// photo kept before WebP.
    pub small: Option<String>,
    pub mini: Option<String>,
    /// The middle copy, 1 200 px on its longer side; none for a photo no bigger, or kept before.
    pub middle: Option<String>,
    /// The whole photo, 1 600 px on its longer side, as WebP; for a photo kept before, the photo
    /// itself.
    pub full: String,
    /// The whole photo as JPEG, as link previews and browsers without WebP get it.
    pub jpeg: String,
    /// The cards' three as AVIF, lighter where it is shown; none for a photo kept before WebP.
    pub card_avif: Option<String>,
    pub small_avif: Option<String>,
    pub mini_avif: Option<String>,
}

/// Where a listing is.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceDto {
    /// Its town: "Tiranë".
    #[schema(examples("Tiranë"))]
    pub city: Option<String>,
    /// Its zone in the town, for a property that says one: "Blloku".
    #[schema(examples("Blloku"))]
    pub zone: Option<String>,
    /// Where it is on the map, as its page's map shows it, a car's or a property's alone (`deal`):
    /// its own point, rounded to about a hundred metres as its form keeps it, or its town's; the
    /// map draws a circle round it.
    pub point: Option<PointDto>,
    /// How many listings of its category its zone has on the site, those on sale at a price with
    /// an area, which the zone's average comes from (`area_price.zone_per_m2`), as its page's note
    /// counts them ("Mesatarja: 12 njoftime në shitje në Blloku"); none outside a zone, or in one
    /// with too few for an average.
    #[schema(examples(12))]
    pub zone_listings: Option<i64>,
}

/// A point on the map.
#[derive(Debug, Serialize, ToSchema)]
pub struct PointDto {
    #[schema(examples(41.3275))]
    pub lat: f64,
    #[schema(examples(19.8187))]
    pub lng: f64,
}

/// Who sells a listing, as its page's seller card says.
#[derive(Debug, Serialize, ToSchema)]
pub struct SellerDto {
    /// A private seller ("user"), or a store ("business").
    pub kind: SellerKind,
    /// The account that posted the listing (listings.created_by_user_id): a private seller's own,
    /// whose profile is /perdorues/{user_id}, or one of a store's people. Blocking the seller
    /// from the listing's page takes it (`PUT /sellers/{user_id}/blocked`), as the pages' block
    /// does.
    pub user_id: Uuid,
    /// A store's address, /biznes/{slug}; none for a private seller.
    pub slug: Option<String>,
    /// A store's name, or the private seller's.
    pub name: String,
    /// A store's logo, its address absolute; none without one.
    pub logo: Option<String>,
    /// Whether the number or the address the listing shows is one its seller signed in with,
    /// proved theirs by the code ("Numër i verifikuar").
    pub verified: bool,
    /// Since when they have been on the site.
    #[serde(with = "time::serde::rfc3339")]
    pub since: OffsetDateTime,
    /// How many listings they have up.
    pub listings: i64,
    /// How quickly they usually answer, in words, in the language asked for ("brenda 1 ore",
    /// "within an hour"); none until they had three conversations and answered half of them.
    pub answers_within: Option<String>,
    /// Whether they were on the site today, by Albania's clock: a store's, any of its people.
    pub active_today: bool,
    /// When they were last on the site, by Albania's clock, by the days their card counts
    /// (format::Activity): "today" ("Aktiv sot"), within the "week", from yesterday to six days
    /// ago ("Aktiv dje", "Aktiv para 3 ditësh"), within the "month", up to thirty days ago, or
    /// "longer" ago, the last two of which the card leaves unsaid; null for one never seen. A
    /// store's, any of its people.
    #[schema(value_type = Option<kinds::Activity>)]
    pub active: Option<Activity>,
    /// Whether a moderator suspended their account or their store, which took the listing off the
    /// site: its seller and the moderators, who alone see it then, are told so.
    pub suspended: bool,
}

/// How a listing's seller can be reached.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContactDto {
    /// What `POST /listings/{no}/contact` reveals: ["phone"], or ["email"] for a seller who signed
    /// in by e-mail and shows no number; none once the listing is off the site or sold, or where
    /// its seller shows neither.
    pub kinds: Vec<ContactKind>,
    /// Whether the number or the address is here: never, so that a reveal is asked for, and
    /// counted.
    pub revealed: bool,
}

/// A kind of contact: a phone number, or an e-mail address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ContactKind {
    Phone,
    Email,
}

/// What a seller can do to their listing from its page, by where it stands (domain::Status, the
/// rules its routes on the pages and db::ListingAction go by): edit, pause, resume, sell, reserve,
/// unreserve, renew or delete, each its route (`POST /listings/{no}/<action>`, `PUT` and `DELETE
/// /listings/{no}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// Up, or paused: change it.
    Edit,
    /// Up: take it down for a while.
    Pause,
    /// Paused or sold: put it back up.
    Resume,
    /// Up, on sale or for rent: say it is sold, or let out.
    Sell,
    /// Active, on sale or for rent: say someone is taking it.
    Reserve,
    /// Reserved: take the reservation off.
    Unreserve,
    /// Expired, or expiring within a week: keep it up 60 days more.
    Renew,
    /// Delete it for good.
    Delete,
}

/// `POST /listings/{no}/contact`: the seller's number or address, revealed.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContactRevealedDto {
    /// The number, as +355691234567.
    #[schema(examples("+355691234567"))]
    pub phone: Option<String>,
    /// The address, of a seller who shows no number.
    #[schema(examples("ana@example.com"))]
    pub email: Option<String>,
    /// Beside a number WhatsApp can reach (not an Albanian landline), the address that opens a chat
    /// with it, a first message naming the listing typed in the language asked for.
    pub whatsapp_url: Option<String>,
}

/// `GET /listings/{no}`: the listing numbered `no`, as its page shows it (`ListingDto`, `listing_dto`),
/// to whom its page shows; for a number no listing has, not found; a bearer token no good session
/// has, 401.
#[utoipa::path(
    get,
    path = "/listings/{no}",
    tag = "listing",
    operation_id = "getListing",
    summary = "A listing",
    description = "The listing as its page shows it, to anyone while it is on the site, and to its seller and the moderators whatever it is.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    security((), ("bearer" = [])),
    responses(
        (status = 200, description = "The listing", body = ListingDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn show(
    State(state): State<AppState>,
    OptionalApiUser(user): OptionalApiUser,
    headers: HeaderMap,
    Path(no): Path<String>,
) -> Result<Json<ListingDto>, ApiError> {
    let no: i64 = no.parse().map_err(|_| ApiError::NotFound)?;
    Ok(Json(listing_dto(&state, user.as_ref(), &headers, no).await?))
}

/// The listing numbered `no`, as its page shows it to `user`, to whom its page shows
/// (domain::Status::shown_to): anyone while it is on the site (active, reserved or sold); whatever
/// it is, its seller (`own`: a private seller's own, or a store's people who post for it) and a
/// moderator (MODERATORS in the settings); for anyone else, or a number no listing has, not found.
/// `GET /listings/{no}`'s, and what own.rs answers a listing posted or edited with.
pub(crate) async fn listing_dto(state: &AppState, user: Option<&SessionUser>, headers: &HeaderMap, no: i64) -> Result<ListingDto, ApiError> {
    let listing = db::listing_by_public_no(&state.db, no).await?.ok_or(ApiError::NotFound)?;
    let status: Status = kind_of(&listing, &listing.status)?;
    let own = match user {
        Some(user) => owns(&state.db, user.id, &listing).await?,
        None => false,
    };
    let moderator = user.is_some_and(|user| state.config.moderators.include(user));
    if !status.shown_to(own, moderator, false) {
        return Err(ApiError::NotFound);
    }
    let kind: ListingType = kind_of(&listing, &listing.listing_type)?;
    let price_type: PriceType = kind_of(&listing, &listing.price_type)?;
    let condition = listing.condition.as_deref().map(|condition| kind_of::<Condition>(&listing, condition)).transpose()?;
    let (attributes, photos, contact, categories, views, was) = tokio::try_join!(
        db::listing_attributes(&state.db, listing.id, listing.category_id),
        db::listing_photos(&state.db, listing.id),
        db::visible_contact(&state.db, listing.id),
        db::categories(&state.db),
        db::listing_views(&state.db, listing.id),
        db::price_was(&state.db, listing.id),
    )?;
    // A car's page (CARS) or a property's (MAPPED), by its category and those above it.
    let lineage = rules::lineage(&categories, &listing.category_slug);
    let deal = lineage.iter().any(|slug| MAPPED.contains(slug));
    // Its category, and those above it, the main one first (none above one the site has taken
    // off, which the active categories lack).
    let above = categories.by_slug(&listing.category_slug).map(|category| categories.ancestors(category)).unwrap_or_default();
    let category = ListingCategoryDto {
        slug: listing.category_slug.clone(),
        name: listing.category_name.clone(),
        name_en: english(&listing.category_name),
        path: above.into_iter().map(CategoryNameDto::from).collect(),
    };
    let car_price = match lineage.contains(&rules::CARS) {
        true => db::car_price(&state.db, listing.id).await?,
        false => None,
    };
    // Off the site with its seller's account or store, which a moderator suspended, rather than
    // for its reports.
    let seller_suspended = status == Status::Suspended && db::seller_suspended(&state.db, listing.public_no).await?;

    let origin = browse::origin(&state.config, headers);
    let now = OffsetDateTime::now_utc();
    let expires_soon = status.up() && listing.expires_at.is_some_and(|at| format::expires_soon(at, now).is_some());
    let car_price = car_price.and_then(|car| {
        let prices = CarPrices::new(&car.prices, &listing.currency, listing.eur_rate)?;
        Some(CarPriceDto {
            median: prices.median_amount.normalize(),
            verdict: rules::car_verdict(car.price_eur, car.prices.median_eur),
            percent: rules::car_percent(car.price_eur, car.prices.median_eur),
            text: prices.text(),
        })
    });
    let area_price = rules::area_price(kind, price_type, listing.price_amount, listing.area_m2).map(|per_m2| {
        // Its zone's average, where it is in a zone that has one, as its page weighs it.
        let zone = listing
            .zone
            .as_ref()
            .and(listing.zone_listings)
            .and(listing.zone_average_eur)
            .and_then(|average| rules::zone_price(per_m2, price_type, average, listing.eur_rate));
        AreaPriceDto { per_m2, zone_per_m2: zone.map(|zone| zone.average), verdict: zone.map(|zone| zone.verdict) }
    });
    let place = PlaceDto {
        city: city(&listing),
        zone: listing.zone.clone(),
        point: match (listing.latitude, listing.longitude) {
            (Some(lat), Some(lng)) if deal => Some(PointDto { lat, lng }),
            _ => None,
        },
        zone_listings: listing.zone.as_ref().and(listing.zone_listings),
    };
    // When its seller was last here, as their card counts the days.
    let active = listing.seller_seen.map(|seen| Activity::of(seen, now));
    let seller = SellerDto {
        kind: if listing.seller_business_slug.is_some() { SellerKind::Business } else { SellerKind::User },
        user_id: listing.created_by_user_id,
        slug: listing.seller_business_slug.clone(),
        name: listing.seller_name.clone(),
        logo: listing.seller_logo.as_deref().map(|logo| browse::absolute(&origin, logo)),
        verified: listing.contact_verified && contact.is_some(),
        since: listing.seller_since,
        listings: listing.seller_listings,
        answers_within: listing.seller_answers.map(|seconds| format::answers_within(seconds).to_string()),
        active_today: active == Some(Activity::Today),
        active,
        suspended: seller_suspended,
    };
    let contact = ContactDto {
        kinds: contact
            .iter()
            .map(|contact| match contact {
                SellerContact::Phone(_) => ContactKind::Phone,
                SellerContact::Email(_) => ContactKind::Email,
            })
            .collect(),
        revealed: false,
    };
    Ok(ListingDto {
        no: listing.public_no,
        slug: listing.slug.clone(),
        title: listing.title.clone(),
        heading: listing.heading.clone(),
        category,
        deal,
        kind,
        condition,
        status,
        status_note: status.page_note(own, own, seller_suspended, listing.rejection_reason.as_deref()),
        price: ListingPriceDto {
            amount: listing.price_amount.map(|amount| amount.normalize()),
            currency: listing.currency.clone(),
            kind: price_type,
            was: was.map(|was| was.normalize()),
            per_month: kind.monthly(),
            drop: rules::price_drop(was, listing.price_amount).map(|drop| DropDto {
                from: drop.from.normalize(),
                to: drop.to.normalize(),
                percent: drop.percent,
            }),
            eur_rate: listing.eur_rate.map(|rate| rate.normalize()),
        },
        car_price,
        area_price,
        loan: rules::loan_amount(kind, price_type, &listing.currency, listing.price_amount)
            .map(|amount| LoanDto { amount: amount.normalize(), down_percent: LOAN_DOWN_PERCENT }),
        description: listing.description.clone(),
        attributes: attributes.iter().map(AttributeDto::new).collect(),
        photos: photos.iter().enumerate().map(|(at, photo)| ListingPhotoDto::new(photo, at == 0, &origin)).collect(),
        place,
        seller,
        contact,
        own,
        actions: match own {
            true => actions(status, kind, expires_soon),
            false => Vec::new(),
        },
        published_at: listing.published_at,
        expires_at: listing.expires_at,
        views: (views > 0).then_some(views),
    })
}

/// `GET /listings/{no}/related?after=8`: the listings like it, `db::RELATED` at a time from the
/// `after`th (db::related_listings: from other sellers, the most alike first), as its page's
/// frames go on, as cards (`CardDto`); `next` is the next page's `after`, null on the last. Those
/// of the accounts a signed-in caller blocked are left out, as the pages leave them out. Not found
/// for a listing not on the site; an `after` that isn't a place in the list, 422.
#[utoipa::path(
    get,
    path = "/listings/{no}/related",
    tag = "listing",
    operation_id = "listRelatedListings",
    summary = "The listings like one",
    description = "The listings like it, eight at a time, as its page's frames go on, without those of the accounts a signed-in caller blocked.",
    params(
        ("no" = i64, Path, description = "The listing's number", example = 1001),
        ("after" = Option<String>, Query, description = "The last page's `next`: the place in the list the next page starts at", example = "8"),
    ),
    security((), ("bearer" = [])),
    responses(
        (status = 200, description = "A page of the listings like it", body = CardPage),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn related(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    Path(no): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Json<Page<CardDto>>, ApiError> {
    let listing = found(&state, &no).await?;
    if !kind_of::<Status>(&listing, &listing.status)?.on_site() {
        return Err(ApiError::NotFound);
    }
    let from = match Params::parse(query.as_deref()).first("after") {
        None => 0,
        Some(after) => after.parse::<i64>().ok().filter(|from| *from >= 0).ok_or_else(|| browse::unreadable("after"))?,
    };
    let mut cards = db::related_listings(&state.db, listing.id, db::RELATED + 1, from).await?;
    // The one past the page says only that another follows; the next starts after this page's
    // place in the list, whoever's listings the caller doesn't see on it.
    let more = cards.len() as i64 > db::RELATED;
    cards.truncate(db::RELATED as usize);
    cards.retain(|card| !caller.hidden.contains(&card.created_by_user_id));
    let items = browse::cards_of(&cards, &browse::origin(&state.config, &headers))?;
    Ok(Json(Page { items, next: more.then(|| (from + db::RELATED).to_string()) }))
}

/// `POST /listings/{no}/seen`: the listing's page was shown in the app, for "Më të klikuarat" and
/// its views (views.rs): 204. Counted as the pages count it (`POST /listing/{number}/pare`), by
/// the caller's address and browser (pluribus-services' listings::visitor), each once in a while,
/// a bot's not at all; the app asks only for a page it showed, and not for its seller's own. Not
/// found for a listing not on the site.
#[utoipa::path(
    post,
    path = "/listings/{no}/seen",
    tag = "listing",
    operation_id = "markListingSeen",
    summary = "A listing's page was shown",
    description = "Counts a view of the listing's page, as the pages count one: by the caller's address and app, each once in a while, a bot's not at all; for a page the app showed, not its seller's own.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    responses(
        (status = 204, description = "Seen"),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn seen(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Path(no): Path<String>,
) -> Result<StatusCode, ApiError> {
    let listing = found(&state, &no).await?;
    if !kind_of::<Status>(&listing, &listing.status)?.on_site() {
        return Err(ApiError::NotFound);
    }
    let agent = headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();
    if let Some(visitor) = visitor(ip.as_deref(), agent) {
        state.views.record(&state.db, listing.public_no, &visitor).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /listings/{no}/contact`: its seller's number, or the address of a seller who shows no
/// number, revealed and counted in the day's statistics, as the page's button reveals and counts
/// it (db::reveal_contact); beside a number WhatsApp can reach, the address that opens a chat with
/// it about the listing, its first message typed in the language asked for, as the page's button
/// beside the number has it (format::whatsapp_url). Not found for a listing not up (active or
/// reserved), or whose seller shows neither.
#[utoipa::path(
    post,
    path = "/listings/{no}/contact",
    tag = "listing",
    operation_id = "revealContact",
    summary = "Reveal a seller's number",
    description = "Reveals the seller's number, or the address of a seller who shows no number, counted as the page's button counts it, with the address of a WhatsApp chat about the listing beside a number WhatsApp reaches.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    responses(
        (status = 200, description = "The seller's number or address", body = ContactRevealedDto),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn contact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(no): Path<String>,
) -> Result<Json<ContactRevealedDto>, ApiError> {
    let no: i64 = no.parse().map_err(|_| ApiError::NotFound)?;
    let revealed = db::reveal_contact(&state.db, no).await?.ok_or(ApiError::NotFound)?;
    let url = format!("{}{}", browse::origin(&state.config, &headers), format::listing_path(no, &revealed.slug));
    Ok(Json(match revealed.contact {
        SellerContact::Phone(phone) => {
            ContactRevealedDto { whatsapp_url: format::whatsapp_url(&phone, &revealed.title, &url), phone: Some(phone), email: None }
        }
        SellerContact::Email(email) => ContactRevealedDto { phone: None, email: Some(email), whatsapp_url: None },
    }))
}

/// The listing numbered `no`, as an address names it, as its page reads it
/// (db::listing_by_public_no, which reads neither a draft nor one deleted): not found for what
/// isn't a number, or a number no such listing has.
async fn found(state: &AppState, no: &str) -> Result<ListingDetail, ApiError> {
    let no: i64 = no.parse().map_err(|_| ApiError::NotFound)?;
    db::listing_by_public_no(&state.db, no).await?.ok_or(ApiError::NotFound)
}

/// One of the listing's kinds, as its row spells it, which the schema's CHECKs hold to those the
/// domain knows (pluribus-core's kinds.rs): one it didn't would be an error.
fn kind_of<K: FromStr<Err = UnknownKind>>(listing: &ListingDetail, text: &str) -> anyhow::Result<K> {
    text.parse().with_context(|| format!("listing {}: {text:?}", listing.public_no))
}

/// The listing's town: its place's name ("Tiranë"), or a zone's town after the zone ("Blloku,
/// Tiranë", as db::listing_by_public_no names a zone).
fn city(listing: &ListingDetail) -> Option<String> {
    let place = listing.location_name.as_deref()?;
    match &listing.zone {
        Some(zone) => place.strip_prefix(zone.as_str())?.strip_prefix(", ").map(str::to_string),
        None => Some(place.to_string()),
    }
}

/// What its seller can do to a listing in `status`, of `kind`, which expires within a week or
/// not, by domain::Status's rules: the buttons its page and "Njoftimet e mia" show them (own.rs's
/// `GET /me/listings` too).
pub(crate) fn actions(status: Status, kind: ListingType, expires_soon: bool) -> Vec<Action> {
    [
        (Action::Edit, status.editable()),
        (Action::Pause, status.pausable()),
        (Action::Resume, status.resumable()),
        (Action::Sell, status.sellable(kind)),
        (Action::Reserve, status.reservable(kind)),
        (Action::Unreserve, status.unreservable(kind)),
        (Action::Renew, status.renewable(expires_soon)),
        (Action::Delete, status.deletable()),
    ]
    .into_iter()
    .filter_map(|(action, allowed)| allowed.then_some(action))
    .collect()
}

/// What an English page says for one of the database's own words (a property's name, an option's
/// label): the catalogue's English, or the word itself where it has none (as reference.rs's).
fn english(text: &str) -> String {
    i18n::in_english(text).unwrap_or(text).to_string()
}

impl AttributeDto {
    /// `attribute`, its words in the language now and in English.
    fn new(attribute: &AttributeValue) -> Self {
        let number = attribute.value_number.map(|number| number.normalize());
        let unit = attribute.unit.clone().filter(|_| number.is_some());
        Self {
            code: attribute.code.clone(),
            label: attribute.label.clone(),
            label_en: english(&attribute.label),
            shown: shown(attribute),
            multi: attribute.multi,
            key: attribute.key,
            unit_en: unit.as_deref().map(|unit| browse::unit_en(unit, number)),
            number,
            unit,
            flag: attribute.value_bool,
            date: attribute.value_date.and_then(|date| date.format(format_description!("[year]-[month]-[day]")).ok()),
            text: attribute.value_text.clone(),
            options: attribute
                .codes
                .iter()
                .zip(&attribute.labels)
                .map(|(code, label)| OptionDto { code: code.clone(), label: label.clone(), label_en: english(label) })
                .collect(),
        }
    }
}

impl ListingPhotoDto {
    /// `photo`, the cover if `cover`, its copies' addresses absolute from `origin` (`CopiesDto::new`).
    fn new(photo: &Photo, cover: bool, origin: &str) -> Self {
        let side = |side: Option<i32>| side.and_then(|side| u32::try_from(side).ok()).filter(|side| *side > 0);
        Self {
            id: photo.id,
            cover,
            copies: CopiesDto::new(&photo.card_url, photo.webp_url.as_deref(), photo.middle_url.as_deref(), &photo.url, origin),
            width: side(photo.width_px),
            height: side(photo.height_px),
            placeholder: photo.placeholder.clone(),
        }
    }
}

impl CopiesDto {
    /// A photo's copies, each address absolute from `origin`: a WebP photo's (`webp`, its full copy)
    /// as its own and its card's names give them, the small, mini and AVIF ones the card's name
    /// with their own endings and the query of the rules the cards' copies are made by
    /// (pluribus-services' photos::card_copy, as the pages name them); a photo kept before WebP its
    /// card-sized JPEG (`card`) and itself (`jpeg`). A listing's photos' (`ListingPhotoDto`), and
    /// an upload's (own.rs).
    pub(crate) fn new(card: &str, webp: Option<&str>, middle: Option<&str>, jpeg: &str, origin: &str) -> Self {
        let at = |url: &str| browse::absolute(origin, url);
        let copy = |ending: &str| webp.and(card_copy(card, ending)).map(|url| at(&url));
        Self {
            card: copy(CARD_COPY).unwrap_or_else(|| at(card)),
            small: copy(SMALL_COPY),
            mini: copy(MINI_COPY),
            middle: middle.map(at),
            full: at(webp.unwrap_or(jpeg)),
            jpeg: at(jpeg),
            card_avif: copy(CARD_AVIF),
            small_avif: copy(SMALL_AVIF),
            mini_avif: copy(MINI_AVIF),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sellers_buttons_go_by_where_the_listing_stands() {
        use Action::*;
        assert_eq!(actions(Status::Active, ListingType::Sell, false), [Edit, Pause, Sell, Reserve, Delete]);
        assert_eq!(actions(Status::Reserved, ListingType::Rent, true), [Edit, Pause, Sell, Unreserve, Renew, Delete]);
        assert_eq!(actions(Status::Active, ListingType::Service, false), [Edit, Pause, Delete], "nothing to hand over");
        assert_eq!(actions(Status::Paused, ListingType::Sell, false), [Edit, Resume, Delete]);
        assert_eq!(actions(Status::Sold, ListingType::Sell, false), [Resume, Delete]);
        assert_eq!(actions(Status::Expired, ListingType::Sell, false), [Renew, Delete]);
        assert_eq!(actions(Status::PendingReview, ListingType::Sell, false), [Delete]);
        assert_eq!(serde_json::to_value(Unreserve).unwrap(), "unreserve");
    }
}
