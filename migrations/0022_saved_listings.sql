-- The listings people keep, with the heart over a listing's photo (web/saved.rs): a signed-in
-- user's, to find again on /te-ruajturat, the last kept first.
CREATE TABLE saved_listings (
    user_id    uuid        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    listing_id uuid        NOT NULL REFERENCES listings(id) ON DELETE CASCADE,
    saved_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, listing_id)
);
CREATE INDEX idx_saved_listings_user_saved ON saved_listings(user_id, saved_at DESC);
CREATE INDEX idx_saved_listings_listing    ON saved_listings(listing_id);
