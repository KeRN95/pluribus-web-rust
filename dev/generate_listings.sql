-- As many listings as you ask for, for trying the development site full and at a real site's
-- scale: in every category, in about the shares a real site has them, each with the properties its
-- category takes, a town and a point near it on the map, a seller with a phone number, and two to
-- four photos of the kind of thing it is. Titles, descriptions and prices are put together from
-- what Albanian listings say and cost, so search, the filters and the map have realistic words and
-- numbers to work on.
--
-- After the migrations (run the app once), from a bash shell:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v count=10000 < dev/generate_listings.sql
-- count is how many to make, 10 000 if left out; runs add up. seed, between -1 and 1, makes a run
-- repeatable: -v seed=0.42. remove=1 removes everything the script has made, and nothing else:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v remove=1 < dev/generate_listings.sql
-- only=<top-level category> keeps either to that one's listings (removing, with the sellers it
-- leaves without any): making electronics anew, say, after their properties changed:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v remove=1 -v only=elektronike < dev/generate_listings.sql
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v count=20000 -v only=elektronike < dev/generate_listings.sql
-- photos=1 makes nothing, but gives the listings there that have only a cover (from an earlier
-- version of this script, dev/sample_listings.sql or tests/fixtures/listings.sql) more of their
-- category's photos; listings people made on the site keep theirs:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v photos=1 < dev/generate_listings.sql
-- A property listing stands in the zone of its town its title names, where the town has zones
-- (migrations/0019_zones.sql). zones=1 makes nothing, but moves the ones earlier versions of this
-- script left in the town into their zones:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v zones=1 < dev/generate_listings.sql
-- views=1 makes nothing, but gives a twentieth of the active listings it made views over the last
-- two weeks (listing_daily_stats), a few of them many, as the site counts them (src/views.rs), for
-- the welcome page's "Më të klikuarat"; runs add up:
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v views=1 < dev/generate_listings.sql
-- drops=1 makes nothing, but makes a twentieth of the active listings it made with a price, on
-- sale, for rent or a service, 8 to 25% cheaper, as if their sellers had lowered them: the price
-- history records it (its trigger), and their cards and pages say by how much (price_was! in
-- src/db/mod.rs); runs add up. Those who keep a listing hear only of the drops its seller makes on
-- the site (src/db/price_drops.rs):
--   docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v drops=1 < dev/generate_listings.sql
--
-- All of it is fictional: the sellers (dev|generated-…, …@example.test, phone numbers
-- +355 69 1xx xxxx, businesses' +355 68 1xx xxxx), what they sell, and for how much. The photos
-- are free ones from Unsplash, served by its image CDN, and credited (see photos below).

\if :{?count}
\else
\set count 10000
\endif
\if :{?remove}
\else
\set remove false
\endif
\if :{?photos}
\else
\set photos false
\endif
\if :{?zones}
\else
\set zones false
\endif
\if :{?views}
\else
\set views false
\endif
\if :{?drops}
\else
\set drops false
\endif

\if :drops
-- Each a price 8 to 25% lower, rounded as people round prices, to the hundred from a thousand up and
-- to the ten from a hundred; one rounding left less than 1% lower is left as it was.
WITH picked AS (
    SELECT l.id, l.currency, l.price_amount AS was, l.price_amount * (0.75 + 0.17 * random())::numeric AS cheaper
    FROM listings l
    JOIN users u ON u.id = l.created_by_user_id
    WHERE u.auth_subject LIKE 'dev|generated-%' AND l.status = 'ACTIVE'
      AND l.listing_type IN ('SELL', 'RENT', 'SERVICE') AND l.price_type IN ('FIXED', 'NEGOTIABLE', 'FROM')
      AND l.price_amount IS NOT NULL AND random() < 0.05
),
cut AS (
    SELECT id, currency, was,
           CASE WHEN cheaper >= 1000 THEN round(cheaper, -2) WHEN cheaper >= 100 THEN round(cheaper, -1) ELSE round(cheaper) END AS amount
    FROM picked
),
lowered AS (
    UPDATE listings l
    SET price_amount = cut.amount, price_normalized = price_in_eur(cut.amount, cut.currency)
    FROM cut
    WHERE l.id = cut.id AND cut.amount > 0 AND cut.amount <= cut.was * 0.99
    RETURNING l.id
)
SELECT format('Lowered the prices of %s listings.', count(*)) AS done FROM lowered \gset
\echo :done
\quit
\endif

\if :views
-- Each listing a number of views a day, most of them few and a few hundreds, a little more or less
-- from day to day and fewer the longer ago; those under one a day get none.
INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views)
SELECT seen.id, current_date - day, round(seen.daily * (0.6 + 0.8 * random()) * (1 - day / 20.0))::bigint
FROM (SELECT l.id, 300 * power(random(), 6) AS daily
      FROM listings l
      JOIN users u ON u.id = l.created_by_user_id
      WHERE u.auth_subject LIKE 'dev|generated-%' AND l.status = 'ACTIVE' AND random() < 0.05) seen
CROSS JOIN generate_series(0, 13) AS day
WHERE seen.daily >= 1
ON CONFLICT (listing_id, stat_date)
DO UPDATE SET detail_views = listing_daily_stats.detail_views + EXCLUDED.detail_views;
SELECT format('Gave %s listings views.', count(DISTINCT listing_id)) AS done
FROM listing_daily_stats WHERE stat_date = current_date AND detail_views > 0 \gset
\echo :done
\quit
\endif

\if :remove
BEGIN;
\if :{?only}
DELETE FROM listings
WHERE created_by_user_id IN (SELECT id FROM users WHERE auth_subject LIKE 'dev|generated-%')
  AND category_id IN (WITH RECURSIVE family AS (
                          SELECT id FROM categories WHERE slug = :'only'
                          UNION ALL
                          SELECT c.id FROM categories c JOIN family f ON c.parent_id = f.id
                      )
                      SELECT id FROM family);
-- And the sellers that leaves without listings.
DELETE FROM businesses b
WHERE b.slug LIKE 'gen-%' AND NOT EXISTS (SELECT 1 FROM listings l WHERE l.seller_business_id = b.id);
DELETE FROM users u
WHERE u.auth_subject LIKE 'dev|generated-%'
  AND NOT EXISTS (SELECT 1 FROM listings l WHERE l.created_by_user_id = u.id)
  AND NOT EXISTS (SELECT 1 FROM business_members m WHERE m.user_id = u.id);
\else
DELETE FROM listings WHERE created_by_user_id IN (SELECT id FROM users WHERE auth_subject LIKE 'dev|generated-%');
DELETE FROM businesses WHERE slug LIKE 'gen-%';
DELETE FROM users WHERE auth_subject LIKE 'dev|generated-%';
\endif
REFRESH MATERIALIZED VIEW search_words;
REFRESH MATERIALIZED VIEW zone_area_prices;
REFRESH MATERIALIZED VIEW car_model_prices;
COMMIT;
\if :{?only}
\echo 'Removed the generated listings under' :only 'and the sellers left without any.'
\else
\echo 'Removed the generated listings, their sellers and businesses.'
\endif
\quit
\endif

\if :{?seed}
SELECT setseed(:seed) \g /dev/null
\endif

-- Parallel workers share memory through /dev/shm, which Docker keeps to 64 MB, and vacuuming a few
-- hundred thousand listings asks for more: this run goes without them.
SET max_parallel_workers_per_gather = 0;
SET max_parallel_maintenance_workers = 0;

BEGIN;

-- ---------------------------------------------------------------- helpers

-- One of the choices; a choice listed twice comes twice as often.
CREATE FUNCTION pg_temp.pick(choices anyarray) RETURNS anyelement LANGUAGE sql VOLATILE
AS $$ SELECT choices[1 + floor(random() * cardinality(choices))::int] $$;

-- A whole number from low to high, both included.
CREATE FUNCTION pg_temp.number_between(low int, high int) RETURNS int LANGUAGE sql VOLATILE
RETURN low + floor(random() * (high - low + 1))::int;

-- A few of the sentences, in no particular order: the middle of a description.
CREATE FUNCTION pg_temp.sentences(choices text[], how_many int) RETURNS text LANGUAGE sql VOLATILE
RETURN (SELECT string_agg(sentence, ' ') FROM (SELECT sentence FROM unnest(choices) AS sentence ORDER BY random() LIMIT how_many) chosen);

-- A price as sellers round it: to tens, hundreds or thousands as it grows. It takes a double,
-- as prices worked out with random() are.
CREATE FUNCTION pg_temp.round_price(price float8) RETURNS numeric LANGUAGE sql IMMUTABLE
RETURN CASE WHEN price < 1000 THEN round(price::numeric, -1)
            WHEN price < 20000 THEN round(price::numeric, -2)
            WHEN price < 1000000 THEN round(price::numeric, -3)
            ELSE round(price::numeric, -4) END;

