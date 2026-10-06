-- Constraint and behaviour checks for the marketplace schema.
--
--   psql -v ON_ERROR_STOP=1 -f tests/schema_checks.sql
--
-- Run against a database with the migrations applied. Everything happens in one transaction that is
-- rolled back at the end, so the database is left unchanged. Each check prints "ok"; the first
-- failure stops the script with "FAIL".

\set QUIET on
\pset tuples_only on
\pset format unaligned

BEGIN;

-- Requires stmt to fail with the given SQLSTATE (23502 not null, 23503 foreign key,
-- 23505 unique, 23514 check) and prints the constraint that fired.
CREATE FUNCTION pg_temp.expect_error(label text, stmt text, expected_sqlstate text)
RETURNS text
LANGUAGE plpgsql
AS $$
DECLARE
    v_constraint text;
    v_column     text;
BEGIN
    BEGIN
        EXECUTE stmt;
    EXCEPTION WHEN OTHERS THEN
        GET STACKED DIAGNOSTICS v_constraint = CONSTRAINT_NAME, v_column = COLUMN_NAME;
        IF SQLSTATE = expected_sqlstate THEN
            RETURN format('ok    rejects %s  [%s]', label, coalesce(nullif(v_constraint, ''), v_column));
        END IF;
        RAISE EXCEPTION 'FAIL  %: expected SQLSTATE %, got %: %', label, expected_sqlstate, SQLSTATE, SQLERRM;
    END;
    RAISE EXCEPTION 'FAIL  %: statement was accepted', label;
END;
$$;

CREATE FUNCTION pg_temp.expect_ok(label text, stmt text)
RETURNS text
LANGUAGE plpgsql
AS $$
BEGIN
    BEGIN
        EXECUTE stmt;
    EXCEPTION WHEN OTHERS THEN
        RAISE EXCEPTION 'FAIL  %: % (%)', label, SQLERRM, SQLSTATE;
    END;
    RETURN 'ok    accepts ' || label;
END;
$$;

CREATE FUNCTION pg_temp.check(label text, passed boolean)
RETURNS text
LANGUAGE plpgsql
AS $$
BEGIN
    IF passed IS NOT TRUE THEN
        RAISE EXCEPTION 'FAIL  %', label;
    END IF;
    RETURN 'ok    ' || label;
END;
$$;

-- ------------------------------------------------------------
-- Fixtures
--   alice 11111111-...  private seller    golf  55555555-...  Alice's car (ACTIVE)
--   bob   22222222-...  buyer             flat  66666666-...  the shop's rental (ACTIVE)
--   carol 33333333-...  owns shop         audi  77777777-...  Alice's second car (ACTIVE)
--   shop  44444444-...  business          draft 99999999-...  Alice's draft, no attributes yet
--   chat  88888888-...  Bob <-> Alice about the golf
-- ------------------------------------------------------------

INSERT INTO users (id, auth_subject, email, phone_e164, display_name) VALUES
('11111111-1111-4111-8111-111111111111', 'kc|alice', 'alice@example.test', '+355691111111', 'Alice'),
('22222222-2222-4222-8222-222222222222', 'kc|bob',   'bob@example.test',   '+355692222222', 'Bob'),
('33333333-3333-4333-8333-333333333333', 'kc|carol', 'carol@example.test', NULL,            'Carol');

INSERT INTO businesses (id, legal_name, display_name, slug)
VALUES ('44444444-4444-4444-8444-444444444444', 'Auto Tirana shpk', 'Auto Tirana', 'auto-tirana');

INSERT INTO business_members (business_id, user_id, role)
VALUES ('44444444-4444-4444-8444-444444444444', '33333333-3333-4333-8333-333333333333', 'OWNER');

INSERT INTO listings (id, seller_user_id, seller_business_id, created_by_user_id, category_id, location_id,
                      slug, title, description, listing_type, price_amount, currency, price_normalized,
                      status, published_at)
