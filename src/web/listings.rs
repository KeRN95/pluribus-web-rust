//! A listing's page, its phone number reveal, and its views.

use std::cmp::Ordering;

use askama::Template;
use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use rust_decimal::{Decimal, RoundingStrategy, prelude::ToPrimitive};

use super::{
    Result,
    account::{ClientIp, cookie_header, current_user},
    browse::{self, ListingPage, NextPage, Subcategory, category_icon, verified_label},
    car_prices,
    cookie, is_next_page_request, not_found,
    icons::{self, Reads},
    photos,
    post::POSTED_COOKIE,
    properties, render, render_stream, safety,
    seo::{self, PageMeta},
};
use crate::{
    AppState,
    db::{self, ListingCard, ListingDetail, MapPoint, Photo, SellerContact},
    format,
    media::{CARD_SIDE, FULL_SIDE},
};

/// How wide a listing's photos show, for the browser to pick a copy of each (Photo::webp_srcset):
/// as wide as the page on a wide screen (listing.css), up to 760px on others.
pub const GALLERY_SIZES: &str =
    "(min-width: 1100px) 1068px, (min-width: 960px) calc(100vw - 2rem), (min-width: 792px) 760px, calc(100vw - 2rem)";
/// The same for every other listing, whose photo stands at the left of its page from 960 px up,
/// seven twelfths of the page, which is 60rem at most there, less the 2rem between the columns
/// (.listing-split, listing.css).
pub const SPLIT_GALLERY_SIZES: &str =
    "(min-width: 992px) 541px, (min-width: 960px) calc((100vw - 4rem) * .5833), (min-width: 792px) 760px, calc(100vw - 2rem)";
/// The same for a car's page, whose photos run across the page, which is 60rem at most from 960 px
/// up (.listing-car, listing.css).
pub const CAR_GALLERY_SIZES: &str =
    "(min-width: 992px) 960px, (min-width: 960px) calc(100vw - 2rem), (min-width: 792px) 760px, calc(100vw - 2rem)";

/// The down payment the loan calculator takes for given, as a percentage of the price
/// (loan_calculator.html, loan_controller.js): what Albania's banks ask for a used car, which
/// they finance 60 to 65% of (Intesa Sanpaolo 65%, Tirana Bank 60%, in 2026; a new car 70 to
/// 75%, a lease 80%). The loan is the rest of the price.
pub const LOAN_DOWN_PERCENT: u32 = 35;

/// A car's key facts, in a row under its title (listing.html, .key-facts), in this order: the
/// year, the mileage, the gearbox and the fuel, as FINN has them. They leave the details.
const CAR_FACTS: [&str; 4] = ["vehicle_year", "vehicle_mileage", "vehicle_transmission", "vehicle_fuel"];

/// The categories whose listings' pages show where they are on a map, under the seller: property
/// (Prona and those under it) and cars (Makina), which buyers go to see.
const MAPPED: [&str; 2] = ["prona", "makina"];

/// Where a property or a car is, on the map under its seller (listing_map_controller.js): its
/// point, which is its place's (its town's, for a zone without one of its own), so a circle round
/// it rather than a pin; and the place's name, "Blloku, Tiranë".
pub struct ListingMap {
    pub latitude: f64,
    pub longitude: f64,
    pub place: String,
}

/// A photo on a card, with the copies the browser may pick from.
pub struct Slide<'a> {
    /// Its card-sized copy, WebP for a photo kept as WebP.
    pub small: &'a str,
    /// What the img shows where the browser picks nothing else: a WebP photo's JPEG, for browsers
    /// without WebP, which the others never fetch; else the card-sized copy.
    pub src: &'a str,
    /// The img's srcset, for a photo kept before WebP: the card-sized copy and the full one.
    pub srcset: Option<String>,
    /// The WebP copies' srcset (photos::srcset), for the picture's source.
    pub webp: Option<String>,
    /// Before it, the cards' AVIF copies' (photos::avif_srcset), for the browsers that show AVIF.
    pub avif: Option<String>,
    /// Its tiny copy as a data: URI, behind it while it loads.
    pub placeholder: Option<&'a str>,
    /// Wider than tall: a big card's frame, 4:3, cuts it at its sides and scales it by its height
    /// (ListingPage::sizes_of).
    pub landscape: bool,
}

