//! Listing photos: checked, turned the right way up, resized and re-encoded here, then kept in an
//! S3-compatible store. Locally that is Garage (compose.yaml); in production Cloudflare R2, or any
//! S3 service, with the same code and other values in .env (see .env.example).
//!
//! Every photo is decoded and written out again, which also drops what phones keep inside them,
//! the place a photo was taken included. It is kept as WebP, about a third lighter than JPEG at the
//! same quality, in five sizes: the full one for the listing page on big screens, a middle one for
//! phones and big cards, and three for the cards: the card's, for dense screens, a small one, and
//! a mini one for cards on a plain screen. The first two are sized by their longer side, for a
//! photo shown whole; the cards' three by their shorter one, for the square a card cuts a photo
//! to, which scales it by that side (page.css, object-fit: cover): a card's copy of a 3:2 photo is
//! 900 × 600, a small one's 600 × 400, a mini one's 450 × 300. The cards' three are AVIF too, a
//! fifth lighter again, for the browsers that show it; a photo full of fine texture, whose AVIF
//! copy comes out heavy, has it made at a lower quality (AVIF_BUDGET). Pages name them all and the
//! browser picks the one its screen needs (srcset, web/photos.rs). A JPEG of the full one, by
//! mozjpeg, is for what doesn't show WebP: link previews, and old browsers. A tiny copy, a few
//! hundred bytes, goes in the pages themselves (a data: URI), blurred behind the photo while it
//! loads and around it where it doesn't fill its frame.
//!
//! Photos kept before there were small, mini or AVIF copies get theirs from their WebP ones, and
//! those kept before the cards' copies were made as they are now (CARD_COPIES_EDITION: sized by
//! their longer side, 600 × 400 for a 3:2 photo, which a card stretched; or heavier than
//! AVIF_BUDGET) get theirs made again: all of them in the background as the server starts, by one
//! server at a time (Media::fill_in_copies, jobs.rs); before then a WebP one asked for is made as
//! it is (Media::make_copy), and an AVIF one's WebP twin stands in (web/photos.rs, serve).
//!
//! The site serves the files itself, from /media/..., so they work wherever the site does; or a CDN
//! does, from the address MEDIA_URL gives (README, "Photos"). Each is stored with its type and a
//! year's caching, which the store, or a CDN in front of it, sends with it.

use std::{collections::HashMap, io::Cursor, sync::Arc};

use anyhow::Context;
use axum::body::Bytes;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use futures_util::TryStreamExt;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader, Limits, RgbaImage,
    codecs::{avif::AvifEncoder, jpeg::JpegEncoder},
    imageops::FilterType,
    metadata::Orientation,
};
use object_store::{
    Attribute, AttributeValue, Attributes, ObjectStore, PutOptions, PutPayload, aws::AmazonS3Builder,
    memory::InMemory, path::Path,
};
use sha2::{Digest, Sha256};

use crate::config::Config;

/// Photos per listing.
pub const MAX_PHOTOS: usize = 8;
/// One photo as sent. Phones send them smaller: photos_controller.js shrinks them first.
pub const MAX_UPLOAD_BYTES: usize = 15 * 1024 * 1024;
/// The longer side of the full copy and the middle one, which show whole, and of the tiny one in
/// the pages (fit_size).
pub const FULL_SIDE: u32 = 1600;
pub const MIDDLE_SIDE: u32 = 1200;
const PLACEHOLDER_SIDE: u32 = 16;
/// The shorter side of the card's copy, the small one and the mini one: the square each covers
/// at full sharpness, as a card cuts a photo to one (cover_size). Their longer side is no more
/// than twice that, for a panorama. The card's serves a dense screen (a card of 161 px on a phone
/// of three pixels to a point, 255 on a laptop of two), the small one a middling one (a phone's
/// two to a point, a laptop's 1.25), the mini one a plain screen (a card of 255 or 300 px at one
/// pixel to a point), each with the pixels it needs and not many more.
pub const CARD_SIDE: u32 = 600;
pub const SMALL_SIDE: u32 = 400;
pub const MINI_SIDE: u32 = 300;
/// How many photos' copies the server's start makes at once (make_missing_copies): their AVIF
/// copies take most of a second each.
const AT_ONCE: usize = 4;
/// The rules the cards' copies are made by, numbered: 2 since 2026-10-09, when they came to be
/// sized by their shorter side, with a mini one and a weight an AVIF one may not go over. Pages
/// name the cards' copies with it (web/photos.rs, CARD_COPIES_EDITION), since the copies were
/// made again under their old names, which browsers keep for a year; and a file in the store,
/// `scan_note`, says the start has looked at every photo's copies and brought them to it, so that
/// later starts needn't read them all again. Change it whenever the rules change.
pub const CARD_COPIES_EDITION: &str = "2";

