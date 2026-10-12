-- Photos the phone app uploads before the listing they go with (pluribus-api, POST /api/v1/photos),
-- as the pages' form sends its own with the listing: decoded, made into their copies and put in the
-- store at once (pluribus-services' photos.rs), each kept here under its uploader with what
-- listing_media keeps of a photo (its key in the store, its copies' addresses, its tiny copy, its
-- size and the SHA-256 of what was sent) until a listing or an edit takes it, by its id, in the
-- transaction that saves it (db::create_listing, db::update_listing): its row then moves into
-- listing_media, keeping its id. One no listing took is forgotten a day on, with its files
-- (db::forget_uploads, which the expiry calls).
CREATE TABLE photo_uploads (
    id                  uuid PRIMARY KEY DEFAULT uuid_v7(),
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_key         text NOT NULL UNIQUE,
    public_url          text,
    middle_url          text,
    thumbnail_url       text,
    jpeg_url            text,
    placeholder         text,
    size_bytes          bigint,
    width_px            integer,
    height_px           integer,
    sha256              bytea,
    created_at          timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX idx_photo_uploads_user ON photo_uploads(user_id, created_at);