VALUES
('55555555-5555-4555-8555-555555555555', '11111111-1111-4111-8111-111111111111', NULL,
 '11111111-1111-4111-8111-111111111111', '10000000-0000-0000-0000-000000000101',
 (SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'),
 'vw-golf-7', 'VW Golf 7 në gjendje shumë të mirë', 'Makinë e mirëmbajtur, kambio manuale.',
 'SELL', 9500, 'EUR', 9500, 'ACTIVE', now() - interval '10 days'),
('66666666-6666-4666-8666-666666666666', NULL, '44444444-4444-4444-8444-444444444444',
 '33333333-3333-4333-8333-333333333333', '10000000-0000-0000-0000-000000000002',
 (SELECT id FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'),
 'apartament-2-1', 'Shtëpi me qira në Tiranë', 'Apartament 2+1, i mobiluar, pranë qendrës.',
 'RENT', 60000, 'ALL', 600, 'ACTIVE', now() - interval '3 days'),
('77777777-7777-4777-8777-777777777777', '11111111-1111-4111-8111-111111111111', NULL,
 '11111111-1111-4111-8111-111111111111', '10000000-0000-0000-0000-000000000101',
 (SELECT id FROM locations WHERE slug = 'durres' AND location_type = 'MUNICIPALITY'),
 'audi-a4-2018', 'Audi A4 2018', 'Krahasuar me një Golf, ka më shumë vend.',
 'SELL', 18500, 'EUR', 18500, 'ACTIVE', now() - interval '1 day'),
('99999999-9999-4999-8999-999999999999', '11111111-1111-4111-8111-111111111111', NULL,
 '11111111-1111-4111-8111-111111111111', '10000000-0000-0000-0000-000000000101',
 NULL, 'draft', 'Draft', '', 'SELL', NULL, 'EUR', NULL, 'DRAFT', NULL);

-- Golf: make, model, transmission (SELECT) plus year and mileage (NUMBER).
INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
SELECT '55555555-5555-4555-8555-555555555555', a.id, a.data_type, o.id
FROM attributes a
JOIN attribute_options o ON o.attribute_id = a.id
WHERE (a.code, o.value_code) IN (('vehicle_make', 'VOLKSWAGEN'),
                                 ('vehicle_model', 'VOLKSWAGEN_GOLF'),
                                 ('vehicle_transmission', 'MANUAL'));

INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
SELECT '55555555-5555-4555-8555-555555555555', a.id, a.data_type, v.value
FROM attributes a
JOIN (VALUES ('vehicle_year', 2016), ('vehicle_mileage', 148000)) AS v(code, value) USING (code);

INSERT INTO conversations (id, listing_id, buyer_user_id, seller_user_id)
VALUES ('88888888-8888-4888-8888-888888888888', '55555555-5555-4555-8555-555555555555',
        '22222222-2222-4222-8222-222222222222', '11111111-1111-4111-8111-111111111111');

INSERT INTO conversation_participants (conversation_id, user_id) VALUES
('88888888-8888-4888-8888-888888888888', '22222222-2222-4222-8222-222222222222'),
('88888888-8888-4888-8888-888888888888', '11111111-1111-4111-8111-111111111111');

INSERT INTO messages (id, conversation_id, sender_user_id, body)
VALUES ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', '88888888-8888-4888-8888-888888888888',
        '22222222-2222-4222-8222-222222222222', 'A është ende në shitje?');

INSERT INTO subscription_plans (code, name, price_amount, billing_period)
VALUES ('PRO_MONTHLY', 'Pro', 29.00, 'MONTHLY');

\echo '--- reference data'

SELECT pg_temp.check('Albania has 12 counties and 61 municipalities, each under a county',
    (SELECT count(*) FILTER (WHERE location_type = 'REGION') = 12
        AND count(*) FILTER (WHERE location_type = 'MUNICIPALITY') = 61
     FROM locations)
    AND NOT EXISTS (SELECT 1 FROM locations m
                    JOIN locations p ON p.id = m.parent_id
                    WHERE m.location_type = 'MUNICIPALITY' AND p.location_type <> 'REGION'));

SELECT pg_temp.check('labels are Albanian with English translations',
    (SELECT name = 'Makina' AND name_i18n ->> 'en' = 'Cars' AND default_currency = 'EUR'
     FROM categories WHERE slug = 'makina')
    AND (SELECT coalesce(label_i18n ->> 'sq', label) = 'Naftë' AND coalesce(label_i18n ->> 'en', label) = 'Diesel'
         FROM attribute_options WHERE value_code = 'DIESEL'));

\echo '--- dynamic attributes'

SELECT pg_temp.expect_error('an option that belongs to a different attribute (fuel = AUTOMATIC)',
$$INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
  SELECT '99999999-9999-4999-8999-999999999999', a.id, 'SELECT', o.id
  FROM attributes a, attribute_options o
  WHERE a.code = 'vehicle_fuel' AND o.value_code = 'AUTOMATIC'$$, '23503');

SELECT pg_temp.expect_error('a value typed differently from its attribute (mileage as TEXT)',
$$INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text)
  SELECT '99999999-9999-4999-8999-999999999999', id, 'TEXT', 'about 150000'
  FROM attributes WHERE code = 'vehicle_mileage'$$, '23503');

