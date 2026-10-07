-- Electronics' own properties, subcategory by subcategory, for the seller to give if they like and
-- for buyers to filter by: a phone's model, RAM, battery and colour, a laptop's processor, screen
-- and graphics, a TV's size and resolution, a camera's kind and sensor, a console's model. Brands
-- and storage sizes were one list for all electronics, Canon offered for phones and a terabyte
-- for TVs; now each subcategory offers its own (category_attribute_options), and storage goes
-- only where it means something.

-- The options a category offers for a property, where it offers only some, in its own order: a
-- phone's brands, a laptop's storage sizes. A category that lists none of a property's options
-- offers them all, as a parent category's filters do.
CREATE TABLE category_attribute_options (
    category_id uuid NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    option_id   uuid NOT NULL REFERENCES attribute_options(id) ON DELETE CASCADE,
    sort_order  integer NOT NULL DEFAULT 0,
    PRIMARY KEY (category_id, option_id)
);
CREATE INDEX idx_category_attribute_options_option ON category_attribute_options(option_id);

-- The options a category offers for a property, in its order: the ones it lists, where it lists
-- any, otherwise all of the property's. Only listed ones, never what sellers typed in and nobody
-- has approved yet.
CREATE FUNCTION category_options(category uuid, attribute uuid)
RETURNS TABLE (id uuid, value_code varchar, label varchar, parent_option_id uuid, sort_order integer)
LANGUAGE sql STABLE
AS $$
    SELECT o.id, o.value_code, o.label, o.parent_option_id, coalesce(s.sort_order, o.sort_order)
    FROM attribute_options o
    LEFT JOIN category_attribute_options s ON s.category_id = category AND s.option_id = o.id
    WHERE o.attribute_id = attribute AND o.active
      AND (s.option_id IS NOT NULL
           OR NOT EXISTS (SELECT 1 FROM category_attribute_options listed
                          JOIN attribute_options lo ON lo.id = listed.option_id
                          WHERE listed.category_id = category AND lo.attribute_id = attribute))
$$;

-- ---------------------------------------------------------------- the properties

INSERT INTO attributes (code, name, name_i18n, data_type, unit, validation_json, allow_other)
VALUES
    ('electronics_warranty',   'Me garanci',              '{"en": "Under warranty"}',      'BOOLEAN',     NULL,       '{}',                         false),
    ('electronics_included',   'Vjen me',                 '{"en": "Comes with"}',          'MULTISELECT', NULL,       '{}',                         false),
    ('electronics_ram',        'RAM',                     '{"en": "RAM"}',                 'SELECT',      NULL,       '{}',                         false),
    ('electronics_color',      'Ngjyra',                  '{"en": "Colour"}',              'SELECT',      NULL,       '{}',                         false),
    ('electronics_features',   'Veçori',                  '{"en": "Features"}',            'MULTISELECT', NULL,       '{}',                         false),
    ('electronics_screen',     'Ekrani',                  '{"en": "Screen"}',              'NUMBER',      'inç',      '{"min": 3, "max": 120}',     false),
    ('electronics_device',     'Pajisja',                 '{"en": "Device"}',              'SELECT',      NULL,       '{}',                         false),
    ('phone_model',            'Modeli',                  '{"en": "Model"}',               'SELECT',      NULL,       '{}',                         true),
    ('phone_battery',          'Bateria',                 '{"en": "Battery health"}',      'NUMBER',      '%',        '{"min": 1, "max": 100}',     false),
    ('computer_cpu',           'Procesori',               '{"en": "Processor"}',           'SELECT',      NULL,       '{}',                         false),
    ('computer_disk',          'Lloji i diskut',          '{"en": "Disk type"}',           'SELECT',      NULL,       '{}',                         false),
    ('computer_gpu',           'Karta grafike',           '{"en": "Graphics card"}',       'SELECT',      NULL,       '{}',                         false),
    ('computer_os',            'Sistemi operativ',        '{"en": "Operating system"}',    'SELECT',      NULL,       '{}',                         false),
    ('computer_monitor',       'Me monitor',              '{"en": "With a monitor"}',      'BOOLEAN',     NULL,       '{}',                         false),
    ('tablet_connectivity',    'Lidhja',                  '{"en": "Connectivity"}',        'SELECT',      NULL,       '{}',                         false),
    ('tv_resolution',          'Rezolucioni',             '{"en": "Resolution"}',          'SELECT',      NULL,       '{}',                         false),
    ('tv_panel',               'Teknologjia e ekranit',   '{"en": "Panel"}',               'SELECT',      NULL,       '{}',                         false),
    ('tv_smart',               'Smart TV',                '{}',                            'BOOLEAN',     NULL,       '{}',                         false),
    ('audio_wireless',         'Me valë (Bluetooth)',     '{"en": "Wireless"}',            'BOOLEAN',     NULL,       '{}',                         false),
    ('audio_noise_cancelling', 'Anulim zhurme',           '{"en": "Noise cancelling"}',    'BOOLEAN',     NULL,       '{}',                         false),
    ('camera_sensor',          'Sensori',                 '{"en": "Sensor"}',              'SELECT',      NULL,       '{}',                         false),
    ('camera_mount',           'Montimi i objektivit',    '{"en": "Lens mount"}',          'SELECT',      NULL,       '{}',                         false),
    ('camera_shutter_count',   'Numri i shkrepjeve',      '{"en": "Shutter count"}',       'NUMBER',      'shkrepje', '{"min": 0, "max": 2000000}', false),
    ('console_model',          'Konsola',                 '{"en": "Console"}',             'SELECT',      NULL,       '{}',                         true),
    ('console_edition',        'Edicioni',                '{"en": "Edition"}',             'SELECT',      NULL,       '{}',                         false),
    ('console_games',          'Me lojëra',               '{"en": "With games"}',          'BOOLEAN',     NULL,       '{}',                         false),
    ('console_controllers',    'Kontrollues',             '{"en": "Controllers"}',         'NUMBER',      NULL,       '{"min": 0, "max": 8}',       false);

