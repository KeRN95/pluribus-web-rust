# Marketplace

A MerrJep / FINN-style classifieds site for Albania. Rust (axum, sqlx, Askama) renders the pages on the server, Turbo and Stimulus make them feel instant in the browser, and PostgreSQL 15+ stores everything. The same pages can later be wrapped as iOS and Android apps with Hotwire Native.

## Running it locally

You need Docker and Rust 1.85 or newer. Start the database and the photo store, create your `.env`, and run the app, which applies the migrations at startup:

```bash
docker compose up -d
```

The photo store (Garage, see "Photos" below) needs a one-time setup after its first start, which creates its bucket and the app's key, the ones `.env.example` names:

```bash
sh dev/setup_garage.sh
```

```bash
cp .env.example .env
```

```bash
cargo run
```

Then open http://127.0.0.1:3000. On Windows, use `127.0.0.1` rather than `localhost`: `localhost` tries IPv6 first, which adds about 200 ms to every request through WSL.

To load the sample listings, from a bash shell. Their cover photos are free Unsplash photos, loaded from Unsplash's image CDN; the fixture credits each one.

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 < tests/fixtures/listings.sql
```

For a fuller site, also load 305 more listings across every category, each with a photo, a point on the map near its town, its seller's (fake) phone number, and the properties its category defines (a car's make, model, year, mileage, fuel, gearbox, body type and colour; a flat's rooms, area, floor and furnishing; and so on), so every filter has data to work on:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 < dev/sample_listings.sql
```

The tests need the database running; each test gets a fresh database of its own:

```bash
cargo test
```

**Windows:** Rust's MSVC toolchain needs Microsoft's C++ build tools for its linker (`link.exe`): Visual Studio Build Tools with the "Desktop development with C++" workload. Without them nothing links, so build and run in WSL instead.

## Project layout

