//! HTTP layer: routes, error handling and response helpers shared by the page handlers.

mod account;
mod ai_fill;
mod browse;
mod filters;
mod icons;
mod listings;
mod messages;
mod moderation;
mod named;
mod newest;
mod pages;
mod photos;
mod post;
mod properties;
mod safety;
mod saved;
mod seo;
mod suggestions;

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use askama::Template;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use sha2::{Digest, Sha256};
use tower_http::{
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{AppState, db};

pub use newest::close_streams;

/// The posting and edit forms carry photos: MAX_PHOTOS of them, which phones shrink before sending
/// (photos_controller.js). Other requests keep axum's 2 MB.
const FORM_WITH_PHOTOS_BYTES: usize = 64 * 1024 * 1024;

pub fn router(state: AppState) -> Router {
    let with_photos = || DefaultBodyLimit::max(FORM_WITH_PHOTOS_BYTES);
    Router::new()
        .route("/", get(browse::home))
        .route("/c/{slug}", get(browse::category))
        .route("/search", get(browse::search))
        .route("/sugjerime", get(suggestions::suggestions))
        .route("/te-rejat", get(newest::stream))
        .route("/harta", get(browse::map))
        .route("/listing/{key}", get(listings::show))
        .route("/listing/{public_no}/phone", post(listings::reveal_phone))
        .route("/listing/{public_no}/pare", post(listings::seen))
        .route("/listing/{public_no}/ruaj", post(saved::save))
        .route("/listing/{public_no}/raporto", get(safety::report_page).post(safety::report))
        .route("/listing/{public_no}/blloko", post(safety::block))
        .route("/llogaria/zhblloko/{id}", post(safety::unblock))
        .route("/shto", get(post::new_listing).post(post::create_listing).layer(with_photos()))
        .route("/listing/{public_no}/ndrysho", get(post::edit_listing).post(post::update_listing).layer(with_photos()))
        .route("/media/{*key}", get(photos::serve))
        .route("/shto/fushat", get(properties::fields_for_category))
        .route("/shto/ploteso", post(ai_fill::fill).layer(with_photos()))
        .route("/njoftimet-e-mia", get(post::my_listings))
        .route("/te-ruajturat", get(saved::page))
        .route("/te-ruajturat/numrat", get(saved::numbers))
        .route("/te-ruajturat/pare", post(saved::seen))
        .route("/listing/{public_no}/mesazh", get(messages::compose).post(messages::start))
        .route("/mesazhet", get(messages::inbox))
        .route("/mesazhet/numri", get(messages::unread))
        .route("/mesazhet/{id}", get(messages::conversation).post(messages::reply))
        .route("/mesazhet/{id}/te-rejat", get(messages::newer))
        .route("/mesazhet/{id}/lexuar", post(messages::read))
        .route("/mesazhet/{id}/blloko", post(messages::block))
        .route("/mesazhet/{id}/zhblloko", post(messages::unblock))
        .route("/moderim", get(moderation::page))
        .route("/moderim/{id}/mirato", post(moderation::approve))
        .route("/moderim/{id}/bashko", post(moderation::merge))
        .route("/moderim/njoftim/{public_no}/mbaje", post(moderation::keep))
        .route("/moderim/njoftim/{public_no}/hiq", post(moderation::remove))
        .route("/listing/{public_no}/caktivizo", post(post::pause_listing))
        .route("/listing/{public_no}/aktivizo", post(post::resume_listing))
        .route("/listing/{public_no}/fshi", post(post::delete_listing))
        .route("/llogaria", get(account::account_page))
        .route("/perdorues/{id}", get(browse::user_profile))
        .route("/biznes/{slug}", get(browse::business_profile))
        .route("/hyr", post(account::request_code))
        .route("/hyr/kodi", get(account::code_page).post(account::check_code))
        .route("/dil", post(account::sign_out))
        .route("/si-funksionon", get(pages::how_it_works))
        .route("/siguria", get(pages::safety_tips))
        .route("/kushtet", get(pages::terms))
        .route("/privatesia", get(pages::privacy))
        .route("/kontakt", get(pages::contact))
        .route("/healthz", get(health))
        .route("/robots.txt", get(seo::robots))
        .route("/sitemap.xml", get(seo::sitemap_index))
        .route("/sitemaps/{name}", get(seo::sitemap))
        // Where browsers and apps look for a site's icon without asking the page (base.html names
        // it, with the SVG, the home screens' and the manifest's).
        .route_service("/favicon.ico", ServeFile::new("static/img/favicon.ico"))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(not_found)
        .layer(middleware::from_fn(same_site_posts))
        .layer(middleware::from_fn(cache_headers))
        .layer(middleware::from_fn(security_headers))
        .layer(middleware::from_fn(account::signed_in_cookie))
        // WOFF2 fonts are Brotli-compressed already; compressing them again only costs time.
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("font/")),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Refuses forms posted from other sites. Browsers send the posting page's origin with every
/// POST, so a form on another site, trying to sign someone out or post in their name, carries
/// that site's; the session cookie is SameSite=Lax as well. Requests without an Origin (curl, the
/// tests) pass: they carry no visitor's cookies by accident.
async fn same_site_posts(request: Request, next: Next) -> Response {
    if request.method() == Method::POST {
        let host = request.headers().get(header::HOST).and_then(|value| value.to_str().ok());
        let origin = request.headers().get(header::ORIGIN).and_then(|value| value.to_str().ok());
        if let (Some(origin), Some(host)) = (origin, host) {
            let origin_host = origin.split_once("://").map_or(origin, |(_, rest)| rest);
            if origin_host != host {
                return (StatusCode::FORBIDDEN, "Kjo kërkesë vjen nga një faqe tjetër.").into_response();
            }
        }
    }
    next.run(request).await
}

/// Vendor files carry their version in the file name, and the site's own CSS and JavaScript
/// their fingerprint in the address (?v=, asset_version), so browsers keep them for a year and
/// never ask again: a page loaded in full asks for none it has. Once a deploy changes them the
/// fingerprint changes with them, and so do their addresses. Any other file under /static (the
/// icons, the manifest), /favicon.ico, or one asked for with another fingerprint (from a page
/// loaded before a deploy), is checked again on every use, so a change shows up at once.
async fn cache_headers(request: Request, next: Next) -> Response {
    let fingerprinted = request.uri().query().and_then(|query| query.strip_prefix("v=")) == Some(asset_version());
    let policy = match request.uri().path() {
        path if path.starts_with("/static/vendor/") => Some(KEEP),
        path if fingerprinted && (path.starts_with("/static/css/") || path.starts_with("/static/js/")) => Some(KEEP),
        path if path.starts_with("/static/") || path == "/favicon.ico" => Some("no-cache"),
        _ => None,
    };
    let mut response = next.run(request).await;
    if let Some(policy) = policy {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
    }
    response
}

/// What every response tells the browser about keeping the site safe, unless the response says
/// otherwise itself: what its pages may load and run (`content_security_policy`); no guessing a
/// file's type from its bytes, so a photo can't be taken for a script or a page; no page of the
/// site inside another site's frame, for browsers that know only X-Frame-Options too; other sites
/// told, where a link leads to them, the site's address but not the page's; no camera,
/// microphone, payments or USB for any page, and the visitor's location for the site's own alone
/// ("Afër meje", "Vendndodhja ime"); and a window of its own, which a page of another site it
/// opens (a share) can't reach back into. HTTPS-only (Strict-Transport-Security) is for whatever
/// serves the site over HTTPS to say.
async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CONTENT_SECURITY_POLICY, content_security_policy().clone()),
        (header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")),
        (header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY")),
        (header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin")),
        (
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(), microphone=(), payment=(), usb=(), geolocation=(self)"),
        ),
        (HeaderName::from_static("cross-origin-opener-policy"), HeaderValue::from_static("same-origin")),
    ] {
        headers.entry(name).or_insert(value);
    }
    response
}

/// The Content-Security-Policy of every response. Scripts come from the site alone, and of those
/// in a page only base.html's two, by their hashes (the theme and the import map: THEME_SCRIPT,
/// `import_map`), so a script that found its way into a page, in a listing's words or a crafted
/// link, doesn't run; Leaflet, which the maps load, comes from /static too. Styles come from the
/// site and from the page itself (a card's placeholder photo, Leaflet's positions, Turbo's progress
/// bar). Pictures come from the site, from anywhere over HTTPS (OpenStreetMap's tiles, the photos
/// from a CDN when MEDIA_URL names one), as `data:` (the placeholders) and `blob:` (the posting
/// form's previews). Requests go to the site alone. No plugins, no `<base>` elsewhere, no form
/// sent to another site, and no other site framing a page (`frame-ancestors`). A script added to a
/// page later needs its hash here, or a file of its own under /static/js.
fn content_security_policy() -> &'static HeaderValue {
    static POLICY: OnceLock<HeaderValue> = OnceLock::new();
    POLICY.get_or_init(|| {
        let hash = |script: &str| format!("'sha256-{}'", BASE64.encode(Sha256::digest(script)));
        let policy = format!(
            "default-src 'self'; script-src 'self' {} {}; style-src 'self' 'unsafe-inline'; \
             img-src 'self' https: data: blob:; font-src 'self'; connect-src 'self'; object-src 'none'; \
             base-uri 'self'; form-action 'self'; frame-ancestors 'none'",
            hash(THEME_SCRIPT),
            hash(import_map()),
        );
        HeaderValue::from_str(&policy).expect("the policy is plain ASCII")
    })
}

