//! Listing photos: checked, turned the right way up, resized and re-encoded here, then kept in an
//! S3-compatible store. Locally that is Garage (compose.yaml); in production Cloudflare R2, or any
//! S3 service, with the same code and other values in .env (see .env.example).
//!
//! Every photo is decoded and written out again, which also drops what phones keep inside them,
//! the place a photo was taken included. It is kept as WebP, about a third lighter than JPEG at the
//! same quality, in three sizes: the full one for the listing page on big screens, a middle one for
//! phones and big cards, and the card's. Pages name all three and the browser picks the one its
//! screen needs (srcset). A JPEG of the full one, by mozjpeg, is for what doesn't show WebP: link
//! previews, and old browsers. A tiny copy, a few hundred bytes, goes in the pages themselves (a
//! data: URI), blurred behind the photo while it loads and around it where it doesn't fill its
//! frame.
//!
//! The site serves the files itself, from /media/..., so they work wherever the site does; or a CDN
//! does, from the address MEDIA_URL gives (README, "Photos"). Each is stored with its type and a
//! year's caching, which the store, or a CDN in front of it, sends with it.

use std::{io::Cursor, sync::Arc};

use anyhow::Context;
use axum::body::Bytes;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits, codecs::jpeg::JpegEncoder, imageops::FilterType,
    metadata::Orientation,
};
use object_store::{
    Attribute, AttributeValue, Attributes, ObjectStore, PutOptions, PutPayload, aws::AmazonS3Builder,
    memory::InMemory, path::Path,
};
use sha2::{Digest, Sha256};

/// Photos per listing.
pub const MAX_PHOTOS: usize = 8;
/// One photo as sent. Phones send them smaller: photos_controller.js shrinks them first.
pub const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
/// The longest side of each copy: the full photo, the middle one, the card's, and the tiny one in
/// the pages.
pub const FULL_SIDE: u32 = 1600;
pub const MIDDLE_SIDE: u32 = 1200;
pub const CARD_SIDE: u32 = 600;
const PLACEHOLDER_SIDE: u32 = 16;
/// libwebp's quality, out of 100, for the copies and for the tiny one, which is blurred anyway;
/// mozjpeg's for the JPEG, which with its trellis quantisation looks as a plain encoder's 85 does.
const WEBP_QUALITY: f32 = 80.0;
const PLACEHOLDER_QUALITY: f32 = 40.0;
const JPEG_QUALITY: f32 = 80.0;
/// The AI's copies (small_copy) are made as quickly as they can be, by the image crate's encoder.
const QUICK_JPEG_QUALITY: u8 = 82;
/// A stored file never changes: a new photo gets new names. Browsers and CDNs may keep it a year.
pub const CACHE_FOR_A_YEAR: &str = "public, max-age=31536000, immutable";

/// Where photos are kept, and the address pages give for them.
#[derive(Clone)]
pub struct Media {
    store: Arc<dyn ObjectStore>,
    /// /media, which the site serves itself (photos::serve), or a CDN's address (MEDIA_URL), with
    /// no slash at the end.
    base: Arc<str>,
}

impl Media {
    /// The S3 store named in the environment: S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and
    /// S3_SECRET_ACCESS_KEY, S3_REGION if the service wants one ("auto" for R2). Without them,
    /// photos are kept in memory, which is enough for the tests; the server then says so. Pages
    /// give the photos' addresses under MEDIA_URL, a CDN's, when it is set, else under /media.
    pub fn from_env() -> Self {
        let env = |name: &str| std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
        let store: Arc<dyn ObjectStore> =
            match (env("S3_ENDPOINT"), env("S3_BUCKET"), env("S3_ACCESS_KEY_ID"), env("S3_SECRET_ACCESS_KEY")) {
                (Some(endpoint), Some(bucket), Some(key_id), Some(secret)) => {
                    let store = AmazonS3Builder::new()
                        .with_endpoint(endpoint)
                        .with_bucket_name(bucket)
                        .with_access_key_id(key_id)
                        .with_secret_access_key(secret)
                        .with_region(env("S3_REGION").unwrap_or_else(|| "auto".to_string()))
                        .with_allow_http(true)
                        .with_virtual_hosted_style_request(false)
                        .build();
                    match store {
                        Ok(store) => Arc::new(store),
                        Err(error) => {
                            tracing::error!(%error, "the photo store isn't set up right; photos are kept in memory only");
                            Arc::new(InMemory::new())
                        }
                    }
                }
                _ => {
                    tracing::warn!("no S3_* settings: photos are kept in memory only, and lost when the server stops");
                    Arc::new(InMemory::new())
                }
            };
        let base = env("MEDIA_URL").map_or_else(|| "/media".to_string(), |url| url.trim_end_matches('/').to_string());
        Self { store, base: base.into() }
    }

    pub fn in_memory() -> Self {
        Self { store: Arc::new(InMemory::new()), base: "/media".into() }
    }

