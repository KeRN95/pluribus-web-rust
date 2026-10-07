//! A listing's page, its phone number reveal, and its views.

use std::cmp::Ordering;

use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use rust_decimal::{Decimal, RoundingStrategy};

use super::{
    Result,
    account::{ClientIp, cookie_header, current_user},
    browse::ListingPage,
    cookie, not_found,
    post::POSTED_COOKIE,
    properties, render, render_stream,
};
use crate::{
    AppState,
    db::{self, ListingCard, ListingDetail, MapPoint, Photo, SellerContact},
    format,
};

impl ListingCard {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// "1 234": how many times its page was seen this week, for "Më të klikuarat".
    pub fn views_display(&self) -> Option<String> {
        self.views.map(|views| format::number(Decimal::from(views)))
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// The photos a big card slides through, the cover first: each its card-sized copy and its
    /// full one, for the browser to pick from (ListingPage::sizes). A small card shows the first.
    pub fn slides(&self) -> impl Iterator<Item = (&str, &str)> {
        self.photos.iter().zip(&self.photos_full).map(|(small, full)| (small.as_str(), full.as_str()))
    }

    /// "Përdorues që nga tetor 2026", under the seller's name over a big card's photo.
    pub fn seller_since_text(&self) -> String {
        format!("Përdorues që nga {}", format::month_year(self.seller_since))
    }

    /// The seller's initial, in a circle before their name, as on the listing page.
    pub fn seller_initial(&self) -> String {
        format::initial(&self.seller_name)
    }

    /// The city at the start of the details line: the listing's municipality, or its location if
    /// that isn't in one. None when the search is in that city already.
    pub fn city_label(&self) -> Option<&str> {
        if self.city_known {
            return None;
        }
        self.city.as_deref().or(self.location_name.as_deref())
    }

    /// The place at the start of the details line: the zone and the town ("Blloku, Tiranë"), the
    /// town alone, or the zone alone when the search is in that town already (city_label).
    pub fn place_label(&self) -> Option<String> {
        match (&self.zone, self.city_label()) {
            (Some(zone), Some(city)) => Some(format!("{zone}, {city}")),
            (Some(zone), None) => Some(zone.clone()),
            (None, city) => city.map(str::to_string),
        }
    }

    /// What follows the city on the details line: the distance from the map pin, when there is
    /// one, then the category's card properties. Without the city, one more property goes in
    /// first, so the line still says something new: "Apple · 128 GB" rather than "Vlorë · 128 GB".
    pub fn details_line(&self) -> String {
        let distance = self.distance_m.map(format::distance);
        let spare = self.spare_detail.clone().filter(|_| self.city_known);
        let parts: Vec<String> =
            distance.into_iter().chain(spare).chain(self.details.iter().cloned()).collect();
        parts.join(" · ")
    }
}

impl MapPoint {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }
}

/// How far a property's price per square metre may be from its zone's average, in euros, and still
/// count as about it (yellow): further under it, it is cheap for the zone (green), further over it,
/// dear (red).
const ZONE_MARGIN_EUR: i64 = 200;

impl ListingDetail {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// The price itself; what follows it, smaller on the page, is price_note.
    pub fn price_main(&self) -> String {
        format::price_parts(self.price_amount, &self.currency, &self.price_type, &self.listing_type).0
    }

    /// "/muaj", " (i diskutueshëm)", or empty.
    pub fn price_note(&self) -> String {
        format::price_parts(self.price_amount, &self.currency, &self.price_type, &self.listing_type).1
    }

    /// A property on sale with a price and an area: what a square metre of it costs, in its own
    /// currency and to the whole euro or lek, with what it was worked out from, for the bubble
    /// under the price and the card it opens. Rents, wanted ads and "price on request" have none.
    pub fn area_price(&self) -> Option<AreaPrice> {
        let priced = matches!(self.price_type.as_str(), "FIXED" | "NEGOTIABLE" | "FROM");
        let (amount, area) = (self.price_amount?, self.area_m2?);
        if self.listing_type != "SELL" || !priced || area <= Decimal::ZERO {
            return None;
        }
        let per_area = (amount / area).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
        let from = if self.price_type == "FROM" { "nga " } else { "" };
        Some(AreaPrice {
            per_area: format!("{from}{}/m²", format::money(per_area, &self.currency)),
            price: format!("{from}{}", format::money(amount, &self.currency)),
            area: format!("{}\u{a0}m²", format::number(area)),
            rounded_to: if self.currency == "ALL" { "lek" } else { "euro" },
            zone: self.zone_price(per_area),
        })
    }

