//! Photos on the way in and out: the posting form, which brings them (Submission), and serving
//! them from /media/..., with the copies a page offers the browser (srcset). Their checks, their
//! copies and the store they go into are pluribus-services' (photos.rs, media.rs), which the JSON
//! API's uploads go through too.

use axum::{
    Form,
    body::Bytes,
    extract::{FromRequest, Multipart, Path, Request, State, multipart::MultipartError},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use pluribus_services::photos::{card_copy, unread};

use super::{Result, not_found};
use crate::{
    AppState,
    media::{
        self, CARD_AVIF, CARD_COPY, CARD_SIDE, FULL_SIDE, MIDDLE_SIDE, MINI_AVIF, MINI_COPY, MINI_SIDE, SMALL_AVIF, SMALL_COPY,
        SMALL_SIDE,
    },
};

/// Deletes the files of photos taken off a listing, every copy, or a store's picture's one: the
/// work media.rs does for the expiry too (media::delete_photos).
pub use media::delete_photos as delete;

/// How long browsers may keep an AVIF copy's WebP twin, which stands in for it until it is made
/// (serve): a few minutes, so that they take the AVIF one once it is there.
const STAND_IN: &str = "public, max-age=300";

/// The posting and edit forms: their fields, and the photos chosen, if any. The forms are sent
/// as multipart (enctype in listing_form.html); a plain form, with no photos, is read too. Other
/// files, by their field's name (a store's "logo" and "cover", store_form.html), go to `files`.
pub struct Submission {
    pub pairs: Vec<(String, String)>,
    pub photos: Vec<Bytes>,
    pub files: Vec<(String, Bytes)>,
}

impl<S: Send + Sync> FromRequest<S> for Submission {
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> std::result::Result<Self, Response> {
        let multipart = request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("multipart/form-data"));
        if !multipart {
            let Form(pairs) = Form::<Vec<(String, String)>>::from_request(request, state)
                .await
                .map_err(IntoResponse::into_response)?;
            return Ok(Self { pairs, photos: Vec::new(), files: Vec::new() });
        }
        let mut multipart = Multipart::from_request(request, state).await.map_err(IntoResponse::into_response)?;
        let (mut pairs, mut photos, mut files) = (Vec::new(), Vec::new(), Vec::new());
        while let Some(field) = multipart.next_field().await.map_err(refused)? {
            let name = field.name().unwrap_or_default().to_string();
            if name == "photos" {
                // An empty file input still sends a part, with nothing in it.
                let data = field.bytes().await.map_err(refused)?;
                if !data.is_empty() {
                    photos.push(data);
                }
            } else if field.file_name().is_some() {
                let data = field.bytes().await.map_err(refused)?;
                if !data.is_empty() {
                    files.push((name, data));
                }
            } else {
                pairs.push((name, field.text().await.map_err(refused)?));
            }
        }
        Ok(Self { pairs, photos, files })
    }
}

/// A form too big to take (more than the routes allow, see web/mod.rs), or cut short, said as
/// pluribus-services' photos::unread says it.
fn refused(error: MultipartError) -> Response {
    let status = error.status();
    (status, unread(status)).into_response()
}

/// A WebP photo's copies as a page names them in a srcset (`srcset`, `avif_srcset`): the card's
/// address, the middle one's (none for a photo no bigger), the full one's, and the full copy's
/// size, from which each copy's follows (media::cover_size, fit_size).
pub struct Copies<'a> {
    pub card: &'a str,
    pub middle: Option<&'a str>,
    pub full: &'a str,
    pub width: u32,
    pub height: u32,
}

impl Copies<'_> {
    /// A photo's size as listing_media keeps it, or, for one whose size wasn't kept, a square of
    /// FULL_SIDE, which names the copies as a square photo's.
    pub fn size(width: Option<i32>, height: Option<i32>) -> (u32, u32) {
        match (width.and_then(|w| u32::try_from(w).ok()), height.and_then(|h| u32::try_from(h).ok())) {
            (Some(width), Some(height)) if width > 0 && height > 0 => (width, height),
            _ => (FULL_SIDE, FULL_SIDE),
        }
    }
}

/// Which side of a copy a srcset gives as its width, which the browser sets against the `sizes`
/// the page says and its screen's density: the shorter one for a card, which cuts the photo to a
/// square (page.css, object-fit: cover) and so scales it by that side, a wide photo by its height;
/// the longer one on the listing page, which shows the photo whole.
#[derive(Clone, Copy)]
pub enum Measure {
    Shorter,
    Longer,
}

impl Measure {
    fn of(self, (width, height): (u32, u32)) -> u32 {
        match self {
            Measure::Shorter => width.min(height),
            Measure::Longer => width.max(height),
        }
    }
}

/// Whether a copy of one size has fewer pixels along its sides than one of another: the copies
/// of a photo share its shape.
fn smaller(size: (u32, u32), than: (u32, u32)) -> bool {
    size.0.max(size.1) < than.0.max(than.1)
}