    /// The address pages give for the file kept under `key`.
    pub fn url(&self, key: &str) -> String {
        format!("{}/{key}", self.base)
    }

    /// Keeps the file with its type and a year's caching, which the store sends with it whenever
    /// it serves it, to a CDN in front of it say.
    pub async fn put(&self, key: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        let attributes = Attributes::from_iter([
            (Attribute::ContentType, AttributeValue::from(content_type(key))),
            (Attribute::CacheControl, AttributeValue::from(CACHE_FOR_A_YEAR)),
        ]);
        let options = PutOptions { attributes, ..PutOptions::default() };
        self.store
            .put_opts(&Path::from(key), PutPayload::from(bytes), options)
            .await
            .with_context(|| format!("storing {key}"))?;
        Ok(())
    }

    /// The file, or None when there is none by that name.
    pub async fn get(&self, key: &str) -> anyhow::Result<Option<Bytes>> {
        match self.store.get(&Path::from(key)).await {
            Ok(result) => Ok(Some(result.bytes().await.with_context(|| format!("reading {key}"))?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(error) => Err(error).with_context(|| format!("reading {key}")),
        }
    }

    /// Gone already is fine.
    pub async fn delete(&self, key: &str) -> anyhow::Result<()> {
        match self.store.delete(&Path::from(key)).await {
            Ok(()) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(error).with_context(|| format!("deleting {key}")),
        }
    }
}

/// A stored file's type, from its name.
pub fn content_type(key: &str) -> &'static str {
    if key.ends_with(".webp") { "image/webp" } else { "image/jpeg" }
}

/// A photo ready to keep: its WebP copies (no middle one for a photo no bigger than that), the
/// full one as a JPEG too, the tiny one as a data: URI, the full one's size, and a fingerprint of
/// what was sent.
pub struct Processed {
    pub full: Vec<u8>,
    pub middle: Option<Vec<u8>>,
    pub card: Vec<u8>,
    pub jpeg: Vec<u8>,
    pub placeholder: String,
    pub width: u32,
    pub height: u32,
    pub sha256: [u8; 32],
}

/// Not a photo this site reads: not JPEG, PNG or WebP, damaged, or absurdly large.
#[derive(Debug)]
pub struct NotAPhoto;

/// Decodes a photo, turns it the way the camera held it, and writes its copies, each smaller one
/// from the one before. Slow work: run it off the async threads (tokio::task::spawn_blocking).
pub fn process(bytes: &[u8]) -> Result<Processed, NotAPhoto> {
    let full = DynamicImage::from(shrink(decode(bytes)?, FULL_SIDE).into_rgb8());
    let middle = (full.width().max(full.height()) > MIDDLE_SIDE).then(|| shrink(full.clone(), MIDDLE_SIDE));
    let card = shrink(middle.clone().unwrap_or_else(|| full.clone()), CARD_SIDE);
    let placeholder = webp(&shrink(card.clone(), PLACEHOLDER_SIDE), PLACEHOLDER_QUALITY)?;
    Ok(Processed {
        full: webp(&full, WEBP_QUALITY)?,
        middle: middle.map(|middle| webp(&middle, WEBP_QUALITY)).transpose()?,
        card: webp(&card, WEBP_QUALITY)?,
        jpeg: jpeg(&full)?,
        placeholder: format!("data:image/webp;base64,{}", BASE64.encode(placeholder)),
        width: full.width(),
        height: full.height(),
        sha256: Sha256::digest(bytes).into(),
    })
}

/// A JPEG copy no longer than `side` on its longer side, turned the way the camera held it: what
/// the AI looks at when it fills in the listing form (web/ai_fill.rs). Slow work too.
pub fn small_copy(bytes: &[u8], side: u32) -> Result<Vec<u8>, NotAPhoto> {
    quick_jpeg(&shrink(decode(bytes)?, side))
}

/// The photo, decoded within sane limits and turned the way the camera held it.
fn decode(bytes: &[u8]) -> Result<DynamicImage, NotAPhoto> {
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|_| NotAPhoto)?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(12_000);
    limits.max_image_height = Some(12_000);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| NotAPhoto)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| NotAPhoto)?;
    image.apply_orientation(orientation);
    Ok(image)
}

/// No side longer than `side`; smaller photos stay as they are.
fn shrink(image: DynamicImage, side: u32) -> DynamicImage {
    if image.width() <= side && image.height() <= side {
        image
    } else {
        image.resize(side, side, FilterType::Triangle)
    }
}