-- A wanted listing's title: "Blej" and the thing's name, its first letter in lower case after it
-- ("Blej divan qoshe"), unless the name starts with a proper name or an abbreviation ("Blej
-- Golden Retriever", "Blej GPS Garmin").
CREATE FUNCTION pg_temp.wanted(name text) RETURNS text LANGUAGE sql IMMUTABLE
RETURN 'Blej ' || CASE WHEN substr(name, 2, 1) ~ '[[:upper:]]'
                            OR split_part(name, ' ', 1) IN ('Golden', 'Husky', 'Pomeranian', 'British', 'Apple')
                       THEN name
                       ELSE lower(left(name, 1)) || substr(name, 2) END;

-- What a listing says and costs, and its properties: {"code": option code, number, or a
-- multi-select's option codes}. Prices are in its category's currency.
CREATE TYPE pg_temp.writing AS (title text, description text, listing_type text, condition text,
                                price numeric, price_type text, properties jsonb);

-- A condition, in about the shares second-hand things come in.
CREATE FUNCTION pg_temp.condition() RETURNS text LANGUAGE sql VOLATILE
RETURN pg_temp.pick(ARRAY['NEW', 'NEW', 'LIKE_NEW', 'LIKE_NEW', 'LIKE_NEW', 'USED', 'USED', 'USED', 'USED']);

-- What a condition does to a price; without one (pets), nothing.
CREATE FUNCTION pg_temp.worth(condition text) RETURNS numeric LANGUAGE sql IMMUTABLE
RETURN CASE condition WHEN 'LIKE_NEW' THEN 0.8 WHEN 'USED' THEN 0.6 WHEN 'FOR_PARTS' THEN 0.25 ELSE 1 END;

-- The makes and models sellers pick from, how often each is sold here, and about what a new one
-- costs, in euros.
CREATE TEMP TABLE makes AS
SELECT o.value_code AS code, o.label,
       CASE o.value_code
           WHEN 'MERCEDES_BENZ' THEN 14 WHEN 'VOLKSWAGEN' THEN 14 WHEN 'BMW' THEN 9 WHEN 'AUDI' THEN 8
           WHEN 'TOYOTA' THEN 5 WHEN 'OPEL' THEN 5 WHEN 'FIAT' THEN 5 WHEN 'FORD' THEN 4
           WHEN 'PEUGEOT' THEN 3 WHEN 'RENAULT' THEN 3 WHEN 'HYUNDAI' THEN 3 WHEN 'KIA' THEN 3
           WHEN 'SKODA' THEN 3 WHEN 'NISSAN' THEN 2 WHEN 'CITROEN' THEN 2 WHEN 'SEAT' THEN 2
           WHEN 'LAND_ROVER' THEN 2 WHEN 'VOLVO' THEN 1.5 WHEN 'PORSCHE' THEN 1 WHEN 'MAZDA' THEN 1
           WHEN 'HONDA' THEN 1 WHEN 'JEEP' THEN 1 WHEN 'DACIA' THEN 1 WHEN 'SUZUKI' THEN 1
           WHEN 'MINI' THEN 1 WHEN 'ALFA_ROMEO' THEN 1 WHEN 'MITSUBISHI' THEN 0.8 WHEN 'LEXUS' THEN 0.7
           WHEN 'TESLA' THEN 0.6 WHEN 'BYD' THEN 0.5
           ELSE 0.3
       END AS weight,
       CASE WHEN o.value_code = 'PORSCHE' THEN 95000
            WHEN o.value_code = 'LAND_ROVER' THEN 70000
            WHEN o.value_code IN ('MERCEDES_BENZ', 'BMW', 'AUDI', 'LEXUS', 'JAGUAR', 'TESLA', 'VOLVO') THEN 50000
            WHEN o.value_code IN ('VOLKSWAGEN', 'TOYOTA', 'MAZDA', 'HONDA', 'JEEP', 'MINI', 'SUBARU', 'ALFA_ROMEO', 'BYD', 'MITSUBISHI') THEN 30000
            WHEN o.value_code IN ('DACIA', 'LADA', 'DAEWOO', 'SMART', 'FIAT', 'SUZUKI') THEN 16000
            ELSE 24000
       END AS new_price
FROM attribute_options o
JOIN attributes a ON a.id = o.attribute_id
WHERE a.code = 'vehicle_make' AND o.active;

CREATE TEMP TABLE models AS
SELECT p.value_code AS make, m.value_code AS code, m.label
FROM attribute_options m
JOIN attributes a ON a.id = m.attribute_id
JOIN attribute_options p ON p.id = m.parent_option_id
WHERE a.code = 'vehicle_model' AND m.active AND p.active;
CREATE INDEX ON models (make);

-- ---------------------------------------------------------------- photos

-- Photos of each category's things: the sample listings' and more of the same kinds, all free
-- under the Unsplash License (https://unsplash.com/license) and served by Unsplash's image CDN.
-- dev/sample_listings.sql credits its own; the end of this file credits the others.
CREATE TEMP TABLE photos (category text, photo text, PRIMARY KEY (category, photo));
INSERT INTO photos
SELECT category, unnest(string_to_array(list, ','))
FROM (VALUES
    ('aksesore', 'photo-1572635196237-14b3f281503f,photo-1584036553516-bf83210aa16c,photo-1624222247344-550fb60583dc,photo-1620109176813-e91290f6c795,photo-1595642527925-4d41cb781653,photo-1521369909029-2afed882baee'),
    ('aksesore-per-kafshe', 'photo-1653638390484-bc1c263ce9e4,photo-1708062270853-ee7c66b69f07,photo-1546840125-ba012c71366f,photo-1565338441575-78707da1528c,photo-1598397678815-c5dc869035b8,photo-1761614282055-29e039aac354'),
    ('aparate-fotografike', 'photo-1616088886430-ccd86fef0713,photo-1495707902641-75cac588d2e9,photo-1512790182412-b19e6d62bc39,photo-1580707221190-bd94d9087b7f,photo-1502982720700-bfff97f2ecac'),
    ('apartamente', 'photo-1515263487990-61b07816b324,photo-1545324418-cc1a3fa10c00,photo-1560448075-57d0285fc59b,photo-1629042306558-7d1e15cc02fa,photo-1630699144641-72fa7a6b8aa1,photo-1630699144867-37acec97df5a,photo-1630699293875-e56c25151c4b,photo-1630699375019-c334927264df,photo-1654506012740-09321c969dc2,photo-1665249934445-1de680641f50,photo-1677100091551-748c921e6609'),
    ('audio-dhe-kufje', 'photo-1505740420928-5e560c06d30e,photo-1572569511254-d8f925fe2cbb,photo-1606841837239-c5a1a4a07af7,photo-1618366712010-f4ae9c647dcb'),
    ('audio-dhe-video', 'photo-1592570714618-15e2d4719c6c,photo-1523987659364-8ad26ea657ef,photo-1470948062737-aeb014e274c2,photo-1648892936786-de40bb1080a6,photo-1726248985986-20bef82d2b52,photo-1758411898478-4f7e70d533d9'),
    ('automjete-tregtare', 'photo-1587813369290-091c9d432daf,photo-1641199788912-9a7385a35c82,photo-1768400554801-2002b63e0591,photo-1742858492775-8f58f645aa12,photo-1768393775846-6d0bd756a847,photo-1776982298134-d24cc9a3de2b'),
    ('bicikleta', 'photo-1501147830916-ce44a6359892,photo-1533107862482-0e6974b06ec4,photo-1584278140365-2ab8bff2be82,photo-1606087492572-424ebe0f2f61'),
    ('canta', 'photo-1605733513597-a8f8341084e6,photo-1705909237050-7a7625b47fac,photo-1598532163257-ae3c6b2524b6,photo-1584917865442-de89df76afd3,photo-1614179689702-355944cd0918,photo-1600857062241-98e5dba7f214'),
    ('dekor', 'photo-1595455967759-5f1356993e53,photo-1612392549429-a874f09c65f1,photo-1531762948975-73032b7b61f4,photo-1612196808214-b8e1d6145a8c,photo-1631125915902-d8abe9225ff2,photo-1572048572872-2394404cf1f3'),
    ('dyqane', 'photo-1584472376859-889e77a8ccac,photo-1777879760931-3a21a9bf42d6,photo-1761850648640-2ee5870ee883,photo-1580554430120-94cfcb3adf25,photo-1687114835860-4c22e258781f'),
    ('elektroshtepiake', 'photo-1626806787461-102c1bfaaea1,photo-1626806819282-2c1dc01a5e0c,photo-1642497590397-7f4384a19966,photo-1718203862467-c33159fdc504'),
    ('energji-dhe-gjeneratore', 'photo-1658260867231-535a1f7c98b9,photo-1509391366360-2e959784a276,photo-1768839722298-0491a671634d,photo-1508514177221-188b1cf16e9d,photo-1724041875334-0a6397111c7e'),
    ('fitnes', 'photo-1638536532686-d610adfc8e5c,photo-1672344048213-76b6e77304bd,photo-1775993699105-4d18bcac7e54,photo-1784894645353-81486c66a687,photo-1576678927484-cc907957088c,photo-1770493895453-4f758c40d11d'),
    ('foto-dhe-video', 'photo-1623783356340-95375aac85ce,photo-1770027705793-e68be23cdd5f,photo-1772441936562-b7136602856e,photo-1772441936553-31bf98952dfc,photo-1542038784456-1ea8e935640e'),
    ('fustane', 'photo-1549488497-94b52bddac5d,photo-1623609163859-ca93c959b98a,photo-1773394343278-21f63954867e,photo-1595777457583-95e059d581b8,photo-1612336307429-8a898d10e223'),
    ('garazhe', 'photo-1635108198854-26645ffe6714,photo-1551522435-a13afa10f103,photo-1778674556730-357e85ec6cb2,photo-1762568742298-bdb91e2cbdd9,photo-1780142904615-a7ed0b22f98c'),
    ('garsoniera', 'photo-1702014859878-5d4743176d28,photo-1702014862053-946a122b920d,photo-1786564026100-7ad7ae7654b2,photo-1702014857653-dcea938d51f0,photo-1785232244548-5cfe6fb60418'),
    ('goma-dhe-disqe', 'photo-1571335746824-742511d49bce,photo-1764699186616-8f707281e4f3,photo-1760836395760-9831facb4754,photo-1444947295498-07f60c19a4ff,photo-1623564493214-6137dff043ad,photo-1477823986828-5aff156284aa'),
    ('gps-dhe-navigatore', 'photo-1634743556192-d19f0c69ff3a,photo-1621948535633-544972f1b944,photo-1514861736016-646d83403e24,photo-1765959106936-851735565c12,photo-1733135029996-a447004b7bb0,photo-1695632231361-7468689ed9a4'),
    ('instrumente-muzikore', 'photo-1510915361894-db8b60106cb1,photo-1541689592655-f5f52825a3b8,photo-1561014816-50ce5acb5e73,photo-1564186763535-ebb21ef5277f,photo-1787074191751-78c2044f2fb7,photo-1511379938547-c1f69419868d,photo-1516924962500-2b4b3b99ea02'),
    ('kamping-dhe-peshkim', 'photo-1504280390367-361c6d9f38f4,photo-1541742425281-c1d3fc8aff96,photo-1631635589499-afd87d52bf64,photo-1778017342222-e9c6047eb565,photo-1775626095828-cb5312e199b1'),
    ('kepuce', 'photo-1542291026-7eec264c27ff,photo-1595950653106-6c9ebd614d3a,photo-1761777939781-2fb1b636d5fc,photo-1606107557195-0e29a4b5b4aa'),
    ('kompjutere', 'photo-1517518295033-d5ab8ca078cc,photo-1560195307-95127677e806,photo-1660855551740-4474188debdb,photo-1696710257827-75e2e5954059,photo-1719927604476-dc404b85358f'),
    ('konsola-lojerash', 'photo-1606144042614-b2417e99c4e3,photo-1607853202273-797f1c22a38e,photo-1612036781124-847f8939b154,photo-1612287230202-1ff1d85d1bdf,photo-1634924052395-c8a61c1caedb'),
    ('kopsht', 'photo-1538307602205-80b5c2ff26ec,photo-1621506821957-1b50ab7787a4,photo-1601654253194-260e0b6984f9,photo-1778594006034-230d11763ebb,photo-1591383496652-db773e57b1d0,photo-1585320806297-9794b3e4eeae'),
    ('laptope', 'photo-1496181133206-80ce9b88a853,photo-1541807084-5c52b6b3adef,photo-1743456056112-0739a6742135,photo-1743456056142-1aaf69656dfa'),
    ('mace', 'photo-1585137173132-cf49e10ad27d,photo-1514888286974-6c03e2ca1dba,photo-1764708844823-00d2188dafd8,photo-1759588032622-1388cf9505ad,photo-1574144611937-0df059b5ef3e'),
    ('makina', 'photo-1563456162079-bdcab0fe5f2f,photo-1589215951246-45f030301f5f,photo-1591230740238-e9a71182b67c,photo-1602837381669-9e60df2c0c8c,photo-1605475300127-0a31e8273bc2,photo-1606664515524-ed2f786a0bd6,photo-1611580568467-a8e2bb344bbf,photo-1611580568660-28050c1d5b5c,photo-1618767747322-64e376fd4826,photo-1624085568108-36410cfe4d24,photo-1625690096555-a0a4d190901c,photo-1626072557464-90403d788e8d,photo-1627018549036-9e88bf289218,photo-1638618164682-12b986ec2a75,photo-1726003536800-b9ec0888cf36,photo-1727500569755-008ab3baecea,photo-1729966085578-c7b281cf11da,photo-1734554381974-56e06a32453c'),
    ('makineri-bujqesore', 'photo-1614977645540-7abd88ba8e56,photo-1761839257961-4dce65b72d99,photo-1615811361523-6bd03d7748e7,photo-1606739211185-2c846d734a6d'),
    ('makineri-industriale', 'photo-1630930678172-63343537a00a,photo-1655165312002-9d781ad4046e,photo-1717386255773-1e3037c81788,photo-1610891015188-5369212db097,photo-1496247749665-49cf5b1022e9,photo-1717386255767-52643970d483'),
    ('makineri-ndertimi', 'photo-1628645419184-26a1f2757340,photo-1690719744562-249937b9c03a,photo-1768085091650-e8e89ef57b76,photo-1764422743505-97c9ea0e43f2,photo-1630288214173-a119cf823388'),
    ('mbushes-per-makina-elektrike', 'photo-1593941707874-ef25b8b4a92b,photo-1775259928422-779c18e32007,photo-1593941707882-a5bba14938c7,photo-1704475336842-0ab3798abf0e,photo-1766507680004-1c71007aefe5,photo-1784039476800-0b45dda6d2ef'),
    ('mesime', 'photo-1653276071161-379c70560951,photo-1580894732444-8ecded7900cd,photo-1758270704262-ecc82b23dc37,photo-1454165804606-c3d57bc86b40,photo-1758687126375-e2c1683219e9'),
    ('mobilje', 'photo-1521587760476-6c12a4b040da,photo-1547062200-f195b1c77e30,photo-1590938401285-eba3a552ece4,photo-1593430980369-68efc5a5eb34,photo-1601366533287-5ee4c763ae4e,photo-1658595149684-3c3c6aa3c154,photo-1672137233327-37b0c1049e77,photo-1698936061086-2bf99c7b9fc5,photo-1714872245785-674ae3038d21,photo-1718894071402-fb944e2a1849,photo-1759722665935-0967b4e0da93'),
    ('motocikleta', 'photo-1609630875171-b1321377ee65,photo-1591637333184-19aa84b3e01f,photo-1588627541420-fce3f661b779,photo-1554223789-df81106a45ed,photo-1519750292352-c9fc17322ed7,photo-1761026533058-1c0080104977'),
    ('ndertim-dhe-lyerje', 'photo-1525909002-1b05e0c869d8,photo-1562259949-e8e7689d7828,photo-1761986757577-140af8859587,photo-1717281234297-3def5ae3eee1'),
    ('ora', 'photo-1523170335258-f5ed11844a49,photo-1620625515032-6ed0c1790c75,photo-1773755060442-6d128b70dd20,photo-1524805444758-089113d48a6d,photo-1542496658-e33a6d0d50f6'),
    ('pastrim', 'photo-1740657254989-42fe9c3b8cce,photo-1758272422155-d898efd14f81,photo-1758273238415-01ec03d9ef27,photo-1686178827149-6d55c72d81df'),
    ('peshq', 'photo-1606137029647-71daa7c9c5c8,photo-1522069169874-c58ec4b76be5,photo-1784766029024-fe0adb02c313,photo-1764175760902-9cf284d444de,photo-1535591273668-578e31182c4f'),
    ('pjese-kembimi', 'photo-1767340078222-98633f3ffd27,photo-1759419281419-b04552b2691a,photo-1779263569870-e7a0fbc25686,photo-1549207107-2704df6b92ab,photo-1696494561430-de087dd0bd69,photo-1537462715879-360eeb61a0ad'),
    ('pjese-te-tjera', 'photo-1781737873599-2cdef978d65d,photo-1585569695919-db237e7cc455,photo-1535190458486-5fc153d35688,photo-1765211003923-bffcb36fb4a7,photo-1789286789372-22e0cb35f045,photo-1558906050-d6d6aa390fd3'),
    ('pune-call-center', 'photo-1560264280-88b68371db39,photo-1766066014237-00645c74e9c6,photo-1553775282-20af80779df7,photo-1504384308090-c894fdcc538d'),
    ('pune-it', 'photo-1542831371-29b0f74f9713,photo-1607799279861-4dd421887fb3,photo-1758873268745-dd2cf0d677b5,photo-1787285187589-ddbdf58b81b1'),
    ('pune-ndertim', 'photo-1587582423116-ec07293f0395,photo-1589939705384-5185137a7f0f,photo-1772442198624-4fc4d7281e89,photo-1773430272653-375174373c4f'),
    ('pune-shendetesi', 'photo-1584432810601-6c7f27d2362b,photo-1624727828489-a1e03b79bba8,photo-1758691462651-611d730c5272,photo-1758575514475-2a84975db58e,photo-1761234852472-85aeea9c3eac'),
    ('pune-shitje', 'photo-1567401893414-76b7b1e5a7a5,photo-1647427017067-8f33ccbae493,photo-1556740738-b6a63e27c4df,photo-1778790508237-3c6cb00f152a,photo-1641029956071-272caab5857a'),
    ('pune-transport', 'photo-1485575301924-6891ef935dcd,photo-1616432043562-3671ea2e5242,photo-1778103617525-76877c583fa5,photo-1591768793355-74d04bb6608f,photo-1592805144716-feeccccef5ac'),
    ('pune-turizem', 'photo-1512485800893-b08ec1ea59b1,photo-1572715376701-98568319fd0b,photo-1588675646184-f5b0b0b0b2de,photo-1611266353853-d370b67187ed,photo-1647776112336-72f4c30fafc1,photo-1653233797467-1a528819fd4f,photo-1759038085950-1234ca8f5fed'),
    ('qen', 'photo-1586671267731-da2cf3ceeb80,photo-1587764379873-97837921fd44,photo-1619980296991-5c0d64b23950,photo-1655986910739-daa90b28095a'),
    ('rimorkio', 'photo-1499147463149-adc471bbc639,photo-1790707760473-e1d8b2126d06,photo-1637417494980-c9b1992df445,photo-1520101244246-293f77ffc39e,photo-1639678592362-b2bb42fa2725,photo-1776221307349-717fc7944510'),
    ('riparime', 'photo-1515378791036-0648a3ef77b2,photo-1620653713380-7a34b773fef8,photo-1621905251189-08b45d6a269e,photo-1676210134188-4c05dd172f89,photo-1682345262055-8f95f3c513ea'),
    ('sport-te-tjera', 'photo-1676827613262-5fba25cee5fd,photo-1774755461355-5c576a2e6030,photo-1786025106105-88a13e54f548,photo-1609710228159-0fa9bd7c0827'),
    ('sporte-dimerore', 'photo-1551698618-1dfe5d97d256,photo-1771777400683-8614965bfce5,photo-1565992441121-4367c2967103,photo-1516352267226-f5f3e4c53781'),
    ('styling-dhe-pajisje-sportive', 'photo-1781120358189-20949d90a604,photo-1762153972684-d15c9e8ce662,photo-1788718600150-84f1e18a9f36,photo-1785620656656-58c683ee409f,photo-1784894379255-0f21e0e86322,photo-1777173687467-c5b4e09392f1'),
    ('tableta', 'photo-1587033411391-5d9e51cce126,photo-1660100970784-a1cc183a91b7,photo-1758973935099-5b662a863f6c,photo-1760708369071-e8a50a8979cb,photo-1775288487309-35c588890167'),
    ('tapete-makine', 'photo-1761846786526-706cf7015afb,photo-1626966473515-71dc32f2aedd,photo-1685783681555-903feddd4ced,photo-1688054004445-7c9108eb4005,photo-1640705049944-de916f415bb4,photo-1768853143263-72d7e744bbc2'),
    ('te-tjera', 'photo-1497633762265-9d179a990aa6,photo-1616714109948-c74fe5029a4d,photo-1714392512700-4cab9e51710b,photo-1596461404969-9ae70f2830c1,photo-1766090503766-623b62f0da26,photo-1599623560574-39d485900c95,photo-1558060370-d644479cb6f7'),
    ('telefona', 'photo-1483478550801-ceba5fe50e8e,photo-1632518192421-56aba73eae55,photo-1632518193213-b34d7731376b,photo-1707438095940-1eee18e85400,photo-1709744722656-9b850470293f,photo-1726574686436-5ef90358e032'),
    ('televizore', 'photo-1567690187548-f07b1d7bf5a9,photo-1646861039459-fd9e3aabf3fb,photo-1560169897-fc0cdbdfa4d5,photo-1509281373149-e957c6296406'),
    ('transport-dhe-transferime', 'photo-1586781383963-8e66f88077ec,photo-1614359835514-92f8ba196357,photo-1779517226273-bcf843b759b9,photo-1758219944627-4d9b32ed7019,photo-1695222833131-54ee679ae8e5,photo-1485575301924-6891ef935dcd'),
    ('truall', 'photo-1560493676-04071c5f467b,photo-1563514227147-6d2ff665a6a0,photo-1651769005074-2ef5c6f74276,photo-1599809563132-4b678fb6f611,photo-1586803555480-d98e84e39cf2,photo-1787773894172-50005b81be16'),
    ('vila-dhe-shtepi', 'photo-1599932904184-02a07911e629,photo-1721222204128-3f8262e14f35,photo-1787304680952-d53c92b40df6,photo-1781797221329-c3d083ab6d99,photo-1787094497609-5efe6029b950,photo-1598228723793-52759bba239c,photo-1777428762767-e3a797da389a'),
    ('xhaketa-dhe-pallto', 'photo-1521223890158-f9f7c3d5d504,photo-1539533018447-63fcce2678e3,photo-1539533113208-f6df8cc8b543,photo-1551028719-00167b16eac5,photo-1624548140129-74786c5f1279,photo-1785669430022-12ff54ab8948'),
    ('zogj', 'photo-1552728089-57bdde30beb3,photo-1771413105794-f15d1e9ed6c7,photo-1544923408-75c5cef46f14,photo-1758120014319-a7237c9f4a47,photo-1452570053594-1b985d6ea890'),
    ('zyra', 'photo-1497215728101-856f4ea42174,photo-1606857521015-7f9fcf423740,photo-1758448500688-3ababa93fd67,photo-1768836180167-6d4a25c421b2,photo-1579487785973-74d2ca7abdd5,photo-1765371514743-45bd8e6c0a28')
) AS pools (category, list);
ANALYZE photos;

-- The listings to give photos to: how many each is to have in all, two to four, drawn once and
-- kept (drawn in the query that picks them, it could be drawn again for every photo it tries);
-- the number of the first to make, 0 for the cover; the photo it has already, which it doesn't get
-- twice; and the key its cover is stored under, which the other photos' add /1, /2... to.
CREATE TEMP TABLE galleries (id uuid PRIMARY KEY, category text NOT NULL, photos int NOT NULL,
                             first int NOT NULL, cover text, storage_key text NOT NULL);

-- Gives the listings in galleries their photos: their category's, in a random order, the first
-- the cover. A category without photos of its own gets free ones from Lorem Picsum
-- (picsum.photos), picked by the listing's id: the same at both sizes and on every visit, though
-- landscapes and streets rather than the things themselves.
CREATE FUNCTION pg_temp.give_photos() RETURNS void LANGUAGE sql VOLATILE
-- Memoize would keep a category's draw for its next listing, which would get the same photos.
SET enable_memoize = off
AS $$
INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url,
                           width_px, height_px, sort_order, is_cover)
SELECT g.id, 'IMAGE', 'READY', g.storage_key || CASE WHEN chosen.n > 0 THEN '/' || chosen.n ELSE '' END,
       'https://images.unsplash.com/' || chosen.photo || '?auto=format&fit=crop&w=1200&h=900&q=75',
       'https://images.unsplash.com/' || chosen.photo || '?auto=format&fit=crop&w=480&h=360&q=70',
       1200, 900, chosen.n, chosen.n = 0
FROM galleries g
CROSS JOIN LATERAL (SELECT p.photo, g.first - 1 + row_number() OVER (ORDER BY random()) AS n
                    FROM photos p
                    WHERE p.category = g.category AND p.photo IS DISTINCT FROM g.cover
                    ORDER BY n
                    LIMIT g.photos - g.first) AS chosen;

INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url,
                           width_px, height_px, sort_order, is_cover)
SELECT g.id, 'IMAGE', 'READY', g.storage_key || CASE WHEN n > 0 THEN '/' || n ELSE '' END,
       'https://picsum.photos/seed/' || g.id || CASE WHEN n > 0 THEN '-' || n ELSE '' END || '/1200/900',
       'https://picsum.photos/seed/' || g.id || CASE WHEN n > 0 THEN '-' || n ELSE '' END || '/480/360',
       1200, 900, n, n = 0
FROM galleries g
CROSS JOIN generate_series(g.first, g.photos - 1) AS n
WHERE NOT EXISTS (SELECT 1 FROM photos p WHERE p.category = g.category);
$$;

-- The zone of its town a property's title names, "…, Blloku, Tiranë" (migrations/0019_zones.sql),
-- if any: the place the listing is in. The names earlier versions of this script spelled otherwise
-- than the zones count too.
CREATE FUNCTION pg_temp.zone_of(title text, town uuid) RETURNS uuid LANGUAGE sql STABLE
RETURN (SELECT z.id
        FROM locations z
        LEFT JOIN (VALUES ('lunder', 'Lunder'), ('xhamlliku', 'Xhamllik')) AS spelled (slug, name) ON spelled.slug = z.slug
        WHERE z.parent_id = town AND z.location_type = 'AREA'
          AND (title LIKE '%, ' || z.name || ', %' OR title LIKE '%, ' || spelled.name || ', %')
        ORDER BY length(z.name) DESC
        LIMIT 1);

\if :zones
-- Property listings this script made before it put them in their zones, which stand in their
-- town, move to the zone their title names, and search finds them by it. Listings people made on
-- the site stay where they are.
CREATE TEMP TABLE zoned AS
SELECT l.id, pg_temp.zone_of(l.title, l.location_id) AS zone_id
FROM listings l
JOIN users u ON u.id = l.created_by_user_id
JOIN categories c ON c.id = l.category_id
JOIN categories family ON family.id = coalesce(c.parent_id, c.id)
WHERE u.auth_subject LIKE 'dev|generated-%' AND family.slug = 'prona';
DELETE FROM zoned WHERE zone_id IS NULL;

UPDATE listings l SET location_id = z.zone_id FROM zoned z WHERE z.id = l.id;
UPDATE listings SET search_extra = listing_search_extra(id) WHERE id IN (SELECT id FROM zoned);
REFRESH MATERIALIZED VIEW zone_area_prices;

COMMIT;

-- The two updates wrote the listings again: vacuuming frees their old versions' space.
VACUUM (ANALYZE) listings;

SELECT format('Put %s listings in their zones.', count(*)) AS done FROM zoned \gset
\echo :done
\quit
\endif

\if :photos
-- Listings with only a cover, as this script made them before it gave more, and as
-- dev/sample_listings.sql and the tests' fixtures make them, get more of their category's photos.
-- One this script made whose cover isn't of its category's own (a Lorem Picsum one, or a near
-- category's, from before its category had photos) gets all new ones.
INSERT INTO galleries
SELECT m.listing_id, c.slug, pg_temp.number_between(2, 4), 1,
       substring(m.public_url FROM 'photo-[0-9]+-[0-9a-f]+'), m.storage_key
FROM listing_media m
JOIN listings l ON l.id = m.listing_id
JOIN categories c ON c.id = l.category_id
WHERE m.is_cover
  AND split_part(m.storage_key, '/', 1) IN ('generated', 'sample', 'unsplash')
  AND NOT EXISTS (SELECT 1 FROM listing_media other WHERE other.listing_id = m.listing_id AND NOT other.is_cover);

UPDATE galleries g SET first = 0, cover = NULL
WHERE g.storage_key LIKE 'generated/%'
  AND EXISTS (SELECT 1 FROM photos p WHERE p.category = g.category)
  AND NOT EXISTS (SELECT 1 FROM photos p WHERE p.category = g.category AND p.photo = g.cover);
DELETE FROM listing_media m USING galleries g WHERE g.first = 0 AND m.listing_id = g.id;

SELECT pg_temp.give_photos() \g /dev/null

COMMIT;

-- The new photos' rows, and the replaced covers' left behind.
VACUUM (ANALYZE) listing_media;

SELECT format('Gave %s listings %s photos, %s of them new covers.', count(*),
              coalesce(sum(photos - first), 0), count(*) FILTER (WHERE first = 0)) AS done
FROM galleries \gset
\echo :done
\quit
\endif

-- ---------------------------------------------------------------- cars

CREATE FUNCTION pg_temp.car() RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    brand record;
    kind record;
    this_year int := extract(year FROM now())::int;
    year int;
    km int;
    fuel text;
    gearbox text;
    body text;
    size text;
    engine text;
BEGIN
    SELECT * INTO brand FROM pg_temp.makes ORDER BY -ln(1 - random()) / weight LIMIT 1;
    SELECT * INTO kind FROM pg_temp.models WHERE make = brand.code ORDER BY random() LIMIT 1;
    -- Mostly 2008 to 2020, as the cars on Albanian roads are.
    year := least(this_year - 1, 2003 + floor((random() + random()) * 11.5)::int);
    fuel := CASE
        WHEN brand.code IN ('TESLA', 'BYD') OR kind.label ~ '^(i[34]|iX|e-tron|Q4 e-tron|EQ[A-Z]|ID\.|Model )' THEN 'ELECTRIC'
        ELSE pg_temp.pick(ARRAY['DIESEL', 'DIESEL', 'DIESEL', 'DIESEL', 'DIESEL', 'DIESEL', 'PETROL', 'PETROL', 'PETROL', 'HYBRID', 'LPG'])
    END;
    IF fuel = 'ELECTRIC' THEN year := greatest(year, 2016); END IF;
    IF fuel = 'HYBRID' THEN year := greatest(year, 2010); END IF;
    body := CASE
        WHEN kind.label ~* '^(X[1-7]|iX|Q[2-8]|GL[ABCEKS]?|ML|G-Class|EQ[ABC]|Tiguan|Touareg|T-Roc|T-Cross|Taigo|ID\.[45]|Sportage|Sorento|Tucson|Santa Fe|Kona|ix35|RAV4|C-HR|Land Cruiser|Highlander|Yaris Cross|CR-V|HR-V|Qashqai|X-Trail|Juke|Duster|Kuga|EcoSport|Puma|Range Rover|Discovery|Defender|Freelander|Cayenne|Macan|XC[0-9]+|Kodiaq|Karoq|Kamiq|Ateca|Arona|Captur|Kadjar|Koleos|[235]008|Mokka|Grandland|Crossland|Antara|Frontera|Compass|Wrangler|Cherokee|Grand Cherokee|Renegade|Vitara|Grand Vitara|Jimny|S-Cross|Forester|Outback|Model [XY]|Pajero|Outlander|ASX|CX-[0-9]+|Niro|Stonic|500X|Sedici|Amarok|Hilux|Navara|e-tron|Tivoli|Korando|Rexton|Captiva|Countryman)' THEN 'SUV'
        WHEN kind.label ~* '^(Sprinter|Vito|Viano|V-Class|Vaneo|Citan|Transporter|Multivan|Caddy|Crafter|Sharan|Touran|ID\. Buzz|Ducato|Doblò|Fiorino|Qubo|Scudo|Multipla|Berlingo|Jumper|Jumpy|Kangoo|Master|Trafic|Espace|Scenic|Partner|Boxer|Expert|Bipper|Rifter|807|Combo|Movano|Vivaro|Zafira|Meriva|Proace|Previa|Verso|Corolla Verso|Transit|Galaxy|S-Max|C-Max|Tourneo|Daily|Carens|Carnival|Alhambra|Lodgy|Dokker)' THEN 'VAN'
        WHEN kind.label ~* '^(TT|R8|Z[34]|SLK|SL|CLK|Scirocco|Corrado|GT86|Supra|Celica|RCZ|Eos|Brera|MX-5|F-Type|Calibra|Tigra|[468] Series)$' THEN pg_temp.pick(ARRAY['COUPE', 'COUPE', 'CONVERTIBLE'])
        ELSE pg_temp.pick(ARRAY['HATCHBACK', 'HATCHBACK', 'HATCHBACK', 'SEDAN', 'SEDAN', 'SEDAN', 'WAGON'])
    END;
    gearbox := CASE WHEN fuel IN ('ELECTRIC', 'HYBRID')
                         OR random() < 0.2 + (year - 2003) * 0.03 + CASE WHEN brand.new_price >= 50000 THEN 0.3 ELSE 0 END
                    THEN 'AUTOMATIC' ELSE 'MANUAL' END;
    size := CASE WHEN brand.new_price >= 50000 THEN pg_temp.pick(ARRAY['2.0', '2.0', '2.2', '3.0'])
                 ELSE pg_temp.pick(ARRAY['1.2', '1.4', '1.5', '1.6', '1.6', '1.9', '2.0', '2.0']) END;
    engine := CASE fuel
        WHEN 'ELECTRIC' THEN pg_temp.pick(ARRAY['', 'Long Range', 'Performance'])
        WHEN 'HYBRID' THEN size || ' Hybrid'
        WHEN 'LPG' THEN size || ' Benzinë + Gaz'
        WHEN 'DIESEL' THEN CASE
            WHEN brand.code = 'BMW' THEN size || 'd'
            ELSE size || ' ' || CASE
                WHEN brand.code IN ('VOLKSWAGEN', 'AUDI', 'SKODA', 'SEAT', 'PORSCHE') THEN 'TDI'
                WHEN brand.code = 'MERCEDES_BENZ' THEN 'CDI'
                WHEN brand.code = 'OPEL' THEN 'CDTI'
                WHEN brand.code IN ('FIAT', 'ALFA_ROMEO', 'LANCIA', 'JEEP') THEN 'Multijet'
                WHEN brand.code IN ('PEUGEOT', 'CITROEN') THEN 'HDi'
                WHEN brand.code IN ('RENAULT', 'DACIA', 'NISSAN') THEN 'dCi'
                WHEN brand.code = 'FORD' THEN 'TDCi'
                WHEN brand.code IN ('TOYOTA', 'LEXUS') THEN 'D-4D'
                WHEN brand.code IN ('HYUNDAI', 'KIA') THEN 'CRDi'
                WHEN brand.code = 'LAND_ROVER' THEN 'SD4'
                ELSE 'Diesel' END
            END
        ELSE CASE
            WHEN brand.code = 'BMW' THEN size || 'i'
            WHEN brand.code IN ('VOLKSWAGEN', 'SKODA', 'SEAT') THEN size || ' TSI'
            WHEN brand.code = 'AUDI' THEN size || ' TFSI'
            ELSE size END
    END;
    -- About 15 000 km a year; a car from last year may be all but new.
    km := CASE WHEN year = this_year - 1 AND random() < 0.3 THEN pg_temp.number_between(0, 15) * 1000
               ELSE greatest(1000, round(((this_year - year) * pg_temp.number_between(9000, 21000))::numeric, -3)::int) END;
    w.condition := CASE WHEN km = 0 THEN 'NEW' WHEN random() < 0.01 THEN 'FOR_PARTS' ELSE 'USED' END;
    w.price := greatest(800, pg_temp.round_price(brand.new_price * power(0.86, this_year - year) * (0.85 + random() * 0.3)
                                                 * CASE WHEN fuel IN ('ELECTRIC', 'HYBRID') THEN 1.1 ELSE 1 END
                                                 * CASE w.condition WHEN 'FOR_PARTS' THEN 0.3 ELSE 1 END));
    w.properties := jsonb_build_object(
        'vehicle_make', brand.code, 'vehicle_model', kind.code, 'vehicle_year', year, 'vehicle_mileage', km,
        'vehicle_fuel', fuel, 'vehicle_transmission', gearbox, 'vehicle_body_type', body,
        'vehicle_color', pg_temp.pick(ARRAY['BLACK', 'BLACK', 'BLACK', 'BLACK', 'BLACK', 'WHITE', 'WHITE', 'WHITE', 'WHITE', 'GREY', 'GREY', 'GREY', 'GREY', 'SILVER', 'SILVER', 'SILVER', 'BLUE', 'BLUE', 'RED', 'OTHER']),
        'vehicle_features', to_jsonb(array_remove(ARRAY[
            CASE WHEN random() < 0.92 THEN 'ABS' END,
            CASE WHEN random() < 0.88 THEN 'AIR_CONDITIONING' END,
            CASE WHEN year >= 2011 AND random() < 0.5 THEN 'NAVIGATION' END,
            CASE WHEN year >= 2009 AND random() < 0.45 THEN 'PARKING_SENSORS' END], NULL)));
    IF random() < 0.04 THEN
        w.listing_type := 'BUY';
        w.title := format('Blej %s %s, viti %s e lart', brand.label, kind.label, year);
        w.description := pg_temp.sentences(ARRAY[
            'Kërkoj të blej një makinë në gjendje të mirë.', 'Paguaj menjëherë, para në dorë.',
            'Preferohet me doganë të paguar.', 'Jo makina të aksidentuara.',
            'Mund të vij ta shoh në çdo qytet.', 'Ofertat me foto në mesazh.'], 3);
        w.price_type := 'NEGOTIABLE';
    ELSE
        w.listing_type := 'SELL';
        w.title := concat_ws(' ', brand.label, kind.label, nullif(engine, '')) || ', ' || year
                   || CASE w.condition WHEN 'FOR_PARTS' THEN ', për pjesë' ELSE '' END;
        w.description := pg_temp.sentences(ARRAY[
            'Makina është në gjendje shumë të mirë.', 'Makina është në gjendje perfekte.',
            'E mirëmbajtur rregullisht, gati për t''u përdorur.', 'Kilometrazh real, i dokumentuar.',
            'Dogana e paguar, me targa shqiptare.', 'Pronari i parë në Shqipëri.', 'Pa asnjë aksident.',
            'Goma të reja dimri.', 'Goma të reja vere.', 'Servisi i fundit u bë para dy muajsh.',
            'Sapo i janë ndërruar vaji dhe filtrat.', 'Mundësi ndërrimi me makinë më të vogël.',
            'Nuk ka nevojë për asnjë investim.', 'Konsum i ulët karburanti.',
            'Siguracioni dhe kolaudimi të rinj.', 'Faturat e servisit në dispozicion.',
            format('Importuar nga %s.', pg_temp.pick(ARRAY['Italia', 'Gjermania', 'Zvicra', 'Belgjika', 'Franca', 'Holanda']))],
            pg_temp.number_between(2, 4))
            || format(' Kambio %s, %s.', CASE gearbox WHEN 'AUTOMATIC' THEN 'automatike' ELSE 'manuale' END,
                      format_attribute_number(km, 'km'));
        w.price_type := pg_temp.pick(ARRAY['FIXED', 'FIXED', 'NEGOTIABLE']);
    END IF;
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- motorcycles and vans

-- "name|price new, in euros".
CREATE FUNCTION pg_temp.vehicle(category text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    this_year int := extract(year FROM now())::int;
    item text := pg_temp.pick(CASE category
        WHEN 'motocikleta' THEN ARRAY[
            'Honda SH 150|4500', 'Honda CBR 600RR|13000', 'Yamaha MT-07|8000', 'Yamaha TMAX 560|13000',
            'Kawasaki Ninja 650|8500', 'Vespa Primavera 125|5000', 'Piaggio Liberty 125|3200',
            'Suzuki GSX-R 750|13000', 'BMW R 1250 GS|22000', 'KTM 390 Duke|6000', 'Aprilia SR 50|2000',
            'Ducati Monster 821|13000', 'Harley-Davidson Sportster 883|12000', 'Kymco Agility 125|2200',
            'Honda Africa Twin|16000', 'Yamaha NMAX 125|3800']
        ELSE ARRAY[
            'Mercedes-Benz Sprinter 313 CDI|42000', 'Fiat Ducato 2.3 Multijet|33000', 'Ford Transit 2.2 TDCi|35000',
            'Iveco Daily 35C15|40000', 'Volkswagen Transporter T6 2.0 TDI|42000', 'Renault Master 2.3 dCi|33000',
            'Opel Vivaro 1.6 CDTI|30000', 'Peugeot Boxer 2.2 HDi|31000', 'Citroën Jumper 2.0 BlueHDi|31000',
            'Mercedes-Benz Vito 111 CDI|38000', 'Volkswagen Caddy 2.0 TDI|26000', 'Fiat Doblò 1.3 Multijet|19000',
            'Mercedes-Benz Atego 818|60000', 'Isuzu D-Max|32000']
    END);
    year int := least(this_year - 1, 2004 + floor((random() + random()) * 10.5)::int);
    km int := greatest(1000, round(((this_year - year) * CASE category WHEN 'motocikleta' THEN pg_temp.number_between(2000, 6000)
                                                                       ELSE pg_temp.number_between(15000, 35000) END)::numeric, -3)::int);
BEGIN
    w.listing_type := 'SELL';
    w.condition := 'USED';
    w.title := split_part(item, '|', 1) || ', ' || year;
    w.price := greatest(500, pg_temp.round_price(split_part(item, '|', 2)::numeric * power(0.88, this_year - year) * (0.85 + random() * 0.3)));
    w.price_type := pg_temp.pick(ARRAY['FIXED', 'NEGOTIABLE']);
    w.description := pg_temp.sentences(ARRAY[
        'E mirëmbajtur rregullisht.', 'Dokumentet në rregull.', 'Dogana e paguar, me targa shqiptare.',
        'Goma të reja.', 'Servis i plotë para një muaji.', 'Pa asnjë problem teknik.',
        'Mundësi ndërrimi.', 'Gati për rrugë.'], pg_temp.number_between(2, 3))
        || ' ' || format_attribute_number(km, 'km') || '.';
    w.properties := '{}';
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- car parts and accessories

-- "name|price|what it fits": a make and model, a make, or anything. Prices in the category's
-- currency (lekë, euros for trailers and chargers).
CREATE FUNCTION pg_temp.part(category text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    item text := pg_temp.pick(CASE category
        WHEN 'pjese-kembimi' THEN ARRAY[
            'Motor|180000|model', 'Kambio|90000|model', 'Far i përparmë|18000|model', 'Stop i pasmë|9000|model',
            'Parakolp i përparmë|15000|model', 'Pasqyrë anësore|6000|model', 'Turbinë|45000|model',
            'Amortizatorë|12000|model', 'Disqe dhe pastiglia frenash|8000|model', 'Radiator|10000|model',
            'Alternator|14000|model', 'Motorino|11000|model', 'Derë e përparme|20000|model', 'Kapak motori|16000|model']
        WHEN 'goma-dhe-disqe' THEN ARRAY[
            'Goma 205/55 R16 Michelin, verore|28000|any', 'Goma 225/45 R17 Continental, dimërore|38000|any',
            'Goma 195/65 R15 Pirelli, 4 stinë|24000|any', 'Goma 235/55 R18 Bridgestone, verore|52000|any',
            'Disqe alumini R16|35000|make', 'Disqe alumini R17|45000|make', 'Disqe alumini R18 me goma|90000|make']
        WHEN 'audio-dhe-video' THEN ARRAY[
            'Radio Android 9" me GPS|18000|model', 'Kamerë parkimi|4000|any', 'Altoparlantë JBL|9000|any',
            'Ekran multimedial 10"|25000|model', 'Amplifikator Pioneer|15000|any']
        WHEN 'gps-dhe-navigatore' THEN ARRAY[
            'GPS Garmin DriveSmart 65|19000|any', 'Navigator TomTom GO|15000|any', 'Kamerë makine 4K|9000|any']
        WHEN 'styling-dhe-pajisje-sportive' THEN ARRAY[
            'Spoiler i pasmë|14000|model', 'Grila sportive|9000|model', 'Mbajtëse çatie|12000|make',
            'Kit karroceri|60000|model', 'Pragje anësore|11000|model']
        WHEN 'tapete-makine' THEN ARRAY[
            'Tapete gome|3500|model', 'Tapete stofi të personalizuara|5500|model', 'Tapet bagazhi|3000|model']
        WHEN 'rimorkio' THEN ARRAY[
            'Rimorkio 750 kg|900|any', 'Rimorkio 1 300 kg me frena|1800|any', 'Rimorkio për varkë|1500|any',
            'Rimorkio për makinë|2500|any']
        WHEN 'mbushes-per-makina-elektrike' THEN ARRAY[
            'Karikues shtëpie 11 kW Type 2|650|any', 'Karikues shtëpie 22 kW|900|any',
            'Kabllo karikimi Type 2, 5 m|180|any', 'Karikues portativ 3.7 kW|350|any']
        ELSE ARRAY[
            'Kuti veglash|6000|any', 'Mbajtëse biçikletash për makinë|9000|make', 'Kanoçe karburanti 20 L|1500|any',
            'Kriko hidraulike 2 t|5000|any', 'Kabllo ndezjeje|2000|any']
    END);
    fits text := split_part(item, '|', 3);
    brand record;
    kind record;
    from_year int := pg_temp.number_between(1998, 2016);
    to_year int := least(from_year + pg_temp.number_between(3, 8), extract(year FROM now())::int);
BEGIN
    SELECT * INTO brand FROM pg_temp.makes ORDER BY -ln(1 - random()) / weight LIMIT 1;
    SELECT * INTO kind FROM pg_temp.models WHERE make = brand.code ORDER BY random() LIMIT 1;
    w.listing_type := pg_temp.pick(ARRAY['SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'BUY']);
    w.condition := pg_temp.pick(ARRAY['NEW', 'NEW', 'NEW', 'NEW', 'USED', 'USED', 'USED', 'USED', 'USED', 'FOR_PARTS']);
    w.price := pg_temp.round_price(split_part(item, '|', 2)::numeric * pg_temp.worth(w.condition) * (0.8 + random() * 0.4));
    w.price_type := pg_temp.pick(ARRAY['FIXED', 'FIXED', 'NEGOTIABLE']);
    w.title := CASE w.listing_type WHEN 'BUY' THEN pg_temp.wanted(split_part(item, '|', 1)) ELSE split_part(item, '|', 1) END
               || CASE fits WHEN 'model' THEN format(' për %s %s, %s–%s', brand.label, kind.label, from_year, to_year)
                            WHEN 'make' THEN ' për ' || brand.label
                            ELSE '' END;
    w.properties := CASE fits
        WHEN 'model' THEN jsonb_build_object('vehicle_make', brand.code, 'vehicle_model', kind.code,
                                             'fits_from_year', from_year, 'fits_to_year', to_year)
        WHEN 'make' THEN jsonb_build_object('vehicle_make', brand.code)
        ELSE '{}' END;
    w.description := pg_temp.sentences(ARRAY[
        'Origjinale, jo imitim.', 'E testuar, punon perfekt.', 'Montimi mund të bëhet në servisin tonë.',
        'Dërgojmë me postë në gjithë Shqipërinë.', 'Garanci 3 muaj.', 'E hequr nga makinë e importuar.',
        'Për çdo pyetje më shkruani.', 'Çmim i diskutueshëm për disa copë.'], 2);
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- property

CREATE FUNCTION pg_temp.property(category text, town_slug text, town text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    -- The town's neighbourhoods people name in listings.
    place text := pg_temp.pick(CASE town_slug
        WHEN 'tirane' THEN ARRAY['Blloku', 'Komuna e Parisit', 'Don Bosko', 'Ali Demi', 'Fresku', 'Kombinat', 'Laprakë',
                                 'Selitë', 'Yzberisht', 'Astir', '21 Dhjetori', 'Rruga e Kavajës', 'Myslym Shyri',
                                 'Pazari i Ri', 'Sauk', 'Lundër', 'Kodra e Diellit', 'Unaza e Re', 'Tirana e Re', 'Xhamlliku']
        WHEN 'durres' THEN ARRAY['Plazh', 'Shkëmbi i Kavajës', 'Currila', 'Qendër', 'Ish-Kënetë', 'Golem']
        WHEN 'vlore' THEN ARRAY['Lungomare', 'Uji i Ftohtë', 'Skelë', 'Qendër', 'Radhimë']
        WHEN 'sarande' THEN ARRAY['Qendër', 'Kodër', 'Shirokë']
        WHEN 'shkoder' THEN ARRAY['Qendër', 'Rus', 'Parruca']
        WHEN 'korce' THEN ARRAY['Qendër', 'Pazari i Vjetër', 'Lagjja 5']
        ELSE ARRAY['Qendër', 'Lagjja e Re', 'pranë stadiumit', 'pranë bashkisë']
    END) || ', ' || town;
    coast boolean := town_slug IN ('durres', 'vlore', 'sarande', 'himare', 'kavaje', 'lezhe', 'konispol', 'delvine');
    -- Euros a square metre, and what rents are here against Tirana's.
    per_m2 int := CASE
        WHEN town_slug = 'tirane' THEN pg_temp.number_between(1500, 2600)
        WHEN town_slug IN ('sarande', 'himare') THEN pg_temp.number_between(1400, 2400)
        WHEN town_slug IN ('durres', 'vlore') THEN pg_temp.number_between(1100, 2000)
        WHEN town_slug IN ('shkoder', 'korce', 'elbasan', 'fier', 'berat', 'pogradec', 'kamez', 'lezhe', 'gjirokaster', 'kavaje')
            THEN pg_temp.number_between(700, 1200)
        ELSE pg_temp.number_between(450, 900) END;
    rents numeric := CASE WHEN town_slug = 'tirane' THEN 1 WHEN coast THEN 0.75 ELSE 0.5 END;
    kind text := CASE category
        WHEN 'apartamente' THEN 'APARTMENT' WHEN 'garsoniera' THEN 'STUDIO' WHEN 'vila-dhe-shtepi' THEN 'HOUSE'
        WHEN 'dyqane' THEN 'SHOP' WHEN 'zyra' THEN 'OFFICE' WHEN 'truall' THEN 'LAND' ELSE 'GARAGE' END;
    rooms text;
    -- The rooms as a title says them: the filter's "4+1 e më shumë" is a 4+1, 4+2 or 5+1.
    layout text;
    area int;
    floor int;
    furnished text;
    features text[];
    said text[];
BEGIN
    w.listing_type := CASE kind
        WHEN 'LAND' THEN pg_temp.pick(ARRAY['SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'RENT', 'WANTED_RENT'])
        ELSE pg_temp.pick(ARRAY['SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'RENT', 'RENT', 'RENT', 'RENT', 'WANTED_RENT']) END;
    rooms := CASE WHEN kind = 'APARTMENT' THEN pg_temp.pick(ARRAY['R11', 'R11', 'R21', 'R21', 'R21', 'R21', 'R31', 'R31', 'R41'])
                  WHEN kind = 'HOUSE' THEN pg_temp.pick(ARRAY['R31', 'R41', 'R41']) END;
    layout := CASE rooms WHEN 'R11' THEN '1+1' WHEN 'R21' THEN '2+1' WHEN 'R31' THEN '3+1'
                         ELSE pg_temp.pick(ARRAY['4+1', '4+1', '4+2', '5+1']) END;
    area := CASE
        WHEN kind = 'APARTMENT' THEN CASE rooms WHEN 'R11' THEN pg_temp.number_between(45, 75) WHEN 'R21' THEN pg_temp.number_between(70, 120)
                                                WHEN 'R31' THEN pg_temp.number_between(100, 160) ELSE pg_temp.number_between(140, 250) END
        WHEN kind = 'STUDIO' THEN pg_temp.number_between(25, 45)
        WHEN kind = 'HOUSE' THEN pg_temp.number_between(120, 400)
        WHEN kind = 'SHOP' THEN pg_temp.number_between(30, 250)
        WHEN kind = 'OFFICE' THEN pg_temp.number_between(40, 300)
        WHEN kind = 'LAND' THEN pg_temp.number_between(2, 50) * 100
        ELSE pg_temp.number_between(12, 30) END;
    floor := CASE WHEN kind IN ('APARTMENT', 'STUDIO', 'OFFICE') THEN pg_temp.number_between(0, 12)
                  WHEN kind = 'SHOP' THEN 0
                  WHEN kind = 'GARAGE' THEN pg_temp.pick(ARRAY[-1, -1, 0]) END;
    furnished := CASE WHEN kind IN ('APARTMENT', 'STUDIO', 'HOUSE', 'OFFICE') THEN
                     CASE WHEN random() < CASE w.listing_type WHEN 'SELL' THEN 0.3 ELSE 0.65 END THEN 'FURNISHED'
                          ELSE pg_temp.pick(ARRAY['PARTLY', 'UNFURNISHED', 'UNFURNISHED']) END END;
    features := array_remove(ARRAY[
        CASE WHEN floor >= 3 AND random() < 0.85 THEN 'ELEVATOR' END,
        CASE WHEN random() < CASE kind WHEN 'HOUSE' THEN 0.7 ELSE 0.35 END AND kind NOT IN ('LAND', 'GARAGE') THEN 'PARKING' END,
        CASE WHEN kind IN ('APARTMENT', 'STUDIO') AND random() < 0.75 THEN 'BALCONY' END,
        CASE WHEN coast AND kind NOT IN ('GARAGE', 'SHOP') AND random() < 0.4 THEN 'SEA_VIEW' END,
        CASE WHEN w.listing_type = 'SELL' AND random() < 0.85 THEN 'TITLE_DEED' END], NULL);
    w.properties := jsonb_strip_nulls(jsonb_build_object(
        'property_rooms', rooms, 'property_area', area, 'property_floor', floor,
        'property_furnished', furnished, 'property_features', to_jsonb(features),
        -- Central heating in a fifth of the buildings.
        'property_heating', CASE WHEN kind NOT IN ('LAND', 'GARAGE') AND random() < 0.2 THEN 'CENTRAL' END));

    w.price := pg_temp.round_price(CASE w.listing_type
        WHEN 'SELL' THEN CASE kind
            WHEN 'LAND' THEN area * CASE WHEN town_slug = 'tirane' THEN pg_temp.number_between(150, 500)
                                         WHEN coast THEN pg_temp.number_between(60, 300) ELSE pg_temp.number_between(10, 80) END
            WHEN 'GARAGE' THEN pg_temp.number_between(8, 25) * 1000
            ELSE area * per_m2 * CASE kind WHEN 'HOUSE' THEN 0.8 WHEN 'SHOP' THEN 1.3 WHEN 'OFFICE' THEN 1.2 WHEN 'STUDIO' THEN 1.1 ELSE 1 END
        END
        -- A month's rent, or what a seeker means to pay.
        ELSE CASE kind
            WHEN 'APARTMENT' THEN CASE rooms WHEN 'R11' THEN 380 WHEN 'R21' THEN 520 WHEN 'R31' THEN 750 ELSE 1100 END
            WHEN 'STUDIO' THEN 300 WHEN 'HOUSE' THEN 900 WHEN 'SHOP' THEN area * 12 WHEN 'OFFICE' THEN area * 10
            WHEN 'LAND' THEN 300 ELSE 80 END
            * rents * (0.8 + random() * 0.45) * CASE furnished WHEN 'FURNISHED' THEN 1.1 ELSE 1 END
            * CASE w.listing_type WHEN 'WANTED_RENT' THEN 0.85 ELSE 1 END
    END);
    w.price_type := CASE w.listing_type WHEN 'WANTED_RENT' THEN 'NEGOTIABLE' ELSE pg_temp.pick(ARRAY['FIXED', 'FIXED', 'NEGOTIABLE']) END;

    w.title := CASE w.listing_type
        WHEN 'SELL' THEN CASE kind
            WHEN 'APARTMENT' THEN format(pg_temp.pick(ARRAY['Shitet apartament %s, %s m², %s', 'Shes apartament %s, %s m², %s',
                                                           'Apartament %s në shitje, %s m², %s']),
                                         layout, area, place)
            WHEN 'STUDIO' THEN format('Shitet garsonierë %s m², %s', area, place)
            WHEN 'HOUSE' THEN format(pg_temp.pick(ARRAY['Shitet vilë %s me oborr, %s', 'Shitet shtëpi private %s, %s']),
                                     pg_temp.pick(ARRAY['dykatëshe', 'dykatëshe', 'trekatëshe', 'njëkatëshe']), place)
            WHEN 'SHOP' THEN format('Shitet dyqan %s m², %s', area, place)
            WHEN 'OFFICE' THEN format('Shitet zyrë %s m², %s', area, place)
            WHEN 'LAND' THEN format(pg_temp.pick(ARRAY['Shitet truall %s m², %s', 'Shitet tokë %s m², %s']), area, place)
            ELSE format('Shitet garazh %s m², %s', area, place) END
        WHEN 'RENT' THEN CASE kind
            WHEN 'APARTMENT' THEN format('Jepet me qira apartament %s%s, %s', layout,
                                        CASE furnished WHEN 'FURNISHED' THEN ' i mobiluar' ELSE '' END, place)
            WHEN 'STUDIO' THEN format('Jepet me qira garsonierë%s, %s', CASE furnished WHEN 'FURNISHED' THEN ' e mobiluar' ELSE '' END, place)
            WHEN 'HOUSE' THEN format('Jepet me qira vilë, %s', place)
            WHEN 'SHOP' THEN format('Jepet me qira ambient biznesi %s m², %s', area, place)
            WHEN 'OFFICE' THEN format('Jepet me qira zyrë %s m², %s', area, place)
            WHEN 'LAND' THEN format('Jepet me qira tokë bujqësore %s m², %s', area, place)
            ELSE format('Jepet me qira garazh, %s', place) END
        ELSE CASE kind
            WHEN 'APARTMENT' THEN format('Kërkoj me qira apartament %s në %s', layout, town)
            WHEN 'STUDIO' THEN format('Kërkoj garsonierë me qira në %s', town)
            WHEN 'HOUSE' THEN format('Kërkoj shtëpi me qira në %s', town)
            WHEN 'SHOP' THEN format('Kërkoj ambient biznesi me qira në %s', town)
            WHEN 'OFFICE' THEN format('Kërkoj zyrë me qira në %s', town)
            WHEN 'LAND' THEN format('Kërkoj tokë me qira në %s', town)
            ELSE format('Kërkoj garazh me qira në %s', town) END
    END;

    said := CASE kind
        WHEN 'APARTMENT' THEN ARRAY['Pranë shkollës, marketit dhe stacionit të autobusit.', 'Ngrohje qendrore dhe kondicioner në çdo dhomë.',
                                    'I sapo rikonstruktuar, me materiale cilësore.', 'Kuzhina e ndarë nga dhoma e ndenjes.',
                                    'Ndriçim i mirë natyral gjatë gjithë ditës.', 'Pallat i ri, me ashensor dhe hyrje të pastër.',
                                    'Ballkon i madh me pamje të hapur.', 'Shikimi me takim paraprak.']
        WHEN 'STUDIO' THEN ARRAY['Ideale për studentë ose çift të ri.', 'Kuzhinë e pajisur plotësisht.', 'Në zonë të qetë, pranë qendrës.',
                                 'Shikimi me takim paraprak.']
        WHEN 'HOUSE' THEN ARRAY['Oborr i madh me pemë frutore.', 'Garazh për dy makina.', 'Ndodhet në zonë të qetë banimi.',
                                'Ngrohje me dru dhe kondicionerë.', 'Mundësi për të ndërtuar edhe një kat.', 'Pus uji në oborr.']
        WHEN 'SHOP' THEN ARRAY['Vitrinë e madhe në rrugë kryesore.', 'Ideal për bar-kafe, dyqan ose zyrë.', 'Kalim i madh këmbësorësh.',
                               'Me tualet dhe magazinë.']
        WHEN 'OFFICE' THEN ARRAY['Ambient i ndriçuar, me ndarje xhami.', 'Me internet të shpejtë dhe kondicioner.', 'Parkim për punonjësit.',
                                 'Ideal për studio ose kompani të vogël.']
        WHEN 'LAND' THEN ARRAY['Me leje ndërtimi për pallat.', 'Ka dalje në rrugë të asfaltuar.', 'Me ujë dhe energji elektrike pranë.',
                               'Tokë pjellore, e përshtatshme për bujqësi.', 'Me hipotekë.']
        ELSE ARRAY['Me derë automatike.', 'Në katin nëntokë të pallatit.', 'I përshtatshëm edhe si magazinë.'] END;
    w.description := concat_ws(' ',
        CASE w.listing_type
            WHEN 'RENT' THEN pg_temp.pick(ARRAY['Qiraja paguhet çdo muaj, depozita një muaj.', 'I disponueshëm menjëherë.',
                                                'Shpenzimet e pallatit përfshihen në qira.', 'Preferohen familje ose studentë seriozë.'])
            WHEN 'WANTED_RENT' THEN pg_temp.pick(ARRAY['Jemi familje e qetë, pa kafshë shtëpiake.', 'Paguaj disa muaj përpara.',
                                                       'Për një periudhë të gjatë.'])
            ELSE CASE WHEN 'TITLE_DEED' = ANY (features) THEN 'Me hipotekë, gati për kalim pronësie.' END END,
        CASE WHEN floor > 0 THEN format('Kati %s nga %s.', floor, floor + pg_temp.number_between(1, 6))
             WHEN floor = 0 AND kind <> 'SHOP' THEN 'Kati përdhes.' END,
        pg_temp.sentences(said, 2));
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- electronics

-- "category|brand|model|price in lekë, new|storages in GB|…", then what that kind of thing has
-- (migrations/0021_electronics_properties.sql), a/b where it varies: a phone's model, RAM and
-- 5G (1 or 0); a laptop's processor, RAM, screen in inches, graphics and system; a desktop's kind,
-- processor, RAM, graphics and system; a tablet's screen; a TV's size, resolution and panel; a
-- console's model and edition; a camera's kind, sensor and lens mount; audio's kind, wireless and
-- noise cancelling (1 or 0).
CREATE FUNCTION pg_temp.electronics(category text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    item text := pg_temp.pick(ARRAY(SELECT i FROM unnest(ARRAY[
        'telefona|APPLE|iPhone 11|38000|64,128|APPLE_IPHONE_11|GB4|0',
        'telefona|APPLE|iPhone 12|48000|64,128,256|APPLE_IPHONE_12|GB4|1',
        'telefona|APPLE|iPhone 13|58000|128,256|APPLE_IPHONE_13|GB4|1',
        'telefona|APPLE|iPhone 13 Pro|72000|128,256,512|APPLE_IPHONE_13_PRO|GB6|1',
        'telefona|APPLE|iPhone 14|72000|128,256|APPLE_IPHONE_14|GB6|1',
        'telefona|APPLE|iPhone 14 Pro Max|105000|128,256,512|APPLE_IPHONE_14_PRO_MAX|GB6|1',
        'telefona|APPLE|iPhone 15|92000|128,256|APPLE_IPHONE_15|GB6|1',
        'telefona|APPLE|iPhone 15 Pro|118000|128,256,512|APPLE_IPHONE_15_PRO|GB8|1',
        'telefona|APPLE|iPhone 16|112000|128,256|APPLE_IPHONE_16|GB8|1',
        'telefona|APPLE|iPhone 16 Pro Max|155000|256,512,1000|APPLE_IPHONE_16_PRO_MAX|GB8|1',
        'telefona|APPLE|iPhone 17 Pro|160000|256,512,1000|APPLE_IPHONE_17_PRO|GB12|1',
        'telefona|SAMSUNG|Galaxy S21|38000|128,256|SAMSUNG_GALAXY_S21|GB8|1',
        'telefona|SAMSUNG|Galaxy S22 Ultra|62000|128,256,512|SAMSUNG_GALAXY_S22_ULTRA|GB8/GB12|1',
        'telefona|SAMSUNG|Galaxy S23|62000|128,256|SAMSUNG_GALAXY_S23|GB8|1',
        'telefona|SAMSUNG|Galaxy S24 Ultra|115000|256,512|SAMSUNG_GALAXY_S24_ULTRA|GB12|1',
        'telefona|SAMSUNG|Galaxy S25 Ultra|135000|256,512,1000|SAMSUNG_GALAXY_S25_ULTRA|GB12|1',
        'telefona|SAMSUNG|Galaxy A54|32000|128,256|SAMSUNG_GALAXY_A54|GB6/GB8|1',
        'telefona|SAMSUNG|Galaxy A34|24000|128|SAMSUNG_GALAXY_A34|GB6/GB8|1',
        'telefona|SAMSUNG|Galaxy A15|15000|64,128|SAMSUNG_GALAXY_A15|GB4/GB6|0',
        'telefona|SAMSUNG|Galaxy Z Flip5|75000|256,512|SAMSUNG_GALAXY_Z_FLIP5|GB8|1',
        'telefona|XIAOMI|Redmi Note 12|18000|64,128|XIAOMI_REDMI_NOTE_12|GB4/GB6/GB8|0',
        'telefona|XIAOMI|Redmi Note 13 Pro|26000|128,256|XIAOMI_REDMI_NOTE_13_PRO|GB8/GB12|1',
        'telefona|XIAOMI|Xiaomi 13T|42000|256|XIAOMI_13T|GB8/GB12|1',
        'telefona|XIAOMI|Poco X6 Pro|30000|256,512|XIAOMI_POCO_X6_PRO|GB8/GB12|1',
        'telefona|HUAWEI|Huawei P30 Pro|22000|128,256|HUAWEI_P30_PRO|GB8|0',
        'telefona|GOOGLE|Google Pixel 8|58000|128,256|GOOGLE_PIXEL_8|GB8|1',
        'telefona|GOOGLE|Google Pixel 9|78000|128,256|GOOGLE_PIXEL_9|GB12|1',
        'telefona|ONEPLUS|OnePlus 11|48000|128,256|ONEPLUS_11|GB8/GB16|1',
        'laptope|APPLE|MacBook Air M1|68000|256,512|APPLE_M1|GB8/GB16|13.3|INTEGRATED|MACOS',
        'laptope|APPLE|MacBook Air M2|98000|256,512|APPLE_M2|GB8/GB16/GB24|13.6|INTEGRATED|MACOS',
        'laptope|APPLE|MacBook Air M4|125000|256,512,1000|APPLE_M4|GB16/GB24|13.6|INTEGRATED|MACOS',
        'laptope|APPLE|MacBook Pro 14" M3|185000|512,1000|APPLE_M3|GB8/GB16/GB24|14.2|INTEGRATED|MACOS',
        'laptope|LENOVO|ThinkPad T14|62000|256,512|INTEL_CORE_I5/INTEL_CORE_I7/AMD_RYZEN_5/AMD_RYZEN_7|GB8/GB16/GB32|14|INTEGRATED|WINDOWS',
        'laptope|LENOVO|IdeaPad 5|46000|512|INTEL_CORE_I5/INTEL_CORE_I7/AMD_RYZEN_5/AMD_RYZEN_7|GB8/GB16|14|INTEGRATED|WINDOWS',
        'laptope|LENOVO|Legion 5|115000|512,1000|AMD_RYZEN_7/INTEL_CORE_I7|GB16/GB32|15.6|NVIDIA|WINDOWS',
        'laptope|DELL|XPS 13|92000|512,1000|INTEL_CORE_I5/INTEL_CORE_I7/INTEL_CORE_ULTRA_7|GB8/GB16/GB32|13.4|INTEGRATED|WINDOWS',
        'laptope|DELL|Latitude 7420|56000|256,512|INTEL_CORE_I5/INTEL_CORE_I7|GB8/GB16|14|INTEGRATED|WINDOWS',
        'laptope|DELL|Inspiron 15|42000|256,512|INTEL_CORE_I3/INTEL_CORE_I5/INTEL_CORE_I7|GB8/GB16|15.6|INTEGRATED|WINDOWS',
        'laptope|HP|EliteBook 840 G8|57000|256,512|INTEL_CORE_I5/INTEL_CORE_I7|GB8/GB16/GB32|14|INTEGRATED|WINDOWS',
        'laptope|HP|Pavilion 15|42000|512|INTEL_CORE_I5/INTEL_CORE_I7/AMD_RYZEN_5|GB8/GB16|15.6|INTEGRATED|WINDOWS',
        'laptope|HP|Victus 16|88000|512,1000|INTEL_CORE_I5/INTEL_CORE_I7/AMD_RYZEN_5/AMD_RYZEN_7|GB16/GB32|16.1|NVIDIA|WINDOWS',
        'laptope|ASUS|ZenBook 14|72000|512|INTEL_CORE_I5/INTEL_CORE_I7/INTEL_CORE_ULTRA_7/AMD_RYZEN_7|GB16/GB32|14|INTEGRATED|WINDOWS',
        'laptope|ASUS|ROG Strix G15|125000|512,1000|AMD_RYZEN_7/AMD_RYZEN_9/INTEL_CORE_I7|GB16/GB32|15.6|NVIDIA|WINDOWS',
        'laptope|ASUS|VivoBook 15|36000|256,512|INTEL_CORE_I3/INTEL_CORE_I5/AMD_RYZEN_5|GB8/GB16|15.6|INTEGRATED|WINDOWS',
        'laptope|ACER|Aspire 5|38000|256,512|INTEL_CORE_I5/INTEL_CORE_I7/AMD_RYZEN_5|GB8/GB16|15.6|INTEGRATED|WINDOWS',
        'laptope|MSI|Katana 15|92000|512,1000|INTEL_CORE_I7|GB16/GB32|15.6|NVIDIA|WINDOWS',
        'laptope|MICROSOFT|Surface Laptop 5|92000|256,512|INTEL_CORE_I5/INTEL_CORE_I7|GB8/GB16|13.5|INTEGRATED|WINDOWS',
        'kompjutere|BUILT|PC Gaming RTX 3060, Ryzen 5 5600|92000|512,1000|PC_GAMING|AMD_RYZEN_5|GB16/GB32|NVIDIA|WINDOWS',
        'kompjutere|BUILT|PC Gaming RTX 4070, Intel i7|185000|1000,2000|PC_GAMING|INTEL_CORE_I7|GB32/GB64|NVIDIA|WINDOWS',
        'kompjutere|DELL|OptiPlex 7080, Intel i5, 16 GB RAM|36000|256,512|PC_DESKTOP|INTEL_CORE_I5|GB16|INTEGRATED|WINDOWS',
        'kompjutere|HP|EliteDesk 800 G5|32000|256,512|PC_DESKTOP|INTEL_CORE_I5/INTEL_CORE_I7|GB8/GB16|INTEGRATED|WINDOWS',
        'kompjutere|APPLE|iMac 24" M1|112000|256,512|ALL_IN_ONE|APPLE_M1|GB8/GB16|INTEGRATED|MACOS',
        'kompjutere|APPLE|Mac mini M2|68000|256,512|MINI_PC|APPLE_M2|GB8/GB16|INTEGRATED|MACOS',
        'kompjutere|LENOVO|ThinkCentre M720|26000|256|PC_DESKTOP|INTEL_CORE_I3/INTEL_CORE_I5|GB8/GB16|INTEGRATED|WINDOWS',
        'tableta|APPLE|iPad 9|32000|64,256|10.2', 'tableta|APPLE|iPad Air 5|62000|64,256|10.9',
        'tableta|APPLE|iPad Pro 11" M2|98000|128,256,512|11', 'tableta|SAMSUNG|Galaxy Tab S8|56000|128,256|11',
        'tableta|SAMSUNG|Galaxy Tab A8|19000|64,128|10.5', 'tableta|XIAOMI|Xiaomi Pad 6|32000|128,256|11',
        'tableta|LENOVO|Tab P11|23000|64,128|11',
        'televizore|SAMSUNG|Smart TV 55" 4K Crystal UHD|46000||55|UHD_4K|LED', 'televizore|SAMSUNG|QLED 65" Q70|98000||65|UHD_4K|QLED',
        'televizore|SAMSUNG|Smart TV 43" Full HD|26000||43|FULL_HD|LED', 'televizore|LG|OLED 55" C2|125000||55|UHD_4K|OLED',
        'televizore|LG|Smart TV 50" 4K UHD|42000||50|UHD_4K|LED', 'televizore|SONY|Bravia 55" 4K|72000||55|UHD_4K|LED',
        'televizore|TCL|TCL 55" 4K|36000||55|UHD_4K|LED', 'televizore|HISENSE|Hisense 65" 4K|56000||65|UHD_4K|LED',
        'televizore|PHILIPS|Philips 32" HD|15000||32|HD|LED',
        'konsola-lojerash|SONY|PlayStation 5|56000||PLAYSTATION_5|DISC',
        'konsola-lojerash|SONY|PlayStation 5 Slim Digital|49000||PLAYSTATION_5|DIGITAL',
        'konsola-lojerash|SONY|PlayStation 5 Pro|85000||PLAYSTATION_5_PRO|DIGITAL',
        'konsola-lojerash|SONY|PlayStation 4 Pro|26000||PLAYSTATION_4_PRO|DISC',
        'konsola-lojerash|MICROSOFT|Xbox Series X|52000||XBOX_SERIES_X|DISC',
        'konsola-lojerash|MICROSOFT|Xbox Series S|29000||XBOX_SERIES_S|DIGITAL',
        'konsola-lojerash|NINTENDO|Nintendo Switch OLED|33000||SWITCH_OLED|', 'konsola-lojerash|NINTENDO|Nintendo Switch Lite|18000||SWITCH_LITE|',
        'konsola-lojerash|NINTENDO|Nintendo Switch 2|52000||SWITCH_2|', 'konsola-lojerash|VALVE|Steam Deck OLED|62000||STEAM_DECK_OLED|',
        'aparate-fotografike|CANON|Canon EOS 2000D|36000||DSLR|APS_C|CANON_EF', 'aparate-fotografike|CANON|Canon EOS 90D|92000||DSLR|APS_C|CANON_EF',
        'aparate-fotografike|CANON|Canon EOS R6|195000||MIRRORLESS|FULL_FRAME|CANON_RF',
        'aparate-fotografike|SONY|Sony Alpha a7 III|155000||MIRRORLESS|FULL_FRAME|SONY_E',
        'aparate-fotografike|SONY|Sony ZV-E10|62000||MIRRORLESS|APS_C|SONY_E', 'aparate-fotografike|NIKON|Nikon D750|82000||DSLR|FULL_FRAME|NIKON_F',
        'aparate-fotografike|FUJIFILM|Fujifilm X-T30|78000||MIRRORLESS|APS_C|FUJIFILM_X', 'aparate-fotografike|GOPRO|GoPro HERO11|36000||ACTION|ONE_INCH|',
        'aparate-fotografike|CANON|Objektiv Canon EF 50mm f/1.8 STM|14000||LENS||CANON_EF', 'aparate-fotografike|DJI|DJI Mini 3 Pro|62000||DRONE|ONE_INCH|',
        'audio-dhe-kufje|APPLE|AirPods Pro 2|25000||EARBUDS|1|1', 'audio-dhe-kufje|APPLE|AirPods 3|18000||EARBUDS|1|0',
        'audio-dhe-kufje|SONY|Sony WH-1000XM5|36000||HEADPHONES|1|1', 'audio-dhe-kufje|SONY|Sony WF-1000XM4|21000||EARBUDS|1|1',
        'audio-dhe-kufje|SAMSUNG|Galaxy Buds2 Pro|15000||EARBUDS|1|1', 'audio-dhe-kufje|JBL|JBL Charge 5|16000||SPEAKER|1|0',
        'audio-dhe-kufje|BOSE|Bose QuietComfort 45|29000||HEADPHONES|1|1', 'audio-dhe-kufje|MARSHALL|Marshall Emberton II|18000||SPEAKER|1|0',
        'audio-dhe-kufje|JBL|JBL PartyBox 310|46000||SPEAKER|1|0', 'audio-dhe-kufje|SONY|Soundbar Sony HT-S400|24000||SOUNDBAR|1|0',
        'audio-dhe-kufje|YAMAHA|Amplifikator Yamaha A-S501|42000||AMPLIFIER|0|0']) AS i
        WHERE split_part(i, '|', 1) = category));
    spec text[] := string_to_array(item, '|');
    storages text[] := string_to_array(nullif(spec[5], ''), ',');
    storage text := pg_temp.pick(storages);
    selling boolean;
    battery int;
    warranty boolean;
    included text[];
    features text[];
    smart boolean;
    controllers int;
    games int;
    shutters int;
BEGIN
    w.listing_type := pg_temp.pick(ARRAY['SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'SELL', 'BUY']);
    w.condition := CASE WHEN random() < 0.04 THEN 'FOR_PARTS' ELSE pg_temp.condition() END;
    w.price := pg_temp.round_price(spec[4]::numeric * pg_temp.worth(w.condition) * (0.85 + random() * 0.3)
                                   * (1 + 0.12 * coalesce(array_position(storages, storage) - 1, 0)));
    w.price_type := pg_temp.pick(ARRAY['FIXED', 'FIXED', 'NEGOTIABLE']);
    w.title := concat_ws(', ', CASE w.listing_type WHEN 'BUY' THEN 'Blej ' ELSE '' END || spec[3],
                         CASE storage WHEN '1000' THEN '1 TB' WHEN '2000' THEN '2 TB' ELSE storage || ' GB' END,
                         CASE WHEN w.listing_type = 'BUY' THEN NULL
                              WHEN w.condition = 'NEW' THEN pg_temp.pick(ARRAY['i ri në kuti', 'i pahapur', 'i ri, me garanci'])
                              WHEN w.condition = 'LIKE_NEW' THEN 'si i ri'
                              WHEN w.condition = 'USED' THEN pg_temp.pick(ARRAY['i përdorur', 'në gjendje të mirë', 'punon perfekt'])
                              ELSE 'për pjesë' END);
    -- What the thing is goes into a wanted ad too; how this one is kept, only into a sale.
    selling := w.listing_type = 'SELL';
    IF selling THEN
        battery := CASE WHEN category IN ('telefona', 'tableta', 'laptope') AND w.condition <> 'NEW'
                        THEN pg_temp.number_between(78, 100) END;
        warranty := random() < CASE w.condition WHEN 'NEW' THEN 0.85 WHEN 'LIKE_NEW' THEN 0.3 WHEN 'USED' THEN 0.05 ELSE 0 END;
        included := array_remove(ARRAY[
            CASE WHEN random() < CASE w.condition WHEN 'NEW' THEN 0.95 WHEN 'LIKE_NEW' THEN 0.6 ELSE 0.25 END THEN 'BOX' END,
            CASE WHEN random() < CASE w.condition WHEN 'NEW' THEN 0.6 WHEN 'LIKE_NEW' THEN 0.35 ELSE 0.1 END THEN 'INVOICE' END,
            CASE WHEN category IN ('telefona', 'tableta', 'laptope', 'audio-dhe-kufje', 'aparate-fotografike') AND random() < 0.5
                 THEN 'CHARGER' END,
            CASE WHEN category IN ('telefona', 'tableta') AND random() < 0.2 THEN 'CASE' END,
            CASE WHEN category = 'tableta' AND random() < 0.1 THEN 'PEN' END,
            CASE WHEN category = 'tableta' AND random() < 0.1 THEN 'KEYBOARD' END,
            CASE WHEN category = 'televizore' AND random() < 0.85 THEN 'REMOTE' END], NULL);
    END IF;
    w.properties := jsonb_build_object(
        'electronics_brand', spec[2],
        'electronics_storage', CASE storage WHEN '1000' THEN 'TB1' WHEN '2000' THEN 'TB2' ELSE 'GB' || storage END,
        'electronics_warranty', CASE WHEN warranty THEN true END,
        'electronics_included', CASE WHEN cardinality(included) > 0 THEN to_jsonb(included) END);
    CASE category
    WHEN 'telefona' THEN
        features := array_remove(ARRAY[
            CASE WHEN spec[8] = '1' THEN 'FIVE_G' END,
            CASE WHEN random() < CASE spec[2] WHEN 'APPLE' THEN 0.2 ELSE 0.6 END THEN 'DUAL_SIM' END,
            CASE WHEN random() < CASE spec[2] WHEN 'APPLE' THEN 0.5 WHEN 'GOOGLE' THEN 0.4 ELSE 0.1 END THEN 'ESIM' END], NULL);
        w.properties := w.properties || jsonb_build_object(
            'phone_model', spec[6],
            'electronics_ram', pg_temp.pick(string_to_array(spec[7], '/')),
            'phone_battery', battery,
            'electronics_color', CASE WHEN selling THEN pg_temp.pick(ARRAY['BLACK', 'BLACK', 'BLACK', 'WHITE', 'WHITE', 'GREY', 'SILVER',
                                                                           'GOLD', 'BLUE', 'BLUE', 'GREEN', 'RED', 'PURPLE', 'PINK']) END,
            'electronics_features', CASE WHEN selling AND cardinality(features) > 0 THEN to_jsonb(features) END);
    WHEN 'laptope' THEN
        features := array_remove(ARRAY[CASE WHEN random() < 0.08 THEN 'TOUCHSCREEN' END,
                                       CASE WHEN random() < 0.5 THEN 'BACKLIT_KEYBOARD' END,
                                       CASE WHEN random() < 0.04 THEN 'TWO_IN_ONE' END], NULL);
        w.properties := w.properties || jsonb_build_object(
            'computer_cpu', pg_temp.pick(string_to_array(spec[6], '/')),
            'electronics_ram', pg_temp.pick(string_to_array(spec[7], '/')),
            'electronics_screen', spec[8]::numeric,
            'computer_gpu', spec[9],
            'computer_os', spec[10],
            'computer_disk', CASE WHEN selling THEN CASE WHEN random() < 0.95 THEN 'SSD' ELSE 'SSD_HDD' END END,
            'electronics_features', CASE WHEN selling AND cardinality(features) > 0 THEN to_jsonb(features) END);
    WHEN 'kompjutere' THEN
        w.properties := w.properties || jsonb_build_object(
            'electronics_device', spec[6],
            'computer_cpu', pg_temp.pick(string_to_array(spec[7], '/')),
            'electronics_ram', pg_temp.pick(string_to_array(spec[8], '/')),
            'computer_gpu', spec[9],
            'computer_os', spec[10],
            'computer_disk', CASE WHEN selling THEN CASE WHEN random() < 0.75 THEN 'SSD' WHEN random() < 0.8 THEN 'SSD_HDD' ELSE 'HDD' END END,
            'computer_monitor', CASE WHEN selling AND spec[6] IN ('PC_DESKTOP', 'PC_GAMING') AND random() < 0.25 THEN true END);
    WHEN 'tableta' THEN
        w.properties := w.properties || jsonb_build_object(
            'electronics_screen', spec[6]::numeric,
            'tablet_connectivity', CASE WHEN random() < 0.25 THEN 'CELLULAR' ELSE 'WIFI' END);
    WHEN 'televizore' THEN
        smart := spec[7] <> 'HD' OR random() < 0.5;
        w.properties := w.properties || jsonb_build_object(
            'electronics_screen', spec[6]::numeric,
            'tv_resolution', spec[7],
            'tv_panel', spec[8],
            'tv_smart', CASE WHEN smart THEN true END);
    WHEN 'konsola-lojerash' THEN
        controllers := CASE WHEN selling AND spec[6] NOT IN ('SWITCH_LITE', 'STEAM_DECK_OLED')
                            THEN pg_temp.pick(ARRAY[1, 1, 1, 1, 2, 2, 2, 3]) END;
        games := CASE WHEN selling AND random() < 0.4 THEN pg_temp.number_between(2, 6) END;
        w.properties := w.properties || jsonb_build_object(
            'console_model', spec[6],
            'console_edition', nullif(spec[7], ''),
            'console_games', CASE WHEN games IS NOT NULL THEN true END,
            'console_controllers', controllers);
    WHEN 'aparate-fotografike' THEN
        shutters := CASE WHEN selling AND w.condition <> 'NEW' AND spec[6] IN ('DSLR', 'MIRRORLESS')
                         THEN round(pg_temp.number_between(1000, 60000)::numeric, -2)::int END;
        w.properties := w.properties || jsonb_build_object(
            'electronics_device', spec[6],
            'camera_sensor', nullif(spec[7], ''),
            'camera_mount', nullif(spec[8], ''),
            'camera_shutter_count', shutters);
    WHEN 'audio-dhe-kufje' THEN
        w.properties := w.properties || jsonb_build_object(
            'electronics_device', spec[6],
            'audio_wireless', CASE WHEN spec[7] = '1' THEN true END,
            'audio_noise_cancelling', CASE WHEN spec[8] = '1' THEN true END);
    ELSE
        NULL;
    END CASE;
    w.properties := jsonb_strip_nulls(w.properties);
    -- The description doesn't say what the properties don't: no remote that didn't come with it.
    w.description := concat_ws(' ',
        CASE WHEN battery IS NOT NULL THEN format('Bateria %s%%.', battery) END,
        pg_temp.sentences(array_remove(CASE category
            WHEN 'televizore' THEN ARRAY[CASE WHEN 'REMOTE' = ANY(included) THEN 'Me telekomandë dhe këmbë origjinale.' END,
                                         CASE WHEN smart THEN 'Smart TV me Netflix dhe YouTube.' END, 'Asnjë pixel i djegur.',
                                         CASE WHEN warranty THEN 'Garanci 6 muaj.' END, 'Shitet sepse kam marrë një më të madh.']
            WHEN 'konsola-lojerash' THEN ARRAY[CASE controllers WHEN 2 THEN 'Me dy kontrollerë.' WHEN 3 THEN 'Me tre kontrollerë.' END,
                                               CASE WHEN games IS NOT NULL THEN format('Me %s lojëra.', games) END,
                                               CASE WHEN warranty THEN 'Garanci 12 muaj.' END,
                                               CASE WHEN 'BOX' = ANY(included) THEN 'Me kuti dhe të gjithë kabllot.' END,
                                               'Përdorur shumë pak.']
            WHEN 'aparate-fotografike' THEN ARRAY[CASE WHEN shutters < 15000 THEN 'Numër i ulët shkrepjesh.' END,
                                                  'Me çantë dhe kartë memorie.',
                                                  CASE WHEN spec[6] = 'DSLR' AND spec[7] = 'APS_C' THEN 'Me objektiv 18-55 mm.' END,
                                                  CASE WHEN spec[6] <> 'LENS' THEN 'Sensor i pastër, asnjë njollë.' END,
                                                  'Ideal për fotografi dhe video.']
            WHEN 'audio-dhe-kufje' THEN ARRAY['Zë i pastër dhe bas i fortë.',
                                              CASE WHEN spec[6] = 'EARBUDS' THEN 'Me kutinë karikuese.' END,
                                              CASE WHEN spec[8] = '1' THEN 'Anulim aktiv i zhurmës.' END,
                                              CASE WHEN spec[7] = '1' THEN 'Bateria mban gjatë.' END,
                                              CASE WHEN 'INVOICE' = ANY(included) THEN 'Origjinale, me faturë.' END]
            WHEN 'laptope' THEN ARRAY[CASE WHEN 'BACKLIT_KEYBOARD' = ANY(features) THEN 'Tastierë me ndriçim.' END,
                                      CASE WHEN spec[10] = 'WINDOWS' THEN 'Windows 11 i instaluar.' END,
                                      'Ideal për punë dhe studime.',
                                      CASE WHEN 'CHARGER' = ANY(included) THEN 'Me karikues origjinal.' END,
                                      'Ekrani pa asnjë gërvishtje.', 'Shumë i shpejtë, me SSD.']
            WHEN 'kompjutere' THEN ARRAY[CASE WHEN spec[10] = 'WINDOWS' THEN 'Windows 11 i instaluar.' END,
                                         'Ideal për lojëra dhe punë grafike.', 'I pastër nga brenda, me ftohje të mirë.',
                                         'Me tastierë dhe maus.',
                                         CASE WHEN warranty AND 'INVOICE' = ANY(included) THEN 'Fatura dhe garancia në dispozicion.' END]
            ELSE ARRAY['Pa asnjë gërvishtje.',
                       CASE WHEN 'BOX' = ANY(included) AND 'CHARGER' = ANY(included) THEN 'Me kuti dhe karikues.' END,
                       CASE WHEN warranty THEN 'Garanci 6 muaj.' END,
                       'Ekrani origjinal, asnjë riparim.', 'I pabllokuar, punon me çdo operator.',
                       'Shitet sepse kam marrë një të ri.', 'Mbrojtëse ekrani e vendosur që ditën e parë.'] END, NULL), 3));
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- the other things people sell

-- "category|name|price in the category's currency|for whom (fashion)". A name can end in a
-- choice of colours, materials or brands, which the listing picks from: "Divan qoshe {gri;bezhë}".
CREATE FUNCTION pg_temp.thing(category text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    item text := pg_temp.pick(ARRAY(SELECT i FROM unnest(ARRAY[
        'mobilje|Divan qoshe {ngjyrë gri;bezhë;ngjyrë kafe;i bardhë}|60000', 'mobilje|Divan 3+2+1 {lëkure;stofi}|90000',
        'mobilje|Tavolinë ngrënieje me 6 karrige|45000', 'mobilje|Krevat dopio me dyshek|50000', 'mobilje|Dollap rrobash me 3 kanate|35000',
        'mobilje|Komodinë {druri;e bardhë}|8000', 'mobilje|Kolltuk relaksi|20000', 'mobilje|Tavolinë kafeje {druri;xhami}|10000',
        'mobilje|Mobilje kuzhine e plotë|150000', 'mobilje|Raft librash|9000', 'mobilje|Tavolinë pune me karrige|15000',
        'elektroshtepiake|Lavatriçe {Bosch;Samsung;LG;Beko;Whirlpool;Candy} {7;8;9} kg|40000',
        'elektroshtepiake|Frigorifer {Samsung;LG;Beko;Bosch} No Frost|55000',
        'elektroshtepiake|Kondicioner {LG;Samsung;Gree;Daikin;Midea} {9000;12000;18000} BTU inverter|45000',
        'elektroshtepiake|Pjatalarëse {Bosch;Beko;Candy}|35000', 'elektroshtepiake|Furrë elektrike {Beko;Bosch;Gorenje}|30000',
        'elektroshtepiake|Fshesë me korrent {Dyson;Philips;Rowenta}|15000', 'elektroshtepiake|Mikrovalë {Samsung;LG}|8000',
        'elektroshtepiake|Makinë kafeje {DeLonghi;Philips;Nespresso}|20000', 'elektroshtepiake|Sobë me dru|25000',
        'elektroshtepiake|Bojler {50;80} litra|12000', 'elektroshtepiake|Tharëse rrobash {Bosch;Beko}|45000',
        'kopsht|Kositëse bari {Bosch;Makita;Husqvarna}|20000', 'kopsht|Set tavoline kopshti me 4 karrige|25000',
        'kopsht|Barbecue me qymyr|9000', 'kopsht|Çadër kopshti|7000', 'kopsht|Pompë uji|12000', 'kopsht|Fidanë ulliri|1500',
        'kopsht|Shtëpizë druri për kopsht|90000', 'kopsht|Lule dekorative në vazo|2500',
        'dekor|Pasqyrë muri|8000', 'dekor|Llambadar kristali|25000', 'dekor|Tablo në kanavacë|6000',
        'dekor|Qilim {160x230;200x300}|15000', 'dekor|Perde ditore dhe nate|9000', 'dekor|Vazo qeramike|3000', 'dekor|Orë muri|4000',
        'xhaketa-dhe-pallto|Xhaketë lëkure {Zara;Mango;Massimo Dutti}|12000|ANY', 'xhaketa-dhe-pallto|Pallto leshi {Zara;Mango;Massimo Dutti}|15000|WOMEN',
        'xhaketa-dhe-pallto|Xhup dimri {The North Face;Columbia;Moncler}|20000|ANY', 'xhaketa-dhe-pallto|Xhaketë xhins {Levi''s;H&M}|6000|UNISEX',
        'kepuce|Atlete Nike Air Max|11000|ANY', 'kepuce|Atlete Adidas Superstar|9000|ANY', 'kepuce|Atlete New Balance 574|10000|ANY',
        'kepuce|Këpucë lëkure|7000|MEN', 'kepuce|Çizme lëkure|9000|WOMEN', 'kepuce|Sandale|3500|WOMEN',
        'fustane|Fustan mbrëmjeje|12000|WOMEN', 'fustane|Fustan nusërie|60000|WOMEN', 'fustane|Fustan veror|4000|WOMEN',
        'fustane|Fustan kokteji|8000|WOMEN', 'fustane|Fustan për vajza|3000|KIDS',
        'canta|Çantë Michael Kors|18000|WOMEN', 'canta|Çantë Guess|12000|WOMEN', 'canta|Çantë lëkure|7000|WOMEN',
        'canta|Çantë shpine|4000|UNISEX', 'canta|Çantë laptopi|3500|UNISEX',
        'ora|Orë Casio G-Shock|12000|MEN', 'ora|Orë Tissot|35000|MEN', 'ora|Apple Watch Series 8|35000|UNISEX',
        'ora|Orë Seiko automatike|28000|MEN', 'ora|Orë Fossil|14000|WOMEN',
        'aksesore|Syze dielli Ray-Ban|12000|UNISEX', 'aksesore|Rrip lëkure|3000|MEN', 'aksesore|Shall kashmiri|5000|WOMEN',
        'aksesore|Kapelë|2000|UNISEX', 'aksesore|Portofol lëkure|3500|MEN',
        'bicikleta|Biçikletë mali Trek Marlin 5|60000', 'bicikleta|Biçikletë qyteti|15000', 'bicikleta|Biçikletë elektrike|90000',
        'bicikleta|Biçikletë për fëmijë 16"|8000', 'bicikleta|Biçikletë rruge Specialized|120000',
        'fitnes|Shirit vrapimi|60000', 'fitnes|Set peshash 20 kg|8000', 'fitnes|Bankë stërvitjeje|12000', 'fitnes|Biçikletë statike|25000',
        'fitnes|Stol shtytjeje me shufër|35000',
        'instrumente-muzikore|Kitarë akustike Yamaha|15000', 'instrumente-muzikore|Kitarë elektrike Fender Stratocaster|90000',
        'instrumente-muzikore|Piano dixhitale Casio|45000', 'instrumente-muzikore|Violinë 4/4|12000',
        'instrumente-muzikore|Bateri elektronike Roland|80000', 'instrumente-muzikore|Klarinetë|25000',
        'kamping-dhe-peshkim|Çadër kampingu për 4 veta|12000', 'kamping-dhe-peshkim|Kallam peshkimi me makinë|6000',
        'kamping-dhe-peshkim|Thes gjumi|4000', 'kamping-dhe-peshkim|Varkë gome|45000',
        'sporte-dimerore|Ski Atomic me lidhëse|25000', 'sporte-dimerore|Snowboard Burton|30000', 'sporte-dimerore|Këpucë skish|10000',
        'sporte-dimerore|Kostum skish|9000',
        'sport-te-tjera|Tavolinë pingpongu|20000', 'sport-te-tjera|Top futbolli Adidas|3000', 'sport-te-tjera|Dërrasë surfi|25000',
        'sport-te-tjera|Shah druri|2500',
        'qen|Këlysh Labrador, {2;3;4} muajsh|25000', 'qen|Këlysh Pastor Gjerman|20000', 'qen|Golden Retriever, {2;3} muajsh|30000',
        'qen|Husky Siberian|25000', 'qen|Maltez mini|30000', 'qen|Pomeranian|40000', 'qen|Qen Sharri|15000',
        'qen|Dhurohet këlysh i përzier|0',
        'mace|Mace Persiane|20000', 'mace|British Shorthair|35000', 'mace|Dhurohen kotele të vogla|0',
        'zogj|Papagall Ara|60000', 'zogj|Kanarina këngëtare|3000', 'zogj|Pëllumba race|5000', 'zogj|Papagall Kakadu|80000',
        'peshq|Akuarium 100 litra me peshq|15000', 'peshq|Peshq tropikalë|500', 'peshq|Breshkë uji|2000',
        'aksesore-per-kafshe|Kafaz për zogj|4000', 'aksesore-per-kafshe|Shtrat për qen|3500',
        'aksesore-per-kafshe|Transportues për mace|3000', 'aksesore-per-kafshe|Rrip dhe qafore|1500',
        'makineri-ndertimi|Ekskavator JCB 3CX|4500000', 'makineri-ndertimi|Ekskavator CAT 320|6000000',
        'makineri-ndertimi|Betoniere 350 litra|60000', 'makineri-ndertimi|Fadromë Komatsu WA320|5000000',
        'makineri-bujqesore|Traktor John Deere 6110|3000000', 'makineri-bujqesore|Traktor New Holland TD5|2500000',
        'makineri-bujqesore|Motokultivator|150000', 'makineri-bujqesore|Frezë toke|80000',
        'energji-dhe-gjeneratore|Gjenerator {5;10;20} kW|120000', 'energji-dhe-gjeneratore|Panele diellore 450 W, 10 copë|150000',
        'energji-dhe-gjeneratore|Inverter hibrid 10 kW|180000', 'energji-dhe-gjeneratore|Bateri litiumi 10 kWh|400000',
        'makineri-industriale|Kompresor ajri 500 litra|250000', 'makineri-industriale|Makinë qepëse industriale Juki|90000',
        'makineri-industriale|Torno metali|600000', 'makineri-industriale|Pirun elektrik Toyota|1500000',
        'te-tjera|Bileta për koncert|3000', 'te-tjera|Libra të përdorur|500', 'te-tjera|Koleksion pullash|15000',
        'te-tjera|Lodra për fëmijë|2000', 'te-tjera|Karrocë për fëmijë|12000', 'te-tjera|Kostum karnavali|2500']) AS i
        WHERE split_part(i, '|', 1) = category));
    -- The name, with one of each {a;b} choice in it.
    name text := split_part(item, '|', 2);
    choice text;
    pets boolean := category IN ('qen', 'mace', 'zogj', 'peshq', 'aksesore-per-kafshe');
    machines boolean := category LIKE 'makineri-%' OR category = 'energji-dhe-gjeneratore';
    fashion boolean := category IN ('xhaketa-dhe-pallto', 'kepuce', 'fustane', 'canta', 'ora', 'aksesore');
    -- Clothes' sizes; shoes' are numbers, which the property doesn't have.
    size text := CASE WHEN category IN ('xhaketa-dhe-pallto', 'fustane')
                      THEN pg_temp.pick(ARRAY['XS', 'S', 'S', 'M', 'M', 'M', 'L', 'L', 'XL', 'XXL']) END;
    year int := pg_temp.number_between(1998, extract(year FROM now())::int - 1);
BEGIN
    LOOP
        choice := substring(name FROM '\{[^}]*\}');
        EXIT WHEN choice IS NULL;
        name := overlay(name PLACING pg_temp.pick(string_to_array(btrim(choice, '{}'), ';'))
                        FROM strpos(name, choice) FOR length(choice));
    END LOOP;
    w.listing_type := CASE WHEN random() < 0.06 AND split_part(item, '|', 3) <> '0' THEN 'BUY' ELSE 'SELL' END;
    w.condition := CASE WHEN pets THEN NULL WHEN machines THEN pg_temp.pick(ARRAY['NEW', 'USED', 'USED', 'USED']) ELSE pg_temp.condition() END;
    IF split_part(item, '|', 3) = '0' THEN
        w.price := 0;
        w.price_type := 'FREE';
    ELSE
        w.price := pg_temp.round_price(split_part(item, '|', 3)::numeric * coalesce(pg_temp.worth(w.condition), 1) * (0.8 + random() * 0.4)
                                       * CASE WHEN machines AND w.condition = 'USED' THEN power(0.94, extract(year FROM now()) - year) / 0.6 ELSE 1 END);
        w.price_type := pg_temp.pick(ARRAY['FIXED', 'FIXED', 'NEGOTIABLE']);
    END IF;
    w.title := CASE w.listing_type WHEN 'BUY' THEN pg_temp.wanted(name) ELSE name END
               || CASE WHEN size IS NOT NULL THEN ', masa ' || size ELSE '' END
               || CASE WHEN machines AND w.condition = 'USED' THEN ', viti ' || year ELSE '' END;
    w.properties := jsonb_strip_nulls(jsonb_build_object(
        'fashion_for', CASE WHEN fashion THEN CASE split_part(item, '|', 4) WHEN 'ANY' THEN pg_temp.pick(ARRAY['MEN', 'WOMEN']) ELSE split_part(item, '|', 4) END END,
        'fashion_size', size,
        'machine_year', CASE WHEN machines THEN CASE w.condition WHEN 'NEW' THEN extract(year FROM now())::int ELSE year END END));
    w.description := pg_temp.sentences(CASE
        WHEN pets THEN ARRAY['I vaksinuar dhe me pasaportë.', 'Prindërit mund t''i shihni.', 'Shumë i dashur me fëmijët.', 'Me mikroçip.',
                             'Ushqehet me ushqim të thatë.', 'Kërkoj familje që do të kujdeset mirë.']
        WHEN machines THEN ARRAY['Gati për punë.', 'Servis i rregullt, me libreze.', 'Dokumentet në rregull.', 'Mundësi transporti.',
                                 'Shitet sepse po mbyllim aktivitetin.', 'Pjesë këmbimi gjenden lehtë.']
        WHEN fashion THEN ARRAY['E veshur vetëm një herë.', 'Origjinale, me faturë.', 'Ngjyra si në foto.', 'Pa asnjë dëmtim.',
                                'Dërgoj me postë në gjithë Shqipërinë.', 'Mund ta provoni para se ta blini.']
        WHEN category IN ('mobilje', 'dekor', 'kopsht') THEN ARRAY['Pothuajse i ri, përdorur vetëm pak muaj.', 'Transporti organizohet me pagesë.',
                                 'Merret në vend.', 'Shitet sepse po ndërrojmë shtëpi.', 'Cilësi e lartë, material i qëndrueshëm.', 'Ngjyra si në foto.']
        WHEN category = 'elektroshtepiake' THEN ARRAY['Punon perfekt.', 'Klasa energjetike A+++.', 'Me garanci 6 muaj.', 'Konsum i ulët energjie.',
                                 'Shitet sepse kam blerë model më të madh.', 'Transporti falas brenda qytetit.']
        ELSE ARRAY['Në gjendje shumë të mirë.', 'Përdorur shumë pak.', 'Merret në vend ose dërgohet me postë.', 'Çmimi pak i diskutueshëm.',
                   'Për çdo pyetje më telefononi.'] END, 2);
    RETURN w;
END
$$;

-- ---------------------------------------------------------------- jobs and services

CREATE FUNCTION pg_temp.job(category text, town text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    work text := pg_temp.pick(CASE category
        WHEN 'pune-turizem' THEN ARRAY['kamarier/e', 'kuzhinier/e', 'banakier/e', 'recepsionist/e', 'ndihmës kuzhinier/e', 'pastrues/e dhomash', 'pjatalarës/e']
        WHEN 'pune-ndertim' THEN ARRAY['murator', 'elektricist', 'hidraulik', 'inxhinier ndërtimi', 'punëtor ndërtimi', 'operator ekskavatori']
        WHEN 'pune-shendetesi' THEN ARRAY['infermier/e', 'farmacist/e', 'asistent/e stomatologjie', 'fizioterapist/e', 'mjek/e i/e përgjithshëm/e']
        WHEN 'pune-it' THEN ARRAY['programues/e Java', 'zhvillues/e frontend (React)', 'administrator/e sistemi', 'QA tester', 'analist/e të dhënash', 'zhvillues/e Android']
        WHEN 'pune-shitje' THEN ARRAY['shitës/e', 'agjent/e shitjesh', 'arkëtar/e', 'menaxher/e dyqani', 'merchandiser']
        WHEN 'pune-transport' THEN ARRAY['shofer kamioni', 'shofer taksie', 'korrier', 'shofer furgoni', 'shofer autobusi']
        ELSE ARRAY['operator/e call center me italisht', 'operator/e call center me anglisht', 'operator/e call center me gjermanisht', 'supervizor/e call center']
    END);
    salary int := CASE category
        WHEN 'pune-it' THEN pg_temp.number_between(120, 400) WHEN 'pune-shendetesi' THEN pg_temp.number_between(60, 150)
        WHEN 'pune-ndertim' THEN pg_temp.number_between(50, 120) ELSE pg_temp.number_between(45, 100) END * 1000;
BEGIN
    w.listing_type := 'JOB';
    w.title := format(pg_temp.pick(ARRAY['Kërkohet %s në %s', 'Punësojmë %s në %s', 'Ofrohet punë si %s, %s']), work, town);
    w.price_type := pg_temp.pick(ARRAY['FROM', 'FROM', 'NEGOTIABLE', 'ON_REQUEST']);
    w.price := CASE WHEN w.price_type <> 'ON_REQUEST' THEN pg_temp.round_price(salary) END;
    w.properties := jsonb_build_object('job_schedule', CASE
        WHEN category = 'pune-turizem' THEN pg_temp.pick(ARRAY['FULL_TIME', 'FULL_TIME', 'PART_TIME', 'SEASONAL', 'SEASONAL'])
        ELSE pg_temp.pick(ARRAY['FULL_TIME', 'FULL_TIME', 'FULL_TIME', 'PART_TIME']) END);
    w.description := pg_temp.sentences(ARRAY[
        'Kërkojmë person serioz dhe të përgjegjshëm.', 'Eksperienca përbën avantazh.', 'Pagë e mirë dhe bonuse sipas rezultateve.',
        'Sigurime shoqërore të plota.', 'Ofrohet trajnim në vendin e punës.', 'Për aplikim, na telefononi ose dërgoni CV-në.',
        'Ambient pune i rregullt dhe miqësor.', 'Fillim i menjëhershëm.',
        format('Njohja e %s është e domosdoshme.', pg_temp.pick(ARRAY['anglishtes', 'italishtes', 'gjermanishtes']))], 3);
    RETURN w;
END
$$;

CREATE FUNCTION pg_temp.service(category text, town text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
DECLARE
    w pg_temp.writing;
    -- "what is offered|from, in lekë".
    item text := pg_temp.pick(CASE category
        WHEN 'riparime' THEN ARRAY['Riparim kondicionerësh|2000', 'Riparim telefonash dhe tabletash|1500', 'Elektricist 24/7|2000',
                                   'Hidraulik për çdo defekt|2000', 'Riparim lavatriçesh dhe frigoriferësh|2500', 'Riparim kompjuterësh|1500']
        WHEN 'transport-dhe-transferime' THEN ARRAY['Transport mallrash brenda dhe jashtë qytetit|5000', 'Transferim nga aeroporti i Rinasit|2500',
                                   'Transport mobiliesh me ekip|8000', 'Shërbim taksie për çdo destinacion|1000']
        WHEN 'pastrim' THEN ARRAY['Pastrim apartamentesh dhe zyrash|3000', 'Pastrim kolltukësh me avull|4000', 'Pastrim pas rikonstruksionit|10000',
                                  'Pastrim xhamash|2500']
        WHEN 'mesime' THEN ARRAY['Mësime private matematike|1000', 'Kurs anglishteje për të gjitha nivelet|8000', 'Mësime pianoje|1500',
                                 'Përgatitje për maturën shtetërore|1200', 'Kurs gjermanishteje A1–B2|10000', 'Mësime programimi për fëmijë|1500']
        WHEN 'foto-dhe-video' THEN ARRAY['Fotograf për dasma dhe evente|30000', 'Video me dron|15000', 'Foto studio për portrete|5000',
                                         'Montim videosh|8000']
        ELSE ARRAY['Lyerje dhe suvatim|300', 'Rikonstruksion banjash dhe kuzhinash|150000', 'Shtrim pllakash|800',
                   'Montim gipsi dhe tavane|1200', 'Hidroizolim tarracash|700'] END);
BEGIN
    w.listing_type := 'SERVICE';
    w.title := split_part(item, '|', 1) || CASE WHEN random() < 0.6 THEN ' në ' || town ELSE '' END;
    w.price_type := pg_temp.pick(ARRAY['FROM', 'FROM', 'FROM', 'ON_REQUEST']);
    w.price := CASE WHEN w.price_type = 'FROM' THEN pg_temp.round_price(split_part(item, '|', 2)::numeric * (0.8 + random() * 0.5)) END;
    w.properties := '{}';
    w.description := pg_temp.sentences(ARRAY[
        format('Me përvojë mbi %s vjet.', pg_temp.number_between(3, 20)), 'Punë cilësore me çmime të arsyeshme.',
        'Ofrojmë garanci për çdo punë.', 'Vijmë brenda ditës.', 'Kontaktoni për një ofertë falas.',
        'Punojmë edhe të shtunën dhe të dielën.', 'Fatura tatimore për bizneset.'], 3);
    RETURN w;
END
$$;

-- A listing in any category, by its top-level category's slug (family) and its own.
CREATE FUNCTION pg_temp.write(family text, category text, town_slug text, town text) RETURNS pg_temp.writing LANGUAGE plpgsql VOLATILE AS $$
BEGIN
    RETURN CASE
        WHEN category = 'makina' THEN pg_temp.car()
        WHEN category IN ('motocikleta', 'automjete-tregtare') THEN pg_temp.vehicle(category)
        WHEN family = 'automjete' THEN pg_temp.part(category)
        WHEN family = 'prona' THEN pg_temp.property(category, town_slug, town)
        WHEN family = 'elektronike' THEN pg_temp.electronics(category)
        WHEN family = 'pune' THEN pg_temp.job(category, town)
        WHEN family = 'sherbime' THEN pg_temp.service(category, town)
        ELSE pg_temp.thing(category) END;
END
$$;

-- ---------------------------------------------------------------- what to make

-- How many listings of each kind out of 1 000, about as on a real site.
CREATE TEMP TABLE shares (slug text PRIMARY KEY, share int NOT NULL);
INSERT INTO shares VALUES
    ('makina', 220), ('motocikleta', 12), ('automjete-tregtare', 15), ('pjese-kembimi', 30), ('goma-dhe-disqe', 15),
    ('audio-dhe-video', 5), ('gps-dhe-navigatore', 3), ('styling-dhe-pajisje-sportive', 3), ('tapete-makine', 3),
    ('rimorkio', 2), ('mbushes-per-makina-elektrike', 2), ('pjese-te-tjera', 5),
    ('apartamente', 140), ('garsoniera', 30), ('vila-dhe-shtepi', 30), ('dyqane', 15), ('zyra', 10), ('truall', 15), ('garazhe', 5),
    ('telefona', 60), ('laptope', 20), ('kompjutere', 10), ('tableta', 10), ('televizore', 15), ('konsola-lojerash', 10),
    ('aparate-fotografike', 5), ('audio-dhe-kufje', 10),
    ('mobilje', 30), ('elektroshtepiake', 30), ('kopsht', 7), ('dekor', 5),
    ('pune-turizem', 15), ('pune-ndertim', 6), ('pune-shendetesi', 4), ('pune-it', 4), ('pune-shitje', 5), ('pune-transport', 4),
    ('pune-call-center', 4),
    ('riparime', 8), ('transport-dhe-transferime', 6), ('pastrim', 4), ('mesime', 5), ('foto-dhe-video', 3), ('ndertim-dhe-lyerje', 5),
    ('xhaketa-dhe-pallto', 8), ('kepuce', 8), ('fustane', 6), ('canta', 5), ('ora', 5), ('aksesore', 3),
    ('bicikleta', 8), ('fitnes', 5), ('instrumente-muzikore', 5), ('kamping-dhe-peshkim', 3), ('sporte-dimerore', 2), ('sport-te-tjera', 2),
    ('qen', 7), ('mace', 3), ('zogj', 2), ('peshq', 1), ('aksesore-per-kafshe', 2),
    ('makineri-ndertimi', 4), ('makineri-bujqesore', 3), ('energji-dhe-gjeneratore', 3), ('makineri-industriale', 2),
    ('te-tjera', 5);

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM shares s LEFT JOIN categories c ON c.slug = s.slug AND c.active AND c.allow_listings WHERE c.id IS NULL) THEN
        RAISE EXCEPTION 'a category in shares is not one listings go in: %',
            (SELECT string_agg(s.slug, ', ') FROM shares s LEFT JOIN categories c ON c.slug = s.slug AND c.active AND c.allow_listings WHERE c.id IS NULL);
    END IF;
END
$$;

-- Each category with its top-level category, which says how its listings are written.
CREATE TEMP TABLE families AS
WITH RECURSIVE up AS (
    SELECT id, id AS category_id, parent_id, slug FROM categories
    UNION ALL
    SELECT p.id, up.category_id, p.parent_id, p.slug FROM up JOIN categories p ON p.id = up.parent_id
)
SELECT c.id, c.slug, c.default_currency::text AS currency, up.slug AS family
FROM categories c JOIN up ON up.category_id = c.id AND up.parent_id IS NULL;

\if :{?only}
-- Only one top-level category's listings, in its own categories' shares.
DELETE FROM shares s WHERE NOT EXISTS (SELECT 1 FROM families f WHERE f.slug = s.slug AND f.family = :'only');
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM shares) THEN
        RAISE EXCEPTION 'only: no top-level category by that slug takes listings';
    END IF;
END
$$;
\endif

-- Towns, by about how many listings each has.
CREATE TEMP TABLE towns AS
SELECT m.id, m.slug, m.name::text AS name, m.latitude::float8 AS latitude, m.longitude::float8 AS longitude,
       CASE m.slug WHEN 'tirane' THEN 300 WHEN 'durres' THEN 90 WHEN 'vlore' THEN 55 WHEN 'elbasan' THEN 45 WHEN 'shkoder' THEN 45
                   WHEN 'fier' THEN 40 WHEN 'korce' THEN 40 WHEN 'kamez' THEN 30 WHEN 'berat' THEN 20 WHEN 'lushnje' THEN 20
                   WHEN 'sarande' THEN 18 WHEN 'kavaje' THEN 15 WHEN 'pogradec' THEN 15 WHEN 'lezhe' THEN 15 WHEN 'gjirokaster' THEN 12
                   WHEN 'kukes' THEN 8 WHEN 'kruje' THEN 8 WHEN 'vore' THEN 8 WHEN 'patos' THEN 6 WHEN 'himare' THEN 5
                   ELSE 3 END AS share
FROM locations m
WHERE m.location_type = 'MUNICIPALITY' AND m.active;

CREATE TEMP TABLE drafts AS
WITH slots AS (
    SELECT (SELECT array_agg(s.slug) FROM shares s, generate_series(1, s.share)) AS categories,
           (SELECT array_agg(t.slug) FROM towns t, generate_series(1, t.share)) AS places
)
SELECT g AS n, uuid_v7() AS id,
       slots.categories[1 + floor(random() * cardinality(slots.categories))::int] AS category,
       slots.places[1 + floor(random() * cardinality(slots.places))::int] AS town
FROM generate_series(1, :count) g, slots;

-- Written once each: in the FROM clause, as (write(...)).* would write a listing once per column.
CREATE TEMP TABLE written AS
SELECT d.n, d.id, f.id AS category_id, f.slug AS category, f.family, f.currency,
       t.id AS town_id, t.slug AS town_slug, t.latitude AS town_latitude, t.longitude AS town_longitude,
       w.*
FROM drafts d
JOIN families f ON f.slug = d.category
JOIN towns t ON t.slug = d.town
CROSS JOIN LATERAL pg_temp.write(f.family, f.slug, t.slug, t.name) AS w;

-- ---------------------------------------------------------------- sellers

-- Numbers go on from earlier runs', so every run's sellers have addresses and numbers of their own.
SELECT coalesce(max(split_part(auth_subject, '-', 2)::int), 0) AS made FROM users WHERE auth_subject LIKE 'dev|generated-%' \gset

-- Private sellers, about one for every 12 listings; some sell much more than others.
CREATE TEMP TABLE people AS
SELECT g AS n, uuid_v7() AS id, :made + g AS number,
       pg_temp.pick(ARRAY['Arben', 'Arta', 'Besa', 'Blerina', 'Dritan', 'Elira', 'Enkel', 'Erion', 'Fatjon', 'Gentian', 'Ilir', 'Jonida',
                          'Juxhin', 'Kejsi', 'Klajdi', 'Ledia', 'Lorena', 'Marsel', 'Mirela', 'Ornela', 'Rezarta', 'Sokol', 'Teuta',
                          'Valbona', 'Xhulio', 'Aurela', 'Denis', 'Eni', 'Florian', 'Kristi', 'Albana', 'Dorian', 'Elsa', 'Gerti',
                          'Ina', 'Klevis', 'Megi', 'Redi', 'Sara', 'Andi'])
       || ' ' || pg_temp.pick(ARRAY['A.', 'B.', 'Ç.', 'D.', 'Dh.', 'E.', 'G.', 'H.', 'K.', 'L.', 'M.', 'N.', 'P.', 'R.', 'S.', 'Sh.', 'T.', 'V.', 'Xh.', 'Z.'])
       AS name
FROM generate_series(1, greatest(10, :count / 12)) g;

INSERT INTO users (id, auth_subject, email, phone_e164, display_name, account_type)
SELECT id, 'dev|generated-' || number, 'gen.' || number || '@example.test', '+35569' || (1000000 + number), name, 'PRIVATE'
FROM people;

-- Businesses, about one for every 250 listings, each run by a seller of its own: car dealers,
-- property agencies, electronics shops, furniture shops, and employers, each kind selling that
-- share of its top-level category's listings.
CREATE TEMP TABLE firm_kinds (n int, kind text, family text, share float8);
INSERT INTO firm_kinds VALUES (1, 'auto', 'automjete', 0.35), (2, 'imobiliare', 'prona', 0.5), (3, 'tekno', 'elektronike', 0.25),
                              (4, 'mobileri', 'shtepi-dhe-kopsht', 0.2), (5, 'pune', 'pune', 0.7);
-- Of the kinds that sell where this run makes listings: all of them, or with only= one or none.
CREATE TEMP TABLE firms AS
SELECT g AS n, uuid_v7() AS id, uuid_v7() AS owner_id, :made + (SELECT count(*) FROM people) + g AS number,
       kinds.kind[1 + g % cardinality(kinds.kind)] AS family,
       NULL::text AS name
FROM (SELECT array_agg(k.kind ORDER BY k.n) AS kind
      FROM firm_kinds k
      WHERE k.family IN (SELECT f.family FROM shares s JOIN families f ON f.slug = s.slug)) kinds,
     generate_series(1, greatest(5, :count / 250)) g
WHERE kinds.kind IS NOT NULL;
UPDATE firms SET name = CASE family
    WHEN 'auto' THEN 'Auto '
    WHEN 'imobiliare' THEN 'Imobiliare '
    WHEN 'tekno' THEN pg_temp.pick(ARRAY['Tekno ', 'Smart ', 'Digital '])
    WHEN 'mobileri' THEN 'Mobileri '
    -- Employers in every line of work, so no name that says a hotel hires excavator drivers.
    ELSE pg_temp.pick(ARRAY['Kompania ', 'Grupi ', 'Agjencia '])
END || pg_temp.pick(ARRAY['Shqiponja', 'Drini', 'Vjosa', 'Iliria', 'Dardania', 'Arbëria', 'Tomori', 'Adriatiku', 'Butrinti',
                          'Kodra', 'Ylli', 'Dielli', 'Ura', 'Bregdeti', 'Lugina', 'Kalaja', 'Liqeni', 'Rozafa']);
-- Each firm's place in its family, for picking one.
ALTER TABLE firms ADD COLUMN k int;
UPDATE firms SET k = ranked.k FROM (SELECT id, row_number() OVER (PARTITION BY family ORDER BY n) AS k FROM firms) ranked WHERE ranked.id = firms.id;

INSERT INTO users (id, auth_subject, email, phone_e164, display_name, account_type)
SELECT owner_id, 'dev|generated-' || number, 'gen.' || number || '@example.test', '+35569' || (1000000 + number), name, 'BUSINESS'
FROM firms;
INSERT INTO businesses (id, legal_name, display_name, slug, public_phone)
SELECT id, name || ' shpk', name, 'gen-' || regexp_replace(lower(immutable_unaccent(name)), '[^a-z0-9]+', '-', 'g') || '-' || number,
       '+35568' || (1000000 + number)
FROM firms;
INSERT INTO business_members (business_id, user_id, role)
SELECT id, owner_id, 'OWNER' FROM firms;

-- ---------------------------------------------------------------- the listings

-- Who sells each: a firm of its kind for some, a private seller for the rest.
CREATE TEMP TABLE sold_by AS
SELECT w.id, CASE WHEN random() < coalesce(k.share, 0) THEN k.kind END AS firm_family, random() AS which
FROM written w
LEFT JOIN firm_kinds k ON k.family = w.family;

-- Points on the map, approximate like real sellers' ones: up to 1.5 km from the town's centre.
-- Where the centre is on the shore, they spread less far around a spot inland, so none land in
-- water (as in dev/sample_listings.sql).
CREATE TEMP TABLE inland (slug text, latitude float8, longitude float8, km float8);
INSERT INTO inland VALUES ('durres', 41.3260, 19.4540, 1.0), ('vlore', 40.4694, 19.4861, 1.0),
                          ('sarande', 39.8800, 20.0100, 0.5), ('pogradec', 40.8963, 20.6568, 0.5);

CREATE TEMP TABLE placed AS
SELECT w.*,
       coalesce(i.latitude, w.town_latitude) AS origin_latitude,
       coalesce(i.longitude, w.town_longitude) AS origin_longitude,
       coalesce(i.km, 1.5) * sqrt(random()) AS km,
       2 * pi() * random() AS angle,
       -- Mostly the last few weeks', some up to three months old.
       now() - power(random(), 1.5) * interval '90 days' AS published_at,
       random() AS fate,
       coalesce(people.id, firms.owner_id) AS created_by,
       CASE WHEN firms.id IS NULL THEN people.id END AS seller_user_id,
       firms.id AS seller_business_id
FROM written w
JOIN sold_by s ON s.id = w.id
LEFT JOIN inland i ON i.slug = w.town_slug
LEFT JOIN firms ON s.firm_family IS NOT NULL AND firms.family = s.firm_family
                  AND firms.k = 1 + floor(s.which * (SELECT count(*) FROM firms f WHERE f.family = s.firm_family))::int
LEFT JOIN people ON s.firm_family IS NULL
                   -- A few sellers have dozens of listings, most a handful.
                   AND people.n = 1 + floor(power(s.which, 1.3) * (SELECT count(*) FROM people))::int;

INSERT INTO listings (id, seller_user_id, seller_business_id, created_by_user_id, category_id, location_id,
                      slug, title, description, listing_type, condition, price_amount, currency, price_type,
                      price_normalized, status, published_at, sold_at, bumped_at, featured_until, latitude, longitude)
SELECT p.id, p.seller_user_id, p.seller_business_id, p.created_by, p.category_id,
       -- A property in the zone its title names, where its town has zones.
       coalesce(CASE WHEN p.family = 'prona' THEN pg_temp.zone_of(p.title, p.town_id) END, p.town_id),
       coalesce(nullif(left(btrim(regexp_replace(lower(immutable_unaccent(p.title)), '[^a-z0-9]+', '-', 'g'), '-'), 80), ''), 'njoftim'),
       p.title, p.description, p.listing_type, p.condition, p.price, p.currency, p.price_type,
       CASE p.price_type WHEN 'FREE' THEN 0 WHEN 'ON_REQUEST' THEN NULL ELSE price_in_eur(p.price, p.currency) END,
       -- Most are on the site; a few are sold, expired or taken down for a while.
       CASE WHEN p.fate < 0.92 THEN 'ACTIVE' WHEN p.fate < 0.96 THEN 'SOLD' WHEN p.fate < 0.99 THEN 'EXPIRED' ELSE 'PAUSED' END,
       p.published_at,
       CASE WHEN p.fate >= 0.92 AND p.fate < 0.96 THEN p.published_at + (now() - p.published_at) * random() END,
       -- One in twenty bumped since, one in a hundred featured for a while yet.
       CASE WHEN random() < 0.05 THEN p.published_at + (now() - p.published_at) * random() END,
       CASE WHEN random() < 0.01 THEN now() + pg_temp.number_between(1, 14) * interval '1 day' END,
       round((p.origin_latitude + p.km * cos(p.angle) / 111.32)::numeric, 6),
       round((p.origin_longitude + p.km * sin(p.angle) / (111.32 * cos(radians(p.origin_latitude))))::numeric, 6)
FROM placed p;

-- Properties: each subcategory's kind (migration 0008), then options, numbers and ticked boxes,
-- then multi-select options. A property code or option that doesn't exist fails the run here.
INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
SELECT w.id, o.attribute_id, 'SELECT', o.id
FROM written w
JOIN categories c ON c.id = w.category_id
JOIN attribute_options o ON o.id = c.replaces_option_id;

INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id, value_number, value_boolean)
SELECT w.id, a.id, a.data_type,
       CASE WHEN a.data_type = 'SELECT' THEN o.id END,
       CASE WHEN a.data_type = 'NUMBER' THEN (property.value #>> '{}')::numeric END,
       CASE WHEN a.data_type = 'BOOLEAN' THEN (property.value #>> '{}')::boolean END
FROM written w
CROSS JOIN LATERAL jsonb_each(w.properties) AS property (code, value)
JOIN attributes a ON a.code = property.code AND a.data_type IN ('SELECT', 'NUMBER', 'BOOLEAN')
LEFT JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = property.value #>> '{}';

INSERT INTO listing_attribute_options (listing_id, attribute_id, option_id)
SELECT w.id, a.id, o.id
FROM written w
CROSS JOIN LATERAL jsonb_each(w.properties) AS property (code, value)
JOIN attributes a ON a.code = property.code AND a.data_type = 'MULTISELECT'
CROSS JOIN LATERAL jsonb_array_elements_text(property.value) AS chosen (value_code)
JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = chosen.value_code;

DO $$
BEGIN
    -- Every property written is one the listing's category has.
    IF EXISTS (SELECT 1
               FROM written w
               CROSS JOIN LATERAL jsonb_object_keys(w.properties) AS property (code)
               JOIN attributes a ON a.code = property.code
               WHERE NOT EXISTS (SELECT 1 FROM effective_category_attributes eca
                                 WHERE eca.category_id = w.category_id AND eca.attribute_id = a.id)) THEN
        RAISE EXCEPTION 'a generated listing has a property its category does not';
    END IF;
    -- And every option written one its category offers (category_options, migration 0021): a
    -- phone's brand one of phones' brands.
    IF EXISTS (SELECT 1
               FROM written w
               CROSS JOIN LATERAL jsonb_each(w.properties) AS property (code, value)
               JOIN attributes a ON a.code = property.code AND a.data_type IN ('SELECT', 'MULTISELECT')
               CROSS JOIN LATERAL jsonb_array_elements_text(CASE jsonb_typeof(property.value) WHEN 'array' THEN property.value
                                                                 ELSE jsonb_build_array(property.value) END) AS chosen (value_code)
               WHERE NOT EXISTS (SELECT 1 FROM category_options(w.category_id, a.id) o WHERE o.value_code = chosen.value_code)) THEN
        RAISE EXCEPTION 'a generated listing has an option its category does not offer';
    END IF;
END
$$;

-- Each listing's photos: two to four of its category's.
INSERT INTO galleries
SELECT w.id, w.category, pg_temp.number_between(2, 4), 0, NULL, 'generated/' || w.id
FROM written w;
SELECT pg_temp.give_photos() \g /dev/null

-- Each listing shows its seller's phone: the business's number, or the private seller's own.
INSERT INTO listing_contacts (listing_id, contact_name, phone_e164, show_phone)
SELECT l.id, coalesce(b.display_name, u.display_name), coalesce(b.public_phone, u.phone_e164), true
FROM written w
JOIN listings l ON l.id = w.id
LEFT JOIN businesses b ON b.id = l.seller_business_id
LEFT JOIN users u ON u.id = l.seller_user_id;

-- What search finds them by, and cars' titles made from what they are.
UPDATE listings SET search_extra = listing_search_extra(id), generated_title = listing_generated_title(id)
WHERE id IN (SELECT id FROM written);

REFRESH MATERIALIZED VIEW search_words;
REFRESH MATERIALIZED VIEW zone_area_prices;
REFRESH MATERIALIZED VIEW car_model_prices;

COMMIT;

-- Setting search_extra above wrote every new listing again, leaving its first version behind:
-- vacuuming makes that space free for the next run's, which would otherwise grow the disk by as
-- much again.
VACUUM (ANALYZE) listings;
ANALYZE listing_attribute_values;
ANALYZE listing_attribute_options;
ANALYZE listing_media;

SELECT format('Made %s listings. Generated ones on the site now: %s, of %s on it in all.',
              :count,
              (SELECT count(*) FROM listings l JOIN users u ON u.id = l.created_by_user_id
               WHERE u.auth_subject LIKE 'dev|generated-%' AND l.status = 'ACTIVE'),
              (SELECT count(*) FROM listings WHERE status = 'ACTIVE')) AS done \gset
\echo :done

-- ---------------------------------------------------------------- credits

-- The photos above that aren't the sample listings', by category: each one's page,
-- unsplash.com/photos/<id>, and its photographer, unsplash.com/@<name>.
--   aksesore: K62u25Jk6vo (giorgiotrovato), lSl94SZHRgA (kiranck123),
--       KAx7H0jxo28 (ls8), Xbjf9nnEQ4s (kisetsuco), zVycYmcblDY (michealcopley03),
--       2loKxdi6Hmo (palon)
--   aksesore-per-kafshe: 9bjRS9MKN6c (brettwharton), TDTwMuHPHp4 (pozva),
--       Vct2D4rZfmc (deleece), wim1htSwf8Y (maur1ts), GUqczWy1bns (dexezekiel),
--       OBu_nabEpAw (olesborys)
--   aparate-fotografike: gGgoDJRD2WM (hansonluu), N10auyEVst8 (sharegrid),
--       McztPB7Uqx8 (nate_dumlao), ny6tO4ItOEY (williamauto)
--   audio-dhe-video: 2DrK_hrVsrk (garbera), EOG8BeJfz4I (shotaspot),
--       KpSdMeuaHRg (mpho_mojapelo), vEbmGrevc8s (yanli247), km3i2DsbUd4 (icedcocoa),
--       FqTXd5e_zjY (introspectivedsgn)
--   automjete-tregtare: 3vlGNkDep4E (markuswinkler), OL84QWu3Ong (matreding),
--       7e8QdaunUEI (autotraderuk), dQJiFROxTvg (aleksi_p),
--       3D_e8UPMw_E (autotraderuk), PWb9OLVzGfg (francotheshooter)
--   canta: tcVH_BwHtrc (ikredenets), oCXVxwTFwqE (arnosenoner),
--       CtOA9wbFAdQ (laurachouette), xzrJCS4grC4 (aureliaduboisphotographie)
--   dekor: xnLHz7NJzRA (elli19), EwKX1wH8Tyk (giorgiotrovato), 0zERrbey8XM (williamk),
--       WUrXahlyjBo (mathildelangevin), zeGT9j4ltRA (yana_hurska),
--       tuJtzghMuEw (angelabaileyy)
--   dyqane: xps0EgdThbI (stadinstudio), iVINr8-ZFmY (landolakesinc),
--       Uw9wQp0uv7I (marcusloke), XaEKRwPeXTw (mana5280)
--   energji-dhe-gjeneratore: 513dBrMJ_5w (publicpowerorg), SFNUwmLEOis (sasun1990),
--       XGAZzyLzn18 (publicpowerorg), 2SfssudtyIA (benjopen)
--   fitnes: 8MCy6GeU490 (palak_pitroda), YSIJUXZI48o (mo_khalid222),
--       VJ2s0c20qCo (samuelgirven), xc_uXmo7qBs (viktorr_aj)
--   foto-dhe-video: -80J-7NQNfQ (dvhoang2106), WroEwJ-0xms (gnhearingglobal),
--       rfcK18wpQ-4 (gnhearingglobal), WxM465oM4j4 (reinhartjulian)
--   fustane: LR99Ho2eIec (apfelstrudel), fdcNKs6ek_4 (donguyenkhanhs),
--       tyfqOL1FAQc (1rfn), p7I07kuPSyU (suheylburak)
--   garazhe: AHnhdjyTNGM (sooprun), LegXEtjnwTw (jjemanuel), rzs-Qd64vZQ (kaprion),
--       MYArovblj9U (hypernature)
--   garsoniera: p_xTs-dajHk (curaits), dEuvqNbwRuw (aquilionproperty),
--       wrmg3RP_OyA (alexphotogram)
--   goma-dhe-disqe: -gqb3xbGa5Y (shadrach_warid), pi7Jlnh5nuY (tuccera),
--       Y_251QYX55Y (introspectivedsgn), 9e_ssIz4jB4 (masonjonesphoto),
--       60ZSTXNgXgM (luca_nicoletti), vVLrfAzmWYw (imthaz)
--   gps-dhe-navigatore: HJjc4uczOJU (introspectivedsgn), ng0pmUvygyM (sandratansh),
--       pvcfz7VCc3I (alvarordesign), V3iAg86XwBo (leoonliang), VSoUXfHm36U (domrvg),
--       DWcgojbxGMY (swansway)
--   instrumente-muzikore: phS37wg8cQg (simonweisser), QW2rkf7NQQM (metropolitanmuseum),
--       MEL-jJnm7RQ (sickhews), YStboKiFPVw (jerroams)
--   kamping-dhe-peshkim: a3e7yEtQxJs (iamthedave), 9md3TZpx-EY (liosha_shyp),
--       yBGaJDSC7Is (mazinomron)
--   kepuce: 1t9fMwjWvZE (aucoeurdumonstre), kP6knT7tjn4 (usama_1248)
--   kopsht: Q0IrpWQIMR4 (robinwersich), mYKv1ubOY_g (anastasioskara),
--       KLcaQxlmhtk (ro_ka), 1_yycyoMT6g (igcorreia)
--   mace: gKXKBY-C-Dk (madhatterzone), UxWy8Un2Mjc (ikemura),
--       f6ZQIQ6wAmE (gaellemarcel), 7AIDE8PrvA0 (l_oan)
--   makineri-bujqesore: DdcWKBbJeEI (landolakesinc), yJDZTDeHeG8 (viramedio),
--       zTY0RQgqb5g (sharonros)
--   makineri-industriale: pWUyHVJgLhg (homaappliances), HpPmiduLDC0 (klalit),
--       SLIFI67jv5k (rozetsky), 5hPe-Tr2wog (homaappliances)
--   makineri-ndertimi: OyI1n2iR52c (jiorjegfirjg), n6dpoztQ2FY (fokila),
--       nkxB5Ab-ONY (billyfreeman)
--   mbushes-per-makina-elektrike: xJLsHl0hIik (chuttersnap), EGKP0FpNSzI (achrislive),
--       xfaYAsMV1p8 (chuttersnap), 2jRNVr0ac7s (zaptec), LP9D8zD4Xmw (privategrid),
--       TyVsfIliUAg (sigmund)
--   mesime: TXxiFuQLBKQ (thisisengineering), RDf6Cvns88Q (silverkblack),
--       5fNmWej4tAA (amstram), 0eNgbGtsXd0 (silverkblack)
--   motocikleta: 7GeprSfVqLQ (maxitin), -5rcxih1e44 (gijsparadijs),
--       Ptx8G07I6xI (zacwolff), EDY1KzDP0a4 (purzlbaum),
--       wlQDgb79UAk (tarasovaanastasiia), yOKTYm8thtw (lgnwvr)
--   ndertim-dhe-lyerje: -WK5bA14jt0 (kderrick049), 57ldq9age5U (ernys)
--   ora: 3XbFQkhWKIA (runie), 12V36G17IbQ (ptaylor_), 4R_WEmhx8og (fabianheimann)
--   pastrim: YWS51jdrfdA (silverkblack), 2NcTLdFHpH8 (silverkblack),
--       5TXz228u4eo (giorgiotrovato)
--   peshq: V7SKRhXskv8 (tangzhengtao), yeTvRuf9fwg (tifitis),
--       avwdcJklaCA (hieuanhcauam), rEM3cK8F1pk (rachelhisko)
--   pjese-kembimi: FYGMJfE4yyY (zoshuacolah), REXWw_IW0wo (enginakyurt),
--       xCLneDUHt4Q (nazariykovalov), w8RT-3E61Ys (mrsamuelelias),
--       ii4XEyJEm_I (toby_hall28), qy27JnsH9sU (samloyd)
--   pjese-te-tjera: s1sLdHL78Hs (lgnwvr), EcE9dFfXwwE (tekton_tools),
--       wSvYAXmWJQo (riiiis), aIIx4RpP0W8 (wesleyphotography),
--       vPW4W2weFqo (vonshnauzer), 2nSdQEd-Exc (shs521)
--   pune-call-center: wnpf3Q5pkXA (baljkann4), BeVGrXEktIk (machec),
--       QBpZGqEMsKg (frantic)
--   pune-it: UikYLDQj9_I (silverkblack), DI93sfGJfBU (1703adrq)
--   pune-ndertim: 9gCRnfwSkwo (gnhearingglobal), Dbn68nqrA14 (tisfortroy)
--   pune-shendetesi: Y-3Dt0us7e0 (pattybphoto), _zbqco3m7dA (silverkblack),
--       -RtJ388r3xo (navymedicine), O8ekTIhHBRY (lisa_marie_theck)
--   pune-shitje: -gkndM1GvSA (simonkadula), tE6th1h6Bfk (blakewisz),
--       2cGIkBSspFM (spotonpos), HaZhm6xIUgs (sql)
--   pune-transport: 3FuQylRWQeg (ftodne), mVqTumQH-c0 (zetong),
--       ucuOscdCaO4 (josiahfarrow)
--   rimorkio: uXTkE1oiPs8 (heyquilia), -DZkYpnr9Y8 (awcreativeut),
--       3PiJKN2poPg (markuswinkler), CfhtS2nj95c (benjaminzanatta),
--       aLf3S7eg7-g (markusspiske), P9xRyXAzGls (jhc)
--   sport-te-tjera: MNS5u2Dx2L4 (wu_hua_jiang), COyw36wUP4g (astuardom),
--       i0kB5B9J8Ds (likeffer)
--   sporte-dimerore: bY7uZyolzGk (lgnwvr), Pf6e3o0GL4M (mattpunsplash),
--       VXiG4N229uY (nicolaiberntsen)
--   styling-dhe-pajisje-sportive: pDU7mk2-PDQ (theinthanzaw6102), tfnjhrb5EKI (vikraw),
--       HHHcqDlj6TM (jaym30), 3Fr60KtfvAw (ibrahim_syed), MBmGZmMrfNU (__menglong),
--       yg5NTGN8bR8 (shaturniano_photography)
--   tableta: RVHtHpedipQ (howardbouchevereau), BAawNji5kHs (zelebb),
--       8m5fv2W0O3M (bunnystudio)
--   tapete-makine: xNRbX3q7FxY (obionyeador), molQiGqrfIY (lonestarexotic),
--       8kuYZRR2MPc (divaris), bVU8L0yvyMY (elmartinbaron),
--       m6brRzQ-UjI (jalalphotography), 3MYLsv07nXo (justalifein)
--   te-tjera: qBrF1yu5Wys (jerry_318), eypQOcZa5R4 (jonathankslim),
--       H5Ffv4I5ZMI (chrishardyphotography), zoyBqT7ytLU (trinhhuyhung)
--   televizore: EOQhsfFBhRk (glenncarstenspeters), UBhpOIHnazM (ajeetmestry)
--   transport-dhe-transferime: qBD1__CH_MI (hdbernd), axfG4brgts0 (djw1999q),
--       ZhNYKwjRMh4 (abalashevsky)
--   truall: ImYFLuZljdE (hangocthuy), LILMme-32xk (valentinsalja),
--       hnA2FdyqrXk (alikose), vqNYntkDMCw (hdbernd)
--   vila-dhe-shtepi: 1XRV7Z4lhGo (rstar50), y_JNknYLtfs (rstar50),
--       U6Q6zVDgmSs (liujs), RykkS_DVhPQ (qy_liu)
--   zogj: 6GHL995JYNU (floatflower), 3osGqRRtQBE (krizphoto), rEPNx7sl1DI (happysurd),
--       lylCw4zcA7I (imandrewpons)
--   zyra: pt0nGH-NvoA (aalolens), 6fA3wftnmGc (insungpandora),
--       DyFjxmHt3Es (joselosada), B74lBYC3PXI (___atmos)
