-- Every category's name one word where it can be, as the user asked on 2026-10-11 ("go through
-- every category and subcategory and try to make it 1 word"), for the tiles and the path, where two
-- words were cut short: "Konsola lojërash" is Konsola, "Goma / disqe" Rrota, "Makineri / biznes"
-- Makineri. "Të tjera" stays as it is, no one saying it in one word. The slugs, and with them every
-- address, stay as they were (/c/konsola-lojerash). A search still names a category by its old
-- name's words as well as its new one's (web/named.rs, FORMER_NAMES): "goma" is Rrota, "shtëpi me
-- qira" the houses, Vila. The words a listing is found by (search_extra) keep its category's old
-- name until the listing is saved again, as after migration 0028, so nothing needs making again,
-- and name_i18n's English stays, words for search. Two names are their old name's first word, where
-- a new word would have named searches about other things: Mbushës, the electric cars' chargers
-- ("karikues telefoni" is a phone's), and Aksesorë, the pets' ("pajisje" is any equipment).
UPDATE categories SET name = renamed.name, updated_at = now()
FROM (VALUES
    ('shtepi-dhe-kopsht', 'Shtëpi'),
    ('veshje-dhe-aksesore', 'Veshje'),
    ('sport-dhe-hobi', 'Sport'),
    ('kafshe-shtepiake', 'Kafshë'),
    ('makineri-dhe-biznes', 'Makineri'),
    ('automjete-tregtare', 'Tregtare'),
    ('pjese-dhe-aksesore', 'Pjesë'),
    ('pjese-kembimi', 'Rezervë'),
    ('goma-dhe-disqe', 'Rrota'),
    ('audio-dhe-video', 'Multimedia'),
    ('gps-dhe-navigatore', 'GPS'),
    ('styling-dhe-pajisje-sportive', 'Tuning'),
    ('tapete-makine', 'Tapete'),
    ('mbushes-per-makina-elektrike', 'Mbushës'),
    ('vila-dhe-shtepi', 'Vila'),
    ('konsola-lojerash', 'Konsola'),
    ('aparate-fotografike', 'Fotoaparate'),
    ('audio-dhe-kufje', 'Audio'),
    ('pune-turizem', 'Turizëm'),
    ('pune-call-center', 'Operatorë'),
    ('transport-dhe-transferime', 'Transport'),
    ('foto-dhe-video', 'Fotografi'),
    ('ndertim-dhe-lyerje', 'Rikonstruksion'),
    ('xhaketa-dhe-pallto', 'Xhaketa'),
    ('instrumente-muzikore', 'Muzikë'),
    ('kamping-dhe-peshkim', 'Kamping'),
    ('sporte-dimerore', 'Ski'),
    ('aksesore-per-kafshe', 'Aksesorë'),
    ('makineri-ndertimi', 'Ndërtim'),
    ('makineri-bujqesore', 'Bujqësi'),
    ('energji-dhe-gjeneratore', 'Energji'),
    ('makineri-industriale', 'Industri')
) AS renamed(slug, name)
WHERE categories.slug = renamed.slug;