impl Photo {
    /// A WebP photo's copies for the browser to pick from on the listing page, which shows it
    /// whole (GALLERY_SIZES): each by its longer side.
    pub fn webp_srcset(&self) -> Option<String> {
        let full = self.webp_url.as_deref()?;
        let (width, height) = photos::Copies::size(self.width_px, self.height_px);
        let copies = photos::Copies { card: &self.card_url, middle: self.middle_url.as_deref(), full, width, height };
        Some(photos::srcset(&copies, photos::Measure::Longer))
    }

    /// What the frame shows blurred around the photo, and behind it while it loads: its tiny copy,
    /// or for a photo kept before there were any, its card-sized one.
    pub fn backdrop(&self) -> &str {
        self.placeholder.as_deref().unwrap_or(&self.card_url)
    }

    /// Its thumbnail in the filmstrip under the photos (listing.html, .gallery-thumbs): the mini
    /// copy of a photo kept as WebP, else its card-sized copy.
    pub fn thumb(&self) -> String {
        photos::mini_url(&self.card_url).unwrap_or_else(|| self.card_url.clone())
    }
}

impl ListingCard {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// "1 234": how many times its page was seen this week, for "Më të klikuarat".
    pub fn views_display(&self) -> Option<String> {
        self.views.map(|views| format::number(Decimal::from(views)))
    }