SELECT pg_temp.expect_error('a value in the wrong column for its type (NUMBER in value_text)',
$$INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text)
  SELECT '99999999-9999-4999-8999-999999999999', id, 'NUMBER', '150000'
  FROM attributes WHERE code = 'vehicle_mileage'$$, '23514');

SELECT pg_temp.expect_error('a single-select value stored as multi-select',
$$INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
  SELECT '99999999-9999-4999-8999-999999999999', a.id, o.id
  FROM attributes a JOIN attribute_options o ON o.attribute_id = a.id
  WHERE a.code = 'vehicle_fuel' AND o.value_code = 'DIESEL'$$, '23503');

SELECT pg_temp.expect_ok('fuel = DIESEL on the draft',
$$INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
  SELECT '99999999-9999-4999-8999-999999999999', a.id, a.data_type, o.id
  FROM attributes a JOIN attribute_options o ON o.attribute_id = a.id
  WHERE a.code = 'vehicle_fuel' AND o.value_code = 'DIESEL'$$);

SELECT pg_temp.expect_error('a second option for a single-select attribute',
$$INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
  SELECT '99999999-9999-4999-8999-999999999999', a.id, a.data_type, o.id
  FROM attributes a JOIN attribute_options o ON o.attribute_id = a.id
  WHERE a.code = 'vehicle_fuel' AND o.value_code = 'PETROL'$$, '23505');

SELECT pg_temp.expect_ok('two options for a multi-select attribute (features)',
$$INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
  SELECT '99999999-9999-4999-8999-999999999999', a.id, o.id
  FROM attributes a JOIN attribute_options o ON o.attribute_id = a.id
  WHERE a.code = 'vehicle_features' AND o.value_code IN ('ABS', 'NAVIGATION')$$);

SELECT pg_temp.expect_error('changing an attribute''s data_type while values exist',
$$UPDATE attributes SET data_type = 'TEXT' WHERE code = 'vehicle_fuel'$$, '23503');

\echo '--- listings'

