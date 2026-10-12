//! A seller's own listings, as the phone app posts and looks after them, by what the pages' posting
//! form goes by (pluribus-web's post.rs, and what C9 moved out of it into pluribus-services:
//! properties.rs, photos.rs, posting.rs), signed in throughout (`ApiUser`): the API takes no listing
//! without an account. Photos are uploaded first (`POST /photos`), each made into its copies and put
//! in the store at once, and kept for a listing to take (db::add_photo_uploads, photo_uploads); a
//! listing is posted (`POST /listings`) or edited (`PUT /listings/{no}`) naming them by their ids,
//! checked by the listing's rules (domain::listing::check, in the form's words, posting::message)
//! and its category's properties' (properties::check, each problem under `properties.<code>`),
//! within the day's limits (posting::may_post), waiting for a moderator where the pages' would
//! (posting::review_for), and saved in one transaction that takes its uploads, or nothing of it
//! (db::create_listing, db::update_listing, `Photos::Uploads`). The seller's listings as "Njoftimet e
//! mia" shows them (`GET /me/listings`); pausing, putting back, selling, reserving, taking the
//! reservation off, renewing and deleting one, the seller's buttons, by domain::Status's rules
//! (db::change_own_listing, db::renew_listing); and the posting form's helpers, the zone a listing's
//! words name (`POST /listings/zone`) and what cars of a model ask (`GET /listings/price-hint`). The
//! shapes they answer in are here, beside their handlers.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str::FromStr,
    sync::{Mutex, OnceLock, PoisonError},
};

