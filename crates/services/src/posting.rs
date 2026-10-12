//! Posting a listing, besides the form that sends it: what the pages' posting form (pluribus-web's
//! post.rs) and the JSON API's listings (pluribus-api's own.rs) share, moved out of the pages in
//! C9. What the listing's rules find wrong, said in the form's words in the language now
//! (`message`, for each domain::listing::FieldError); whether an account may post one more
//! listing (`may_post`: POSTS_PER_DAY a day, each title once, domain::listing::daily_limit by what
//! db::recent_posts says); why a new listing waits for a moderator (`review_for`, a
//! domain::ReviewReason); who it is posted as, the account, the contact buyers get and the store
//! (`Poster`, `stores_posting_for`), and the listing as the database saves it (`listing_to_save`,
//! a db::NewListing); the zone its words name (`zone_named`, by places.rs); and what cars of a
//! model and year ask, as the form's hint says it under a price typed (`price_hint`, by
//! listings::CarPrices). The limit on posting without signing in, per network, stays the pages'
//! (the API is signed in alone), and so does the check that a person sent the form.

use pluribus_core::{
    domain::{
        City, Condition, ReviewReason, Zone,
        listing::{self, CarVerdict, Checked, FieldError, MAX_DESCRIPTION_CHARS, MAX_TITLE_CHARS, POSTS_PER_DAY, car_verdict},
    },
    i18n::{t, t1},
};
use pluribus_db::{self as db, Categories, FormAttribute, ListingProperty, MyStore, NewListing, SessionUser};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{AppState, listings::CarPrices, places};

/// What the form says of a problem a listing's rules found (domain::listing::FieldError), in the
/// language now: the pages' words under their fields, which the API gives under its own.
pub fn message(error: FieldError) -> String {
    match error {
        FieldError::NoCategory => t("Zgjidh kategorinë.").to_string(),
        FieldError::GroupCategory => t("Zgjidh nënkategorinë, p.sh. Makina te Automjete.").to_string(),
        FieldError::NoListingType => t("Zgjidh llojin e njoftimit.").to_string(),
        FieldError::TitleLength { .. } => t("Titulli duhet të ketë 5 deri në 120 shkronja.").to_string(),
        FieldError::TitleLinks { .. } => t("Titulli nuk mund të ketë lidhje.").to_string(),
        FieldError::DescriptionLength { .. } => t("Përshkrimi duhet të ketë 10 deri në 5000 shkronja.").to_string(),
        FieldError::DescriptionLinks { .. } => t("Përshkrimi mund të ketë deri në 2 lidhje.").to_string(),
        FieldError::NoPrice => t("Shkruaj çmimin, ose zgjidh „Falas“ ose „Me marrëveshje“.").to_string(),
        FieldError::NoCity => t("Zgjidh qytetin.").to_string(),
        FieldError::ZoneElsewhere => t("Zgjidh një zonë të qytetit.").to_string(),
        FieldError::NameLength { .. } => t("Emri mund të ketë deri në 60 shkronja.").to_string(),
        FieldError::PointFar { .. } => t("Pika në hartë është larg qytetit që zgjodhe: vendose te qyteti, ose hiqe.").to_string(),
        FieldError::PointUnread => t("Pika në hartë nuk u lexua: vendose përsëri.").to_string(),
        FieldError::TooManyToday => t1("Ke publikuar {} njoftime sot. Mund të publikosh të tjera nesër.", POSTS_PER_DAY),
        FieldError::SameTitleToday => t("Ke publikuar tashmë një njoftim me këtë titull sot.").to_string(),
    }
}

/// Why an account may not post a listing now (domain::listing::daily_limit): POSTS_PER_DAY in the
/// last day already, which the form says over itself (`field` "form") and the API answers as too
/// many requests; or one with the same title, which the form says under the title.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Refused {
    pub field: &'static str,
    pub error: FieldError,
}

/// Whether `account` may post a listing titled `title` now: a few a day (POSTS_PER_DAY), and each
/// title once (domain::listing::daily_limit, by db::recent_posts), whoever it is posted for.
pub async fn may_post(db: &PgPool, account: Uuid, title: &str) -> sqlx::Result<Result<(), Refused>> {
    let recent = db::recent_posts(db, account, title.trim()).await?;
    Ok(match listing::daily_limit(&recent) {
        Some((field, error)) => Err(Refused { field, error }),
        None => Ok(()),
    })
}

