//! The pages about the site, which every page's footer links: how it works, the safety tips,
//! the terms of use, privacy and contact. Each is a column of text (text_page in page.css) with
//! its description and canonical address for search engines (seo.rs), listed in the pages'
//! sitemap. In English each says the same as a whole, from a template of its own
//! (how_it_works_en.html...), which its Albanian one includes.

use askama::Template;
use axum::{http::HeaderMap, response::Response};

use super::{
    Result, render,
    seo::{self, PageMeta},
};
use crate::i18n::t;

/// The pages, their paths and names, as the footer links them and the sitemap lists them.
pub const PAGES: [(&str, &str); 5] = [
    ("/si-funksionon", "Si funksionon"),
    ("/siguria", "Këshilla sigurie"),
    ("/kushtet", "Kushtet e përdorimit"),
    ("/privatesia", "Privatësia"),
    ("/kontakt", "Kontakt"),
];

/// When the terms and the privacy page last changed, as they say at their top: the day, the
/// month by its Albanian name (whose English format.txt has) and the year.
const TERMS_UPDATED: (u8, &str, u16) = (10, "tetor", 2026);
const PRIVACY_UPDATED: (u8, &str, u16) = (11, "tetor", 2026);

/// Such a date as the page says it: "10 tetor 2026", in English "10 October 2026".
fn said((day, month, year): (u8, &str, u16)) -> String {
    format!("{day} {} {year}", t(month))
}

/// A setting from .env, when it is set: CONTACT_EMAIL, the address the contact and privacy pages
/// give; OPERATOR, who runs the site (a person's name, or a company's), whom the privacy page names
/// as the one responsible for the data.
fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

/// A page's metadata: its title for the previews (og:title) in Albanian, as the apps that fetch
/// them, which never choose a language, see it; its description, which the handlers give in the
/// page's language.
fn meta(headers: &HeaderMap, path: &str, title: &str, description: &str) -> PageMeta {
    let origin = seo::origin(headers);
    PageMeta::new(&origin, title, description, seo::canonical(&origin, path, None))
}

#[derive(Template)]
#[template(path = "how_it_works.html")]
struct HowItWorks {
    meta: PageMeta,
}

pub async fn how_it_works(headers: HeaderMap) -> Result<Response> {
    render(HowItWorks {
        meta: meta(
            &headers,
            "/si-funksionon",
            "Si funksionon Pluribus",
            t("Si kërkon, ruan dhe ndjek njoftimet, si kontakton shitësin, dhe si shton një njoftim me numrin e celularit, pa fjalëkalim."),
        ),
    })
}

#[derive(Template)]
#[template(path = "safety_tips.html")]
struct SafetyTips {
    meta: PageMeta,
}

pub async fn safety_tips(headers: HeaderMap) -> Result<Response> {
    render(SafetyTips {
        meta: meta(
            &headers,
            "/siguria",
            "Këshilla sigurie",
            t("Si blen dhe shet pa rrezik: takohu në vend publik, shiko mallin para se të paguash, mos dërgo para paraprakisht, dhe çfarë të bësh kur diçka s'shkon."),
        ),
    })
}

#[derive(Template)]
#[template(path = "terms.html")]
struct Terms {
    meta: PageMeta,
    updated: String,
}

pub async fn terms(headers: HeaderMap) -> Result<Response> {
    render(Terms {
        meta: meta(
            &headers,
            "/kushtet",
            "Kushtet e përdorimit",
            t("Çfarë lejohet dhe çfarë jo në Pluribus: llogaria, njoftimet, mesazhet, moderimi dhe përgjegjësia."),
        ),
        updated: said(TERMS_UPDATED),
    })
}

#[derive(Template)]
#[template(path = "privacy.html")]
struct Privacy {
    meta: PageMeta,
    updated: String,
    /// Who runs the site (OPERATOR), and the address to write to (CONTACT_EMAIL), when they are set.
    operator: Option<String>,
    email: Option<String>,
}

pub async fn privacy(headers: HeaderMap) -> Result<Response> {
    render(Privacy {
        meta: meta(
            &headers,
            "/privatesia",
            "Privatësia",
            t("Cilat të dhëna mban Pluribus, pse, kush tjetër i merr, sa kohë, dhe si i sheh, i ndryshon ose i fshin."),
        ),
        updated: said(PRIVACY_UPDATED),
        operator: setting("OPERATOR"),
        email: setting("CONTACT_EMAIL"),
    })
}

#[derive(Template)]
#[template(path = "contact.html")]
struct Contact {
    meta: PageMeta,
    /// CONTACT_EMAIL in .env, when it is set.
    email: Option<String>,
}

pub async fn contact(headers: HeaderMap) -> Result<Response> {
    render(Contact {
        meta: meta(&headers, "/kontakt", "Kontakt", t("Si i shkruan Pluribus: për llogarinë, për një njoftim, për një ide.")),
        email: setting("CONTACT_EMAIL"),
    })
}