use anyhow::Context;
use axum::{
    Json,
    extract::{
        Multipart, Path, RawQuery, State,
        multipart::{MultipartError, MultipartRejection},
        rejection::PathRejection,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pluribus_core::{
    domain::{
        City, Condition, ListingType, PriceType, Status,
        listing::{self as rules, CarVerdict, Checked, FieldError, ListingInput},
    },
    format,
    i18n::t,
};
use pluribus_db::{self as db, Categories, FormAttribute, ListingAction, ListingProperty, MyListing, NotSaved, OnPromotion, Photos, SessionUser};
use pluribus_services::{
    AppState,
    filters::Params,
    media::{self, CARD_COPY, MAX_PHOTOS},
    photos,
    posting::{self, Poster},
    properties::{self, OTHER},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody, browse,
    listing::{Action, CopiesDto, ListingDto, actions, listing_dto},
};

/// At most this many of a user's uploads wait for a listing at once: four listings' photos. More
/// would be photos kept for nothing until they are forgotten (photos::UPLOADS_KEPT).
const WAITING_MOST: i64 = 4 * MAX_PHOTOS as i64;

/// A photo uploaded (`POST /photos`), waiting for a listing to name it: its id, its copies as a
/// listing's photos' are (`ListingDto`'s), its size and its tiny copy.
#[derive(Debug, Serialize)]
pub struct UploadDto {
    /// What a listing names it by (`photo_ids`, `cover`, `order`), and the listing's photo keeps.
    pub id: Uuid,
    pub copies: CopiesDto,
    /// The whole photo's size in pixels, which every copy has the shape of.
    pub width: u32,
    pub height: u32,
    /// A tiny blurred copy as a data: URI, to show while one comes.
    pub placeholder: String,
}

/// `POST /photos`: a listing's photos, before the listing, as multipart, each a file of the field
/// `photos`, MAX_PHOTOS at most, each 15 MB at most as sent and the whole body 64 MB
/// (photos::UPLOAD_BODY_BYTES), JPEG, PNG or WebP: each made into its copies and put in the store,
/// as the pages' form's are, and kept a day for a listing to take (photos::UPLOADS_KEPT): 201, the
/// uploads in their order. What isn't one, or too many or too big, is a validation error on
/// `photos`, in the pages' words; a body that isn't multipart, one on `body`; more than
/// WAITING_MOST waiting, too many requests.
pub async fn upload(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    headers: HeaderMap,
    multipart: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<Vec<UploadDto>>), ApiError> {
    let mut multipart = multipart.map_err(|_| invalid("body", t("Dërgo fotot si multipart/form-data, në fushën „photos“.")))?;
    let mut sent = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(unread)? {
        if field.name() == Some("photos") {
            let data = field.bytes().await.map_err(unread)?;
            if !data.is_empty() {
                sent.push(data);
            }
        }
    }
    if sent.is_empty() {
        return Err(invalid("photos", t("Dërgo të paktën një foto, në fushën „photos“.")));
    }
    if let Some(message) = photos::check(&sent, 0) {
        return Err(invalid("photos", message));
    }
    if db::uploads_waiting(&state.db, user.id).await? + sent.len() as i64 > WAITING_MOST {
        return Err(ApiError::RateLimited.said(t("Ke shumë foto që presin një njoftim. Publiko njoftimin, ose provo përsëri nesër.")));
    }
    let processed = photos::process(sent).await?.map_err(|message| invalid("photos", message))?;
    let stored = photos::store(&state.media, processed).await?;
    let ids = match db::add_photo_uploads(&state.db, user.id, &stored).await {
        Ok(ids) => ids,
        Err(error) => {
            photos::unstore(&state.media, &stored).await;
            return Err(error.into());
        }
    };
    let origin = browse::origin(&state.config, &headers);
    let uploads = ids
        .into_iter()
        .zip(&stored)
        .map(|(id, photo)| UploadDto {
            id,
            copies: CopiesDto::new(&photo.card_url, Some(&photo.url), photo.middle_url.as_deref(), &photo.jpeg_url, &origin),
            width: u32::try_from(photo.width).unwrap_or_default(),
            height: u32::try_from(photo.height).unwrap_or_default(),
            placeholder: photo.placeholder.clone(),
        })
        .collect();
    Ok((StatusCode::CREATED, Json(uploads)))
}

/// A body of photos that couldn't be read whole: too big, or cut short (photos::unread).
fn unread(error: MultipartError) -> ApiError {
    invalid("photos", photos::unread(error.status()))
}

/// A validation error on one field.
fn invalid(field: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::Validation(vec![(field, message.into())])
}

/// What `POST /listings` takes, and `PUT /listings/{no}` with what an edit adds (`ListingEditDto`):
/// a listing's fields, by the posting form's names for them, its properties by their codes, and its
/// photos by their uploads' ids. A field left out is as the form's left empty, and what is wrong
/// with it is said under its name.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListingInputDto {
    /// The category's slug ("makina"), one listings go into, not one that only groups others.
    pub category: String,
    /// What it offers or asks for, one the category takes: SELL, BUY, RENT, WANTED_RENT, SERVICE or
    /// JOB (a category of one kind is of that kind whatever this says).
    pub listing_type: String,
    /// What state the thing is in, where the category asks: NEW, LIKE_NEW, USED or FOR_PARTS.
    pub condition: Option<String>,
    /// 5 to 120 characters, without links.
    pub title: String,
    /// 10 to 5 000 characters, two links at most.
    pub description: String,
    /// FIXED, NEGOTIABLE, FREE or ON_REQUEST; FIXED without one.
    pub price_type: String,
    /// The amount, a number (or its digits as text, "9500.50"), above 0, for a fixed or negotiable
    /// price; kept to the cent.
    pub price: Option<Decimal>,
    /// EUR or ALL; any other is euros.
    pub currency: String,
    /// The town's slug ("tirane"), and for a property a zone of it ("blloku").
    pub city: String,
    pub zone: Option<String>,
    /// For a property or a car, its own point on the map, within 40 km of its town; none for its
    /// town's.
    pub point: Option<PointInput>,
    /// The category's properties (`GET /categories/{slug}/fields?for=form`), by code: a value as
    /// text or a number ("VOLKSWAGEN", 2016, "148 000"), several for a multi-select (["ABS",
    /// "NAVIGATION"]), true for a box ticked, and for a list's "Tjetër" the name typed,
    /// {"other": "Zastava"} (or as the form sends it, "__other__" with the name under
    /// "<code>_other").
    pub properties: BTreeMap<String, Value>,
    /// Its photos, by the ids of the caller's uploads (`POST /photos`), in their order, MAX_PHOTOS at
    /// most with those an edit keeps.
    pub photo_ids: Vec<Uuid>,
    /// The cover, one of `photo_ids` (or, on an edit, of the listing's photos): first, whatever
    /// their order.
    pub cover: Option<Uuid>,
    /// The name buyers see, 60 characters at most; none for the account's.
    pub name: Option<String>,
    /// "Posto si": the store it is posted as, by its slug, one the caller posts for; none for their
    /// own. A new listing's alone, as on the pages.
    pub store: Option<String>,
}

/// What `PUT /listings/{no}` takes: the listing's fields as `POST /listings` does, the photos it
/// takes out, and the order of all of them.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListingEditDto {
    #[serde(flatten)]
    pub listing: ListingInputDto,
    /// The listing's photos taken out, by their ids; one that isn't the listing's counts for nothing.
    pub removed: Vec<Uuid>,
    /// The order of its photos, by their ids, those it keeps and those `photo_ids` adds, the first
    /// the cover; those it leaves out follow, as they were.
    pub order: Vec<Uuid>,
}