fn scan_note() -> String {
    format!("photos/card-copies-edition-{CARD_COPIES_EDITION}.txt")
}
/// What the cards' copies add to a photo's name, as WebP and as AVIF (COPIES): the others' names
/// are the card's WebP one's with their own ending.
pub const CARD_COPY: &str = "-card.webp";
pub const SMALL_COPY: &str = "-small.webp";
pub const MINI_COPY: &str = "-mini.webp";
pub const CARD_AVIF: &str = "-card.avif";
pub const SMALL_AVIF: &str = "-small.avif";
pub const MINI_AVIF: &str = "-mini.avif";
/// What each of a photo's files adds to its name (photos/ and 32 hex digits): the full WebP, whose
/// key listing_media keeps, the middle one, the card's, the small one, the mini one, those three
/// as AVIF, and the JPEG. The small, mini and AVIF ones' addresses are the card's with their own
/// ending (pluribus-web's photos.rs: card_copy_url, srcset, avif_srcset), so listing_media keeps
/// none. Photos kept before WebP (migration 0025) were a JPEG, whose key it kept, and a JPEG for
/// the card.
pub const COPIES: [&str; 9] = [".webp", "-middle.webp", CARD_COPY, SMALL_COPY, MINI_COPY, CARD_AVIF, SMALL_AVIF, MINI_AVIF, ".jpg"];
const EARLIER_CARD: &str = "-card.jpg";
/// libwebp's quality, out of 100, for the copies and for the tiny one, which is blurred anyway;
/// mozjpeg's for the JPEG, which with its trellis quantisation looks as a plain encoder's 85 does.
const WEBP_QUALITY: f32 = 80.0;
/// rav1e's quality for the cards' AVIF copies, out of 100, where they matched libwebp's 80 over
/// twelve of the site's photos: as close to them by SSIM (closer for half, less for the other
/// half), closer by PSNR, and about a fifth lighter (a card's 13.3 KB rather than 17.2, a small
/// one's 6.9 rather than 8.9). Its speed, from 1 (slowest) to 10: at 6 a card's takes about 0.45 s
/// and a small one's 0.2, without the assembly NASM would build (WebP takes a hundredth of that);
/// at 10, a third of the time, the files are a fifth heavier.
const AVIF_QUALITY: u8 = 78;
const AVIF_SPEED: u8 = 6;
/// What an AVIF copy may weigh at AVIF_QUALITY, in bytes a pixel: a photo full of fine texture
/// (gravel, leaves, an engine) comes out four or five times as heavy as a plain one at the same
/// quality (0.18 bytes a pixel against 0.04), and its texture hides what a lower quality takes
/// away, so a copy heavier than this is made again at AVIF_TEXTURE_QUALITY, about three tenths
/// lighter (over five of the site's photos, 72 took a tenth off 78's bytes, 66 a fifth, 60 three
/// tenths and 55 four tenths; a textured photo's 0.18 bytes a pixel came to 0.13 at 60). Lighthouse
/// holds a photo to about 0.17 bytes over the pixels a card shows of it, the middle square.
const AVIF_BUDGET: f64 = 0.14;
const AVIF_TEXTURE_QUALITY: u8 = 60;
const PLACEHOLDER_QUALITY: f32 = 40.0;
const JPEG_QUALITY: f32 = 80.0;
/// A store's logo (web/stores.rs): the middle square of the picture sent, this many pixels a side;
/// and its cover, a band three times as wide as it is tall, this wide at most. Kept under
/// stores/, apart from the listings' photos, whose copies the server's start looks over.
pub const LOGO_SIDE: u32 = 320;
pub const COVER_WIDTH: u32 = 1500;
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
    /// The S3 store the settings name (config.rs): S3_ENDPOINT, S3_BUCKET, S3_ACCESS_KEY_ID and
    /// S3_SECRET_ACCESS_KEY, in S3_REGION if the service wants one ("auto" for R2). Without them,
    /// photos are kept in memory, which is enough for the tests; the server then says so. Pages
    /// give the photos' addresses under MEDIA_URL, a CDN's, when it is set, else under /media.
    pub fn from_config(config: &Config) -> Self {
        let store: Arc<dyn ObjectStore> =
            match (&config.s3_endpoint, &config.s3_bucket, &config.s3_access_key_id, &config.s3_secret_access_key) {
                (Some(endpoint), Some(bucket), Some(key_id), Some(secret)) => {
                    let store = AmazonS3Builder::new()
                        .with_endpoint(endpoint)
                        .with_bucket_name(bucket)
                        .with_access_key_id(key_id)
                        .with_secret_access_key(secret)
                        .with_region(&config.s3_region)
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
        Self { store, base: config.media_url.as_str().into() }
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

    /// The copy kept under `key`, made now and kept, for a photo kept before there were copies of
    /// its kind, or whose copy is from before the rules it is made by now: one of LATER_COPIES,
    /// from its full WebP copy as `process` makes them, or from the card's when it has no full
    /// one. None when the name is no such copy's, or there is no such photo. Another asking for it
    /// at the same time makes the same file.
    pub async fn make_copy(&self, key: &str) -> anyhow::Result<Option<Bytes>> {
        let Some((name, ending)) =
            LATER_COPIES.into_iter().find_map(|(ending, _)| Some((key.strip_suffix(ending)?, ending)))
        else {
            return Ok(None);
        };
        let from = match self.get(&format!("{name}.webp")).await? {
            Some(full) => full,
            None => match self.get(&format!("{name}{CARD_COPY}")).await? {
                Some(card) => card,
                None => return Ok(None),
            },
        };
        let made = tokio::task::spawn_blocking(move || copy_from(&from, ending))
            .await?
            .map_err(|_| anyhow::anyhow!("{name}'s copies aren't a photo"))?;
        self.put(key, made.clone()).await?;
        Ok(Some(Bytes::from(made)))
    }

    /// Makes the copies the photos in the store lack (make_copy), and makes again the card copies
    /// from before the rules they are made by now (remake_card_copies): the photos kept before
    /// there were copies of their kind, or the rules. AT_ONCE photos at a time. Once every photo
    /// has been looked at, a file in the store says so (scan_note), so that later starts needn't
    /// read their card copies again; a start cut short before then takes up where it left. How
    /// many copies it made.
    pub async fn make_missing_copies(&self) -> anyhow::Result<usize> {
        let kept: HashMap<String, u64> = self
            .store
            .list(Some(&Path::from("photos")))
            .map_ok(|meta| (meta.location.to_string(), meta.size))
            .try_collect()
            .await
            .context("listing the photos")?;
        let scanning = !kept.contains_key(&scan_note());
        let names: Vec<String> = kept.keys().filter_map(|key| key.strip_suffix(CARD_COPY)).map(str::to_string).collect();
        let kept = Arc::new(kept);
        let mut tasks = tokio::task::JoinSet::new();
        let mut made = 0;
        for name in names {
            if tasks.len() >= AT_ONCE {
                made += tasks.join_next().await.expect("a photo's copies are being made")?;
            }
            let (media, kept) = (self.clone(), Arc::clone(&kept));
            tasks.spawn(async move { media.fill_in(&name, &kept, scanning).await });
        }
        while let Some(done) = tasks.join_next().await {
            made += done?;
        }
        if scanning {
            let note = format!(
                "Every photo's card copies have been looked at, and those from before edition {CARD_COPIES_EDITION} of the rules they are made by (media.rs) made again.\n"
            );
            self.put(&scan_note(), note.into_bytes()).await?;
        }
        Ok(made)
    }

    /// One photo's copies, for make_missing_copies: those from before the rules made again when
    /// the start is looking (`scanning`), then those it lacks made. How many were made; one that
    /// can't be is logged, and leaves the others to be.
    async fn fill_in(&self, name: &str, kept: &HashMap<String, u64>, scanning: bool) -> usize {
        let mut made = 0;
        if scanning {
            match self.remake_card_copies(name, kept).await {
                Ok((remade, true)) => return remade,
                Ok((remade, false)) => made += remade,
                Err(error) => tracing::warn!(%error, "a photo's card copies could not be made again"),
            }
        }
        for (ending, _) in LATER_COPIES {
            let key = format!("{name}{ending}");
            if kept.contains_key(&key) {
                continue;
            }
            match self.make_copy(&key).await {
                Ok(Some(_)) => made += 1,
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "a photo's copy could not be made"),
            }
        }
        made
    }

    /// A photo's card copies from before the rules they are made by now, made again and kept: all
    /// six from the full copy (card_copies) when the card copy is sized by its longer side (CARD_SIDE,
    /// and its shorter side less), which the first bytes of the card copy tell, and then true, for
    /// nothing else needs making; else each AVIF one heavier than AVIF_BUDGET on its own
    /// (make_copy), and false. How many were made. Nothing for a photo without a full copy to make
    /// them from, or whose card copy can't be read.
    async fn remake_card_copies(&self, name: &str, kept: &HashMap<String, u64>) -> anyhow::Result<(usize, bool)> {
        let card_key = format!("{name}{CARD_COPY}");
        let head = self
            .store
            .get_range(&Path::from(card_key.as_str()), 0..WEBP_HEADER as u64)
            .await
            .with_context(|| format!("reading {card_key}"))?;
        let Some((width, height)) = webp_dimensions(&head) else {
            return Ok((0, false));
        };
        if width.max(height) == CARD_SIDE && width.min(height) < CARD_SIDE {
            let Some(full) = self.get(&format!("{name}.webp")).await? else {
                return Ok((0, false));
            };
            let copies = tokio::task::spawn_blocking(move || card_copies(&full))
                .await?
                .map_err(|_| anyhow::anyhow!("{name}'s full copy isn't a photo"))?;
            let [small_key, mini_key, card_avif_key, small_avif_key, mini_avif_key] =
                [SMALL_COPY, MINI_COPY, CARD_AVIF, SMALL_AVIF, MINI_AVIF].map(|ending| format!("{name}{ending}"));
            tokio::try_join!(
                self.put(&card_key, copies.card),
                self.put(&small_key, copies.small),
                self.put(&mini_key, copies.mini),
                self.put(&card_avif_key, copies.card_avif),
                self.put(&small_avif_key, copies.small_avif),
                self.put(&mini_avif_key, copies.mini_avif),
            )?;
            return Ok((6, true));
        }
        let mut made = 0;
        for (ending, side) in LATER_COPIES.iter().filter(|(ending, _)| ending.ends_with(".avif")) {
            let key = format!("{name}{ending}");
            let Some(bytes) = kept.get(&key) else { continue };
            let (copy_width, copy_height) = cover_size(width, height, *side);
            if *bytes as f64 > f64::from(copy_width * copy_height) * AVIF_BUDGET {
                made += usize::from(self.make_copy(&key).await?.is_some());
            }
        }
        Ok((made, false))
    }

    /// make_missing_copies, what it made or its failure in the log: run in the background as a
    /// server starts, which the pages needn't wait for, by one server at a time, which leaves them
    /// to another making them already (jobs.rs, PHOTO_COPIES; this spawned its own task, from
    /// `pluribus_web::state`, until D2): until then, the first to ask for a small WebP copy has it
    /// made, and an AVIF one's WebP twin stands in for it (web/photos.rs, serve).
    pub async fn fill_in_copies(&self) {
        match self.make_missing_copies().await {
            Ok(0) => {}
            Ok(made) => tracing::info!(made, "made the copies of the photos kept before there were any of their kind"),
            Err(error) => tracing::warn!(%error, "could not make the copies photos lack"),
        }
    }
}

/// Deletes the files of photos taken off a listing, every copy, those of photos kept before WebP
/// included (pluribus-web's photos::delete, and the expiry's, for the drafts it forgets); a
/// store's logo or cover (`stores/`) is one file. A file left behind only takes space, so a
/// failure is logged rather than shown.
pub async fn delete_photos(media: &Media, keys: &[String]) {
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

/// How many of a WebP's first bytes tell its size (webp_dimensions).
const WEBP_HEADER: usize = 30;

/// A WebP's width and height, from its first bytes (WEBP_HEADER are enough): a lossy one's (VP8,
/// as libwebp writes the copies), a lossless one's (VP8L) or an extended one's (VP8X). None for
/// anything else.
pub fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < WEBP_HEADER || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return None;
    }
    let u16_at = |at: usize| u32::from(u16::from_le_bytes([bytes[at], bytes[at + 1]]));
    let u24_at = |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], 0]);
    match &bytes[12..16] {
        b"VP8 " => (bytes[23..26] == [0x9d, 0x01, 0x2a]).then(|| (u16_at(26) & 0x3fff, u16_at(28) & 0x3fff)),
        b"VP8L" => {
            let bits = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            (bytes[20] == 0x2f).then(|| ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => Some((u24_at(24) + 1, u24_at(27) + 1)),
        _ => None,
    }
}

/// A stored file's type, from its name.
pub fn content_type(key: &str) -> &'static str {
    if key.ends_with(".webp") {
        "image/webp"
    } else if key.ends_with(".avif") {
        "image/avif"
    } else if key.ends_with(".txt") {
        "text/plain"
    } else {
        "image/jpeg"
    }
}

