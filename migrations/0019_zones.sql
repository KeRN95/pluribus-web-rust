-- Zones: the parts of a town people name when they say where a flat or a house is ("Blloku,
-- Tiranë"), as places under their municipality (location_type 'AREA'), for a property listing to
-- say its own (the posting form's "Zona"). A listing in a zone is in its town too: filtering by
-- the town takes in its zones (the city filter walks the places under it), cards and the listing
-- page name the town after the zone (location_municipality()), and search finds the listing by
-- either name (listing_search_extra()). The zones have no point of their own yet: a listing in
-- one stands on the map at its town's. The names are the ones listings use; more towns' zones can
-- follow the same way.

INSERT INTO locations (parent_id, location_type, name, slug)
SELECT town.id, 'AREA', zone.name, zone.slug
FROM (VALUES
    ('tirane', '21 Dhjetori', '21-dhjetori'),
    ('tirane', 'Ali Demi', 'ali-demi'),
    ('tirane', 'Allias', 'allias'),
    ('tirane', 'Astir', 'astir'),
    ('tirane', 'Babrru', 'babrru'),
    ('tirane', 'Blloku', 'blloku'),
    ('tirane', 'Bregu i Lumit', 'bregu-i-lumit'),
    ('tirane', 'Dajt', 'dajt'),
    ('tirane', 'Don Bosko', 'don-bosko'),
    ('tirane', 'Farkë', 'farke'),
    ('tirane', 'Fresku', 'fresku'),
    ('tirane', 'Kashar', 'kashar'),
    ('tirane', 'Kinostudio', 'kinostudio'),
    ('tirane', 'Kodra e Diellit', 'kodra-e-diellit'),
    ('tirane', 'Kodra e Priftit', 'kodra-e-priftit'),
    ('tirane', 'Kombinat', 'kombinat'),
    ('tirane', 'Komuna e Parisit', 'komuna-e-parisit'),
    ('tirane', 'Kopshti Botanik', 'kopshti-botanik'),
    ('tirane', 'Laprakë', 'laprake'),
    ('tirane', 'Liqeni i Thatë', 'liqeni-i-thate'),
    ('tirane', 'Lundër', 'lunder'),
    ('tirane', 'Medreseja', 'medreseja'),
    ('tirane', 'Myslym Shyri', 'myslym-shyri'),
    ('tirane', 'Paskuqan', 'paskuqan'),
    ('tirane', 'Pazari i Ri', 'pazari-i-ri'),
    ('tirane', 'Porcelan', 'porcelan'),
    ('tirane', 'Qendër', 'qender'),
    ('tirane', 'Rruga e Dibrës', 'rruga-e-dibres'),
    ('tirane', 'Rruga e Durrësit', 'rruga-e-durresit'),
    ('tirane', 'Rruga e Elbasanit', 'rruga-e-elbasanit'),
    ('tirane', 'Rruga e Kavajës', 'rruga-e-kavajes'),
    ('tirane', 'Sauk', 'sauk'),
    ('tirane', 'Selitë', 'selite'),
    ('tirane', 'Shkozë', 'shkoze'),
    ('tirane', 'Stacioni i Trenit', 'stacioni-i-trenit'),
    ('tirane', 'Tirana e Re', 'tirana-e-re'),
    ('tirane', 'Unaza e Re', 'unaza-e-re'),
    ('tirane', 'Vasil Shanto', 'vasil-shanto'),
    ('tirane', 'Xhamlliku', 'xhamlliku'),
    ('tirane', 'Yzberisht', 'yzberisht'),
    ('tirane', 'Zogu i Zi', 'zogu-i-zi'),
    ('durres', 'Currila', 'currila'),
    ('durres', 'Ish-Kënetë', 'ish-kenete'),
    ('durres', 'Plazh', 'plazh'),
    ('durres', 'Porto Romano', 'porto-romano'),
    ('durres', 'Qendër', 'qender'),
    ('durres', 'Shkëmbi i Kavajës', 'shkembi-i-kavajes'),
    ('durres', 'Spitallë', 'spitalle'),
    ('durres', 'Vollga', 'vollga'),
    ('vlore', 'Lungomare', 'lungomare'),
    ('vlore', 'Plazhi i Vjetër', 'plazhi-i-vjeter'),
    ('vlore', 'Qendër', 'qender'),
    ('vlore', 'Radhimë', 'radhime'),
    ('vlore', 'Skelë', 'skele'),
    ('vlore', 'Uji i Ftohtë', 'uji-i-ftohte'),
    ('vlore', 'Zvërnec', 'zvernec')
) AS zone (town, name, slug)
JOIN locations town ON town.slug = zone.town AND town.location_type = 'MUNICIPALITY';
