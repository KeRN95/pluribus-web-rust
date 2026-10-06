-- Properties for every category, and what browsing needs to filter on them.
--
-- Properties (attributes) are configuration: the site builds its filters, the details on
-- listing cards and the listing page from these rows, so adding a property to a category is a
-- migration like this one, with no code change. See README, "Category properties".

-- ============================================================
-- CATEGORY SETTINGS
-- ============================================================

-- What kinds of listing a category takes (the filter offers them when there is more than one),
-- and whether its listings have a condition (new, used...). Subcategories have their own values.
ALTER TABLE categories
    ADD COLUMN listing_types varchar(20)[] NOT NULL DEFAULT '{SELL,BUY}'
        CHECK (cardinality(listing_types) > 0
               AND listing_types <@ '{SELL,BUY,RENT,WANTED_RENT,SERVICE,JOB}'),
    ADD COLUMN uses_condition boolean NOT NULL DEFAULT true;

UPDATE categories SET listing_types = '{SELL,BUY,RENT}'
WHERE slug IN ('automjete', 'makina', 'makineri-dhe-biznes');

UPDATE categories SET listing_types = '{SELL,RENT,WANTED_RENT}', uses_condition = false
WHERE slug = 'patundshmeri';

UPDATE categories SET listing_types = '{JOB}', uses_condition = false WHERE slug = 'pune';
UPDATE categories SET listing_types = '{SERVICE}', uses_condition = false WHERE slug = 'sherbime';
UPDATE categories SET uses_condition = false WHERE slug = 'kafshe-shtepiake';

-- Every category's properties, including those of its parent categories. Where a subcategory
-- configures a property its parent also has, the subcategory's settings win.
CREATE VIEW effective_category_attributes AS
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
       ca.show_in_card, ca.show_in_details, ca.sort_order
FROM lineage
JOIN category_attributes ca ON ca.category_id = lineage.source_id
ORDER BY lineage.category_id, ca.attribute_id, lineage.depth;

-- ============================================================
-- PRICES
-- ============================================================

-- What one unit of each currency is worth in euros. listings.price_normalized is price_amount
-- converted with these rates (price_in_eur), and price filters convert their bounds the same
-- way, so the two agree. The lek rate is a round placeholder: before launch, load the Bank of
-- Albania's rate daily and recompute price_normalized for listings priced in lekë.
CREATE TABLE exchange_rates (
    currency            currency_code PRIMARY KEY,
    eur_rate            numeric(18,8) NOT NULL CHECK (eur_rate > 0),
    updated_at          timestamptz NOT NULL DEFAULT now()
);

INSERT INTO exchange_rates (currency, eur_rate) VALUES ('EUR', 1), ('ALL', 0.01)
ON CONFLICT DO NOTHING;

CREATE FUNCTION price_in_eur(amount numeric, code text)
RETURNS numeric
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN round(amount * (SELECT eur_rate FROM exchange_rates WHERE currency = code), 2);

-- ============================================================
-- DISPLAY AND SEARCH
-- ============================================================

-- Numbers with a unit get grouped thousands and a decimal comma ("148 000 km", "1,6 l");
-- numbers without one, like a year, are shown as they are.
CREATE FUNCTION format_attribute_number(value numeric, unit text)
RETURNS text
LANGUAGE sql
IMMUTABLE PARALLEL SAFE
RETURN CASE
    WHEN unit IS NULL THEN replace(trim_scale(value)::text, '.', ',')
    ELSE regexp_replace(trunc(value)::text, '(\d)(?=(\d{3})+$)', E'\\1\u00a0', 'g')
         || CASE WHEN value <> trunc(value)
                 THEN ',' || split_part(trim_scale(abs(value))::text, '.', 2)
                 ELSE ''
            END
         || E'\u00a0' || unit
END;

