//! The pages about the site, which every page's footer links: how it works, the safety tips,
//! the terms of use, privacy and contact. Each is a column of text (text_page in page.css) with
//! its description and canonical address for search engines (seo.rs), listed in the pages'
//! sitemap.

use askama::Template;
use axum::{http::HeaderMap, response::Response};

use super::{
    Result, render,
    seo::{self, PageMeta},
};

/// The pages, their paths and names, as the footer links them and the sitemap lists them.
pub const PAGES: [(&str, &str); 5] = [
    ("/si-funksionon", "Si funksionon"),
    ("/siguria", "Këshilla sigurie"),
    ("/kushtet", "Kushtet e përdorimit"),
    ("/privatesia", "Privatësia"),
    ("/kontakt", "Kontakt"),
];

/// When the terms and the privacy page last changed, as they say at their top.
const TERMS_UPDATED: &str = "8 tetor 2026";
const PRIVACY_UPDATED: &str = "10 tetor 2026";

/// A setting from .env, when it is set: CONTACT_EMAIL, the address the contact and privacy pages
/// give; OPERATOR, who runs the site (a person's name, or a company's), whom the privacy page names
/// as the one responsible for the data.
fn setting(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

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
            "Si kërkon, ruan dhe ndjek njoftimet, si kontakton shitësin, dhe si shton një njoftim me numrin e celularit, pa fjalëkalim.",
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
            "Si blen dhe shet pa rrezik: takohu në vend publik, shiko mallin para se të paguash, mos dërgo para paraprakisht, dhe çfarë të bësh kur diçka s'shkon.",
        ),
    })
}

#[derive(Template)]
#[template(path = "terms.html")]
struct Terms {
    meta: PageMeta,
    updated: &'static str,
}

pub async fn terms(headers: HeaderMap) -> Result<Response> {
    render(Terms {
        meta: meta(
            &headers,
            "/kushtet",
            "Kushtet e përdorimit",
            "Çfarë lejohet dhe çfarë jo në Pluribus: llogaria, njoftimet, mesazhet, moderimi dhe përgjegjësia.",
        ),
        updated: TERMS_UPDATED,
    })
}

#[derive(Template)]
#[template(path = "privacy.html")]
struct Privacy {
    meta: PageMeta,
    updated: &'static str,
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
            "Cilat të dhëna mban Pluribus, pse, kush tjetër i merr, sa kohë, dhe si i sheh, i ndryshon ose i fshin.",
        ),
        updated: PRIVACY_UPDATED,
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
        meta: meta(&headers, "/kontakt", "Kontakt", "Si i shkruan Pluribus: për llogarinë, për një njoftim, për një ide."),
        email: setting("CONTACT_EMAIL"),
    })
}