-- A phone's model and a console's are picked under their brand, as a car's model under its make.
UPDATE attributes
SET parent_attribute_id = (SELECT id FROM attributes WHERE code = 'electronics_brand')
WHERE code IN ('phone_model', 'console_model');

-- ---------------------------------------------------------------- their options

-- Brands the list lacked. One a seller typed in already (waiting for a moderator) is listed now.
INSERT INTO attribute_options (attribute_id, value_code, label, sort_order)
SELECT a.id, v.code, v.label, v.sort_order
FROM attributes a,
     (VALUES ('HUAWEI', 'Huawei', 130), ('GOOGLE', 'Google', 140), ('ONEPLUS', 'OnePlus', 150), ('OPPO', 'Oppo', 160),
             ('REALME', 'Realme', 170), ('HONOR', 'Honor', 180), ('MOTOROLA', 'Motorola', 190), ('NOKIA', 'Nokia', 200),
             ('ACER', 'Acer', 210), ('MSI', 'MSI', 220), ('AMAZON', 'Amazon', 230), ('TCL', 'TCL', 240),
             ('HISENSE', 'Hisense', 250), ('PHILIPS', 'Philips', 260), ('PANASONIC', 'Panasonic', 270), ('GRUNDIG', 'Grundig', 280),
             ('JBL', 'JBL', 290), ('BOSE', 'Bose', 300), ('SENNHEISER', 'Sennheiser', 310), ('MARSHALL', 'Marshall', 320),
             ('BANG_OLUFSEN', 'Bang & Olufsen', 330), ('HARMAN_KARDON', 'Harman Kardon', 340), ('YAMAHA', 'Yamaha', 350),
             ('PIONEER', 'Pioneer', 360), ('NIKON', 'Nikon', 370), ('FUJIFILM', 'Fujifilm', 380),
             ('OLYMPUS', 'Olympus / OM System', 390), ('LEICA', 'Leica', 400), ('GOPRO', 'GoPro', 410), ('DJI', 'DJI', 420),
             ('SIGMA', 'Sigma', 430), ('TAMRON', 'Tamron', 440), ('VALVE', 'Valve', 450), ('BUILT', 'PC i montuar', 460))
         AS v(code, label, sort_order)
WHERE a.code = 'electronics_brand'
ON CONFLICT (attribute_id, value_code) DO UPDATE SET active = true, label = EXCLUDED.label, sort_order = EXCLUDED.sort_order;

-- Storage: from 32 GB, the largest 2 TB and up; 1 TB is no longer the top of the list.
INSERT INTO attribute_options (attribute_id, value_code, label, sort_order)
SELECT a.id, v.code, v.label, v.sort_order
FROM attributes a, (VALUES ('GB32', '32 GB', 5), ('TB2', '2 TB e më shumë', 60)) AS v(code, label, sort_order)
WHERE a.code = 'electronics_storage';
UPDATE attribute_options SET label = '1 TB'
WHERE value_code = 'TB1' AND attribute_id = (SELECT id FROM attributes WHERE code = 'electronics_storage');

