//! A listing's own rules, apart from the form that sends it and the database that keeps it: what a
//! new listing or an edit must say to go up (`check`: a category listings go into, a kind of
//! listing it takes, a title and a description of the right length with few links, a price, a
//! town and, for a property, a zone of it, the name buyers see, and a property's or a car's own
//! point on the map, near its town), the price rules the posting form and the AI's fill share
//! (`price_of`), and how many listings an account may post in a day (`daily_limit`). What is wrong
//! comes back as codes by field (`FieldErrors`), which web/post.rs says in the page's language.
//! Reading what people typed stays there too (a number as they type it, the form's fields), and so
//! does the check that a person sent the form, which the sign-in form shares.
//!
//! And what a listing's page works out from its price, which the pages (web/listings.rs,
//! web/car_prices.rs) and the JSON API (api/listing.rs) share, moved out of web/listings.rs in step
//! C5: a loan for it (`loan_amount`, LOAN_DOWN_PERCENT), by how much its price dropped
//! (`price_drop`), a car's price against what cars of its model and year ask (`car_verdict`, and a
//! clear bargain, `bargain`), a property's square metre (`area_price`) against its zone's average
//! (`zone_price`, ZONE_MARGIN_EUR); and how its page is laid out, by its category and those above
//! it (`lineage`: a car's, CARS, or a property's, MAPPED).

use std::fmt;

use rust_decimal::{Decimal, RoundingStrategy, prelude::ToPrimitive};
use serde::Serialize;

use super::{Categories, Category, City, Condition, ListingType, PriceType, Zone};
use crate::i18n::t;

/// At most this many listings a day from one account (`daily_limit`).
pub const POSTS_PER_DAY: i64 = 5;
/// The shortest and the longest title a listing takes, in characters.
pub const MIN_TITLE_CHARS: usize = 5;
pub const MAX_TITLE_CHARS: usize = 120;
/// The shortest and the longest description.
pub const MIN_DESCRIPTION_CHARS: usize = 10;
pub const MAX_DESCRIPTION_CHARS: usize = 5000;
/// Links in a description; spam is mostly links. A title takes none.
pub const MAX_LINKS: usize = 2;
/// The longest name buyers see on a listing, in characters.
pub const MAX_NAME_CHARS: usize = 60;
/// How far from its town's point a listing's own may be, in kilometres: a municipality's villages
/// are well within it, another town isn't.
pub const POINT_NEAR_KM: f64 = 40.0;

/// The kinds of price a seller picks from, in the form's order. FROM ("nga 925 €"), which a listing
/// can have, isn't one of them: `price_of` makes it a fixed price.
pub const PRICE_TYPES: [PriceType; 4] = [PriceType::Fixed, PriceType::Negotiable, PriceType::Free, PriceType::OnRequest];

/// What listings.price_amount holds (numeric(14,2)): an amount under a trillion, to the cent.
const PRICE_LIMIT: i64 = 1_000_000_000_000;

/// The category whose listings can say their zone in the town (Blloku, Tiranë), and its
/// subcategories': property.
const ZONED: &str = "prona";

/// The cars' category (Makina): a car's page has its key facts under its title, and its price
/// against what cars of its model and year ask (web/listings.rs, api/listing.rs).
pub const CARS: &str = "makina";

/// The categories whose listings' pages show where they are on a map, and whose form can put them
/// there more exactly than their town's point: property and cars, and those under them. Their
/// pages are laid out as a car's (web/listings.rs; `deal` in the API's answers), by this list
/// since C5, where web/listings.rs kept a copy of it.
pub const MAPPED: [&str; 2] = [ZONED, CARS];

/// The down payment a listing's loan calculator takes for given, as a percentage of the price
/// (loan_calculator.html, loan_controller.js; the API's `loan`): what Albania's banks ask for a
/// used car, which they finance 60 to 65% of (Intesa Sanpaolo 65%, Tirana Bank 60%, in 2026; a
/// new car 70 to 75%, a lease 80%). The loan is the rest of the price.
pub const LOAN_DOWN_PERCENT: u32 = 35;

/// How far a property's price per square metre may be from its zone's average, in euros, and still
/// count as about it (yellow): further under it, it is cheap for the zone (green), further over it,
/// dear (red).
pub const ZONE_MARGIN_EUR: i64 = 200;

/// A listing as a form sends it, its fields read into the domain's terms but not judged yet: what
/// `check` judges. The texts are as typed; a choice that isn't one of its kind's values is none.
#[derive(Debug, Clone, Default)]
pub struct ListingInput {
    /// The category's slug ("makina").
    pub category: String,
    pub listing_type: Option<ListingType>,
    pub condition: Option<Condition>,
    pub title: String,
    pub description: String,
    /// The kind of price chosen; `price_of` makes none, or one the form doesn't offer, a fixed one.
    pub price_type: Option<PriceType>,
    /// The price typed, read as a number where it is one: web/filters.rs's `parse_number` reads it
    /// as people type numbers, in Albanian's way or English's.
    pub price: Option<Decimal>,
    /// The currency's code, as sent ("EUR", "ALL").
    pub currency: String,
    /// The town's slug, and a property's zone's in it, empty for none.
    pub city: String,
    pub zone: String,
    /// The name buyers see, as typed.
    pub name: String,
    /// A property's or a car's point on the map, as the form's map sent it; empty for its town's.
    pub latitude: String,
    pub longitude: String,
}

/// A listing as its form describes it, once every field checks out.
#[derive(Debug)]
pub struct Checked<'a> {
    pub category: &'a Category,
    pub listing_type: ListingType,
    /// For a category that asks for one.
    pub condition: Option<Condition>,
    /// Trimmed, as is the name.
    pub title: &'a str,
    pub description: &'a str,
    pub price: Price,
    pub city: &'a City,
    /// A property's zone in its town, if it says one.
    pub zone: Option<&'a Zone>,
    pub name: &'a str,
    /// A property's or a car's own point on the map, rounded to about a hundred metres; none for
    /// its town's.
    pub point: Option<(f64, f64)>,
}

