-- Reference data: Albania with its counties and municipalities, top-level categories, a sample car
-- attribute model, promotion products. Labels are Albanian; English goes in the *_i18n columns.
-- Safe to re-run (ON CONFLICT DO NOTHING).

-- ============================================================
-- LOCATIONS
-- ============================================================

INSERT INTO locations (id, parent_id, location_type, name, slug, country_code, sort_order)
VALUES ('00000000-0000-0000-0000-000000000001', NULL, 'COUNTRY', 'Shqipëri', 'shqiperi', 'AL', 0)
ON CONFLICT DO NOTHING;

-- The 12 counties (qarqe).
INSERT INTO locations (parent_id, location_type, name, slug, country_code, sort_order)
SELECT '00000000-0000-0000-0000-000000000001', 'REGION', county.name, county.slug, 'AL', county.sort_order
FROM (VALUES
    ('Berat',       'berat',        10),
    ('Dibër',       'diber',        20),
    ('Durrës',      'durres',       30),
    ('Elbasan',     'elbasan',      40),
    ('Fier',        'fier',         50),
    ('Gjirokastër', 'gjirokaster',  60),
    ('Korçë',       'korce',        70),
    ('Kukës',       'kukes',        80),
    ('Lezhë',       'lezhe',        90),
    ('Shkodër',     'shkoder',     100),
    ('Tiranë',      'tirane',      110),
    ('Vlorë',       'vlore',       120)
) AS county(name, slug, sort_order)
ON CONFLICT DO NOTHING;

-- The 61 municipalities (bashki) of the 2015 territorial reform. Towns, villages and neighbourhoods
-- belong below them: people search for "Peshkopi" or "Burrel", the seats of Dibër and Mat.
INSERT INTO locations (parent_id, location_type, name, slug, country_code)
SELECT county.id, 'MUNICIPALITY', m.name, m.slug, 'AL'
FROM (VALUES
    ('berat',       'Berat',          'berat'),
    ('berat',       'Kuçovë',         'kucove'),
    ('berat',       'Poliçan',        'polican'),
    ('berat',       'Skrapar',        'skrapar'),
    ('berat',       'Ura Vajgurore',  'ura-vajgurore'),
    ('diber',       'Bulqizë',        'bulqize'),
    ('diber',       'Dibër',          'diber'),
    ('diber',       'Klos',           'klos'),
    ('diber',       'Mat',            'mat'),
    ('durres',      'Durrës',         'durres'),
    ('durres',      'Krujë',          'kruje'),
    ('durres',      'Shijak',         'shijak'),
    ('elbasan',     'Belsh',          'belsh'),
    ('elbasan',     'Cërrik',         'cerrik'),
    ('elbasan',     'Elbasan',        'elbasan'),
    ('elbasan',     'Gramsh',         'gramsh'),
    ('elbasan',     'Librazhd',       'librazhd'),
    ('elbasan',     'Peqin',          'peqin'),
    ('elbasan',     'Prrenjas',       'prrenjas'),
    ('fier',        'Divjakë',        'divjake'),
    ('fier',        'Fier',           'fier'),
    ('fier',        'Lushnjë',        'lushnje'),
    ('fier',        'Mallakastër',    'mallakaster'),
    ('fier',        'Patos',          'patos'),
    ('fier',        'Roskovec',       'roskovec'),
    ('gjirokaster', 'Dropull',        'dropull'),
    ('gjirokaster', 'Gjirokastër',    'gjirokaster'),
    ('gjirokaster', 'Këlcyrë',        'kelcyre'),
    ('gjirokaster', 'Libohovë',       'libohove'),
    ('gjirokaster', 'Memaliaj',       'memaliaj'),
    ('gjirokaster', 'Përmet',         'permet'),
    ('gjirokaster', 'Tepelenë',       'tepelene'),
    ('korce',       'Devoll',         'devoll'),
    ('korce',       'Kolonjë',        'kolonje'),
    ('korce',       'Korçë',          'korce'),
    ('korce',       'Maliq',          'maliq'),
    ('korce',       'Pogradec',       'pogradec'),
    ('korce',       'Pustec',         'pustec'),
    ('kukes',       'Has',            'has'),
    ('kukes',       'Kukës',          'kukes'),
    ('kukes',       'Tropojë',        'tropoje'),
    ('lezhe',       'Kurbin',         'kurbin'),
    ('lezhe',       'Lezhë',          'lezhe'),
    ('lezhe',       'Mirditë',        'mirdite'),
    ('shkoder',     'Fushë-Arrëz',    'fushe-arrez'),
    ('shkoder',     'Malësi e Madhe', 'malesi-e-madhe'),
    ('shkoder',     'Pukë',           'puke'),
    ('shkoder',     'Shkodër',        'shkoder'),
    ('shkoder',     'Vau i Dejës',    'vau-i-dejes'),
    ('tirane',      'Kamëz',          'kamez'),
    ('tirane',      'Kavajë',         'kavaje'),
    ('tirane',      'Rrogozhinë',     'rrogozhine'),
    ('tirane',      'Tiranë',         'tirane'),
    ('tirane',      'Vorë',           'vore'),
    ('vlore',       'Delvinë',        'delvine'),
    ('vlore',       'Finiq',          'finiq'),
    ('vlore',       'Himarë',         'himare'),
    ('vlore',       'Konispol',       'konispol'),
    ('vlore',       'Sarandë',        'sarande'),
    ('vlore',       'Selenicë',       'selenice'),
    ('vlore',       'Vlorë',          'vlore')
) AS m(county_slug, name, slug)
JOIN locations county
  ON county.parent_id = '00000000-0000-0000-0000-000000000001'
 AND county.slug = m.county_slug
