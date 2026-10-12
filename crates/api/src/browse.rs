//! Browsing, as the phone app does it, by what the pages browse by: the listings a query finds
//! (`GET /listings`), in the pages' own vocabulary through the same parser (pluribus-services'
//! filters.rs), a page at a time, and how many it finds, or those recommended for the seeds a
//! recommended list names; and the front page's rows in one call (`GET /home`): the main
//! categories, the listings seen the most this week, those the caller saw lately, and those
//! recommended for them, or the newest. A listing is a card (`CardDto`), made from the row the
//! pages' cards are made from (`db::ListingCard`): its kinds as the database spells them, its
//! properties as data (`db::CardDetail`), its photo's addresses absolute. A signed-in caller's
//! lists leave out the listings of the accounts they blocked, as the pages leave them out.

use std::str::FromStr;

use anyhow::Context;
use axum::{
    Json,
    extract::{FromRequestParts, RawQuery, State},
    http::{HeaderMap, header, request::Parts},
};
use pluribus_core::{
    domain::{Condition, ListingType, PriceType, Status, UnknownKind, listing},
    i18n::{self, t},
};
use pluribus_db::{self as db, CardDetail, Cursor, ListingCard, ListingFilter, SessionUser, Sort};
use pluribus_services::{
    AppState,
    config::Config,
    filters::{Params, ParsedFilters},
    media::{CARD_COPIES_EDITION, CARD_COPY},
    recommended::{self, Seeds},
};
use rust_decimal::Decimal;
use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{ApiError, OptionalApiUser, Page, auth};

/// How many of the listings the caller saw lately `/home` reads (`seen`), the latest first: as
/// many as the pages' cookie of them keeps (pluribus-web's SEEN_MOST).
const SEEN_MOST: usize = 12;

/// How many of the listings the caller keeps `/home` reads (`kept`): as many as a signed-in user's
/// recommendations grow from (db::seed_listings).
const KEPT_MOST: usize = 12;

/// How many of the most seen this week `/home` gives, as the welcome page's row shows them
/// (pluribus-web's MOST_VIEWED_SHOWN): the rest of db::MOST_VIEWED stand in for those of the
/// accounts the caller blocked.
const MOST_VIEWED_SHOWN: usize = 8;

/// The main categories the welcome page leads with, in this order, every other one after them in
/// theirs (pluribus-web's MAIN_ON_WELCOME): `/home` lists them so.
const MAIN_FIRST: [&str; 4] = ["automjete", "prona", "elektronike", "veshje-dhe-aksesore"];

/// What a recommended list's `next` puts between its seeds and the place it reached: "1001x10~24".
const SEEDS_END: char = '~';

/// A listing as its card shows it, from the row the pages' cards are made from (`db::ListingCard`):
/// what the app draws a card with, and the stand-in its page opens with while the rest comes.
#[derive(Debug, Serialize)]
pub struct CardDto {
    /// Its number, which its page goes by.
    pub no: i64,
    pub slug: String,
    /// What its card says: a car's made from what the car is ("Volkswagen Golf 7, 2016, naftë"),
    /// another's as its seller wrote it.
    pub title: String,
    /// A car's title as its seller wrote it, where `title` was made: what its page heads itself
    /// with; none for the others.
    pub heading: Option<String>,
    /// Whether its page is laid out as a car's or a property's, its photos 4:3 on a phone, which the
    /// stand-in frames them in too.
    pub deal: bool,
    /// What it offers or asks for: SELL, BUY, RENT, WANTED_RENT, SERVICE or JOB.
    pub kind: ListingType,
    /// What state the thing is in, in the categories that ask: NEW, LIKE_NEW, USED or FOR_PARTS.
    pub condition: Option<Condition>,
    /// Where it stands: ACTIVE; among those seen lately, RESERVED or SOLD too, which its card says.
    pub status: Status,
    pub price: PriceDto,
    /// Its cover photo; none for a listing without photos.
    pub photo: Option<PhotoDto>,
    /// Where it is: the zone and the town ("Blloku, Tiranë"), or the town; none without a place.
    pub place: Option<String>,
    /// When it was posted, or last brought up again, by which the newest come first.
    #[serde(with = "time::serde::rfc3339")]
    pub when: OffsetDateTime,
    /// Its category's card properties in their order, as data, with their words in English as the
    /// English pages say them (`DetailDto`), which the app words in its language as the pages do.
    pub details: Vec<DetailDto>,
    /// Whether a promotion features it now: the pages put it at the top of the results, marked.
    pub promoted: bool,
    /// Whether a promotion highlights it now: its card stands out wherever it shows.
    pub highlighted: bool,
    /// A clear bargain: a car well under what cars of its model and year ask ("Çmim i mirë"), its
    /// price not lately dropped, which `price.was` says enough of.
    pub good_price: bool,
    /// Who sells it: a private seller ("user"), or a business ("business").
    pub seller_kind: SellerKind,
    /// How many times its page was seen this week: on `/home`'s most seen alone.
    pub views: Option<i64>,
    /// How far it is from the map's pin, in metres, in a query with one (lat, lng).
    pub distance_m: Option<f64>,
}

