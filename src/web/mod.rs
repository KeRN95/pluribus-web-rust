//! HTTP layer: routes, error handling and response helpers shared by the page handlers.

mod account;
mod ai_fill;
mod browse;
mod filters;
mod icons;
mod listings;
mod moderation;
mod newest;
mod photos;
mod post;
mod properties;
mod saved;
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
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use tower_http::{
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    services::ServeDir,
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
        .route("/shto", get(post::new_listing).post(post::create_listing).layer(with_photos()))
        .route("/listing/{public_no}/ndrysho", get(post::edit_listing).post(post::update_listing).layer(with_photos()))
        .route("/media/{*key}", get(photos::serve))
        .route("/shto/fushat", get(properties::fields_for_category))
        .route("/shto/ploteso", post(ai_fill::fill).layer(with_photos()))
        .route("/njoftimet-e-mia", get(post::my_listings))
        .route("/te-ruajturat", get(saved::page))
        .route("/te-ruajturat/numrat", get(saved::numbers))
        .route("/moderim", get(moderation::page))
        .route("/moderim/{id}/mirato", post(moderation::approve))
        .route("/moderim/{id}/bashko", post(moderation::merge))
        .route("/listing/{public_no}/caktivizo", post(post::pause_listing))
        .route("/listing/{public_no}/aktivizo", post(post::resume_listing))
        .route("/listing/{public_no}/fshi", post(post::delete_listing))
        .route("/llogaria", get(account::account_page))
        .route("/perdorues/{id}", get(browse::user_profile))
        .route("/biznes/{slug}", get(browse::business_profile))
        .route("/hyr", post(account::request_code))
        .route("/hyr/kodi", get(account::code_page).post(account::check_code))
        .route("/dil", post(account::sign_out))
        .route("/healthz", get(health))
        .nest_service("/static", ServeDir::new("static"))
        .fallback(not_found)
        .layer(middleware::from_fn(same_site_posts))
        .layer(middleware::from_fn(cache_headers))
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

/// Vendor files carry their version in the file name, so browsers may keep them for a year.
/// The site's own CSS and JavaScript are revalidated on every use, so a deploy shows up at once.
async fn cache_headers(request: Request, next: Next) -> Response {
    let policy = match request.uri().path() {
        path if path.starts_with("/static/vendor/") => Some("public, max-age=31536000, immutable"),
        path if path.starts_with("/static/") => Some("no-cache"),
        _ => None,
    };
    let mut response = next.run(request).await;
    if let Some(policy) = policy {
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(policy));
    }
    response
}

/// The site's stylesheets in static/css/, in the order base.html loads them, which the cascade
/// follows: base.css (the colours, the type, the header), page.css (the cards, form fields),
/// welcome.css, browse.css, map.css, posting.css and listing.css, each its part of the site.
pub const STYLESHEETS: [&str; 7] = ["base", "page", "welcome", "browse", "map", "posting", "listing"];

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

/// Renders an Askama template as an HTML response.
pub fn render(template: impl Template) -> Result<Response> {
    Ok(Html(template.render()?).into_response())
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
