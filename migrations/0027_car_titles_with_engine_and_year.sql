-- A car's title says its engine and its year after its make and model ("Volkswagen Golf 1.6, 2016";
-- "Volkswagen Golf, 2016" without the engine), rather than its body type and colour ("Volkswagen
-- Golf Hatchback, e zezë"): four Golfs under a listing read four times "Volkswagen Golf Hatchback",
-- while the year and the engine are what tell one Golf from another, as sellers' own titles say
-- ("Golf 7 1.6 TDI 2016"). The engine is in cm³, said in litres to a tenth as engines are named
-- (1598 cm³: "1.6"); a size under 100 cm³ is a slip and is left out. The listing page's details
-- show the body type and the colour again, and leave the year out (web/properties.rs, TITLE_CODES).
--
-- Still NULL for any other listing, a part's included, and for a car without its make or model
-- (migration 0015), so the listings to make again are those with one.
CREATE OR REPLACE FUNCTION listing_generated_title(listing uuid)
RETURNS text
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN (
    SELECT concat_ws(' ', max(o.label) FILTER (WHERE a.code = 'vehicle_make'),
                          max(o.label) FILTER (WHERE a.code = 'vehicle_model'),
                          to_char(round(max(v.value_number) FILTER (WHERE a.code = 'vehicle_engine'
                                                                      AND v.value_number >= 100) / 1000, 1),
                                  'FM990.0'))
           || coalesce(', ' || (max(v.value_number) FILTER (WHERE a.code = 'vehicle_year'))::integer, '')
    FROM listings l
    JOIN listing_attribute_values v ON v.listing_id = l.id
    JOIN attributes a ON a.id = v.attribute_id
    LEFT JOIN attribute_options o ON o.id = v.value_option_id
    WHERE l.id = listing
      -- A car's: its category has a year of its own, which a part's hasn't.
      AND EXISTS (SELECT 1 FROM effective_category_attributes eca JOIN attributes y ON y.id = eca.attribute_id
                  WHERE eca.category_id = l.category_id AND y.code = 'vehicle_year')
    HAVING count(o.id) FILTER (WHERE a.code IN ('vehicle_make', 'vehicle_model')) = 2
);

-- Every car's at once, as the function makes one: calling it for each of a quarter of a million
-- cars took three minutes, its category check again for every car, where they are cars already.
UPDATE listings l
SET generated_title = made.title
FROM (
    SELECT v.listing_id,
           concat_ws(' ', max(o.label) FILTER (WHERE a.code = 'vehicle_make'),
                          max(o.label) FILTER (WHERE a.code = 'vehicle_model'),
                          to_char(round(max(v.value_number) FILTER (WHERE a.code = 'vehicle_engine'
                                                                      AND v.value_number >= 100) / 1000, 1),
                                  'FM990.0'))
           || coalesce(', ' || (max(v.value_number) FILTER (WHERE a.code = 'vehicle_year'))::integer, '') AS title
    FROM listing_attribute_values v
    JOIN attributes a ON a.id = v.attribute_id
                     AND a.code IN ('vehicle_make', 'vehicle_model', 'vehicle_engine', 'vehicle_year')
    LEFT JOIN attribute_options o ON o.id = v.value_option_id
    WHERE v.listing_id IN (SELECT id FROM listings WHERE generated_title IS NOT NULL)
    GROUP BY v.listing_id
    HAVING count(o.id) FILTER (WHERE a.code IN ('vehicle_make', 'vehicle_model')) = 2
) made
WHERE l.id = made.listing_id
  AND l.generated_title IS DISTINCT FROM made.title;
