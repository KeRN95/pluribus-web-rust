-- Search finds listings by their category and their place, and leaves out filler words.
--
-- A listing was searched by its title, its description and its properties' labels. Now the names
-- of its category and of the categories above it, in Albanian and English, and of its place join
-- them in search_extra: "apartamente tirane" finds the flats in Tirana, "goma" what is listed
-- under "Goma dhe disqe", "cars" the cars. They are words, not filters, so they combine with the
-- rest of a search: "makinë larëse" finds washing machines rather than cars, which don't say
-- "larëse".
--
-- search_query() leaves out filler words (në, me, për, dhe...), which most listings don't say:
-- "apartament me qira në tiranë" searches apartament, qira and tiranë. "ose" and "apo" mean or.

-- A listing's words besides its title and description: its searchable properties' labels, then
-- the name of its category and of each category above it, then its place's and its
-- municipality's. Run it whenever a listing's property values, category or place change:
--   UPDATE listings SET search_extra = listing_search_extra(id) WHERE id = $1
CREATE OR REPLACE FUNCTION listing_search_extra(listing uuid)
RETURNS text
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN nullif(concat_ws(' ',
    (SELECT string_agg(label, ' ' ORDER BY sort_order, label)
     FROM (
        SELECT eca.sort_order, coalesce(o.label::text, v.value_text, trim_scale(v.value_number)::text) AS label
        FROM listings l
        JOIN effective_category_attributes eca ON eca.category_id = l.category_id AND eca.searchable
        JOIN listing_attribute_values v ON v.listing_id = l.id AND v.attribute_id = eca.attribute_id
        LEFT JOIN attribute_options o ON o.id = v.value_option_id
        WHERE l.id = listing
        UNION ALL
        SELECT eca.sort_order, o.label::text
        FROM listings l
        JOIN effective_category_attributes eca ON eca.category_id = l.category_id AND eca.searchable
        JOIN listing_attribute_options lo ON lo.listing_id = l.id AND lo.attribute_id = eca.attribute_id
        JOIN attribute_options o ON o.id = lo.option_id
        WHERE l.id = listing
     ) labels),
    -- "Makina Cars Automjete Vehicles".
    (WITH RECURSIVE path AS (
         SELECT c.parent_id, c.name::text AS name, c.name_i18n ->> 'en' AS english, 1 AS depth
         FROM listings l
         JOIN categories c ON c.id = l.category_id
         WHERE l.id = listing
         UNION ALL
         SELECT c.parent_id, c.name::text, c.name_i18n ->> 'en', path.depth + 1
         FROM path
         JOIN categories c ON c.id = path.parent_id
         WHERE path.depth < 10
     )
     SELECT string_agg(concat_ws(' ', name, english), ' ' ORDER BY depth) FROM path),
    -- "Tiranë". A name in -ër also with its article, "Shkodër Shkodra": one ending off doesn't
    -- turn either into the other, as it does Durrës and Durrësi.
    (SELECT string_agg(DISTINCT names.place, ' ')
     FROM listings l
     JOIN locations own ON own.id = l.location_id
     LEFT JOIN locations m ON m.id = location_municipality(l.location_id)
     CROSS JOIN LATERAL (VALUES (own.name::text), (m.name::text),
                                (regexp_replace(own.name::text, 'ër$', 'ra')),
                                (regexp_replace(m.name::text, 'ër$', 'ra'))) AS names (place)
     WHERE l.id = listing AND names.place IS NOT NULL)
), '');

UPDATE listings SET search_extra = listing_search_extra(id);

-- As in migration 0016, and now: filler words left out, "ose" and "apo" as or.
CREATE OR REPLACE FUNCTION search_query(words text, OUT query tsquery, OUT corrections text[])
LANGUAGE plpgsql
STABLE PARALLEL SAFE
-- A filler word is left out through an empty tsquery, which comes with a notice each time.
SET client_min_messages = warning
AS $$
DECLARE
    -- Unaccented, as the search index keeps words: në, me, për, dhe, e, i, të, një, nga, tek, që.
    -- Not "se": "iPhone SE".
    fillers CONSTANT text[] := ARRAY['ne', 'me', 'per', 'dhe', 'e', 'i', 'te', 'nje', 'nga', 'tek', 'qe'];
    without_fillers tsquery;
    term text;
    correction text;
BEGIN
    query := websearch_to_tsquery('public.sq_unaccent', regexp_replace(words, '(^|\s)(ose|apo)(?=\s|$)', '\1or', 'gi'));
    -- Filler words say nothing about what is sought, and a listing would have to say them all to
    -- match. A search of fillers only ("në") keeps them.
    without_fillers := query;
    FOR term IN
        SELECT t.lexeme FROM unnest(to_tsvector('public.sq_unaccent', words)) AS t WHERE t.lexeme = ANY (fillers)
    LOOP
        without_fillers := ts_rewrite(without_fillers, quote_literal(term)::tsquery, ''::tsquery);
    END LOOP;
    IF numnode(without_fillers) > 0 THEN
        query := without_fillers;
    END IF;

    corrections := '{}';
    FOR term IN
        SELECT t.lexeme
        FROM unnest(to_tsvector('public.sq_unaccent', words)) AS t
        WHERE t.lexeme ~ '^[a-z]{4,}$'
        ORDER BY t.positions[1]
        LIMIT 8
    LOOP
        correction := search_correction(term);
        IF correction IN (SELECT search_forms(term)) THEN
            -- Another form of the same word (makinat -> makina), which the forms cover.
            correction := NULL;
        ELSIF correction IS NOT NULL THEN
            corrections := corrections || correction;
        END IF;
        query := ts_rewrite(query, quote_literal(term)::tsquery,
            (SELECT string_agg(quote_literal(form), ' | ')::tsquery
             FROM (SELECT search_forms(term) UNION SELECT search_forms(correction)) AS forms (form)));
    END LOOP;
END
$$;

-- The words that start with what is being typed, for search suggestions (db::words_starting_with).
-- The database's collation sorts words in a way LIKE 'gol%' can't follow; text_pattern_ops can.
CREATE INDEX search_words_prefix ON search_words (word text_pattern_ops);

-- With categories and places, listings use new words.
REFRESH MATERIALIZED VIEW search_words;
