-- Listings expire (expiry.rs): every few minutes the server looks for those up or reserved whose
-- expires_at has passed or is a few days off. Reserved listings are on the site too, so they
-- expire as well; listings from before there was an expiry have none and never expire, so the
-- index leaves them out (a query's "expires_at < ..." keeps to it).
DROP INDEX idx_listings_expiry;
CREATE INDEX idx_listings_expiry ON listings(expires_at)
    WHERE status IN ('ACTIVE', 'RESERVED') AND expires_at IS NOT NULL;
