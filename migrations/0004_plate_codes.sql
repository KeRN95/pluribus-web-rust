-- The two-letter district codes that Albanian number plates carried from 1993 to 2011
-- ("TR 1234 A"). Listing cards show them over the photo, linking to the town's listings. The
-- 2015 reform replaced the 36 districts with 61 municipalities; each municipality gets the code
-- of the district it was part of, so Kamëz and Vorë are TR, as they were on the plates.

ALTER TABLE locations
    ADD COLUMN plate_code varchar(2) CHECK (plate_code ~ '^[A-Z]{2}$');

UPDATE locations l
SET plate_code = v.code
FROM (VALUES
    -- Qarku Berat
    ('berat', 'BR'), ('kucove', 'KV'), ('polican', 'SK'), ('skrapar', 'SK'), ('ura-vajgurore', 'BR'),
    -- Qarku Dibër
    ('diber', 'DI'), ('bulqize', 'BZ'), ('klos', 'MT'), ('mat', 'MT'),
    -- Qarku Durrës
    ('durres', 'DR'), ('kruje', 'KR'), ('shijak', 'DR'),
    -- Qarku Elbasan
    ('elbasan', 'EL'), ('belsh', 'EL'), ('cerrik', 'EL'), ('gramsh', 'GR'), ('librazhd', 'LB'),
    ('peqin', 'PE'), ('prrenjas', 'LB'),
    -- Qarku Fier
    ('fier', 'FR'), ('divjake', 'LU'), ('lushnje', 'LU'), ('mallakaster', 'MK'), ('patos', 'FR'),
    ('roskovec', 'FR'),
    -- Qarku Gjirokastër
    ('gjirokaster', 'GJ'), ('dropull', 'GJ'), ('kelcyre', 'PR'), ('libohove', 'GJ'), ('memaliaj', 'TP'),
    ('permet', 'PR'), ('tepelene', 'TP'),
    -- Qarku Korçë
    ('korce', 'KO'), ('devoll', 'DV'), ('kolonje', 'ER'), ('maliq', 'KO'), ('pogradec', 'PG'),
    ('pustec', 'KO'),
    -- Qarku Kukës
    ('kukes', 'KU'), ('has', 'HS'), ('tropoje', 'BC'),
    -- Qarku Lezhë
    ('lezhe', 'LE'), ('kurbin', 'LA'), ('mirdite', 'MR'),
    -- Qarku Shkodër
    ('shkoder', 'SH'), ('fushe-arrez', 'PU'), ('malesi-e-madhe', 'MA'), ('puke', 'PU'), ('vau-i-dejes', 'SH'),
    -- Qarku Tiranë
    ('tirane', 'TR'), ('kamez', 'TR'), ('kavaje', 'KJ'), ('rrogozhine', 'KJ'), ('vore', 'TR'),
    -- Qarku Vlorë
    ('vlore', 'VL'), ('delvine', 'DL'), ('finiq', 'DL'), ('himare', 'VL'), ('konispol', 'SR'),
    ('sarande', 'SR'), ('selenice', 'VL')
) AS v(slug, code)
WHERE l.location_type = 'MUNICIPALITY' AND l.slug = v.slug;

-- A slug typo above would leave a municipality without a code; stop instead.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM locations WHERE location_type = 'MUNICIPALITY' AND plate_code IS NULL) THEN
        RAISE EXCEPTION 'a municipality has no plate code';
    END IF;
END
$$;

-- The municipality a location is in: the location itself if it is one, or else the nearest
-- municipality above it. A town or neighbourhood added under a municipality then shows the
-- municipality's plate code, and the code links to the municipality's listings.
CREATE FUNCTION location_municipality(location uuid)
RETURNS uuid
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN (
    WITH RECURSIVE up AS (
        SELECT id, parent_id, location_type, 0 AS depth FROM locations WHERE id = location
        UNION ALL
        SELECT l.id, l.parent_id, l.location_type, up.depth + 1
        FROM locations l
        JOIN up ON l.id = up.parent_id
        WHERE up.location_type <> 'MUNICIPALITY' AND up.depth < 10
    )
    SELECT id FROM up WHERE location_type = 'MUNICIPALITY'
);

CREATE FUNCTION location_plate_code(location uuid)
RETURNS varchar
LANGUAGE sql
STABLE PARALLEL SAFE
RETURN (SELECT plate_code FROM locations WHERE id = location_municipality(location));