ON CONFLICT DO NOTHING;

-- ============================================================
-- CATEGORIES
-- ============================================================

INSERT INTO categories (id, parent_id, name, name_i18n, slug, description, description_i18n, icon_key, sort_order, default_currency)
VALUES
('10000000-0000-0000-0000-000000000001', NULL, 'Automjete', '{"en": "Vehicles"}', 'automjete',
 'Makina, motoçikleta, automjete tregtare dhe pjesë këmbimi', '{"en": "Cars, motorcycles, commercial vehicles and parts"}', 'car', 10, 'EUR'),
('10000000-0000-0000-0000-000000000002', NULL, 'Patundshmëri', '{"en": "Real Estate"}', 'patundshmeri',
 'Prona në shitje dhe me qira', '{"en": "Property for sale and rent"}', 'home', 20, 'EUR'),
('10000000-0000-0000-0000-000000000003', NULL, 'Elektronikë', '{"en": "Electronics"}', 'elektronike',
 'Telefona, kompjuterë, lojëra dhe elektronikë', '{"en": "Phones, computers, gaming and electronics"}', 'device', 30, 'ALL'),
('10000000-0000-0000-0000-000000000004', NULL, 'Shtëpi dhe kopsht', '{"en": "Home & Garden"}', 'shtepi-dhe-kopsht',
 'Mobilie, pajisje shtëpiake dhe artikuj për shtëpinë', '{"en": "Furniture, appliances and home goods"}', 'sofa', 40, 'ALL'),
('10000000-0000-0000-0000-000000000005', NULL, 'Punë', '{"en": "Jobs"}', 'pune',
 'Oferta pune dhe punëkërkues', '{"en": "Job offers and job seekers"}', 'briefcase', 50, 'ALL'),
('10000000-0000-0000-0000-000000000006', NULL, 'Shërbime', '{"en": "Services"}', 'sherbime',
 'Shërbime profesionale dhe lokale', '{"en": "Professional and local services"}', 'tools', 60, 'ALL'),
('10000000-0000-0000-0000-000000000007', NULL, 'Veshje dhe aksesorë', '{"en": "Fashion"}', 'veshje-dhe-aksesore',
 'Veshje, këpucë dhe aksesorë', '{"en": "Clothing, shoes and accessories"}', 'shirt', 70, 'ALL'),
('10000000-0000-0000-0000-000000000008', NULL, 'Sport dhe hobi', '{"en": "Sports & Leisure"}', 'sport-dhe-hobi',
 'Sport, hobi dhe kohë e lirë', '{"en": "Sports, hobbies and leisure"}', 'ball', 80, 'ALL'),
