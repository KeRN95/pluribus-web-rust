//! Promotions as the pages show them (db/promotions.rs): /promovo, what each on sale does and
//! costs, and how to get one (by writing to CONTACT_EMAIL, until there are payments), which the
//! row of featured listings and the seller's own pages lead to; what each kind does, in words,
//! for that page and /moderim, where moderators give them.

use askama::Template;
use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::Response,
};
use serde::Deserialize;

use super::{Result, render, seo::{self, PageMeta}};
use crate::{AppState, db, format};

/// What a kind of promotion does, as the pages say it.
pub fn does(promotion_type: &str) -> &'static str {
    match promotion_type {
        "FEATURED" => "Del te „Të veçuara“, rreshti mbi njoftimet e kategorisë së tij dhe të kërkimeve ku përputhet, me radhë me të tjerët.",
        "HIGHLIGHT" => "Karta e tij dallohet me një sfond të verdhë të lehtë, kudo që shfaqet.",
        "BUMP" => "Ngjitet menjëherë në krye të njoftimeve më të reja, një herë.",
        "CATEGORY_TOP" => "Ngjitet në krye të njoftimeve më të reja çdo ditë, sa zgjat.",
        _ => "",
    }
}

impl db::PromotionProduct {
    pub fn does(&self) -> &'static str {
        does(&self.promotion_type)
    }

    /// "8,99 €".
    pub fn price(&self) -> String {
        format::money(self.price_amount, &self.currency)
    }
}

impl db::OnPromotion {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// The day it ends: "17.10.2026".
    pub fn until(&self) -> String {
        format::date(self.ends_at)
    }
}

#[derive(Template)]
#[template(path = "promote.html")]
struct PromotePage {
    meta: PageMeta,
    products: Vec<db::PromotionProduct>,
    /// The listing it is for, when a seller's page led here ("Promovoje", njoftimi=1234).
    number: Option<i64>,
    /// CONTACT_EMAIL in .env, when it is set.
    email: Option<String>,
}

impl PromotePage {
    /// The e-mail's subject, ready for a mailto link: the listing's number in it, when known.
    pub fn subject(&self) -> String {
        let subject = match self.number {
            Some(number) => format!("Promovim për njoftimin {number}"),
            None => "Promovim".to_string(),
        };
        form_urlencoded::byte_serialize(subject.as_bytes()).collect::<String>().replace('+', "%20")
    }
}

#[derive(Deserialize)]
pub struct ForListing {
    njoftimi: Option<String>,
}

/// GET /promovo: the promotions on sale, what each does and costs, and how to get one.
pub async fn page(State(state): State<AppState>, headers: HeaderMap, Query(query): Query<ForListing>) -> Result<Response> {
    let products = db::promotion_products(&state.db).await?;
    let origin = seo::origin(&headers);
    let meta = PageMeta::new(
        &origin,
        "Promovo një njoftim",
        "Si shihet më shumë një njoftim në Pluribus: në rreshtin „Të veçuara“, në krye të më të rejave, ose me një kartë që dallohet.",
        seo::canonical(&origin, "/promovo", None),
    );
    let number = query.njoftimi.and_then(|number| number.trim().parse::<i64>().ok()).filter(|number| *number > 0);
    let email = std::env::var("CONTACT_EMAIL").ok().map(|email| email.trim().to_string()).filter(|email| !email.is_empty());
    render(PromotePage { meta, products, number, email })
}