-- The labels of a listing's searchable properties, for listings.search_extra. Run it whenever
-- a listing's property values change:
--   UPDATE listings SET search_extra = listing_search_extra(id) WHERE id = $1
CREATE FUNCTION listing_search_extra(listing uuid)
RETURNS text
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN (
    SELECT string_agg(label, ' ' ORDER BY sort_order, label)
    FROM (
        SELECT eca.sort_order, coalesce(o.label, v.value_text, trim_scale(v.value_number)::text) AS label
        FROM listings l
        JOIN effective_category_attributes eca ON eca.category_id = l.category_id AND eca.searchable
        JOIN listing_attribute_values v ON v.listing_id = l.id AND v.attribute_id = eca.attribute_id
        LEFT JOIN attribute_options o ON o.id = v.value_option_id
        WHERE l.id = listing
        UNION ALL
        SELECT eca.sort_order, o.label
        FROM listings l
        JOIN effective_category_attributes eca ON eca.category_id = l.category_id AND eca.searchable
        JOIN listing_attribute_options lo ON lo.listing_id = l.id AND lo.attribute_id = eca.attribute_id
        JOIN attribute_options o ON o.id = lo.option_id
        WHERE l.id = listing
    ) labels
);

-- ============================================================
-- PROPERTIES
-- ============================================================

INSERT INTO attributes (id, code, name, name_i18n, data_type, unit, validation_json)
VALUES
('20000000-0000-0000-0000-000000000008', 'vehicle_body_type',   'Karroceria',        '{"en": "Body type"}',     'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000009', 'vehicle_color',       'Ngjyra',            '{"en": "Colour"}',        'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000010', 'property_type',       'Lloji i pronës',    '{"en": "Property type"}', 'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000011', 'property_rooms',      'Dhoma',             '{"en": "Rooms"}',         'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000012', 'property_area',       'Sipërfaqja',        '{"en": "Area"}',          'NUMBER',      'm²', '{"min": 1}'),
('20000000-0000-0000-0000-000000000013', 'property_floor',      'Kati',              '{"en": "Floor"}',         'NUMBER',      NULL, '{"min": -3, "max": 60}'),
('20000000-0000-0000-0000-000000000014', 'property_furnished',  'Mobilimi',          '{"en": "Furnishing"}',    'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000015', 'property_features',   'Lehtësitë',         '{"en": "Features"}',      'MULTISELECT', NULL, '{}'),
('20000000-0000-0000-0000-000000000016', 'electronics_type',    'Lloji',             '{"en": "Type"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000017', 'electronics_brand',   'Marka',             '{"en": "Brand"}',         'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000018', 'electronics_storage', 'Memoria',           '{"en": "Storage"}',       'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000019', 'home_type',           'Lloji',             '{"en": "Type"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000020', 'job_schedule',        'Orari',             '{"en": "Schedule"}',      'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000021', 'job_field',           'Fusha',             '{"en": "Field"}',         'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000022', 'service_type',        'Lloji i shërbimit', '{"en": "Service type"}',  'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000023', 'fashion_for',         'Për',               '{"en": "For"}',           'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000024', 'fashion_type',        'Lloji',             '{"en": "Type"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000025', 'fashion_size',        'Masa',              '{"en": "Size"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000026', 'sport_type',          'Lloji',             '{"en": "Type"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000027', 'pet_type',            'Kafsha',            '{"en": "Animal"}',        'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000028', 'machine_type',        'Lloji',             '{"en": "Type"}',          'SELECT',      NULL, '{}'),
('20000000-0000-0000-0000-000000000029', 'machine_year',        'Viti',              '{"en": "Year"}',          'NUMBER',      NULL, '{"min": 1950}')
ON CONFLICT DO NOTHING;