INSERT INTO attribute_options (attribute_id, value_code, label, label_i18n, sort_order)
SELECT a.id, v.code, v.label, v.en::jsonb, v.sort_order
FROM (VALUES
    ('electronics_included', 'BOX',              'Kutia origjinale',          '{"en": "Original box"}',          10),
    ('electronics_included', 'INVOICE',          'Fatura',                    '{"en": "Receipt"}',               20),
    ('electronics_included', 'CHARGER',          'Karikuesi',                 '{"en": "Charger"}',               30),
    ('electronics_included', 'CASE',             'Mbulesë',                   '{"en": "Case"}',                  40),
    ('electronics_included', 'PEN',              'Laps',                      '{"en": "Pen"}',                   50),
    ('electronics_included', 'KEYBOARD',         'Tastierë',                  '{"en": "Keyboard"}',              60),
    ('electronics_included', 'REMOTE',           'Telekomanda',               '{"en": "Remote"}',                70),
    ('electronics_ram',      'GB2',              '2 GB RAM',                  '{}',                              10),
    ('electronics_ram',      'GB3',              '3 GB RAM',                  '{}',                              20),
    ('electronics_ram',      'GB4',              '4 GB RAM',                  '{}',                              30),
    ('electronics_ram',      'GB6',              '6 GB RAM',                  '{}',                              40),
    ('electronics_ram',      'GB8',              '8 GB RAM',                  '{}',                              50),
    ('electronics_ram',      'GB12',             '12 GB RAM',                 '{}',                              60),
    ('electronics_ram',      'GB16',             '16 GB RAM',                 '{}',                              70),
    ('electronics_ram',      'GB24',             '24 GB RAM',                 '{}',                              80),
    ('electronics_ram',      'GB32',             '32 GB RAM',                 '{}',                              90),
    ('electronics_ram',      'GB64',             '64 GB RAM e më shumë',      '{"en": "64 GB RAM or more"}',    100),
    ('electronics_color',    'BLACK',            'E zezë',                    '{"en": "Black"}',                 10),
    ('electronics_color',    'WHITE',            'E bardhë',                  '{"en": "White"}',                 20),
    ('electronics_color',    'GREY',             'Gri',                       '{"en": "Grey"}',                  30),
    ('electronics_color',    'SILVER',           'Argjendi',                  '{"en": "Silver"}',                40),
    ('electronics_color',    'GOLD',             'E artë',                    '{"en": "Gold"}',                  50),
    ('electronics_color',    'BLUE',             'Blu',                       '{"en": "Blue"}',                  60),
    ('electronics_color',    'GREEN',            'E gjelbër',                 '{"en": "Green"}',                 70),
    ('electronics_color',    'RED',              'E kuqe',                    '{"en": "Red"}',                   80),
    ('electronics_color',    'PURPLE',           'Vjollcë',                   '{"en": "Purple"}',                90),
    ('electronics_color',    'PINK',             'Rozë',                      '{"en": "Pink"}',                 100),
    ('electronics_color',    'OTHER',            'Tjetër',                    '{"en": "Other"}',                999),
    ('electronics_features', 'FIVE_G',           '5G',                        '{}',                              10),
    ('electronics_features', 'DUAL_SIM',         'Dual SIM',                  '{}',                              20),
    ('electronics_features', 'ESIM',             'eSIM',                      '{}',                              30),
    ('electronics_features', 'TOUCHSCREEN',      'Ekran me prekje',           '{"en": "Touchscreen"}',           40),
    ('electronics_features', 'BACKLIT_KEYBOARD', 'Tastierë me ndriçim',       '{"en": "Backlit keyboard"}',      50),
    ('electronics_features', 'TWO_IN_ONE',       '2-në-1',                    '{"en": "2-in-1"}',                60),
    ('electronics_device',   'PC_DESKTOP',       'PC desktop',                '{"en": "Desktop PC"}',            10),
    ('electronics_device',   'PC_GAMING',        'PC gaming',                 '{"en": "Gaming PC"}',             20),
    ('electronics_device',   'ALL_IN_ONE',       'All-in-One',                '{}',                              30),
    ('electronics_device',   'MINI_PC',          'Mini PC',                   '{}',                              40),
    ('electronics_device',   'HEADPHONES',       'Kufje',                     '{"en": "Headphones"}',            50),
    ('electronics_device',   'EARBUDS',          'Kufje në vesh',             '{"en": "Earbuds"}',               60),
    ('electronics_device',   'SPEAKER',          'Altoparlant',               '{"en": "Speaker"}',               70),
    ('electronics_device',   'SOUNDBAR',         'Soundbar',                  '{}',                              80),
    ('electronics_device',   'HIFI',             'Hi-Fi / Home cinema',       '{}',                              90),
    ('electronics_device',   'AMPLIFIER',        'Amplifikator',              '{"en": "Amplifier"}',            100),
    ('electronics_device',   'TURNTABLE',        'Gramafon',                  '{"en": "Turntable"}',            110),
    ('electronics_device',   'MICROPHONE',       'Mikrofon',                  '{"en": "Microphone"}',           120),
    ('electronics_device',   'DSLR',             'DSLR',                      '{}',                             130),
    ('electronics_device',   'MIRRORLESS',       'Pa pasqyrë (mirrorless)',   '{"en": "Mirrorless"}',           140),
    ('electronics_device',   'COMPACT',          'Kompakte',                  '{"en": "Compact"}',              150),
    ('electronics_device',   'ACTION',           'Kamerë aksioni',            '{"en": "Action camera"}',        160),
    ('electronics_device',   'CAMCORDER',        'Kamerë video',              '{"en": "Camcorder"}',            170),
    ('electronics_device',   'FILM',             'Analoge (me film)',         '{"en": "Film camera"}',          180),
    ('electronics_device',   'LENS',             'Objektiv',                  '{"en": "Lens"}',                 190),
    ('electronics_device',   'DRONE',            'Dron',                      '{"en": "Drone"}',                200),
    ('computer_cpu',         'INTEL_CORE_I3',    'Intel Core i3',             '{}',                              10),
    ('computer_cpu',         'INTEL_CORE_I5',    'Intel Core i5',             '{}',                              20),
    ('computer_cpu',         'INTEL_CORE_I7',    'Intel Core i7',             '{}',                              30),
    ('computer_cpu',         'INTEL_CORE_I9',    'Intel Core i9',             '{}',                              40),
    ('computer_cpu',         'INTEL_CORE_ULTRA_5', 'Intel Core Ultra 5',      '{}',                              50),
    ('computer_cpu',         'INTEL_CORE_ULTRA_7', 'Intel Core Ultra 7',      '{}',                              60),
    ('computer_cpu',         'INTEL_CORE_ULTRA_9', 'Intel Core Ultra 9',      '{}',                              70),
    ('computer_cpu',         'INTEL_CELERON',    'Intel Celeron / Pentium',   '{}',                              80),
    ('computer_cpu',         'AMD_RYZEN_3',      'AMD Ryzen 3',               '{}',                              90),
    ('computer_cpu',         'AMD_RYZEN_5',      'AMD Ryzen 5',               '{}',                             100),
    ('computer_cpu',         'AMD_RYZEN_7',      'AMD Ryzen 7',               '{}',                             110),
    ('computer_cpu',         'AMD_RYZEN_9',      'AMD Ryzen 9',               '{}',                             120),
    ('computer_cpu',         'APPLE_M1',         'Apple M1',                  '{}',                             130),
    ('computer_cpu',         'APPLE_M2',         'Apple M2',                  '{}',                             140),
    ('computer_cpu',         'APPLE_M3',         'Apple M3',                  '{}',                             150),
    ('computer_cpu',         'APPLE_M4',         'Apple M4',                  '{}',                             160),
    ('computer_cpu',         'APPLE_M5',         'Apple M5',                  '{}',                             170),
    ('computer_cpu',         'SNAPDRAGON_X',     'Qualcomm Snapdragon X',     '{}',                             180),
    ('computer_cpu',         'OTHER',            'Tjetër',                    '{"en": "Other"}',                999),
    ('computer_disk',        'SSD',              'SSD',                       '{}',                              10),
    ('computer_disk',        'HDD',              'HDD',                       '{}',                              20),
    ('computer_disk',        'SSD_HDD',          'SSD + HDD',                 '{}',                              30),
    ('computer_gpu',         'INTEGRATED',       'E integruar',               '{"en": "Integrated"}',            10),
    ('computer_gpu',         'NVIDIA',           'NVIDIA GeForce',            '{}',                              20),
    ('computer_gpu',         'AMD',              'AMD Radeon',                '{}',                              30),
    ('computer_os',          'WINDOWS',          'Windows',                   '{}',                              10),
    ('computer_os',          'MACOS',            'macOS',                     '{}',                              20),
    ('computer_os',          'LINUX',            'Linux',                     '{}',                              30),
    ('computer_os',          'CHROMEOS',         'ChromeOS',                  '{}',                              40),
    ('computer_os',          'NONE',             'Pa sistem',                 '{"en": "None"}',                  50),
    ('tablet_connectivity',  'WIFI',             'Wi-Fi',                     '{}',                              10),
    ('tablet_connectivity',  'CELLULAR',         'Wi-Fi + SIM',               '{"en": "Wi-Fi + cellular"}',      20),
    ('tv_resolution',        'HD',               'HD',                        '{}',                              10),
    ('tv_resolution',        'FULL_HD',          'Full HD',                   '{}',                              20),
    ('tv_resolution',        'UHD_4K',           '4K Ultra HD',               '{}',                              30),
    ('tv_resolution',        'UHD_8K',           '8K',                        '{}',                              40),
    ('tv_panel',             'LED',              'LED / LCD',                 '{}',                              10),
    ('tv_panel',             'QLED',             'QLED',                      '{}',                              20),
    ('tv_panel',             'OLED',             'OLED',                      '{}',                              30),
    ('tv_panel',             'MINI_LED',         'Mini-LED',                  '{}',                              40),
    ('camera_sensor',        'FULL_FRAME',       'Full-frame',                '{}',                              10),
    ('camera_sensor',        'APS_C',            'APS-C',                     '{}',                              20),
    ('camera_sensor',        'MFT',              'Micro 4/3',                 '{}',                              30),
    ('camera_sensor',        'ONE_INCH',         '1″ e më i vogël',           '{"en": "1 inch or smaller"}',     40),
    ('camera_mount',         'CANON_EF',         'Canon EF / EF-S',           '{}',                              10),
    ('camera_mount',         'CANON_RF',         'Canon RF',                  '{}',                              20),
    ('camera_mount',         'NIKON_F',          'Nikon F',                   '{}',                              30),
    ('camera_mount',         'NIKON_Z',          'Nikon Z',                   '{}',                              40),
    ('camera_mount',         'SONY_E',           'Sony E / FE',               '{}',                              50),
    ('camera_mount',         'FUJIFILM_X',       'Fujifilm X',                '{}',                              60),
    ('camera_mount',         'MFT',              'Micro 4/3',                 '{}',                              70),
    ('camera_mount',         'L_MOUNT',          'L-Mount',                   '{}',                              80),
    ('camera_mount',         'OTHER',            'Tjetër',                    '{"en": "Other"}',                999),
    ('console_edition',      'DISC',             'Me disk',                   '{"en": "Disc"}',                  10),
    ('console_edition',      'DIGITAL',          'Digital',                   '{}',                              20)
) AS v(attr, code, label, en, sort_order)
JOIN attributes a ON a.code = v.attr;