/// Lossy WebP, by libwebp.
fn webp(image: &DynamicImage, quality: f32) -> Result<Vec<u8>, NotAPhoto> {
    let rgb = image.to_rgb8();
    let width = i32::try_from(rgb.width()).map_err(|_| NotAPhoto)?;
    let height = i32::try_from(rgb.height()).map_err(|_| NotAPhoto)?;
    let mut out = std::ptr::null_mut();
    // SAFETY: the pixels are `height` rows of `width` RGB pixels, three bytes each, one row after
    // the other. libwebp allocates what it writes, which is copied here and then freed by libwebp;
    // it leaves `out` null when it fails.
    unsafe {
        let len = libwebp_sys::WebPEncodeRGB(rgb.as_ptr(), width, height, width * 3, quality, &mut out);
        if out.is_null() {
            return Err(NotAPhoto);
        }
        let encoded = std::slice::from_raw_parts(out, len).to_vec();
        libwebp_sys::WebPFree(out.cast());
        if encoded.is_empty() { Err(NotAPhoto) } else { Ok(encoded) }
    }
}

/// A progressive JPEG, by mozjpeg: about a tenth lighter than a plain encoder's at the same
/// quality, and it shows whole, if blurred, before it has all arrived. mozjpeg reports an error by
/// unwinding, which stops here.
fn jpeg(image: &DynamicImage) -> Result<Vec<u8>, NotAPhoto> {
    let rgb = image.to_rgb8();
    std::panic::catch_unwind(|| -> std::io::Result<Vec<u8>> {
        let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        compress.set_size(rgb.width() as usize, rgb.height() as usize);
        compress.set_quality(JPEG_QUALITY);
        compress.set_progressive_mode();
        let mut started = compress.start_compress(Vec::new())?;
        started.write_scanlines(rgb.as_raw())?;
        started.finish()
    })
    .map_err(|_| NotAPhoto)?
    .map_err(|_| NotAPhoto)
}

/// A plain JPEG, quickly.
fn quick_jpeg(image: &DynamicImage) -> Result<Vec<u8>, NotAPhoto> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, QUICK_JPEG_QUALITY).encode_image(&image.to_rgb8()).map_err(|_| NotAPhoto)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small JPEG, as a camera might send: wider than tall.
    fn sample_jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::new_rgb8(width, height);
        quick_jpeg(&image).unwrap()
    }

    /// A WebP's or a JPEG's size, and which it is.
    fn size_of(bytes: &[u8]) -> (u32, u32, image::ImageFormat) {
        let image = image::load_from_memory(bytes).unwrap();
        (image.width(), image.height(), image::guess_format(bytes).unwrap())
    }

    #[test]
    fn photos_come_out_as_webp_in_three_sizes_with_a_jpeg_and_a_tiny_copy() {
        use image::ImageFormat::{Jpeg, WebP};
        let processed = process(&sample_jpeg(3000, 2000)).unwrap();
        assert_eq!((processed.width, processed.height), (1600, 1067));
        assert_eq!(size_of(&processed.full), (1600, 1067, WebP));
        assert_eq!(size_of(processed.middle.as_deref().unwrap()), (1200, 800, WebP));
        assert_eq!(size_of(&processed.card), (600, 400, WebP));
        assert_eq!(size_of(&processed.jpeg), (1600, 1067, Jpeg));
        // mozjpeg's, progressive: its frame starts with 0xFFC2 rather than a baseline's 0xFFC0.
        assert!(processed.jpeg.windows(2).any(|marker| marker == [0xFF, 0xC2]));
        let tiny = processed.placeholder.strip_prefix("data:image/webp;base64,").unwrap();
        assert_eq!(size_of(&BASE64.decode(tiny).unwrap()), (16, 11, WebP));
        assert!(processed.placeholder.len() < 400, "{}", processed.placeholder.len());
        // Small photos aren't made bigger, and need no middle copy.
        let small = process(&sample_jpeg(1000, 750)).unwrap();
        assert_eq!((small.width, small.height), (1000, 750));
        assert!(small.middle.is_none());
        assert_eq!(size_of(&small.card), (600, 450, WebP));
        let tiny = process(&sample_jpeg(400, 300)).unwrap();
        assert_eq!(size_of(&tiny.card), (400, 300, WebP));
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(process(b"not a photo at all").is_err());
        assert!(process(&sample_jpeg(800, 600)[..200]).is_err());
    }

    #[tokio::test]
    async fn the_store_keeps_and_forgets() {
        let media = Media::in_memory();
        media.put("photos/a.webp", vec![1, 2, 3]).await.unwrap();
        assert_eq!(media.get("photos/a.webp").await.unwrap().as_deref(), Some(&[1, 2, 3][..]));
        // With its type and caching, for whatever serves it.
        let attributes = media.store.get(&Path::from("photos/a.webp")).await.unwrap().attributes;
        assert_eq!(attributes.get(&Attribute::ContentType).map(AsRef::as_ref), Some("image/webp"));
        assert_eq!(attributes.get(&Attribute::CacheControl).map(AsRef::as_ref), Some(CACHE_FOR_A_YEAR));
        assert_eq!(media.url("photos/a.webp"), "/media/photos/a.webp");
        media.delete("photos/a.webp").await.unwrap();
        assert!(media.get("photos/a.webp").await.unwrap().is_none());
        media.delete("photos/a.webp").await.unwrap();
    }
}