/// How long browsers keep a file whose address changes whenever the file does: a year, without
/// asking again.
const KEEP: &str = "public, max-age=31536000, immutable";

/// The site's stylesheets in static/css/, in the order base.html loads them, which the cascade
/// follows: base.css (the colours, the type, the header), page.css (the cards, form fields),
/// welcome.css, browse.css, map.css, posting.css, listing.css and messages.css, each its part of
/// the site.
pub const STYLESHEETS: [&str; 8] = ["base", "page", "welcome", "browse", "map", "posting", "listing", "messages"];

/// A fingerprint of the site's own CSS and JavaScript, which base.html adds to their URLs. Turbo
/// keeps the stylesheet and scripts of the first page a visitor opens while they move around the
/// site; it tracks these tags (data-turbo-track="reload"), so once a deploy changes the files and
/// with them the fingerprint, the visitor's next click loads the page in full, with the new files.
/// Worked out from the files on disk the first time a page needs it, so once per server start.
pub fn asset_version() -> &'static str {
    static VERSION: OnceLock<String> = OnceLock::new();
    VERSION.get_or_init(|| {
        let mut files = Vec::new();
        for dir in ["static/css", "static/js"] {
            collect_files(Path::new(dir), &mut files);
        }
        files.sort();
        let mut hasher = DefaultHasher::new();
        for file in &files {
            file.hash(&mut hasher);
            std::fs::read(file).unwrap_or_default().hash(&mut hasher);
        }
        format!("{:08x}", hasher.finish() as u32)
    })
}