/// A point on the map.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct PointInput {
    pub lat: f64,
    pub lng: f64,
}

impl ListingInputDto {
    /// The listing the fields describe, for its rules (domain::listing::check), each choice one of its
    /// kind's values or none, as the pages' form reads its own.
    fn input(&self) -> ListingInput {
        let point = |coordinate: fn(&PointInput) -> f64| self.point.as_ref().map(|point| coordinate(point).to_string()).unwrap_or_default();
        ListingInput {
            category: self.category.clone(),
            listing_type: ListingType::parse(&self.listing_type),
            condition: self.condition.as_deref().and_then(Condition::parse),
            title: self.title.clone(),
            description: self.description.clone(),
            price_type: PriceType::parse(&self.price_type),
            price: self.price,
            currency: self.currency.clone(),
            city: self.city.clone(),
            zone: self.zone.clone().unwrap_or_default(),
            name: self.name.clone().unwrap_or_default(),
            latitude: point(|point| point.lat),
            longitude: point(|point| point.lng),
        }
    }

    /// Its properties as the pages' form sends them, by code, each value as text
    /// (properties::check reads them so): a number as one types it, a box ticked "1", a list's
    /// "Tjetër" with the name typed under `<code>_other`. What is none of these is left out.
    fn property_values(&self) -> HashMap<String, Vec<String>> {
        let mut values: HashMap<String, Vec<String>> = HashMap::new();
        for (code, value) in &self.properties {
            match value {
                Value::Object(other) => {
                    if let Some(Value::String(name)) = other.get("other") {
                        values.entry(code.clone()).or_default().push(OTHER.to_string());
                        values.entry(format!("{code}_other")).or_default().push(name.clone());
                    }
                }
                Value::Array(items) => values.entry(code.clone()).or_default().extend(items.iter().filter_map(typed)),
                value => values.entry(code.clone()).or_default().extend(typed(value)),
            }
        }
        values
    }
}

/// One value as the form's field would hold it: text as it is; a number with a decimal point, a
/// fourth decimal where it has three, which the form's reader of numbers would take for a
/// thousands' mark ("1.625" is written "1.6250": filters::parse_number); a box ticked "1"; none for
/// a box left unticked, or nothing.
fn typed(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => {
            let text = number.to_string();
            let plain = Decimal::from_str(&text).or_else(|_| Decimal::from_scientific(&text)).ok()?.normalize().to_string();
            Some(match plain.split_once('.') {
                Some((_, decimals)) if decimals.len() == 3 => format!("{plain}0"),
                _ => plain,
            })
        }
        Value::Bool(true) => Some("1".to_string()),
        _ => None,
    }
}

/// A property's field in the envelope, `properties.<code>`, which `ApiError::Validation` names as
/// a `&'static str`: each made once and kept. The codes are the database's properties', a few
/// hundred, whatever a request sends: only a property of the category has a problem.
fn property_field(code: &str) -> &'static str {
    static FIELDS: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let name = format!("properties.{code}");
    let mut fields = FIELDS.get_or_init(Default::default).lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(field) = fields.get(name.as_str()) {
        return field;
    }
    let field: &'static str = Box::leak(name.into_boxed_str());
    fields.insert(field);
    field
}

/// Each id once, in its order.
fn distinct(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = Vec::with_capacity(ids.len());
    for id in ids {
        if !seen.contains(id) {
            seen.push(*id);
        }
    }
    seen
}

/// What a listing's fields check out as, by its rules and its category's properties' (`listing`,
/// none while anything is wrong), what to save of its properties, and what is wrong, each field's
/// problems under the request's names for them, in the form's words.
struct Judged<'a> {
    listing: Option<Checked<'a>>,
    properties: Vec<ListingProperty>,
    wrong: Vec<(&'static str, String)>,
}