/// One of a card's properties, as data (db::CardDetail), with its words as the English pages say
/// them, from the catalogue (i18n/en/), as pluribus-web's CardDetailExt::shown and
/// format::attribute_number word them, and not the database's own English for a choice
/// (label_i18n), which differs from it for many. One of: a number (as text, every digit it has),
/// with its unit; a choice's label; a text someone typed, as they typed it; a box ticked (`flag`),
/// with the property's name. What it hasn't is left out.
#[derive(Debug, PartialEq, Serialize)]
pub struct DetailDto {
    /// The property's code: "vehicle_mileage".
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<Decimal>,
    /// The number's unit as the database keeps it ("km", "muaj"), and as an English page says it
    /// after this number ("km", "months"; "month" after a 1).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_en: Option<String>,
    /// A choice's label ("Naftë"), and as an English page says it ("Diesel"; the label itself where
    /// the catalogue has no English for it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_en: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// A box's: ticked, which a card shows by the property's name (`name`, and `name_en` as an
    /// English page says it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flag: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_en: Option<String>,
}

impl From<&CardDetail> for DetailDto {
    fn from(detail: &CardDetail) -> Self {
        let english = |words: &str| i18n::in_english(words).unwrap_or(words).to_string();
        Self {
            code: detail.code.clone(),
            number: detail.number,
            unit: detail.unit.clone(),
            unit_en: detail.unit.as_deref().map(|unit| unit_en(unit, detail.number)),
            label: detail.label.clone(),
            label_en: detail.label.as_deref().map(english),
            text: detail.text.clone(),
            flag: detail.flag,
            name: detail.name.clone(),
            name_en: detail.name.as_deref().map(english),
        }
    }
}

/// A number's unit as an English page says it after `number` (format::attribute_number): the
/// catalogue's English, singular after a 1 ("month"), the units being kept in the plural ("muaj",
/// "vite"). A card's properties' (`DetailDto`) and a listing's (listing.rs).
pub(crate) fn unit_en(unit: &str, number: Option<Decimal>) -> String {
    let english = i18n::in_english(unit).unwrap_or(unit);
    match number == Some(Decimal::ONE) {
        true => english.strip_suffix('s').filter(|one| one.len() > 1).unwrap_or(english).to_string(),
        false => english.to_string(),
    }
}

/// A card's price.
#[derive(Debug, Serialize)]
pub struct PriceDto {
    /// The amount, as text, every digit it has ("9500", "12.5"); none for a price on request, nor
    /// for a free one without.
    pub amount: Option<Decimal>,
    /// EUR or ALL.
    pub currency: String,
    /// How the price is meant: FIXED, NEGOTIABLE, FROM, FREE or ON_REQUEST.
    #[serde(rename = "type")]
    pub kind: PriceType,
    /// What it was, when it dropped lately (by a hundredth or more, within the last 30 days), as
    /// text; the drop is worked out from the two.
    pub was: Option<Decimal>,
    /// Whether the amount is a month's: a rent's, or a job's pay.
    pub per_month: bool,
}