/// Why a new listing waits for a moderator before it goes up, on a site with moderators (db's
/// review.rs): a photo another seller's listing has, the very same file by its fingerprint
/// (`hashes`, db::photos_elsewhere; `seller`, the store it is posted for or else the poster), is
/// DUPLICATE_PHOTOS; else, posted without signing in, the first listing of an account made with
/// it, its contact confirmed by no code (`unconfirmed`, the pages' alone), NEW_ACCOUNT. None on a
/// site without moderators, or for one that goes up at once.
pub async fn review_for(state: &AppState, hashes: &[Vec<u8>], seller: Uuid, unconfirmed: bool) -> sqlx::Result<Option<ReviewReason>> {
    Ok(if state.moderators.is_empty() {
        None
    } else if db::photos_elsewhere(&state.db, hashes, seller).await? {
        Some(ReviewReason::DuplicatePhotos)
    } else if unconfirmed {
        Some(ReviewReason::NewAccount)
    } else {
        None
    })
}

/// Who a listing is posted as: the account, the contact buyers get, and the store it is posted
/// for, if any.
pub struct Poster<'a> {
    pub id: Uuid,
    pub phone: Option<&'a str>,
    pub email: Option<&'a str>,
    pub business_id: Option<Uuid>,
}

impl<'a> Poster<'a> {
    /// A signed-in user, posting as themselves: buyers get their number, or their address where
    /// they have no number.
    pub fn of(user: &'a SessionUser) -> Self {
        let phone = user.phone.as_deref();
        Self { id: user.id, phone, email: if phone.is_none() { user.email.as_deref() } else { None }, business_id: None }
    }

    /// Posting for a store: buyers get its public number, or the poster's contact when it has none.
    pub fn for_store(user: &'a SessionUser, store: &'a MyStore) -> Self {
        match store.public_phone.as_deref() {
            Some(phone) => Self { id: user.id, phone: Some(phone), email: None, business_id: Some(store.id) },
            None => Self { business_id: Some(store.id), ..Self::of(user) },
        }
    }

    /// The seller whose photos are its own (review_for): the store it is posted for, or else the
    /// poster.
    pub fn seller(&self) -> Uuid {
        self.business_id.unwrap_or(self.id)
    }
}

/// The stores `user` may post for (domain::Role::posts), which "Posto si" offers.
pub async fn stores_posting_for(db: &PgPool, user: Uuid) -> sqlx::Result<Vec<MyStore>> {
    Ok(db::my_stores(db, user).await?.into_iter().filter(MyStore::posts).collect())
}

/// The properties a listing in the category `slug` names is asked for, its posting form's
/// (db::form_attributes): none for a category the site hasn't, or one that only groups others.
pub async fn form_attributes_of(db: &PgPool, categories: &Categories, slug: &str) -> sqlx::Result<Vec<FormAttribute>> {
    match categories.by_slug(slug).filter(|category| !categories.has_children(category.id)) {
        Some(category) => db::form_attributes(db, category.id).await,
        None => Ok(Vec::new()),
    }
}

/// A listing that checks out, as the database saves it, posted as `poster`, published at once or
/// waiting for its code (`publish`): a function rather than a method of `Checked`, which is
/// pluribus-core's. Why it waits for a moderator, if it does, is the caller's to say
/// (`NewListing::review`, review_for).
pub fn listing_to_save<'a>(
    checked: &Checked<'a>,
    poster: &Poster<'a>,
    slug: &'a str,
    properties: &'a [ListingProperty],
    publish: bool,
) -> NewListing<'a> {
    NewListing {
        user_id: poster.id,
        category_id: checked.category.id,
        location_id: checked.zone.map_or(checked.city.id, |zone| zone.id),
        title: checked.title,
        slug,
        description: checked.description,
        listing_type: checked.listing_type.as_str(),
        condition: checked.condition.map(Condition::as_str),
        price_amount: checked.price.amount,
        currency: checked.price.currency,
        price_type: checked.price.price_type.as_str(),
        contact_name: (!checked.name.is_empty()).then_some(checked.name),
        phone: poster.phone,
        email: poster.email,
        properties,
        publish,
        review: None,
        business_id: poster.business_id,
    }
}

