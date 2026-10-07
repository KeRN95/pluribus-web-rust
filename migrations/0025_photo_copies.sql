-- Photos are kept as WebP, with a JPEG copy and a tiny one in the pages (media.rs).
--
-- A photo stored since this migration is WebP in three sizes: public_url the full one (1600 px on
-- its longer side), middle_url a middle one (1200 px; none for a photo no bigger) and
-- thumbnail_url the card's (600 px), which the browser picks between (srcset). jpeg_url is the
-- full one as a JPEG, for link previews and browsers without WebP, and placeholder a 16 px copy as
-- a data: URI, a few hundred characters, which pages show blurred while the photo loads and around
-- it where it doesn't fill its frame. Photos stored before, as JPEG, have none of the three and
-- show as they did.
ALTER TABLE listing_media
    ADD COLUMN middle_url text,
    ADD COLUMN jpeg_url text,
    ADD COLUMN placeholder text CHECK (length(placeholder) <= 2000);
