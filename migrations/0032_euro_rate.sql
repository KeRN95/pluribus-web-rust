-- The euro rate (rates.rs): every few hours the server reads the Bank of Albania's official rate,
-- and when it has moved far enough from the one prices are worked out at, takes it and works the
-- euro prices of the listings priced in lekë out again (price_normalized), a few thousand a time.
--   rate_date      the day of the official rate eur_rate is (NULL for the placeholder seeded with
--                  the schema)
--   normalized_at  when the listings priced in the currency last had their euro prices worked out
--                  at eur_rate; NULL while that is due, after eur_rate changed or a pass broke off
ALTER TABLE exchange_rates ADD COLUMN rate_date date;
ALTER TABLE exchange_rates ADD COLUMN normalized_at timestamptz;
-- The listings saved so far were worked out at the rates they have.
UPDATE exchange_rates SET normalized_at = updated_at;

-- A listing's updated_at says when it last changed, which the sitemaps tell search engines; a
-- price worked out again in euros at a new rate is no change to it. The re-pricing says so for its
-- transaction (app.keep_updated_at), as the status history's reason is said (app.status_reason).
CREATE FUNCTION set_listing_updated_at()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF coalesce(current_setting('app.keep_updated_at', true), '') <> 'on' THEN
        NEW.updated_at = now();
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER trg_listings_updated_at ON listings;
CREATE TRIGGER trg_listings_updated_at
BEFORE UPDATE ON listings FOR EACH ROW EXECUTE FUNCTION set_listing_updated_at();
