-- A category's name joins its two halves with "/" rather than "dhe" ("Shtëpi / kopsht", "Veshje /
-- aksesorë", "Pjesë / aksesorë"): shorter on the pills, the headings and the menus. The slugs, and
-- with them every address, stay as they were (/c/shtepi-dhe-kopsht). Search reads a "/" in a name as
-- it read "dhe" (web/named.rs). The words a listing is found by (search_extra) keep the name's "dhe"
-- until the listing is saved again; searches leave "dhe" out as a filler, and its other words are the
-- same, so nothing needs making again.
UPDATE categories SET name = replace(name, ' dhe ', ' / ') WHERE name LIKE '% dhe %';
