//! Listing photos: checked, turned the right way up, resized and re-encoded here, then kept in an
//! S3-compatible store. Locally that is Garage (compose.yaml); in production Cloudflare R2, or any
//! S3 service, with the same code and other values in .env (see .env.example).
//!
//! Every photo is decoded and written out again as JPEG, which also drops what phones keep inside
//! them, the place a photo was taken included. Two sizes are kept: the full one for the listing
//! page and a smaller one for cards. The site serves both itself, from /media/..., so they work
//! wherever the site does.

use std::{io::Cursor, sync::Arc};

use anyhow::Context;
use axum::body::Bytes;
use image::{
    DynamicImage, ImageDecoder, ImageReader, Limits, codecs::jpeg::JpegEncoder, imageops::FilterType,
    metadata::Orientation,
};
use object_store::{ObjectStore, PutPayload, aws::AmazonS3Builder, memory::InMemory, path::Path};
use sha2::{Digest, Sha256};

/// Photos per listing.
pub const MAX_PHOTOS: usize = 8;
/// One photo as sent. Phones send them smaller: photos_controller.js shrinks them first.
pub const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
/// The longest side of the full photo, and of the card's.
const FULL_SIDE: u32 = 1600;
const CARD_SIDE: u32 = 600;
const JPEG_QUALITY: u8 = 82;

/// Where photos are kept.
#[derive(Clone)]
pub struct Media {
    store: Arc<dyn ObjectStore>,
}

impl Media {
    /// The S3 store named in the environment: S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and
    /// S3_SECRET_ACCESS_KEY, S3_REGION if the service wants one ("auto" for R2). Without them,
    /// photos are kept in memory, which is enough for the tests; the server then says so.
    pub fn from_env() -> Self {
        let env = |name: &str| std::env::var(name).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
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
                    Ok(store) => Self { store: Arc::new(store) },
                    Err(error) => {
                        tracing::error!(%error, "the photo store isn't set up right; photos are kept in memory only");
                        Self::in_memory()
                    }
                }
            }
            _ => {
                tracing::warn!("no S3_* settings: photos are kept in memory only, and lost when the server stops");
                Self::in_memory()
            }
        }
    }

    pub fn in_memory() -> Self {
        Self { store: Arc::new(InMemory::new()) }
    }

    pub async fn put(&self, key: &str, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.store.put(&Path::from(key), PutPayload::from(bytes)).await.with_context(|| format!("storing {key}"))?;
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

/// A photo ready to keep: the two JPEGs, the full one's size, and a fingerprint of what was sent.
pub struct Processed {
    pub full: Vec<u8>,
    pub card: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub sha256: [u8; 32],
}

/// Not a photo this site reads: not JPEG, PNG or WebP, damaged, or absurdly large.
#[derive(Debug)]
pub struct NotAPhoto;

/// Decodes a photo, turns it the way the camera held it, and writes the two sizes. Slow work: run
/// it off the async threads (tokio::task::spawn_blocking).
pub fn process(bytes: &[u8]) -> Result<Processed, NotAPhoto> {
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

    let full = shrink(image, FULL_SIDE);
    let card = shrink(full.clone(), CARD_SIDE);
    Ok(Processed {
        width: full.width(),
        height: full.height(),
        full: jpeg(&full)?,
        card: jpeg(&card)?,
        sha256: Sha256::digest(bytes).into(),
    })
}

/// No side longer than `side`; smaller photos stay as they are.
fn shrink(image: DynamicImage, side: u32) -> DynamicImage {
    if image.width() <= side && image.height() <= side {
        image
    } else {
        image.resize(side, side, FilterType::Triangle)
    }
}

fn jpeg(image: &DynamicImage) -> Result<Vec<u8>, NotAPhoto> {
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(&image.to_rgb8()).map_err(|_| NotAPhoto)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small JPEG, as a camera might send: wider than tall.
    fn sample_jpeg(width: u32, height: u32) -> Vec<u8> {
        let image = DynamicImage::new_rgb8(width, height);
        jpeg(&image).unwrap()
    }

    #[test]
    fn photos_come_out_in_two_sizes() {
        let processed = process(&sample_jpeg(3000, 2000)).unwrap();
        assert_eq!((processed.width, processed.height), (1600, 1067));
        let card = image::load_from_memory(&processed.card).unwrap();
        assert_eq!((card.width(), card.height()), (600, 400));
        // Small photos aren't made bigger.
        let small = process(&sample_jpeg(400, 300)).unwrap();
        assert_eq!((small.width, small.height), (400, 300));
    }

    #[test]
    fn anything_else_is_refused() {
        assert!(process(b"not a photo at all").is_err());
        assert!(process(&sample_jpeg(800, 600)[..200]).is_err());
    }

    #[tokio::test]
    async fn the_store_keeps_and_forgets() {
        let media = Media::in_memory();
        media.put("photos/a.jpg", vec![1, 2, 3]).await.unwrap();
        assert_eq!(media.get("photos/a.jpg").await.unwrap().as_deref(), Some(&[1, 2, 3][..]));
        media.delete("photos/a.jpg").await.unwrap();
        assert!(media.get("photos/a.jpg").await.unwrap().is_none());
        media.delete("photos/a.jpg").await.unwrap();
    }
}
