//! Photos on the way in and out: the posting form, which brings them (Submission), their checks,
//! keeping them (media.rs does the decoding and the store), and serving them from /media/....

use std::collections::HashMap;

use axum::{
    Form,
    body::Bytes,
    extract::{FromRequest, Multipart, Path, Request, State, multipart::MultipartError},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

use super::{Result, not_found};
use crate::{
    AppState,
    db::NewPhoto,
    media::{
        self, CARD_AVIF, CARD_COPY, CARD_SIDE, FULL_SIDE, MAX_PHOTOS, MAX_UPLOAD_BYTES, MIDDLE_SIDE, MINI_AVIF, MINI_COPY,
        MINI_SIDE, Media, Processed, SMALL_AVIF, SMALL_COPY, SMALL_SIDE,
    },
};

/// What each of a photo's files adds to its name (photos/ and 32 hex digits): the full WebP, whose
/// key listing_media keeps, the middle one, the card's, the small one, the mini one, those three
/// as AVIF, and the JPEG. The small, mini and AVIF ones' addresses are the card's with their own
/// ending (card_copy_url, srcset, avif_srcset), so listing_media keeps none. Photos kept before
/// WebP (migration 0025) were a JPEG, whose key it kept, and a JPEG for the card.
const COPIES: [&str; 9] = [".webp", "-middle.webp", CARD_COPY, SMALL_COPY, MINI_COPY, CARD_AVIF, SMALL_AVIF, MINI_AVIF, ".jpg"];
const EARLIER_CARD: &str = "-card.jpg";
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

/// A form too big to take (more than the routes allow, see web/mod.rs), or cut short.
fn refused(error: MultipartError) -> Response {
    let status = error.status();
    let message = match status {
        StatusCode::PAYLOAD_TOO_LARGE => "Fotot janë shumë të mëdha. Zgjidh më pak, ose më të vogla.",
        _ => "Formulari nuk arriti i plotë. Provo përsëri.",
    };
    (status, message).into_response()
}

/// How many, and how big: `kept` is how many the listing has already and keeps.
pub fn check(photos: &[Bytes], kept: usize, errors: &mut HashMap<String, String>) {
    let message = if kept + photos.len() > MAX_PHOTOS {
        Some(format!("Një njoftim mund të ketë deri në {MAX_PHOTOS} foto."))
    } else if photos.iter().any(|photo| photo.len() > MAX_UPLOAD_BYTES) {
        Some(format!("Secila foto mund të jetë deri në {} MB.", MAX_UPLOAD_BYTES / 1024 / 1024))
    } else {
        None
    };
    if let Some(message) = message {
        errors.entry("photos".to_string()).or_insert(message);
    }
}

/// How many of a form's photos are made at once (process): their AVIF copies take most of a second
/// each, so eight would take seven one after the other. Each holds a decoded photo, so not more.
const AT_ONCE: usize = 4;

/// Decodes and resizes them all, off the async threads, a few at once (AT_ONCE, or fewer on fewer
/// cores), in their order. Err with the message for the form when one isn't a photo the site can
/// read.
pub async fn process(photos: Vec<Bytes>) -> Result<std::result::Result<Vec<Processed>, &'static str>> {
    let processed = tokio::task::spawn_blocking(move || {
        let threads = std::thread::available_parallelism().map_or(1, usize::from).clamp(1, AT_ONCE);
        let each = photos.len().div_ceil(threads).max(1);
        std::thread::scope(|scope| {
            let groups: Vec<_> = photos
                .chunks(each)
                .map(|group| scope.spawn(move || group.iter().map(|photo| media::process(photo)).collect::<Vec<_>>()))
                .collect();
            groups
                .into_iter()
                .flat_map(|group| group.join().expect("making a photo's copies doesn't panic"))
                .collect::<std::result::Result<Vec<_>, _>>()
        })
    })
    .await?;
    Ok(processed.map_err(|_| "Një nga fotot nuk u lexua. Zgjidh foto JPG, PNG ose WebP."))
}

