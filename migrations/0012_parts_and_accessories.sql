-- Car parts get kinds of their own, a level further down: Automjete > Pjesë dhe aksesorë > GPS dhe
-- navigatorë, Goma dhe disqe... the kinds MerrJep has. "Pjesë këmbimi" (spare parts) becomes one
-- of them and keeps its address (/c/pjese-kembimi) and its listings. Every part can say which
-- vehicle it fits: a make and model from the cars' own lists, and the years, which buyers filter by.

-- A category's own name for a property, where the property's name alone would mislead: on a part,
-- "Marka" would read as the part's brand, so there it is "Marka e automjetit". NULL: the
-- property's name.
ALTER TABLE category_attributes ADD COLUMN label varchar(160);

-- As in 0003, with the label (inherited like the other settings).
CREATE OR REPLACE VIEW effective_category_attributes AS
WITH RECURSIVE lineage AS (
    SELECT id AS category_id, id AS source_id, 0 AS depth FROM categories
    UNION ALL
    SELECT lineage.category_id, c.parent_id, lineage.depth + 1
    FROM lineage
    JOIN categories c ON c.id = lineage.source_id
    WHERE c.parent_id IS NOT NULL
)
SELECT DISTINCT ON (lineage.category_id, ca.attribute_id)
       lineage.category_id, ca.attribute_id, ca.required, ca.searchable, ca.filterable,
       ca.show_in_card, ca.show_in_details, ca.sort_order, ca.label
FROM lineage
JOIN category_attributes ca ON ca.category_id = lineage.source_id
ORDER BY lineage.category_id, ca.attribute_id, lineage.depth;

-- The new level, which only groups: listings go into its kinds.
INSERT INTO categories (parent_id, name, slug, sort_order, default_currency, listing_types, uses_condition, allow_listings)
SELECT id, 'Pjesë dhe aksesorë', 'pjese-dhe-aksesore', 40, 'ALL', listing_types, uses_condition, false
FROM categories
WHERE slug = 'automjete';

UPDATE categories
SET parent_id = (SELECT id FROM categories WHERE slug = 'pjese-dhe-aksesore'), sort_order = 10, updated_at = now()
WHERE slug = 'pjese-kembimi';

INSERT INTO categories (parent_id, name, slug, sort_order, default_currency, listing_types, uses_condition)
SELECT parent.id, k.name, k.slug, k.sort_order, k.currency::currency_code, parent.listing_types, parent.uses_condition
FROM (VALUES ('Goma dhe disqe', 'goma-dhe-disqe', 20, 'ALL'),
             ('Audio dhe video', 'audio-dhe-video', 30, 'ALL'),
             ('GPS dhe navigatorë', 'gps-dhe-navigatore', 40, 'ALL'),
             ('Styling dhe pajisje sportive', 'styling-dhe-pajisje-sportive', 50, 'ALL'),
             ('Tapete makine', 'tapete-makine', 60, 'ALL'),
             ('Rimorkio', 'rimorkio', 70, 'EUR'),
             ('Mbushës për makina elektrike', 'mbushes-per-makina-elektrike', 80, 'EUR'),
             ('Të tjera', 'pjese-te-tjera', 90, 'ALL')) AS k(name, slug, sort_order, currency)
JOIN categories parent ON parent.slug = 'pjese-dhe-aksesore';

-- Which years of the vehicle a part fits. A pair "<x>_from_year" and "<x>_to_year" is one filter,
-- "<x>_year", the year of the buyer's vehicle (web/filters.rs); and like every "_year" property,
-- no later than next year (web/properties.rs).
INSERT INTO attributes (code, name, name_i18n, data_type, validation_json)
VALUES ('fits_from_year', 'Nga viti', '{"en": "From year"}', 'NUMBER', '{"min": 1950}'),
       ('fits_to_year', 'Deri në vitin', '{"en": "To year"}', 'NUMBER', '{"min": 1950}');

-- On every kind of part, none required: the cars' make and model (their lists, "Tjetër" included),
-- shown on cards and filtered by, and the years.
INSERT INTO category_attributes (category_id, attribute_id, required, searchable, filterable, show_in_card,
                                 show_in_details, sort_order, label)
SELECT parts.id, a.id, false, v.searchable, true, v.show_in_card, true, v.sort_order, v.label
FROM (VALUES ('vehicle_make', 'Marka e automjetit', true, true, 10),
             ('vehicle_model', 'Modeli i automjetit', true, true, 20),
             ('fits_from_year', NULL, false, false, 30),
             ('fits_to_year', NULL, false, false, 40)) AS v(code, label, searchable, show_in_card, sort_order)
JOIN attributes a ON a.code = v.code
JOIN categories parts ON parts.slug = 'pjese-dhe-aksesore';

DO $$
BEGIN
    IF (SELECT count(*) FROM categories
        WHERE parent_id = (SELECT id FROM categories WHERE slug = 'pjese-dhe-aksesore')) <> 9 THEN
        RAISE EXCEPTION 'expected 9 kinds of parts';
    END IF;
    IF (SELECT count(*) FROM categories
        WHERE parent_id = (SELECT id FROM categories WHERE slug = 'automjete')) <> 4 THEN
        RAISE EXCEPTION 'expected Automjete to keep 4 subcategories';
    END IF;
    IF (SELECT count(*) FROM effective_category_attributes e
        JOIN categories c ON c.id = e.category_id WHERE c.slug = 'gps-dhe-navigatore') <> 4 THEN
        RAISE EXCEPTION 'expected the kinds of parts to inherit the vehicle properties';
    END IF;
END
$$;