/// A listing's price, as the rules leave it (`price_of`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price {
    pub price_type: PriceType,
    /// The amount, to the cent, for a fixed or negotiable price; none for the others.
    pub amount: Option<Decimal>,
    /// "EUR" or "ALL".
    pub currency: &'static str,
}

/// What is wrong with one of a listing's fields, as a code, with what was found where that says
/// more. web/post.rs says each in the page's language; Display says it in English, for logs and
/// tests.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FieldError {
    /// No category, or none of the site's.
    NoCategory,
    /// A category that only groups others ("Automjete"): listings go into one of those.
    GroupCategory,
    /// No kind of listing, or one the category doesn't take.
    NoListingType,
    /// A title, trimmed, shorter than MIN_TITLE_CHARS or longer than MAX_TITLE_CHARS.
    TitleLength { chars: usize },
    /// Links in the title.
    TitleLinks { links: usize },
    /// A description shorter than MIN_DESCRIPTION_CHARS or longer than MAX_DESCRIPTION_CHARS.
    DescriptionLength { chars: usize },
    /// More links in the description than MAX_LINKS.
    DescriptionLinks { links: usize },
    /// A fixed or negotiable price without an amount above 0 that the price column holds.
    NoPrice,
    /// No town, or none of the site's.
    NoCity,
    /// A zone that isn't one of the town's.
    ZoneElsewhere,
    /// A name for buyers longer than MAX_NAME_CHARS.
    NameLength { chars: usize },
    /// A point on the map farther from the town than POINT_NEAR_KM.
    PointFar { km: f64 },
    /// A point on the map that isn't two numbers.
    PointUnread,
    /// POSTS_PER_DAY listings from the account in the last day already.
    TooManyToday,
    /// A listing with this title from the account in the last day already.
    SameTitleToday,
}

impl fmt::Display for FieldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCategory => f.write_str("no category listings go into"),
            Self::GroupCategory => f.write_str("a category that only groups others"),
            Self::NoListingType => f.write_str("no kind of listing the category takes"),
            Self::TitleLength { chars } => write!(f, "a title of {chars} characters, not {MIN_TITLE_CHARS} to {MAX_TITLE_CHARS}"),
            Self::TitleLinks { links } => write!(f, "{links} links in the title, which takes none"),
            Self::DescriptionLength { chars } => {
                write!(f, "a description of {chars} characters, not {MIN_DESCRIPTION_CHARS} to {MAX_DESCRIPTION_CHARS}")
            }
            Self::DescriptionLinks { links } => write!(f, "{links} links in the description, more than {MAX_LINKS}"),
            Self::NoPrice => f.write_str("a fixed or negotiable price without an amount"),
            Self::NoCity => f.write_str("no town"),
            Self::ZoneElsewhere => f.write_str("a zone that isn't the town's"),
            Self::NameLength { chars } => write!(f, "a name of {chars} characters, more than {MAX_NAME_CHARS}"),
            Self::PointFar { km } => write!(f, "a point {km:.1} km from the town, more than {POINT_NEAR_KM}"),
            Self::PointUnread => f.write_str("a point on the map that isn't two numbers"),
            Self::TooManyToday => write!(f, "{POSTS_PER_DAY} listings posted in the last day already"),
            Self::SameTitleToday => f.write_str("a listing with this title posted in the last day already"),
        }
    }
}

impl std::error::Error for FieldError {}

/// What is wrong with a listing, field by field, by the form's names for them ("title", "price"),
/// in the order found; the first for a field is the one to say.
pub type FieldErrors = Vec<(&'static str, FieldError)>;

/// The rules a new listing and an edit share: each field's problems in the `FieldErrors`, and the
/// listing they leave when there are none.
pub fn check<'a>(input: &'a ListingInput, categories: &'a Categories, cities: &'a [City]) -> (Option<Checked<'a>>, FieldErrors) {
    let mut errors = FieldErrors::new();

    let category = categories.by_slug(&input.category);
    if category.is_none() {
        errors.push(("category", FieldError::NoCategory));
    }
    // A category with subcategories only groups them.
    let category = category.filter(|category| {
        let leaf = !categories.has_children(category.id);
        if !leaf {
            errors.push(("category", FieldError::GroupCategory));
        }
        leaf
    });
    // A category of one kind is of that kind, whatever the form says.
    let listing_type = category.and_then(|category| match category.listing_types.as_slice() {
        [only] => ListingType::parse(only),
        kinds => input.listing_type.filter(|wanted| kinds.iter().any(|kind| kind == wanted.as_str())),
    });
    if category.is_some() && listing_type.is_none() {
        errors.push(("listing_type", FieldError::NoListingType));
    }
    let condition = input.condition.filter(|_| category.is_some_and(|category| category.uses_condition));

    let title = input.title.trim();
    let description = input.description.trim();
    match (title.chars().count(), link_count(title)) {
        (chars, _) if !(MIN_TITLE_CHARS..=MAX_TITLE_CHARS).contains(&chars) => errors.push(("title", FieldError::TitleLength { chars })),
        (_, links) if links > 0 => errors.push(("title", FieldError::TitleLinks { links })),
        _ => {}
    }
    match (description.chars().count(), link_count(description)) {
        (chars, _) if !(MIN_DESCRIPTION_CHARS..=MAX_DESCRIPTION_CHARS).contains(&chars) => {
            errors.push(("description", FieldError::DescriptionLength { chars }));
        }
        (_, links) if links > MAX_LINKS => errors.push(("description", FieldError::DescriptionLinks { links })),
        _ => {}
    }

    let price = match price_of(input.price_type, input.price, &input.currency) {
        Ok(price) => Some(price),
        Err(error) => {
            errors.push(("price", error));
            None
        }
    };

    let city = cities.iter().find(|city| city.slug == input.city);
    if city.is_none() {
        errors.push(("city", FieldError::NoCity));
    }
    // A property may say its zone in the town; another town's is a mistake. Other listings have
    // none.
    let zone = match (category, city) {
        (Some(category), Some(city)) if !input.zone.is_empty() && zoned(categories, category) => {
            let zone = city.zones.iter().find(|zone| zone.slug == input.zone);
            if zone.is_none() {
                errors.push(("zone", FieldError::ZoneElsewhere));
            }
            zone
        }
        _ => None,
    };
    let name = input.name.trim();
    let name_chars = name.chars().count();
    if name_chars > MAX_NAME_CHARS {
        errors.push(("name", FieldError::NameLength { chars: name_chars }));
    }
    // Where on the map, more exactly than the town's point: a property's or a car's alone, near
    // the town chosen, rounded to about a hundred metres. Another category's is let go.
    let point = match (input.latitude.trim(), input.longitude.trim(), category, city) {
        ("", "", ..) | (_, _, None, _) | (_, _, _, None) => None,
        (_, _, Some(category), _) if !mapped(categories, category) => None,
        (latitude, longitude, _, Some(city)) => match (latitude.parse::<f64>(), longitude.parse::<f64>()) {
            (Ok(latitude), Ok(longitude)) if latitude.is_finite() && longitude.is_finite() => {
                let km = distance_km((latitude, longitude), (city.latitude, city.longitude));
                if km <= POINT_NEAR_KM {
                    Some(rounded((latitude, longitude)))
                } else {
                    errors.push(("point", FieldError::PointFar { km }));
                    None
                }
            }
            _ => {
                errors.push(("point", FieldError::PointUnread));
                None
            }
        },
    };

    let checked = match (category, listing_type, price, city) {
        (Some(category), Some(listing_type), Some(price), Some(city)) if errors.is_empty() => {
            Some(Checked { category, listing_type, condition, title, description, price, city, zone, name, point })
        }
        _ => None,
    };
    (checked, errors)
}