    /// How `per_area`, the price per square metre as the bubble shows it, compares with what one
    /// costs in the listing's zone on average, where its zone has an average: within
    /// ZONE_MARGIN_EUR of it either way is about the same. The two are compared rounded, as the card
    /// shows them, so that its figures add up. A starting price ("nga") says too little to compare.
    fn zone_price(&self, per_area: Decimal) -> Option<ZonePrice> {
        let zone = self.zone.as_ref()?;
        let (average_eur, listings) = (self.zone_average_eur?, self.zone_listings?);
        let rate = self.eur_rate.filter(|rate| *rate > Decimal::ZERO)?;
        if self.price_type == "FROM" {
            return None;
        }
        let average = (average_eur / rate).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
        let difference = per_area - average;
        let margin = Decimal::from(ZONE_MARGIN_EUR);
        let (tone, verdict) = match difference * rate {
            eur if eur <= -margin => ("price-below", "Nën mesataren"),
            eur if eur >= margin => ("price-above", "Mbi mesataren"),
            _ if difference.is_zero() => ("price-near", "Njësoj si mesatarja"),
            _ => ("price-near", "Afër mesatares"),
        };
        let gap = format::money(difference.abs(), &self.currency);
        let difference = match difference.cmp(&Decimal::ZERO) {
            Ordering::Less => Some(format!("{gap}/m² më lirë")),
            Ordering::Greater => Some(format!("{gap}/m² më shtrenjtë")),
            Ordering::Equal => None,
        };
        Some(ZonePrice {
            zone: zone.clone(),
            category: self.category_name.clone(),
            average: format!("{}/m²", format::money(average, &self.currency)),
            listings: format::listing_count(listings, i64::MAX),
            tone,
            verdict,
            difference,
        })
    }

    /// The price a loan calculator starts from: only for things on sale with a price of their own
    /// (not rents, wanted ads or "price on request") that cost enough to borrow for, at least
    /// 10 000 € or, about the same, 1 000 000 Lekë.
    pub fn loan_amount(&self) -> Option<Decimal> {
        let priced = matches!(self.price_type.as_str(), "FIXED" | "NEGOTIABLE" | "FROM");
        let enough = match self.currency.as_str() {
            "EUR" => Decimal::from(10_000),
            "ALL" => Decimal::from(1_000_000),
            _ => return None,
        };
        self.price_amount.filter(|amount| priced && self.listing_type == "SELL" && *amount >= enough)
    }

    pub fn currency_symbol(&self) -> &str {
        format::currency_symbol(&self.currency)
    }

    pub fn seller_initial(&self) -> String {
        format::initial(&self.seller_name)
    }

    /// A business sells it, not a private seller: its seller's profile is a business's.
    pub fn seller_is_business(&self) -> bool {
        self.seller_url.starts_with("/biznes/")
    }

    /// "Në Pluribus që nga tetor 2026 · 12 njoftime".
    pub fn seller_summary(&self) -> String {
        format!(
            "Në Pluribus që nga {} · {}",
            format::month_year(self.seller_since),
            format::listing_count(self.seller_listings, i64::MAX)
        )
    }

    pub fn published_display(&self) -> Option<String> {
        self.published_at.map(format::date)
    }

    /// What kind of listing it is, and in what condition when it has one: "Në shitje · E përdorur".
    pub fn kind_line(&self) -> String {
        let kind = format::listing_type(&self.listing_type);
        match self.condition.as_deref() {
            Some(condition) => format!("{kind} · {}", format::condition(condition)),
            None => kind.to_string(),
        }
    }

    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "RESERVED" => Some("Rezervuar"),
            "SOLD" => Some("Shitur"),
            _ => None,
        }
    }
}

