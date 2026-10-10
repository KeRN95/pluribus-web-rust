-- What cars of a model ask, by year (car_model_prices): for each model and year among the cars up
-- for sale at a price, the cars of that model two years either side, how many, and the quartiles
-- of their prices in euros, the median the middle one; where at least eight make one. A car's page
-- compares its price with it (web/car_prices.rs), the cards mark a clear bargain, and the listing
-- form says what such cars ask as a price is typed (/shto/cmimi). Cars for parts are left out, as
-- are starting prices ("nga"). The app rebuilds it with the other views, at startup and every 15
-- minutes (main.rs); after loading listings with psql, rebuild it too:
-- REFRESH MATERIALIZED VIEW car_model_prices;
CREATE MATERIALIZED VIEW car_model_prices AS
WITH cars AS (
    SELECT model.value_option_id AS model_id, year.value_number::int AS year, l.price_normalized AS price
    FROM listings l
    JOIN listing_attribute_values model
      ON model.listing_id = l.id AND model.attribute_id = (SELECT a.id FROM attributes a WHERE a.code = 'vehicle_model')
    JOIN listing_attribute_values year
      ON year.listing_id = l.id AND year.attribute_id = (SELECT a.id FROM attributes a WHERE a.code = 'vehicle_year')
    WHERE l.category_id = (SELECT c.id FROM categories c WHERE c.slug = 'makina')
      AND l.status = 'ACTIVE' AND l.listing_type = 'SELL' AND l.price_type IN ('FIXED', 'NEGOTIABLE')
      AND l.price_normalized > 0 AND l.condition IS DISTINCT FROM 'FOR_PARTS'
      AND model.value_option_id IS NOT NULL AND year.value_number IS NOT NULL
)
SELECT y.model_id, y.year, count(*) AS listings,
       round(percentile_cont(0.25) WITHIN GROUP (ORDER BY c.price)::numeric, 2) AS low_eur,
       round(percentile_cont(0.5) WITHIN GROUP (ORDER BY c.price)::numeric, 2) AS median_eur,
       round(percentile_cont(0.75) WITHIN GROUP (ORDER BY c.price)::numeric, 2) AS high_eur
FROM (SELECT DISTINCT model_id, year FROM cars) y
JOIN cars c ON c.model_id = y.model_id AND c.year BETWEEN y.year - 2 AND y.year + 2
GROUP BY y.model_id, y.year
HAVING count(*) >= 8;

-- One row per model and year, which the cards look up and the rebuild needs to swap the figures
-- in while pages read them.
CREATE UNIQUE INDEX car_model_prices_key ON car_model_prices (model_id, year);
