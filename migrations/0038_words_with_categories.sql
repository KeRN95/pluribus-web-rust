-- Searches whose words seldom appear in a category ("apartament" among the cars) read their few
-- matches through one index that has both the words and the category (db/listing_pages.rs,
-- planned): a GIN index over the category (btree_gin's operators) and the words, which answers
-- "these words in these categories" without reading a listing that doesn't match. Against a
-- million listings, that search's matches took 6 ms through it, 270 ms through the words' index
-- alone (the 137 000 listings with the word, each read for its category). A GIN index of several
-- columns serves a search on any of them as well as one of that column alone, so it takes the
-- place of the words' own index. Built in 6 s for 921 000 listings, writes to listings waiting.
CREATE EXTENSION IF NOT EXISTS btree_gin;

CREATE INDEX idx_listings_active_words ON listings USING gin (category_id, search_vector)
    WHERE status = 'ACTIVE';

DROP INDEX idx_listings_search_vector;
