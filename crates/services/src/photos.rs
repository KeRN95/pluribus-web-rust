//! Photos on their way in, which the pages' forms (pluribus-web's photos.rs, its `Submission`, and
//! post.rs) and the JSON API's uploads (pluribus-api's own.rs) take alike: how many, and how big
//! (`check`); each made into its copies, a few at once (`process`, by media.rs's `process`); and
//! its files put in the store under a new name (`store`), which says where each is served and what
//! it is (db::NewPhoto: its key in the store, its copies' addresses, its size, its tiny copy and its
//! fingerprint), for the rows that name it: a listing's (listing_media, by db::create_listing and
//! update_listing), or an upload's until a listing takes it (photo_uploads, the API's); taken out of
//! the store again when they don't go in (`unstore`). And the addresses of a WebP photo's copies
//! that the rows keep none of, named from its card copy's (`card_copy`), as the pages and the API
//! name them. pluribus-web's photos.rs's until C9.

use std::time::Duration;

use axum::{body::Bytes, http::StatusCode};
use pluribus_core::i18n::{t, t1};
use pluribus_db::NewPhoto;

use crate::media::{self, CARD_COPIES_EDITION, CARD_COPY, COPIES, MAX_PHOTOS, MAX_UPLOAD_BYTES, Media, Processed};

/// How many photos are made at once (`process`): their AVIF copies take most of a second each, so
/// eight would take seven one after the other. Each holds a decoded photo, so not more.
const AT_ONCE: usize = 4;

/// What a request with photos may weigh, the pages' posting form's or the API's uploads: MAX_PHOTOS
/// of them, which phones shrink before sending (photos_controller.js, the app). Other requests keep
/// axum's 2 MB. (pluribus-web's FORM_WITH_PHOTOS_BYTES says the same for its forms.)
pub const UPLOAD_BODY_BYTES: usize = 64 * 1024 * 1024;

/// How long a photo uploaded before its listing (photo_uploads) waits for one to take it: the
/// expiry forgets those older, with their files (db::forget_uploads).
pub const UPLOADS_KEPT: Duration = Duration::from_secs(24 * 60 * 60);

/// What is wrong with the photos sent, in the language now, if anything: more than MAX_PHOTOS with
/// the `kept` a listing has already and keeps (`too_many`), or one over MAX_UPLOAD_BYTES.
pub fn check(photos: &[Bytes], kept: usize) -> Option<String> {
    too_many(kept + photos.len()).or_else(|| {
        let too_big = photos.iter().any(|photo| photo.len() > MAX_UPLOAD_BYTES);
        too_big.then(|| t1("Secila foto mund të jetë deri në {} MB.", MAX_UPLOAD_BYTES / 1024 / 1024))
    })
}

/// What is said of `count` photos for one listing, more than MAX_PHOTOS, in the language now: a
/// form's (`check`), or the uploads a listing names (pluribus-api's own.rs).
pub fn too_many(count: usize) -> Option<String> {
    (count > MAX_PHOTOS).then(|| t1("Një njoftim mund të ketë deri në {} foto.", MAX_PHOTOS))
}

/// What a request with photos says when its body couldn't be read whole, by the status its reading
/// failed with: too big to take (over what the route allows, UPLOAD_BODY_BYTES), or cut short.
pub fn unread(status: StatusCode) -> &'static str {
    match status {
        StatusCode::PAYLOAD_TOO_LARGE => t("Fotot janë shumë të mëdha. Zgjidh më pak, ose më të vogla."),
        _ => t("Formulari nuk arriti i plotë. Provo përsëri."),
    }
}

/// Decodes and resizes them all, off the async threads, a few at once (AT_ONCE, or fewer on fewer
/// cores), in their order. Err with what to say, in the language now, when one isn't a photo the
/// site can read.
pub async fn process(photos: Vec<Bytes>) -> anyhow::Result<Result<Vec<Processed>, &'static str>> {
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
                .collect::<Result<Vec<_>, _>>()
        })
    })
    .await?;
    Ok(processed.map_err(|_| t("Një nga fotot nuk u lexua. Zgjidh foto JPG, PNG ose WebP.")))
}

/// Puts each one's files in the store, under a new name, and says where they are served and what
/// they are, as the rows that name them keep it (db::NewPhoto).
pub async fn store(media: &Media, processed: Vec<Processed>) -> anyhow::Result<Vec<NewPhoto>> {
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

/// Takes the files of photos just put in the store out again, every copy, when the listing, the
/// edit or the upload they came with wasn't kept, so that nothing is left of it.
pub async fn unstore(media: &Media, stored: &[NewPhoto]) {
    let keys: Vec<String> = stored.iter().map(|photo| photo.storage_key.clone()).collect();
    media::delete_photos(media, &keys).await;
}

/// The address of a WebP photo's copy named by `ending` (one of the cards' copies of media::COPIES:
/// the card's, the small and the mini ones, as WebP or AVIF), as the pages and the API name it: its
/// card copy's address with that copy's ending, and the query of the rules the cards' copies are made
/// by (media::CARD_COPIES_EDITION), since they were made again under the names they had, which
/// browsers keep for a year (media::CACHE_FOR_A_YEAR), so that a browser that kept the old ones
/// fetches the new. The server serves a photo by its path, whatever the query. None for a card copy
/// that isn't a WebP photo's (one kept before WebP, its card-sized JPEG).
pub fn card_copy(card_url: &str, ending: &str) -> Option<String> {
    card_url.strip_suffix(CARD_COPY).map(|name| format!("{name}{ending}?{CARD_COPIES_EDITION}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{CARD_AVIF, MINI_COPY};

    #[test]
    fn a_cards_copies_are_named_from_its_card_copy() {
        assert_eq!(card_copy("/media/photos/ab-card.webp", MINI_COPY).as_deref(), Some("/media/photos/ab-mini.webp?2"));
        assert_eq!(card_copy("https://cdn.test/photos/ab-card.webp", CARD_AVIF).as_deref(), Some("https://cdn.test/photos/ab-card.avif?2"));
        assert_eq!(card_copy("/media/photos/ab-card.jpg", MINI_COPY), None, "a photo kept before WebP");
    }

    #[test]
    fn too_many_photos_or_one_too_big_is_said() {
        let photo = || Bytes::from_static(b"photo");
        assert_eq!(check(&vec![photo(); MAX_PHOTOS], 0), None);
        assert_eq!(check(&[photo()], MAX_PHOTOS).as_deref(), Some("Një njoftim mund të ketë deri në 8 foto."));
        assert_eq!(check(&[Bytes::from(vec![0; MAX_UPLOAD_BYTES + 1])], 0).as_deref(), Some("Secila foto mund të jetë deri në 15 MB."));
        assert_eq!(unread(StatusCode::PAYLOAD_TOO_LARGE), "Fotot janë shumë të mëdha. Zgjidh më pak, ose më të vogla.");
        assert_eq!(unread(StatusCode::BAD_REQUEST), "Formulari nuk arriti i plotë. Provo përsëri.");
    }
}
