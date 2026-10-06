-- Subcategories. Most categories told their kinds apart with a "type" property (Prona's
-- property_type: apartment, studio, house...). Each of those kinds becomes a subcategory of its
-- own (/c/apartamente), and the listings move into theirs. The property stays on the listings,
-- searchable ("telefon" still finds a phone), but it is no longer a filter or shown on the page:
-- the subcategory says it. Automjete also gets motorcycles, commercial vehicles and parts.

-- The option of the old "type" property a subcategory stands for. Listings moving in, and new
-- listings posted there, carry that option, so search and older code keep working.
ALTER TABLE categories ADD COLUMN replaces_option_id uuid REFERENCES attribute_options(id);

CREATE TEMPORARY TABLE kinds (parent_slug, attribute_code, value_code, name, slug) ON COMMIT DROP AS
VALUES
    ('elektronike', 'electronics_type', 'PHONE', 'Telefona', 'telefona'),
    ('elektronike', 'electronics_type', 'LAPTOP', 'Laptopë', 'laptope'),
    ('elektronike', 'electronics_type', 'DESKTOP', 'Kompjuterë', 'kompjutere'),
    ('elektronike', 'electronics_type', 'TABLET', 'Tableta', 'tableta'),
    ('elektronike', 'electronics_type', 'TV', 'Televizorë', 'televizore'),
    ('elektronike', 'electronics_type', 'CONSOLE', 'Konsola lojërash', 'konsola-lojerash'),
    ('elektronike', 'electronics_type', 'CAMERA', 'Aparate fotografike', 'aparate-fotografike'),
    ('elektronike', 'electronics_type', 'AUDIO', 'Audio dhe kufje', 'audio-dhe-kufje'),
    ('prona', 'property_type', 'APARTMENT', 'Apartamente', 'apartamente'),
    ('prona', 'property_type', 'STUDIO', 'Garsoniera', 'garsoniera'),
    ('prona', 'property_type', 'HOUSE', 'Vila dhe shtëpi', 'vila-dhe-shtepi'),
    ('prona', 'property_type', 'SHOP', 'Dyqane', 'dyqane'),
    ('prona', 'property_type', 'OFFICE', 'Zyra', 'zyra'),
    ('prona', 'property_type', 'LAND', 'Truall', 'truall'),
    ('prona', 'property_type', 'GARAGE', 'Garazhe', 'garazhe'),
    ('shtepi-dhe-kopsht', 'home_type', 'FURNITURE', 'Mobilje', 'mobilje'),
    ('shtepi-dhe-kopsht', 'home_type', 'APPLIANCE', 'Elektroshtëpiake', 'elektroshtepiake'),
    ('shtepi-dhe-kopsht', 'home_type', 'GARDEN', 'Kopsht', 'kopsht'),
    ('shtepi-dhe-kopsht', 'home_type', 'DECOR', 'Dekor', 'dekor'),
    ('pune', 'job_field', 'HOSPITALITY', 'Turizëm dhe gastronomi', 'pune-turizem'),
    ('pune', 'job_field', 'CONSTRUCTION', 'Ndërtim', 'pune-ndertim'),
    ('pune', 'job_field', 'HEALTH', 'Shëndetësi', 'pune-shendetesi'),
    ('pune', 'job_field', 'IT', 'IT', 'pune-it'),
    ('pune', 'job_field', 'SALES', 'Shitje', 'pune-shitje'),
    ('pune', 'job_field', 'TRANSPORT', 'Transport', 'pune-transport'),
    ('pune', 'job_field', 'CALL_CENTER', 'Call center', 'pune-call-center'),
    ('sherbime', 'service_type', 'REPAIRS', 'Riparime', 'riparime'),
    ('sherbime', 'service_type', 'MOVING', 'Transport dhe transferime', 'transport-dhe-transferime'),
    ('sherbime', 'service_type', 'CLEANING', 'Pastrim', 'pastrim'),
    ('sherbime', 'service_type', 'LESSONS', 'Mësime', 'mesime'),
    ('sherbime', 'service_type', 'PHOTO', 'Foto dhe video', 'foto-dhe-video'),
    ('sherbime', 'service_type', 'BUILDING', 'Ndërtim dhe lyerje', 'ndertim-dhe-lyerje'),
    ('veshje-dhe-aksesore', 'fashion_type', 'OUTERWEAR', 'Xhaketa dhe pallto', 'xhaketa-dhe-pallto'),
    ('veshje-dhe-aksesore', 'fashion_type', 'SHOES', 'Këpucë', 'kepuce'),
    ('veshje-dhe-aksesore', 'fashion_type', 'DRESSES', 'Fustane', 'fustane'),
    ('veshje-dhe-aksesore', 'fashion_type', 'BAGS', 'Çanta', 'canta'),
    ('veshje-dhe-aksesore', 'fashion_type', 'WATCHES', 'Ora', 'ora'),
    ('veshje-dhe-aksesore', 'fashion_type', 'ACCESSORIES', 'Aksesorë', 'aksesore'),
    ('sport-dhe-hobi', 'sport_type', 'BICYCLES', 'Biçikleta', 'bicikleta'),
    ('sport-dhe-hobi', 'sport_type', 'FITNESS', 'Fitnes', 'fitnes'),
    ('sport-dhe-hobi', 'sport_type', 'MUSIC', 'Instrumente muzikore', 'instrumente-muzikore'),
    ('sport-dhe-hobi', 'sport_type', 'OUTDOOR', 'Kamping dhe peshkim', 'kamping-dhe-peshkim'),
    ('sport-dhe-hobi', 'sport_type', 'WINTER', 'Sporte dimërore', 'sporte-dimerore'),
    ('sport-dhe-hobi', 'sport_type', 'OTHER', 'Të tjera', 'sport-te-tjera'),
    ('kafshe-shtepiake', 'pet_type', 'DOG', 'Qen', 'qen'),
    ('kafshe-shtepiake', 'pet_type', 'CAT', 'Mace', 'mace'),
    ('kafshe-shtepiake', 'pet_type', 'BIRD', 'Zogj', 'zogj'),
    ('kafshe-shtepiake', 'pet_type', 'FISH', 'Peshq', 'peshq'),
    ('kafshe-shtepiake', 'pet_type', 'ACCESSORIES', 'Aksesorë për kafshë', 'aksesore-per-kafshe'),
    ('makineri-dhe-biznes', 'machine_type', 'CONSTRUCTION', 'Makineri ndërtimi', 'makineri-ndertimi'),
    ('makineri-dhe-biznes', 'machine_type', 'AGRICULTURE', 'Makineri bujqësore', 'makineri-bujqesore'),
    ('makineri-dhe-biznes', 'machine_type', 'ENERGY', 'Energji dhe gjeneratorë', 'energji-dhe-gjeneratore'),
    ('makineri-dhe-biznes', 'machine_type', 'INDUSTRY', 'Makineri industriale', 'makineri-industriale');