#[derive(Template)]
#[template(path = "listing.html")]
struct ListingShow {
    listing: ListingDetail,
    /// Its category and those above it, from the top: Automjete, Pjesë dhe aksesorë, GPS dhe
    /// navigatorë.
    trail: Vec<Crumb>,
    details: Vec<Detail>,
    photos: Vec<Photo>,
    /// The reveal button's label, when the seller's phone number or e-mail can be shown.
    /// The button that shows how to reach the seller, when it can be shown.
    reveal: Option<Reveal>,
    /// The visitor posted this listing: the page offers to edit it.
    own: bool,
    /// Posted just now without signing in: the page says it is up, and how to change it later.
    posted: bool,
    /// The seller's other listings, over the related ones (db::seller_listings); None when this
    /// is their only one. The page names them `page` too while it shows them.
    seller_page: Option<ListingPage>,
    /// The seller has more than those: a link leads to all of them.
    seller_more: bool,
    /// Listings like it, under it (db::related_listings): `page`, the name listing_card.html
    /// reads its cards by.
    page: ListingPage,
}

/// URLs look like /listing/1042-vw-golf-7: the number identifies the listing, and the slug is
/// cosmetic, so an outdated slug redirects to the current one.
pub async fn show(State(state): State<AppState>, headers: HeaderMap, Path(key): Path<String>) -> Result<Response> {
    let Some(public_no) = key.split('-').next().and_then(|n| n.parse::<i64>().ok()) else {
        return not_found().await;
    };
    let Some(listing) = db::listing_by_public_no(&state.db, public_no).await? else {
        return not_found().await;
    };
    let canonical = format!("{}-{}", listing.public_no, listing.slug);
    if key != canonical {
        return Ok(Redirect::permanent(&format!("/listing/{canonical}")).into_response());
    }
    // One more of the seller's listings than the page shows says whether they have more.
    let (attributes, photos, contact, categories, mut from_seller, related) = tokio::try_join!(
        db::listing_attributes(&state.db, listing.id, listing.category_id),
        db::listing_photos(&state.db, listing.id),
        db::visible_contact(&state.db, listing.id),
        db::categories(&state.db),
        db::seller_listings(&state.db, listing.id, db::FROM_SELLER + 1),
        db::related_listings(&state.db, listing.id, db::RELATED),
    )?;
    let crumb = |name: &str, slug: &str| Crumb { name: name.to_string(), url: format!("/c/{slug}") };
    let trail = match categories.by_slug(&listing.category_slug) {
        Some(category) => categories
            .ancestors(category)
            .into_iter()
            .chain([category])
            .map(|category| crumb(&category.name, &category.slug))
            .collect(),
        None => vec![crumb(&listing.category_name, &listing.category_slug)],
    };
    let own = match current_user(&state, &headers).await? {
        Some(user) => db::is_own_listing(&state.db, listing.id, user.id).await?,
        None => false,
    };
    let reveal = contact.map(|contact| match contact {
        SellerContact::Phone(_) => Reveal::Phone,
        SellerContact::Email(_) => Reveal::Email,
    });
    // A car's title says its make, model, body type and colour: its details leave them out.
    let titled = listing.heading.is_some();
    let mut details: Vec<Detail> = attributes
        .into_iter()
        .filter(|attribute| !(titled && properties::TITLE_CODES.contains(&attribute.code.as_str())))
        .map(|attribute| Detail {
            icon: properties::icon(&attribute.code),
            multi: attribute.multi,
            label: attribute.label,
            values: attribute.shown,
        })
        .collect();
    // A property's zone in its town comes first among its details.
    if let Some(zone) = &listing.zone {
        let zone = Detail { label: "Zona".to_string(), values: vec![zone.clone()], icon: properties::icon("zone"), multi: false };
        details.insert(0, zone);
    }
    // Said once: the cookie that says so (post.rs) goes with this answer.
    let posted = !own && cookie(&headers, POSTED_COOKIE).is_some_and(|no| no == listing.public_no.to_string());
    let seller_more = from_seller.len() as i64 > db::FROM_SELLER;
    from_seller.truncate(db::FROM_SELLER as usize);
    let seller_page = (!from_seller.is_empty()).then(|| ListingPage::related(from_seller));
    let page = ListingPage::related(related);
    let mut response =
        render(ListingShow { listing, trail, details, photos, reveal, own, posted, seller_page, seller_more, page })?;
    if posted {
        response.headers_mut().append(header::SET_COOKIE, cookie_header(&headers, POSTED_COOKIE, "", 0));
    }
    Ok(response)
}

