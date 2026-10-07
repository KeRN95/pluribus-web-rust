-- What a square metre costs in each zone (0019_zones.sql), for a property on sale there to say how
-- its own price per square metre compares ("Mesatarja e zonës" on its page): for each zone and
-- category, the mean of the euros a square metre that the zone's active sales with a price and an
-- area ask, leaving out the cheapest and the dearest tenth so that a mistyped price or area
-- doesn't pull it, where at least five listings make one. A starting price ("nga") or a price on
-- request says too little to count.
--
-- zone_area_prices is a materialized view. The application rebuilds it at startup and every
-- quarter of an hour (db::refresh_zone_prices), as it does search_words; after loading listings
-- with psql, rebuild it by hand:
--   REFRESH MATERIALIZED VIEW zone_area_prices;

CREATE MATERIALIZED VIEW zone_area_prices AS
WITH priced AS (
    SELECT l.location_id AS zone_id, l.category_id, l.price_normalized / v.value_number AS per_m2
    FROM locations z
    JOIN listings l ON l.location_id = z.id
    -- The area's property looked up first, so that a listing's area is read by its key: joined
    -- as a table, it had every area read again for each zone.
    JOIN listing_attribute_values v
      ON v.listing_id = l.id AND v.attribute_id = (SELECT a.id FROM attributes a WHERE a.code = 'property_area')
    WHERE z.location_type = 'AREA'
      AND l.status = 'ACTIVE' AND l.listing_type = 'SELL' AND l.price_type IN ('FIXED', 'NEGOTIABLE')
      AND l.price_normalized > 0 AND v.value_number > 0
),
spread AS (
    SELECT zone_id, category_id, count(*) AS listings,
           percentile_cont(0.1) WITHIN GROUP (ORDER BY per_m2) AS low,
           percentile_cont(0.9) WITHIN GROUP (ORDER BY per_m2) AS high
    FROM priced
    GROUP BY zone_id, category_id
    HAVING count(*) >= 5
)
SELECT p.zone_id, p.category_id, round(avg(p.per_m2), 2) AS average_eur, s.listings
FROM priced p
JOIN spread s USING (zone_id, category_id)
WHERE p.per_m2 BETWEEN s.low AND s.high
GROUP BY p.zone_id, p.category_id, s.listings;

-- One row a zone and category: what a listing's page finds its own by, and what rebuilding the
-- view while pages go on reading it (REFRESH ... CONCURRENTLY) needs.
CREATE UNIQUE INDEX zone_area_prices_key ON zone_area_prices (zone_id, category_id);

-- 0019 added the zones to a table too small for autovacuum to look at it again for them: the
-- planner took them for one place, and read every listing's area once for each zone.
ANALYZE locations;