/// The mini copy's address, from a photo's card copy's (pluribus-services' photos::card_copy, with
/// the query of the rules the cards' copies are made by): a thumbnail in the filmstrip under the
/// photos on a listing's page (listing.html, Photo::thumb). None for a photo kept before WebP,
/// whose card copy stands in.
pub fn mini_url(card: &str) -> Option<String> {
    card_copy(card, MINI_COPY)
}

/// A WebP photo's copies for a srcset, the mini one, the small one, the card's, the middle one and
/// the full one, each with the side `by` names as its width. A copy no smaller than the full one,
/// which a small photo's card copy is, is left out.
pub fn srcset(copies: &Copies, by: Measure) -> String {
    let full = (copies.width, copies.height);
    let mut listed = Vec::with_capacity(5);
    for (ending, side) in [(MINI_COPY, MINI_SIDE), (SMALL_COPY, SMALL_SIDE), (CARD_COPY, CARD_SIDE)] {
        let size = media::cover_size(copies.width, copies.height, side);
        if let Some(url) = card_copy(copies.card, ending).filter(|_| smaller(size, full)) {
            listed.push((url, size));
        }
    }
    if let Some(middle) = copies.middle {
        listed.push((middle.to_string(), media::fit_size(copies.width, copies.height, MIDDLE_SIDE)));
    }
    listed.push((copies.full.to_string(), full));
    listed.iter().map(|(url, size)| format!("{url} {}w", by.of(*size))).collect::<Vec<_>>().join(", ")
}

/// A WebP photo's copies for a srcset of AVIF ones, which the browsers that show AVIF take before
/// the WebP ones (`srcset`): the mini one, the small one and the card's as AVIF, then the middle
/// WebP one and the full one, which every such browser shows too, for a big card or a screen
/// denser than the card's copy serves; those no bigger than the card's AVIF copy are left out.
/// None for a photo kept before WebP, or from elsewhere.
pub fn avif_srcset(copies: &Copies, by: Measure) -> Option<String> {
    let card_avif = card_copy(copies.card, CARD_AVIF)?;
    let full = (copies.width, copies.height);
    let card = media::cover_size(copies.width, copies.height, CARD_SIDE);
    let mut listed = Vec::with_capacity(5);
    for (ending, side) in [(MINI_AVIF, MINI_SIDE), (SMALL_AVIF, SMALL_SIDE)] {
        let size = media::cover_size(copies.width, copies.height, side);
        if let Some(url) = card_copy(copies.card, ending).filter(|_| smaller(size, card)) {
            listed.push((url, size));
        }
    }
    listed.push((card_avif, card));
    if smaller(card, full) {
        if let Some(middle) = copies.middle {
            listed.push((middle.to_string(), media::fit_size(copies.width, copies.height, MIDDLE_SIDE)));
        }
        listed.push((copies.full.to_string(), full));
    }
    Some(listed.iter().map(|(url, size)| format!("{url} {}w", by.of(*size))).collect::<Vec<_>>().join(", "))
}

/// The WebP copy of the same size as an AVIF one, which stands in for it until it is made.
fn webp_twin(key: &str) -> Option<String> {
    [(CARD_AVIF, CARD_COPY), (SMALL_AVIF, SMALL_COPY), (MINI_AVIF, MINI_COPY)]
        .into_iter()
        .find_map(|(avif, webp)| key.strip_suffix(avif).map(|name| format!("{name}{webp}")))
}

/// /media/photos/...: a stored photo. Names never change, so browsers may keep them for a year. A
/// photo kept before there were copies of their kind may lack its small or AVIF ones until the
/// server's start has made them all (Media::fill_in_copies): a small WebP one asked for before
/// then is made as it is (Media::make_copy), and an AVIF one, which takes longer, has its WebP twin
/// stand in for it, kept a few minutes only (STAND_IN).
pub async fn serve(State(state): State<AppState>, Path(key): Path<String>) -> Result<Response> {
    let safe = (key.starts_with("photos/") || key.starts_with("stores/"))
        && key.chars().all(|c| c.is_ascii_alphanumeric() || "/-.".contains(c))
        && !key.contains("..");
    if !safe {
        return not_found().await;
    }
    if let Some(bytes) = state.media.get(&key).await? {
        return Ok(photo(bytes, &key, media::CACHE_FOR_A_YEAR));
    }
    let (key, caching, stored) = match webp_twin(&key) {
        Some(twin) => {
            let stored = state.media.get(&twin).await?;
            (twin, STAND_IN, stored)
        }
        None => (key, media::CACHE_FOR_A_YEAR, None),
    };
    let bytes = match stored {
        Some(bytes) => Some(bytes),
        None => state.media.make_copy(&key).await.unwrap_or_else(|error| {
            tracing::warn!(%error, "a photo's copy could not be made");
            None
        }),
    };
    match bytes {
        Some(bytes) => Ok(photo(bytes, &key, caching)),
        None => not_found().await,
    }
}

/// A stored file, with its type and how long it may be kept.
fn photo(bytes: Bytes, key: &str, caching: &'static str) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(media::content_type(key)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(caching));
    response
}
