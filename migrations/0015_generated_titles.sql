-- A car's title, made from what it is: its make, model and body type, then its colour
-- ("Mercedes-Benz S-Class Sedan, e zezë"). Pages show it as the listing's title, with the
-- seller's own heading the description, and leave those four out of the details.
--
-- NULL for any other listing, a part's included (its make and model are the vehicle it fits,
-- not what it is), and for a car without its make or model. "Tjetër" for a colour says nothing.
-- Kept up to date like search_extra, in the same statements:
--   UPDATE listings SET search_extra = listing_search_extra(id),
--                       generated_title = listing_generated_title(id) WHERE id = $1
ALTER TABLE listings ADD COLUMN generated_title text;

CREATE FUNCTION listing_generated_title(listing uuid)
RETURNS text
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN (
    SELECT concat_ws(' ', max(o.label) FILTER (WHERE a.code = 'vehicle_make'),
                          max(o.label) FILTER (WHERE a.code = 'vehicle_model'),
                          max(o.label) FILTER (WHERE a.code = 'vehicle_body_type'))
           || coalesce(', ' || lower(max(o.label) FILTER (WHERE a.code = 'vehicle_color' AND o.value_code <> 'OTHER')), '')
    FROM listings l
    JOIN listing_attribute_values v ON v.listing_id = l.id
    JOIN attributes a ON a.id = v.attribute_id
    JOIN attribute_options o ON o.id = v.value_option_id
    WHERE l.id = listing
      -- A car's: its category has a year of its own, which a part's hasn't.
      AND EXISTS (SELECT 1 FROM effective_category_attributes eca JOIN attributes y ON y.id = eca.attribute_id
                  WHERE eca.category_id = l.category_id AND y.code = 'vehicle_year')
    HAVING count(*) FILTER (WHERE a.code IN ('vehicle_make', 'vehicle_model')) = 2
);

UPDATE listings SET generated_title = listing_generated_title(id);