/// A photo ready to keep: its WebP copies (no middle one for a photo no bigger than that), the
/// full one as a JPEG too, the tiny one as a data: URI, the full one's size, and a fingerprint of
/// what was sent.
pub struct Processed {
    pub full: Vec<u8>,
    pub middle: Option<Vec<u8>>,
    pub card: Vec<u8>,
    pub small: Vec<u8>,
    pub mini: Vec<u8>,
    pub card_avif: Vec<u8>,
    pub small_avif: Vec<u8>,
    pub mini_avif: Vec<u8>,
    pub jpeg: Vec<u8>,
    pub placeholder: String,
    pub width: u32,
    pub height: u32,
    pub sha256: [u8; 32],
}

/// Not a photo this site reads: not JPEG, PNG or WebP, damaged, or absurdly large.
#[derive(Debug)]
pub struct NotAPhoto;

/// Decodes a photo, turns it the way the camera held it, and writes its copies, each from the
/// full one. Slow work: run it off the async threads (tokio::task::spawn_blocking).
pub fn process(bytes: &[u8]) -> Result<Processed, NotAPhoto> {
    let full = DynamicImage::from(fit(decode(bytes)?, FULL_SIDE).into_rgb8());
    let middle = (full.width().max(full.height()) > MIDDLE_SIDE).then(|| fit(full.clone(), MIDDLE_SIDE));
    let card = cover(&full, CARD_SIDE);
    let small = cover(&full, SMALL_SIDE);
    let mini = cover(&full, MINI_SIDE);
    let placeholder = webp(&shrink(mini.clone(), PLACEHOLDER_SIDE), PLACEHOLDER_QUALITY)?;
    Ok(Processed {
        full: webp(&full, WEBP_QUALITY)?,
        middle: middle.map(|middle| webp(&middle, WEBP_QUALITY)).transpose()?,
        card: webp(&card, WEBP_QUALITY)?,
        small: webp(&small, WEBP_QUALITY)?,
        mini: webp(&mini, WEBP_QUALITY)?,
        card_avif: avif(&card)?,
        small_avif: avif(&small)?,
        mini_avif: avif(&mini)?,
        jpeg: jpeg(&full)?,
        placeholder: format!("data:image/webp;base64,{}", BASE64.encode(placeholder)),
        width: full.width(),
        height: full.height(),
        sha256: Sha256::digest(bytes).into(),
    })
}