/// What a square metre of a property on sale costs, and what it was worked out from.
pub struct AreaPrice {
    /// "925 €/m²", or "nga 925 €/m²" for a starting price.
    pub per_area: String,
    /// The price it was worked out from: "185 000 €".
    pub price: String,
    /// The area: "200 m²".
    pub area: String,
    /// What the result is rounded to: "euro" or "lek".
    pub rounded_to: &'static str,
    /// How it compares with its zone's average, where its zone has one.
    pub zone: Option<ZonePrice>,
}

/// A property's price per square metre against its zone's average (zone_area_prices).
pub struct ZonePrice {
    /// The zone's name, "Blloku", and the listing's category's, "Apartamente".
    pub zone: String,
    pub category: String,
    /// The average, in the listing's currency, rounded as the price per square metre is:
    /// "1 000 €/m²".
    pub average: String,
    /// How many sales it is worked out from: "124 njoftime".
    pub listings: String,
    /// The bubble's class: "price-below" (green), "price-near" (yellow) or "price-above" (red).
    pub tone: &'static str,
    /// What that means: "Nën mesataren", "Afër mesatares", "Mbi mesataren", or "Njësoj si
    /// mesatarja" when the two are the same.
    pub verdict: &'static str,
    /// By how much: "300 €/m² më lirë", "300 €/m² më shtrenjtë"; None when the same.
    pub difference: Option<String>,
}

/// One of a listing's details, as pills: its icon before its value where it has one, otherwise
/// its name; a pill for each of several values (a car's options), each with a check.
pub struct Detail {
    pub label: String,
    pub values: Vec<String>,
    pub icon: Option<&'static str>,
    pub multi: bool,
}

impl Detail {
    /// Whether its name goes before `value`: not where the value says it, as "6 GB RAM" does RAM.
    pub fn named_before(&self, value: &str) -> bool {
        !value.split_whitespace().any(|word| word.eq_ignore_ascii_case(&self.label))
    }
}

/// How buyers reach the seller, behind a button that shows it and counts the showing: the phone
/// number, or the e-mail address of a seller who signed in by e-mail.
#[derive(Clone, Copy)]
pub enum Reveal {
    Phone,
    Email,
}

impl Reveal {
    /// The button's name, for screen readers and as a tooltip: it shows only an icon.
    pub fn label(&self) -> &'static str {
        match self {
            Reveal::Phone => "Shfaq numrin",
            Reveal::Email => "Shfaq email-in",
        }
    }

    /// The copy button's name, beside the number or address once shown.
    pub fn copy_label(&self) -> &'static str {
        match self {
            Reveal::Phone => "Kopjo numrin",
            Reveal::Email => "Kopjo email-in",
        }
    }
}

/// A category on the way to a listing's own.
pub struct Crumb {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "phone_stream.html")]
struct PhoneStream {
    reveal: Reveal,
    href: String,
    display: String,
    /// What the copy button copies: the number as a phone dials it, or the address.
    copy: String,
}

/// POST /listing/{public_no}/pare: the listing's page was shown to a visitor (application.js says
/// so as Turbo shows it, so that a page fetched ahead under the pointer, and never opened, doesn't
/// count, nor do bots that run no scripts), for "Më të klikuarat" (views.rs). Its seller seeing it
/// doesn't count either: their page doesn't ask. Visitors are told apart by their address and
/// browser, and whatever says it is a bot is left out.
pub async fn seen(
    State(state): State<AppState>,
    Path(public_no): Path<i64>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
) -> Result<StatusCode> {
    let agent = headers.get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();
    let lower = agent.to_ascii_lowercase();
    if agent.is_empty() || ["bot", "spider", "crawl", "slurp"].iter().any(|word| lower.contains(word)) {
        return Ok(StatusCode::NO_CONTENT);
    }
    let visitor = format!("{}
{agent}", ip.unwrap_or_default());
    state.views.record(&state.db, public_no, &visitor).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Swaps the button for the seller's number (a tap calls it) or e-mail address.
pub async fn reveal_phone(
    State(state): State<AppState>,
    Path(public_no): Path<i64>,
) -> Result<Response> {
    let Some(contact) = db::reveal_contact(&state.db, public_no).await? else {
        return not_found().await;
    };
    let (reveal, href, display, copy) = match contact {
        SellerContact::Phone(phone) => (Reveal::Phone, format!("tel:{phone}"), format::phone(&phone), phone),
        SellerContact::Email(email) => (Reveal::Email, format!("mailto:{email}"), email.clone(), email),
    };
    render_stream(PhoneStream { reveal, href, display, copy })
}