INSERT INTO attribute_options (attribute_id, value_code, label, label_i18n, sort_order)
SELECT ('20000000-0000-0000-0000-0000000000' || v.attr)::uuid, v.code, v.label, v.en::jsonb, v.sort_order
FROM (VALUES
    ('08', 'SEDAN',        'Sedan',                     '{}',                              10),
    ('08', 'HATCHBACK',    'Hatchback',                 '{}',                              20),
    ('08', 'WAGON',        'Karavan',                   '{"en": "Estate"}',                30),
    ('08', 'SUV',          'SUV / Xhip',                '{"en": "SUV"}',                   40),
    ('08', 'COUPE',        'Kupe',                      '{"en": "Coupe"}',                 50),
    ('08', 'CONVERTIBLE',  'Kabriolet',                 '{"en": "Convertible"}',           60),
    ('08', 'VAN',          'Minivan',                   '{}',                              70),
    ('09', 'BLACK',        'E zezë',                    '{"en": "Black"}',                 10),
    ('09', 'WHITE',        'E bardhë',                  '{"en": "White"}',                 20),
    ('09', 'GREY',         'Gri',                       '{"en": "Grey"}',                  30),
    ('09', 'SILVER',       'Argjendi',                  '{"en": "Silver"}',                40),
    ('09', 'BLUE',         'Blu',                       '{"en": "Blue"}',                  50),
    ('09', 'RED',          'E kuqe',                    '{"en": "Red"}',                   60),
    ('09', 'OTHER',        'Tjetër',                    '{"en": "Other"}',                 70),
    ('10', 'APARTMENT',    'Apartament',                '{"en": "Apartment"}',             10),
    ('10', 'STUDIO',       'Garsonierë',                '{"en": "Studio"}',                20),
    ('10', 'HOUSE',        'Vilë / Shtëpi',             '{"en": "House / villa"}',         30),
    ('10', 'SHOP',         'Dyqan',                     '{"en": "Shop"}',                  40),
    ('10', 'OFFICE',       'Zyrë',                      '{"en": "Office"}',                50),
    ('10', 'LAND',         'Truall',                    '{"en": "Land"}',                  60),
    ('10', 'GARAGE',       'Garazh',                    '{"en": "Garage"}',                70),
    ('11', 'R11',          '1+1',                       '{}',                              10),
    ('11', 'R21',          '2+1',                       '{}',                              20),
    ('11', 'R31',          '3+1',                       '{}',                              30),
    ('11', 'R41',          '4+1 e më shumë',            '{"en": "4+1 or more"}',           40),
    ('14', 'FURNISHED',    'I mobiluar',                '{"en": "Furnished"}',             10),
    ('14', 'PARTLY',       'Gjysmë i mobiluar',         '{"en": "Partly furnished"}',      20),
    ('14', 'UNFURNISHED',  'I pamobiluar',              '{"en": "Unfurnished"}',           30),
    ('15', 'ELEVATOR',     'Ashensor',                  '{"en": "Lift"}',                  10),
    ('15', 'PARKING',      'Parkim',                    '{"en": "Parking"}',               20),
    ('15', 'BALCONY',      'Ballkon',                   '{"en": "Balcony"}',               30),
    ('15', 'HEATING',      'Ngrohje qendrore',          '{"en": "Central heating"}',       40),
    ('15', 'SEA_VIEW',     'Pamje nga deti',            '{"en": "Sea view"}',              50),
    ('15', 'TITLE_DEED',   'Me hipotekë',               '{"en": "Title deed"}',            60),
    ('16', 'PHONE',        'Telefon',                   '{"en": "Phone"}',                 10),
    ('16', 'LAPTOP',       'Laptop',                    '{}',                              20),
    ('16', 'DESKTOP',      'Kompjuter (PC)',            '{"en": "Desktop PC"}',            30),
    ('16', 'TABLET',       'Tablet',                    '{}',                              40),
    ('16', 'TV',           'Televizor',                 '{"en": "TV"}',                    50),
    ('16', 'CONSOLE',      'Konsolë lojërash',          '{"en": "Games console"}',         60),
    ('16', 'CAMERA',       'Aparat fotografik',         '{"en": "Camera"}',                70),
    ('16', 'AUDIO',        'Audio dhe kufje',           '{"en": "Audio and headphones"}',  80),
    ('17', 'APPLE',        'Apple',                     '{}',                              10),
    ('17', 'SAMSUNG',      'Samsung',                   '{}',                              20),
    ('17', 'XIAOMI',       'Xiaomi',                    '{}',                              30),
    ('17', 'LENOVO',       'Lenovo',                    '{}',                              40),
    ('17', 'DELL',         'Dell',                      '{}',                              50),
    ('17', 'HP',           'HP',                        '{}',                              60),
    ('17', 'ASUS',         'Asus',                      '{}',                              70),
    ('17', 'LG',           'LG',                        '{}',                              80),
    ('17', 'SONY',         'Sony',                      '{}',                              90),
    ('17', 'MICROSOFT',    'Microsoft',                 '{}',                             100),
    ('17', 'NINTENDO',     'Nintendo',                  '{}',                             110),
    ('17', 'CANON',        'Canon',                     '{}',                             120),
    ('17', 'OTHER',        'Tjetër',                    '{"en": "Other"}',                999),
    ('18', 'GB64',         '64 GB',                     '{}',                              10),
    ('18', 'GB128',        '128 GB',                    '{}',                              20),
    ('18', 'GB256',        '256 GB',                    '{}',                              30),
    ('18', 'GB512',        '512 GB',                    '{}',                              40),
    ('18', 'TB1',          '1 TB+',                     '{}',                              50),
    ('19', 'FURNITURE',    'Mobilje',                   '{"en": "Furniture"}',             10),
    ('19', 'APPLIANCE',    'Elektroshtëpiake',          '{"en": "Home appliances"}',       20),
    ('19', 'GARDEN',       'Kopsht',                    '{"en": "Garden"}',                30),
    ('19', 'DECOR',        'Dekor',                     '{"en": "Decor"}',                 40),
    ('20', 'FULL_TIME',    'Me kohë të plotë',          '{"en": "Full time"}',             10),
    ('20', 'PART_TIME',    'Me kohë të pjesshme',       '{"en": "Part time"}',             20),
    ('20', 'SEASONAL',     'Sezonale',                  '{"en": "Seasonal"}',              30),
    ('21', 'HOSPITALITY',  'Turizëm dhe gastronomi',    '{"en": "Hospitality"}',           10),
    ('21', 'CONSTRUCTION', 'Ndërtim',                   '{"en": "Construction"}',          20),
    ('21', 'HEALTH',       'Shëndetësi',                '{"en": "Healthcare"}',            30),
    ('21', 'IT',           'IT',                        '{}',                              40),
    ('21', 'SALES',        'Shitje',                    '{"en": "Sales"}',                 50),
    ('21', 'TRANSPORT',    'Transport',                 '{}',                              60),
    ('21', 'CALL_CENTER',  'Call center',               '{}',                              70),
    ('22', 'REPAIRS',      'Riparime',                  '{"en": "Repairs"}',               10),
    ('22', 'MOVING',       'Transport dhe transferime', '{"en": "Moving"}',                20),
    ('22', 'CLEANING',     'Pastrim',                   '{"en": "Cleaning"}',              30),
    ('22', 'LESSONS',      'Mësime',                    '{"en": "Lessons"}',               40),
    ('22', 'PHOTO',        'Foto dhe video',            '{"en": "Photo and video"}',       50),
    ('22', 'BUILDING',     'Ndërtim dhe lyerje',        '{"en": "Building and painting"}', 60),
    ('23', 'MEN',          'Meshkuj',                   '{"en": "Men"}',                   10),
    ('23', 'WOMEN',        'Femra',                     '{"en": "Women"}',                 20),
    ('23', 'KIDS',         'Fëmijë',                    '{"en": "Children"}',              30),
    ('23', 'UNISEX',       'Unisex',                    '{}',                              40),
    ('24', 'OUTERWEAR',    'Xhaketa dhe pallto',        '{"en": "Jackets and coats"}',     10),
    ('24', 'SHOES',        'Këpucë',                    '{"en": "Shoes"}',                 20),
    ('24', 'DRESSES',      'Fustane',                   '{"en": "Dresses"}',               30),
    ('24', 'BAGS',         'Çanta',                     '{"en": "Bags"}',                  40),
    ('24', 'WATCHES',      'Ora',                       '{"en": "Watches"}',               50),
    ('24', 'ACCESSORIES',  'Aksesorë',                  '{"en": "Accessories"}',           60),
    ('25', 'XS',           'XS',                        '{}',                              10),
    ('25', 'S',            'S',                         '{}',                              20),
    ('25', 'M',            'M',                         '{}',                              30),
    ('25', 'L',            'L',                         '{}',                              40),
    ('25', 'XL',           'XL',                        '{}',                              50),
    ('25', 'XXL',          'XXL',                       '{}',                              60),
    ('26', 'BICYCLES',     'Biçikleta',                 '{"en": "Bicycles"}',              10),
    ('26', 'FITNESS',      'Fitnes',                    '{"en": "Fitness"}',               20),
    ('26', 'MUSIC',        'Instrumente muzikore',      '{"en": "Musical instruments"}',   30),
    ('26', 'OUTDOOR',      'Kamping dhe peshkim',       '{"en": "Camping and fishing"}',   40),
    ('26', 'WINTER',       'Sporte dimërore',           '{"en": "Winter sports"}',         50),
    ('26', 'OTHER',        'Të tjera',                  '{"en": "Other"}',                 60),
    ('27', 'DOG',          'Qen',                       '{"en": "Dogs"}',                  10),
    ('27', 'CAT',          'Mace',                      '{"en": "Cats"}',                  20),
    ('27', 'BIRD',         'Zogj',                      '{"en": "Birds"}',                 30),
    ('27', 'FISH',         'Peshq',                     '{"en": "Fish"}',                  40),
    ('27', 'ACCESSORIES',  'Aksesorë',                  '{"en": "Accessories"}',           50),
    ('28', 'CONSTRUCTION', 'Ndërtim',                   '{"en": "Construction"}',          10),
    ('28', 'AGRICULTURE',  'Bujqësi',                   '{"en": "Agriculture"}',           20),
    ('28', 'ENERGY',       'Energji',                   '{"en": "Energy"}',                30),
    ('28', 'INDUSTRY',     'Industri',                  '{"en": "Industry"}',              40)
) AS v(attr, code, label, en, sort_order)
ON CONFLICT DO NOTHING;

