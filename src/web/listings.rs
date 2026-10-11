//! A listing's page, its phone number reveal, and its views.

use std::{borrow::Cow, cmp::Ordering};

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
    i18n::{t, t_value, t1, t2},
    media::{CARD_SIDE, FULL_SIDE},
};

/// How wide a listing's photos show, for the browser to pick a copy of each (Photo::webp_srcset):
/// for every listing but a car's or a property's, whose photo stands at the left of its page from
/// 960 px up,
/// seven twelfths of the page, which is 60rem at most there, less the 2rem between the columns
/// (.listing-split, listing.css).
pub const SPLIT_GALLERY_SIZES: &str =
    "(min-width: 992px) 541px, (min-width: 960px) calc((100vw - 4rem) * .5833), (min-width: 792px) 760px, calc(100vw - 2rem)";
/// The same for a car's page and a property's, whose slider is the mosaic's first two thirds from
/// 960 px up, the
/// page 60rem at most and the rail beside it 88 px from 720 px up, the gutters 1rem (.listing-car,
/// listing.css); the page's 760 px column under 960.
pub const CAR_GALLERY_SIZES: &str = "(min-width: 1080px) 635px, (min-width: 960px) calc((100vw - 128px) * .6667), (min-width: 880px) 760px, (min-width: 720px) calc(100vw - 120px), calc(100vw - 2rem)";

/// The down payment the loan calculator takes for given, as a percentage of the price
/// (loan_calculator.html, loan_controller.js): what Albania's banks ask for a used car, which
/// they finance 60 to 65% of (Intesa Sanpaolo 65%, Tirana Bank 60%, in 2026; a new car 70 to
/// 75%, a lease 80%). The loan is the rest of the price.
pub const LOAN_DOWN_PERCENT: u32 = 35;

/// A car's key facts, in a row under its title (listing.html, .key-facts), in this order: the
/// year, the mileage, the gearbox and the fuel, as FINN has them. They leave the details.
const CAR_FACTS: [&str; 4] = ["vehicle_year", "vehicle_mileage", "vehicle_transmission", "vehicle_fuel"];

/// A property's key facts, as the canvas has them: its rooms, its area, its floor and whether it is
/// furnished.
const PROPERTY_FACTS: [&str; 4] = ["property_rooms", "property_area", "property_floor", "property_furnished"];

/// The categories whose listings' pages show where they are on a map, under the seller: property
/// (Prona and those under it) and cars (Makina), which buyers go to see.
pub(super) const MAPPED: [&str; 2] = ["prona", "makina"];

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
    /// whole (CAR_GALLERY_SIZES, SPLIT_GALLERY_SIZES): each by its longer side.
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

/// What a loan for it would be, where its page offers the loan calculator: something on sale at
/// a set price, a negotiable one or one from which, of more than 500 €, or 50 000 Lekë.
fn loan_amount(listing_type: &str, price_type: &str, currency: &str, price_amount: Option<Decimal>) -> Option<Decimal> {
    let priced = matches!(price_type, "FIXED" | "NEGOTIABLE" | "FROM");
    let enough = match currency {
        "EUR" => Decimal::from(500),
        "ALL" => Decimal::from(50_000),
        _ => return None,
    };
    price_amount.filter(|amount| priced && listing_type == "SELL" && *amount > enough)
}

impl ListingCard {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// Whether its page offers the loan calculator, a button at the price's line's end, which the
    /// stand-in a tap shows keeps a place for (data-loan, application.js).
    pub fn loan(&self) -> bool {
        loan_amount(&self.listing_type, &self.price_type, &self.currency, self.price_amount).is_some()
    }