/// The listing's rules (domain::listing::check) and its category's properties' (properties::check)
/// on `body`'s fields.
fn judged<'a>(input: &'a ListingInput, body: &ListingInputDto, categories: &'a Categories, cities: &'a [City], attributes: &[FormAttribute]) -> Judged<'a> {
    let (listing, found) = rules::check(input, categories, cities);
    let mut wrong: Vec<(&'static str, String)> = found.into_iter().map(|(field, error)| (field, posting::message(error))).collect();
    let (properties, problems) = properties::check(attributes, &body.property_values());
    wrong.extend(problems.into_iter().map(|(code, message)| (property_field(&code), message)));
    Judged { listing, properties, wrong }
}

/// The ids of the photos a listing ends with, the cover first, and those named in `order` before
/// the rest (`shown`: its photos, those it keeps and those added), each once.
fn ordered(shown: &[Uuid], cover: Option<Uuid>, order: &[Uuid]) -> Vec<Uuid> {
    let named = cover.into_iter().chain(order.iter().copied()).filter(|id| shown.contains(id));
    distinct(&named.collect::<Vec<_>>())
}

/// The answer to a listing or an edit not saved: an upload it named that isn't there to take, a
/// validation error on `photo_ids`; the database's failure, an internal one.
fn not_saved(error: NotSaved) -> ApiError {
    match error {
        NotSaved::UploadsGone => invalid("photo_ids", t("Disa nga fotot nuk janë më: ngarkoji përsëri.")),
        NotSaved::Database(error) => error.into(),
    }
}

/// The cover named, where it isn't one of the listing's photos (`shown`).
fn cover_elsewhere(cover: Option<Uuid>, shown: &[Uuid]) -> Option<(&'static str, String)> {
    cover
        .filter(|cover| !shown.contains(cover))
        .map(|_| ("cover", t("Kopertina duhet të jetë një nga fotot e njoftimit.").to_string()))
}

/// `POST /listings`: a listing posted, as the pages' form posts one signed in, with its photos
/// uploaded before (`photo_ids`), as the caller or as a store they post for (`store`): checked as
/// the form checks it, every problem a validation error under its field (its properties' under
/// `properties.<code>`, an upload that isn't the caller's to take under `photo_ids`); a sixth
/// listing in a day too many requests, the same title twice in a day a validation error on
/// `title`, in the pages' words; up at once, or, where the pages' would, waiting for a moderator
/// (a photo another seller's listing has: PENDING_REVIEW, with its `status_note`). 201, the
/// listing as `GET /listings/{no}` gives it.
pub async fn create(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    headers: HeaderMap,
    JsonBody(body): JsonBody<ListingInputDto>,
) -> Result<(StatusCode, Json<ListingDto>), ApiError> {
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let attributes = posting::form_attributes_of(&state.db, &categories, &body.category).await?;
    let input = body.input();
    let Judged { listing, properties, mut wrong } = judged(&input, &body, &categories, &cities, &attributes);
    let ids = distinct(&body.photo_ids);
    if let Some(message) = photos::too_many(ids.len()) {
        wrong.push(("photo_ids", message));
    }
    wrong.extend(cover_elsewhere(body.cover, &ids));
    // "Posto si": a store the caller may post for, or their own.
    let stores = match body.store.as_deref() {
        None | Some("") => Vec::new(),
        Some(_) => posting::stores_posting_for(&state.db, user.id).await?,
    };
    let store = match body.store.as_deref() {
        None | Some("") => None,
        Some(slug) => {
            let store = stores.iter().find(|store| store.slug == slug);
            if store.is_none() {
                wrong.push(("store", t("Zgjidh një nga dyqanet e tua, ose veten.").to_string()));
            }
            store
        }
    };
    let (Some(checked), true) = (listing, wrong.is_empty()) else {
        return Err(ApiError::Validation(wrong));
    };
    // A few a day, and each title once (posting::may_post), as on the pages.
    if let Err(refused) = posting::may_post(&state.db, user.id, &input.title).await? {
        let message = posting::message(refused.error);
        return Err(match refused.error {
            FieldError::TooManyToday => ApiError::RateLimited.said(message),
            _ => invalid(refused.field, message),
        });
    }
    let poster = match store {
        Some(store) => Poster::for_store(&user, store),
        None => Poster::of(&user),
    };
    let slug = format::slug(checked.title);
    // What waits for a moderator first, as on the pages: a photo another seller's listing has, the
    // very same file. Signed in, its contact was proved by a code, so no new account waits.
    let hashes = db::upload_hashes(&state.db, user.id, &ids).await?;
    let review = posting::review_for(&state, &hashes, poster.seller(), false).await?;
    let mut to_save = posting::listing_to_save(&checked, &poster, &slug, &properties, true);
    to_save.review = review;
    let photos = ordered(&ids, body.cover, &ids);
    let (_, no) = db::create_listing(&state.db, &to_save, checked.point, Photos::Uploads { ids: &photos, user_id: user.id })
        .await
        .map_err(not_saved)?;
    Ok((StatusCode::CREATED, Json(listing_dto(&state, Some(&user), &headers, no).await?)))
}