-- Which category has which property. filterable: offered as a filter; show_in_card: shown on
-- listing cards, in sort_order. Subcategories inherit their parents' properties.
INSERT INTO category_attributes (category_id, attribute_id, required, searchable, filterable, show_in_card, sort_order)
SELECT ('10000000-0000-0000-0000-000000000' || v.category)::uuid,
       ('20000000-0000-0000-0000-0000000000' || v.attr)::uuid,
       v.required, v.searchable, v.filterable, v.show_in_card, v.sort_order
FROM (VALUES
    -- Makina (cars), alongside make, model, year, mileage, fuel, gearbox and features
    ('101', '08', false, true,  true,  false, 25),
    ('101', '09', false, true,  true,  false, 65),
    -- Patundshmëri (property)
    ('002', '10', true,  true,  true,  false, 10),
    ('002', '11', false, true,  true,  true,  20),
    ('002', '12', false, false, true,  true,  30),
    ('002', '13', false, false, true,  false, 40),
    ('002', '14', false, true,  true,  false, 50),
    ('002', '15', false, false, true,  false, 60),
    -- Elektronikë
    ('003', '16', true,  true,  true,  false, 10),
    ('003', '17', false, true,  true,  false, 20),
    ('003', '18', false, true,  true,  true,  30),
    -- Shtëpi dhe kopsht
    ('004', '19', false, true,  true,  false, 10),
    -- Punë (jobs)
    ('005', '21', true,  true,  true,  true,  10),
    ('005', '20', false, true,  true,  true,  20),
    -- Shërbime
    ('006', '22', true,  true,  true,  true,  10),
    -- Veshje dhe aksesorë (titles carry the size, so cards don't repeat it)
    ('007', '23', false, true,  true,  false, 10),
    ('007', '24', false, true,  true,  false, 20),
    ('007', '25', false, true,  true,  false, 30),
    -- Sport dhe hobi
    ('008', '26', false, true,  true,  false, 10),
    -- Kafshë shtëpiake
    ('009', '27', true,  true,  true,  false, 10),
    -- Makineri dhe biznes
    ('010', '28', false, true,  true,  false, 10),
    ('010', '29', false, true,  true,  true,  20)
) AS v(category, attr, required, searchable, filterable, show_in_card, sort_order)
ON CONFLICT DO NOTHING;

-- Car cards show year, mileage and fuel ("2016 · 148 000 km · Naftë"); make and model are
-- already in the title.
UPDATE category_attributes
SET show_in_card = attribute_id IN ('20000000-0000-0000-0000-000000000003',
                                    '20000000-0000-0000-0000-000000000004',
                                    '20000000-0000-0000-0000-000000000005')
WHERE category_id = '10000000-0000-0000-0000-000000000101';