/// A card's photo, every address absolute.
#[derive(Debug, Serialize)]
pub struct PhotoDto {
    /// The card's copy (600 px on its shorter side for a photo kept as WebP, as `card_webp`), or
    /// for a photo kept before WebP its card-sized JPEG.
    pub card: String,
    /// The card's copy as WebP, for a photo kept as WebP; none for one kept before.
    pub card_webp: Option<String>,
    /// The whole photo, 1600 px on its longer side: for a photo kept as WebP its JPEG copy, as link
    /// previews get it; the photo itself for one kept before.
    pub full: String,
    /// A tiny blurred copy as a data: URI, to show while the card's comes; none for a photo kept
    /// before WebP.
    pub placeholder: Option<String>,
    /// The whole photo's size in pixels, for the card's frame before it comes; none where it isn't
    /// known.
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Who sells a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SellerKind {
    /// A private seller, on their own account.
    User,
    /// A business, which its people post for.
    Business,
}

/// `GET /listings`: a page of the listings a query finds, where the next starts, and how many it
/// finds.
#[derive(Debug, Serialize)]
pub struct ListingsDto {
    pub items: Vec<CardDto>,
    /// Sent back as `after`, with the same query, for the next page; null on the last.
    pub next: Option<String>,
    pub count: CountDto,
}

/// How many listings a query finds, counted up to a limit: past it, `n` is the limit and `more`
/// says there are more ("1 000+").
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct CountDto {
    pub n: i64,
    pub more: bool,
}

/// `GET /home`: the front page's rows, as the welcome page has them.
#[derive(Debug, Serialize)]
pub struct HomeDto {
    /// The main categories, in the welcome page's order.
    pub categories: Vec<MainCategoryDto>,
    /// The listings seen the most this week ("Më të klikuarat"), eight at most, each with its views.
    pub most_viewed: Vec<CardDto>,
    /// The listings the caller saw lately ("Shikuar së fundmi"), of the numbers `seen` named, in
    /// their order, those still on the site.
    pub seen_lately: Vec<CardDto>,
    /// The first page of those recommended for the caller ("Të rekomanduara për ty"), or for a
    /// caller with too few like what they looked at, of the newest ("Njoftimet më të reja"), as
    /// `for_you` says; its `next` goes to `/listings` as `after` for the page after it.
    pub recommended: Page<CardDto>,
    /// Whether `recommended` is those recommended for the caller, rather than the newest.
    pub for_you: bool,
}

/// A main category, as `/home` lists them: its slug, which `/listings` takes as `category`, and its
/// name in Albanian and in English.
#[derive(Debug, Serialize)]
pub struct MainCategoryDto {
    pub slug: String,
    pub name: String,
    pub name_en: String,
}

/// Who calls, for the lists: the signed-in user, if a bearer token signs one in (`OptionalApiUser`,
/// 401 for a token no good session has), and the accounts they blocked, whose listings the lists
/// leave out for them, by the token's session (db::blocked_for_session), as the pages leave them
/// out by their cookie's; none for a caller not signed in.
pub struct Caller {
    pub user: Option<SessionUser>,
    pub hidden: Vec<Uuid>,
}

impl FromRequestParts<AppState> for Caller {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let OptionalApiUser(user) = OptionalApiUser::from_request_parts(parts, state).await?;
        let hidden = match (&user, auth::bearer(&parts.headers)) {
            (Some(_), Some(token)) => db::blocked_for_session(&state.db, &pluribus_services::hash(token)).await?,
            _ => Vec::new(),
        };
        Ok(Self { user, hidden })
    }
}