/// A listing's price by the rules the posting form and the AI's fill share: a fixed or negotiable
/// price needs an amount above 0, kept to the cent, that the price column holds (under a
/// trillion); the other kinds keep none, whatever was typed. No kind, or one the form doesn't offer
/// (PRICE_TYPES), is a fixed price; any currency but lekë (ALL) is euros.
pub fn price_of(price_type: Option<PriceType>, amount: Option<Decimal>, currency: &str) -> Result<Price, FieldError> {
    let price_type = price_type.filter(|kind| PRICE_TYPES.contains(kind)).unwrap_or(PriceType::Fixed);
    let amount = match price_type {
        // Above 0 as typed: an amount under a cent goes in as 0.
        PriceType::Fixed | PriceType::Negotiable => match amount.map(|typed| (typed, typed.round_dp(2))) {
            Some((typed, cents)) if typed > Decimal::ZERO && cents < Decimal::from(PRICE_LIMIT) => Some(cents),
            _ => return Err(FieldError::NoPrice),
        },
        PriceType::From | PriceType::Free | PriceType::OnRequest => None,
    };
    let currency = match currency {
        "ALL" => "ALL",
        _ => "EUR",
    };
    Ok(Price { price_type, amount, currency })
}

/// How many listings the user posted in the last day, and whether one of them has this title
/// (db::recent_posts): what `daily_limit` judges.
pub struct RecentPosts {
    pub count: i64,
    pub same_title: bool,
}

/// Whether the account may post one more listing, by what it posted over the last day: a few a day
/// (POSTS_PER_DAY), and the same one only once. If not, the field that says why, and why.
pub fn daily_limit(recent: &RecentPosts) -> Option<(&'static str, FieldError)> {
    if recent.count >= POSTS_PER_DAY {
        Some(("form", FieldError::TooManyToday))
    } else if recent.same_title {
        Some(("title", FieldError::SameTitleToday))
    } else {
        None
    }
}

/// How many links a text has, however they are written ("http://", "https://", "www.", in any
/// case): what a listing's title and description may have few of, and a message none of
/// (web/messages.rs).
pub fn link_count(text: &str) -> usize {
    let text = text.to_lowercase();
    ["http://", "https://", "www."].iter().map(|marker| text.matches(marker).count()).sum()
}

/// Whether the category's listings can say their zone in the town: property's (ZONED), and its
/// subcategories'.
pub fn zoned(categories: &Categories, category: &Category) -> bool {
    category.slug == ZONED || categories.ancestors(category).iter().any(|above| above.slug == ZONED)
}

/// Whether the category's listings can say where they are on the map, more exactly than their
/// town's point (MAPPED).
pub fn mapped(categories: &Categories, category: &Category) -> bool {
    MAPPED.contains(&category.slug.as_str()) || categories.ancestors(category).iter().any(|above| MAPPED.contains(&above.slug.as_str()))
}

