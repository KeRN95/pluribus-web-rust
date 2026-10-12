//! Photos uploaded before their listing (photo_uploads, migration 0044): the phone app sends a
//! listing's photos first (pluribus-api's `POST /photos`), each made into its copies and put in the
//! store at once (pluribus-services' photos.rs), and kept here under its uploader (`add_photo_uploads`)
//! until a listing or an edit takes it by its id, in the transaction that saves it (posting.rs,
//! `Photos::Uploads`, by `take_uploads`): its row moves into listing_media then, keeping its id, so
//! that an edit's order names it as the upload did. One no listing took is forgotten a day on, its
//! files with it (`forget_uploads`, which the expiry calls).

use std::time::Duration;

use super::*;

/// Keeps the photos `user_id` uploaded, their files in the store already (`photos`, as
/// pluribus-services' photos::store gives them), for a listing to take; their ids, in their order.
pub async fn add_photo_uploads(db: &PgPool, user_id: Uuid, photos: &[NewPhoto]) -> sqlx::Result<Vec<Uuid>> {
    let mut tx = db.begin().await?;
    let mut ids = Vec::with_capacity(photos.len());
    for photo in photos {
        let id = sqlx::query_scalar!(
            "INSERT INTO photo_uploads (user_id, storage_key, public_url, middle_url, thumbnail_url, jpeg_url, placeholder,
                                        size_bytes, width_px, height_px, sha256)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING id",
            user_id,
            photo.storage_key,
            photo.url,
            photo.middle_url,
            photo.card_url,
            photo.jpeg_url,
            photo.placeholder,
            photo.size_bytes,
            photo.width,
            photo.height,
            photo.sha256,
        )
        .fetch_one(&mut *tx)
        .await?;
        ids.push(id);
    }
    tx.commit().await?;
    Ok(ids)
}

/// How many of `user_id`'s uploads wait for a listing, which the API holds to a few listings'
/// photos (pluribus-api's own.rs).
pub async fn uploads_waiting(db: &PgPool, user_id: Uuid) -> sqlx::Result<i64> {
    sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM photo_uploads WHERE user_id = $1"#, user_id).fetch_one(db).await
}

/// The fingerprints (SHA-256) of these uploads of `user_id`'s, those still there: what the
/// moderators' review weighs a new listing's photos by (review.rs, photos_elsewhere).
pub async fn upload_hashes(db: &PgPool, user_id: Uuid, ids: &[Uuid]) -> sqlx::Result<Vec<Vec<u8>>> {
    sqlx::query_scalar!(
        r#"SELECT sha256 AS "sha256!" FROM photo_uploads WHERE id = ANY($1) AND user_id = $2 AND sha256 IS NOT NULL"#,
        ids,
        user_id,
    )
    .fetch_all(db)
    .await
}

/// Moves the uploads `ids` of `user_id`'s into the listing's photos, in that order (each once),
/// after those it has, the first it gets its cover where it has none, each keeping its id: inside
/// the transaction that saves the listing or its edit (posting.rs). False when any of them isn't
/// there to take (another's, taken by a listing already, or forgotten): the caller then fails the
/// transaction, which puts back those it took.
pub(super) async fn take_uploads(tx: &mut sqlx::PgConnection, listing_id: Uuid, user_id: Uuid, ids: &[Uuid]) -> sqlx::Result<bool> {
    let mut wanted: Vec<Uuid> = Vec::with_capacity(ids.len());
    for id in ids {
        if !wanted.contains(id) {
            wanted.push(*id);
        }
    }
    if wanted.is_empty() {
        return Ok(true);
    }
    let taken = sqlx::query_scalar!(
        "WITH so_far AS (
             SELECT coalesce(max(sort_order), -1) AS last, coalesce(bool_or(is_cover), false) AS has_cover
             FROM listing_media WHERE listing_id = $1
         ), taken AS (
             DELETE FROM photo_uploads WHERE id = ANY($2) AND user_id = $3
             RETURNING id, storage_key, public_url, middle_url, thumbnail_url, jpeg_url, placeholder, size_bytes,
                       width_px, height_px, sha256
         )
         INSERT INTO listing_media (id, listing_id, media_type, status, storage_key, public_url, middle_url,
                                    thumbnail_url, jpeg_url, placeholder, mime_type, size_bytes, width_px,
                                    height_px, sha256, sort_order, is_cover)
         SELECT t.id, $1, 'IMAGE', 'READY', t.storage_key, t.public_url, t.middle_url, t.thumbnail_url, t.jpeg_url,
                t.placeholder, 'image/webp', t.size_bytes, t.width_px, t.height_px, t.sha256,
                s.last + o.place::int, NOT s.has_cover AND o.place = 1
         FROM taken t
         JOIN unnest($2::uuid[]) WITH ORDINALITY AS o(id, place) ON o.id = t.id
         CROSS JOIN so_far s
         RETURNING id",
        listing_id,
        &wanted,
        user_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(taken.len() == wanted.len())
}

/// Forgets the uploads no listing took within `older_than` of their upload: their rows, and the
/// keys of their files in the store, which the caller deletes, every copy (pluribus-services'
/// media::delete_photos; the expiry, with photos::UPLOADS_KEPT).
pub async fn forget_uploads(db: &PgPool, older_than: Duration) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        "DELETE FROM photo_uploads WHERE created_at < now() - make_interval(secs => $1) RETURNING storage_key",
        older_than.as_secs_f64(),
    )
    .fetch_all(db)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A photo whose files are in the store, as photos::store gives it.
    fn photo(name: &str) -> NewPhoto {
        NewPhoto {
            storage_key: format!("photos/{name}.webp"),
            url: format!("/media/photos/{name}.webp"),
            middle_url: None,
            card_url: format!("/media/photos/{name}-card.webp"),
            jpeg_url: format!("/media/photos/{name}.jpg"),
            placeholder: "data:image/webp;base64,".to_string(),
            size_bytes: 1000,
            width: 800,
            height: 600,
            sha256: vec![name.as_bytes()[0]; 32],
        }
    }

    /// Uploads wait under their uploader, their fingerprints for the review; those a day old no
    /// listing took are forgotten, their keys given back for their files, the newer ones kept.
    #[sqlx::test(migrator = "crate::MIGRATOR")]
    async fn uploads_wait_for_a_listing_and_are_forgotten_a_day_on(db: PgPool) -> sqlx::Result<()> {
        let user: Uuid = sqlx::query_scalar("INSERT INTO users (auth_subject, display_name) VALUES ('uploads-tests', 'Ana') RETURNING id")
            .fetch_one(&db)
            .await?;
        let ids = add_photo_uploads(&db, user, &[photo("a"), photo("b")]).await?;
        assert_eq!(ids.len(), 2);
        assert_eq!(uploads_waiting(&db, user).await?, 2);
        assert_eq!(upload_hashes(&db, user, &ids).await?.len(), 2);
        assert!(upload_hashes(&db, Uuid::nil(), &ids).await?.is_empty(), "not another's");

        sqlx::query("UPDATE photo_uploads SET created_at = now() - interval '25 hours' WHERE id = $1").bind(ids[0]).execute(&db).await?;
        assert_eq!(forget_uploads(&db, Duration::from_secs(24 * 60 * 60)).await?, ["photos/a.webp"]);
        let left: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM photo_uploads").fetch_all(&db).await?;
        assert_eq!(left, [ids[1]]);
        assert!(forget_uploads(&db, Duration::from_secs(24 * 60 * 60)).await?.is_empty());
        Ok(())
    }
}