/// A photo's six card copies, the card's, the small one and the mini one as WebP and as AVIF,
/// from one of its WebP copies, the full one, or the card's for a photo without one, as `process`
/// makes them: for the photos kept before the cards' copies were sized by their shorter side
/// (Media::remake_card_copies). Slow work too.
pub struct CardCopies {
    pub card: Vec<u8>,
    pub small: Vec<u8>,
    pub mini: Vec<u8>,
    pub card_avif: Vec<u8>,
    pub small_avif: Vec<u8>,
    pub mini_avif: Vec<u8>,
}

pub fn card_copies(webp_copy: &[u8]) -> Result<CardCopies, NotAPhoto> {
    let photo = decode(webp_copy)?;
    let card = cover(&photo, CARD_SIDE);
    let small = cover(&photo, SMALL_SIDE);
    let mini = cover(&photo, MINI_SIDE);
    Ok(CardCopies {
        card: webp(&card, WEBP_QUALITY)?,
        small: webp(&small, WEBP_QUALITY)?,
        mini: webp(&mini, WEBP_QUALITY)?,
        card_avif: avif(&card)?,
        small_avif: avif(&small)?,
        mini_avif: avif(&mini)?,
    })
}

/// The copies a photo kept before may lack, or have from before the rules they are made by now
/// (Media::make_copy, make_missing_copies): every card copy but the card's WebP one, which every
/// WebP photo has, with the square each covers.
const LATER_COPIES: [(&str, u32); 5] =
    [(MINI_COPY, MINI_SIDE), (SMALL_COPY, SMALL_SIDE), (MINI_AVIF, MINI_SIDE), (SMALL_AVIF, SMALL_SIDE), (CARD_AVIF, CARD_SIDE)];