| Path | Contents |
|---|---|
| `src/main.rs` | Startup: configuration, database pool, migrations, HTTP server |
| `src/web/` | Routes and page handlers: `browse.rs` (welcome, category, search and map pages), `account.rs` (signing in with a code), `post.rs` (posting and editing listings), `properties.rs` (a category's properties in the listing form), `filters.rs` (filters read from the URL, the filter form), `listings.rs` (a listing's page) |
| `src/db.rs` | Database queries |
| `src/sender.rs` | Sending sign-in codes: e-mail by SMTP, phone numbers by WhatsApp (Meta's Cloud API); a channel not set up writes to the log |
| `src/format.rs` | Albanian display formatting: prices, counts, dates, phone numbers |
| `templates/` | Askama templates, compiled into the binary and checked at compile time |
| `static/` | CSS, JavaScript and fonts; Turbo, Stimulus, Leaflet and the Geist font live in `static/vendor/`, so there's no Node or npm |
| `migrations/0001_marketplace_schema.sql` | Schema: tables, indexes, triggers |
| `migrations/0002_marketplace_reference_data.sql` | Albania's 12 counties and 61 municipalities, categories in Albanian with English translations, a sample Cars attribute model, promotion products |
| `migrations/0003_category_properties.sql` | Properties for every category, the kinds of listing each takes, exchange rates, and helpers for display and search |
| `migrations/0004_plate_codes.sql` | `location_municipality()`, which finds the municipality a place is in, and each municipality's district code from the 1993–2011 number plates (TR, DR, VL...; stored, not shown at the moment) |
| `migrations/0005_map_search.sql` | Radius search: the `cube` and `earthdistance` extensions, the index radius searches use, and a point for each municipality (its seat) |
| `migrations/0006_rename_patundshmeri_to_prona.sql` | Renames the property category from Patundshmëri to Prona (`/c/prona`) |
| `migrations/0007_sign_in_codes.sql` | Sign-in codes and sessions, for signing in with a phone number or an e-mail address |
| `migrations/0008_subcategories.sql` | Subcategories: each category's kinds (flats, houses... phones, laptops...) as subcategories, with the listings moved into them |
| `migrations/0009_seller_options.sql` | "Tjetër": options sellers type when a list lacks theirs (a car make or model, a phone brand), kept inactive until approved |
| `migrations/0010_post_before_sign_in.sql` | A listing posted before signing in waits for the code (`login_codes.listing_id`), as does a new account's name |
| `migrations/0011_paused_listings.sql` | Sellers can take a listing down for a while and put it back (`PAUSED`) |
| `migrations/0012_parts_and_accessories.sql` | Pjesë dhe aksesorë: car parts get kinds of their own, a third level (Pjesë këmbimi, Goma dhe disqe, GPS dhe navigatorë...), and say which vehicle they fit; a category's own name for a property (`category_attributes.label`) |
| `migrations/0013_car_makes_and_models.sql` | The makes and models sellers pick from: 42 makes common in Albania with their usual models; makes and models sellers had typed that are on the list join it |
| `migrations/0014_options_name.sql` | A car's equipment (ABS, air conditioning...) is called "Opsionet" |
| `migrations/0015_generated_titles.sql` | A car's title made from what it is (make, model, body type, colour), kept in `listings.generated_title` |
| `tests/` | Request tests (`web.rs`), the 71 schema checks (`schema_checks.sql`, also run by `schema.rs`) and sample data (`fixtures/`) |
| `dev/sample_listings.sql` | 305 extra sample listings for the development database (not used by the tests) |
| `.sqlx/` | Cached query metadata, so the project builds without a database (set `SQLX_OFFLINE=true`) |
| `CHANGES.md` | What changed from the first version of the schema, and why |

## How the frontend works

- Every page is plain HTML rendered by the server, so it loads fast and search engines index it as it is.
- Turbo fetches pages and form responses in the background and swaps them in without a full reload. It also prefetches links on hover.
- Listing pages load the next page as the visitor scrolls near the end. A `<turbo-frame loading="lazy">` points at the next page, and the server answers that request with just the frame.
- Small updates come back as Turbo Streams. For example, the phone button ("Shfaq numrin") swaps in the phone number and counts the reveal.
- The little JavaScript there is lives in Stimulus controllers in `static/js/controllers/`, like the share button.
- The welcome page (`/`) asks what the visitor is looking for, in words only, in a pill like the message box in Claude's app with a round coral search button inside it, and searches every category and town; the results' filters narrow them. Under it, pills like the listing page's details lead straight to every main category, all at once, each with its icon (`SHORTCUTS` in `browse.rs`; a main category added later gets a box until it has one); they take as many lines as they need.
- Category pages (`/c/makina`) and search (`/search?q=golf`) share one handler and one filter form. Filters are ordinary query parameters (`/c/makina?vehicle_fuel=DIESEL&price_max=12000&sort=price_asc`), so any filtered page can be bookmarked, shared and indexed, and infinite scroll keeps them.
- A listing card shows the photo, the title, the price, and a details line that always starts with the city: "Tiranë · 2016 · 148 000 km · Naftë". The details line takes one line and ends in "…" when longer. In a search limited to a city (`city=vlore`), the cards in that city leave the city out, since it says nothing new, and show instead the first property the listing page has that neither the card nor the title already gives (a phone's brand: "Apple · 128 GB"; for a "Volkswagen Golf 7", the body type). The city is the listing's municipality, found by `location_municipality()`, so a town or neighbourhood added later under a municipality shows the municipality.
- A card is an `<article>` whose title link is stretched over the whole card with CSS (`.card-link::after`): a tap anywhere opens the listing, and screen readers name the link by its title. Under a mouse pointer, a rounded panel in the site's muted colour appears behind the card, a little larger than it (`.card::before`).
- On wide screens the filters are a sticky sidebar and a ticked option applies at once. On phones and tablets they are a section right under the "Filtrat" button, which opens and closes it, and apply together with its own button (`filters_controller.js`); a make picked there narrows the models to its own at once, as in the posting form (`narrow_controller.js`). A listener in `application.js` drops empty fields from the URL before Turbo fetches it.
- The map page (`/harta`) finds listings around a point. As on Facebook's, the pin stands fixed in the middle of the map and the map moves under it, starting in the middle of Tirana (`TIRANA` in `filters.rs`, which the map page's results follow until another point is set); "Vendos hartën", under the map, sets its middle as the point. A slider sets how far to look, any whole distance from 1 to 50 km (it moves slowly near the pin and faster further out, so its middle is 7 km; `map_radius.html`), and the results panel, a `<turbo-frame>`, reloads with what lies inside the circle, nearest first, each card saying how far away it is ("Tiranë · 850 m · 2+1 · 101 m²"). The address bar follows (`/harta?lat=41.32833&lng=19.81806&radius=10`), so a map search can be bookmarked and shared, and category and search pages take the same parameters. "Vendndodhja ime" moves the map to where the visitor is, if the browser may say; the distance and the place both wait for "Vendos hartën". The map has a dot for every result, up to 500 (`db::MAP_POINTS`), while the list loads 24 at a time. Zoomed out, results near each other show as one numbered bubble and a lone one as a small dot; neither takes taps, so they don't get in the way of moving the map. From zoom 15 (`CLOSE_ZOOM`, streets readable) each result is its own dot, and a tap on one opens its title and price. It is 3:2, three wide and two tall; the mouse wheel scrolls the page rather than zooming the map, and the + and − buttons, a pinch and a double tap zoom around the middle, so the pin stays where it is.
- Every category and search page has the same map above the results, in a card that always comes folded to its header ("Harta · Brenda 10 km nga pika", or "Harta · Kudo" without a point); the header opens it, and only then do Leaflet and the results' dots load (the dots from the same page with `harta=1`, which Turbo takes their `<turbo-frame>` out of). Its pin starts on the point set before, or on Tirana. "Vendos hartën" reloads the page for the map's middle and the distance, so the heading, the subcategories, the filters and the sort stay, the card comes folded again and the point shows as a chip; "Të gjitha", beside it, drops the point and searches everywhere again, as the chip's cross does. The filter form and the category menu carry the point along. Nothing links to the map page (`/harta`) since the welcome page's link went, but its addresses still work.
- `map_controller.js` draws the map with [Leaflet](https://leafletjs.com/) 1.9.4 (in `static/vendor/` with its BSD licence, 46 KB gzipped), which it fetches only when a map shows: on the map page, or once a category page's map card is opened. The tiles come from OpenStreetMap's tile server (see "Before production"); the dark theme darkens them with a CSS filter.
- Anyone can post a listing ("+ Shto njoftim", `/shto`) with just a phone number or an e-mail address: no password, and no code. The listing is published at once, at its town's point on the map, and buyers reach the seller through the phone button, a phone in a coral circle named "Shfaq numrin" for screen readers (an envelope, "Shfaq email-in", for sellers who signed in by e-mail), which shows the number with a button beside it that copies it (`copy_controller.js`). Sellers edit their listings from the listing page ("Ndrysho", seen only by them) or from "Njoftimet e mia" (`/njoftimet-e-mia`); an edit goes through the same checks, keeps the listing's place in "newest first", and only the listing's own seller, while it is active or reserved, can make it. Sellers can also take a listing down for a while ("Çaktivizo", status `PAUSED`, migration 0011): it leaves the site but stays on "Njoftimet e mia" with "Aktivizo" to put it back where it was. "Fshi" deletes it for good (status `DELETED`, after the browser asks to be sure). Both are on the listing page too, for its seller, and the status history records who did it. The form asks for the chosen category's own properties too (a car's make, model, year, mileage, fuel...; `web/properties.rs`), which reload in a turbo-frame (`/shto/fushat?category=makina`) when the category changes; models narrow to the chosen make, required properties and numeric limits (`validation_json`, and next year at most for a year) are checked, and the values feed the cards, filters and search like the sample listings' do. Where a list can't be complete (car makes and models, phone brands: `attributes.allow_other`), it ends with "Tjetër", which opens a box to type the option in. A typed option is saved as an inactive option (`attribute_options.added_by_user_id` says who added it): the listing shows it and search finds it, but the form and the filters don't offer it until a moderator approves it. Moderators are the phone numbers and e-mail addresses `MODERATORS` in `.env` names; signed in with one, "Njoftimet e mia" links to "Për miratim" (`/moderim`, `web/moderation.rs`), which lists what waits, makes before their models, with how many listings have each and who typed it. "Mirato" approves one, its spelling put right if need be, and the lists offer it after the names that come before it alphabetically; "Bashko" merges it into the listed one the seller meant, chosen already when the names are alike (pg_trgm), and its listings take that one, along with a typed make's typed models. Anyone else gets "not found" there. Typing an option that exists ("bmw", or a make someone else typed) reuses it. Posts can have up to 8 photos (see "Photos").
- Accounts come by themselves: someone who posts without being signed in types a phone number or e-mail address with the listing, which goes live at once under the account that contact has, or one made there and then, active, under the name typed ("Përdorues" without one; `db::poster_account`). Posting signs no one in, though, and its page says once that it is up and that changing it later takes signing in (the `posted` cookie). Signing in takes a six-digit code sent to the contact, and typing it back signs the seller in for 30 days (a `session` cookie; only a SHA-256 hash of the token is stored, likewise for the codes); so typing someone else's number can put a listing under it, but never opens their account. The account button beside the theme toggle (`/llogaria`) signs in, or creates an account with a name, or leads a signed-in seller to their listings. (Listings posted before this waited as a `DRAFT` for the code, `login_codes.listing_id`; an account still `PENDING` from then becomes active when it posts again.)
- Things on sale for at least 10 000 € or, about the same, 1 000 000 Lekë have "Llogarit kredinë" beside the price (`loan_controller.js`): a down payment, years and yearly interest give the monthly payment of an ordinary bank loan (equal payments), with the total paid and the interest. It works in the page alone; rents, wanted ads and "price on request" don't have it (`ListingDetail::loan_amount`).
- Every listing page shows its seller over the buttons (on a wide screen both in a card in a right-hand column, which stays in view as the page scrolls), with a small initial: the name, since when they are on the site and how many listings they have, linking to their profile with all their active listings: `/perdorues/{id}` for a private seller, `/biznes/{slug}` for a business. The category's path sits above the photo, as pills with a › between them.
- What keeps spam out without passwords: a listing posted without signing in belongs to the contact typed, which nothing proves (the price of posting without a code: anyone can type anyone's number, and buyers who tap the phone button see it), so a network (IP address) posts at most 20 listings an hour that way (in memory, `post.rs`); signing in proves the contact: a code works for 10 minutes and 5 guesses, and a contact gets one code a minute and five an hour, and a network twenty an hour, which also caps what a bot could spend on SMS; an account posts at most 5 listings a day, never the same title twice in a day, and at most 2 links per listing; a hidden field only bots fill in, and a form sent less than 3 seconds after it was shown, are refused; and forms posted from other sites are refused (`Origin` check, `SameSite=Lax` cookies). The limits are constants at the top of `account.rs` and `post.rs`.
- Every select is chosen from a list in the page, not the phone's own picker (a wheel on iPhones, a dialog on Android): categories, towns, the kind of listing, the condition, the price, the currency, the order, a car's make and model. `picker_controller.js` puts a button where the `<select>` was and opens a list under it, styled like the category menu, groups under their headings (a town under its county). A list with groups folds, towns under their county and models under their make, as does a category list (`data-picker-tree-value`): a group's or a category's arrow opens it, the list opens with only the path to the choice open, and a list of one group (the models of the make chosen) comes open; a list wider than a narrow field grows from the field's side nearer the middle of the screen; where a category can be chosen itself (the welcome form, the filters, the map), its name chooses it and its arrow opens it. The posting form's groups nest by their path ("Automjete › Pjesë dhe aksesorë"), the other lists by the levels `category_choices` gives them. The select stays, unseen, and is what the form sends; choosing sets it and fires its `change`, and the button follows the select however a script sets it (the kind of listing a category takes, its currency, a model that no longer fits its make), the list being built anew each time it opens. It takes the arrow keys (right and left open and close a category), Enter, Escape and a first letter; without JavaScript a select is just a select.
- The category page's menu, under its heading, looks the same: rows like the lists', the page's category in the site's colour, and a category with subcategories folds them in a `<details>` (no script needed), its name leading to it and its arrow opening it, the path to the page's category open (`MenuItem` in `browse.rs`).
- The import map in `templates/base.html` loads the JavaScript modules directly, with no bundler. To upgrade Turbo or Stimulus, replace its file in `static/vendor/` and update the import map.
- The site follows the device's light or dark setting until the visitor picks a theme with the header toggle. The choice is kept in `localStorage`, and a small inline script in `base.html` applies it before the first paint, so pages never flash the wrong theme.
- Everything is set in one typeface, [Geist](https://vercel.com/font), Vercel's: headings, prices and text differ only in weight and colour. It is a variable font, every weight in one file (29 KB, Latin subset, which covers Albanian), in `static/vendor/fonts/` with its licence (`OFL-Geist.txt`). The site serves it itself, so visitors make no extra connection and their IPs don't go to Google or Vercel, and `base.html` preloads it so text rarely waits for it. `app.css` names the weights it uses: light (300) for the welcome page's intro line, regular (400) for body text, medium (500) for chips and detail values, semibold (600) for headings, labels, card titles and buttons, bold (700) for prices, and heavy (800) for the logo.
- Phones get everything smaller: the root size drops from 16px to 14px below 720px (13.5px at 360px and under), and everything sized in `rem` follows. Form fields keep 16px text, because iPhones zoom into a focused field with smaller text, and get less padding instead.
- Turbo, Stimulus, Leaflet and the fonts carry their version in the file name, so browsers may cache them for a year. The site's own CSS and JavaScript are sent with `Cache-Control: no-cache`: browsers keep them but check for changes on each use, so a deploy shows up immediately. Turbo, though, keeps the stylesheet and scripts of the first page a visitor opens while they click around, so their URLs carry a fingerprint of the files (`app.css?v=8d6d43be`, from `web::asset_version()`) and Turbo tracks those tags (`data-turbo-track="reload"`): after a deploy, an open page reloads in full on its next click instead of running on with the old files. The fingerprint is worked out once per server start, so restart the server after changing CSS or JavaScript.

## Categories and their properties

Every category except Të tjera has subcategories, and listings go into those: the posting form groups them under their category, and a category page shows its subcategories as links and its listings with theirs (`/c/prona` lists every property, `/c/apartamente` flats only).

| Category | Subcategories |
|---|---|
| Automjete | Makina, Motoçikleta, Automjete tregtare, Pjesë dhe aksesorë |
| Prona | Apartamente, Garsoniera, Vila dhe shtëpi, Dyqane, Zyra, Truall, Garazhe |
| Elektronikë | Telefona, Laptopë, Kompjuterë, Tableta, Televizorë, Konsola lojërash, Aparate fotografike, Audio dhe kufje |
| Shtëpi dhe kopsht | Mobilje, Elektroshtëpiake, Kopsht, Dekor |
| Punë | Turizëm dhe gastronomi, Ndërtim, Shëndetësi, IT, Shitje, Transport, Call center |
| Shërbime | Riparime, Transport dhe transferime, Pastrim, Mësime, Foto dhe video, Ndërtim dhe lyerje |
| Veshje dhe aksesorë | Xhaketa dhe pallto, Këpucë, Fustane, Çanta, Ora, Aksesorë |
| Sport dhe hobi | Biçikleta, Fitnes, Instrumente muzikore, Kamping dhe peshkim, Sporte dimërore, Të tjera |
| Kafshë shtëpiake | Qen, Mace, Zogj, Peshq, Aksesorë për kafshë |
| Makineri dhe biznes | Makineri ndërtimi, Makineri bujqësore, Energji dhe gjeneratorë, Makineri industriale |

Most subcategories were a "type" property before (`property_type`, `electronics_type`...). Migration 0008 turned each of its options into a subcategory (`categories.replaces_option_id`) and moved the listings; the property stays on the listings, and on new ones posted in that subcategory, only so that search finds them by it ("telefon" finds the phones). `categories.allow_listings` is false for a category with subcategories.

Pjesë dhe aksesorë goes one level further, as on MerrJep: Pjesë këmbimi, Goma dhe disqe, Audio dhe video, GPS dhe navigatorë, Styling dhe pajisje sportive, Tapete makine, Rimorkio, Mbushës për makina elektrike and Të tjera (migration 0012; Pjesë këmbimi kept its address, `/c/pjese-kembimi`, and its listings). Nothing in the code counts levels: the category menu indents each level further, the posting form groups the kinds under "Automjete › Pjesë dhe aksesorë", and a category's page lists everything under it, however deep.

Links to a narrower category, the subcategory chips and the menu's entries under the page's category, keep the page's filters, since a subcategory has its parent's properties: "Audi, 2006" on Pjesë dhe aksesorë stays on in GPS dhe navigatorë (`Filters::narrowed_url`). Links up the tree or across it keep only the search words, the town and the map pin (`Filters::category_url`).

### Properties

Each category's properties are rows in the database, not code. A browse page builds its filters from the category's `filterable` properties, listing cards show the `show_in_card` ones after the city ("Tiranë · 2016 · 148 000 km · Naftë"), and the listing page shows the `show_in_details` ones. What `0003_category_properties.sql` sets up:

| Category | Properties |
|---|---|
| Makina | make, model (grouped by make), year, mileage, fuel, gearbox, body type, colour, options ("Opsionet"); the posting form shows them two to a line, each with its icon before its name, the options under them; its title is made from its make, model, body type and colour ("Mercedes-Benz S-Class Sedan, e zezë", `listings.generated_title`, kept by `listing_generated_title()` wherever `search_extra` is), the seller's own heading the description and the details leaving those four out |
| Prona | rooms (1+1, 2+1...), area, floor, furnishing, features (lift, parking, sea view...) |
| Elektronikë | brand, storage |
| Punë | schedule |
| Veshje dhe aksesorë | for whom, size |
| Makineri dhe biznes | year |
| Pjesë dhe aksesorë (0012) | the vehicle a part fits, none required: make and model from the cars' lists ("Marka e automjetit", "Modeli i automjetit"), and the years from and to |

The car makes and models are migration 0013's list: 42 makes common in Albania (Alfa Romeo to Volvo, Iveco and the Chinese BYD and MG included) with 556 models, in alphabetical order, numbers first. Their codes are the ones the posting form gives a name typed under "Tjetër" (`format::slug`, upper case, a model's after its make's: `VOLKSWAGEN_ID_BUZZ`), so a typed "id buzz" is the listed ID. Buzz. To add one, write a migration like 0013's, or approve one a seller typed.

To add a property, write a new migration:

```sql
INSERT INTO attributes (code, name, name_i18n, data_type, unit)
VALUES ('laptop_screen', 'Ekrani', '{"en": "Screen"}', 'NUMBER', 'inç');

INSERT INTO category_attributes (category_id, attribute_id, filterable, show_in_card, sort_order)
SELECT c.id, a.id, true, false, 40
FROM categories c, attributes a
WHERE c.slug = 'elektronike' AND a.code = 'laptop_screen';
```

How each type of property is offered as a filter:

| Type | Filter | In the URL |
|---|---|---|
| SELECT with up to 8 options | checkboxes; a listing matches any ticked option | `property_rooms=R21&property_rooms=R31` |
| SELECT with more options, or dependent on another (model under make) | dropdown; dependent options are grouped by their parent and narrow to the chosen parent, the others' groups hidden in the page for when it changes; an option under another parent filters nothing | `vehicle_make=BMW` |
| MULTISELECT | checkboxes; a listing must have every ticked option | `vehicle_features=ABS&vehicle_features=NAVIGATION` |
| NUMBER | from and to | `vehicle_year_min=2015&vehicle_mileage_max=150000` |
| BOOLEAN | one checkbox | `<code>=1` |
| a pair of NUMBERs, `<x>_from_year` and `<x>_to_year` (the years a part fits) | one year, the buyer's vehicle's ("Viti i automjetit"); a listing matches unless its years leave that year out, so one that doesn't give its years still shows | `fits_year=2015` |

TEXT and DATE properties appear on the listing page but not as filters.

- Subcategories inherit their parents' properties, from every level above them. The `effective_category_attributes` view resolves that, and where a subcategory configures a property its parent has too, the subcategory's settings win.
- `category_attributes.label` is a category's own name for a property, where the property's name would mislead: on a part, "Marka" would read as the part's own brand, so there it is "Marka e automjetit". It is inherited like the other settings; NULL means the property's name.
- Property codes become URL parameters, so don't name one `q`, `category`, `city`, `lat`, `lng`, `radius`, `type`, `condition`, `price_min`, `price_max`, `currency`, `sort` or `after`; a year pair's filter takes `<x>_year` too.
- A property whose code ends in `_year` can't be later than next year, and in a year pair the second can't come before the first.
- `categories.listing_types` lists the kinds of listing a category takes (sale, wanted, rent...); with more than one, the filter offers them. `categories.uses_condition` is false where new or used means nothing: property, jobs, services and pets.
- Searchable property labels reach full-text search through `listings.search_extra`. After saving a listing's values, run `UPDATE listings SET search_extra = listing_search_extra(id) WHERE id = $1`; that is how "telefon" finds an iPhone.
- `format_attribute_number(value, unit)` formats numbers for display: grouped with the unit when there is one ("148 000 km"), as they are when there isn't (a year).

## Photos

Sellers add up to 8 photos to a listing, on the posting and edit forms; the first is the cover, which cards show. When editing, each photo has "Hiq" to remove it (the next one becomes the cover). On the listing's page, each photo shows whole in a 4:3 frame, never cropped: where it doesn't fill the frame (beside a tall photo, above and below a wide one), the photo shows again behind it, blurred, from its card-sized copy. Several photos make a strip to swipe through one at a time, with a counter in its corner ("2 / 5") and, on screens with a mouse, arrows (`gallery_controller.js`); one photo is just the photo.

- **In the browser** (`photos_controller.js`): photos can be picked a few at a time, as phones offer them: each pick adds to the ones before (a file input alone keeps only the last pick), each new one shows as a preview with "Hiq" to take it out, and big phone photos are made smaller before they go (longest side 2000 px, JPEG), so posting doesn't send megabytes over mobile data.
- **On the server** (`src/media.rs`, `src/web/photos.rs`): each photo is decoded (JPEG, PNG or WebP; at most 15 MB as sent, 64 MB for the whole form), turned the way the camera held it, and written out again as JPEG in two sizes: 1600 px for the listing page and 600 px for cards. Writing it out again drops what phones keep inside photos, the GPS position they were taken at included. `listing_media` keeps the key, both URLs, the size and a SHA-256 of what was sent.
- **Storage**: an S3-compatible store. Locally that is [Garage](https://garagehq.deuxfleurs.fr/) in Docker (`compose.yaml`, `dev/garage.toml`, set up once by `dev/setup_garage.sh`); in production, Cloudflare R2. The app talks plain S3 to either (the `object_store` crate), with the `S3_*` settings in `.env`. Without them, photos are kept in memory, which is what the tests use.
- **Serving**: the site serves photos itself from `/media/photos/...`, with year-long caching (their names never change), so they work wherever the site does, through ngrok included.

**Moving to R2.** Create an R2 bucket and an API token with read and write on it, then set `S3_ENDPOINT=https://<account id>.r2.cloudflarestorage.com`, `S3_REGION=auto`, `S3_BUCKET` and the token's key id and secret. Copy the photos already in Garage across with [rclone](https://rclone.org/), with a remote for each (both of type `s3`; Garage's endpoint is `http://localhost:3900`, region `garage`):

```bash
rclone sync garage:marketplace-media r2:marketplace-media --progress
```

Nothing in the database changes: the photos keep their keys, and the site keeps serving them from `/media/...`.

## Database

The application applies the migrations at startup with `sqlx::migrate!()`, which embeds `migrations/` in the binary; `sqlx migrate run` does the same from the command line. sqlx stores a checksum of every migration it has run and refuses to continue if one changes, so add schema changes as new files (`migrations/0014_<description>.sql`) instead of editing the existing ones.

All tables live in the default `public` schema, next to the citext, pg_trgm, unaccent, cube and earthdistance extensions, so connections need no search-path setup. Don't take `public` off a connection's search path: without the citext operators, `WHERE email = $1` silently becomes a case-sensitive text comparison.

To apply and check the schema with psql instead:

```bash
psql -v ON_ERROR_STOP=1 --single-transaction -f migrations/0001_marketplace_schema.sql -f migrations/0002_marketplace_reference_data.sql -f migrations/0003_category_properties.sql -f migrations/0004_plate_codes.sql -f migrations/0005_map_search.sql -f migrations/0006_rename_patundshmeri_to_prona.sql -f migrations/0007_sign_in_codes.sql -f migrations/0008_subcategories.sql -f migrations/0009_seller_options.sql -f migrations/0010_post_before_sign_in.sql -f migrations/0011_paused_listings.sql -f migrations/0012_parts_and_accessories.sql -f migrations/0013_car_makes_and_models.sql -f migrations/0014_options_name.sql -f migrations/0015_generated_titles.sql
```

```bash
psql -v ON_ERROR_STOP=1 -f tests/schema_checks.sql
```

## What is included

- Users with external authentication (`auth_subject`) and verification history
- Private sellers and business/store accounts with members and roles
- Hierarchical categories and locations (Albania's counties and municipalities seeded), labelled in Albanian with translations
- Category-specific dynamic attributes, type-checked by the database, including dependent dropdowns (make → model)
- Listings with lifecycle, and automatic status and price history
- Photo/video metadata with a processing status and cover-image handling
- Public contact preferences
- Accent-insensitive full-text search (ë/ç), weighted title/attributes/description, fuzzy title and location matching
- Favorites, saved searches and alert preferences
- Buyer/seller chat, including shared business inboxes
- Reviews and reputation
- Listing promotions / bumps / featured ads
- Payment orders with line items, transactions and webhook idempotency
- Business subscription plans
- Abuse reports (listings, users, businesses, chat messages) and moderator or automated actions
- User blocking
- Notifications + device tokens + notification preferences
- Daily listing analytics
- Transactional outbox for search indexing and notifications
- Audit log

## Important architectural choices

### 1. Authentication is external

Passwords are deliberately not stored in this schema. `users.auth_subject` maps to Keycloak/Auth0/Cognito/etc. Staff roles (moderator, admin) come from the identity provider as well.

### 2. Dynamic category fields are normalized and type-checked

Do not create a `listings` table with hundreds of nullable columns such as `mileage`, `floor`, `screen_size`, `engine`, etc. Instead:

- `attributes`, `attribute_options`, `category_attributes`
- `listing_attribute_values`: one row per listing and attribute, for TEXT, NUMBER, BOOLEAN, DATE and SELECT
- `listing_attribute_options`: one row per chosen option, for MULTISELECT

The usual weakness of this design is that any value can end up anywhere. Here the database prevents that:

- `listing_attribute_values.data_type` must equal the attribute's type (composite foreign key to `attributes(id, data_type)`), and a CHECK then requires the matching value column. An attribute's type can't change while values exist.
- Options are referenced as `(attribute_id, option_id)`, so a listing can't store `vehicle_fuel = MANUAL`.
- A SELECT value is a single `value_option_id`, so the primary key allows one per listing. Only MULTISELECT uses `listing_attribute_options`.

When inserting a value, copy `data_type` from the attribute. Leave `listing_attribute_options.data_type` out of inserts; it defaults to `MULTISELECT`.

Still the application's job: which attributes a category has and which are required, `validation_json` ranges, and that the chosen model belongs to the chosen make (`attribute_options.parent_option_id`), and that a part's years don't end before they start.

Attributes are inherited down the category tree. A listing in Makina (Cars) gets the attributes of Makina and of Automjete (Vehicles), and the nearest category's settings win (query below). `categories.allow_listings = false` marks categories that only group subcategories.

### 3. Media belongs in object storage

Do not store image/video blobs in PostgreSQL. Store them in an S3-compatible store (Garage locally, Cloudflare R2 in production; see "Photos") and keep only keys/URLs/metadata in `listing_media`. Rows start as `PROCESSING`; only `READY` media is shown, after virus scanning and moderation. `sha256` finds the same photo reused across listings, a common scam signal.

### 4. Search can start in PostgreSQL

- PostgreSQL has no Albanian stemmer, so `sq_unaccent` is the `simple` configuration plus accent folding: "shtepi tirane" finds "Shtëpi me qira në Tiranë". Use the same configuration in queries.
- The search vector weights the title (A) above searchable attribute labels (B, from `listings.search_extra`) and the description (C), so `ts_rank` puts title matches first.
- `sort_at = greatest(published_at, bumped_at)` is the "newest first" key. A bump lifts a listing to the top, and newer listings then push it down. Featured listings get their own slot rather than being sorted into every page.
- Browse and search indexes are partial (`WHERE status = 'ACTIVE'`). After a year most rows are expired or sold, and this keeps the hot indexes small.
- Listings are priced in ALL and EUR. Filter and sort on `price_normalized`, never on `price_amount`. It holds the price in euros, computed with `price_in_eur(amount, currency)` from the `exchange_rates` table; price filters convert their bounds with the same function, so the two always agree. Free listings count as 0; "price on request" has no price and sorts last.
- Trigram indexes on unaccented titles and location names support "did you mean" and autocomplete ("korce" finds Korçë).
- Radius search ("within 10 km") uses the `cube` and `earthdistance` extensions rather than PostGIS. A listing's point is `listings.latitude`/`longitude`, and the partial GiST index `idx_listings_active_earth` covers the active listings that have one: a 5 km search around Tirana's centre takes about a millisecond (query below). Municipalities have points too, at their seats (from Wikidata, CC0).

This is enough for an MVP and a fairly large site: each example query below ran in under 10 ms against 200,000 listings. When traffic grows, mirror active listings into OpenSearch/Elasticsearch, fed from `outbox_events` so the index never misses a change.

### 5. Analytics are aggregated

`listing_daily_stats` stores daily counters rather than one database row per page impression. Buffer increments (e.g. in Redis) and flush them periodically, because an UPDATE per page view turns popular listings into lock hot spots. High-volume raw events should go to Kafka/ClickHouse/BigQuery/etc. if needed.

### 6. Users are anonymised, not deleted

Foreign keys follow one rule:

- Data that only matters to the user (favorites, saved searches, notifications, device tokens, preferences, verifications, blocks, memberships, chat read state) is deleted with them (`ON DELETE CASCADE`).
- Anything other people rely on (listings, messages, reviews, reports, moderation actions, payments, history) blocks a hard delete.

A user who never did anything can be deleted outright. Everyone else is anonymised in one transaction:

1. In `users`, set `email`, `phone_e164` and `avatar_url` to NULL, `display_name` to "Deleted user", `auth_subject` to `'deleted:' || id`, `status` to `DELETED` and `deleted_at` to `now()`. A CHECK constraint refuses a DELETED user who still has an e-mail address or phone number.
2. Delete their device tokens, notifications, notification preferences, saved searches, favorites, verifications, blocks and business memberships.
3. Set their listings to `DELETED`; delete their `listing_contacts` rows and their media, including the objects in storage.
4. Decide, with legal advice, what happens to their messages: kept for the other party under "Deleted user", or bodies blanked.
5. Keep payments (accounting retention), reports and moderation actions (for a limited period) and reviews (shown under "Deleted user").
6. Delete the account in the identity provider.

Financial rows are never cascade-deleted.

### 7. Payments are idempotent

`payment_orders` is one checkout, `payment_order_items` is what was bought (promotion for a listing, subscription period, fee) at the price paid, and `payment_transactions` records provider events. Every provider retries webhooks, so:

- `(provider, provider_order_id)` and `(payment_order_id, transaction_type, provider_reference)` are unique;
- each webhook is first inserted into `payment_webhook_events`, in the same transaction that processes it. If `ON CONFLICT DO NOTHING` returns no row, it was already handled.

`listing_promotions.payment_order_item_id` links a promotion to the item that paid for it (NULL when included in a plan). `listings.featured_until` and `bumped_at` are copied from active promotions for fast sorting. The other promotion types (HIGHLIGHT, HOMEPAGE, CATEGORY_TOP) need the same treatment or a lookup in `listing_promotions`.

## Conventions

- **IDs** come from `uuid_v7()`: time-ordered, so inserts append to the index instead of scattering across it. On PostgreSQL 18+, use the built-in `uuidv7()`. A v7 id reveals its creation time.
- **Listing URLs** use `public_no`: `/listing/12345-vw-golf-7`. The slug is cosmetic and not unique.
- **Enumerations** are `varchar` + CHECK: easy to extend in a migration, and they map to Rust enums by name.
- **Language**: labels (`name`, `label`, `description`) and category slugs (`automjete`, `makina`) are Albanian. Other languages go in the `*_i18n` columns, e.g. `{"en": "Vehicles"}`. Read them with `coalesce(name_i18n ->> :lang, name)`, which falls back to Albanian.
- **Currency**: listing prices default to lekë (`ALL`). `categories.default_currency` preselects euros for cars and property, which Albanian sellers price in EUR.
- **Timestamps** are `timestamptz`, and `updated_at` is set by trigger. The history tables use `clock_timestamp()` so several changes made in one transaction stay in order. Compute calendar days (`listing_daily_stats.stat_date`, daily digests) in `Europe/Tirane`, not UTC, or set the database default with `ALTER DATABASE <name> SET timezone = 'Europe/Tirane'`.
- **History is automatic**: triggers write `listing_status_history` and `listing_price_history`, so don't insert those rows yourself. To record who changed a status and why, run `SELECT set_config('app.user_id', $1, true), set_config('app.status_reason', $2, true);` earlier in the same transaction.
- **Chat**: start conversations with `INSERT ... ON CONFLICT (listing_id, buyer_user_id)` (below). A message's sender must be a participant, so add business members as participants when they open a thread.
- **Phone numbers** are E.164 (`+355691234567`) and currencies are upper-case ISO 4217 codes (`e164_phone` and `currency_code` domains).

## Using it from Rust (sqlx)

- **Checked queries:** use `query!` / `query_as!` for fixed queries; they're checked against the real schema when you compile. Set `DATABASE_URL` for local builds, and commit the `.sqlx` folder that `cargo sqlx prepare` writes so builds without a database still work.
- **Search page:** build it with `QueryBuilder`, adding a condition and a bound value for each filter the user set. These queries aren't checked at compile time, so cover them with tests.
- **Types:** enable sqlx's `uuid`, `time` (or `chrono`), `rust_decimal` and `json` features, so `uuid`, `timestamptz`, `numeric` and `jsonb` map to `Uuid`, `OffsetDateTime`, `Decimal` and `Json<T>`. Never select `search_vector`, which has no Rust type; only filter on it. If a macro rejects the `e164_phone` or `currency_code` domains, cast to text in SQL (`$1::text`).
- **Nullability:** the macros infer whether each column can be NULL and sometimes guess wrong, for example after a LEFT JOIN. Override with `AS "name!"` (never NULL) or `AS "name?"` (may be NULL).
- **IDs:** columns default to `uuid_v7()`. If you'd rather create IDs in Rust, `Uuid::now_v7()` from the `uuid` crate produces the same kind.
- **Tests:** `#[sqlx::test]` gives each test a fresh database with `migrations/` applied. `tests/schema_checks.sql` is a psql script, and cargo ignores `.sql` files in `tests/`.

## Typical listing creation flow

1. User selects a category (one with `allow_listings = true`).
2. Backend loads the category's attributes, including those inherited from parent categories.
3. User fills common listing fields and required category attributes, and can put a pin on a map for the listing's point. Without one, use the town's point (`locations.latitude`/`longitude`): a listing with no point never appears in map searches. The listing's `listing_contacts` row holds the number buyers see behind the phone button: by default the seller's own, or the business's `public_phone`. Without one, the listing page has no phone button.
4. Backend validates values against `validation_json` and the options, including make → model.
5. Listing is inserted as `DRAFT` (the price may still be missing) or `PENDING_REVIEW`, with `created_by_user_id`. Attribute values are inserted with their `data_type`, then `search_extra = listing_search_extra(id)` copies the searchable labels into full-text search, and `price_normalized = price_in_eur(price_amount, currency)`.
6. Images are uploaded to object storage and rows are inserted into `listing_media` as `PROCESSING`; a worker marks them `READY` after scanning.
7. Moderation or automatic rules approve it (`moderation_actions.moderator_user_id` is NULL for automatic decisions).
8. Listing becomes `ACTIVE` with `published_at` and `expires_at`, and an outbox event is written for search indexing and saved-search alerts.

## Query examples

Attribute and option ids are reference data: cache them by code in the application and pass the ids. Placeholders like `:category_ids` stand for bound parameters (`$1`, `$2`, … in sqlx).

### Browse a category, newest first

```sql
SELECT l.id, l.public_no, l.slug, l.title, l.price_amount, l.currency, l.sort_at
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.category_id = ANY(:category_ids)                -- the category and its descendants
  AND (l.sort_at, l.id) < (:last_sort_at, :last_id)     -- omit on the first page
ORDER BY l.sort_at DESC, l.id DESC
LIMIT 30;
```

Keyset pagination costs the same on page 50 as on page 1. OFFSET gets slower with every page, and shows duplicates when a bump reorders listings between page loads.

### Sort by price

Listings without a price come last in both directions, so a missing price counts as infinitely high (cheapest first) or low (most expensive first). That keeps the keyset a plain row comparison:

```sql
SELECT l.id, l.title, l.price_normalized
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.category_id = ANY(:category_ids)
  AND (coalesce(l.price_normalized, 'Infinity'), l.id)
      > (coalesce(:last_price, 'Infinity'), :last_id)   -- omit on the first page
ORDER BY coalesce(l.price_normalized, 'Infinity'), l.id
LIMIT 30;
```

Most expensive first: `'-Infinity'`, `<` and `DESC`.

### Within a radius, nearest first

```sql
SELECT l.id, l.title,
       earth_distance(ll_to_earth(:lat, :lng), ll_to_earth(l.latitude::float8, l.longitude::float8)) AS meters
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.latitude IS NOT NULL
  AND earth_box(ll_to_earth(:lat, :lng), :radius_m)
      @> ll_to_earth(l.latitude::float8, l.longitude::float8)          -- the index: a box around the circle
  AND earth_distance(ll_to_earth(:lat, :lng), ll_to_earth(l.latitude::float8, l.longitude::float8))
      <= :radius_m                                                     -- the circle itself
  AND (earth_distance(ll_to_earth(:lat, :lng), ll_to_earth(l.latitude::float8, l.longitude::float8)), l.id)
      > (:last_meters, :last_id)                                       -- omit on the first page
ORDER BY meters, l.id
LIMIT 30;
```

The index only applies when the query repeats its expression, `ll_to_earth(latitude::float8, longitude::float8)`, and its condition, `status = 'ACTIVE' AND latitude IS NOT NULL`, word for word.

### Featured slot

```sql
SELECT l.id, l.title
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.category_id = ANY(:category_ids)
  AND l.featured_until > now()
ORDER BY random()
LIMIT 4;
```

### Text search

```sql
SELECT l.id, l.title, l.sort_at
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.search_vector @@ websearch_to_tsquery('sq_unaccent', :query)
ORDER BY l.sort_at DESC, l.id DESC
LIMIT 30;
```

For best-match ordering, use `ORDER BY ts_rank(l.search_vector, websearch_to_tsquery('sq_unaccent', :query)) DESC`.

Fuzzy title matching ("did you mean"): `WHERE l.status = 'ACTIVE' AND immutable_unaccent(l.title) % immutable_unaccent(:query)`.

### Attribute filters

One EXISTS per filter, so filters combine with AND and a listing never appears twice:

```sql
SELECT l.id, l.title, l.price_amount, l.currency
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.category_id = ANY(:category_ids)
  AND l.price_normalized BETWEEN price_in_eur(:min, :currency) AND price_in_eur(:max, :currency)
  AND EXISTS (SELECT 1 FROM listing_attribute_values v      -- NUMBER: mileage <= 150000
              WHERE v.listing_id = l.id
                AND v.attribute_id = :mileage_attribute_id
                AND v.value_number <= 150000)
  AND EXISTS (SELECT 1 FROM listing_attribute_values v      -- SELECT: diesel or hybrid
              WHERE v.listing_id = l.id
                AND v.attribute_id = :fuel_attribute_id
                AND v.value_option_id = ANY(:fuel_option_ids))
  AND EXISTS (SELECT 1 FROM listing_attribute_options o     -- MULTISELECT: has navigation
              WHERE o.listing_id = l.id
                AND o.attribute_id = :features_attribute_id
                AND o.option_id = :navigation_option_id)
ORDER BY l.sort_at DESC, l.id DESC
LIMIT 30;
```

For "has all of these features", add one EXISTS per feature.

### Category subtree

```sql
WITH RECURSIVE subtree AS (
    SELECT id FROM categories WHERE slug = 'automjete'
    UNION ALL
    SELECT c.id FROM categories c JOIN subtree s ON c.parent_id = s.id
)
SELECT array_agg(id) FROM subtree;
```

The category tree is small and changes rarely, so the application can also keep it in memory.

### Listing form: attributes for a category, inherited from its ancestors

```sql
SELECT a.code, coalesce(a.name_i18n ->> :lang, a.name) AS label, a.data_type, a.unit, eca.required
FROM effective_category_attributes eca      -- own and inherited; the nearest category's settings win
JOIN attributes a ON a.id = eca.attribute_id
WHERE eca.category_id = :category_id
ORDER BY eca.sort_order;
```

### Start (or reopen) a conversation

```sql
INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id, seller_business_id)
SELECT l.id, :buyer_id, l.seller_user_id, l.seller_business_id
FROM listings l
WHERE l.id = :listing_id
ON CONFLICT (listing_id, buyer_user_id) DO UPDATE SET updated_at = now()
RETURNING id;
```

A double click returns the same conversation instead of creating a second one.

### Inbox with unread counts

```sql
SELECT c.id, c.listing_id, c.last_message_at,
       (SELECT count(*) FROM messages m
        WHERE m.conversation_id = c.id
          AND m.created_at > coalesce(p.last_read_at, '-infinity')
          AND m.sender_user_id IS DISTINCT FROM p.user_id
          AND m.deleted_at IS NULL) AS unread
FROM conversation_participants p
JOIN conversations c ON c.id = p.conversation_id
WHERE p.user_id = :me AND p.archived_at IS NULL
ORDER BY c.last_message_at DESC NULLS LAST
LIMIT 30;
```

Update `conversations.last_message_at` whenever a message is inserted.

### Payment webhook

```sql
INSERT INTO payment_webhook_events (provider, provider_event_id, event_type, payload)
VALUES (:provider, :event_id, :event_type, :payload)
ON CONFLICT DO NOTHING
RETURNING provider_event_id;   -- no row: already handled, stop here
```

## Before production

You should still add project-specific decisions for:

- GDPR/privacy retention rules (the deletion policy above is the starting point)
- towns, villages and neighbourhoods under the seeded municipalities: people search for "Peshkopi" or "Burrel", not "Dibër" or "Mat", and Tirana's neighbourhoods matter for property
- payment provider integration, and whether promotions are charged in lekë or euros (the seeded prices are EUR placeholders)
- SMS/email provider integration
- image moderation/virus scanning
- rate limiting and anti-spam
- automated fraud scoring (duplicate photo hashes, new accounts posting high-value items)
- invoicing and tax: Albania requires fiscalised invoices (fiskalizimi) and VAT (TVSH) handling. Check with an accountant how this applies to online sales of promotions and subscriptions, and add what it needs (e.g. fiscal invoice codes) to the billing tables
- data retention for chat/audit logs
- backups, PITR, observability and disaster recovery
- partitioning or scheduled clean-up for the fast-growing tables: `messages`, `notifications`, `audit_log`, `listing_daily_stats`, `outbox_events`
- a daily job that loads the Bank of Albania's euro rate into `exchange_rates` (the seeded 100 lekë per euro is a round placeholder) and recomputes `price_normalized` for listings priced in lekë
- setting up the sign-in code channels (`src/sender.rs`, settings in `.env.example`), and turning test mode off (`SIGN_IN_CODES_ON_SCREEN=false`): e-mail by SMTP (Amazon SES, Postmark, Brevo...: SMTP_HOST, SMTP_USERNAME, SMTP_PASSWORD, EMAIL_FROM), and WhatsApp for phone numbers through Meta's WhatsApp Cloud API (a Meta business account, a WhatsApp Business phone number, an approved template of category Authentication with a "copy code" button, and a system user's permanent token: WHATSAPP_TOKEN, WHATSAPP_PHONE_NUMBER_ID, WHATSAPP_TEMPLATE). A channel not set up writes its codes to the log. Phone numbers without WhatsApp need an SMS fallback, which needs WhatsApp's delivery webhooks to know when to use it
- a moderation queue (new sellers' first listings as `PENDING_REVIEW`), a way to report listings, and ways for sellers to mark their listings as sold and delete them
- a nightly clean-up of expired `login_codes` and `sessions`, and of drafts (and `PENDING` accounts) whose code never came back
- map tiles from a provider that allows production traffic (MapTiler, Stadia Maps, Thunderforest...) or from your own tile server, keeping the OpenStreetMap credit: OpenStreetMap's own tile server is for light use and may block a busy site. The tile URL is in `map_controller.js`
- PostGIS, if map search outgrows radius queries, e.g. to search inside an area drawn on the map
- a denormalised seller rating (count and sum) once review volume makes the per-seller aggregate noticeable
