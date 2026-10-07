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
    media::{self, CARD_SIDE, MAX_PHOTOS, MAX_UPLOAD_BYTES, MIDDLE_SIDE, Media, Processed},
};

/// What each of a photo's files adds to its name (photos/ and 32 hex digits): the full WebP, whose
/// key listing_media keeps, the middle one, the card's, and the JPEG. Photos kept before WebP
/// (migration 0025) were a JPEG, whose key it kept, and a JPEG for the card.
const COPIES: [&str; 4] = [".webp", "-middle.webp", "-card.webp", ".jpg"];
const EARLIER_CARD: &str = "-card.jpg";

/// The posting and edit forms: their fields, and the photos chosen, if any. The forms are sent
/// as multipart (enctype in listing_form.html); a plain form, with no photos, is read too.
pub struct Submission {
    pub pairs: Vec<(String, String)>,
    pub photos: Vec<Bytes>,
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
            return Ok(Self { pairs, photos: Vec::new() });
        }
        let mut multipart = Multipart::from_request(request, state).await.map_err(IntoResponse::into_response)?;
        let (mut pairs, mut photos) = (Vec::new(), Vec::new());
        while let Some(field) = multipart.next_field().await.map_err(refused)? {
            let name = field.name().unwrap_or_default().to_string();
            if name == "photos" {
                // An empty file input still sends a part, with nothing in it.
                let data = field.bytes().await.map_err(refused)?;
                if !data.is_empty() {
                    photos.push(data);
                }
            } else {
                pairs.push((name, field.text().await.map_err(refused)?));
            }
        }
        Ok(Self { pairs, photos })
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

/// Decodes and resizes them all, off the async threads. Err with the message for the form when one
/// isn't a photo the site can read.
pub async fn process(photos: Vec<Bytes>) -> Result<std::result::Result<Vec<Processed>, &'static str>> {
    let processed = tokio::task::spawn_blocking(move || {
        photos.iter().map(|photo| media::process(photo)).collect::<std::result::Result<Vec<_>, _>>()
    })
    .await?;
    Ok(processed.map_err(|_| "Një nga fotot nuk u lexua. Zgjidh foto JPG, PNG ose WebP."))
}

/// Puts each one's files in the store, under a new name, and says where they are served.
pub async fn store(media: &Media, processed: Vec<Processed>) -> Result<Vec<NewPhoto>> {
    let mut stored = Vec::with_capacity(processed.len());
    for photo in processed {
        let name = format!("photos/{:032x}", rand::random::<u128>());
        let [full_key, middle_key, card_key, jpeg_key] = COPIES.map(|copy| format!("{name}{copy}"));
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
/// included. A file left behind only takes space, so a failure is logged rather than shown.
pub async fn delete(media: &Media, keys: &[String]) {
    for key in keys {
        let files: Vec<String> = match key.strip_suffix(".webp").or_else(|| key.strip_suffix(".jpg")) {
            Some(name) => COPIES.iter().chain([&EARLIER_CARD]).map(|copy| format!("{name}{copy}")).collect(),
            None => vec![key.clone()],
        };
        for file in files {
            if let Err(error) = media.delete(&file).await {
                tracing::warn!(%error, "a removed photo's file stays in the store");
            }
        }
    }
}

/// A WebP photo's copies for a srcset, the card's, the middle one and the full one, each with its
/// longer side as its width: photos are made by it, and it is about what a tall photo needs too,
/// shown whole on a listing page or cut square on a card. `side` is the full copy's longer side;
/// a copy no smaller than it isn't there, or is left out.
pub fn srcset(card: &str, middle: Option<&str>, full: &str, side: u32) -> String {
    let mut copies = Vec::with_capacity(3);
    if side > CARD_SIDE {
        copies.push(format!("{card} {CARD_SIDE}w"));
    }
    if let Some(middle) = middle.filter(|_| side > MIDDLE_SIDE) {
        copies.push(format!("{middle} {MIDDLE_SIDE}w"));
    }
    copies.push(format!("{full} {side}w"));
    copies.join(", ")
}

/// /media/photos/...: a stored photo. Names never change, so browsers may keep them for a year.
pub async fn serve(State(state): State<AppState>, Path(key): Path<String>) -> Result<Response> {
    let safe = key.starts_with("photos/") && key.chars().all(|c| c.is_ascii_alphanumeric() || "/-.".contains(c)) && !key.contains("..");
    let Some(bytes) = (if safe { state.media.get(&key).await? } else { None }) else {
        return not_found().await;
    };
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(media::content_type(&key)));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(media::CACHE_FOR_A_YEAR));
    Ok(response)
}