('10000000-0000-0000-0000-000000000009', NULL, 'Kafshë shtëpiake', '{"en": "Pets"}', 'kafshe-shtepiake',
 'Kafshë dhe artikuj për kafshë', '{"en": "Pets and pet supplies"}', 'paw', 90, 'ALL'),
('10000000-0000-0000-0000-000000000010', NULL, 'Makineri dhe biznes', '{"en": "Machinery & Business"}', 'makineri-dhe-biznes',
 'Makineri, vegla dhe pajisje për biznes', '{"en": "Machinery, tools and commercial equipment"}', 'factory', 100, 'ALL'),
('10000000-0000-0000-0000-000000000011', NULL, 'Të tjera', '{"en": "Other"}', 'te-tjera',
 'Njoftime të tjera', '{"en": "Other classifieds"}', 'tag', 999, 'ALL')
ON CONFLICT DO NOTHING;

-- The car attributes below hang off this subcategory. On the Automjete root they would also be
-- required for motorcycles and spare parts.
INSERT INTO categories (id, parent_id, name, name_i18n, slug, description, description_i18n, icon_key, sort_order, default_currency)
VALUES ('10000000-0000-0000-0000-000000000101', '10000000-0000-0000-0000-000000000001', 'Makina', '{"en": "Cars"}', 'makina',
        'Makina të reja dhe të përdorura', '{"en": "New and used cars"}', 'car', 10, 'EUR')
ON CONFLICT DO NOTHING;

-- ============================================================
-- SAMPLE ATTRIBUTE MODEL (CARS)
-- ============================================================

INSERT INTO attributes (id, code, name, name_i18n, data_type, unit, parent_attribute_id, validation_json)
VALUES
('20000000-0000-0000-0000-000000000001', 'vehicle_make',         'Marka',        '{"en": "Make"}',         'SELECT',      NULL, NULL,                                   '{}'),
('20000000-0000-0000-0000-000000000002', 'vehicle_model',        'Modeli',       '{"en": "Model"}',        'SELECT',      NULL, '20000000-0000-0000-0000-000000000001', '{}'),
('20000000-0000-0000-0000-000000000003', 'vehicle_year',         'Viti',         '{"en": "Year"}',         'NUMBER',      NULL, NULL,                                   '{"min": 1900}'),
('20000000-0000-0000-0000-000000000004', 'vehicle_mileage',      'Kilometrazhi', '{"en": "Mileage"}',      'NUMBER',      'km', NULL,                                   '{"min": 0}'),
('20000000-0000-0000-0000-000000000005', 'vehicle_fuel',         'Karburanti',   '{"en": "Fuel"}',         'SELECT',      NULL, NULL,                                   '{}'),
('20000000-0000-0000-0000-000000000006', 'vehicle_transmission', 'Kambio',       '{"en": "Transmission"}', 'SELECT',      NULL, NULL,                                   '{}'),
('20000000-0000-0000-0000-000000000007', 'vehicle_features',     'Pajisjet',     '{"en": "Features"}',     'MULTISELECT', NULL, NULL,                                   '{}')
ON CONFLICT DO NOTHING;

INSERT INTO attribute_options (attribute_id, value_code, label, label_i18n, sort_order)
VALUES
('20000000-0000-0000-0000-000000000005', 'PETROL',           'Benzinë',         '{"en": "Petrol"}',           10),
('20000000-0000-0000-0000-000000000005', 'DIESEL',           'Naftë',           '{"en": "Diesel"}',           20),
('20000000-0000-0000-0000-000000000005', 'HYBRID',           'Hibrid',          '{"en": "Hybrid"}',           30),
('20000000-0000-0000-0000-000000000005', 'ELECTRIC',         'Elektrik',        '{"en": "Electric"}',         40),
('20000000-0000-0000-0000-000000000005', 'LPG',              'Benzinë + Gaz',   '{"en": "Petrol + LPG"}',     50),
('20000000-0000-0000-0000-000000000006', 'MANUAL',           'Manuale',         '{"en": "Manual"}',           10),
('20000000-0000-0000-0000-000000000006', 'AUTOMATIC',        'Automatike',      '{"en": "Automatic"}',        20),
('20000000-0000-0000-0000-000000000007', 'ABS',              'ABS',             '{}',                         10),
('20000000-0000-0000-0000-000000000007', 'AIR_CONDITIONING', 'Kondicioner',     '{"en": "Air conditioning"}', 20),
('20000000-0000-0000-0000-000000000007', 'NAVIGATION',       'Navigacion',      '{"en": "Navigation"}',       30),
('20000000-0000-0000-0000-000000000007', 'PARKING_SENSORS',  'Sensorë parkimi', '{"en": "Parking sensors"}',  40)
ON CONFLICT DO NOTHING;