/// The copy named by `ending` (LATER_COPIES) of a photo kept before there were copies of its kind
/// (Media::make_copy), from one of its WebP copies, the full one or the card's, as `process`
/// makes it. Slow work too.
pub fn copy_from(webp_copy: &[u8], ending: &str) -> Result<Vec<u8>, NotAPhoto> {
    let Some((_, side)) = LATER_COPIES.iter().find(|(name, _)| *name == ending) else {
        return Err(NotAPhoto);
    };
    let copy = cover(&decode(webp_copy)?, *side);
    if ending.ends_with(".avif") { avif(&copy) } else { webp(&copy, WEBP_QUALITY) }
}

/// A JPEG copy no longer than `side` on its longer side, turned the way the camera held it: what
/// the AI looks at when it fills in the listing form (web/ai_fill.rs). Slow work too.
pub fn small_copy(bytes: &[u8], side: u32) -> Result<Vec<u8>, NotAPhoto> {
    quick_jpeg(&shrink(decode(bytes)?, side))
}

/// A store's logo as WebP: the middle square of the picture, LOGO_SIDE a side (less for a smaller
/// one), what is see-through in it white, as the pages' light theme is. Slow work too.
pub fn logo(bytes: &[u8]) -> Result<Vec<u8>, NotAPhoto> {
    let image = on_white(decode(bytes)?);
    let side = image.width().min(image.height());
    let square = image.crop_imm((image.width() - side) / 2, (image.height() - side) / 2, side, side);
    let logo = if side > LOGO_SIDE { square.resize_exact(LOGO_SIDE, LOGO_SIDE, FilterType::Lanczos3) } else { square };
    webp(&logo, WEBP_QUALITY)
}

/// A store's cover as WebP: the middle band of the picture three times as wide as it is tall,
/// COVER_WIDTH wide at most. Slow work too.
pub fn store_cover(bytes: &[u8]) -> Result<Vec<u8>, NotAPhoto> {
    let image = on_white(decode(bytes)?);
    let (width, height) = (image.width(), image.height());
    let (band_width, band_height) = if width >= height * 3 { (height * 3, height) } else { (width, (width / 3).max(1)) };
    let band = image.crop_imm((width - band_width) / 2, (height - band_height) / 2, band_width, band_height);
    let cover = if band_width > COVER_WIDTH {
        band.resize_exact(COVER_WIDTH, COVER_WIDTH / 3, FilterType::Lanczos3)
    } else {
        band
    };
    webp(&cover, WEBP_QUALITY)
}