/// Turbo and Stimulus, their versions in their names: the import map names them, and every page
/// preloads them (base.html). To upgrade one, put its file in static/vendor/ and change its name
/// here.
pub const TURBO: &str = "/static/vendor/turbo-8.0.23.js";
pub const STIMULUS: &str = "/static/vendor/stimulus-3.2.2.js";

/// The one script base.html runs in the page itself, before the first paint: the theme the
/// visitor picked (theme_controller.js keeps it in localStorage), so a page never shows the other
/// one first. Here rather than in the template so that its hash in the Content-Security-Policy is
/// that of the very text pages have.
pub const THEME_SCRIPT: &str = r#"try { const theme = localStorage.getItem("theme"); if (theme === "light" || theme === "dark") document.documentElement.dataset.theme = theme; } catch {}"#;

/// The import map base.html gives the browser: Turbo, Stimulus, and each of the site's modules at
/// its address with the fingerprint, which browsers keep for a year (`cache_headers`), and
/// "controllers/" for any other. Made once, so that its hash in the Content-Security-Policy is that
/// of the very text pages have.
pub fn import_map() -> &'static str {
    static MAP: OnceLock<String> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut imports = serde_json::Map::new();
        imports.insert("@hotwired/turbo".into(), TURBO.into());
        imports.insert("@hotwired/stimulus".into(), STIMULUS.into());
        for module in js_modules().iter().filter(|module| module.starts_with("controllers/")) {
            imports.insert(module.clone(), format!("/static/js/{module}?v={}", asset_version()).into());
        }
        imports.insert("controllers/".into(), "/static/js/controllers/".into());
        serde_json::to_string_pretty(&serde_json::json!({ "imports": imports })).expect("plain strings")
    })
}

/// Where base.html's head takes the page's own modules, which `render` puts there.
pub const MODULES_HERE: &str = "<!-- modulepreload: web::render -->";

