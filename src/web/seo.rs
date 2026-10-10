//! What search engines and link previews read. Every page says what it is in its head (meta.html):
//! a description, its canonical address, and the Open Graph tags that WhatsApp, Facebook, X and
//! the rest show a shared link with (a title, a description, a photo); a listing's page also says
//! what it sells to Google, as schema.org data (a Product and its Offer: the price, the currency,
//! the condition, whether it is still for sale). robots.txt keeps crawlers out of the pages that
//! are a visitor's own (signing in, their listings, the forms), and the sitemaps list the pages
//! worth finding: the welcome page, every category's, the businesses', and every active listing,
//! LISTINGS_PER_SITEMAP to a file, as Google takes at most 50 000 addresses a file.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use time::macros::format_description;

use super::{Result, not_found};
use crate::{AppState, db};

/// What a page says about itself in its head (meta.html).
pub struct PageMeta {
    /// What a shared link is called: the page's name, as its <title> has it without " | Pluribus".
    pub title: String,
    /// A sentence or two: what search results and previews show under the title.
    pub description: String,
    /// The page's canonical address, absolute.
    pub url: String,
    /// The preview's photo, absolute: a listing's first, or the site's own picture (SITE_IMAGE).
    pub image: String,
    pub image_alt: String,
    /// Open Graph's og:type: "product" for a listing with a price, "website" for the rest.
    pub kind: &'static str,
    /// A listing's price, its amount and currency code, for the product:price tags.
    pub price: Option<(String, String)>,
    /// schema.org data, as JSON made safe for a script tag (json_ld).
    pub json_ld: Option<String>,
    /// Kept out of search engines' results, its links still followed: a search's results, a sold
    /// listing.
    pub noindex: bool,
}

impl PageMeta {
    /// A page's metadata with the site's picture, as a website.
    pub fn new(origin: &str, title: impl Into<String>, description: impl Into<String>, url: String) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            url,
            image: format!("{origin}{SITE_IMAGE}"),
            image_alt: "Pluribus".to_string(),
            kind: "website",
            price: None,
            json_ld: None,
            noindex: false,
        }
    }

    /// The preview's photo: `url` (a listing's), made absolute, with what it shows.
    pub fn with_image(mut self, origin: &str, url: &str, alt: impl Into<String>) -> Self {
        self.image = absolute(origin, url);
        self.image_alt = alt.into();
        self
    }
}

/// What the site is, for the welcome page and the pages that say nothing of their own (base.html).
pub const SITE_DESCRIPTION: &str = "Pluribus: njoftime në gjithë Shqipërinë. Makina, prona, elektronikë, punë, shërbime dhe më shumë, me foto, çmime dhe vendndodhje.";

/// The site's own picture, for a page without a photo of its own: the logo's creature and the
/// name on the ivory page, 1200 by 630, as the previews want it.
pub const SITE_IMAGE: &str = "/static/img/pluribus.png";

/// How many listings a sitemap file lists, by their numbers: a file has those numbered from
/// n × LISTINGS_PER_SITEMAP on, fewer where some are no longer on the site.
const LISTINGS_PER_SITEMAP: i64 = 50_000;

/// The site's address, which every address the previews and sitemaps give starts with:
/// PUBLIC_URL in .env, or as the request came: the host it named (Host; not X-Forwarded-Host,
/// which a visitor may set to anything), over https when the proxy in front says so
/// (X-Forwarded-Proto), as the tunnel does.
pub fn origin(headers: &HeaderMap) -> String {
    if let Ok(url) = std::env::var("PUBLIC_URL") {
        let url = url.trim().trim_end_matches('/');
        if !url.is_empty() {
            return url.to_string();
        }
    }
    let host = headers.get(header::HOST).and_then(|value| value.to_str().ok()).unwrap_or("127.0.0.1:3000");
    let https = headers.get("x-forwarded-proto").is_some_and(|proto| proto == "https");
    format!("{}://{host}", if https { "https" } else { "http" })
}

/// `url` as an absolute address: a path on this site, or one already absolute (a photo on a CDN).
pub fn absolute(origin: &str, url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("{origin}{url}")
    }
}

/// A page's canonical address: its path and query, but for what only orders it (sort), pages it
/// (after) or asks for the map's dots (harta), which show the same listings.
pub fn canonical(origin: &str, path: &str, query: Option<&str>) -> String {
    let kept: Vec<(String, String)> = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .filter(|(name, value)| !matches!(name.as_ref(), "sort" | "after" | "harta") && !value.is_empty())
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    match kept.is_empty() {
        true => format!("{origin}{path}"),
        false => format!("{origin}{path}?{}", form_urlencoded::Serializer::new(String::new()).extend_pairs(kept).finish()),
    }
}

/// `text` in one line, cut to at most `max` characters at a word's end, with "…" where it was cut.
pub fn excerpt(text: &str, max: usize) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let whole = words.join(" ");
    if whole.chars().count() <= max {
        return whole;
    }
    let mut cut = String::new();
    for word in words {
        if cut.chars().count() + word.chars().count() + 1 > max.saturating_sub(1) {
            break;
        }
        if !cut.is_empty() {
            cut.push(' ');
        }
        cut.push_str(word);
    }
    format!("{}…", cut.trim_end_matches([',', '.', ';', ':', '-', '–']))
}

