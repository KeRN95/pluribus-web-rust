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
use crate::{
    AppState, db, format,
    i18n::{t, t1},
};

/// What a kind of promotion does, as the pages say it, in their language.
pub fn does(promotion_type: &str) -> &'static str {
    match promotion_type {
        "FEATURED" => t("Del mbi njoftimet e kategorisë së tij dhe të kërkimeve ku përputhet, me çmimin e shënuar me ngjyrë, me radhë me të tjerët."),
        "HIGHLIGHT" => t("Karta e tij dallohet me një sfond të verdhë të lehtë, kudo që shfaqet."),
        "BUMP" => t("Ngjitet menjëherë në krye të njoftimeve më të reja, një herë."),
        "CATEGORY_TOP" => t("Ngjitet në krye të njoftimeve më të reja çdo ditë, sa zgjat."),
        _ => "",
    }
}

/// What the pages say of a promotion on sale: `PromotionProduct`'s own methods in all but name, a
/// trait because the type is pluribus-db's (each method's doc is on its body, below).
pub trait PromotionProductExt {
    fn does(&self) -> &'static str;
    fn price(&self) -> String;
}

impl PromotionProductExt for db::PromotionProduct {
    fn does(&self) -> &'static str {
        does(&self.promotion_type)
    }

    /// "8,99 €".
    fn price(&self) -> String {
        format::money(self.price_amount, &self.currency)
    }
}

/// What the pages say of a promotion on a listing: `OnPromotion`'s own methods in all but name, a
/// trait because the type is pluribus-db's (each method's doc is on its body, below).
pub trait OnPromotionExt {
    fn url(&self) -> String;
    fn until(&self) -> String;
}

impl OnPromotionExt for db::OnPromotion {
    fn url(&self) -> String {
        format::listing_path(self.public_no, &self.slug)
    }

    /// The day it ends: "17.10.2026".
    fn until(&self) -> String {
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
    /// CONTACT_EMAIL in the settings, when it is set.
    email: Option<String>,
}

impl PromotePage {
    /// The e-mail's subject, ready for a mailto link: the listing's number in it, when known.
    pub fn subject(&self) -> String {
        let subject = match self.number {
            Some(number) => t1("Promovim për njoftimin {}", number),
            None => t("Promovim").to_string(),
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
    // The title for the previews (og:title) in Albanian, as the apps that fetch them see it; the
    // description, the page's own, in its language.
    let meta = PageMeta::new(
        &origin,
        "Promovo një njoftim",
        t("Si shihet më shumë një njoftim në Pluribus: mbi njoftimet e tjera me çmimin e shënuar me ngjyrë, në krye të më të rejave, ose me një kartë që dallohet."),
        seo::canonical(&origin, "/promovo", None),
    );
    let number = query.njoftimi.and_then(|number| number.trim().parse::<i64>().ok()).filter(|number| *number > 0);
    let email = state.config.contact_email.clone();
    render(PromotePage { meta, products, number, email })
}