/// The modules a page uses, each preloaded (`<link rel="modulepreload">`) where base.html marks it,
/// so the browser fetches them alongside the stylesheets rather than after application.js: that,
/// the controller of every `data-controller` the page names ("map" for map_controller.js), and what
/// these import, from the site's own modules ("controllers/..."). No others: application.js loads a
/// controller only once the page names it, here or in a part that comes later.
fn module_preloads(html: &str) -> String {
    let mut wanted = vec!["application.js".to_string()];
    let mut rest = html;
    while let Some(at) = rest.find("data-controller=\"") {
        rest = &rest[at + "data-controller=\"".len()..];
        let end = rest.find('"').unwrap_or(rest.len());
        for identifier in rest[..end].split_whitespace() {
            wanted.push(format!("controllers/{}_controller.js", identifier.replace('-', "_")));
        }
        rest = &rest[end..];
    }
    let mut preloaded: Vec<&str> = Vec::new();
    let mut next = 0;
    while let Some(module) = wanted.get(next).cloned() {
        next += 1;
        let Some(known) = js_modules().iter().find(|known| **known == module) else { continue };
        if preloaded.contains(&known.as_str()) {
            continue;
        }
        preloaded.push(known);
        wanted.extend(module_imports().get(known).into_iter().flatten().cloned());
    }
    preloaded
        .iter()
        .map(|module| format!(r#"<link rel="modulepreload" href="/static/js/{module}?v={}">"#, asset_version()))
        .collect::<Vec<_>>()
        .join("\n  ")
}

/// What each of the site's modules imports of the others, read from their files once: the
/// specifiers after `from` or a bare `import` that start with "controllers/", as the import map
/// names the site's modules.
fn module_imports() -> &'static std::collections::HashMap<String, Vec<String>> {
    static IMPORTS: OnceLock<std::collections::HashMap<String, Vec<String>>> = OnceLock::new();
    IMPORTS.get_or_init(|| {
        js_modules()
            .iter()
            .map(|module| {
                let source = std::fs::read_to_string(Path::new("static/js").join(module)).unwrap_or_default();
                let mut imports = Vec::new();
                for keyword in ["from \"", "import \""] {
                    let mut rest = source.as_str();
                    while let Some(at) = rest.find(keyword) {
                        rest = &rest[at + keyword.len()..];
                        let end = rest.find('"').unwrap_or(rest.len());
                        if rest[..end].starts_with("controllers/") {
                            imports.push(rest[..end].to_string());
                        }
                        rest = &rest[end..];
                    }
                }
                (module.clone(), imports)
            })
            .collect()
    })
}

/// The site's JavaScript modules (static/js), as paths under it ("controllers/heart_controller.js"),
/// application.js first: those the import map names with the fingerprint, and those a page may
/// preload.
pub fn js_modules() -> &'static [String] {
    static MODULES: OnceLock<Vec<String>> = OnceLock::new();
    MODULES.get_or_init(|| {
        let mut files = Vec::new();
        collect_files(Path::new("static/js"), &mut files);
        let mut modules: Vec<String> = files
            .iter()
            .filter_map(|file| file.strip_prefix("static/js").ok())
            .map(|path| path.to_string_lossy().replace('\\', "/"))
            .filter(|path| path.ends_with(".js"))
            .collect();
        modules.sort_by_key(|module| (module != "application.js", module.clone()));
        modules
    })
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

/// Any error that ends a request: logged in full, shown to the visitor as a plain 500.
pub struct AppError(anyhow::Error);

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(error: E) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!(error = ?self.0, "request failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Diçka shkoi keq. Provo përsëri.",
        )
            .into_response()
    }
}

pub type Result<T, E = AppError> = std::result::Result<T, E>;

/// Renders an Askama template as an HTML response; a whole page (base.html) with the modules it
/// uses preloaded in its head (`module_preloads`).
pub fn render(template: impl Template) -> Result<Response> {
    let mut html = template.render()?;
    if let Some(at) = html.find(MODULES_HERE) {
        let preloads = module_preloads(&html);
        html.replace_range(at..at + MODULES_HERE.len(), &preloads);
    }
    Ok(Html(html).into_response())
}

/// Renders a Turbo Stream: instructions Turbo applies to the page that made the request.
pub fn render_stream(template: impl Template) -> Result<Response> {
    let content_type = HeaderValue::from_static("text/vnd.turbo-stream.html; charset=utf-8");
    Ok(([(header::CONTENT_TYPE, content_type)], template.render()?).into_response())
}

/// Infinite scroll: Turbo loading a next-page `<turbo-frame id="page-…">`, which only needs the
/// frame. (Other frames, like the map page's results, get the whole page; Turbo takes the frame
/// out of it.)
pub fn is_next_page_request(headers: &HeaderMap) -> bool {
    headers
        .get("turbo-frame")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|frame| frame.starts_with("page-"))
}

/// A cookie's value, if the request carries it.
pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| {
            let (key, value) = pair.trim().split_once('=')?;
            (key == name && !value.is_empty()).then(|| value.to_string())
        })
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundPage;

pub async fn not_found() -> Result<Response> {
    Ok((StatusCode::NOT_FOUND, render(NotFoundPage)?).into_response())
}

async fn health(State(state): State<AppState>) -> Result<&'static str> {
    db::ping(&state.db).await?;
    Ok("ok")
}
