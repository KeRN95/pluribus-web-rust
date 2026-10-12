//! A listing's own rules, apart from the form that sends it and the database that keeps it: what a
//! new listing or an edit must say to go up (`check`: a category listings go into, a kind of
//! listing it takes, a title and a description of the right length with few links, a price, a
//! town and, for a property, a zone of it, the name buyers see, and a property's or a car's own
//! point on the map, near its town), the price rules the posting form and the AI's fill share
//! (`price_of`), and how many listings an account may post in a day (`daily_limit`). What is wrong
//! comes back as codes by field (`FieldErrors`), which web/post.rs says in the page's language.
//! Reading what people typed stays there too (a number as they type it, the form's fields), and so
//! does the check that a person sent the form, which the sign-in form shares.

use std::fmt;

use rust_decimal::Decimal;

use super::{Condition, ListingType, PriceType};
use crate::db::{Categories, Category, City, RecentPosts, Zone};

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

/// The categories whose listings' pages show where they are on a map, and whose form can put them
/// there more exactly than their town's point: property and cars, and those under them.
/// web/listings.rs lays their pages out by a copy of its own (its MAPPED), which a test in
/// web/post.rs holds to this one.
pub const MAPPED: [&str; 2] = ["prona", "makina"];

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

#[cfg(test)]
mod tests {
    use sqlx::types::Json;
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
                zones: Json(vec![blloku]),
            },
            City {
                id: Uuid::from_u128(20),
                name: "Durrës".to_string(),
                slug: "durres".to_string(),
                county: "Durrës".to_string(),
                latitude: 41.313333,
                longitude: 19.445833,
                zones: Json(Vec::new()),
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
}