-- A few makes and models to show the dependent dropdown; import the full catalogue separately.
INSERT INTO attribute_options (attribute_id, value_code, label, sort_order)
VALUES
('20000000-0000-0000-0000-000000000001', 'AUDI',          'Audi',          10),
('20000000-0000-0000-0000-000000000001', 'BMW',           'BMW',           20),
('20000000-0000-0000-0000-000000000001', 'MERCEDES_BENZ', 'Mercedes-Benz', 30),
('20000000-0000-0000-0000-000000000001', 'TOYOTA',        'Toyota',        40),
('20000000-0000-0000-0000-000000000001', 'VOLKSWAGEN',    'Volkswagen',    50)
ON CONFLICT DO NOTHING;

INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, sort_order)
SELECT '20000000-0000-0000-0000-000000000002', make.id, model.value_code, model.label, model.sort_order
FROM (VALUES
    ('AUDI',          'AUDI_A4',               'A4',       10),
    ('AUDI',          'AUDI_A6',               'A6',       20),
    ('BMW',           'BMW_3_SERIES',          '3 Series', 10),
    ('BMW',           'BMW_5_SERIES',          '5 Series', 20),
    ('MERCEDES_BENZ', 'MERCEDES_BENZ_C_CLASS', 'C-Class',  10),
    ('MERCEDES_BENZ', 'MERCEDES_BENZ_E_CLASS', 'E-Class',  20),
    ('TOYOTA',        'TOYOTA_COROLLA',        'Corolla',  10),
    ('TOYOTA',        'TOYOTA_YARIS',          'Yaris',    20),
    ('VOLKSWAGEN',    'VOLKSWAGEN_GOLF',       'Golf',     10),
    ('VOLKSWAGEN',    'VOLKSWAGEN_PASSAT',     'Passat',   20)
) AS model(make_code, value_code, label, sort_order)
JOIN attribute_options make
  ON make.attribute_id = '20000000-0000-0000-0000-000000000001'
 AND make.value_code = model.make_code
ON CONFLICT DO NOTHING;

INSERT INTO category_attributes (category_id, attribute_id, required, searchable, filterable, show_in_card, sort_order)
VALUES
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000001', true,  true,  true, true,  10),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000002', true,  true,  true, true,  20),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000003', true,  true,  true, true,  30),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000004', false, false, true, true,  40),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000005', false, true,  true, true,  50),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000006', false, true,  true, true,  60),
('10000000-0000-0000-0000-000000000101', '20000000-0000-0000-0000-000000000007', false, false, true, false, 70)
ON CONFLICT DO NOTHING;

-- ============================================================
-- PROMOTION PRODUCTS
-- ============================================================

-- Prices are placeholders. If you charge in lekë, change the prices and currencies here.
INSERT INTO promotion_products (code, name, name_i18n, promotion_type, duration_hours, price_amount, currency)
VALUES
('BUMP_ONCE',       'Ngritje në krye',                  '{"en": "Bump listing"}',               'BUMP',         NULL,  1.99, 'EUR'),
('FEATURED_3D',     'I veçuar për 3 ditë',              '{"en": "Featured for 3 days"}',        'FEATURED',       72,  4.99, 'EUR'),
('FEATURED_7D',     'I veçuar për 7 ditë',              '{"en": "Featured for 7 days"}',        'FEATURED',      168,  8.99, 'EUR'),
('CATEGORY_TOP_7D', 'Në krye të kategorisë për 7 ditë', '{"en": "Top of category for 7 days"}', 'CATEGORY_TOP',  168, 12.99, 'EUR')
ON CONFLICT DO NOTHING;
