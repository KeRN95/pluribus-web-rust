-- A category's listings in the order a page asks for, read straight off an index as the newest
-- first already are (idx_listings_active_category_sort), so a page reads its 24 rather than
-- sorting every listing in the category: by price, the cheapest or the dearest first, a listing
-- without a price last either way (db.rs, Sort::push_key), and the nearest first around a point
-- on the map (Near::push_distance). A page over several categories reads each one's first in
-- order and merges them (db.rs, push_ordered).

CREATE EXTENSION IF NOT EXISTS btree_gist WITH SCHEMA public;

-- Cheapest first reads this one forwards; dearest first reads the other backwards.
CREATE INDEX idx_listings_active_category_cheapest ON listings
    (category_id, (coalesce(price_normalized, 'Infinity'::numeric)), id)
    WHERE status = 'ACTIVE';
CREATE INDEX idx_listings_active_category_dearest ON listings
    (category_id, (coalesce(price_normalized, '-Infinity'::numeric)), id)
    WHERE status = 'ACTIVE';

-- Nearest first: the category beside the place in one GiST index (btree_gist lets a uuid stand
-- in one), which gives a category's listings in the order of the cube's <-> to the point (KNN),
-- within the radius's box.
CREATE INDEX idx_listings_active_category_earth ON listings
    USING gist (category_id, ll_to_earth(latitude::float8, longitude::float8))
    WHERE status = 'ACTIVE' AND latitude IS NOT NULL;