-- Phones' models under their brands, newest first; a model the list lacks the seller types in.
-- Codes start with the brand, as two brands have a "13": APPLE_IPHONE_13, ONEPLUS_13,
-- SAMSUNG_GALAXY_S25_PLUS.
INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, sort_order)
SELECT a.id, brand.id,
       models.brand || '_' || upper(btrim(regexp_replace(replace(m.label, '+', ' Plus'), '[^a-zA-Z0-9]+', '_', 'g'), '_')),
       m.label, m.n * 10
FROM attributes a
CROSS JOIN LATERAL (VALUES
    ('APPLE', ARRAY['iPhone 17 Pro Max', 'iPhone 17 Pro', 'iPhone Air', 'iPhone 17', 'iPhone 16 Pro Max', 'iPhone 16 Pro',
                    'iPhone 16 Plus', 'iPhone 16', 'iPhone 16e', 'iPhone 15 Pro Max', 'iPhone 15 Pro', 'iPhone 15 Plus', 'iPhone 15',
                    'iPhone 14 Pro Max', 'iPhone 14 Pro', 'iPhone 14 Plus', 'iPhone 14', 'iPhone 13 Pro Max', 'iPhone 13 Pro',
                    'iPhone 13', 'iPhone 13 mini', 'iPhone 12 Pro Max', 'iPhone 12 Pro', 'iPhone 12', 'iPhone 12 mini', 'iPhone SE',
                    'iPhone 11 Pro Max', 'iPhone 11 Pro', 'iPhone 11', 'iPhone XS Max', 'iPhone XS', 'iPhone XR', 'iPhone X',
                    'iPhone 8 Plus', 'iPhone 8']),
    ('SAMSUNG', ARRAY['Galaxy S25 Ultra', 'Galaxy S25 Edge', 'Galaxy S25+', 'Galaxy S25', 'Galaxy S25 FE', 'Galaxy S24 Ultra',
                      'Galaxy S24+', 'Galaxy S24', 'Galaxy S24 FE', 'Galaxy S23 Ultra', 'Galaxy S23+', 'Galaxy S23', 'Galaxy S23 FE',
                      'Galaxy S22 Ultra', 'Galaxy S22+', 'Galaxy S22', 'Galaxy S21 Ultra', 'Galaxy S21+', 'Galaxy S21', 'Galaxy S21 FE',
                      'Galaxy S20 FE', 'Galaxy S20', 'Galaxy Note 20 Ultra', 'Galaxy Note 20', 'Galaxy Z Fold7', 'Galaxy Z Fold6',
                      'Galaxy Z Fold5', 'Galaxy Z Flip7', 'Galaxy Z Flip6', 'Galaxy Z Flip5', 'Galaxy A56', 'Galaxy A55',
                      'Galaxy A54', 'Galaxy A36', 'Galaxy A35', 'Galaxy A34', 'Galaxy A26', 'Galaxy A25', 'Galaxy A16',
                      'Galaxy A15', 'Galaxy A14']),
    ('XIAOMI', ARRAY['15 Ultra', '15', '14T Pro', '14T', '14', '13T Pro', '13T', '13', 'Redmi Note 14 Pro+', 'Redmi Note 14 Pro',
                     'Redmi Note 14', 'Redmi Note 13 Pro+', 'Redmi Note 13 Pro', 'Redmi Note 13', 'Redmi Note 12 Pro',
                     'Redmi Note 12', 'Redmi 14C', 'Redmi 13', 'Redmi 13C', 'Poco F6 Pro', 'Poco F6', 'Poco X7 Pro', 'Poco X7',
                     'Poco X6 Pro', 'Poco X6']),
    ('HUAWEI', ARRAY['Pura 70 Pro', 'Pura 70', 'P60 Pro', 'P50 Pro', 'P40 Pro', 'P40', 'P30 Pro', 'P30', 'Mate 50 Pro',
                     'Mate 40 Pro', 'Mate 20 Pro', 'Nova 12', 'Nova 11', 'Nova 10', 'P Smart']),
    ('GOOGLE', ARRAY['Pixel 10 Pro XL', 'Pixel 10 Pro', 'Pixel 10', 'Pixel 9 Pro XL', 'Pixel 9 Pro', 'Pixel 9', 'Pixel 9a',
                     'Pixel 8 Pro', 'Pixel 8', 'Pixel 8a', 'Pixel 7 Pro', 'Pixel 7', 'Pixel 7a', 'Pixel 6 Pro', 'Pixel 6', 'Pixel 6a']),
    ('ONEPLUS', ARRAY['13', '12', '11', '10 Pro', 'Nord 4', 'Nord CE4', 'Nord 3', 'Nord CE3'])
) AS models (brand, labels)
CROSS JOIN LATERAL unnest(models.labels) WITH ORDINALITY AS m (label, n)
JOIN attribute_options brand ON brand.value_code = models.brand
                            AND brand.attribute_id = (SELECT id FROM attributes WHERE code = 'electronics_brand')