/// `PUT /listings/{no}`: an edit of the caller's listing (a private seller's own, or one of a store
/// they post for), while it is up or paused, as the pages' edit form makes one: its fields checked
/// as a new listing's are (not its day's limits, nor a moderator's review, as on the pages), photos
/// taken out (`removed`), uploads added (`photo_ids`), all of them in `order`, the cover first; the
/// files of those taken out deleted once it has gone through. The listing as `GET /listings/{no}`
/// gives it; not found for a listing that isn't the caller's to edit, or isn't up or paused.
pub async fn update(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    headers: HeaderMap,
    no: Result<Path<i64>, PathRejection>,
    JsonBody(edit): JsonBody<ListingEditDto>,
) -> Result<Json<ListingDto>, ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    let own = db::own_listing(&state.db, no, user.id).await?.ok_or(ApiError::NotFound)?;
    let body = &edit.listing;
    let (categories, cities, photos_now) =
        tokio::try_join!(db::categories(&state.db), db::cities(&state.db), db::own_listing_photos(&state.db, own.id))?;
    let attributes = posting::form_attributes_of(&state.db, &categories, &body.category).await?;
    let input = body.input();
    let Judged { listing, properties, mut wrong } = judged(&input, body, &categories, &cities, &attributes);
    // Only the listing's own photos go, whatever was sent.
    let removed: Vec<Uuid> = distinct(&edit.removed).into_iter().filter(|id| photos_now.iter().any(|photo| photo.id == *id)).collect();
    let added = distinct(&body.photo_ids);
    let kept: Vec<Uuid> = photos_now.iter().map(|photo| photo.id).filter(|id| !removed.contains(id)).collect();
    if let Some(message) = photos::too_many(kept.len() + added.len()) {
        wrong.push(("photo_ids", message));
    }
    let shown: Vec<Uuid> = kept.iter().chain(&added).copied().collect();
    wrong.extend(cover_elsewhere(body.cover, &shown));
    let (Some(checked), true) = (listing, wrong.is_empty()) else {
        return Err(ApiError::Validation(wrong));
    };
    let slug = format::slug(checked.title);
    let changed = posting::listing_to_save(&checked, &Poster::of(&user), &slug, &properties, true);
    let order: Vec<String> = ordered(&shown, body.cover, &edit.order).iter().map(Uuid::to_string).collect();
    let added = Photos::Uploads { ids: &added, user_id: user.id };
    let gone = db::update_listing(&state.db, no, &changed, checked.point, &removed, added, &order)
        .await
        .map_err(not_saved)?
        .ok_or(ApiError::NotFound)?;
    // The files of the photos taken out, now that the listing names them no more.
    media::delete_photos(&state.media, &gone).await;
    Ok(Json(listing_dto(&state, Some(&user), &headers, no).await?))
}