/// How far apart two points are, in kilometres (the haversine, on a sphere the earth's size).
pub fn distance_km((latitude, longitude): (f64, f64), (to_latitude, to_longitude): (f64, f64)) -> f64 {
    let (from, to) = (latitude.to_radians(), to_latitude.to_radians());
    let (across, along) = ((to_latitude - latitude).to_radians(), (to_longitude - longitude).to_radians());
    let a = (across / 2.0).sin().powi(2) + from.cos() * to.cos() * (along / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().asin()
}

/// A point to about a hundred metres (three decimals of a degree): near enough to say where a
/// property or a car is, not a private seller's front door.
fn rounded(point: (f64, f64)) -> (f64, f64) {
    ((point.0 * 1000.0).round() / 1000.0, (point.1 * 1000.0).round() / 1000.0)
}

/// A category's slug and those of the categories above it, the nearest first (its own alone where
/// `categories`, the active ones, lack it): what a listing's page is laid out by, a car's (CARS)
/// or a property's (MAPPED), and whose icons its details take (web/icons.rs).
pub fn lineage<'a>(categories: &'a Categories, slug: &'a str) -> Vec<&'a str> {
    match categories.by_slug(slug) {
        Some(category) => std::iter::once(category)
            .chain(categories.ancestors(category).into_iter().rev())
            .map(|category| category.slug.as_str())
            .collect(),
        None => vec![slug],
    }
}

/// What a loan for a listing would be on, where its page offers the loan calculator: something on
/// sale at a price of its own (PriceType::priced) that costs enough to borrow for, more than 500 €
/// or, about the same, 50 000 lekë; none for a rent, a wanted ad, a price on request or a free one,
/// nor in another currency.
pub fn loan_amount(kind: ListingType, price_type: PriceType, currency: &str, amount: Option<Decimal>) -> Option<Decimal> {
    let enough = match currency {
        "EUR" => Decimal::from(500),
        "ALL" => Decimal::from(50_000),
        _ => return None,
    };
    amount.filter(|amount| price_type.priced() && kind == ListingType::Sell && *amount > enough)
}

/// A price lowered lately: what it was (db's price_was!: the lowest of the 30 days before it last
/// changed, when that change made it 1 % lower or more), what it is, and by how much, in whole
/// percent, at least 1, as the pill beside a price says it ("−7%").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceDrop {
    pub from: Decimal,
    pub to: Decimal,
    pub percent: i64,
}

/// By how much a price dropped from `was`, where it did: none without a `was` (db's price_was!
/// gives one only for a drop), or without a price.
pub fn price_drop(was: Option<Decimal>, price: Option<Decimal>) -> Option<PriceDrop> {
    let (from, to) = (was?, price?);
    let percent = ((from - to).checked_div(from)? * Decimal::ONE_HUNDRED).round().to_i64()?;
    Some(PriceDrop { from, to, percent: percent.max(1) })
}

/// How a car's price compares with what cars of its model and year ask (db's car_model_prices: the
/// median of those two years either side), both in euros: what a car's page says in the bubble
/// under its price (web/car_prices.rs, car_price.html), the cards' clear bargain (`bargain`), the
/// listing form's hint on a price typed, and the API's `verdict`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CarVerdict {
    /// 15 % or more under the median: green.
    Good,
    /// Within 15 % of it: yellow.
    Near,
    /// 15 % or more over it: red.
    Above,
}

impl CarVerdict {
    /// What a car's page says of it, in the language now: "Çmim i mirë", "Rreth mesatares", "Mbi
    /// mesataren".
    pub fn words(self) -> &'static str {
        match self {
            Self::Good => t("Çmim i mirë"),
            Self::Near => t("Rreth mesatares"),
            Self::Above => t("Mbi mesataren"),
        }
    }

    /// The bubble's colour, as a property's square metre has them (listing.css; ZoneVerdict::tone).
    pub fn tone(self) -> &'static str {
        match self {
            Self::Good => "price-below",
            Self::Near => "price-near",
            Self::Above => "price-above",
        }
    }
}

/// The verdict on a car's price against the median, both in euros: none under half of it, where a
/// price says only that it is likely wrong (lekë typed as euros), or a lure; none without a median.
pub fn car_verdict(price_eur: Decimal, median_eur: Decimal) -> Option<CarVerdict> {
    if median_eur <= Decimal::ZERO {
        return None;
    }
    match price_eur / median_eur {
        ratio if ratio < Decimal::new(50, 2) => None,
        ratio if ratio <= Decimal::new(85, 2) => Some(CarVerdict::Good),
        ratio if ratio < Decimal::new(115, 2) => Some(CarVerdict::Near),
        _ => Some(CarVerdict::Above),
    }
}

/// A clear bargain, as a card marks one ("Çmim i mirë"; the API's `good_price`): a car whose price
/// in euros is well under what cars of its model and year ask (`car_verdict`, Good: a card has the
/// median, `median_eur`, for a car on sale at a price alone), unless its price dropped lately
/// (`dropped`), whose pill says enough.
pub fn bargain(price_eur: Option<Decimal>, median_eur: Option<Decimal>, dropped: bool) -> bool {
    match (price_eur, median_eur) {
        (Some(price), Some(median)) => !dropped && car_verdict(price, median) == Some(CarVerdict::Good),
        _ => false,
    }
}

/// What a square metre of a property on sale costs: its price over its area, in its own currency,
/// to the whole euro or lek; only for one on sale at a price of its own (PriceType::priced) with an
/// area. Rents, wanted ads and "price on request" have none.
pub fn area_price(kind: ListingType, price_type: PriceType, amount: Option<Decimal>, area: Option<Decimal>) -> Option<Decimal> {
    let (amount, area) = (amount?, area?);
    if kind != ListingType::Sell || !price_type.priced() || area <= Decimal::ZERO {
        return None;
    }
    Some((amount / area).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero))
}

/// How a property's price per square metre compares with what one costs in its zone on average
/// (`zone_price`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ZoneVerdict {
    /// ZONE_MARGIN_EUR or more under it: green.
    Below,
    /// The very same, as both show rounded: yellow.
    Same,
    /// Less than ZONE_MARGIN_EUR from it either way: yellow.
    Near,
    /// ZONE_MARGIN_EUR or more over it: red.
    Above,
}

impl ZoneVerdict {
    /// What the page's card says of it, in the language now.
    pub fn words(self) -> &'static str {
        match self {
            Self::Below => t("Nën mesataren"),
            Self::Same => t("Njësoj si mesatarja"),
            Self::Near => t("Afër mesatares"),
            Self::Above => t("Mbi mesataren"),
        }
    }

    /// The bubble's colour (listing.css), as a car's price has them (CarVerdict::tone).
    pub fn tone(self) -> &'static str {
        match self {
            Self::Below => "price-below",
            Self::Same | Self::Near => "price-near",
            Self::Above => "price-above",
        }
    }
}

