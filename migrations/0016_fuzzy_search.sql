-- Search that forgives typos and finds a word's other forms.
--
-- Full-text search (sq_unaccent, migration 0001) matches each word exactly, and PostgreSQL has no
-- Albanian stemmer: "makinat" didn't find "makinë", "apartamente" didn't find "apartament", and
-- "golff" or "mercedez" found nothing. Searches now go through search_query(), which widens each
-- word of four letters or more with the words active listings use (search_words):
--   * its other forms, the words with the same rough stem (sq_stem): makinat -> makina | makine |
--     makinave | makinen (unaccented, as the search index keeps them);
--   * when few listings use it or none do (sellers misspell too), the common word it most likely
--     misspells, with that word's forms: golff -> golf, mercedez -> mercedes.
--
-- The application works a search out once per page (db::text_search) and passes it to the listing
-- queries as a parameter. Computed inside them, it would be hidden from the planner, and a search
-- with no results would walk the whole newest-first index.
--
-- search_words is a materialized view. The application refreshes it at startup and every quarter
-- of an hour (db::refresh_search_words); after loading listings with psql, refresh it by hand:
--   REFRESH MATERIALIZED VIEW search_words;

-- A rough stem for Albanian: one common ending off (definite, plural and case endings), keeping at
-- least four letters. Its input is unaccented already (makinë arrives as makine), so makina,
-- makine, makinat, makinave and makinen all give makin. One ending only: a second would turn
-- benzine into benz. Four letters keep short words whole, so audi stays apart from audio, and
-- Berat from bere.
CREATE FUNCTION sq_stem(word text)
RETURNS text
LANGUAGE sql
IMMUTABLE PARALLEL SAFE STRICT
RETURN (
    SELECT CASE WHEN length(stem) >= 4 THEN stem ELSE word END
    FROM (SELECT regexp_replace(word, '(ave|eve|ash|ve|at|et|it|ut|in|en|es|ja|a|e|i|u)$', '') AS stem) s
);

-- The words active listings use, how many listings use each, and their stems. Only words of
-- letters, three or more: numbers and model codes (2016, a4, c220) are only ever matched as typed.
-- The table is named in full because PostgreSQL 17 and later refresh materialized views with only
-- pg_catalog on the search path.
CREATE MATERIALIZED VIEW search_words AS
SELECT word, ndoc AS listings, sq_stem(word) AS stem
FROM ts_stat($$SELECT search_vector FROM public.listings WHERE status = 'ACTIVE'$$)
WHERE word ~ '^[a-z]{3,}$';

-- REFRESH MATERIALIZED VIEW CONCURRENTLY, which lets searches go on meanwhile, needs a unique index.
CREATE UNIQUE INDEX search_words_word ON search_words (word);
CREATE INDEX search_words_stem ON search_words (stem);
-- Finds the words spelt like a typed one (pg_trgm's %).
CREATE INDEX search_words_trgm ON search_words USING gist (word gist_trgm_ops);

-- The common word a typed word most likely misspells, or NULL. A candidate is spelt like it
-- (pg_trgm similarity of 0.5 or more) and used by more than three times as many listings; the one
-- both closest in spelling and most used wins. So a word many listings use is left alone (audi
-- doesn't become audio), while a misspelling a few sellers share is still fixed.
CREATE FUNCTION search_correction(term text)
RETURNS text
LANGUAGE sql
STABLE PARALLEL SAFE STRICT
RETURN (
    SELECT w.word
    FROM search_words w
    WHERE w.word % term
      AND similarity(w.word, term) >= 0.5
      AND w.listings > 3 * coalesce((SELECT k.listings FROM search_words k WHERE k.word = term), 0)
    ORDER BY similarity(w.word, term) * ln(w.listings + 1) DESC, w.word
    LIMIT 1
);

-- A word and its other forms among the words listings use: those with the same stem, plus those
-- whose stem is the word itself or that are the word's stem, as one ending off doesn't always
-- give the same stem (citroen and citroeni, shtepi and shtepia).
CREATE FUNCTION search_forms(term text)
RETURNS SETOF text
LANGUAGE sql
STABLE PARALLEL SAFE STRICT
BEGIN ATOMIC
    SELECT term
    UNION
    SELECT w.word
    FROM search_words w
    WHERE w.stem = sq_stem(term) OR w.stem = term OR w.word = sq_stem(term);
END;

-- A search as a tsquery: the visitor's words in websearch syntax, as before (-word, "a phrase",
-- or), each word of four letters or more widened to its forms and, when it seems misspelt, to the
-- common word it most likely misspells and that word's forms. ts_rewrite swaps each word where it
-- stands, so an excluded word stays excluded, with its forms. corrections lists those common
-- words in the order typed, for the page to say they were searched too. The first eight words are
-- widened; any after them are searched as typed.
CREATE FUNCTION search_query(words text, OUT query tsquery, OUT corrections text[])
LANGUAGE plpgsql
STABLE PARALLEL SAFE
AS $$
DECLARE
    term text;
    correction text;
BEGIN
    query := websearch_to_tsquery('public.sq_unaccent', words);
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