    /// Whether its page has a circle after the loan calculator's that weighs its price: what a
    /// square metre of it costs (ListingDetail::area_price: on sale at a price, with an area), or
    /// how it compares with what cars of its model and year ask (car_prices::CarPrice: a verdict),
    /// which the stand-in a tap shows keeps a place for too (data-compare).
    /// The line under its page's title, in words, as the page says it (listing.html, .listing-kind):
    /// on a car's or a property's page the title made from what the car is first where the
    /// seller's heads the page, in what state, where, when and its number; on the others in what
    /// state and its number (what kind, "Në shitje", stands under the price since 2026-10-12, as
    /// the user asked). The stand-in a tap shows says it, unseen, so that it wraps as the page's
    /// does (data-line).
    pub fn page_line(&self) -> String {
        let condition = self.condition.as_deref().map(|condition| format::condition(condition).to_string());
        let mut parts: Vec<String> = Vec::new();
        if !self.deal {
            parts.extend(condition);
            parts.push(format!("{}: {}", t("Numri i njoftimit"), self.public_no));
            return parts.join(" · ");
        }
        if self.heading.is_some() {
            parts.push(self.title.clone());
        }
        parts.extend(condition);
        // Where, as the page's query says it (db::listing): a zone with its town after it, or the
        // place alone.
        let place = match (&self.zone, &self.city) {
            (Some(zone), Some(city)) => Some(format!("{zone}, {city}")),
            (Some(_), None) => None,
            (None, _) => self.location_name.clone(),
        };
        parts.extend(place);
        parts.extend(self.published_at.map(|published| format::when(published, time::OffsetDateTime::now_utc())));
        parts.push(t1("Nr. {}", self.public_no));
        parts.join(" · ")
    }

    /// What kind of listing it is, as its page says it under its price ("Në shitje"), for the
    /// stand-in a tap shows (data-kind).
    pub fn kind_label(&self) -> &str {
        format::listing_type(&self.listing_type)
    }

    pub fn compares(&self) -> bool {
        let priced = matches!(self.price_type.as_str(), "FIXED" | "NEGOTIABLE" | "FROM");
        let area = self.listing_type == "SELL" && priced && self.price_amount.is_some() && self.area_m2.is_some_and(|area| area > Decimal::ZERO);
        let car = match (self.price_normalized, self.model_median) {
            (Some(price), Some(median)) => price > Decimal::ZERO && car_prices::verdict(price, median).is_some(),
            _ => false,
        };
        area || car
    }

    /// "1 234": how many times its page was seen this week, for "Më të klikuarat".
    pub fn views_display(&self) -> Option<String> {
        self.views.map(|views| format::number(Decimal::from(views)))
    }