/// One of the caller's listings, as "Njoftimet e mia" shows it (`GET /me/listings`).
#[derive(Debug, Serialize)]
pub struct MyListingDto {
    /// Its number, which its page and these routes go by.
    pub no: i64,
    pub slug: String,
    /// What its page calls it: a car's made from what the car is, another's as its seller wrote it.
    pub title: String,
    /// What it offers or asks for: SELL, BUY, RENT, WANTED_RENT, SERVICE or JOB.
    pub kind: ListingType,
    /// Where it stands: ACTIVE, RESERVED, SOLD, PAUSED, EXPIRED, PENDING_REVIEW (or DRAFT, waiting
    /// for its code), SUSPENDED or REJECTED.
    pub status: Status,
    /// What its row says of it once it is no longer simply up, in the language asked for:
    /// "Rezervuar", "Shitur" ("Dhënë me qira" for a rental), "Skaduar", "Në pritje",
    /// "Çaktivizuar"... (domain::Status::seller_label); none while it is up.
    pub status_label: Option<String>,
    /// What its page says over it to its seller, while nobody else sees it, in the language asked
    /// for: that it waits for a moderator, that its reports took it off the site, why the moderators
    /// took it off, that it is paused or expired, with the button that puts it back
    /// (domain::Status::page_note); none while it is on the site.
    pub status_note: Option<String>,
    /// Its first photo's card copy, its address absolute; none without photos.
    pub photo: Option<String>,
    pub price: OwnPriceDto,
    /// What it drew, all along and over the last seven days: its page seen, its seller's number or
    /// address shown, and kept by how many (those kept in the week).
    pub views: i64,
    pub views_week: i64,
    pub contacts: i64,
    pub contacts_week: i64,
    pub saves: i64,
    pub saves_week: i64,
    /// When it went up, and when it expires if it does.
    #[serde(with = "time::serde::rfc3339::option")]
    pub published_at: Option<OffsetDateTime>,
    #[serde(with = "time::serde::rfc3339::option")]
    pub expires_at: Option<OffsetDateTime>,
    /// Its seller's buttons, by where it stands (domain::Status), as on its page (`ListingDto`):
    /// any of edit, pause, resume, sell, reserve, unreserve, renew and delete, each its route here.
    pub actions: Vec<Action>,
    /// The promotions on it now, the soonest to end first.
    pub promotions: Vec<PromotionDto>,
}

/// A listing's price, as its row on "Njoftimet e mia" says it.
#[derive(Debug, Serialize)]
pub struct OwnPriceDto {
    /// The amount, as text, every digit it has; none for a price on request, nor for a free one.
    pub amount: Option<Decimal>,
    /// EUR or ALL.
    pub currency: String,
    /// How the price is meant: FIXED, NEGOTIABLE, FROM, FREE or ON_REQUEST.
    #[serde(rename = "type")]
    pub kind: PriceType,
    /// Whether the amount is a month's: a rent's, or a job's pay.
    pub per_month: bool,
}

/// A promotion on a listing now.
#[derive(Debug, Serialize)]
pub struct PromotionDto {
    /// Its name, in the language asked for: "I veçuar për 7 ditë".
    pub name: String,
    /// What it does: FEATURED, HIGHLIGHT, BUMP or CATEGORY_TOP.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(with = "time::serde::rfc3339")]
    pub ends_at: OffsetDateTime,
}

impl MyListingDto {
    /// `listing`'s row, with its first photo's card copy (`photo`) and the promotions on it now
    /// among `promotions`, its photo's address absolute from `origin`; its buttons as of `now`.
    fn new(listing: &MyListing, photo: Option<&String>, promotions: &[OnPromotion], origin: &str, now: OffsetDateTime) -> anyhow::Result<Self> {
        let status: Status = listing.status.parse().with_context(|| format!("listing {}: {:?}", listing.public_no, listing.status))?;
        let kind: ListingType = listing.listing_type.parse().with_context(|| format!("listing {}: {:?}", listing.public_no, listing.listing_type))?;
        let price_type: PriceType = listing.price_type.parse().with_context(|| format!("listing {}: {:?}", listing.public_no, listing.price_type))?;
        let expires_soon = status.up() && listing.expires_at.is_some_and(|at| format::expires_soon(at, now).is_some());
        Ok(Self {
            no: listing.public_no,
            slug: listing.slug.clone(),
            title: listing.title.clone(),
            kind,
            status,
            status_label: status.seller_label(kind).map(str::to_string),
            // Its seller's, whose account is up as they are signed in: none of its own suspended.
            status_note: status.page_note(true, true, false, listing.rejection_reason.as_deref()),
            photo: photo.map(|url| browse::absolute(origin, &photos::card_copy(url, CARD_COPY).unwrap_or_else(|| url.clone()))),
            price: OwnPriceDto {
                amount: listing.price_amount.map(|amount| amount.normalize()),
                currency: listing.currency.clone(),
                kind: price_type,
                per_month: kind.monthly(),
            },
            views: listing.stats.views,
            views_week: listing.stats.views_week,
            contacts: listing.stats.contacts,
            contacts_week: listing.stats.contacts_week,
            saves: listing.stats.saves,
            saves_week: listing.stats.saves_week,
            published_at: listing.published_at,
            expires_at: listing.expires_at,
            actions: actions(status, kind, expires_soon),
            promotions: promotions
                .iter()
                .filter(|promotion| promotion.public_no == listing.public_no)
                .map(|promotion| PromotionDto { name: t(&promotion.name).to_string(), kind: promotion.promotion_type.clone(), ends_at: promotion.ends_at })
                .collect(),
        })
    }
}

