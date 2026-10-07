-- Words that name a kind of listing find listings of that kind.
--
-- "golf 7 në shitje" found nothing: every word a search keeps must be in a listing's words, and
-- few cars for sale say "shitje". Such words (shitje, shitet, shes: for sale; qira, qera: for
-- rent; blej: wanted) now find the listings of their kind, or those that say them: "golf 7 në
-- shitje" finds the Golfs for sale, and "makina me qira" still finds a rental firm's listing that
-- says "qira", whatever its kind.

-- The kind of listing a word names (listings.listing_type), the word unaccented as the search index
-- keeps words, or NULL.
CREATE FUNCTION search_kind(word text)
RETURNS text
LANGUAGE sql
IMMUTABLE PARALLEL SAFE STRICT
RETURN CASE
    WHEN word IN ('shitje', 'shitja', 'shitjes', 'shitet', 'shiten', 'shes') THEN 'SELL'
    WHEN word IN ('qira', 'qiraja', 'qirase', 'qera', 'qeraja', 'qerase') THEN 'RENT'
    WHEN word IN ('blej', 'blejme', 'blihet') THEN 'BUY'
END;

-- As in migration 0017, and now the words that name a kind of listing (search_kind) leave query,
-- which every listing found matches, for kinds, the kinds they name, and kind_query, the words
-- with their forms: a listing found is of one of those kinds, or matches kind_query too
-- (db::text_search). A word left out ("-shitje") stays in query. query is NULL where only such
-- words were searched ("në shitje"); a search of no words still finds nothing.
DROP FUNCTION search_query(text);
CREATE FUNCTION search_query(words text, OUT query tsquery, OUT corrections text[], OUT kinds text[],
                             OUT kind_query tsquery)
LANGUAGE plpgsql
STABLE PARALLEL SAFE
-- A word is left out through an empty tsquery, which comes with a notice each time.
SET client_min_messages = warning
AS $$
DECLARE
    -- Unaccented, as the search index keeps words: në, me, për, dhe, e, i, të, një, nga, tek, që.
    -- Not "se": "iPhone SE".
    fillers CONSTANT text[] := ARRAY['ne', 'me', 'per', 'dhe', 'e', 'i', 'te', 'nje', 'nga', 'tek', 'qe'];
    without_fillers tsquery;
    term text;
    kind text;
    forms tsquery;
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

    -- The words that name a kind of listing, unless left out.
    kinds := '{}';
    FOR term IN
        SELECT t.lexeme
        FROM unnest(to_tsvector('public.sq_unaccent', words)) AS t
        WHERE search_kind(t.lexeme) IS NOT NULL
        ORDER BY t.positions[1]
    LOOP
        CONTINUE WHEN position('!' || quote_literal(term) IN query::text) > 0;
        kind := search_kind(term);
        IF NOT kind = ANY (kinds) THEN
            kinds := kinds || kind;
        END IF;
        forms := (SELECT string_agg(quote_literal(form), ' | ')::tsquery FROM search_forms(term) AS forms (form));
        kind_query := CASE WHEN kind_query IS NULL THEN forms ELSE kind_query || forms END;
        query := ts_rewrite(query, quote_literal(term)::tsquery, ''::tsquery);
    END LOOP;
    IF numnode(query) = 0 AND kind_query IS NOT NULL THEN
        query := NULL;
    END IF;

    corrections := '{}';
    FOR term IN
        SELECT t.lexeme
        FROM unnest(to_tsvector('public.sq_unaccent', words)) AS t
        WHERE t.lexeme ~ '^[a-z]{4,}$' AND search_kind(t.lexeme) IS NULL
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