/// The zone a new listing's title or description names, with its town (places::zone_in), which
/// its form picks while the seller hasn't: in the town chosen (`city`, its slug, empty for none),
/// or, with none chosen, in a town the words name too ("Blloku, Tiranë"). The title first: a
/// description may name the places nearby too. As much of the words as the form takes, and no more.
pub fn zone_named<'a>(cities: &'a [City], title: &str, description: &str, city: &str) -> Option<(&'a City, &'a Zone)> {
    let town = cities.iter().find(|town| town.slug == city);
    let title: String = title.chars().take(MAX_TITLE_CHARS).collect();
    let description: String = description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    [title, description].iter().find_map(|text| places::zone_in(text, town, cities))
}

/// What the posting form's hint under a car's price says (`price_hint`).
#[derive(Debug, Clone, PartialEq)]
pub struct PriceHint {
    /// What cars of the model and year ask, in a sentence, in the listing's currency
    /// (CarPrices::text).
    pub text: String,
    /// How the price typed compares, as the car's page will judge it (domain::listing::car_verdict):
    /// none without a price, or for one so far under them that it is likely wrong.
    pub verdict: Option<CarVerdict>,
    /// The same in a sentence, where a price was typed: what the car's page will say of it, or,
    /// far under them, that it may be wrong, or in the wrong currency.
    pub said: Option<String>,
}

/// What cars of `model` (its code in the form: VOLKSWAGEN_GOLF) and `year` ask, in `currency` (lekë,
/// ALL, or else euros), and how `price`, as typed ("9 500"; its thousands' spaces or dots aside, a
/// comma before its cents), compares: the posting form's hint (pluribus-web's car_prices.rs,
/// /shto/cmimi) and the API's. None without a model or a year, or where too few cars of them are up
/// to say, or without a rate for the currency.
pub async fn price_hint(db: &PgPool, model: &str, year: Option<i32>, currency: &str, price: &str) -> sqlx::Result<Option<PriceHint>> {
    let currency = if currency == "ALL" { "ALL" } else { "EUR" };
    let (Some(year), false) = (year, model.is_empty()) else {
        return Ok(None);
    };
    let (prices, rate) = tokio::try_join!(db::model_prices(db, model, year), db::eur_rate(db, currency))?;
    let (Some(prices), Some(rate)) = (prices, rate.filter(|rate| *rate > Decimal::ZERO)) else {
        return Ok(None);
    };
    let Some(shown) = CarPrices::new(&prices, currency, Some(rate)) else {
        return Ok(None);
    };
    let digits: String = price.chars().filter(|c| c.is_ascii_digit() || *c == ',').collect();
    let price = digits.replace(',', ".").parse::<Decimal>().ok().filter(|price| *price > Decimal::ZERO);
    let verdict = price.and_then(|price| car_verdict(price * rate, prices.median_eur));
    let said = price.map(|_| match verdict {
        Some(verdict) => t1("Te faqja e makinës, çmimi yt del „{}“.", verdict.words()),
        None => t("Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?").to_string(),
    });
    Ok(Some(PriceHint { text: shown.text(), verdict, said }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each problem the rules find is said as the form said it before they had codes.
    #[test]
    fn each_problem_is_said_in_the_forms_words() {
        assert_eq!(message(FieldError::TitleLength { chars: 3 }), "Titulli duhet të ketë 5 deri në 120 shkronja.");
        assert_eq!(message(FieldError::PointFar { km: 80.0 }), "Pika në hartë është larg qytetit që zgjodhe: vendose te qyteti, ose hiqe.");
        assert_eq!(message(FieldError::TooManyToday), "Ke publikuar 5 njoftime sot. Mund të publikosh të tjera nesër.");
        assert_eq!(message(FieldError::NoPrice), "Shkruaj çmimin, ose zgjidh „Falas“ ose „Me marrëveshje“.");
    }
}