SELECT pg_temp.expect_error('a FREE listing with a price',
$$UPDATE listings SET price_type = 'FREE', price_amount = 10
  WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_error('an ACTIVE fixed-price listing without a price',
$$UPDATE listings SET price_amount = NULL WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_error('an ACTIVE listing without published_at',
$$UPDATE listings SET status = 'ACTIVE', price_amount = 100
  WHERE id = '99999999-9999-4999-8999-999999999999'$$, '23514');

SELECT pg_temp.expect_error('a SOLD listing without sold_at',
$$UPDATE listings SET status = 'SOLD' WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_error('a listing with both a private and a business seller',
$$UPDATE listings SET seller_business_id = '44444444-4444-4444-8444-444444444444'
  WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_error('a listing without created_by_user_id',
$$INSERT INTO listings (seller_user_id, category_id, slug, title, description)
  VALUES ('11111111-1111-4111-8111-111111111111', '10000000-0000-0000-0000-000000000101', 'x', 'x', '')$$, '23502');

SELECT pg_temp.expect_error('latitude without longitude',
$$UPDATE listings SET latitude = 41.33 WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_error('a lowercase currency code',
$$UPDATE listings SET currency = 'eur' WHERE id = '55555555-5555-4555-8555-555555555555'$$, '23514');

SELECT pg_temp.expect_ok('two listings with the same slug',
$$UPDATE listings SET slug = 'vw-golf-7' WHERE id = '77777777-7777-4777-8777-777777777777'$$);

\echo '--- users, businesses, locations'

SELECT pg_temp.expect_error('a phone number that is not E.164',
$$UPDATE users SET phone_e164 = '0691234567' WHERE id = '22222222-2222-4222-8222-222222222222'$$, '23514');

SELECT pg_temp.expect_error('a DELETED user who still has an e-mail address',
$$UPDATE users SET status = 'DELETED', deleted_at = now() WHERE id = '22222222-2222-4222-8222-222222222222'$$, '23514');

SELECT pg_temp.expect_error('hard-deleting a user who has listings',
$$DELETE FROM users WHERE id = '11111111-1111-4111-8111-111111111111'$$, '23503');

SELECT pg_temp.expect_error('a second OWNER for a business',
$$INSERT INTO business_members (business_id, user_id, role)
  VALUES ('44444444-4444-4444-8444-444444444444', '11111111-1111-4111-8111-111111111111', 'OWNER')$$, '23505');

SELECT pg_temp.expect_ok('a VERIFIED phone verification',
$$INSERT INTO user_verifications (user_id, verification_type, verified_value, status, verified_at)
  VALUES ('11111111-1111-4111-8111-111111111111', 'PHONE', '+355691111111', 'VERIFIED', now())$$);

SELECT pg_temp.expect_error('a second VERIFIED phone verification',
$$INSERT INTO user_verifications (user_id, verification_type, verified_value, status, verified_at)
  VALUES ('11111111-1111-4111-8111-111111111111', 'PHONE', '+355699999999', 'VERIFIED', now())$$, '23505');

SELECT pg_temp.expect_error('two top-level locations with the same slug',
$$INSERT INTO locations (parent_id, location_type, name, slug)
  VALUES (NULL, 'COUNTRY', 'Shqipëri (duplicate)', 'shqiperi')$$, '23505');

\echo '--- chat'

SELECT pg_temp.expect_error('a second conversation for the same buyer and listing',
$$INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id)
  VALUES ('55555555-5555-4555-8555-555555555555', '22222222-2222-4222-8222-222222222222',
          '11111111-1111-4111-8111-111111111111')$$, '23505');

SELECT pg_temp.expect_error('a conversation with yourself',
$$INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id)
  VALUES ('77777777-7777-4777-8777-777777777777', '11111111-1111-4111-8111-111111111111',
          '11111111-1111-4111-8111-111111111111')$$, '23514');

SELECT pg_temp.expect_error('a message from someone outside the conversation',
$$INSERT INTO messages (conversation_id, sender_user_id, body)
  VALUES ('88888888-8888-4888-8888-888888888888', '33333333-3333-4333-8333-333333333333', 'spam')$$, '23503');

SELECT pg_temp.expect_error('a TEXT message without a sender',
$$INSERT INTO messages (conversation_id, body)
  VALUES ('88888888-8888-4888-8888-888888888888', 'who sent this?')$$, '23514');

SELECT pg_temp.expect_ok('a SYSTEM message without a sender',
$$INSERT INTO messages (conversation_id, message_type, body)
  VALUES ('88888888-8888-4888-8888-888888888888', 'SYSTEM', 'The listing was reserved.')$$);

\echo '--- reviews and reports'

SELECT pg_temp.expect_error('reviewing yourself',
$$INSERT INTO reviews (listing_id, reviewer_user_id, reviewed_user_id, rating)
  VALUES ('55555555-5555-4555-8555-555555555555', '11111111-1111-4111-8111-111111111111',
          '11111111-1111-4111-8111-111111111111', 5)$$, '23514');

SELECT pg_temp.expect_ok('Bob reviewing Alice for the golf',
$$INSERT INTO reviews (listing_id, conversation_id, reviewer_user_id, reviewed_user_id, rating)
  VALUES ('55555555-5555-4555-8555-555555555555', '88888888-8888-4888-8888-888888888888',
          '22222222-2222-4222-8222-222222222222', '11111111-1111-4111-8111-111111111111', 5)$$);

SELECT pg_temp.expect_error('a second review by Bob for the same listing',
$$INSERT INTO reviews (listing_id, reviewer_user_id, reviewed_user_id, rating)
  VALUES ('55555555-5555-4555-8555-555555555555', '22222222-2222-4222-8222-222222222222',
          '11111111-1111-4111-8111-111111111111', 1)$$, '23505');

SELECT pg_temp.expect_ok('Bob reporting the golf',
$$INSERT INTO reports (reporter_user_id, listing_id, reason_code)
  VALUES ('22222222-2222-4222-8222-222222222222', '55555555-5555-4555-8555-555555555555', 'SCAM')$$);

SELECT pg_temp.expect_error('a second open report by Bob on the same listing',
$$INSERT INTO reports (reporter_user_id, listing_id, reason_code)
  VALUES ('22222222-2222-4222-8222-222222222222', '55555555-5555-4555-8555-555555555555', 'SCAM')$$, '23505');

SELECT pg_temp.expect_error('a message report that does not name the sender',
$$INSERT INTO reports (reporter_user_id, listing_id, message_id, reason_code)
  VALUES ('11111111-1111-4111-8111-111111111111', '55555555-5555-4555-8555-555555555555',
          'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', 'HARASSMENT')$$, '23514');

SELECT pg_temp.expect_ok('an automated moderation action (no moderator)',
$$INSERT INTO moderation_actions (listing_id, action_type, reason)
  VALUES ('55555555-5555-4555-8555-555555555555', 'APPROVE_LISTING', 'auto-approved: trusted seller')$$);

\echo '--- billing'

SELECT pg_temp.expect_ok('an ACTIVE subscription',
$$INSERT INTO business_subscriptions (business_id, plan_id, status, starts_at)
  SELECT '44444444-4444-4444-8444-444444444444', id, 'ACTIVE', now() FROM subscription_plans WHERE code = 'PRO_MONTHLY'$$);

SELECT pg_temp.expect_error('a second live subscription for the same business',
$$INSERT INTO business_subscriptions (business_id, plan_id, status, starts_at)
  SELECT '44444444-4444-4444-8444-444444444444', id, 'TRIAL', now() FROM subscription_plans WHERE code = 'PRO_MONTHLY'$$, '23505');

SELECT pg_temp.expect_ok('a PENDING renewal next to the live subscription',
$$INSERT INTO business_subscriptions (business_id, plan_id, status, starts_at)
  SELECT '44444444-4444-4444-8444-444444444444', id, 'PENDING', now() + interval '1 month'
  FROM subscription_plans WHERE code = 'PRO_MONTHLY'$$);

INSERT INTO payment_orders (id, user_id, amount, provider, provider_order_id, status)
VALUES ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', '11111111-1111-4111-8111-111111111111', 4.99, 'paypal', 'PO-1', 'PAID');
INSERT INTO payment_transactions (payment_order_id, transaction_type, amount, provider_reference)
VALUES ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', 'CAPTURE', 4.99, 'TX-1');

SELECT pg_temp.expect_error('a second order with the same provider order id',
$$INSERT INTO payment_orders (user_id, amount, provider, provider_order_id)
  VALUES ('11111111-1111-4111-8111-111111111111', 4.99, 'paypal', 'PO-1')$$, '23505');

SELECT pg_temp.expect_error('recording the same provider transaction twice',
$$INSERT INTO payment_transactions (payment_order_id, transaction_type, amount, provider_reference)
  VALUES ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', 'CAPTURE', 4.99, 'TX-1')$$, '23505');

SELECT pg_temp.expect_error('deleting a payment order that has transactions',
$$DELETE FROM payment_orders WHERE id = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'$$, '23503');

SELECT pg_temp.expect_error('a PROMOTION order item without a product',
$$INSERT INTO payment_order_items (payment_order_id, item_type, description, unit_amount, amount)
  VALUES ('bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb', 'PROMOTION', 'Featured', 4.99, 4.99)$$, '23514');

\echo '--- behaviour'

SELECT pg_temp.check('uuid_v7() sets version 7 and the RFC variant',
    (SELECT bool_and(substr(u::text, 15, 1) = '7' AND substr(u::text, 20, 1) IN ('8', '9', 'a', 'b'))
     FROM (SELECT uuid_v7() AS u FROM generate_series(1, 1000)) s));

CREATE FUNCTION pg_temp.v7_generated_later_sorts_later()
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    first_id uuid := uuid_v7();
BEGIN
    PERFORM pg_sleep(0.002);
    RETURN first_id < uuid_v7();
END;
$$;

SELECT pg_temp.check('uuid_v7() values from a later millisecond sort later',
    pg_temp.v7_generated_later_sorts_later());

-- Golf: published 10 days ago, bumped 2 days ago. Audi: published 1 day ago. Flat: 3 days ago.
UPDATE listings SET bumped_at = now() - interval '2 days' WHERE id = '55555555-5555-4555-8555-555555555555';

SELECT pg_temp.check('a bump lifts a listing only until newer listings arrive (audi, golf, flat)',
    (SELECT array_agg(id ORDER BY sort_at DESC, id DESC)
     FROM listings
     WHERE status = 'ACTIVE'
       AND id IN ('55555555-5555-4555-8555-555555555555', '66666666-6666-4666-8666-666666666666',
                  '77777777-7777-4777-8777-777777777777'))
    = ARRAY['77777777-7777-4777-8777-777777777777', '55555555-5555-4555-8555-555555555555',
            '66666666-6666-4666-8666-666666666666']::uuid[]);

SELECT pg_temp.check('"shtepi tirane" finds "Shtëpi me qira në Tiranë"',
    EXISTS (SELECT 1 FROM listings
            WHERE status = 'ACTIVE'
              AND search_vector @@ websearch_to_tsquery('sq_unaccent', 'shtepi tirane')
              AND id = '66666666-6666-4666-8666-666666666666'));

UPDATE listings SET search_extra = 'Volkswagen Golf 2016 Naftë Manuale'
WHERE id = '55555555-5555-4555-8555-555555555555';

SELECT pg_temp.check('searchable attribute labels are searchable ("nafte" finds the golf)',
    (SELECT array_agg(id) FROM listings
     WHERE status = 'ACTIVE'
       AND search_vector @@ websearch_to_tsquery('sq_unaccent', 'nafte'))
    = ARRAY['55555555-5555-4555-8555-555555555555']::uuid[]);

SELECT pg_temp.check('a title match outranks a description-only match for "golf"',
    (SELECT array_agg(id ORDER BY ts_rank(search_vector, q) DESC)
     FROM listings, websearch_to_tsquery('sq_unaccent', 'golf') AS q
     WHERE status = 'ACTIVE' AND search_vector @@ q)
    = ARRAY['55555555-5555-4555-8555-555555555555', '77777777-7777-4777-8777-777777777777']::uuid[]);

SELECT pg_temp.check('location autocomplete ignores accents ("kucov" finds Kuçovë)',
    (SELECT array_agg(name::text) FROM locations WHERE immutable_unaccent(name) ILIKE 'kucov%') = ARRAY['Kuçovë']);

DO $$ BEGIN
    PERFORM set_config('app.user_id', '11111111-1111-4111-8111-111111111111', true),
            set_config('app.status_reason', 'Sold to Bob', true);
END $$;
UPDATE listings SET status = 'SOLD', sold_at = now() WHERE id = '55555555-5555-4555-8555-555555555555';
DO $$ BEGIN
    PERFORM set_config('app.user_id', '', true), set_config('app.status_reason', '', true);
END $$;

SELECT pg_temp.check('status history records creation and the sale, with actor and reason',
    (SELECT array_agg(concat_ws(' | ', coalesce(from_status, '-'), to_status, coalesce(reason, '-'),
                                coalesce(changed_by_user_id::text, '-'))
                      ORDER BY created_at)
     FROM listing_status_history WHERE listing_id = '55555555-5555-4555-8555-555555555555')
    = ARRAY['- | ACTIVE | - | -',
            'ACTIVE | SOLD | Sold to Bob | 11111111-1111-4111-8111-111111111111']);

UPDATE listings SET price_amount = 9000 WHERE id = '55555555-5555-4555-8555-555555555555';
UPDATE listings SET title = 'VW Golf 7, çmim i ulur' WHERE id = '55555555-5555-4555-8555-555555555555';

SELECT pg_temp.check('price history records price changes and ignores other edits',
    (SELECT array_agg(price_amount ORDER BY changed_at)
     FROM listing_price_history WHERE listing_id = '55555555-5555-4555-8555-555555555555')
    = ARRAY[9500, 9000]::numeric[]);

SELECT pg_temp.expect_ok('anonymising a user (the erasure path)',
$$UPDATE users SET status = 'DELETED', deleted_at = now(), email = NULL, phone_e164 = NULL,
                   display_name = 'Deleted user', auth_subject = 'deleted:' || id
  WHERE id = '22222222-2222-4222-8222-222222222222'$$);

SELECT pg_temp.expect_ok('deleting a conversation (messages and participants cascade)',
$$DELETE FROM conversations WHERE id = '88888888-8888-4888-8888-888888888888'$$);

SELECT pg_temp.check('the deleted conversation left no messages behind',
    NOT EXISTS (SELECT 1 FROM messages WHERE conversation_id = '88888888-8888-4888-8888-888888888888'));

\echo '--- category properties'

SELECT pg_temp.check('each kind of the old "type" properties is a subcategory, and every category takes listings, a main one too',
    (SELECT count(*) = 53 FROM categories WHERE replaces_option_id IS NOT NULL)
    AND (SELECT count(*) = 4 FROM categories WHERE parent_id = (SELECT id FROM categories WHERE slug = 'automjete'))
    AND (SELECT count(*) = 9 FROM categories WHERE parent_id = (SELECT id FROM categories WHERE slug = 'pjese-dhe-aksesore'))
    AND NOT EXISTS (SELECT 1 FROM categories WHERE NOT allow_listings));

SELECT pg_temp.check('the old "type" properties are no longer filters, the subcategories are',
    NOT EXISTS (SELECT 1 FROM effective_category_attributes e
                JOIN attribute_options o ON o.attribute_id = e.attribute_id
                JOIN categories c ON c.replaces_option_id = o.id
                WHERE e.filterable));

INSERT INTO category_attributes (category_id, attribute_id, filterable, show_in_card, sort_order)
SELECT c.id, a.id, false, true, 99 FROM categories c, attributes a WHERE c.slug = 'apartamente' AND a.code = 'property_floor';

SELECT pg_temp.check('a subcategory inherits its parent''s properties, and its own settings win',
    (SELECT count(*) = 6 FROM effective_category_attributes e
     JOIN categories c ON c.id = e.category_id WHERE c.slug = 'apartamente')
    AND (SELECT NOT e.filterable AND e.show_in_card AND e.sort_order = 99
         FROM effective_category_attributes e
         JOIN categories c ON c.id = e.category_id
         JOIN attributes a ON a.id = e.attribute_id
         WHERE c.slug = 'apartamente' AND a.code = 'property_floor'));

INSERT INTO category_attributes (category_id, attribute_id, searchable, filterable, sort_order)
SELECT c.id, a.id, false, false, 5 FROM categories c, attributes a WHERE c.slug = 'automjete' AND a.code = 'vehicle_color';
INSERT INTO category_attributes (category_id, attribute_id, filterable, sort_order, label)
SELECT c.id, a.id, false, 15, 'Marka e makinës' FROM categories c, attributes a
WHERE c.slug = 'gps-dhe-navigatore' AND a.code = 'vehicle_make';

SELECT pg_temp.check('three levels down, a kind of part inherits from its parent and grandparent, labels too, and its own settings win',
    (SELECT string_agg(a.code || '=' || coalesce(e.label, '-'), ', ' ORDER BY e.sort_order)
     FROM effective_category_attributes e
     JOIN categories c ON c.id = e.category_id
     JOIN attributes a ON a.id = e.attribute_id
     WHERE c.slug = 'gps-dhe-navigatore')
        = 'vehicle_color=-, vehicle_make=Marka e makinës, vehicle_model=Modeli i automjetit, fits_from_year=-, fits_to_year=-'
    AND (SELECT e.label = 'Marka e automjetit' AND e.filterable
         FROM effective_category_attributes e
         JOIN categories c ON c.id = e.category_id
         JOIN attributes a ON a.id = e.attribute_id
         WHERE c.slug = 'goma-dhe-disqe' AND a.code = 'vehicle_make')
    AND (SELECT count(*) = 9 AND count(e.label) = 0 AND bool_or(a.code = 'vehicle_color' AND e.sort_order = 65)
         FROM effective_category_attributes e
         JOIN categories c ON c.id = e.category_id
         JOIN attributes a ON a.id = e.attribute_id
         WHERE c.slug = 'makina'));

DELETE FROM category_attributes ca USING categories c, attributes a
WHERE ca.category_id = c.id AND ca.attribute_id = a.id
  AND ((c.slug = 'automjete' AND a.code = 'vehicle_color') OR (c.slug = 'gps-dhe-navigatore' AND a.code = 'vehicle_make'));

SELECT pg_temp.expect_error('a listing type categories do not know',
$$UPDATE categories SET listing_types = '{SELL,SWAP}' WHERE slug = 'te-tjera'$$, '23514');

SELECT pg_temp.expect_error('a category without listing types',
$$UPDATE categories SET listing_types = '{}' WHERE slug = 'te-tjera'$$, '23514');

SELECT pg_temp.check('price_in_eur converts with exchange_rates (the flat: 60 000 Lekë = 600 €)',
    price_in_eur(60000, 'ALL') = 600 AND price_in_eur(9500, 'EUR') = 9500
    AND (SELECT price_normalized = price_in_eur(price_amount, currency)
         FROM listings WHERE id = '66666666-6666-4666-8666-666666666666'));

SELECT pg_temp.check('numbers with a unit are grouped with a decimal comma; years are not grouped',
    format_attribute_number(148000, 'km') = E'148 000 km'
    AND format_attribute_number(1234567.25, 'm²') = E'1 234 567,25 m²'
    AND format_attribute_number(1.6, 'l') = E'1,6 l'
    AND format_attribute_number(2016, NULL) = '2016');

SELECT pg_temp.check('listing_search_extra lists searchable values in the category''s order',
    listing_search_extra('55555555-5555-4555-8555-555555555555') = 'Volkswagen Golf 2016 Manuale');

\echo '--- number plate codes'

SELECT pg_temp.check('every municipality has the code of its old district, and there were 36 districts',
    (SELECT count(*) = 61 AND count(plate_code) = 61 AND count(DISTINCT plate_code) = 36
     FROM locations WHERE location_type = 'MUNICIPALITY')
    AND (SELECT plate_code = 'TR' FROM locations WHERE slug = 'kamez' AND location_type = 'MUNICIPALITY'));

INSERT INTO locations (id, parent_id, location_type, name, slug)
SELECT 'cccccccc-cccc-4ccc-8ccc-cccccccccccc', id, 'CITY', 'Peshkopi', 'peshkopi'
FROM locations WHERE slug = 'diber' AND location_type = 'MUNICIPALITY';

SELECT pg_temp.check('a town under a municipality belongs to it and shows its plate code (Peshkopi: Dibër, DI)',
    location_municipality('cccccccc-cccc-4ccc-8ccc-cccccccccccc')
        = (SELECT id FROM locations WHERE slug = 'diber' AND location_type = 'MUNICIPALITY')
    AND location_plate_code('cccccccc-cccc-4ccc-8ccc-cccccccccccc') = 'DI');

SELECT pg_temp.expect_error('a plate code that isn''t two capital letters',
$$UPDATE locations SET plate_code = 'T1' WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'$$, '23514');

\echo '--- map search'

SELECT pg_temp.check('every municipality has a point on the map, inside Albania',
    (SELECT count(*) = 61 AND count(latitude) = 61
            AND bool_and(latitude BETWEEN 39.6 AND 42.7 AND longitude BETWEEN 19.2 AND 21.1)
     FROM locations WHERE location_type = 'MUNICIPALITY'));

SELECT pg_temp.check('Tirana is 31 km from Durrës, as the crow flies',
    (SELECT round(earth_distance(ll_to_earth(t.latitude::float8, t.longitude::float8),
                                 ll_to_earth(d.latitude::float8, d.longitude::float8)) / 1000) = 31
     FROM locations t, locations d
     WHERE t.slug = 'tirane' AND t.location_type = 'MUNICIPALITY'
       AND d.slug = 'durres' AND d.location_type = 'MUNICIPALITY'));

\echo 'all checks passed'

ROLLBACK;