/// A property's price per square metre against its zone's average.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZonePrice {
    /// The zone's average in the listing's currency, rounded as the price per square metre is.
    pub average: Decimal,
    /// The price per square metre less that average, in the listing's currency: below zero under
    /// it.
    pub difference: Decimal,
    pub verdict: ZoneVerdict,
}

/// `per_area`, a property's price per square metre as `area_price` gives it, against what one costs
/// in its zone on average among the zone's sales in its category (`average_eur`, in euros: db's
/// zone_area_prices), at the rate of the listing's currency (`eur_rate`, euros a unit of it):
/// within ZONE_MARGIN_EUR of it either way is about the same. The two are compared rounded, as a
/// page shows them, so that its figures add up. None without a rate, and for a starting price
/// ("nga"), which says too little to compare.
pub fn zone_price(per_area: Decimal, price_type: PriceType, average_eur: Decimal, eur_rate: Option<Decimal>) -> Option<ZonePrice> {
    let rate = eur_rate.filter(|rate| *rate > Decimal::ZERO)?;
    if price_type == PriceType::From {
        return None;
    }
    let average = (average_eur / rate).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
    let difference = per_area - average;
    let margin = Decimal::from(ZONE_MARGIN_EUR);
    let verdict = match difference * rate {
        eur if eur <= -margin => ZoneVerdict::Below,
        eur if eur >= margin => ZoneVerdict::Above,
        _ if difference.is_zero() => ZoneVerdict::Same,
        _ => ZoneVerdict::Near,
    };
    Some(ZonePrice { average, difference, verdict })
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn category(id: u128, parent: Option<u128>, slug: &str, kinds: &[ListingType], uses_condition: bool) -> Category {
        Category {
            id: Uuid::from_u128(id),
            parent_id: parent.map(Uuid::from_u128),
            name: slug.to_string(),
            slug: slug.to_string(),
            default_currency: "EUR".to_string(),
            listing_types: kinds.iter().map(ListingType::to_string).collect(),
            uses_condition,
        }
    }

    /// A few of the site's categories: cars and flats under the groups they are in, phones and
    /// jobs on their own.
    fn categories() -> Categories {
        use ListingType::{Buy, Job, Rent, Sell, WantedRent};
        Categories::new(vec![
            category(1, None, "automjete", &[Sell, Buy, Rent], true),
            category(2, Some(1), "makina", &[Sell, Buy, Rent], true),
            category(3, None, "prona", &[Sell, Rent, WantedRent], false),
            category(4, Some(3), "apartamente", &[Sell, Rent, WantedRent], false),
            category(5, None, "telefona", &[Sell, Buy], true),
            category(6, None, "pune", &[Job], false),
        ])
    }

    /// Tiranë, with Blloku, and Durrës, some 31 km west of it, with no zones.
    fn cities() -> Vec<City> {
        let blloku = Zone { id: Uuid::from_u128(11), name: "Blloku".to_string(), slug: "blloku".to_string() };
        vec![
            City {
                id: Uuid::from_u128(10),
                name: "Tiranë".to_string(),
                slug: "tirane".to_string(),
                county: "Tiranë".to_string(),
                latitude: 41.328333,
                longitude: 19.818056,
                zones: vec![blloku],
            },
            City {
                id: Uuid::from_u128(20),
                name: "Durrës".to_string(),
                slug: "durres".to_string(),
                county: "Durrës".to_string(),
                latitude: 41.313333,
                longitude: 19.445833,
                zones: Vec::new(),
            },
        ]
    }

    /// A used car for sale in Tiranë, at a fixed price, that checks out.
    fn car() -> ListingInput {
        ListingInput {
            category: "makina".to_string(),
            listing_type: Some(ListingType::Sell),
            condition: Some(Condition::Used),
            title: "  Volkswagen Golf 7, 2016 ".to_string(),
            description: "148 000 km, kambio manuale, naftë.".to_string(),
            price_type: Some(PriceType::Fixed),
            price: Some(Decimal::from(9500)),
            currency: "EUR".to_string(),
            city: "tirane".to_string(),
            ..ListingInput::default()
        }
    }

    /// A flat for rent in Tiranë, by the month, that checks out.
    fn flat() -> ListingInput {
        ListingInput {
            category: "apartamente".to_string(),
            listing_type: Some(ListingType::Rent),
            title: "Apartament 2+1 me qira".to_string(),
            description: "I mobiluar, kati i tretë, me ashensor.".to_string(),
            price_type: Some(PriceType::Negotiable),
            price: Some(Decimal::from(450)),
            currency: "EUR".to_string(),
            city: "tirane".to_string(),
            ..ListingInput::default()
        }
    }

    /// What the rules find wrong with `input`.
    fn errors(input: &ListingInput) -> FieldErrors {
        check(input, &categories(), &cities()).1
    }

    /// The rules applied to `changed`, a car changed by it.
    fn car_with(changed: impl FnOnce(&mut ListingInput)) -> FieldErrors {
        let mut input = car();
        changed(&mut input);
        errors(&input)
    }

    #[test]
    fn a_listing_that_says_what_it_must_checks_out() {
        let (categories, cities) = (categories(), cities());
        let input = car();
        let (checked, found) = check(&input, &categories, &cities);
        assert_eq!(found, FieldErrors::new());
        let checked = checked.expect("a car that checks out");
        assert_eq!(checked.category.slug, "makina");
        assert_eq!(checked.listing_type, ListingType::Sell);
        assert_eq!(checked.condition, Some(Condition::Used));
        assert_eq!(checked.title, "Volkswagen Golf 7, 2016");
        assert_eq!(checked.price, Price { price_type: PriceType::Fixed, amount: Some(Decimal::from(9500)), currency: "EUR" });
        assert_eq!(checked.city.slug, "tirane");
        assert!(checked.zone.is_none() && checked.point.is_none());
        assert_eq!(checked.name, "");
    }

    #[test]
    fn a_listing_goes_into_a_category_of_the_kinds_it_takes() {
        assert_eq!(car_with(|car| car.category = "anije".to_string()), vec![("category", FieldError::NoCategory)]);
        assert_eq!(car_with(|car| car.category = "automjete".to_string()), vec![("category", FieldError::GroupCategory)]);
        assert_eq!(car_with(|car| car.listing_type = None), vec![("listing_type", FieldError::NoListingType)]);
        assert_eq!(car_with(|car| car.listing_type = Some(ListingType::Job)), vec![("listing_type", FieldError::NoListingType)]);
        // A category of one kind is of that kind, whatever the form says.
        let (categories, cities) = (categories(), cities());
        let job = ListingInput { category: "pune".to_string(), listing_type: Some(ListingType::Sell), ..car() };
        let (checked, found) = check(&job, &categories, &cities);
        assert_eq!(found, FieldErrors::new());
        let checked = checked.unwrap();
        assert_eq!(checked.listing_type, ListingType::Job);
        // And a condition only where the category asks for one.
        assert_eq!(checked.condition, None);
    }

    #[test]
    fn a_title_takes_5_to_120_characters_and_no_links() {
        assert_eq!(car_with(|car| car.title = " Golf ".to_string()), vec![("title", FieldError::TitleLength { chars: 4 })]);
        assert_eq!(car_with(|car| car.title = "ë".repeat(121)), vec![("title", FieldError::TitleLength { chars: 121 })]);
        assert_eq!(car_with(|car| car.title = "ë".repeat(120)), FieldErrors::new());
        assert_eq!(car_with(|car| car.title = "Golf 7 te WWW.makina.al".to_string()), vec![("title", FieldError::TitleLinks { links: 1 })]);
        // Too short says so first.
        assert_eq!(car_with(|car| car.title = "www.".to_string()), vec![("title", FieldError::TitleLength { chars: 4 })]);
    }

    #[test]
    fn a_description_takes_10_to_5000_characters_and_2_links() {
        assert_eq!(car_with(|car| car.description = "Si e re.".to_string()), vec![("description", FieldError::DescriptionLength { chars: 8 })]);
        assert_eq!(car_with(|car| car.description = "a".repeat(5001)), vec![("description", FieldError::DescriptionLength { chars: 5001 })]);
        let two = "Foto te https://foto.al/1 dhe http://foto.al/2.";
        assert_eq!(car_with(|car| car.description = two.to_string()), FieldErrors::new());
        let three = format!("{two} Më shumë te www.makina.al.");
        assert_eq!(car_with(|car| car.description = three), vec![("description", FieldError::DescriptionLinks { links: 3 })]);
    }

    #[test]
    fn links_count_however_they_are_written() {
        assert_eq!(link_count("Shiko HTTPS://a.al, http://b.al dhe Www.c.al"), 3);
        // An address with both counts twice, as it always has.
        assert_eq!(link_count("https://www.a.al"), 2);
        assert_eq!(link_count("httpsx, www, http:/"), 0);
    }

    #[test]
    fn a_fixed_or_negotiable_price_needs_an_amount_above_0() {
        for kind in [PriceType::Fixed, PriceType::Negotiable] {
            for amount in [None, Some(Decimal::ZERO), Some(Decimal::from(-5))] {
                assert_eq!(price_of(Some(kind), amount, "EUR"), Err(FieldError::NoPrice), "{kind} {amount:?}");
            }
        }
        // None the price column can't hold.
        assert_eq!(price_of(Some(PriceType::Fixed), Some(Decimal::from(PRICE_LIMIT)), "EUR"), Err(FieldError::NoPrice));
        let most = Decimal::new(99_999_999_999_999, 2);
        assert_eq!(price_of(Some(PriceType::Fixed), Some(most), "EUR").unwrap().amount, Some(most));
        // Rounded up to it, too much: 999 999 999 999.999 9 is a trillion to the cent.
        assert_eq!(price_of(Some(PriceType::Fixed), Some(Decimal::new(9_999_999_999_999_999, 4)), "EUR"), Err(FieldError::NoPrice));
        // The form says so under the price.
        assert_eq!(car_with(|car| car.price = None), vec![("price", FieldError::NoPrice)]);
    }

    #[test]
    fn a_free_price_or_one_on_request_keeps_no_amount() {
        for kind in [PriceType::Free, PriceType::OnRequest] {
            let price = price_of(Some(kind), Some(Decimal::from(300)), "ALL").unwrap();
            assert_eq!(price, Price { price_type: kind, amount: None, currency: "ALL" });
            assert_eq!(price_of(Some(kind), None, "EUR").unwrap().amount, None);
        }
        let (categories, cities) = (categories(), cities());
        let free = ListingInput { price_type: Some(PriceType::Free), ..car() };
        assert_eq!(check(&free, &categories, &cities).0.unwrap().price.amount, None);
    }

    #[test]
    fn a_price_is_a_fixed_one_in_euros_to_the_cent_unless_it_says_otherwise() {
        let price = |kind, amount: Decimal, currency| price_of(kind, Some(amount), currency).unwrap();
        // To the cent.
        assert_eq!(price(Some(PriceType::Fixed), Decimal::new(123_456, 4), "EUR").amount, Some(Decimal::new(1235, 2)));
        // Above 0 as typed: under a cent, 0.
        assert_eq!(price(Some(PriceType::Fixed), Decimal::new(4, 3), "EUR").amount, Some(Decimal::ZERO));
        // No kind, or one the form doesn't offer, is a fixed price.
        assert_eq!(price(None, Decimal::from(10), "EUR").price_type, PriceType::Fixed);
        assert_eq!(price(Some(PriceType::From), Decimal::from(10), "EUR").price_type, PriceType::Fixed);
        assert_eq!(price(Some(PriceType::Negotiable), Decimal::from(10), "EUR").price_type, PriceType::Negotiable);
        // Lekë, or euros.
        assert_eq!(price(None, Decimal::from(10), "ALL").currency, "ALL");
        for other in ["EUR", "USD", "all", ""] {
            assert_eq!(price(None, Decimal::from(10), other).currency, "EUR", "{other}");
        }
    }

    #[test]
    fn a_zone_is_a_property_listings_and_its_towns() {
        let (categories, cities) = (categories(), cities());
        let in_blloku = ListingInput { zone: "blloku".to_string(), ..flat() };
        assert_eq!(check(&in_blloku, &categories, &cities).0.unwrap().zone.map(|zone| zone.slug.as_str()), Some("blloku"));
        let elsewhere = ListingInput { city: "durres".to_string(), ..in_blloku.clone() };
        assert_eq!(errors(&elsewhere), vec![("zone", FieldError::ZoneElsewhere)]);
        // No town, no zone said wrong.
        let nowhere = ListingInput { city: "atlantis".to_string(), ..in_blloku };
        assert_eq!(errors(&nowhere), vec![("city", FieldError::NoCity)]);
        // None for a car's, whatever it says.
        let car = ListingInput { zone: "blloku".to_string(), ..car() };
        let (checked, found) = check(&car, &categories, &cities);
        assert!(checked.unwrap().zone.is_none());
        assert_eq!(found, FieldErrors::new());
    }

    #[test]
    fn a_point_on_the_map_is_a_property_or_a_cars_near_its_town() {
        let (categories, cities) = (categories(), cities());
        let at = |input: ListingInput, latitude: &str, longitude: &str| ListingInput {
            latitude: latitude.to_string(),
            longitude: longitude.to_string(),
            ..input
        };
        // Near Tiranë, rounded to about a hundred metres.
        let near = at(flat(), " 41.334567", "19.812345 ");
        assert_eq!(check(&near, &categories, &cities).0.unwrap().point, Some((41.335, 19.812)));
        assert_eq!(check(&at(car(), "41.4", "19.7"), &categories, &cities).0.unwrap().point, Some((41.4, 19.7)));
        // Durrës is near enough; Shkodër isn't.
        assert!(errors(&at(car(), "41.313", "19.446")).is_empty());
        let far = errors(&at(car(), "42.0683", "19.5126"));
        assert!(matches!(far.as_slice(), [("point", FieldError::PointFar { km })] if *km > POINT_NEAR_KM), "{far:?}");
        for (latitude, longitude) in [("41,33", "19,81"), ("", "19.8"), ("NaN", "19.8"), ("inf", "19.8")] {
            assert_eq!(errors(&at(car(), latitude, longitude)), vec![("point", FieldError::PointUnread)], "{latitude} {longitude}");
        }
        // None for a phone, whatever it says, nor without one.
        let phone = ListingInput { category: "telefona".to_string(), ..at(car(), "1", "2") };
        let (checked, found) = check(&phone, &categories, &cities);
        assert_eq!((checked.unwrap().point, found), (None, FieldErrors::new()));
        assert_eq!(check(&at(car(), " ", ""), &categories, &cities).0.unwrap().point, None);
    }

    #[test]
    fn the_distance_between_two_towns_is_as_the_crow_flies() {
        let (tirane, durres) = ((41.328333, 19.818056), (41.313333, 19.445833));
        let km = distance_km(tirane, durres);
        assert!((30.5..32.0).contains(&km), "{km}");
        assert_eq!(distance_km(tirane, tirane), 0.0);
    }

    #[test]
    fn the_name_buyers_see_takes_60_characters() {
        assert_eq!(car_with(|car| car.name = format!(" {} ", "ë".repeat(60))), FieldErrors::new());
        assert_eq!(car_with(|car| car.name = "ë".repeat(61)), vec![("name", FieldError::NameLength { chars: 61 })]);
    }

    #[test]
    fn every_problem_is_said_under_its_field() {
        let (nothing, categories, cities) = (ListingInput::default(), categories(), cities());
        let (checked, found) = check(&nothing, &categories, &cities);
        assert!(checked.is_none());
        let fields: Vec<&str> = found.iter().map(|(field, _)| *field).collect();
        assert_eq!(fields, ["category", "title", "description", "price", "city"]);
        // In English, for logs.
        assert_eq!(FieldError::TitleLength { chars: 3 }.to_string(), "a title of 3 characters, not 5 to 120");
    }

    #[test]
    fn an_account_posts_a_few_listings_a_day_and_each_once() {
        let recent = |count, same_title| RecentPosts { count, same_title };
        assert_eq!(daily_limit(&recent(0, false)), None);
        assert_eq!(daily_limit(&recent(POSTS_PER_DAY - 1, true)), Some(("title", FieldError::SameTitleToday)));
        assert_eq!(daily_limit(&recent(POSTS_PER_DAY, true)), Some(("form", FieldError::TooManyToday)));
    }

    #[test]
    fn a_page_is_laid_out_by_its_category_and_those_above_it() {
        let categories = categories();
        assert_eq!(lineage(&categories, "makina"), ["makina", "automjete"]);
        assert_eq!(lineage(&categories, "apartamente"), ["apartamente", "prona"]);
        assert_eq!(lineage(&categories, "telefona"), ["telefona"]);
        // One the active categories lack, alone.
        assert_eq!(lineage(&categories, "anije"), ["anije"]);
        assert_eq!(MAPPED, ["prona", CARS]);
    }

    #[test]
    fn things_on_sale_at_a_price_of_their_own_offer_a_loan() {
        let loan = |kind, price_type, currency, amount: i64| loan_amount(kind, price_type, currency, Some(Decimal::from(amount)));
        assert_eq!(loan(ListingType::Sell, PriceType::Fixed, "EUR", 9_500), Some(Decimal::from(9_500)));
        assert_eq!(loan(ListingType::Sell, PriceType::From, "ALL", 50_001), Some(Decimal::from(50_001)));
        // Too little to borrow for.
        assert_eq!(loan(ListingType::Sell, PriceType::Negotiable, "EUR", 500), None);
        assert_eq!(loan(ListingType::Sell, PriceType::Fixed, "ALL", 50_000), None);
        // Not on sale, no price of its own, another currency, or no price.
        assert_eq!(loan(ListingType::Rent, PriceType::Fixed, "EUR", 9_500), None);
        assert_eq!(loan(ListingType::Sell, PriceType::OnRequest, "EUR", 9_500), None);
        assert_eq!(loan(ListingType::Sell, PriceType::Fixed, "USD", 9_500), None);
        assert_eq!(loan_amount(ListingType::Sell, PriceType::Fixed, "EUR", None), None);
    }

    #[test]
    fn a_price_drops_by_whole_percent_one_at_least() {
        let dropped = |was: i64, price: i64| price_drop(Some(Decimal::from(was)), Some(Decimal::from(price)));
        assert_eq!(dropped(10_000, 9_300), Some(PriceDrop { from: Decimal::from(10_000), to: Decimal::from(9_300), percent: 7 }));
        // A drop db's price_was! counts is 1 % or more, which rounding mustn't make none.
        assert_eq!(dropped(10_000, 9_950).map(|drop| drop.percent), Some(1));
        assert_eq!(price_drop(None, Some(Decimal::from(9_000))), None);
        assert_eq!(price_drop(Some(Decimal::from(9_000)), None), None);
        assert_eq!(dropped(0, 0), None, "nothing to divide by");
    }

    #[test]
    fn a_cars_price_is_good_well_under_the_median_and_says_nothing_far_under_it() {
        let median = Decimal::from(10_000);
        let verdict_at = |price: i64| car_verdict(Decimal::from(price), median);
        assert_eq!(verdict_at(4_999), None);
        assert_eq!(verdict_at(5_000), Some(CarVerdict::Good));
        assert_eq!(verdict_at(8_500), Some(CarVerdict::Good));
        assert_eq!(verdict_at(8_501), Some(CarVerdict::Near));
        assert_eq!(verdict_at(11_499), Some(CarVerdict::Near));
        assert_eq!(verdict_at(11_500), Some(CarVerdict::Above));
        assert_eq!(car_verdict(Decimal::from(8_000), Decimal::ZERO), None, "no median");
        // A clear bargain, as a card marks one: a good price, not lately dropped.
        let bargain_at = |price: i64, dropped| bargain(Some(Decimal::from(price)), Some(median), dropped);
        assert!(bargain_at(8_500, false) && bargain_at(5_000, false));
        assert!(!bargain_at(8_600, false) && !bargain_at(4_900, false), "86 %, or under half: likely wrong");
        assert!(!bargain_at(8_000, true), "its drop says enough");
        assert!(!bargain(Some(Decimal::from(8_000)), None, false), "a car without a median, or not a car");
        // Said on its page in the language now; in JSON by its name.
        assert_eq!((CarVerdict::Near.words(), CarVerdict::Good.tone()), ("Rreth mesatares", "price-below"));
        assert_eq!(serde_json::to_value(CarVerdict::Above).unwrap(), "above");
    }

    #[test]
    fn a_propertys_square_metre_is_weighed_against_its_zones() {
        let per_area = |kind, price_type, amount: i64, area: i64| {
            area_price(kind, price_type, Some(Decimal::from(amount)), Some(Decimal::from(area)))
        };
        assert_eq!(per_area(ListingType::Sell, PriceType::Fixed, 185_000, 200), Some(Decimal::from(925)));
        // To the whole euro, a half up.
        assert_eq!(per_area(ListingType::Sell, PriceType::From, 1_001, 2), Some(Decimal::from(501)));
        assert_eq!(per_area(ListingType::Rent, PriceType::Fixed, 450, 85), None);
        assert_eq!(per_area(ListingType::Sell, PriceType::OnRequest, 450, 85), None);
        assert_eq!(per_area(ListingType::Sell, PriceType::Fixed, 450, 0), None);
        assert_eq!(area_price(ListingType::Sell, PriceType::Fixed, None, Some(Decimal::from(85))), None);

        // A zone's average of 1 000 €: 200 € under it or more, green; over it, red; between, yellow.
        let eur = Some(Decimal::ONE);
        let against = |per_area: i64| {
            zone_price(Decimal::from(per_area), PriceType::Fixed, Decimal::from(1_000), eur).map(|zone| (zone.difference, zone.verdict))
        };
        assert_eq!(against(800), Some((Decimal::from(-200), ZoneVerdict::Below)));
        assert_eq!(against(890), Some((Decimal::from(-110), ZoneVerdict::Near)));
        assert_eq!(against(1_000), Some((Decimal::ZERO, ZoneVerdict::Same)));
        assert_eq!(against(1_200), Some((Decimal::from(200), ZoneVerdict::Above)));
        // In lekë, at the rate (a lek a hundredth of a euro): the average 100 000 lekë, and 15 000
        // under it is 150 € under it, about it.
        let lek = zone_price(Decimal::from(85_000), PriceType::Fixed, Decimal::from(1_000), Some(Decimal::new(1, 2))).unwrap();
        assert_eq!((lek.average, lek.verdict), (Decimal::from(100_000), ZoneVerdict::Near));
        // A starting price, or no rate, is weighed against nothing.
        assert_eq!(zone_price(Decimal::from(800), PriceType::From, Decimal::from(1_000), eur), None);
        assert_eq!(zone_price(Decimal::from(800), PriceType::Fixed, Decimal::from(1_000), None), None);
        assert_eq!(zone_price(Decimal::from(800), PriceType::Fixed, Decimal::from(1_000), Some(Decimal::ZERO)), None);
        assert_eq!((ZoneVerdict::Same.words(), ZoneVerdict::Same.tone()), ("Njësoj si mesatarja", "price-near"));
        assert_eq!(serde_json::to_value(ZoneVerdict::Below).unwrap(), "below");
    }
}