WHERE a.code = 'phone_model';

-- Consoles under their makers.
INSERT INTO attribute_options (attribute_id, parent_option_id, value_code, label, sort_order)
SELECT a.id, brand.id, v.code, v.label, v.sort_order
FROM (VALUES
    ('SONY', 'PLAYSTATION_5_PRO', 'PlayStation 5 Pro', 10), ('SONY', 'PLAYSTATION_5', 'PlayStation 5', 20),
    ('SONY', 'PLAYSTATION_4_PRO', 'PlayStation 4 Pro', 30), ('SONY', 'PLAYSTATION_4', 'PlayStation 4', 40),
    ('SONY', 'PLAYSTATION_3', 'PlayStation 3', 50),
    ('MICROSOFT', 'XBOX_SERIES_X', 'Xbox Series X', 10), ('MICROSOFT', 'XBOX_SERIES_S', 'Xbox Series S', 20),
    ('MICROSOFT', 'XBOX_ONE_X', 'Xbox One X', 30), ('MICROSOFT', 'XBOX_ONE_S', 'Xbox One S', 40),
    ('MICROSOFT', 'XBOX_ONE', 'Xbox One', 50), ('MICROSOFT', 'XBOX_360', 'Xbox 360', 60),
    ('NINTENDO', 'SWITCH_2', 'Switch 2', 10), ('NINTENDO', 'SWITCH_OLED', 'Switch OLED', 20),
    ('NINTENDO', 'SWITCH', 'Switch', 30), ('NINTENDO', 'SWITCH_LITE', 'Switch Lite', 40),
    ('NINTENDO', 'WII_U', 'Wii U', 50), ('NINTENDO', 'WII', 'Wii', 60),
    ('VALVE', 'STEAM_DECK_OLED', 'Steam Deck OLED', 10), ('VALVE', 'STEAM_DECK', 'Steam Deck', 20)
) AS v(brand, code, label, sort_order)
JOIN attribute_options brand ON brand.value_code = v.brand
                            AND brand.attribute_id = (SELECT id FROM attributes WHERE code = 'electronics_brand')
