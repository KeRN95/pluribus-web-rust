-- Map search: listings within a distance of a point on the map ("within 10 km of here").
--
-- PostgreSQL's own earthdistance extension (with cube, which it builds on) measures distances on
-- the globe and can use an index for "within X metres of a point", so no PostGIS is needed. A
-- listing is found by its own point (listings.latitude/longitude); for private sellers that point
-- is approximate, as the schema asks.

CREATE EXTENSION IF NOT EXISTS cube          WITH SCHEMA public;
CREATE EXTENSION IF NOT EXISTS earthdistance WITH SCHEMA public;

-- Where each municipality's seat town is, from Wikidata (public domain, CC0): the map opens on
-- the chosen town, and new listings without a point of their own can start from it.
UPDATE locations l
SET latitude = v.latitude, longitude = v.longitude
FROM (VALUES
    -- Qarku Berat
    ('berat', 40.704939, 19.949673), ('kucove', 40.803889, 19.914444), ('polican', 40.612222, 20.098056),
    ('skrapar', 40.504167, 20.227222), ('ura-vajgurore', 40.768890, 19.877780),
    -- Qarku Dibër (Dibër's seat is Peshkopi, Mat's is Burrel)
    ('diber', 41.683056, 20.428889), ('bulqize', 41.500000, 20.216667), ('klos', 41.499444, 20.094444),
    ('mat', 41.600000, 20.000000),
    -- Qarku Durrës
    ('durres', 41.313333, 19.445833), ('kruje', 41.510833, 19.792500), ('shijak', 41.345556, 19.567222),
    -- Qarku Elbasan
    ('elbasan', 41.113059, 20.081780), ('belsh', 40.983333, 19.883333), ('cerrik', 41.033333, 19.983333),
    ('gramsh', 40.866667, 20.183333), ('librazhd', 41.183333, 20.316667), ('peqin', 41.050000, 19.750000),
    ('prrenjas', 41.066667, 20.550000),
    -- Qarku Fier (Mallakastër's seat is Ballsh)
    ('fier', 40.716667, 19.550000), ('divjake', 40.996400, 19.532700), ('lushnje', 40.933333, 19.700000),
    ('mallakaster', 40.596418, 19.738380), ('patos', 40.684100, 19.619400), ('roskovec', 40.737500, 19.702220),
    -- Qarku Gjirokastër (Dropull's seat is Jorgucat)
    ('gjirokaster', 40.075830, 20.138890), ('dropull', 39.936389, 20.268333), ('kelcyre', 40.313000, 20.192000),
    ('libohove', 40.033333, 20.266667), ('memaliaj', 40.351670, 19.980280), ('permet', 40.233333, 20.350000),
    ('tepelene', 40.296500, 20.018600),
    -- Qarku Korçë (Devoll's seat is Bilisht, Kolonjë's is Ersekë)
    ('korce', 40.616667, 20.766667), ('devoll', 40.627500, 20.990000), ('kolonje', 40.333333, 20.683333),
    ('maliq', 40.709674, 20.696916), ('pogradec', 40.900000, 20.650000), ('pustec', 40.787778, 20.905000),
    -- Qarku Kukës (Has's seat is Krumë, Tropojë's is Bajram Curri)
    ('kukes', 42.075800, 20.423100), ('has', 42.196100, 20.414600), ('tropoje', 42.358217, 20.076011),
    -- Qarku Lezhë (Kurbin's seat is Laç, Mirditë's is Rrëshen)
    ('lezhe', 41.780500, 19.643400), ('kurbin', 41.635278, 19.713056), ('mirdite', 41.768910, 19.874740),
    -- Qarku Shkodër (Malësi e Madhe's seat is Koplik)
    ('shkoder', 42.068132, 19.512138), ('fushe-arrez', 42.062220, 20.016670), ('malesi-e-madhe', 42.213611, 19.436389),
    ('puke', 42.050000, 19.900000), ('vau-i-dejes', 42.000000, 19.633333),
    -- Qarku Tiranë
    ('tirane', 41.328333, 19.818056), ('kamez', 41.383333, 19.766667), ('kavaje', 41.183333, 19.550000),
    ('rrogozhine', 41.076390, 19.665280), ('vore', 41.393889, 19.654444),
    -- Qarku Vlorë
    ('vlore', 40.450000, 19.483333), ('delvine', 39.949444, 20.095833), ('finiq', 39.900000, 20.050000),
    ('himare', 40.116667, 19.733333), ('konispol', 39.658890, 20.181390), ('sarande', 39.875000, 20.010000),
    ('selenice', 40.530560, 19.635830)
) AS v(slug, latitude, longitude)
WHERE l.location_type = 'MUNICIPALITY' AND l.slug = v.slug;

-- A slug typo above would leave a municipality without a point; stop instead.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM locations WHERE location_type = 'MUNICIPALITY' AND latitude IS NULL) THEN
        RAISE EXCEPTION 'a municipality has no coordinates';
    END IF;
END
$$;

-- "Within X metres of a point" uses this index: earth_box() finds the candidates in it, and
-- earth_distance() then checks the exact distance. Queries must use the same expression,
-- ll_to_earth(latitude::float8, longitude::float8), and say latitude IS NOT NULL.
CREATE INDEX idx_listings_active_earth ON listings
    USING gist (ll_to_earth(latitude::float8, longitude::float8))
    WHERE status = 'ACTIVE' AND latitude IS NOT NULL;