/// `GET /me/listings`: the caller's own listings, as "Njoftimet e mia" shows them (db::my_listings:
/// the newest first, those deleted left out, those of a store they post for on its own page), each
/// with where it stands, what it drew and its buttons.
pub async fn mine(State(state): State<AppState>, ApiUser(user): ApiUser, headers: HeaderMap) -> Result<Json<Vec<MyListingDto>>, ApiError> {
    let listings = db::my_listings(&state.db, user.id, None).await?;
    let numbers: Vec<i64> = listings.iter().map(|listing| listing.public_no).collect();
    let (photos, promotions) = tokio::try_join!(db::first_photos(&state.db, &numbers), db::listing_promotions(&state.db, &numbers))?;
    let origin = browse::origin(&state.config, &headers);
    let now = OffsetDateTime::now_utc();
    let mine = listings
        .iter()
        .map(|listing| MyListingDto::new(listing, photos.get(&listing.public_no), &promotions, &origin, now))
        .collect::<anyhow::Result<_>>()?;
    Ok(Json(mine))
}

/// `POST /listings/{no}/pause`: the caller takes their listing down for a while (it is up): 204;
/// not found where it isn't theirs, or can't be paused.
pub async fn pause(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Pause).await
}

/// `POST /listings/{no}/resume`: a paused or sold listing back up: 204, or not found.
pub async fn resume(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Resume).await
}

/// `POST /listings/{no}/sell`: sold, or let out for a rental (on sale or for rent, up): its page
/// stays, saying so: 204, or not found.
pub async fn sell(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Sell).await
}

/// `POST /listings/{no}/reserve`: someone is taking it (on sale or for rent, active): 204, or not
/// found.
pub async fn reserve(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Reserve).await
}

/// `POST /listings/{no}/unreserve`: the reservation off, up as before: 204, or not found.
pub async fn unreserve(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Unreserve).await
}

/// `DELETE /listings/{no}`: the listing gone for good: 204, or not found.
pub async fn delete(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    change(&state, &user, no, ListingAction::Delete).await
}

/// `POST /listings/{no}/renew`: up or reserved and expiring, or expired, it stays up 60 days more
/// from now (db::renew_listing), back at the top of "newest first" once a week at most: 204, or not
/// found.
pub async fn renew(State(state): State<AppState>, ApiUser(user): ApiUser, no: Result<Path<i64>, PathRejection>) -> Result<StatusCode, ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    match db::renew_listing(&state.db, no, user.id).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(ApiError::NotFound),
    }
}

/// One of the seller's buttons, by the rules the pages' go by (db::change_own_listing, by
/// domain::Status's): not found where the listing isn't the caller's, or the action doesn't apply
/// to where it stands or to its kind.
async fn change(state: &AppState, user: &SessionUser, no: Result<Path<i64>, PathRejection>, action: ListingAction) -> Result<StatusCode, ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    match db::change_own_listing(&state.db, no, user.id, action).await? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(ApiError::NotFound),
    }
}

/// What `POST /listings/zone` takes: what the seller wrote, and the town chosen, if any.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct ZoneWordsDto {
    pub title: String,
    pub description: String,
    /// The town's slug ("tirane"); none for a zone in a town the words name too.
    pub city: String,
}

/// The zone a listing's words name, and its town, by their slugs; both null for none.
#[derive(Debug, Serialize)]
pub struct ZoneDto {
    pub city: Option<String>,
    pub zone: Option<String>,
}