INSERT INTO categories (parent_id, name, slug, sort_order, default_currency, listing_types, uses_condition,
                        replaces_option_id)
SELECT parent.id, k.name, k.slug, o.sort_order, parent.default_currency, parent.listing_types,
       parent.uses_condition, o.id
FROM kinds k
JOIN categories parent ON parent.slug = k.parent_slug
JOIN attributes a ON a.code = k.attribute_code
JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = k.value_code;

-- Vehicles: cars kept their subcategory (and their properties); the rest gets one each.
INSERT INTO categories (parent_id, name, slug, sort_order, default_currency, listing_types, uses_condition)
SELECT parent.id, v.name, v.slug, v.sort_order, v.currency::currency_code, parent.listing_types, parent.uses_condition
FROM (VALUES ('Motoçikleta', 'motocikleta', 20, 'EUR'),
             ('Automjete tregtare', 'automjete-tregtare', 30, 'EUR'),
             ('Pjesë këmbimi', 'pjese-kembimi', 40, 'ALL')) AS v(name, slug, sort_order, currency)
JOIN categories parent ON parent.slug = 'automjete';

DO $$
BEGIN
    IF (SELECT count(*) FROM categories WHERE replaces_option_id IS NOT NULL) <> 53 THEN
        RAISE EXCEPTION 'expected a subcategory for each of the 53 kinds';
    END IF;
END
$$;

-- A category with subcategories only groups them: listings go into a subcategory.
UPDATE categories parent
SET allow_listings = false
WHERE EXISTS (SELECT 1 FROM categories child WHERE child.parent_id = parent.id);

-- The old "type" property: still searchable, no longer a filter or a detail on the page.
UPDATE category_attributes ca
SET filterable = false, show_in_card = false, show_in_details = false
FROM categories parent, attributes a
WHERE ca.category_id = parent.id AND ca.attribute_id = a.id
  AND (parent.slug, a.code) IN (SELECT DISTINCT parent_slug, attribute_code FROM kinds);

-- Each listing moves into the subcategory for its kind. Listings without one stay in the parent
-- until their seller picks a subcategory.
UPDATE listings l
SET category_id = sub.id
FROM listing_attribute_values v
JOIN categories sub ON sub.replaces_option_id = v.value_option_id
WHERE v.listing_id = l.id AND sub.parent_id = l.category_id;