/// JSON for a <script type="application/ld+json">, where "</" would end the script early.
pub fn json_ld(value: &serde_json::Value) -> String {
    value.to_string().replace("</", "<\\/")
}

/// GET /robots.txt: crawlers may go anywhere but the pages that are a visitor's own and the ones
/// only scripts ask for; where the sitemaps are.
pub async fn robots(headers: HeaderMap) -> Response {
    let body = format!(
        "User-agent: *\n\
         Disallow: /hyr\n\
         Disallow: /llogaria\n\
         Disallow: /njoftimet-e-mia\n\
         Disallow: /te-ruajturat\n\
         Disallow: /kerkimet\n\
         Disallow: /lajmerimet\n\
         Disallow: /moderim\n\
         Disallow: /shto\n\
         Disallow: /sugjerime\n\
         Disallow: /listing/*/ndrysho\n\
         Disallow: /dyqani\n\
         Disallow: /ftesa\n\
         Disallow: /ndalo\n\
         \n\
         Sitemap: {}/sitemap.xml\n",
        origin(&headers)
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

/// GET /sitemap.xml: the sitemaps, the pages' and as many of the listings' as their numbers need.
pub async fn sitemap_index(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let origin = origin(&headers);
    let top = db::top_active_number(&state.db).await?.unwrap_or(0);
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    let mut add = |name: String| xml.push_str(&format!("  <sitemap><loc>{origin}/sitemaps/{name}</loc></sitemap>\n"));
    add("faqet.xml".to_string());
    for file in 0..=top / LISTINGS_PER_SITEMAP {
        add(format!("njoftimet-{file}.xml"));
    }
    xml.push_str("</sitemapindex>\n");
    Ok(xml_response(xml))
}

/// GET /sitemaps/{name}: faqet.xml, the welcome page, the pages about the site, every category's
/// and the businesses' with listings; njoftimet-{n}.xml, the active listings numbered from
/// n × LISTINGS_PER_SITEMAP on, each with the day it last changed.
pub async fn sitemap(State(state): State<AppState>, headers: HeaderMap, Path(name): Path<String>) -> Result<Response> {
    let origin = origin(&headers);
    let mut urls: Vec<(String, Option<String>)> = Vec::new();
    if name == "faqet.xml" {
        let (categories, businesses) = tokio::try_join!(db::categories(&state.db), db::businesses_with_listings(&state.db))?;
        urls.push((format!("{origin}/"), None));
        urls.extend(super::pages::PAGES.iter().map(|(path, _)| (format!("{origin}{path}"), None)));
        urls.extend(categories.iter().map(|category| (format!("{origin}/c/{}", category.slug), None)));
        urls.extend(businesses.into_iter().map(|slug| (format!("{origin}/biznes/{slug}"), None)));
    } else if let Some(file) = name
        .strip_prefix("njoftimet-")
        .and_then(|rest| rest.strip_suffix(".xml"))
        .and_then(|number| number.parse::<i64>().ok())
        .filter(|file| *file >= 0)
    {
        let from = file * LISTINGS_PER_SITEMAP;
        let day = format_description!("[year]-[month]-[day]");
        for listing in db::active_listings_numbered(&state.db, from, from + LISTINGS_PER_SITEMAP).await? {
            let changed = listing.updated_at.format(&day).ok();
            urls.push((format!("{origin}/listing/{}-{}", listing.public_no, listing.slug), changed));
        }
    } else {
        return not_found().await;
    }
    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str("\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for (url, changed) in urls {
        xml.push_str(&format!("  <url><loc>{}</loc>", escape_xml(&url)));
        if let Some(changed) = changed {
            xml.push_str(&format!("<lastmod>{changed}</lastmod>"));
        }
        xml.push_str("</url>\n");
    }
    xml.push_str("</urlset>\n");
    Ok(xml_response(xml))
}

fn xml_response(xml: String) -> Response {
    ([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], xml).into_response()
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_at_a_word() {
        assert_eq!(excerpt("Garanci  6 muaj.\nPa asnjë gërvishtje.", 100), "Garanci 6 muaj. Pa asnjë gërvishtje.");
        assert_eq!(excerpt("Garanci 6 muaj. Pa asnjë gërvishtje.", 20), "Garanci 6 muaj. Pa…");
        assert_eq!(excerpt("Garanci 6 muaj. Pa asnjë gërvishtje.", 17), "Garanci 6 muaj…");
    }

    #[test]
    fn leaves_the_order_and_the_page_out_of_the_canonical_address() {
        let url = canonical("https://pluribus.al", "/c/makina", Some("vehicle_fuel=DIESEL&sort=price_asc&after=abc&q="));
        assert_eq!(url, "https://pluribus.al/c/makina?vehicle_fuel=DIESEL");
        assert_eq!(canonical("https://pluribus.al", "/c/makina", Some("sort=price_asc")), "https://pluribus.al/c/makina");
    }

    #[test]
    fn keeps_a_script_tag_closed() {
        assert_eq!(json_ld(&serde_json::json!({"name": "</script>"})), r#"{"name":"<\/script>"}"#);
    }
}