/// `POST /listings/zone`: the zone a new listing's title or description names, with its town, as
/// the pages' form guesses it while the seller hasn't chosen (posting::zone_named): in the town
/// chosen, or, with none, in a town the words name too ("Blloku, Tiranë"); the title first.
pub async fn zone(State(state): State<AppState>, ApiUser(_): ApiUser, JsonBody(words): JsonBody<ZoneWordsDto>) -> Result<Json<ZoneDto>, ApiError> {
    let cities = db::cities(&state.db).await?;
    let found = posting::zone_named(&cities, &words.title, &words.description, &words.city);
    Ok(Json(ZoneDto { city: found.map(|(city, _)| city.slug.clone()), zone: found.map(|(_, zone)| zone.slug.clone()) }))
}

/// What cars of a model and year ask, for the posting form's hint under a car's price
/// (`GET /listings/price-hint`).
#[derive(Debug, Serialize)]
pub struct PriceHintDto {
    /// What they ask, in a sentence, in the listing's currency and the language asked for:
    /// "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9 500–11 400 €, mesatarja 10 300 €."
    pub text: String,
    /// How the price typed compares, as the car's page will judge it, as `ListingDto`'s
    /// `car_price.verdict`: "good", "near" or "above"; null without a price, or for one so far under
    /// them that it is likely wrong.
    pub verdict: Option<CarVerdict>,
    /// The same in a sentence, where a price was typed: what the car's page will say of it, or, far
    /// under them, that it may be wrong, or in the wrong currency.
    pub said: Option<String>,
}

/// `GET /listings/price-hint?model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=9500`: what cars of
/// the model (its option's code, as the form's fields name it) and year ask, in the currency, and
/// how the price typed compares, as the pages' form says it (posting::price_hint); 204 without a
/// model or a year, or where too few such cars are up to say. The category and the make it may name
/// too change nothing.
pub async fn price_hint(State(state): State<AppState>, ApiUser(_): ApiUser, RawQuery(query): RawQuery) -> Result<Response, ApiError> {
    let params = Params::parse(query.as_deref());
    let text = |name: &str| params.first(name).unwrap_or_default();
    let year = params.first("year").and_then(|year| year.parse().ok());
    Ok(match posting::price_hint(&state.db, text("model"), year, text("currency"), text("price")).await? {
        Some(hint) => Json(PriceHintDto { text: hint.text, verdict: hint.verdict, said: hint.said }).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_propertys_values_are_read_as_the_forms_fields() {
        let body: ListingInputDto = serde_json::from_value(serde_json::json!({
            "properties": {
                "vehicle_year": 2016,
                "engine_litres": 1.625,
                "vehicle_mileage": "148 000",
                "vehicle_features": ["ABS", "NAVIGATION", 7, null],
                "vehicle_make": {"other": "Zastava"},
                "has_warranty": true,
                "is_imported": false,
                "power": 1e3
            }
        }))
        .unwrap();
        let values = body.property_values();
        let value = |code: &str| values.get(code).cloned().unwrap_or_default();
        assert_eq!(value("vehicle_year"), ["2016"]);
        assert_eq!(value("engine_litres"), ["1.6250"], "three decimals aren't a thousands' mark");
        assert_eq!(pluribus_services::filters::parse_number("1.6250"), Some(Decimal::new(1625, 3)));
        assert_eq!(value("vehicle_mileage"), ["148 000"]);
        assert_eq!(value("vehicle_features"), ["ABS", "NAVIGATION", "7"]);
        assert_eq!((value("vehicle_make"), value("vehicle_make_other")), (vec![OTHER.to_string()], vec!["Zastava".to_string()]));
        assert_eq!(value("has_warranty"), ["1"]);
        assert!(value("is_imported").is_empty());
        assert_eq!(value("power"), ["1000"]);
    }

    #[test]
    fn a_property_field_is_named_once_and_kept() {
        let field = property_field("vehicle_year");
        assert_eq!(field, "properties.vehicle_year");
        assert!(std::ptr::eq(field, property_field("vehicle_year")));
    }

    #[test]
    fn the_cover_comes_first_then_the_order_named() {
        let [a, b, c] = [Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3)];
        assert_eq!(ordered(&[a, b, c], Some(c), &[a, b, c]), [c, a, b]);
        assert_eq!(ordered(&[a, b, c], None, &[b, Uuid::from_u128(9), b]), [b], "a photo not the listing's counts for nothing");
        assert_eq!(ordered(&[a, b], Some(Uuid::from_u128(9)), &[]), Vec::<Uuid>::new());
        assert_eq!(distinct(&[a, b, a]), [a, b]);
    }
}
