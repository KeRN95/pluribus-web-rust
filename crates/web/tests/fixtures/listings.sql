-- Sample data: loaded by the integration tests (#[sqlx::test(fixtures("listings"))]) and used to
-- seed the development database (see README). Listing numbers 1001-1012 are fixed so tests can
-- link to them. Phone numbers are fake.

INSERT INTO users (id, auth_subject, email, phone_e164, display_name, account_type) VALUES
('11111111-1111-4111-8111-111111111111', 'dev|ana',    'ana@example.test',    '+355690000001', 'Ana',    'PRIVATE'),
('22222222-2222-4222-8222-222222222222', 'dev|besnik', 'besnik@example.test', '+355690000002', 'Besnik', 'PRIVATE'),
('33333333-3333-4333-8333-333333333333', 'dev|dritan', 'dritan@example.test', '+355690000003', 'Dritan', 'BUSINESS');

INSERT INTO businesses (id, legal_name, display_name, slug, public_phone)
VALUES ('44444444-4444-4444-8444-444444444444', 'Auto Durrësi shpk', 'Auto Durrësi', 'auto-durresi', '+355690000004');

INSERT INTO business_members (business_id, user_id, role)
VALUES ('44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333', 'OWNER');

-- Free listings count as 0 € in price filters and sorting; "price on request" has no price.
INSERT INTO listings (id, public_no, seller_user_id, seller_business_id, created_by_user_id,
                      category_id, location_id, slug, title, description, listing_type, condition,
                      price_amount, currency, price_type, price_normalized, status, published_at)
OVERRIDING SYSTEM VALUE
SELECT v.id::uuid, v.public_no, v.seller_user::uuid, v.seller_business::uuid, v.created_by::uuid,
       c.id, loc.id, v.slug, v.title, v.description, v.listing_type, v.condition,
       v.price, v.currency, v.price_type,
       CASE v.price_type WHEN 'FREE' THEN 0 ELSE price_in_eur(v.price, v.currency) END,
       'ACTIVE', now() - v.age
FROM (VALUES
    ('a0000000-0000-4000-8000-000000001001', 1001, '11111111-1111-4111-8111-111111111111', NULL, '11111111-1111-4111-8111-111111111111',
     'makina', 'tirane', 'volkswagen-golf-7-2016-nafte', 'Volkswagen Golf 7, 2016, naftë',
     'Makina është në gjendje shumë të mirë, e mirëmbajtur rregullisht. Kambio manuale, 148 000 km.',
     'SELL', 'USED', 9500.00, 'EUR', 'FIXED', interval '2 hours'),
    ('a0000000-0000-4000-8000-000000001002', 1002, NULL, '44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333',
     'makina', 'durres', 'mercedes-benz-c-220-2018-automatike', 'Mercedes-Benz C 220, 2018, automatike',
     'Importuar nga Gjermania, me servis të plotë. Garanci 6 muaj.',
     'SELL', 'USED', 21500.00, 'EUR', 'NEGOTIABLE', interval '5 hours'),
    ('a0000000-0000-4000-8000-000000001003', 1003, '22222222-2222-4222-8222-222222222222', NULL, '22222222-2222-4222-8222-222222222222',
     'prona', 'tirane', 'apartament-2-1-me-qira-ne-tirane', 'Apartament 2+1 me qira në Tiranë',
     'Apartament i mobiluar, pranë qendrës. Qiraja përfshin mirëmbajtjen e pallatit.',
     'RENT', NULL, 450.00, 'EUR', 'FIXED', interval '1 hour'),
    ('a0000000-0000-4000-8000-000000001004', 1004, '22222222-2222-4222-8222-222222222222', NULL, '22222222-2222-4222-8222-222222222222',
     'prona', 'vlore', 'shtepi-private-ne-shitje-ne-vlore', 'Shtëpi private në shitje në Vlorë',
     'Shtëpi dykatëshe me oborr, 200 m², 5 minuta nga deti.',
     'SELL', NULL, 185000.00, 'EUR', 'FIXED', interval '1 day'),
    ('a0000000-0000-4000-8000-000000001005', 1005, '11111111-1111-4111-8111-111111111111', NULL, '11111111-1111-4111-8111-111111111111',
     'elektronike', 'tirane', 'iphone-13-128-gb-si-i-ri', 'iPhone 13, 128 GB, si i ri',
     'Bateria 92%, pa asnjë gërvishtje. Me kuti dhe karikues.',
     'SELL', 'LIKE_NEW', 55000, 'ALL', 'FIXED', interval '30 minutes'),
    ('a0000000-0000-4000-8000-000000001006', 1006, '22222222-2222-4222-8222-222222222222', NULL, '22222222-2222-4222-8222-222222222222',
     'elektronike', 'shkoder', 'laptop-lenovo-thinkpad-t14', 'Laptop Lenovo ThinkPad T14',
     'Intel Core i5, 16 GB RAM, 512 GB SSD. Funksionon në mënyrë perfekte.',
     'SELL', 'USED', 62000, 'ALL', 'FIXED', interval '3 days'),
    ('a0000000-0000-4000-8000-000000001007', 1007, '11111111-1111-4111-8111-111111111111', NULL, '11111111-1111-4111-8111-111111111111',
     'shtepi-dhe-kopsht', 'korce', 'divan-qoshe-ngjyre-gri', 'Divan qoshe ngjyrë gri',
     'Divan qoshe pothuajse i ri, hapet edhe si shtrat.',
     'SELL', 'LIKE_NEW', 30000, 'ALL', 'NEGOTIABLE', interval '2 days'),
    ('a0000000-0000-4000-8000-000000001008', 1008, '33333333-3333-4333-8333-333333333333', NULL, '33333333-3333-4333-8333-333333333333',
     'pune', 'durres', 'kerkohet-kamarier-e-me-eksperience', 'Kërkohet kamarier/e me eksperiencë',
     'Restorant në Durrës kërkon kamarier/e për sezonin veror. Pagë e mirë.',
     'JOB', NULL, NULL, 'ALL', 'ON_REQUEST', interval '6 hours'),
    ('a0000000-0000-4000-8000-000000001009', 1009, '22222222-2222-4222-8222-222222222222', NULL, '22222222-2222-4222-8222-222222222222',
     'kafshe-shtepiake', 'elbasan', 'kelysh-labrador-3-muajsh', 'Këlysh Labrador, 3 muajsh',
     'Këlysh Labrador i vaksinuar, me pasaportë.',
     'SELL', NULL, 40000, 'ALL', 'FIXED', interval '4 hours'),
    ('a0000000-0000-4000-8000-000000001010', 1010, '11111111-1111-4111-8111-111111111111', NULL, '11111111-1111-4111-8111-111111111111',
     'sport-dhe-hobi', 'fier', 'bicikleta-malore-29-inc', 'Biçikletë malore 29 inç',
     'Biçikletë malore me 21 marsha, e përdorur pak.',
     'SELL', 'USED', 25000, 'ALL', 'FIXED', interval '8 hours'),
    ('a0000000-0000-4000-8000-000000001011', 1011, '22222222-2222-4222-8222-222222222222', NULL, '22222222-2222-4222-8222-222222222222',
     'te-tjera', 'berat', 'dhurohen-libra-shkollore', 'Dhurohen libra shkollorë',
     'Libra për klasën e 5-të. Falas për këdo që ka nevojë.',
     'SELL', 'USED', NULL, 'ALL', 'FREE', interval '12 hours'),
    ('a0000000-0000-4000-8000-000000001012', 1012, '11111111-1111-4111-8111-111111111111', NULL, '11111111-1111-4111-8111-111111111111',
     'veshje-dhe-aksesore', 'tirane', 'xhakete-dimri-per-meshkuj-masa-m', 'Xhaketë dimri për meshkuj, masa M',
     'Xhaketë dimri e re, me etiketë.',
     'SELL', 'NEW', 4500, 'ALL', 'FIXED', interval '20 hours')
) AS v(id, public_no, seller_user, seller_business, created_by, category_slug, location_slug, slug,
       title, description, listing_type, condition, price, currency, price_type, age)
JOIN categories c ON c.slug = v.category_slug
JOIN locations loc ON loc.slug = v.location_slug AND loc.location_type = 'MUNICIPALITY';

-- The house in Vlorë was bumped ten minutes ago, so it ranks first.
UPDATE listings SET bumped_at = now() - interval '10 minutes' WHERE public_no = 1004;

-- Points on the map: each listing at its town's centre (migrations/0005_map_search.sql).
UPDATE listings l
SET latitude = loc.latitude, longitude = loc.longitude
FROM locations loc
WHERE loc.id = l.location_id AND l.public_no BETWEEN 1001 AND 1012;

-- Properties (see migrations/0003_category_properties.sql).
INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
SELECT l.id, a.id, a.data_type, o.id
FROM (VALUES
    (1001, 'vehicle_make', 'VOLKSWAGEN'),    (1001, 'vehicle_model', 'VOLKSWAGEN_GOLF'),
    (1001, 'vehicle_fuel', 'DIESEL'),        (1001, 'vehicle_transmission', 'MANUAL'),
    (1001, 'vehicle_body_type', 'HATCHBACK'),
    (1002, 'vehicle_make', 'MERCEDES_BENZ'), (1002, 'vehicle_model', 'MERCEDES_BENZ_C_CLASS'),
    (1002, 'vehicle_fuel', 'DIESEL'),        (1002, 'vehicle_transmission', 'AUTOMATIC'),
    (1002, 'vehicle_body_type', 'SEDAN'),
    (1003, 'property_type', 'APARTMENT'),    (1003, 'property_rooms', 'R21'),
    (1003, 'property_furnished', 'FURNISHED'),
    (1004, 'property_type', 'HOUSE'),        (1004, 'property_rooms', 'R41'),
    (1005, 'electronics_type', 'PHONE'),     (1005, 'electronics_brand', 'APPLE'),
    (1005, 'electronics_storage', 'GB128'),
    (1006, 'electronics_type', 'LAPTOP'),    (1006, 'electronics_brand', 'LENOVO'),
    (1006, 'electronics_storage', 'GB512'),
    (1007, 'home_type', 'FURNITURE'),
    (1008, 'job_field', 'HOSPITALITY'),      (1008, 'job_schedule', 'SEASONAL'),
    (1009, 'pet_type', 'DOG'),
    (1010, 'sport_type', 'BICYCLES'),
    (1012, 'fashion_for', 'MEN'),            (1012, 'fashion_type', 'OUTERWEAR'),
    (1012, 'fashion_size', 'M')
) AS v(public_no, code, value_code)
JOIN listings l ON l.public_no = v.public_no
JOIN attributes a ON a.code = v.code
JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = v.value_code;

INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
SELECT l.id, a.id, a.data_type, v.value
FROM (VALUES
    (1001, 'vehicle_year', 2016), (1001, 'vehicle_mileage', 148000),
    (1002, 'vehicle_year', 2018), (1002, 'vehicle_mileage', 98000),
    (1003, 'property_area', 85),  (1003, 'property_floor', 4),
    (1004, 'property_area', 200)
) AS v(public_no, code, value)
JOIN listings l ON l.public_no = v.public_no
JOIN attributes a ON a.code = v.code;

INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
SELECT l.id, o.attribute_id, o.id
FROM (VALUES
    (1001, 'vehicle_features', 'ABS'), (1001, 'vehicle_features', 'AIR_CONDITIONING'),
    (1002, 'vehicle_features', 'ABS'), (1002, 'vehicle_features', 'NAVIGATION'),
    (1002, 'vehicle_features', 'PARKING_SENSORS'),
    (1003, 'property_features', 'ELEVATOR'), (1003, 'property_features', 'BALCONY'),
    (1004, 'property_features', 'PARKING'),  (1004, 'property_features', 'SEA_VIEW')
) AS v(public_no, code, value_code)
JOIN listings l ON l.public_no = v.public_no
JOIN attributes a ON a.code = v.code
JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = v.value_code;

-- Each listing goes into the subcategory for its kind (migration 0008).
UPDATE listings l
SET category_id = sub.id
FROM listing_attribute_values v
JOIN categories sub ON sub.replaces_option_id = v.value_option_id
WHERE v.listing_id = l.id AND sub.parent_id = l.category_id AND l.public_no BETWEEN 1001 AND 1012;

-- Searchable property labels go into full-text search ("naftë" finds the diesel cars).
UPDATE listings SET search_extra = listing_search_extra(id) WHERE public_no BETWEEN 1001 AND 1012;

-- Each listing shows its seller's phone: the business's number, or the private seller's own.
INSERT INTO listing_contacts (listing_id, contact_name, phone_e164, show_phone)
SELECT l.id, coalesce(b.display_name, u.display_name), coalesce(b.public_phone, u.phone_e164), true
FROM listings l
LEFT JOIN businesses b ON b.id = l.seller_business_id
LEFT JOIN users u ON u.id = l.seller_user_id
WHERE l.public_no BETWEEN 1001 AND 1012;

-- Cover photos: free under the Unsplash License (https://unsplash.com/license), served by Unsplash's
-- image CDN at the sizes the pages use. Each comment links the photo's page, which credits the
-- photographer. Real listings get uploaded photos from object storage instead.
INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url,
                           width_px, height_px, is_cover)
SELECT l.id, 'IMAGE', 'READY', 'unsplash/' || v.photo,
       'https://images.unsplash.com/' || v.photo || '?auto=format&fit=crop&w=1200&h=900&q=75',
       'https://images.unsplash.com/' || v.photo || '?auto=format&fit=crop&w=480&h=360&q=70',
       1200, 900, true
FROM (VALUES
    (1001, 'photo-1591230740238-e9a71182b67c'),  -- unsplash.com/photos/ruSOVQkBeT4
    (1002, 'photo-1624085568108-36410cfe4d24'),  -- unsplash.com/photos/GfA_3_hLUNA
    (1003, 'photo-1654506012740-09321c969dc2'),  -- unsplash.com/photos/OhezdWyzXTI
    (1004, 'photo-1787304680952-d53c92b40df6'),  -- unsplash.com/photos/0WsxUKS5T1c
    (1005, 'photo-1632518192421-56aba73eae55'),  -- unsplash.com/photos/YT-2-VL2QuI
    (1006, 'photo-1743456056112-0739a6742135'),  -- unsplash.com/photos/Tiy7aJwWPvc
    (1007, 'photo-1759722665935-0967b4e0da93'),  -- unsplash.com/photos/MRx-rSdOKtk
    (1008, 'photo-1512485800893-b08ec1ea59b1'),  -- unsplash.com/photos/e5Q5vWO55uU
    (1009, 'photo-1586671267731-da2cf3ceeb80'),  -- unsplash.com/photos/WX4i1Jq_o0Y
    (1010, 'photo-1606087492572-424ebe0f2f61'),  -- unsplash.com/photos/o3_3a_EyNnY
    (1011, 'photo-1497633762265-9d179a990aa6'),  -- unsplash.com/photos/lUaaKCUANVI
    (1012, 'photo-1624548140129-74786c5f1279')   -- unsplash.com/photos/TcRaVI3LXds
) AS v(public_no, photo)
JOIN listings l ON l.public_no = v.public_no;

-- Continue numbering new listings after the fixed ones.
SELECT setval(pg_get_serial_sequence('listings', 'public_no'), (SELECT max(public_no) FROM listings));

-- Search fixes typos from the words listings use (migration 0016), a list the app rebuilds every
-- quarter of an hour; rebuilt here, it has these listings' words at once.
REFRESH MATERIALIZED VIEW search_words;