JOIN attributes a ON a.code = 'console_model';

-- ---------------------------------------------------------------- which subcategory has which

-- Storage only where it means something: no longer for every kind of electronics.
DELETE FROM category_attributes
WHERE category_id = (SELECT id FROM categories WHERE slug = 'elektronike')
  AND attribute_id = (SELECT id FROM attributes WHERE code = 'electronics_storage');

INSERT INTO category_attributes (category_id, attribute_id, required, searchable, filterable, show_in_card, sort_order, label)
SELECT c.id, a.id, false, v.searchable, v.filterable, v.show_in_card, v.sort_order, v.label
FROM (VALUES
    -- Every kind: under warranty, and what comes with it.
    ('elektronike',         'electronics_warranty',   false, true,  false, 80, NULL),
    ('elektronike',         'electronics_included',   false, false, false, 90, NULL),
    -- Phones: storage on the card, as before.
    ('telefona',            'phone_model',            true,  true,  false, 25, NULL),
    ('telefona',            'electronics_storage',    true,  true,  true,  30, NULL),
    ('telefona',            'electronics_ram',        true,  true,  false, 35, NULL),
    ('telefona',            'phone_battery',          false, true,  false, 40, NULL),
    ('telefona',            'electronics_color',      true,  true,  false, 45, NULL),
    ('telefona',            'electronics_features',   true,  true,  false, 50, NULL),
    -- Laptops: processor, RAM and storage on the card ("Intel Core i7 · 16 GB RAM · 512 GB").
    ('laptope',             'computer_cpu',           true,  true,  true,  25, NULL),
    ('laptope',             'electronics_ram',        true,  true,  true,  30, NULL),
    ('laptope',             'electronics_storage',    true,  true,  true,  35, NULL),
    ('laptope',             'computer_disk',          true,  true,  false, 40, NULL),
    ('laptope',             'electronics_screen',     false, true,  false, 45, NULL),
    ('laptope',             'computer_gpu',           true,  true,  false, 50, NULL),
    ('laptope',             'computer_os',            true,  true,  false, 55, NULL),
    ('laptope',             'electronics_features',   true,  false, false, 60, NULL),
    -- Desktops: what it is, its processor and RAM on the card.
    ('kompjutere',          'electronics_device',     true,  true,  true,  15, NULL),
    ('kompjutere',          'computer_cpu',           true,  true,  true,  25, NULL),
    ('kompjutere',          'electronics_ram',        true,  true,  true,  30, NULL),
    ('kompjutere',          'electronics_storage',    true,  true,  false, 35, NULL),
    ('kompjutere',          'computer_disk',          true,  true,  false, 40, NULL),
    ('kompjutere',          'computer_gpu',           true,  true,  false, 50, NULL),
    ('kompjutere',          'computer_os',            true,  true,  false, 55, NULL),
    ('kompjutere',          'computer_monitor',       false, true,  false, 60, NULL),
    -- Tablets: storage and whether it takes a SIM on the card.
    ('tableta',             'electronics_storage',    true,  true,  true,  30, NULL),
    ('tableta',             'electronics_screen',     false, true,  false, 35, NULL),
    ('tableta',             'tablet_connectivity',    true,  true,  true,  40, NULL),
    -- TVs: size and resolution on the card ("55 inç · 4K Ultra HD").
    ('televizore',          'electronics_screen',     false, true,  true,  25, 'Diagonalja'),
    ('televizore',          'tv_resolution',          true,  true,  true,  30, NULL),
    ('televizore',          'tv_panel',               true,  true,  false, 35, NULL),
    ('televizore',          'tv_smart',               true,  true,  false, 40, NULL),
    -- Audio: what it is on the card.
    ('audio-dhe-kufje',     'electronics_device',     true,  true,  true,  15, NULL),
    ('audio-dhe-kufje',     'audio_wireless',         true,  true,  false, 30, NULL),
    ('audio-dhe-kufje',     'audio_noise_cancelling', true,  true,  false, 35, NULL),
    -- Cameras: what it is on the card.
    ('aparate-fotografike', 'electronics_device',     true,  true,  true,  15, NULL),
    ('aparate-fotografike', 'camera_sensor',          true,  true,  false, 30, NULL),
    ('aparate-fotografike', 'camera_mount',           true,  true,  false, 35, NULL),
    ('aparate-fotografike', 'camera_shutter_count',   false, true,  false, 40, NULL),
    -- Consoles: the model on the card.
    ('konsola-lojerash',    'console_model',          true,  true,  true,  25, NULL),
    ('konsola-lojerash',    'console_edition',        true,  true,  false, 30, NULL),
    ('konsola-lojerash',    'console_games',          true,  true,  false, 35, NULL),
    ('konsola-lojerash',    'console_controllers',    false, false, false, 40, NULL)
) AS v(category, attr, searchable, filterable, show_in_card, sort_order, label)
JOIN categories c ON c.slug = v.category
JOIN attributes a ON a.code = v.attr;