    /// Over the photo of a kept listing that is no longer simply up (/te-ruajturat alone lists
    /// them): "Rezervuar", or "Shitur" ("Dhënë me qira" for a rental).
    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "RESERVED" => Some(t("Rezervuar")),
            "SOLD" => Some(format::sold(&self.listing_type)),
            _ => None,
        }
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// The price itself; what follows it, a size down on the card, is price_note.
    pub fn price_main(&self) -> String {
        format::price_parts(self.price_amount, &self.currency, &self.price_type, &self.listing_type).0
    }

    /// "/muaj", " (i diskutueshëm)", or empty.
    pub fn price_note(&self) -> String {
        format::price_parts(self.price_amount, &self.currency, &self.price_type, &self.listing_type).1
    }

    /// What follows the price, in two: its month ("/muaj", a rent's or a salary's), which every card
    /// keeps, and "(i diskutueshëm)", which a narrow card's photo leaves out (page.css); each empty
    /// when there is none.
    pub fn price_note_parts(&self) -> (String, String) {
        let note = self.price_note();
        let negotiable = crate::t("(i diskutueshëm)");
        match note.strip_suffix(negotiable) {
            Some(period) => (period.trim_end().to_string(), negotiable.to_string()),
            None => (note, String::new()),
        }
    }

    /// What the stand-in a tap shows draws its page's description from (data-description): all
    /// of it, its lines as they are, or, when it is longer than db::PAGE_DESCRIPTION, that much,
    /// cut at a word, more than the dozen lines the page shows of it folded (description.html).
    pub fn stand_in_description(&self) -> String {
        if self.page_description.chars().count() <= db::PAGE_DESCRIPTION {
            return self.page_description.clone();
        }
        let cut: String = self.page_description.chars().take(db::PAGE_DESCRIPTION).collect();
        match cut.rfind(char::is_whitespace) {
            Some(at) => cut[..at].trim_end().to_string(),
            None => cut,
        }
    }

    /// Whether its page offers a message to its seller (contact_bar.html: a listing up or reserved),
    /// for the stand-in's contact bar (data-closed when it doesn't).
    pub fn messages(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED" | "")
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
        t1("Përdorues që nga {}", format::month_year(self.seller_since))
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
    /// The properties as the database words them, in the language now (i18n::t_value).
    pub fn details_line(&self) -> String {
        let distance = self.distance_m.map(format::distance);
        let spare = self.spare_detail.as_deref().filter(|_| self.city_known).map(|spare| t_value(spare).into_owned());
        let details = self.details.iter().map(|detail| t_value(detail).into_owned());
        let parts: Vec<String> = distance.into_iter().chain(spare).chain(details).collect();
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

    /// What qualifies the price on the line under it, after what kind of listing it is:
    /// "i diskutueshëm", without its brackets, or nothing. Its month ("/muaj"), which the cards
    /// keep, the page leaves out, as the user asked on 2026-10-12 ("remove the / month marking").
    pub fn price_negotiable(&self) -> String {
        let negotiable = t("(i diskutueshëm)");
        if self.price_note().contains(negotiable) { negotiable.replace(['(', ')'], "") } else { String::new() }
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
        // A starting price says so before each: "nga 925 €/m²".
        let from = |text: String| if self.price_type == "FROM" { t1("nga {}", text) } else { text };
        Some(AreaPrice {
            per_area: from(format!("{}/m²", format::money(per_area, &self.currency))),
            price: from(format::money(amount, &self.currency)),
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
            eur if eur <= -margin => ("price-below", t("Nën mesataren")),
            eur if eur >= margin => ("price-above", t("Mbi mesataren")),
            _ if difference.is_zero() => ("price-near", t("Njësoj si mesatarja")),
            _ => ("price-near", t("Afër mesatares")),
        };
        let gap = format::money(difference.abs(), &self.currency);
        let difference = match difference.cmp(&Decimal::ZERO) {
            Ordering::Less => Some(t1("{}/m² më lirë", gap)),
            Ordering::Greater => Some(t1("{}/m² më shtrenjtë", gap)),
            Ordering::Equal => None,
        };
        Some(ZonePrice {
            zone: zone.clone(),
            category: t(&self.category_name).to_string(),
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
        loan_amount(&self.listing_type, &self.price_type, &self.currency, self.price_amount)
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

    /// Under the seller's name (seller_card.html), since when they are on the site, as the canvas
    /// words it: "Që nga tetor 2026", or for a business "Biznes · që nga mars 2026". (Every page
    /// but a car's said "Në Pluribus që nga tetor 2026 · 12 njoftime" once.)
    pub fn seller_since_line(&self) -> String {
        let since = format::month_year(self.seller_since);
        if self.seller_is_business() { t1("Biznes · që nga {}", since) } else { t1("Që nga {}", since) }
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
    /// the date; format::when), with what screen readers hear before it: "Publikuar" for the
    /// words, "Publikuar më" for a date, which the page puts a space after.
    pub fn published_display(&self) -> Option<(&'static str, String)> {
        let published = self.published_at?;
        let when = format::when(published, time::OffsetDateTime::now_utc());
        let prefix = if when.starts_with(|c: char| c.is_ascii_digit()) { t("Publikuar më") } else { t("Publikuar") };
        Some((prefix, when))
    }

    /// What kind of listing it is, on the line under its price, in the accent's colour: "Në
    /// shitje", "Me qira" (the user's ask of 2026-10-12; it headed the line under the title).
    pub fn kind_label(&self) -> &str {
        format::listing_type(&self.listing_type)
    }

    /// In what condition, at the head of the line under its title, when it has one: "E përdorur".
    pub fn condition_label(&self) -> Option<&str> {
        self.condition.as_deref().map(format::condition)
    }

    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "RESERVED" => Some(t("Rezervuar")),
            "SOLD" => Some(format::sold(&self.listing_type)),
            "PAUSED" => Some(t("Çaktivizuar")),
            "EXPIRED" => Some(t("Skaduar")),
            "SUSPENDED" => Some(t("Në shqyrtim")),
            "REJECTED" => Some(t("Hequr")),
            _ => None,
        }
    }

    /// The tone its badge takes (.badge.warn, .badge.danger in listing.css), as the canvas has
    /// them: a reservation in the warm yellow, a listing expired or taken off in red's tint; the
    /// rest grey.
    pub fn status_tone(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "RESERVED" => Some("warn"),
            "EXPIRED" | "SUSPENDED" | "REJECTED" => Some("danger"),
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
    /// The category above its own, before it in the breadcrumb: a way up. None for a main
    /// category, whose breadcrumb's way up is the welcome page.
    parent: Option<Subcategory>,
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
    /// Posted just now by its seller, signed in, and up: the page says so.
    just_published: bool,
    /// For a listing just posted and up, a WhatsApp message to send anyone, with its title and
    /// address (share_note.html).
    whatsapp_share: String,
    /// For a property or a car, where it is, on a map under the seller.
    map: Option<ListingMap>,
    /// A car's page or a property's (MAPPED), laid out as the canvas has a car's from 960 px up
    /// (.listing-car, listing.css): the photos a mosaic, and under them the title, the key facts,
    /// the description and the details at the left, and the price, the seller and the ways to reach
    /// them in a column at the right (.listing-deal), its photos sized for that
    /// (CAR_GALLERY_SIZES). Every other listing's is split there, the photo at the left and the
    /// rest at its right (.listing-split), its photos sized for that (SPLIT_GALLERY_SIZES).
    deal_page: bool,
    /// A car's page (Makina): its title as its seller wrote it, the one made from what the car is
    /// over the line under it, and how its price compares with its model's.
    car_page: bool,
    /// For a car up for sale at a price, how it compares with what cars of its model and year
    /// ask (car_price.html).
    car_price: Option<car_prices::CarPrice>,
    /// A car's or a property's key facts, in a row under its title: a car's year, mileage, gearbox
    /// and fuel (CAR_FACTS), a property's rooms, area, floor and furnishing (PROPERTY_FACTS), each
    /// with its icon; none on any other page.
    facts: Vec<Detail>,
    /// On a car's or a property's page, its options, the details with several values (a
    /// multi-select's: a car's equipment, a property's amenities), listed under their own names
    /// after the grid of the others; elsewhere among the details.
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
    // Its category and the one above it as pills, as a category page's head has them, and the one
    // above that, where there is one, for the round button before them.
    let pill = |category: &db::Category| Subcategory {
        name: category.name.clone(),
        url: format!("/c/{}", category.slug),
        icon: category_icon(category),
    };
    let above = |of: &db::Category| of.parent_id.and_then(|id| categories.by_id(id));
    let parent = category.and_then(above).map(pill);
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
            Some(t("Njoftimi yt pret miratimin e një moderatori: askush tjetër nuk e sheh ende. Zakonisht brenda pak orësh.").to_string())
        }
        "PENDING_REVIEW" => Some(t("Ky njoftim pret miratimin e moderatorëve.").to_string()),
        // Off the site with its seller's account or store, which a moderator suspended
        // (db::suspend_user), or for its reports.
        "SUSPENDED" if db::seller_suspended(&state.db, listing.public_no).await? => {
            Some(t("Ky njoftim nuk shfaqet në faqe: llogaria e shitësit është pezulluar.").to_string())
        }
        "SUSPENDED" => Some(t("Ky njoftim nuk shfaqet në faqe: disa njerëz e raportuan, dhe moderatorët do ta shqyrtojnë.").to_string()),
        "REJECTED" => Some(match listing.rejection_reason.as_deref().filter(|_| own) {
            Some(reason) => t1("Moderatorët e hoqën këtë njoftim nga faqja: {}.", reason),
            None => t("Moderatorët e hoqën këtë njoftim nga faqja.").to_string(),
        }),
        "PAUSED" if own => Some(t("E ke çaktivizuar këtë njoftim: askush tjetër nuk e sheh. „Aktivizo“ e kthen në faqe aty ku ishte.").to_string()),
        "PAUSED" => Some(t("Shitësi e ka çaktivizuar këtë njoftim: askush tjetër nuk e sheh.").to_string()),
        "EXPIRED" if own => Some(t("Ky njoftim skadoi: askush tjetër nuk e sheh. „Rifresko“ e kthen në faqe për 60 ditë të tjera.").to_string()),
        "EXPIRED" => Some(t("Ky njoftim skadoi: askush tjetër nuk e sheh.").to_string()),
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
            place: false,
            label: attribute.label.clone(),
            values: attribute.shown.clone(),
        }
    };
    // A car's page (Makina) and a property's (MAPPED) have their key facts in a row under the title,
    // which leave the details, and their options under their own names after the grid of the
    // others (listing.html); and their photos a mosaic, and where it is, on a map under the seller.
    let car_page = slugs.contains(&"makina");
    let deal_page = slugs.iter().any(|slug| MAPPED.contains(slug));
    let fact_codes: &[&str] = if car_page {
        &CAR_FACTS
    } else if deal_page {
        &PROPERTY_FACTS
    } else {
        &[]
    };
    let car_price = match car_page {
        true => db::car_price(&state.db, listing.id)
            .await?
            .and_then(|car| car_prices::CarPrice::new(car, &listing.currency, listing.eur_rate)),
        false => None,
    };
    let facts: Vec<Detail> =
        fact_codes.iter().filter_map(|code| attributes.iter().find(|attribute| attribute.code == *code)).map(detail_of).collect();
    let mut details: Vec<Detail> = attributes
        .iter()
        .filter(|attribute| !(titled && properties::TITLE_CODES.contains(&attribute.code.as_str())))
        .filter(|attribute| !fact_codes.contains(&attribute.code.as_str()))
        .map(detail_of)
        .collect();
    // A property's zone in its town comes first among its details, lit as its cards say it too:
    // "Zona" (the page says it in its language), and the zone's name, as it is.
    if let Some(zone) = &listing.zone {
        let zone = Detail {
            label: "Zona".to_string(),
            values: vec![zone.clone()],
            icon: Some(icons::PIN),
            named: false,
            multi: false,
            key: true,
            place: true,
        };
        details.insert(0, zone);
    }
    let (details, options): (Vec<Detail>, Vec<Detail>) =
        if deal_page { details.into_iter().partition(|detail| !detail.multi) } else { (details, Vec::new()) };
    let map = match (listing.latitude, listing.longitude) {
        (Some(latitude), Some(longitude)) if deal_page => {
            Some(ListingMap { latitude, longitude, place: listing.location_name.clone().unwrap_or_default() })
        }
        _ => None,
    };
    // Said once: the cookie that says so (post.rs) goes with this answer.
    let posted = !own && just_posted && !waiting;
    let just_published = own && just_posted && listing.status == "ACTIVE";
    let whatsapp_share = format!("https://wa.me/?text={}", percent_encoded(&format!("{} {}{}", listing.title, seo::origin(&headers), listing.url())));
    let page = ListingPage::related(related, related_next);
    let meta = listing_meta(&seo::origin(&headers), &listing, &photos);
    let mut response =
        render(ListingShow {
            signed_in: user.is_some(),
            listing,
            category: category_pill,
            parent,
            previous,
            next,
            details,
            photos,
            reveal,
            own,
            promotions,
            moderator,
            posted,
            just_published,
            whatsapp_share,
            map,
            deal_page,
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
        })?;
    if posted || just_published {
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
    /// The zone's name, "Blloku", and the listing's category's, "Apartamente", in the page's
    /// language.
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
    /// Its value is a place's name (a property's zone), which stays as it is in every language.
    pub place: bool,
}

impl Detail {
    /// Whether its name goes before `value`: where it's named, unless the value says it, as
    /// "6 GB RAM" does RAM, or is it, as a box ticked is ("Pa aksident", db::listing_attributes).
    /// Both as the database words them: the page says them in its language (crate::t, shown).
    pub fn named_before(&self, value: &str) -> bool {
        self.named && value != self.label && !value.split_whitespace().any(|word| word.eq_ignore_ascii_case(&self.label))
    }

    /// `value` as the page shows it, in the language now: a choice's label, "Jo", a number and its
    /// unit, a date (i18n::t_value); a place's name as it is.
    pub fn shown<'a>(&self, value: &'a str) -> Cow<'a, str> {
        if self.place { Cow::Borrowed(value) } else { t_value(value) }
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
            Reveal::Phone => t("Shfaq numrin"),
            Reveal::Email => t("Shfaq email-in"),
        }
    }

    /// The copy button's name, beside the number or address once shown.
    pub fn copy_label(&self) -> &'static str {
        match self {
            Reveal::Phone => t("Kopjo numrin"),
            Reveal::Email => t("Kopjo email-in"),
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
/// with many listings knows which one; the buyer can change it before sending. In the buyer's
/// language: they write it. None for an Albanian landline (+355 but not 6x, the mobiles), which
/// has no WhatsApp; a number abroad may.
pub fn whatsapp_url(phone: &str, title: &str, url: &str) -> Option<String> {
    let digits = phone.strip_prefix('+')?;
    if digits.starts_with("355") && !digits[3..].starts_with('6') {
        return None;
    }
    let text = t2("Përshëndetje! Ju shkruaj për njoftimin „{}“ në Pluribus: {}", title, url);
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