/// Puts each one's files in the store, under a new name, and says where they are served.
pub async fn store(media: &Media, processed: Vec<Processed>) -> Result<Vec<NewPhoto>> {
    let mut stored = Vec::with_capacity(processed.len());
    for photo in processed {
        let name = format!("photos/{:032x}", rand::random::<u128>());
        let [full_key, middle_key, card_key, small_key, mini_key, card_avif_key, small_avif_key, mini_avif_key, jpeg_key] =
            COPIES.map(|copy| format!("{name}{copy}"));
        let size_bytes = photo.full.len() as i64;
        let middle_url = photo.middle.is_some().then(|| media.url(&middle_key));
        let middle = async {
            match photo.middle {
                Some(bytes) => media.put(&middle_key, bytes).await,
                None => Ok(()),
            }
        };
        tokio::try_join!(
            media.put(&full_key, photo.full),
            middle,
            media.put(&card_key, photo.card),
            media.put(&small_key, photo.small),
            media.put(&mini_key, photo.mini),
            media.put(&card_avif_key, photo.card_avif),
            media.put(&small_avif_key, photo.small_avif),
            media.put(&mini_avif_key, photo.mini_avif),
            media.put(&jpeg_key, photo.jpeg),
        )?;
        stored.push(NewPhoto {
            url: media.url(&full_key),
            middle_url,
            card_url: media.url(&card_key),
            jpeg_url: media.url(&jpeg_key),
            placeholder: photo.placeholder,
            storage_key: full_key,
            size_bytes,
            width: photo.width as i32,
            height: photo.height as i32,
            sha256: photo.sha256.to_vec(),
        });
    }
    Ok(stored)
}

/// Deletes the files of photos taken off a listing, every copy, those of photos kept before WebP
/// included; a store's logo or cover (`stores/`) is one file. A file left behind only takes space,
/// so a failure is logged rather than shown.
pub async fn delete(media: &Media, keys: &[String]) {
    for key in keys {
        let files: Vec<String> = match key.strip_suffix(".webp").or_else(|| key.strip_suffix(".jpg")) {
            Some(name) if !key.starts_with("stores/") => COPIES.iter().chain([&EARLIER_CARD]).map(|copy| format!("{name}{copy}")).collect(),
            _ => vec![key.clone()],
        };
        for file in files {
            if let Err(error) = media.delete(&file).await {
                tracing::warn!(%error, "a removed photo's file stays in the store");
            }
        }
    }
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

/// The address of one of the cards' copies as a page names it: with the query of the rules the
/// copies are made by (media::CARD_COPIES_EDITION), since they were made again on 2026-10-09
/// under the names they had, which browsers keep for a year (media::CACHE_FOR_A_YEAR), and so a
/// browser that kept the old ones fetches the new. The server serves a photo by its path,
/// whatever the query.
fn card_copy_url(name: &str, ending: &str) -> String {
    format!("{name}{ending}?{}", media::CARD_COPIES_EDITION)
}

/// The mini copy's address, from a photo's card copy's: a thumbnail in the filmstrip under the
/// photos on a listing's page (listing.html, Photo::thumb). None for a photo kept before WebP,
/// whose card copy stands in.
pub fn mini_url(card: &str) -> Option<String> {
    card.strip_suffix(CARD_COPY).map(|name| card_copy_url(name, MINI_COPY))
}

/// A WebP photo's copies for a srcset, the mini one, the small one, the card's, the middle one and
/// the full one, each with the side `by` names as its width. A copy no smaller than the full one,
/// which a small photo's card copy is, is left out.
pub fn srcset(copies: &Copies, by: Measure) -> String {
    let full = (copies.width, copies.height);
    let mut listed = Vec::with_capacity(5);
    for (ending, side) in [(MINI_COPY, MINI_SIDE), (SMALL_COPY, SMALL_SIDE), (CARD_COPY, CARD_SIDE)] {
        let size = media::cover_size(copies.width, copies.height, side);
        if let Some(name) = copies.card.strip_suffix(CARD_COPY).filter(|_| smaller(size, full)) {
            listed.push((card_copy_url(name, ending), size));
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
    let name = copies.card.strip_suffix(CARD_COPY)?;
    let full = (copies.width, copies.height);
    let card = media::cover_size(copies.width, copies.height, CARD_SIDE);
    let mut listed = Vec::with_capacity(5);
    for (ending, side) in [(MINI_AVIF, MINI_SIDE), (SMALL_AVIF, SMALL_SIDE)] {
        let size = media::cover_size(copies.width, copies.height, side);
        if smaller(size, card) {
            listed.push((card_copy_url(name, ending), size));
        }
    }
    listed.push((card_copy_url(name, CARD_AVIF), card));
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
