-- Saved searches (web/searches.rs): a search's page and filters are its `filters`, as
-- {"path": "/c/makina", "query": "city=tirane&vehicle_fuel=DIESEL"}, once per user; and when its
-- saver last opened it from /kerkimet-e-ruajtura, which its count of new listings starts from.
ALTER TABLE saved_searches ADD COLUMN last_seen_at timestamptz;
CREATE UNIQUE INDEX idx_saved_searches_user_filters ON saved_searches (user_id, filters);