/// A picture with what is see-through in it laid on white: a logo's background, which WebP
/// written as RGB would otherwise turn black.
fn on_white(image: DynamicImage) -> DynamicImage {
    if !image.color().has_alpha() {
        return image;
    }
    let mut rgba = image.to_rgba8();
    for pixel in rgba.pixels_mut() {
        let alpha = u16::from(pixel[3]);
        for channel in 0..3 {
            pixel[channel] = ((u16::from(pixel[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8;
        }
        pixel[3] = 255;
    }
    DynamicImage::from(rgba)
}

/// The size of a copy of a `width` × `height` photo with no side longer than `side` (the full
/// copy, the middle one): the photo's own when it is no bigger.
pub fn fit_size(width: u32, height: u32, side: u32) -> (u32, u32) {
    scaled(width, height, f64::from(side) / f64::from(width.max(height)))
}

/// The size of a copy of a `width` × `height` photo that covers a square of `side` (the card's
/// copy, the small one): its shorter side `side`, unless the photo's is shorter, and its longer
/// side no more than twice that, for a panorama.
pub fn cover_size(width: u32, height: u32, side: u32) -> (u32, u32) {
    let by_shorter = f64::from(side) / f64::from(width.min(height));
    let by_longer = f64::from(2 * side) / f64::from(width.max(height));
    scaled(width, height, by_shorter.min(by_longer))
}

/// A photo's size scaled down by `scale`, to the nearest pixel; its own for a scale of one or more.
fn scaled(width: u32, height: u32, scale: f64) -> (u32, u32) {
    if scale >= 1.0 {
        return (width, height);
    }
    let side = |pixels: u32| (f64::from(pixels) * scale).round().max(1.0) as u32;
    (side(width), side(height))
}

/// The photo with no side longer than `side` (fit_size), resampled with a Lanczos window, which
/// keeps it sharp; one no bigger as it is.
fn fit(image: DynamicImage, side: u32) -> DynamicImage {
    let (width, height) = fit_size(image.width(), image.height(), side);
    if (width, height) == (image.width(), image.height()) { image } else { image.resize_exact(width, height, FilterType::Lanczos3) }
}

/// The photo covering a square of `side` (cover_size), as a card shows it, resampled with a
/// Lanczos window.
fn cover(image: &DynamicImage, side: u32) -> DynamicImage {
    let (width, height) = cover_size(image.width(), image.height(), side);
    if (width, height) == (image.width(), image.height()) {
        image.clone()
    } else {
        image.resize_exact(width, height, FilterType::Lanczos3)
    }
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

/// No side longer than `side`, quickly, for the tiny copy and the AI's; smaller photos stay as they
/// are.
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

/// Lossy AVIF, by rav1e, as ravif writes it (AVIF_QUALITY, AVIF_SPEED), or, for a copy that comes
/// out heavier than AVIF_BUDGET at that quality, a photo full of fine texture, at
/// AVIF_TEXTURE_QUALITY. Given as RGBA, opaque, which ravif writes without an alpha channel just
/// as it writes RGB: its RGB path is inlined into this crate, which the tests' builds don't
/// optimise, and there it took twenty times as long.
fn avif(image: &DynamicImage) -> Result<Vec<u8>, NotAPhoto> {
    let rgba = image.to_rgba8();
    let usual = avif_at(&rgba, AVIF_QUALITY)?;
    if usual.len() as f64 <= f64::from(rgba.width() * rgba.height()) * AVIF_BUDGET {
        return Ok(usual);
    }
    avif_at(&rgba, AVIF_TEXTURE_QUALITY)
}

fn avif_at(rgba: &RgbaImage, quality: u8) -> Result<Vec<u8>, NotAPhoto> {
    let mut out = Vec::new();
    AvifEncoder::new_with_speed_quality(&mut out, AVIF_SPEED, quality)
        .write_image(rgba.as_raw(), rgba.width(), rgba.height(), ExtendedColorType::Rgba8)
        .map_err(|_| NotAPhoto)?;
    Ok(out)
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

    /// An AVIF's size, from its image's "ispe" box, which the image crate can't read without a
    /// decoder of its own; None for what isn't an AVIF.
    fn avif_size(bytes: &[u8]) -> Option<(u32, u32)> {
        let brand = bytes.get(4..12)? == b"ftypavif";
        let at = bytes.windows(4).position(|window| window == b"ispe")? + 8;
        let number = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        brand.then(|| (number(at), number(at + 4)))
    }

    #[test]
    fn photos_come_out_as_webp_in_five_sizes_the_cards_as_avif_too_with_a_jpeg_and_a_tiny_copy() {
        use image::ImageFormat::{Jpeg, WebP};
        let processed = process(&sample_jpeg(3000, 2000)).unwrap();
        assert_eq!((processed.width, processed.height), (1600, 1067));
        assert_eq!(size_of(&processed.full), (1600, 1067, WebP));
        assert_eq!(size_of(processed.middle.as_deref().unwrap()), (1200, 800, WebP));
        // The cards' three cover a square of their shorter side.
        assert_eq!(size_of(&processed.card), (900, 600, WebP));
        assert_eq!(size_of(&processed.small), (600, 400, WebP));
        assert_eq!(size_of(&processed.mini), (450, 300, WebP));
        assert_eq!(avif_size(&processed.card_avif), Some((900, 600)));
        assert_eq!(avif_size(&processed.small_avif), Some((600, 400)));
        assert_eq!(avif_size(&processed.mini_avif), Some((450, 300)));
        assert_eq!(size_of(&processed.jpeg), (1600, 1067, Jpeg));
        // mozjpeg's, progressive: its frame starts with 0xFFC2 rather than a baseline's 0xFFC0.
        assert!(processed.jpeg.windows(2).any(|marker| marker == [0xFF, 0xC2]));
        let tiny = processed.placeholder.strip_prefix("data:image/webp;base64,").unwrap();
        assert_eq!(size_of(&BASE64.decode(tiny).unwrap()), (16, 11, WebP));
        assert!(processed.placeholder.len() < 400, "{}", processed.placeholder.len());
        // A tall photo the same way round.
        let tall = process(&sample_jpeg(2000, 3000)).unwrap();
        assert_eq!(size_of(&tall.card), (600, 900, WebP));
        assert_eq!(size_of(&tall.small), (400, 600, WebP));
        assert_eq!(size_of(&tall.mini), (300, 450, WebP));
        // Small photos aren't made bigger, and need no middle copy.
        let small = process(&sample_jpeg(1000, 750)).unwrap();
        assert_eq!((small.width, small.height), (1000, 750));
        assert!(small.middle.is_none());
        assert_eq!(size_of(&small.card), (800, 600, WebP));
        assert_eq!(size_of(&small.small), (533, 400, WebP));
        assert_eq!(size_of(&small.mini), (400, 300, WebP));
        let tiny = process(&sample_jpeg(400, 300)).unwrap();
        assert_eq!(size_of(&tiny.card), (400, 300, WebP));
        assert_eq!(size_of(&tiny.small), (400, 300, WebP));
        assert_eq!(size_of(&tiny.mini), (400, 300, WebP));
        // A panorama's longer side is no more than twice the square's.
        let wide = process(&sample_jpeg(4000, 1000)).unwrap();
        assert_eq!(size_of(&wide.card), (1200, 300, WebP));
        assert_eq!(size_of(&wide.small), (800, 200, WebP));
        assert_eq!(size_of(&wide.mini), (600, 150, WebP));
    }

    #[test]
    fn a_textured_photos_avif_copy_is_made_at_a_lower_quality() {
        use rand::Rng;
        // Noise is the finest texture there is: at the usual quality its copy comes out far
        // heavier than the budget, so it is made at the lower one, which takes bytes off; a plain
        // photo's stays at the usual quality.
        let mut rng = rand::rng();
        let noise = DynamicImage::ImageRgb8(image::RgbImage::from_fn(300, 200, |_, _| image::Rgb(rng.random())));
        let usual = avif_at(&noise.to_rgba8(), AVIF_QUALITY).unwrap();
        assert!(usual.len() as f64 > 300.0 * 200.0 * AVIF_BUDGET, "{}", usual.len());
        let budgeted = avif(&noise).unwrap();
        assert!(budgeted.len() < usual.len() * 9 / 10, "{} of {}", budgeted.len(), usual.len());
        assert_eq!(avif_size(&budgeted), Some((300, 200)));
        let plain = DynamicImage::new_rgb8(300, 200);
        assert_eq!(avif(&plain).unwrap(), avif_at(&plain.to_rgba8(), AVIF_QUALITY).unwrap());
    }

    #[test]
    fn a_copys_size_follows_from_the_photos() {
        assert_eq!(fit_size(3000, 2000, 1600), (1600, 1067));
        assert_eq!(fit_size(1000, 750, 1200), (1000, 750));
        assert_eq!(cover_size(1600, 1067, 600), (900, 600));
        assert_eq!(cover_size(1067, 1600, 400), (400, 600));
        assert_eq!(cover_size(1600, 1200, 400), (533, 400));
        assert_eq!(cover_size(800, 600, 600), (800, 600));
        assert_eq!(cover_size(1600, 400, 600), (1200, 300));
        assert_eq!(cover_size(600, 600, 600), (600, 600));
    }

    #[test]
    fn a_webps_size_is_in_its_first_bytes() {
        let lossy = webp(&DynamicImage::new_rgb8(900, 600), WEBP_QUALITY).unwrap();
        assert!(lossy.starts_with(b"RIFF") && &lossy[12..16] == b"VP8 ");
        assert_eq!(webp_dimensions(&lossy[..WEBP_HEADER]), Some((900, 600)));
        let mut lossless = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut lossless)
            .encode(&[0; 3 * 6], 3, 2, ExtendedColorType::Rgb8)
            .unwrap();
        assert_eq!(&lossless[12..16], b"VP8L");
        assert_eq!(webp_dimensions(&lossless), Some((3, 2)));
        let mut extended = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        extended.extend([0x83, 0x03, 0x00, 0x57, 0x02, 0x00]); // 900 - 1 and 600 - 1, three bytes each
        assert_eq!(webp_dimensions(&extended), Some((900, 600)));
        assert_eq!(webp_dimensions(&lossy[..20]), None);
        assert_eq!(webp_dimensions(b"not a photo at all, but thirty bytes."), None);
    }

    #[tokio::test]
    async fn photos_kept_before_there_were_small_or_avif_copies_get_theirs() {
        use image::ImageFormat::WebP;
        let media = Media::in_memory();
        let photo = process(&sample_jpeg(2000, 1500)).unwrap();
        // One kept before the small copies, its card copy from when the cards' copies were sized
        // by their longer side, with its full copy; one before the AVIF ones, with no full copy
        // left to make them from; one from the longer side's days with no full copy either, whose
        // card copy can't be made again; one that isn't a photo.
        let old_card = webp(&shrink(decode(&photo.full).unwrap(), CARD_SIDE), WEBP_QUALITY).unwrap();
        assert_eq!(size_of(&old_card), (600, 450, WebP));
        media.put("photos/old-card.webp", old_card.clone()).await.unwrap();
        media.put("photos/old.webp", photo.full.clone()).await.unwrap();
        media.put("photos/newer-card.webp", photo.card.clone()).await.unwrap();
        media.put("photos/newer-small.webp", vec![1, 2, 3]).await.unwrap();
        media.put("photos/lone-card.webp", old_card).await.unwrap();
        media.put("photos/broken-card.webp", b"not a photo".to_vec()).await.unwrap();
        // And one sized as they are now whose card's AVIF copy weighs more than the budget, from
        // before there was one.
        media.put("photos/heavy-card.webp", photo.card).await.unwrap();
        media.put("photos/heavy.webp", photo.full).await.unwrap();
        media.put("photos/heavy-card.avif", vec![0; 100_000]).await.unwrap();
        // All at once: the old one's six made again, the newer one's AVIF three and mini one (it
        // keeps the small copy it has), the lone one's five from its card copy, the heavy one's
        // card AVIF again and its four others, and the one that isn't a photo leaves the others to
        // be made.
        assert_eq!(media.make_missing_copies().await.unwrap(), 6 + 4 + 5 + 5);
        let card = media.get("photos/old-card.webp").await.unwrap().unwrap();
        assert_eq!(size_of(&card), (800, 600, WebP));
        let small = media.get("photos/old-small.webp").await.unwrap().unwrap();
        assert_eq!(size_of(&small), (533, 400, WebP));
        assert_eq!(size_of(&media.get("photos/old-mini.webp").await.unwrap().unwrap()), (400, 300, WebP));
        assert_eq!(media.get("photos/newer-small.webp").await.unwrap().as_deref(), Some(&[1, 2, 3][..]));
        for (name, card_size) in [("old", (800, 600)), ("newer", (800, 600)), ("lone", (600, 450)), ("heavy", (800, 600))] {
            let card = media.get(&format!("photos/{name}-card.avif")).await.unwrap().unwrap();
            assert_eq!(avif_size(&card), Some(card_size), "{name}");
            let small = media.get(&format!("photos/{name}-small.avif")).await.unwrap().unwrap();
            assert_eq!(avif_size(&small), Some((533, 400)), "{name}");
            let mini = media.get(&format!("photos/{name}-mini.avif")).await.unwrap().unwrap();
            assert_eq!(avif_size(&mini), Some((400, 300)), "{name}");
        }
        let lone = media.get("photos/lone-card.webp").await.unwrap().unwrap();
        assert_eq!(size_of(&lone), (600, 450, WebP));
        assert_eq!(size_of(&media.get("photos/lone-small.webp").await.unwrap().unwrap()), (533, 400, WebP));
        assert!(media.get("photos/heavy-card.avif").await.unwrap().unwrap().len() < 100_000);
        assert!(media.get("photos/broken-small.webp").await.unwrap().is_none());
        // Looked at once: a file in the store says so, and the next start makes nothing.
        let note = media.get("photos/card-copies-edition-2.txt").await.unwrap().unwrap();
        assert!(note.starts_with(b"Every photo's card copies"));
        assert_eq!(media.make_missing_copies().await.unwrap(), 0);
        // Or one asked for before then, kept as it is made; nothing for a photo with no card's copy,
        // or a file that isn't such a copy.
        media.delete("photos/old-small.webp").await.unwrap();
        assert_eq!(media.make_copy("photos/old-small.webp").await.unwrap(), Some(small));
        assert!(media.get("photos/old-small.webp").await.unwrap().is_some());
        assert!(media.make_copy("photos/gone-small.webp").await.unwrap().is_none());
        assert!(media.make_copy("photos/old-card.webp").await.unwrap().is_none());
        assert!(media.make_copy("photos/broken-small.avif").await.is_err());
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
