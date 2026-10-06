//! A listing's page and its phone number reveal.

use askama::Template;
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect, Response},
};
use rust_decimal::Decimal;

use super::{Result, account::current_user, not_found, render, render_stream};
use crate::{
    AppState,
    db::{self, AttributeValue, ListingCard, ListingDetail, MapPoint, Photo, SellerContact},
    format,
};

impl ListingCard {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// The city at the start of the details line: the listing's municipality, or its location if
    /// that isn't in one. None when the search is in that city already.
    pub fn city_label(&self) -> Option<&str> {
        if self.city_known {
            return None;
        }
        self.city.as_deref().or(self.location_name.as_deref())
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

impl ListingDetail {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
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

    /// "Në Marketplace që nga tetor 2026 · 12 njoftime".
    pub fn seller_summary(&self) -> String {
        format!(
            "Në Marketplace që nga {} · {}",
            format::month_year(self.seller_since),
            format::listing_count(self.seller_listings, i64::MAX)
        )
    }

    pub fn published_display(&self) -> Option<String> {
        self.published_at.map(format::date)
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
    attributes: Vec<AttributeValue>,
    photos: Vec<Photo>,
    /// The reveal button's label, when the seller's phone number or e-mail can be shown.
    reveal_label: Option<&'static str>,
    /// The visitor posted this listing: the page offers to edit it.
    own: bool,
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
    let (mut attributes, photos, contact, categories) = tokio::try_join!(
        db::listing_attributes(&state.db, listing.id, listing.category_id),
        db::listing_photos(&state.db, listing.id),
        db::visible_contact(&state.db, listing.id),
        db::categories(&state.db),
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
    let reveal_label = contact.map(|contact| match contact {
        SellerContact::Phone(_) => "Shfaq numrin",
        SellerContact::Email(_) => "Shfaq email-in",
    });
    if let Some(condition) = &listing.condition {
        let value = format::condition(condition).to_string();
        attributes.insert(0, AttributeValue { label: "Gjendja".to_string(), value });
    }
    render(ListingShow { listing, trail, attributes, photos, reveal_label, own })
}

/// A category on the way to a listing's own.
pub struct Crumb {
    pub name: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "phone_stream.html")]
struct PhoneStream {
    href: String,
    display: String,
}

/// Swaps the button for the seller's number (a tap calls it) or e-mail address.
pub async fn reveal_phone(
    State(state): State<AppState>,
    Path(public_no): Path<i64>,
) -> Result<Response> {
    let Some(contact) = db::reveal_contact(&state.db, public_no).await? else {
        return not_found().await;
    };
    let (href, display) = match contact {
        SellerContact::Phone(phone) => (format!("tel:{phone}"), format::phone(&phone)),
        SellerContact::Email(email) => (format!("mailto:{email}"), email),
    };
    render_stream(PhoneStream { href, display })
}