-- What each subcategory offers of the shared lists, in its own order.
INSERT INTO category_attribute_options (category_id, option_id, sort_order)
SELECT c.id, o.id, list.n * 10
FROM (VALUES
    ('telefona',            'electronics_brand',    'APPLE SAMSUNG XIAOMI HUAWEI GOOGLE ONEPLUS OPPO REALME HONOR MOTOROLA NOKIA OTHER'),
    ('tableta',             'electronics_brand',    'APPLE SAMSUNG LENOVO XIAOMI HUAWEI MICROSOFT AMAZON OTHER'),
    ('laptope',             'electronics_brand',    'APPLE LENOVO HP DELL ASUS ACER MSI MICROSOFT HUAWEI SAMSUNG OTHER'),
    ('kompjutere',          'electronics_brand',    'BUILT APPLE DELL HP LENOVO ASUS ACER MSI OTHER'),
    ('televizore',          'electronics_brand',    'SAMSUNG LG SONY TCL HISENSE PHILIPS PANASONIC XIAOMI GRUNDIG OTHER'),
    ('audio-dhe-kufje',     'electronics_brand',    'APPLE SONY JBL BOSE SAMSUNG SENNHEISER MARSHALL BANG_OLUFSEN HARMAN_KARDON YAMAHA PIONEER OTHER'),
    ('aparate-fotografike', 'electronics_brand',    'CANON NIKON SONY FUJIFILM PANASONIC OLYMPUS LEICA GOPRO DJI SIGMA TAMRON OTHER'),
    ('konsola-lojerash',    'electronics_brand',    'SONY MICROSOFT NINTENDO VALVE OTHER'),
    ('telefona',            'electronics_storage',  'GB32 GB64 GB128 GB256 GB512 TB1'),
    ('tableta',             'electronics_storage',  'GB32 GB64 GB128 GB256 GB512 TB1'),
    ('laptope',             'electronics_storage',  'GB128 GB256 GB512 TB1 TB2'),
    ('kompjutere',          'electronics_storage',  'GB128 GB256 GB512 TB1 TB2'),
    ('telefona',            'electronics_ram',      'GB2 GB3 GB4 GB6 GB8 GB12 GB16'),
    ('laptope',             'electronics_ram',      'GB4 GB8 GB16 GB24 GB32 GB64'),
    ('kompjutere',          'electronics_ram',      'GB4 GB8 GB16 GB24 GB32 GB64'),
    ('telefona',            'electronics_features', 'FIVE_G DUAL_SIM ESIM'),
    ('laptope',             'electronics_features', 'TOUCHSCREEN BACKLIT_KEYBOARD TWO_IN_ONE'),
    ('kompjutere',          'electronics_device',   'PC_DESKTOP PC_GAMING ALL_IN_ONE MINI_PC'),
    ('audio-dhe-kufje',     'electronics_device',   'HEADPHONES EARBUDS SPEAKER SOUNDBAR HIFI AMPLIFIER TURNTABLE MICROPHONE'),
    ('aparate-fotografike', 'electronics_device',   'DSLR MIRRORLESS COMPACT ACTION CAMCORDER FILM LENS DRONE'),
    ('telefona',            'electronics_included', 'BOX INVOICE CHARGER CASE'),
    ('tableta',             'electronics_included', 'BOX INVOICE CHARGER PEN KEYBOARD CASE'),
    ('laptope',             'electronics_included', 'BOX INVOICE CHARGER'),
    ('kompjutere',          'electronics_included', 'BOX INVOICE'),
    ('televizore',          'electronics_included', 'BOX INVOICE REMOTE'),
    ('audio-dhe-kufje',     'electronics_included', 'BOX INVOICE CHARGER'),
    ('aparate-fotografike', 'electronics_included', 'BOX INVOICE CHARGER'),
    ('konsola-lojerash',    'electronics_included', 'BOX INVOICE')
) AS v(category, attr, codes)
CROSS JOIN LATERAL unnest(string_to_array(v.codes, ' ')) WITH ORDINALITY AS list (code, n)
JOIN categories c ON c.slug = v.category
JOIN attributes a ON a.code = v.attr
JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = list.code;

-- A storage size a TV, a camera, a console or headphones had is no property of theirs any more.
DELETE FROM listing_attribute_values v
USING attributes a, listings l
WHERE a.code = 'electronics_storage' AND v.attribute_id = a.id AND l.id = v.listing_id
  AND NOT EXISTS (SELECT 1 FROM effective_category_attributes e WHERE e.category_id = l.category_id AND e.attribute_id = a.id);