/// `GET /listings`: a page of the listings a query finds, in the pages' vocabulary, read by the
/// pages' parser (pluribus-services' filters.rs) into the pages' listing query: the words (q)
/// searched as the pages search them (db::text_search), the accounts the caller blocked left out;
/// with how many it finds, up to db::COUNT_LIMIT, or with `count=precise` up to
/// db::PRECISE_COUNT_LIMIT (as a page asks once it shows, its `numri=1`). A value that doesn't
/// exist is ignored, as the pages ignore it, but a category (404) and an `after` that isn't a place
/// in the query's order (422). With `sipas`, or an `after` that a recommended list's `next` gave,
/// the listings recommended for those seeds, then every other one, newest first, as the welcome
/// page goes on (`recommending`), the query's filters aside.
pub async fn listings(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Json<ListingsDto>, ApiError> {
    let params = Params::parse(query.as_deref());
    let origin = origin(&state.config, &headers);
    let limit = match params.first("count") {
        Some("precise") => db::PRECISE_COUNT_LIMIT,
        _ => db::COUNT_LIMIT,
    };
    if let Some(recommending) = recommending(&params)? {
        let viewer = caller.user.as_ref().map(|user| user.id);
        let ranked = state.recommended.ranked(&state.db, &recommending.seeds, viewer, &caller.hidden).await?;
        // Every listing comes once, those recommended first: the count is every listing's.
        let every = ListingFilter { hidden_accounts: caller.hidden.clone(), ..Default::default() };
        let ((cards, next), count) = tokio::try_join!(
            recommended_page(&state, &ranked, &recommending, &caller.hidden),
            db::listing_count_up_to(&state.db, &every, limit),
        )?;
        return Ok(Json(ListingsDto { items: cards_of(&cards, &origin)?, next, count: counted(count, limit) }));
    }
    let slug = params.first("category");
    let words = params.search_words();
    let (categories, cities, attributes, search) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        async {
            match slug {
                Some(slug) => db::filter_attributes(&state.db, slug).await,
                None => Ok(Vec::new()),
            }
        },
        db::text_search(&state.db, words.as_deref()),
    )?;
    let category = match slug {
        Some(slug) => Some(categories.by_slug(slug).ok_or(ApiError::NotFound)?),
        None => None,
    };
    let filters = ParsedFilters::new(&params, category, &categories, &cities, &attributes, None);
    if params.first("after").is_some() && filters.after.is_none() {
        return Err(unreadable("after"));
    }
    let mut filter = filters.listing_filter(search);
    filter.hidden_accounts = caller.hidden;
    let (rows, count) =
        tokio::try_join!(db::listing_page(&state.db, &filter), db::listing_count_up_to(&state.db, &filter, limit))?;
    let (cards, next) = paged(rows, filter.sort);
    Ok(Json(ListingsDto { items: cards_of(&cards, &origin)?, next, count: counted(count, limit) }))
}