    /// Over the photo of a kept listing that is no longer simply up (/te-ruajturat alone lists
    /// them): "Rezervuar", or "Shitur" ("Dhënë me qira" for a rental).
    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "RESERVED" => Some("Rezervuar"),
            "SOLD" => Some(format::sold(&self.listing_type)),
            _ => None,
        }
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// What the list's card shows of its description: all of it, or, when it is longer than
    /// db::DESCRIPTION_EXCERPT, that much, cut at a word and ending in "…".
    pub fn description_excerpt(&self) -> String {
        if self.description.chars().count() <= db::DESCRIPTION_EXCERPT {
            return self.description.clone();
        }
        let cut: String = self.description.chars().take(db::DESCRIPTION_EXCERPT).collect();
        let at_word = cut.rfind(' ').filter(|&at| at > cut.len() / 2).map_or(cut.as_str(), |at| &cut[..at]);
        format!("{}…", at_word.trim_end_matches([',', '.', ';', ':', '-']))
    }

    /// A clear bargain: a car whose price is well under what cars of its model and year ask
    /// (car_prices::verdict), as its page says; not one whose price dropped lately, whose pill
    /// says enough.
    pub fn bargain(&self) -> bool {
        let good = match (self.price_normalized, self.model_median) {
            (Some(price), Some(median)) => car_prices::verdict(price, median) == Some(car_prices::Verdict::Good),
            _ => false,
        };
        good && self.price_drop().is_none()
    }

    /// By how much its price dropped lately, and from what (price_drop), for the badge beside it.
    pub fn price_drop(&self) -> Option<(i64, String)> {
        price_drop(self.price_was, self.price_amount, &self.currency)
    }

    /// The photos a big card slides through, the cover first, each with its copies for the
    /// browser to pick from (ListingPage::sizes_of), each by its shorter side, which a card's
    /// square scales it by: a WebP photo's four, and before them the cards' two as AVIF, or the
    /// card-sized copy and the full one of a photo kept before. A small card shows the first.
    pub fn slides(&self) -> impl Iterator<Item = Slide<'_>> {
        self.photos.iter().zip(&self.photos_full).enumerate().map(|(i, (small, full))| {
            match self.photos_webp.get(i).and_then(Option::as_ref) {
                Some(webp) => {
                    let (width, height) = photos::Copies::size(webp.width, webp.height);
                    let copies = photos::Copies { card: &webp.card, middle: webp.middle.as_deref(), full: &webp.full, width, height };
                    Slide {
                        small,
                        src: full,
                        srcset: None,
                        webp: Some(photos::srcset(&copies, photos::Measure::Shorter)),
                        avif: photos::avif_srcset(&copies, photos::Measure::Shorter),
                        placeholder: webp.placeholder.as_deref(),
                        landscape: width > height,
                    }
                }
                None => Slide {
                    small,
                    src: small,
                    srcset: Some(format!("{small} {CARD_SIDE}w, {full} {FULL_SIDE}w")),
                    webp: None,
                    avif: None,
                    placeholder: None,
                    landscape: false,
                },
            }
        })
    }

    /// "Përdorues që nga tetor 2026", under the seller's name over a big card's photo.
    pub fn seller_since_text(&self) -> String {
        format!("Përdorues që nga {}", format::month_year(self.seller_since))
    }

    /// When it was posted, or last brought up again: "Sot", "para 3 ditësh", the date from a week
    /// on; at the right end of its line in the list (.cards-list in page.css).
    pub fn when_text(&self) -> String {
        format::when(self.sort_at, time::OffsetDateTime::now_utc())
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

    /// The place under the price in the list, whatever the search is in: the zone and the town
    /// ("Blloku, Tiranë"), or the town alone; the listing's municipality, or its location if that
    /// isn't in one.
    pub fn place_text(&self) -> Option<String> {
        let city = self.city.as_deref().or(self.location_name.as_deref());
        match (&self.zone, city) {
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
    /// The note on a line of its own under the price, as a car's page has it: without its
    /// brackets, "i diskutueshëm", "/muaj i diskutueshëm".
    pub fn price_note_alone(&self) -> String {
        self.price_note().replace(['(', ')'], "").trim().to_string()
    }

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
    /// (not rents, wanted ads or "price on request") that cost enough to borrow for, more than
    /// 500 € or, about the same, 50 000 Lekë.
    pub fn loan_amount(&self) -> Option<Decimal> {
        let priced = matches!(self.price_type.as_str(), "FIXED" | "NEGOTIABLE" | "FROM");
        let enough = match self.currency.as_str() {
            "EUR" => Decimal::from(500),
            "ALL" => Decimal::from(50_000),
            _ => return None,
        };
        self.price_amount.filter(|amount| priced && self.listing_type == "SELL" && *amount > enough)
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

    /// On a car's page, where the seller's card is narrow, since when they are on the site, the
    /// summary's words shorter: "Që nga tetor 2026", or for a business "Biznes · që nga mars 2026".
    pub fn seller_since_line(&self) -> String {
        let since = format::month_year(self.seller_since);
        if self.seller_is_business() { format!("Biznes · që nga {since}") } else { format!("Që nga {since}") }
    }

    /// Under the seller's name, how quickly they answer and when they were last on the site, when
    /// there is something to say: "Përgjigjet zakonisht brenda 1 ore · Aktiv sot".
    pub fn seller_activity(&self) -> Option<String> {
        format::seller_activity(self.seller_answers, self.seller_seen, time::OffsetDateTime::now_utc())
    }

    /// And after a dot on that line, how many listings they have (seller_card.html): "12 njoftime".
    pub fn seller_count_text(&self) -> String {
        format::listing_count(self.seller_listings, i64::MAX)
    }

    /// When it was published, as the pill over the photo says it ("Sot", "Dje", "para 3 ditësh",
    /// the date; format::when), with what screen readers hear before it: "Publikuar " for the
    /// words, "Publikuar më " for a date.
    pub fn published_display(&self) -> Option<(&'static str, String)> {
        let published = self.published_at?;
        let when = format::when(published, time::OffsetDateTime::now_utc());
        let prefix = if when.starts_with(|c: char| c.is_ascii_digit()) { "Publikuar më " } else { "Publikuar " };
        Some((prefix, when))
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
            "SOLD" => Some(format::sold(&self.listing_type)),
            "PAUSED" => Some("Çaktivizuar"),
            "EXPIRED" => Some("Skaduar"),
            "SUSPENDED" => Some("Në shqyrtim"),
            "REJECTED" => Some("Hequr"),
            _ => None,
        }
    }

    /// On the site for anyone with its link: not taken off it by its reports or the moderators,
    /// nor down for a while by its seller, nor expired (expiry.rs).
    pub fn on_site(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED" | "SOLD")
    }

    /// Of a kind that can be sold or reserved (db::SELLABLE_KINDS): one on sale or for rent.
    pub fn sellable(&self) -> bool {
        db::SELLABLE_KINDS.contains(&self.listing_type.as_str())
    }

    /// Its seller's button that says it is sold: "U shit", or "U dha me qira" for a rental.
    pub fn sell_action(&self) -> &'static str {
        format::sell_action(&self.listing_type)
    }

    /// Up or reserved and expiring within a week: "Skadon pas 3 ditësh", by its seller's buttons.
    pub fn expires_soon(&self) -> Option<String> {
        match self.status.as_str() {
            "ACTIVE" | "RESERVED" => format::expires_soon(self.expires_at?, time::OffsetDateTime::now_utc()),
            _ => None,
        }
    }

    /// Expired, or expiring within a week: its seller's "Rifresko" keeps it up db::LISTING_DAYS more.
    pub fn renewable(&self) -> bool {
        self.status == "EXPIRED" || self.expires_soon().is_some()
    }
}

#[derive(Template)]
#[template(path = "listing.html")]
struct ListingShow {
    listing: ListingDetail,
    /// Its category, as a category page's own pill is (browse.html): a link to that page.
    category: Subcategory,
    /// The category above its own, the pill before it: a way up. None for a main category.
    parent: Option<Subcategory>,
    /// Three levels down, the category above that one, which the round button before the pills
    /// leads to by its icon (no such button otherwise), as on a category page.
    above_parent: Option<Subcategory>,
    /// The listings either side of it on its category's page, the one before it and the one after
    /// it (db::listing_neighbours), for the round buttons at the right end of the path, after the
    /// one to the list; a button with nowhere to go stays, greyed.
    previous: Option<db::Neighbour>,
    next: Option<db::Neighbour>,
    details: Vec<Detail>,
    photos: Vec<Photo>,
    /// The reveal button's label, when the seller's phone number or e-mail can be shown.
    /// The button that shows how to reach the seller, when it can be shown.
    reveal: Option<Reveal>,
    /// The visitor posted this listing: the page offers to edit it.
    own: bool,
    /// For its seller, the promotions on it now (promotion_note.html).
    promotions: Vec<db::OnPromotion>,
    /// The visitor is a moderator: the page leads to giving it a promotion.
    moderator: bool,
    /// How many times its page was seen, "1 234", under the heart; None before any.
    views: Option<String>,
    /// Posted just now without signing in: the page says it is up, and how to change it later.
    posted: bool,
    /// For a property or a car, where it is, on a map under the seller.
    map: Option<ListingMap>,
    /// A property's or a car's page (MAPPED) has its photos across the page from 960 px up; every
    /// other listing's is split there, the photo at the left and the rest at its right
    /// (.listing-split, listing.css), its photos sized for that (SPLIT_GALLERY_SIZES).
    wide_photos: bool,
    /// A car's page (Makina), laid out as FINN's from 960 px up (.listing-car, listing.css): the
    /// photos across the page, and under them the title, the description and the details at the
    /// left, and the price, the ways to the seller and the seller's card in a
    /// column at the right (.listing-deal), its photos sized for the page (CAR_GALLERY_SIZES).
    car_page: bool,
    /// For a car up for sale at a price, how it compares with what cars of its model and year
    /// ask (car_price.html).
    car_price: Option<car_prices::CarPrice>,
    /// A car's key facts, in a row under its title: the year, the mileage, the gearbox and the
    /// fuel (CAR_FACTS), each with its icon; none on any other page.
    facts: Vec<Detail>,
    /// On a car's page, its options, the details with several values (a multi-select's), listed
    /// under their own names after the grid of the others; elsewhere among the details.
    options: Vec<Detail>,
    /// Listings like it, under it (db::related_listings): `page`, the name listing_card.html
    /// reads its cards by.
    page: ListingPage,
    meta: PageMeta,
    /// By how much its price dropped lately, and from what (price_drop): beside the price.
    price_drop: Option<(i64, String)>,
    /// The visitor is signed in, and so can block its seller.
    signed_in: bool,
    /// The visitor blocked its seller (safety.rs): the page says so, and leaves out their others.
    blocked: bool,
    /// "Numër i verifikuar" when the number it shows is one its seller signed in with.
    verified: Option<&'static str>,
    /// Why it isn't on the site, for its seller and the moderators, who alone see it then.
    hidden_note: Option<String>,
    /// The main category it is in, which the page's body names (base.html, data-section), as a
    /// category page's does: Elektronikë's listings are in Claude's blue and Shtëpi / kopsht's in
    /// its olive (base.css).
    section: Option<String>,
}

/// What a listing's page says about itself (seo.rs). Its preview: the title and the price, where
/// it is, what kind of listing and in what condition, the start of the description, and its first
/// photo. With a price, the price for the product:price tags, and on sale, a schema.org Product
/// and its Offer for Google: the price, the currency, the condition, whether it is still for sale,
/// and the seller. A sold listing is not for search engines to list, though its page stays for
/// those who have its link; a reserved one still is, as it may yet be for sale again; one taken off
/// the site (its seller and the moderators see it) is not.
fn listing_meta(origin: &str, listing: &ListingDetail, photos: &[Photo]) -> PageMeta {
    let price = format::price(listing.price_amount, &listing.currency, &listing.price_type, &listing.listing_type);
    let mut about: Vec<&str> = Vec::new();
    if let Some(place) = &listing.location_name {
        about.push(place);
    }
    about.push(format::listing_type(&listing.listing_type));
    if let Some(condition) = &listing.condition {
        about.push(format::condition(condition));
    }
    let about = about.join(" · ");
    let description = match seo::excerpt(&listing.description, 200usize.saturating_sub(about.chars().count() + 3)) {
        text if text.is_empty() => about,
        text => format!("{about} — {text}"),
    };
    let url = seo::absolute(origin, &listing.url());
    let mut meta = PageMeta::new(origin, format!("{} · {price}", listing.title), description, url.clone());
    if let Some(photo) = photos.first() {
        meta = meta.with_image(origin, &photo.url, listing.title.clone());
    }
    meta.noindex = !matches!(listing.status.as_str(), "ACTIVE" | "RESERVED");
    let priced = matches!(listing.price_type.as_str(), "FIXED" | "NEGOTIABLE" | "FROM");
    let Some(amount) = listing.price_amount.filter(|_| priced) else {
        return meta;
    };
    let amount = amount.normalize().to_string();
    meta.price = Some((amount.clone(), listing.currency.clone()));
    if listing.listing_type == "SELL" {
        meta.kind = "product";
        let availability = match listing.status.as_str() {
            "ACTIVE" => "InStock",
            "SOLD" => "SoldOut",
            "RESERVED" => "LimitedAvailability",
            _ => "OutOfStock",
        };
        let mut offer = serde_json::json!({
            "@type": "Offer",
            "price": amount,
            "priceCurrency": listing.currency,
            "availability": format!("https://schema.org/{availability}"),
            "url": url,
            "seller": {
                "@type": if listing.seller_is_business() { "Organization" } else { "Person" },
                "name": listing.seller_name,
            },
        });
        if let Some(condition) = listing.condition.as_deref() {
            let condition = match condition {
                "NEW" => "NewCondition",
                "FOR_PARTS" => "DamagedCondition",
                _ => "UsedCondition",
            };
            offer["itemCondition"] = format!("https://schema.org/{condition}").into();
        }
        let product = serde_json::json!({
            "@context": "https://schema.org",
            "@type": "Product",
            "name": listing.title,
            "description": seo::excerpt(&listing.description, 500),
            "image": photos.iter().map(|photo| seo::absolute(origin, &photo.url)).collect::<Vec<_>>(),
            "category": listing.category_name,
            "url": url,
            "offers": offer,
        });
        meta.json_ld = Some(seo::json_ld(&product));
    }
    meta
}

/// By how much a price dropped from `was` (db's price_was!), in whole percent, at least 1, and
/// `was` as the site writes a price: (7, "10 000 €").
pub fn price_drop(was: Option<Decimal>, price: Option<Decimal>, currency: &str) -> Option<(i64, String)> {
    let (was, price) = (was?, price?);
    let percent = ((was - price) / was * Decimal::ONE_HUNDRED).round().to_i64()?;
    Some((percent.max(1), format::money(was, currency)))
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
    // One its reports or the moderators took off the site is for its seller and the moderators.
    let user = current_user(&state, &headers).await?;
    // Its seller's: a private seller's, or a store's people who post (manages_listing).
    let own = match (&user, listing.seller_business_id) {
        (None, _) => false,
        (Some(user), None) => user.id == listing.created_by_user_id,
        (Some(user), Some(store)) => db::posts_for(&state.db, store, user.id).await?,
    };
    let moderator = user.as_ref().is_some_and(|user| state.moderators.include(user));
    // Posted just now (post.rs): whoever did, signed in or not, sees it while it waits for a
    // moderator.
    let just_posted = cookie(&headers, POSTED_COOKIE).is_some_and(|no| no == listing.public_no.to_string());
    let waiting = listing.status == "PENDING_REVIEW";
    if !listing.on_site() && !own && !moderator && !(waiting && just_posted) {
        return not_found().await;
    }
    let canonical = format!("{}-{}", listing.public_no, listing.slug);
    if key != canonical {
        return Ok(Redirect::permanent(&format!("/listing/{canonical}")).into_response());
    }
    let (attributes, photos, contact, categories, mut related, views, price_was, hidden) = tokio::try_join!(
        db::listing_attributes(&state.db, listing.id, listing.category_id),
        db::listing_photos(&state.db, listing.id),
        db::visible_contact(&state.db, listing.id),
        db::categories(&state.db),
        db::related_listings(&state.db, listing.id, db::RELATED + 1, 0),
        db::listing_views(&state.db, listing.id),
        db::price_was(&state.db, listing.id),
        safety::hidden_accounts(&state, &headers),
    )?;
    // What promotes it now, for its seller.
    let promotions = match own {
        true => db::listing_promotions(&state.db, &[listing.public_no]).await?,
        false => Vec::new(),
    };
    // Its neighbours on its category's page, among the listings the visitor sees there.
    let (previous, next) = db::listing_neighbours(&state.db, listing.id, listing.category_id, &hidden).await?;
    let price_drop = price_drop(price_was, listing.price_amount, &listing.currency);
    let views = (views > 0).then(|| format::number(Decimal::from(views)));
    let category = categories.by_slug(&listing.category_slug);
    let section = category.map(|category| categories.main_of(category).slug.clone());
    // Its category and the one above it as pills, as a category page's head has them, and the one
    // above that, where there is one, for the round button before them.
    let pill = |category: &db::Category| Subcategory {
        name: category.name.clone(),
        url: format!("/c/{}", category.slug),
        icon: category_icon(category),
    };
    let above = |of: &db::Category| of.parent_id.and_then(|id| categories.by_id(id));
    let parent_category = category.and_then(above);
    let parent = parent_category.map(pill);
    let above_parent = parent_category.and_then(above).map(pill);
    let category_pill = match category {
        Some(category) => pill(category),
        None => Subcategory { name: listing.category_name.clone(), url: format!("/c/{}", listing.category_slug), icon: None },
    };
    // Its category and those above it, the nearest first, whose own icon a property can take.
    let slugs: Vec<&str> = match category {
        Some(category) => std::iter::once(category)
            .chain(categories.ancestors(category).into_iter().rev())
            .map(|category| category.slug.as_str())
            .collect(),
        None => vec![listing.category_slug.as_str()],
    };
    let reveal = contact.map(|contact| match contact {
        SellerContact::Phone(_) => Reveal::Phone,
        SellerContact::Email(_) => Reveal::Email,
    });
    // Whether the contact it shows is verified; whether the visitor blocked its seller; the
    // listings of any seller they blocked go from the related ones.
    let verified = match (listing.contact_verified, &reveal) {
        (true, Some(Reveal::Phone)) => verified_label(Some("PHONE")),
        (true, Some(Reveal::Email)) => verified_label(Some("EMAIL")),
        _ => None,
    };
    let blocked = hidden.contains(&listing.created_by_user_id);
    let related_next = related_next(&mut related, listing.public_no, 0);
    safety::without_hidden(&mut related, &hidden);
    let hidden_note = match listing.status.as_str() {
        "PENDING_REVIEW" if own || just_posted => {
            Some("Njoftimi yt pret miratimin e një moderatori: askush tjetër nuk e sheh ende. Zakonisht brenda pak orësh.".to_string())
        }
        "PENDING_REVIEW" => Some("Ky njoftim pret miratimin e moderatorëve.".to_string()),
        "SUSPENDED" => Some("Ky njoftim nuk shfaqet në faqe: disa njerëz e raportuan, dhe moderatorët do ta shqyrtojnë.".to_string()),
        "REJECTED" => Some(match listing.rejection_reason.as_deref().filter(|_| own) {
            Some(reason) => format!("Moderatorët e hoqën këtë njoftim nga faqja: {reason}."),
            None => "Moderatorët e hoqën këtë njoftim nga faqja.".to_string(),
        }),
        "PAUSED" if own => Some("E ke çaktivizuar këtë njoftim: askush tjetër nuk e sheh. „Aktivizo“ e kthen në faqe aty ku ishte.".to_string()),
        "PAUSED" => Some("Shitësi e ka çaktivizuar këtë njoftim: askush tjetër nuk e sheh.".to_string()),
        "EXPIRED" if own => Some("Ky njoftim skadoi: askush tjetër nuk e sheh. „Rifresko“ e kthen në faqe për 60 ditë të tjera.".to_string()),
        "EXPIRED" => Some("Ky njoftim skadoi: askush tjetër nuk e sheh.".to_string()),
        _ => None,
    };
    // A car's title says its make, model and year: its details leave them out.
    let titled = listing.heading.is_some();
    let detail_of = |attribute: &db::AttributeValue| {
        let look = icons::look(&slugs, &attribute.code);
        Detail {
            icon: look.map(|look| look.icon),
            named: look.is_none_or(|look| look.reads == Reads::Named),
            multi: attribute.multi,
            key: attribute.key,
            label: attribute.label.clone(),
            values: attribute.shown.clone(),
        }
    };
    // A car's page (Makina) has its key facts in a row under the title, which leave the details,
    // and its options under their own names after the grid of the others (listing.html).
    let car_page = slugs.contains(&"makina");
    let car_price = match car_page {
        true => db::car_price(&state.db, listing.id)
            .await?
            .and_then(|car| car_prices::CarPrice::new(car, &listing.currency, listing.eur_rate)),
        false => None,
    };
    let facts: Vec<Detail> = if car_page {
        CAR_FACTS.iter().filter_map(|code| attributes.iter().find(|attribute| attribute.code == *code)).map(detail_of).collect()
    } else {
        Vec::new()
    };
    let mut details: Vec<Detail> = attributes
        .iter()
        .filter(|attribute| !(titled && properties::TITLE_CODES.contains(&attribute.code.as_str())))
        .filter(|attribute| !(car_page && CAR_FACTS.contains(&attribute.code.as_str())))
        .map(detail_of)
        .collect();
    // A property's zone in its town comes first among its details, lit as its cards say it too.
    if let Some(zone) = &listing.zone {
        let zone =
            Detail { label: "Zona".to_string(), values: vec![zone.clone()], icon: Some(icons::PIN), named: false, multi: false, key: true };
        details.insert(0, zone);
    }
    let (details, options): (Vec<Detail>, Vec<Detail>) =
        if car_page { details.into_iter().partition(|detail| !detail.multi) } else { (details, Vec::new()) };
    // A property or a car: its photos across the page, and where it is, on a map under the seller.
    let wide_photos = slugs.iter().any(|slug| MAPPED.contains(slug));
    let map = match (listing.latitude, listing.longitude) {
        (Some(latitude), Some(longitude)) if wide_photos => {
            Some(ListingMap { latitude, longitude, place: listing.location_name.clone().unwrap_or_default() })
        }
        _ => None,
    };
    // Said once: the cookie that says so (post.rs) goes with this answer.
    let posted = !own && just_posted && !waiting;
    let page = ListingPage::related(related, related_next);
    let meta = listing_meta(&seo::origin(&headers), &listing, &photos);
    let mut response =
        render(ListingShow {
            signed_in: user.is_some(),
            listing,
            category: category_pill,
            parent,
            above_parent,
            previous,
            next,
            details,
            photos,
            reveal,
            own,
            promotions,
            moderator,
            posted,
            map,
            wide_photos,
            car_page,
            car_price,
            facts,
            options,
            page,
            views,
            meta,
            price_drop,
            blocked,
            verified,
            hidden_note,
            section,
        })?;
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

/// One of a listing's details, as pills: its icon before its value (icons.rs), and its name too
/// where the value doesn't say what it is; a pill for each of several values (a car's options),
/// each with a check.
pub struct Detail {
    pub label: String,
    pub values: Vec<String>,
    pub icon: Option<&'static str>,
    /// Whether its name goes before its values: where it has no icon, or they don't read alone
    /// beside it ("Kati 6", icons::Reads).
    pub named: bool,
    pub multi: bool,
    /// One of the category's key facts, which its cards show too (show_in_card; a property's
    /// zone as well): its icon lit among the details, in a small round pill of its own.
    pub key: bool,
}

impl Detail {
    /// Whether its name goes before `value`: where it's named, unless the value says it, as
    /// "6 GB RAM" does RAM, or is it, as a box ticked is ("Pa aksident", db::listing_attributes).
    pub fn named_before(&self, value: &str) -> bool {
        self.named && value != self.label && !value.split_whitespace().any(|word| word.eq_ignore_ascii_case(&self.label))
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

#[derive(Template)]
#[template(path = "phone_stream.html")]
struct PhoneStream {
    reveal: Reveal,
    href: String,
    display: String,
    /// What the copy button copies: the number as a phone dials it, or the address.
    copy: String,
    /// For a number WhatsApp can reach, the address that opens a chat with it, a first message
    /// naming the listing already typed (whatsapp_url).
    whatsapp: Option<String>,
}

/// The address that opens a WhatsApp chat with `phone` (E.164), its first message typed in
/// ("Përshëndetje! Ju shkruaj për njoftimin „…“ në Pluribus: <its address>"), so that a seller
/// with many listings knows which one; the buyer can change it before sending. None for an
/// Albanian landline (+355 but not 6x, the mobiles), which has no WhatsApp; a number abroad may.
pub fn whatsapp_url(phone: &str, title: &str, url: &str) -> Option<String> {
    let digits = phone.strip_prefix('+')?;
    if digits.starts_with("355") && !digits[3..].starts_with('6') {
        return None;
    }
    let text = format!("Përshëndetje! Ju shkruaj për njoftimin „{title}“ në Pluribus: {url}");
    Some(format!("https://wa.me/{digits}?text={}", percent_encoded(&text)))
}

/// `text` percent-encoded byte by byte, all but the characters a URL never needs to escape: a
/// space is %20 rather than a form's "+", which WhatsApp would show as it is.
pub(super) fn percent_encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
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

/// `related` holds up to RELATED + 1 listings like one, from the `offset`th; the extra one only
/// shows that more exist: the frame that fetches them, from where these leave off, or None.
fn related_next(related: &mut Vec<ListingCard>, public_no: i64, offset: i64) -> Option<NextPage> {
    let more = related.len() as i64 > db::RELATED;
    related.truncate(db::RELATED as usize);
    more.then(|| {
        let from = offset + db::RELATED;
        NextPage { frame_id: format!("page-related-{from}"), url: format!("/listing/{public_no}/te-ngjashme?nga={from}") }
    })
}

/// The next listings like one, from the `nga`th (RELATED of them, and the frame for the next),
/// as its page's last come into view: the infinite scroll of "Njoftime të ngjashme", as the
/// browse pages'. Asked for as a page rather than a frame (its link, without scripts), it leads
/// back to the listing.
pub async fn related(
    State(state): State<AppState>,
    Path(public_no): Path<i64>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Result<Response> {
    if !is_next_page_request(&headers) {
        return Ok(Redirect::to(&format!("/listing/{public_no}")).into_response());
    }
    let Some(listing) = db::listing_by_public_no(&state.db, public_no).await? else {
        return not_found().await;
    };
    if !listing.on_site() {
        return not_found().await;
    }
    let offset = query
        .as_deref()
        .into_iter()
        .flat_map(|query| query.split('&'))
        .find_map(|pair| pair.strip_prefix("nga="))
        .and_then(|from| from.parse::<i64>().ok())
        .filter(|from| *from >= 0)
        .unwrap_or(0);
    let (mut related, hidden) = tokio::try_join!(
        db::related_listings(&state.db, listing.id, db::RELATED + 1, offset),
        safety::hidden_accounts(&state, &headers),
    )?;
    let next = related_next(&mut related, public_no, offset);
    safety::without_hidden(&mut related, &hidden);
    browse::frame(format!("page-related-{offset}"), &ListingPage::related(related, next))
}

/// Swaps the button for the seller's number (a tap calls it) or e-mail address; beside a number
/// WhatsApp can reach, a button that opens a chat with it about the listing.
pub async fn reveal_phone(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
) -> Result<Response> {
    let Some(revealed) = db::reveal_contact(&state.db, public_no).await? else {
        return not_found().await;
    };
    let url = format!("{}/listing/{public_no}-{}", seo::origin(&headers), revealed.slug);
    let (reveal, href, display, copy, whatsapp) = match revealed.contact {
        SellerContact::Phone(phone) => {
            let whatsapp = whatsapp_url(&phone, &revealed.title, &url);
            (Reveal::Phone, format!("tel:{phone}"), format::phone(&phone), phone, whatsapp)
        }
        SellerContact::Email(email) => (Reveal::Email, format!("mailto:{email}"), email.clone(), email, None),
    };
    render_stream(PhoneStream { reveal, href, display, copy, whatsapp })
}

#[cfg(test)]
mod tests {
    use super::whatsapp_url;

    #[test]
    fn whatsapp_reaches_mobiles_and_numbers_abroad_but_not_albanian_landlines() {
        let url = whatsapp_url("+355691234567", "Golf 7", "https://pluribus.al/listing/1-golf-7").unwrap();
        assert_eq!(
            url,
            "https://wa.me/355691234567?text=P%C3%ABrsh%C3%ABndetje%21%20Ju%20shkruaj%20p%C3%ABr%20njoftimin%20\
             %E2%80%9EGolf%207%E2%80%9C%20n%C3%AB%20Pluribus%3A%20https%3A%2F%2Fpluribus.al%2Flisting%2F1-golf-7"
        );
        assert!(whatsapp_url("+393331234567", "Golf", "/listing/1").is_some_and(|url| url.starts_with("https://wa.me/393331234567?")));
        assert_eq!(whatsapp_url("+35542234567", "Golf", "/listing/1"), None);
        assert_eq!(whatsapp_url("0691234567", "Golf", "/listing/1"), None);
    }
}
