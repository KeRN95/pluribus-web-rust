-- Listings that wait for a moderator before they go up (web/post.rs, moderation.rs), on sites with
-- moderators, and why: the first listing of an account no code has confirmed, posted without
-- signing in (NEW_ACCOUNT), which the code sent with it can put up at once all the same; or one
-- with a photo another seller's listing has, the very same file (DUPLICATE_PHOTOS). The queue,
-- the oldest first, reads the schema's idx_listings_pending_review.
ALTER TABLE listings ADD COLUMN review_reason varchar(30)
    CHECK (review_reason IS NULL OR review_reason IN ('NEW_ACCOUNT', 'DUPLICATE_PHOTOS'));