/// `GET /home?seen=1005.1003&kept=1002`: the front page's rows in one call, as the welcome page
/// has them (pluribus-web's browse.rs, home): the main categories, the listings seen the most this
/// week, those the caller saw lately (`seen`: their numbers, the latest first, joined by dots, as
/// the pages' cookie keeps them, which the app keeps on the phone), and the first page of those
/// recommended for them, grown from those seen, those kept (`kept`, and a signed-in caller's own)
/// and those a signed-in caller wrote to a seller about; with too few like them, the newest. The
/// accounts the caller blocked left out of each.
pub async fn home(
    State(state): State<AppState>,
    caller: Caller,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Result<Json<HomeDto>, ApiError> {
    let params = Params::parse(query.as_deref());
    let seen = numbers(params.first("seen"), SEEN_MOST);
    let kept = numbers(params.first("kept"), KEPT_MOST);
    let viewer = caller.user.as_ref().map(|user| user.id);
    let (categories, mut most_viewed, mut seen_lately, (recommended, next, for_you)) = tokio::try_join!(
        db::categories(&state.db),
        state.views.most_viewed(&state.db, None),
        async {
            match seen.is_empty() {
                true => Ok(Vec::new()),
                false => db::cards_by_numbers(&state.db, &seen).await,
            }
        },
        welcome(&state, &seen, &kept, viewer, &caller.hidden),
    )?;
    without_hidden(&mut most_viewed, &caller.hidden);
    most_viewed.truncate(MOST_VIEWED_SHOWN);
    without_hidden(&mut seen_lately, &caller.hidden);
    let main = MAIN_FIRST.iter().filter_map(|slug| categories.by_slug(slug));
    let others = categories.top_level().filter(|category| !MAIN_FIRST.contains(&category.slug.as_str()));
    let categories = main
        .chain(others)
        .map(|category| MainCategoryDto {
            slug: category.slug.clone(),
            name: category.name.clone(),
            name_en: i18n::in_english(&category.name).unwrap_or(category.name.as_str()).to_string(),
        })
        .collect();
    let origin = origin(&state.config, &headers);
    Ok(Json(HomeDto {
        categories,
        most_viewed: cards_of(&most_viewed, &origin)?,
        seen_lately: cards_of(&seen_lately, &origin)?,
        recommended: Page { items: cards_of(&recommended, &origin)?, next },
        for_you,
    }))
}

/// `/home`'s recommended row, as the welcome page's (pluribus-web's browse.rs, welcome_listings):
/// the first page of those recommended for the seeds of what the caller saw lately (`seen`) and
/// keeps (`kept`, and signed in, those their account keeps), and signed in, of those they wrote to
/// a seller about (db::seed_listings), and true; or, with fewer than recommended::FEWEST like them,
/// the first page of the newest, and false.
async fn welcome(
    state: &AppState,
    seen: &[i64],
    kept: &[i64],
    viewer: Option<Uuid>,
    hidden: &[Uuid],
) -> sqlx::Result<(Vec<ListingCard>, Option<String>, bool)> {
    let (kept, written) = match viewer {
        Some(user) => {
            let (mut own, written) = db::seed_listings(&state.db, user).await?;
            for number in kept {
                if !own.contains(number) {
                    own.push(*number);
                }
            }
            (own, written)
        }
        None => (kept.to_vec(), Vec::new()),
    };
    let seeds = Seeds::from_signals(seen, &kept, &written);
    let ranked = state.recommended.ranked(&state.db, &seeds, viewer, hidden).await?;
    if ranked.len() < recommended::FEWEST {
        let filter = ListingFilter { hidden_accounts: hidden.to_vec(), ..Default::default() };
        let (cards, next) = paged(db::listing_page(&state.db, &filter).await?, Sort::Newest);
        return Ok((cards, next, false));
    }
    let first = Recommending { seeds, from: 0, after: None };
    let (cards, next) = recommended_page(state, &ranked, &first, hidden).await?;
    Ok((cards, next, true))
}

/// Where a recommended list goes on from: its seeds, and the place it reached among the listings
/// ranked for them (`from`, the welcome page's `nga`), or past them among the newest (`after`).
struct Recommending {
    seeds: Seeds,
    from: usize,
    after: Option<Cursor>,
}

/// The recommended list a query asks for, if it asks for one: the seeds and the place reached that
/// an `after` names, as a recommended page's `next` gave it ("1001x10~24", or the seeds and a
/// newest first cursor past the listings ranked for them); or else the seeds `sipas` names
/// ("1001x10.1005x9", as `Seeds::param` writes them), from the start, or from the place a bare
/// `after` names. None for a query with neither; 422 for seeds or a place that don't read.
fn recommending(params: &Params) -> Result<Option<Recommending>, ApiError> {
    let after = params.first("after");
    let (seeds, place) = match after.and_then(|after| after.split_once(SEEDS_END)) {
        Some((seeds, place)) => (Seeds::parse(seeds).ok_or_else(|| unreadable("after"))?, Some(place)),
        None => match params.first("sipas") {
            Some(sipas) => (Seeds::parse(sipas).ok_or_else(|| unreadable("sipas"))?, after),
            None => return Ok(None),
        },
    };
    let (from, after) = match place {
        None => (0, None),
        Some(place) => match place.parse::<usize>() {
            Ok(from) => (from, None),
            Err(_) => (0, Some(Cursor::decode(Sort::Newest, place).ok_or_else(|| unreadable("after"))?)),
        },
    };
    Ok(Some(Recommending { seeds, from, after }))
}

/// A page of a recommended list, as the welcome page pages one (pluribus-web's browse.rs,
/// recommended_page): PAGE_SIZE of `ranked` from the `from`th; past the last of them, the newest,
/// those recommended left out, after `after`. Its `next` names the seeds and the place reached, so
/// that the list goes on in the same order whatever the caller saw meanwhile.
async fn recommended_page(
    state: &AppState,
    ranked: &[Uuid],
    at: &Recommending,
    hidden: &[Uuid],
) -> sqlx::Result<(Vec<ListingCard>, Option<String>)> {
    let sipas = at.seeds.param();
    if at.after.is_none() && at.from < ranked.len() {
        let to = ranked.len().min(at.from + db::PAGE_SIZE as usize);
        let cards = db::cards_by_ids(&state.db, &ranked[at.from..to]).await?;
        return Ok((cards, Some(format!("{sipas}{SEEDS_END}{to}"))));
    }
    let filter =
        ListingFilter { after: at.after, hidden_accounts: hidden.to_vec(), exclude: ranked.to_vec(), ..Default::default() };
    let (cards, next) = paged(db::listing_page(&state.db, &filter).await?, Sort::Newest);
    Ok((cards, next.map(|cursor| format!("{sipas}{SEEDS_END}{cursor}"))))
}

/// A page of `rows`, which hold up to PAGE_SIZE + 1 listings, the one past the page saying only
/// that another follows; and where the next starts, the cursor after its last listing in `sort`
/// (db::Cursor), when one does.
pub(crate) fn paged(mut rows: Vec<ListingCard>, sort: Sort) -> (Vec<ListingCard>, Option<String>) {
    let more = rows.len() > db::PAGE_SIZE as usize;
    rows.truncate(db::PAGE_SIZE as usize);
    let next = rows.last().filter(|_| more).map(|last| Cursor::after(sort, last).encode());
    (rows, next)
}

/// A count as the lists give it: up to `limit`, and whether there are more (db's counts stop at
/// one past their limit).
fn counted(count: i64, limit: i64) -> CountDto {
    CountDto { n: count.min(limit), more: count > limit }
}

/// 422 on a list's paging that doesn't read: its `after`, or the seeds of a recommended one
/// (`sipas`).
pub(crate) fn unreadable(field: &'static str) -> ApiError {
    let message = match field {
        "sipas" => t("Njoftimet nga të cilat vijnë rekomandimet nuk lexohen. Fillo përsëri nga faqja e parë."),
        _ => t("Vazhdimi i listës nuk lexohet. Fillo përsëri nga faqja e parë."),
    };
    ApiError::Validation(vec![(field, message.to_string())])
}

/// Listing numbers as the pages' cookie of those seen lately keeps them, joined by dots
/// ("1005.1003"): each once, in their order, `most` at most; what isn't a number left out.
fn numbers(text: Option<&str>, most: usize) -> Vec<i64> {
    let mut numbers = Vec::new();
    for number in text.unwrap_or_default().split('.').filter_map(|part| part.parse::<i64>().ok()) {
        if number > 0 && !numbers.contains(&number) {
            numbers.push(number);
        }
        if numbers.len() == most {
            break;
        }
    }
    numbers
}

/// Leaves out of a row of cards those of the accounts the caller blocked (as the pages' safety.rs,
/// without_hidden).
fn without_hidden(cards: &mut Vec<ListingCard>, hidden: &[Uuid]) {
    if !hidden.is_empty() {
        cards.retain(|card| !hidden.contains(&card.created_by_user_id));
    }
}

/// The cards of `cards`, their photos' addresses absolute from `origin`.
pub(crate) fn cards_of(cards: &[ListingCard], origin: &str) -> Result<Vec<CardDto>, ApiError> {
    cards.iter().map(|card| CardDto::new(card, origin)).collect::<anyhow::Result<_>>().map_err(ApiError::from)
}

impl CardDto {
    /// `card`'s card, its photo's addresses absolute from `origin`, the site's address. Its kinds are
    /// the database's, which its CHECKs hold to those the domain knows (pluribus-core's kinds.rs):
    /// one it didn't would be an error.
    pub fn new(card: &ListingCard, origin: &str) -> anyhow::Result<Self> {
        let kind: ListingType = kind_of(card, &card.listing_type)?;
        Ok(Self {
            no: card.public_no,
            slug: card.slug.clone(),
            title: card.title.clone(),
            heading: card.heading.clone(),
            deal: card.deal,
            kind,
            condition: card.condition.as_deref().map(|condition| kind_of(card, condition)).transpose()?,
            status: kind_of(card, &card.status)?,
            price: PriceDto {
                amount: card.price_amount.map(|amount| amount.normalize()),
                currency: card.currency.clone(),
                kind: kind_of(card, &card.price_type)?,
                was: card.price_was.map(|was| was.normalize()),
                per_month: kind.monthly(),
            },
            photo: photo(card, origin),
            place: place(card),
            when: card.sort_at,
            details: card.details.iter().map(DetailDto::from).collect(),
            promoted: card.featured,
            highlighted: card.highlighted,
            // As the cards mark one (domain::listing::bargain): not one whose price dropped lately,
            // whose drop says enough.
            good_price: listing::bargain(
                card.price_normalized,
                card.model_median,
                listing::price_drop(card.price_was, card.price_amount).is_some(),
            ),
            seller_kind: if card.seller_business { SellerKind::Business } else { SellerKind::User },
            views: card.views,
            distance_m: card.distance_m,
        })
    }
}

/// One of `card`'s kinds, as its row spells it.
fn kind_of<K: FromStr<Err = UnknownKind>>(card: &ListingCard, text: &str) -> anyhow::Result<K> {
    text.parse().with_context(|| format!("listing {}: {text:?}", card.public_no))
}

/// A card's cover photo, its addresses absolute; none without photos.
fn photo(card: &ListingCard, origin: &str) -> Option<PhotoDto> {
    let (cover, full) = (card.photos.first()?, card.photos_full.first()?);
    let webp = card.photos_webp.first().and_then(Option::as_ref);
    let side = |side: Option<i32>| side.and_then(|side| u32::try_from(side).ok()).filter(|side| *side > 0);
    Some(PhotoDto {
        card: absolute(origin, &named(cover)),
        card_webp: webp.map(|webp| absolute(origin, &named(&webp.card))),
        full: absolute(origin, full),
        placeholder: webp.and_then(|webp| webp.placeholder.clone()),
        width: webp.and_then(|webp| side(webp.width)),
        height: webp.and_then(|webp| side(webp.height)),
    })
}

/// A photo's address as the pages name it: a card's copy with the query of the rules the cards'
/// copies are made by (media::CARD_COPIES_EDITION, as pluribus-web's photos.rs names them), since
/// they were made again under their old names, so that no copy kept from before is taken for one;
/// any other as it is.
fn named(url: &str) -> String {
    match url.ends_with(CARD_COPY) {
        true => format!("{url}?{CARD_COPIES_EDITION}"),
        false => url.to_string(),
    }
}

/// Where the listing is, as the list's cards say it under the price (pluribus-web's
/// ListingCardExt::place_text): the zone and the town ("Blloku, Tiranë"), the town alone (the
/// listing's municipality, or else its location), or the zone alone.
fn place(card: &ListingCard) -> Option<String> {
    let city = card.city.as_deref().or(card.location_name.as_deref());
    match (&card.zone, city) {
        (Some(zone), Some(city)) => Some(format!("{zone}, {city}")),
        (Some(zone), None) => Some(zone.clone()),
        (None, city) => city.map(str::to_string),
    }
}

/// The site's address, which a photo's address on the site itself (/media/...) is made absolute
/// with: PUBLIC_URL in the settings of the state answering the request (`config`), or as the
/// request came, the host it named (Host; not X-Forwarded-Host, which a caller may set to
/// anything), over https when the proxy in front says so (X-Forwarded-Proto), as the pages make
/// their own addresses absolute (pluribus-web's seo::origin). A listing's page's photos and
/// address (listing.rs) are made absolute with it too.
pub(crate) fn origin(config: &Config, headers: &HeaderMap) -> String {
    if let Some(url) = config.public_url.clone() {
        return url;
    }
    let host = headers.get(header::HOST).and_then(|value| value.to_str().ok()).unwrap_or("127.0.0.1:3000");
    let https = headers.get("x-forwarded-proto").is_some_and(|proto| proto == "https");
    format!("{}://{host}", if https { "https" } else { "http" })
}

/// `url` as an absolute address: a path on this site, or one absolute already (a photo on a CDN,
/// MEDIA_URL's).
pub(crate) fn absolute(origin: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{origin}{url}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_details_english_is_the_catalogues_as_the_english_pages_say_it() {
        let detail = |row: serde_json::Value| DetailDto::from(&serde_json::from_value::<CardDetail>(row).unwrap());
        // The catalogue's, not the database's own (label_i18n), which says less.
        let fuel = detail(serde_json::json!({"code": "vehicle_fuel", "label": "Naftë", "label_en": "Diesel oil"}));
        assert_eq!((fuel.label.as_deref(), fuel.label_en.as_deref()), (Some("Naftë"), Some("Diesel")));
        let wireless = detail(serde_json::json!({"code": "audio_wireless", "flag": true, "name": "Me valë (Bluetooth)"}));
        assert_eq!((wireless.flag, wireless.name_en.as_deref()), (Some(true), Some("Wireless (Bluetooth)")));
        // A number with its unit as it comes, the unit's English singular after a 1.
        let km = detail(serde_json::json!({"code": "vehicle_mileage", "number": "148000.000000", "unit": "km"}));
        assert_eq!((km.number, km.unit_en.as_deref()), (Some(Decimal::from(148_000)), Some("km")));
        let month = detail(serde_json::json!({"code": "warranty_months", "number": "1.000000", "unit": "muaj"}));
        let months = detail(serde_json::json!({"code": "warranty_months", "number": "6", "unit": "muaj"}));
        assert_eq!((month.unit_en.as_deref(), months.unit_en.as_deref()), (Some("month"), Some("months")));
        // A label the catalogue lacks, as an English page shows it: as it is; a text as it was typed.
        let typed = detail(serde_json::json!({"code": "fashion_brand", "label": "Zara Kids", "text": "e kuqe"}));
        assert_eq!((typed.label_en.as_deref(), typed.text.as_deref()), (Some("Zara Kids"), Some("e kuqe")));
        assert_eq!(
            serde_json::to_value(&km).unwrap(),
            serde_json::json!({"code": "vehicle_mileage", "number": "148000", "unit": "km", "unit_en": "km"})
        );
    }

    #[test]
    fn paging_and_numbers_read_as_the_pages_do() {
        assert_eq!(numbers(Some("1005.1003.x.1005.-4.0.1009"), 12), [1005, 1003, 1009]);
        assert_eq!(numbers(Some("1.2.3.4"), 3), [1, 2, 3]);
        assert_eq!(numbers(None, 12), Vec::<i64>::new());
        assert_eq!(counted(1_001, 1_000), CountDto { n: 1_000, more: true });
        assert_eq!(counted(12, 1_000), CountDto { n: 12, more: false });
        assert_eq!(named("/media/photos/ab-card.webp"), format!("/media/photos/ab-card.webp?{CARD_COPIES_EDITION}"));
        assert_eq!(named("/media/photos/ab.jpg"), "/media/photos/ab.jpg");
        assert_eq!(absolute("https://pluribus.test", "/media/x.jpg"), "https://pluribus.test/media/x.jpg");
        assert_eq!(absolute("https://pluribus.test", "https://cdn.test/x.jpg"), "https://cdn.test/x.jpg");
    }

    #[test]
    fn a_recommended_list_reads_its_seeds_and_place_from_after_or_sipas() {
        let read = |query: &str| recommending(&Params::parse(Some(query))).map(|at| at.map(|at| (at.seeds.param(), at.from, at.after)));
        assert!(matches!(read("category=makina"), Ok(None)));
        assert_eq!(read("after=1001x10~24").unwrap(), Some(("1001x10".to_string(), 24, None)));
        assert_eq!(read("sipas=1005x20.1001x10").unwrap(), Some(("1005x20.1001x10".to_string(), 0, None)));
        assert_eq!(read("sipas=1001x10&after=6").unwrap(), Some(("1001x10".to_string(), 6, None)));
        let id = Uuid::from_u128(7);
        let cursor = Cursor::Newest { sort_at: OffsetDateTime::from_unix_timestamp(1_790_000_000).unwrap(), id };
        assert_eq!(read(&format!("after=1001x10~{}", cursor.encode())).unwrap(), Some(("1001x10".to_string(), 0, Some(cursor))));
        for (query, field) in [("after=x~24", "after"), ("after=1001x10~soon", "after"), ("sipas=1001", "sipas"), ("sipas=1001x10&after=soon", "after")] {
            match read(query) {
                Err(ApiError::Validation(fields)) => assert_eq!(fields[0].0, field, "{query}"),
                other => panic!("{query}: {:?}", other.map(|_| ())),
            }
        }
    }
}
