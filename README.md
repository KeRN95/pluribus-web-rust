# Pluribus

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

Then open http://127.0.0.1:3000. Every setting is in `.env` (see "Configuration" below); the copied values work locally as they are. On Windows, use `127.0.0.1` rather than `localhost`: `localhost` tries IPv6 first, which adds about 200 ms to every request through WSL.

To keep the release build running there instead, as the development site, deploy it with the script (from WSL): it builds it, keeps the one running as the previous, restarts it on `BIND_ADDR`, checks it answers, and puts the previous back if it doesn't. It stops on a migration git doesn't know yet, which the server would apply, and rebuilds from clean when the binary is older than the sources (a build folder shared with another checkout):

```bash
dev/deploy.sh
```

`dev/deploy.sh --rollback` puts the previous one back.

To load the sample listings, from a bash shell. Their cover photos are free Unsplash photos, loaded from Unsplash's image CDN; the fixture credits each one.

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 < tests/fixtures/listings.sql
```

For a fuller site, also load 305 more listings across every category, each with a photo, a point on the map near its town, its seller's (fake) phone number, and the properties its category defines (a car's make, model, year, mileage, fuel, gearbox, body type and colour; a flat's rooms, area, floor and furnishing; and so on), so every filter has data to work on:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 < dev/sample_listings.sql
```

For a site at a real site's size, generate as many listings as you like (`dev/generate_listings.sql`): in every category, in about the shares a real site has them, each put together from lists of what Albanian listings say and cost ("Volkswagen Golf 1.6 TDI, 2012", "Shitet apartament 2+1, 95 m², Komuna e Parisit, Tiranë"), with the properties its category takes, a town and a point near it, a seller (a few with many listings, car dealers, agencies and shops among them), and two to four photos of its kind of thing (free ones from Unsplash, credited in the script). Runs add up; `-v seed=0.42` (between -1 and 1) repeats one, and `remove=1` takes out everything the script has made, and nothing else. 100 000 listings take about two and a half minutes:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v count=100000 < dev/generate_listings.sql
```

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v remove=1 < dev/generate_listings.sql
```

The sample listings, the test fixtures and listings from earlier versions of the script have only a cover; `photos=1` makes nothing new but gives each of them more photos of its category, two to four in all (a million listings take about a minute):

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v photos=1 < dev/generate_listings.sql
```

A generated property listing stands in the zone its title names where its town has zones ("…, Blloku, Tiranë"), so the zones have their averages a square metre. Earlier versions of the script left them in the town; `zones=1` makes nothing new but moves those into their zones:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v zones=1 < dev/generate_listings.sql
```

`only=<top-level category>` keeps making or removing to that category's listings (removing, with the sellers it leaves without any; making, with sellers and shops of their own): after electronics got their own properties (migration 0021), its listings were made anew with them:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v remove=1 -v only=elektronike < dev/generate_listings.sql
```

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v count=150000 -v only=elektronike < dev/generate_listings.sql
```

`views=1` makes nothing new but gives a twentieth of the active listings the script made two weeks of views (`listing_daily_stats`), most of them few and a few hundreds a day, so "Më të klikuarat" has rows to show before anyone has looked around; runs add up:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v views=1 < dev/generate_listings.sql
```

`drops=1` makes nothing new but makes a twentieth of the active listings the script made with a price (on sale, for rent or a service) 8 to 25% cheaper, rounded as people round prices, as if their sellers had lowered them, so the cards and pages have price drops to show (the price history's trigger records them); runs add up. Those who keep a listing hear only of the drops its seller makes on the site:

```bash
docker compose exec -T db psql -U marketplace -v ON_ERROR_STOP=1 -v drops=1 < dev/generate_listings.sql
```

The tests need the database running; each test gets a fresh database of its own, which is most of what a test costs. Two things keep that cheap. `compose.yaml` runs the development database for speed over durability (no waiting for the disk on each commit), with the lock memory and connections for a dozen test databases at once. And `dev/test_template.sh` makes the cluster's `template1` a migrated copy of the schema (sqlx-cli needed: `cargo install sqlx-cli --no-default-features --features postgres`), so that the `create database` behind every `#[sqlx::test]` copies 12 MB of files in a tenth of a second instead of running the 26 migrations, which the migrator then finds applied. Run it once after `docker compose up -d`, and again whenever `migrations/` changes: a new migration only costs the tests its own run until then, but a migration changed under the tests fails them all with "migration N was previously applied but has been modified". Scratch databases made with `sqlx database create` come migrated too. The 118 request tests then take about 15 seconds, eight at a time:

```bash
sh dev/test_template.sh
```

```bash
cargo test -- --test-threads=8
```

**Windows:** Rust's MSVC toolchain needs Microsoft's C++ build tools for its linker (`link.exe`): Visual Studio Build Tools with the "Desktop development with C++" workload. Without them nothing links, so build and run in WSL instead.

## Project layout

| Path | Contents |
|---|---|
| `src/main.rs` | Startup: `.env`, the log (`RUST_LOG`), the database pool (ten connections, each planning every query for the values it runs with: `db::plan_for_values`), the migrations, the rebuilding of the words search corrects typos from and of the zones' prices a square metre (at start and every 15 minutes), the HTTP server on `BIND_ADDR`, and a clean stop that first ends the newest listings' open streams |
| `src/lib.rs` | The application: `AppState` (the database pool, the photo store, the moderators, the AI, the views gathered), `app()`, which `main` serves, and `app_with()` / `app_with_ai()`, which the tests build with photos in memory, a moderator of their own and views written at once |
| `src/web/mod.rs` | The HTTP layer: every route (see "Routes"), errors and the 404 page, the `Cache-Control` of static files (a year for the vendor files and for those asked for with the fingerprint, `cache_headers`), the check that refuses forms posted from other sites, the fingerprint of the site's CSS and JavaScript (`asset_version`), the modules every page preloads (`js_modules`) and the stylesheets' order (`STYLESHEETS`) |
| `src/web/browse.rs` | The welcome page and the browse pages (a category, a search, the map page), and sellers' profiles: grids of listings with filters, orders and infinite scroll, the subcategory pills, the "Më të klikuarat" rows |
| `src/web/filters.rs` | A browse page's filters: read from the query string, turned into the listing query, and shown back as the filter form, the chips that remove them, the orders and the next page's URL |
| `src/web/listings.rs` | A listing's page (its details and their icons, photos, price per square metre, loan, the seller's other listings and the related ones), what it tells link previews and Google (`listing_meta`), the phone number's reveal, and the views it reports |
| `src/web/newest.rs` | The stream of new listings for the welcome page's row: server-sent events, each a Turbo Stream with a listing's card |
| `src/web/saved.rs` | The hearts: keeping a listing, the page of those kept (`/te-ruajturat`) with the price drops not seen yet, keeping one after signing in, and the header heart's dot |
| `src/web/messages.rs` | Messages between buyers and sellers: the page that starts a conversation from a listing, a conversation, the inbox (`/mesazhet`), the header's unread count, blocking, and the limits that keep spam out |
| `src/web/post.rs` | Posting, editing, pausing, putting back and deleting listings, "Njoftimet e mia" with what each listing drew (views, contacts, saves), and the limits that keep spam out |
| `src/web/properties.rs` | A category's properties in the listing form (the fields, a car's tiles, the body types' drawings), and checking what a seller picked or typed |
| `src/web/ai_fill.rs` | "Plotëso pjesën tjetër me AI": the listing form filled in by the AI, its answer checked against the form's own lists |
| `src/web/photos.rs` | Photos on their way in (the posting form, their checks) and out (`/media/...`) |
| `src/web/account.rs` | Signing in with a code, sessions, "Llogaria", the limits on asking for and guessing codes, and the cookie that tells the pages' scripts someone is signed in (`signed_in_cookie`) |
| `src/web/moderation.rs` | `/moderim`: the moderators keep the listings people reported or take them down, and approve the makes, models and brands sellers typed in, or merge them into listed ones |
| `src/web/safety.rs` | Trust and safety: reporting a listing (its reasons and their names), blocking the seller behind one and letting them be again, and the accounts a visitor blocked, which the pages of listings leave out for them |
| `src/web/named.rs` | The filters a search's words name: a category, a kind of listing (for sale, for rent, wanted) and a town, read from the words in their forms ("apartament me qera ne tirane"), and the page with them on that the search goes to |
| `src/web/suggestions.rs` | What the search boxes suggest as words are typed |
| `src/web/icons.rs` | Every line icon, each drawn once: the details' on a listing's page, the posting form's tiles, the eye of the view counts, the heart and the handset (the hearts, the phone button, the sellers' stats), the flag, the crossed-out circle and the shield with a check (reporting, blocking, a verified number), the main categories' and the subcategories' |
| `src/web/pages.rs` | The pages about the site, which every page's footer links: how it works, the safety tips, the terms of use, privacy and contact (`PAGES`, which the sitemap lists too) |
| `src/web/seo.rs` | What search engines and link previews read: each page's description, canonical address and Open Graph tags (`PageMeta`, which `meta.html` writes), a category page's trail for the results' own (schema.org's `BreadcrumbList`, from `browse.rs`), the site's own description and picture, `robots.txt` and the sitemaps |
| `src/db/mod.rs` | Database access: how the site's connections plan their queries (`plan_for_values`), the page size and count limit, `price_was!` (the SQL for the price a listing dropped from, which cards, a listing's page and the notifications share), and every query of the files below, re-exported, so each is `db::...` |
| `src/db/categories.rs` | The categories and places (towns, their zones), as the pages and the forms list them |
| `src/db/properties.rs` | Categories' properties: what the filters and the posting form offer, and what a listing says |
| `src/db/listing_pages.rs` | Pages of listing cards: the browse pages' filters, orders and pages, counts, the rows under a listing, the most seen, the map's points |
| `src/db/listing.rs` | One listing: its page, its photos and properties, and the contact it reveals |
| `src/db/saved.rs` | The listings a signed-in user keeps |
| `src/db/messages.rs` | Conversations and their messages: starting one, writing, reading up to a message, the inbox with its unread counts, what an account sent lately (for the limits), blocks |
| `src/db/safety.rs` | Reports (one open per person and listing; `HIDE_AFTER` people with a serious reason take a listing off the site, the rule's doing in `moderation_actions`), the moderators' decisions on them, blocks, and the contacts people proved theirs by signing in (`user_verifications`) |
| `src/db/price_drops.rs` | Price drops: what a listing's price dropped from, for its page; the notifications to those who keep it when its seller lowers it (`notify_price_drop`); those not seen yet, and seeing them |
| `src/db/sign_in.rs` | Sign-in codes, sessions and users |
| `src/db/posting.rs` | Posting and editing listings, pausing them and taking them down; a seller's listings with what each drew (`ListingStats`: its views, how many times its contact was shown, how many keep it, all along and over the last seven days) |
| `src/db/typed_in.rs` | What sellers type in where a list lacks theirs ("Tjetër"), and how moderators approve or merge it |
| `src/db/sitemap.rs` | What the sitemaps list: the active listings by their numbers, and the businesses with listings |
| `src/views.rs` | Listing views: counted as listing pages report them, gathered in memory and written into the day's statistics once a minute; and the most seen rows the pages show |
| `src/media.rs` | Photos: checked, turned the right way up, resized and re-encoded (WebP in three sizes, a JPEG, and a tiny copy for the pages), then kept in an S3-compatible store (Garage locally, Cloudflare R2 in production), or in memory without one; the address pages give for them (`MEDIA_URL`) |
| `src/sender.rs` | Sending sign-in codes: e-mail by SMTP, phone numbers by WhatsApp (Meta's Cloud API); a channel not set up writes to the log |
| `src/ai.rs` | Claude, Anthropic's AI, through its Messages API: what fills in the listing form (`web/ai_fill.rs`) |
| `src/format.rs` | Albanian display formatting: prices, counts, dates and when something happened in words ("Sot", "Dje", "para 3 ditësh"), both by Albania's clock, phone numbers |
| `build.rs` | Rebuilds when a migration changes, so `sqlx::migrate!()` embeds the current files |
| `templates/` | Askama templates, compiled into the binary and checked at compile time (see "Templates") |
| `static/css/` | The site's styles: eight stylesheets, loaded in order (see "Stylesheets") |
| `static/js/` | `application.js` and the Stimulus controllers (see "JavaScript") |
| `static/img/` | `pluribus.png`, the site's picture in link previews of the pages without a photo of their own: the logo's creature and name on the ivory, 1200 × 630 |
| `static/vendor/` | Turbo, Stimulus, Leaflet and the fonts (Geist, and Merriweather for the logo), with their versions in their names, so there's no Node or npm |
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
| `migrations/0016_fuzzy_search.sql` | Search that forgives typos and finds a word's other forms: the words listings use (`search_words`), a rough Albanian stem (`sq_stem`), and `search_query()`, which widens a search's words with them |
| `migrations/0017_search_categories_and_places.sql` | A listing's category, the categories above it and its place become words search finds it by; searches leave out filler words (në, me, për...), and "ose" and "apo" mean or |
| `migrations/0018_category_order_indexes.sql` | A category's listings by price (cheapest or dearest first) and nearest a point first, read straight off indexes; `btree_gist`, for the category beside the place in one GiST index |
| `migrations/0019_zones.sql` | Zones: the parts of Tirana, Durrës and Vlorë people name (Blloku, Plazh, Lungomare...), places under their town, which property listings can say they are in |
| `migrations/0020_zone_area_prices.sql` | What a square metre costs in each zone, by category (`zone_area_prices`), which a property's price per square metre is compared with |
| `migrations/0021_electronics_properties.sql` | Electronics' own properties, subcategory by subcategory (a phone's model and battery, a laptop's processor, a TV's size and resolution...), and `category_attribute_options`: the options of a property each category offers, its own brands and storage sizes |
| `migrations/0022_saved_listings.sql` | The listings a signed-in user keeps with the heart over their photo (`saved_listings`) |
| `migrations/0023_more_properties.sql` | The properties every category lacked: a car's customs, plates, engine and power, a motorcycle's and a van's own, a tyre's size, a flat's bathrooms, condition and heating, a plot's permit, a job's contract, a service's areas, shoes' sizes, a pet's breed, a machine's hours...; what only some kinds of a category have moves down to them (a plot has no rooms, a bag no size), and central heating goes from a home's amenities to its kinds of heating |
| `migrations/0024_search_kinds.sql` | Words that name a kind of listing (shitje, shitet, shes; qira, qera; blej) find the listings of that kind, as well as those that say them: "golf 7 në shitje" finds the Golfs for sale |
| `migrations/0026_option_statistics.sql` | Statistics that tell the planner an option names its property (`CREATE STATISTICS ... (dependencies)` on `listing_attribute_values`): it took a filter on a car's make and fuel for a thousandth of the cars, and joined every match to pick the 25 newest (115 ms) rather than walking the newest cars (17 ms) |
| `migrations/0025_photo_copies.sql` | Photos kept as WebP: beside the full and card-sized copies, a middle-sized one (`middle_url`), a JPEG of the full one (`jpeg_url`) and a tiny copy for the pages (`placeholder`) |
| `tests/web.rs` | 118 request tests: each sends requests to the app, with a database of its own, and checks the pages, redirects, cookies and what the database holds after |
| `tests/schema.rs` | Runs `schema_checks.sql` against a freshly migrated database |
| `tests/schema_checks.sql` | 83 checks of the schema's constraints and behaviour (35 checks, 36 statements that must fail, 12 that must succeed), in one transaction rolled back at the end |
| `tests/fixtures/listings.sql` | The tests' sample listings, which can also be loaded into the development database |
| `dev/deploy.sh` | Builds the release and (re)starts it on `BIND_ADDR`, keeping the previous one to go back to |
| `dev/test_template.sh` | Makes the development database's `template1` a migrated copy of the schema, so the tests' databases start migrated (see "Running it locally"); run again when `migrations/` changes |
| `dev/setup_garage.sh`, `dev/garage.toml` | The local photo store's one-time setup, and its configuration |
| `dev/sample_listings.sql` | 305 extra sample listings for the development database (not used by the tests) |
| `dev/generate_listings.sql` | Generates as many realistic listings as you ask for, with sellers and photos, for trying the site at scale; removes them again with `remove=1`, gives listings with only a cover more photos with `photos=1`, puts property listings from earlier versions of it in their zones with `zones=1`, gives listings views with `views=1`, and lowers some of their prices with `drops=1` |
| `compose.yaml` | The development services: PostgreSQL 15 (port 5432), set for speed over durability (`fsync` and `synchronous_commit` off, lock memory and connections for the tests' databases, 256 MB of shared memory), and Garage (port 3900) |
| `.env.example` | Every setting, with values that work locally (see "Configuration") |
| `Cargo.toml` | The crates the site uses (see "Dependencies") |
| `.gitattributes` | LF line endings for migrations, which sqlx checksums byte by byte, and for shell scripts, which a carriage return breaks |
| `.sqlx/` | Cached query metadata, so the project builds without a database (set `SQLX_OFFLINE=true`) |
| `CHANGES.md` | What changed from the first version of the schema, and why |

## Routes

Every page and action, from `web::router`. Anything else is the 404 page. A form posted from another site is refused (the `Origin` check in `web/mod.rs`), and the forms that carry photos may be up to 64 MB; other requests up to 2 MB.

| Method | Path | What it does |
|---|---|---|
| GET | `/` | The welcome page: the search, the main categories, "Më të klikuarat", "Sapo u shtuan", then every listing, newest first; `?after=` is the next page (`browse::home`) |
| GET | `/c/{slug}` | A category's page: its subcategories, the round buttons, "Më të klikuarat" (main categories and a few big subcategories, with no filter on), its listings, filtered by the query string (`browse::category`); words searched there that name a town or a kind of listing send it to the page with those filters on (`named.rs`); with `numri=1`, only how many it finds, up to 10 000, as JSON |
| GET | `/search` | A search, across every category or in the one `category` names, with the same filters and `numri=1` (`browse::search`); words that name a category, a kind of listing or a town send it on to the search with those filters on, the rest of the words searched there: "apartament me qera ne tirane" to `/search?category=apartamente&city=tirane&type=RENT` (`named.rs`) |
| GET | `/harta` | The map page: listings around a pin (`lat`, `lng`, `radius`), nearest first (`browse::map`) |
| GET | `/sugjerime?q=…` | The search boxes' suggestions, as rows of links (`suggestions::suggestions`) |
| GET | `/te-rejat?since=…` | The listings posted after the welcome page's newest, then each as it is posted, as server-sent events (`newest::stream`) |
| GET | `/listing/{number}-{slug}` | A listing's page; an outdated slug redirects to the current one; one taken off the site by its reports or the moderators is for its seller and the moderators alone (`listings::show`) |
| GET, POST | `/listing/{number}/raporto` | Reporting the listing: the reasons, for someone signed in, and keeping the report (`safety::report_page`, `safety::report`) |
| POST | `/listing/{number}/blloko` | Blocks the seller behind the listing for the signed-in user, or lets them be again with `blloko=0`: the same block a conversation's makes (`safety::block`) |
| POST | `/listing/{number}/phone` | Reveals the seller's phone number or e-mail address, as a Turbo Stream, and counts it (`listings::reveal_phone`) |
| POST | `/listing/{number}/pare` | A listing's page was shown, for the view counts (`listings::seen`) |
| POST | `/listing/{number}/ruaj` | Keeps the listing for the signed-in user, or lets it go with `ruaj=0` (`saved::save`) |
| GET, POST | `/shto` | The posting form, and publishing a listing, signed in or not (`post::new_listing`, `post::create_listing`) |
| GET | `/shto/fushat?category=…` | A category's property fields, for the form's frame (`properties::fields_for_category`) |
| POST | `/shto/ploteso` | What the AI made of the form's title, description and photos, as JSON (`ai_fill::fill`) |
| GET, POST | `/listing/{number}/ndrysho` | Editing a listing, for its seller only (`post::edit_listing`, `post::update_listing`) |
| POST | `/listing/{number}/caktivizo` | Pausing a listing (`post::pause_listing`) |
| POST | `/listing/{number}/aktivizo` | Putting a paused listing back (`post::resume_listing`) |
| POST | `/listing/{number}/fshi` | Deleting a listing (`post::delete_listing`) |
| GET | `/njoftimet-e-mia` | "Njoftimet e mia": the signed-in seller's listings, with what they drew, all together and each (`post::my_listings`) |
| GET | `/te-ruajturat` | The listings the user keeps, the last kept first, under those whose price dropped since they last looked (`saved::page`) |
| POST | `/llogaria/zhblloko/{id}` | Lets someone the user blocked be again, from "Njoftimet e mia", whichever page they were blocked on (`safety::unblock`) |
| GET | `/te-ruajturat/numrat` | For the hearts, as JSON: the numbers of those listings, and how many dropped their price unseen, for the header heart's dot: `{"numbers": [1001, 1005], "price_drops": 1}` (`saved::numbers`) |
| POST | `/te-ruajturat/pare?deri=…` | `/te-ruajturat` was shown with its price drops: they are seen, up to the latest it showed, by its notification's id (`saved::seen`) |
| GET, POST | `/listing/{number}/mesazh` | The page that starts a conversation with the listing's seller, and its first message; the conversation the visitor has about it already, if any (`messages::compose`, `messages::start`) |
| GET | `/mesazhet` | The visitor's conversations, the latest written in first (`messages::inbox`) |
| GET | `/mesazhet/numri` | How many of the visitor's conversations have messages unread, as JSON, for the header's button: `{"unread": 2}` (`messages::unread`) |
| GET, POST | `/mesazhet/{id}` | A conversation, for the two in it ("not found" for anyone else), and a message in it (`messages::conversation`, `messages::reply`) |
| GET | `/mesazhet/{id}/te-rejat?pas=…` | The messages after one, as a Turbo Stream that adds them at the page's end; 204 when none came (`messages::newer`) |
| POST | `/mesazhet/{id}/lexuar?deri=…` | The conversation's page showed its messages up to one: they are read (`messages::read`) |
| POST | `/mesazhet/{id}/blloko`, `/mesazhet/{id}/zhblloko` | Blocks the other side of the conversation, or lets them write again (`messages::block`, `messages::unblock`) |
| GET | `/perdorues/{id}` | A private seller's profile and active listings (`browse::user_profile`) |
| GET | `/biznes/{slug}` | A business's profile and active listings (`browse::business_profile`) |
| GET | `/llogaria` | "Llogaria": signing in or creating an account, or, signed in, the account (`account::account_page`) |
| POST | `/hyr` | Sends a sign-in code to a phone number or an e-mail address (`account::request_code`) |
| GET, POST | `/hyr/kodi` | The page for the code, and checking it: signs in, and puts a listing posted before signing in up (`account::code_page`, `account::check_code`) |
| POST | `/dil` | Signs out (`account::sign_out`) |
| GET | `/si-funksionon`, `/siguria`, `/kushtet`, `/privatesia`, `/kontakt` | The pages about the site, from every page's footer: how it works, the safety tips, the terms of use, privacy, and contact, with the address `CONTACT_EMAIL` names (`pages.rs`) |
| GET | `/moderim` | The moderators' page: the reported listings, then the names sellers typed in (`moderation::page`) |
| POST | `/moderim/njoftim/{number}/mbaje` | Keeps a reported listing: its reports dismissed, back on the site if they took it off (`moderation::keep`) |
| POST | `/moderim/njoftim/{number}/hiq` | Takes a reported listing off the site for good (`REJECTED`) (`moderation::remove`) |
| POST | `/moderim/{id}/mirato` | Approves an option sellers typed in, its spelling put right (`moderation::approve`) |
| POST | `/moderim/{id}/bashko` | Merges it into the listed one it is (`moderation::merge`) |
| GET | `/media/{key}` | A stored photo, WebP or JPEG, which browsers may keep for a year (`photos::serve`); unless a CDN serves them (`MEDIA_URL`) |
| GET | `/static/…` | The CSS, the JavaScript and the vendor files: the vendor files, and the site's own CSS and JavaScript asked for with their fingerprint (`?v=`), kept by browsers for a year without asking again; anything else checked on each use (`web::cache_headers`) |
| GET | `/healthz` | "ok" when the server and its database answer (`dev/deploy.sh` checks it) |
| GET | `/robots.txt` | Where crawlers may go: anywhere but a visitor's own pages (signing in, "Llogaria", "Njoftimet e mia", the kept listings, the moderators' page, the forms) and the ones only scripts ask for; and where the sitemaps are (`seo::robots`) |
| GET | `/sitemap.xml` | The sitemaps: the pages', and the listings', one for every 50 000 listing numbers (`seo::sitemap_index`) |
| GET | `/sitemaps/{name}` | `faqet.xml`: the welcome page, the pages about the site, every category's and the businesses' with listings; `njoftimet-{n}.xml`: the active listings numbered from n × 50 000 on, each with the day it last changed (`seo::sitemap`) |

## Configuration

The server reads `.env` at start (`.env.example` has every setting, with values that work locally). A channel or service left out is turned off, and the log says so.

| Setting | Default | What it does |
|---|---|---|
| `DATABASE_URL` | none (required) | The PostgreSQL database; sqlx's compile-time checks read it too, unless `SQLX_OFFLINE=true` |
| `BIND_ADDR` | `127.0.0.1:3000` | Where the server listens |
| `RUST_LOG` | `marketplace=info,tower_http=info` | What the log says, by module (`.env.example` has `marketplace=debug`) |
| `PUBLIC_URL` | the address each request came to | The site's address (`https://pluribus.al`), which the links it gives of itself start with: canonical addresses, link previews' addresses and photos, the sitemaps and `robots.txt`. Unset, a request's `Host`, over https when the proxy in front says so (`X-Forwarded-Proto`), as Cloudflare's tunnel does |
| `SMTP_HOST`, `SMTP_USERNAME`, `SMTP_PASSWORD`, `EMAIL_FROM` | none | Sign-in codes by e-mail, through any SMTP service; all four or none |
| `SMTP_PORT` | `587` | 587 uses STARTTLS, 465 TLS |
| `WHATSAPP_TOKEN`, `WHATSAPP_PHONE_NUMBER_ID`, `WHATSAPP_TEMPLATE` | none | Sign-in codes to phone numbers by WhatsApp: Meta's Cloud API, a system user's permanent token, and an approved authentication template with a "copy code" button |
| `WHATSAPP_LANGUAGE` | `sq` | The template's language |
| `WHATSAPP_API_VERSION` | `v21.0` | The Graph API's version |
| `S3_ENDPOINT`, `S3_BUCKET`, `S3_ACCESS_KEY_ID`, `S3_SECRET_ACCESS_KEY` | none | The photo store (see "Photos"); without them photos are kept in memory, and lost when the server stops |
| `S3_REGION` | `auto` | The store's region: `auto` for R2, `garage` for the local Garage |
| `MEDIA_URL` | `/media` | Where pages say the photos are: under `/media` the site serves them itself; a CDN's address that serves the bucket (an R2 bucket's public domain, say) has it serve them (see "Photos") |
| `SIGN_IN_CODES_ON_SCREEN` | `false` | Test mode: the sign-in page shows the code instead of sending it, so anyone can sign in as any number; only for trying the site before a channel is set up |
| `ANTHROPIC_API_KEY` | none | Turns on "Plotëso pjesën tjetër me AI" on the listing form |
| `ANTHROPIC_MODEL` | `claude-haiku-4-5-20251001` | The model that fills in the form |
| `ANTHROPIC_BASE_URL` | `https://api.anthropic.com` | Where Anthropic's API is; the tests point it at a stand-in |
| `MODERATORS` | none | The phone numbers and e-mail addresses, comma-separated, of those who may decide on reported listings and approve what sellers typed in, on `/moderim`, and see the listings the reports took off the site |
| `CONTACT_EMAIL` | none | The e-mail address the contact page (`/kontakt`) gives; without one, the page gives no address, only the quicker ways for what has one of its own (Raporto, Blloko) |
| `SQLX_OFFLINE` | unset | At build time: `true` builds from `.sqlx/` without a database |

Without e-mail or WhatsApp set up, sign-in codes go to the server's log.

## Dependencies

No Node, no npm, no bundler: the browser's code is plain JavaScript modules, with Turbo, Stimulus and Leaflet in `static/vendor/`. The server's crates (`Cargo.toml`):

| Crate | What it does here |
|---|---|
| `axum` (with `multipart`) | The web server and the router; `multipart` for the posting form's photos |
| `tokio` | The async runtime: the server, the background refresh and the views' writing |
| `tower-http` | Static files, request logs, gzip and Brotli compression |
| `sqlx` | PostgreSQL: queries checked against the schema at compile time (`query!`), built at run time where filters are optional (`QueryBuilder`), and the migrations |
| `askama` | Templates compiled into Rust and checked at compile time |
| `serde`, `serde_json` | JSON: the AI's answers, the hearts' numbers, stored properties |
| `rust_decimal` | Prices and properties' numbers, without a float's rounding |
| `time` | Dates and times |
| `uuid` | Ids (UUIDv7, sortable by when they were made) |
| `image` | Decoding photos, turning them the right way up and resizing them; quick JPEGs for the AI |
| `libwebp-sys` | Photos as lossy WebP, by Google's libwebp, built from its C sources |
| `mozjpeg`, `mozjpeg-sys` | Each photo's JPEG, progressive, by Mozilla's encoder, built from its C sources without its SIMD code (which needs NASM) |
| `object_store` | The S3-compatible photo store |
| `sha2` | Photos' checksums, and sign-in codes and session tokens stored hashed |
| `base64` | Photos sent to the AI, and the tiny copies in the pages |
| `lettre` | E-mail by SMTP |
| `reqwest` | HTTP calls to WhatsApp's API and Anthropic's |
| `rand` | Sign-in codes and session tokens |
| `futures-util` | Streams: the newest listings' server-sent events |
| `form_urlencoded` | Query strings |
| `dotenvy` | Reading `.env` |
| `anyhow` | Startup errors |
| `tracing`, `tracing-subscriber` | The log |
| `tower`, `http-body-util` (tests only) | Sending requests to the router and reading the answers |

## How the frontend works

- Every page is plain HTML rendered by the server, so it loads fast and search engines index it as it is.
- Every page says what it is in its head (`web/seo.rs`, `meta.html`): to search engines, a description and its canonical address; to the apps that preview a shared link (WhatsApp, Facebook, Messenger, X, Telegram, iMessage...), the Open Graph tags they show it with. A listing's preview has its title and price ("Mercedes-Benz C 220, 2018, automatike · 21 500 € (i diskutueshëm)"), where it is, what kind of listing and in what condition followed by the start of its description ("Durrës · Në shitje · E përdorur — Importuar nga Gjermania..."), and its first photo; a category's, its name, how many listings it has and its first listing's photo; a seller's profile, their name, since when they are on the site and how many listings they have. The welcome page, and any page without a photo of its own, show the site's picture (`static/img/pluribus.png`: the logo's creature and name on the ivory, 1200 × 630, the name small enough to stay whole where an app crops the picture square). A listing with a price also gives it as `product:price` tags, and one on sale tells Google what it sells, as schema.org data: a Product and its Offer, with the price, the currency, the condition, whether it is still for sale (in stock, reserved, sold) and the seller, a person or an organization.
- A category page's canonical address keeps its filters but not its order, its page or the map's dots, so `/c/makina?sort=price_asc` counts as `/c/makina` and `/c/makina?vehicle_fuel=DIESEL` as a page of its own. A search's results (`/search`, the map page, a category's or a seller's searched with words) and a sold listing ask search engines to leave them out of their results (`noindex, follow`) but to follow their links to the listings. `/robots.txt` keeps crawlers out of a visitor's own pages and names `/sitemap.xml`, which lists the pages' sitemap (the welcome page, every category's, and the businesses' with listings) and the listings', one for every 50 000 listing numbers (Google reads at most 50 000 addresses from one), each listing with the day it last changed. Every address they give starts with `PUBLIC_URL` (see "Configuration") or, without it, the address the request came to.
- Turbo fetches pages and form responses in the background and swaps them in without a full reload. It also prefetches links on hover.
- Listing pages load the next page as the visitor scrolls near the end. A `<turbo-frame loading="lazy">` points at the next page, and the server answers that request with just the frame.
- Small updates come back as Turbo Streams. For example, the phone button ("Shfaq numrin") swaps in the phone number and counts the reveal.
- The little JavaScript there is lives in Stimulus controllers in `static/js/controllers/`, like the share button.
- Under a mouse, Turbo fetches a link ahead as the pointer rests on it; on a phone, a link starts fetching as soon as a finger touches it, any link Turbo would fetch ahead under a mouse (to another page of the site, not opened elsewhere, nor one that opts out with `data-turbo-prefetch="false"`, as "Raporto" does, nor one that leads a frame inside the page), and the visit the tap makes takes that response (`application.js`), so the page is on its way while the finger is still down: a category's pill, the chip that takes a filter off, "Shiko të gjitha" as well as a listing. A finger that moves, scrolling, drops it.
- A listing's first photo loads ahead of anything else on its page (`fetchpriority="high"`), so it shows as soon as it can.
- Pages change with the browser's cross-fade (view transitions, `<meta name="view-transition">` in `base.html`), shortened to a tenth of a second (`base.css`): the browser's own quarter of a second, during which the page takes no taps, made each click feel slow.
- The welcome page (`/`) asks what the visitor is looking for (the question, "Çfarë po kërkon?", is there for screen readers; the eye sees the search and the categories), in words only, in a pill like the message box in Claude's app with a round coral search button inside it, and searches every category and town; the results' filters narrow them. Under it, pills like the listing page's details lead straight to every main category, all at once, each with its icon (`icons::main_category`, in `icons.rs` with every other icon; a main category added later gets a box until it has one); they take as many lines as they need.
- As words are typed in the welcome pill or the browse pages' search box, a list like the in-page dropdowns opens under it with suggestions (`suggest_controller.js`, which asks `/sugjerime?q=…`, `web/suggestions.rs`): the last word finished as listings use it, the most used first ("gol" → "golf"), or, typed whole and misspelt, the word it most likely misspells; the categories the first of those finds most in ("golf · në Makina", to `/c/makina?q=golf`); the makes, models and brands the words name, which lead to their filters in the category most of their listings are in ("Volkswagen Golf · në Makina", to `/c/makina?vehicle_make=VOLKSWAGEN&vehicle_model=VOLKSWAGEN_GOLF`), a brand of the very name the words have replacing the words searched in its category; and the towns the last word starts, all their listings or, after other words, those words there ("apartament · në Tiranë", to `/search?q=apartament&city=tirane`), without the word that joins them to the town, which the town's filter says ("apartament në tir", typed with its ë or without, or with "te", "nga", "afër"...); and after words and such a word ("apartament në"), the three towns those words have the most listings in (`db::listing_towns`), rather than words starting "ne". The box keeps the focus: the arrow keys mark a row (`aria-activedescendant`), Enter follows it or, with none marked, searches the words as typed, Escape closes the list; a tap follows a row as any link. Suggestions come 120 ms after the last key, and those fetched are kept for the words they are for, so deleting a letter shows them at once.
- Under the categories, the welcome page shows the ten newest listings in one row that slides sideways (`.newest-strip`, `newest_card.html`; a finger slides it and a mouse drags it, the wheel scrolling the page as anywhere else, with no scrollbar under it), and follows new ones for as long as it is open: it asks `/te-rejat` for them (`<turbo-stream-source>`, `web/newest.rs`), which looks every second for listings posted or bumped since and sends each one, no more than two a second, as a server-sent event: a Turbo Stream that puts its card in front of the row. Each response ends after four seconds, and EventSource connects again a second later, going on from the last listing it got (Last-Event-ID), as it does when the connection drops: where nothing stands in between, a listing comes the moment it is posted, and a proxy that holds every response back until it ends, as Cloudflare's quick tunnels do, passes it on a few seconds late rather than never. `newest_controller.js` brings the cards in at least half a second apart, also when several come at once, makes room for each at the row's start, and lets the row grow from ten to the newest thirty. While the row is slid along, under a finger or still moving, new ones wait rather than move it: a fade over its start says how many have come, and a tap on it goes back to the start, where they come in.
- Over the newest, "Më të klikuarat" shows the twelve active listings whose pages were seen the most over the last seven days, the most first, in one row that slides sideways as the seller's other listings do under a listing (`.most-viewed`), each with an eye and how many times it was seen at its photo's top left, on a dark pill like the heart's circle (`.card-views`), and nothing before any were seen (`db::most_viewed`). A listing's page says it was seen once Turbo shows it (`application.js`, `POST /listing/{no}/pare`), so a page Turbo fetches ahead under the pointer and never opens doesn't count, nor do bots that run no scripts or say they are bots, nor its seller; a visitor (their address and browser) counts once per listing per half hour. The views gather in memory and go into `listing_daily_stats.detail_views` once a minute in one statement (`views.rs`, as "Analytics are aggregated" asks), and the row is read again as often. `dev/generate_listings.sql` with `views=1` gives the listings it made two weeks of views.
- A heart over a listing's photo, at its top right corner, on every card and on the listing's own page, keeps the listing for a signed-in user (`saved_listings`, `web/saved.rs`, `heart.html`). `heart_controller.js` asks the server with each page which listings the visitor keeps (`/te-ruajturat/numrat`) and fills their hearts in the buttons' coral, when someone is signed in: the session cookie is out of scripts' reach, so a cookie beside it says there is one (`llogari`, which `account::signed_in_cookie` sets on the answer to a request with a session and clears on one after the session has gone; `controllers/signed_in.js`), and a page for someone not signed in asks for neither the listings kept nor the unread messages; a tap keeps a listing or lets it go at once (`POST /listing/{number}/ruaj`), every heart for it on the page with it. Someone not signed in goes to sign in first, and comes back to the page with the listing kept: the listing and the page wait in cookies (`ruaj`, `kthehu`), which signing in reads (`saved::after_sign_in`). The heart in the top bar leads to `/te-ruajturat`, the listings kept, the last first.
- A price lowered lately says so (`price_was!` in `db/mod.rs`, from `listing_price_history`, which a trigger fills as prices change): a card, like a listing's page, puts what it was after its price, struck through, a touch smaller, then by how much in a pill, green on light green as on the listing's page, on the photo too ("5 000 € ~~5 700 €~~ −12%"; on a small card the old price gives way first, ending in "…", where the line is short), and a listing's page likewise ("5 400 € ~~6 900 €~~ −22%", `price_drop.html`). A price counts as dropped when its last change, in the last 30 days, made it at least 1% lower, in the same currency, than the lowest it was over the 30 days before: so a price raised for a while and lowered back is no drop, and it shows by how much under that lowest. Only listings on sale, for rent or a service, with a price (fixed, negotiable or "from") and still up; a month after, a drop is just the price.
- Buyers write to sellers from a listing's page: a speech bubble in a coral circle beside the phone (and the small one in the seller's card) opens, for someone signed in, a sheet over the page rather than a page of its own: the page scrolls to bring the button near the top, under the header, and the sheet takes the rest of the screen below it, the page under it held still; "Mbyll" (or Escape) closes it and empties it, so that its conversation stops asking for new messages (`message_sheet_controller.js`). The button loads the conversation into the sheet's Turbo frame, `conversation`, which `conversation.html` wraps its page in: writing, sending and blocking go on in the sheet without leaving the listing, its messages scrolling inside it (`data-scroll-root`, which `conversation_controller.js` scrolls in place of the page) and the box to write in at its foot; a response without the frame (the sign-in page, once a session ran out) opens as a page. Signed out, the button leads to the page as before, which asks to sign in first. The sheet, or the page, has the seller's name, the listing (its photo, title and price) and the box to write in, with three quick first messages that fill it in ("Përshëndetje! A është ende në dispozicion?", "Cili është çmimi i fundit?", "Kur mund ta shoh?"; `web/messages.rs`, `conversation.html`). The first message starts their conversation about the listing (`conversations`, one per buyer and listing, both sides in `conversation_participants`; a business's listing reaches whoever posted it), and writing about it again goes on in the same one. `/mesazhet` lists the visitor's conversations, the latest written in first, each with the listing's photo, who with, the listing, the start of the latest message and when it came; those with messages unread in bold, with how many in a coral pill. The header's speech bubble, beside the heart, has how many conversations have messages unread in a coral circle (`inbox_controller.js`, from `/mesazhet/numri`, asked for when someone is signed in). A conversation's page has the messages, the visitor's own on the right in the coral's tint, each with when it was sent by Albania's clock ("14:32", "Dje, 14:32", then the date; the database turns the times into Albania's, `AT TIME ZONE 'Europe/Tirane'`), and the box at the bottom of the screen, which grows with what is typed; Enter sends where there is a keyboard and Shift+Enter starts a new line, while on a phone Enter is a new line and the paper plane sends. While the page is open it asks every 8 seconds for messages that came, a Turbo Stream that adds them at its end (`conversation_controller.js`; server-sent events would be instant, but the development tunnel holds them back), and what it shows is read as it shows (`/mesazhet/{id}/lexuar`, up to the last message shown), not when Turbo only fetched it ahead under the pointer; the header's count follows. Someone not signed in goes to sign in first, and comes back to the page to write (the `kthehu` cookie, as for the hearts).
- Messages are limited so they can't carry spam (`web/messages.rs`): only someone signed in writes, which takes a code sent to their phone or e-mail, so every sender has proved a contact; one account writes to at most 5 new sellers an hour and 15 a day (conversations it starts), and sends at most 8 messages a minute and 200 a day; until the other side answers, it sends at most 3 messages in a row, and none with a link (`http://`, `https://`, `www.`), which is what spam and scams mostly carry; links never become links on the page anyway. A message has at most 1 000 characters. Whoever gets messages they don't want blocks the other side, under the conversation ("Blloko …", `user_blocks`): they can't write there any more, nor start a conversation about another of the blocker's listings, and "Zhblloko" lets them again. A message refused goes back in the box with why. The counts come from the messages and conversations themselves (`db::sent_lately`), so a restart forgets none of them.
- Those who keep a listing hear when its seller lowers its price (`db::notify_price_drop`, run in the edit's transaction): each gets a notification (`notifications`, type `PRICE_DROP`, with what it was and what it is), in place of one about it they haven't seen yet, keeping that one's "was", the price they last saw; not the seller, nor anyone who turned price drop alerts off (`notification_preferences.price_drop_alerts`, on unless a row says otherwise). The heart in the top bar gets a coral dot while they have drops unseen, and says how many to screen readers ("Të ruajturat: 1 me çmim më të ulët"; `heart_controller.js`, from `/te-ruajturat/numrat`), and `/te-ruajturat` lists them over the cards, each from what to what ("Çmime më të ulëta te njoftimet që ke ruajtur"). Once that page is shown, it says so (`data-seen-url`, the beacon `application.js` sends for a listing's views, to `/te-ruajturat/pare`), not when Turbo only fetched it ahead under the pointer, and the drops are seen up to the latest it showed (`?deri=`, the notification's UUIDv7 id), so a copy of the page the browser shows again from its cache doesn't see a drop that came after: the dot goes. Drops of a listing let go, or no longer up, are left out. E-mail or push notices could follow from the same rows.
- The pages of every main category, and of a few big subcategories (`MOST_VIEWED_IN` in `views.rs`: Makina, Apartamente, Vila dhe shtëpi), have their own "Më të klikuarat", as the welcome page has it for every category: the listings seen the most this week in the category and those under it, under the page's buttons and over its listings, small cards whichever size the listings' are, while no filter or words are on (they would take no notice of them). The rows are read in the background, so that no page waits for them (`Views::most_viewed`): all of them as the server starts, then every minute, after the views are written, those that pages asked for in the last ten minutes; a row no page asked for in a while is read as it is asked for, if older than five minutes.
- Category pages (`/c/makina`) and search (`/search?q=golf`) share one handler and one filter form. A search forgives typos and finds a word's other forms (see "Search can start in PostgreSQL"); when it searched a word in place of a misspelt one, a line under the heading names it: "Shfaqen edhe rezultatet për „golf“." A search across every category names, as chips under its heading, the categories it found listings in, the most first, with how many ("Makina 17"; the names alone once more than 1 000 are found): each narrows the search to its category, whose filters it then offers (`db::listing_categories`). Filters are ordinary query parameters (`/c/makina?vehicle_fuel=DIESEL&price_max=12000&sort=price_asc`), so any filtered page can be bookmarked, shared and indexed, and infinite scroll keeps them.
- A listing card shows the photo, the title, the price, and a details line that always starts with the city: "Tiranë · 2016 · 148 000 km · Naftë". The details line takes one line and ends in "…" when longer. In a search limited to a city (`city=vlore`), the cards in that city leave the city out, since it says nothing new, and show instead the first property the listing page has that neither the card nor the title already gives (a phone's brand: "Apple · 128 GB"; for a "Volkswagen Golf 7", the body type). The city is the listing's municipality, found by `location_municipality()`, so a town or neighbourhood added later under a municipality shows the municipality.
- A card is an `<article>` whose title link is stretched over the whole card with CSS (`.card-link::after`): a tap anywhere opens the listing, and screen readers name the link by its title. Under a mouse pointer, a rounded panel in the site's muted colour appears behind the card, a little larger than it (`.card::before`).
- The top bar has no search: the welcome page has its pill, and a browse page's row starts with a round search button, which opens a search box under the row, as the filters do (`panel_controller.js`); its words search the page's category, its filters and order kept (`Filters::search_form_fields`). After it come Filtrat and Harta (the map), in the page's own colours, as the listing page's share button is, side by side on every screen; the search is the site's coral. Filtrat and Harta say their names and what is on, as pills: "Filtrat" with how many filters are on in a coral circle, the map its place, the town ("Durrës"), the point's nearest town and the distance ("Tiranë · 10 km"), or "Harta", its pin in the location colour once one is set (on a narrow phone the map's words give up room first, ending in "…"). Under the row, two smaller pills: Rendit (the order) says the order now ("⇅ Më të rejat"), and a pill with the icon of the cards' view now opens the views (below). What each is set to is for screen readers and their tooltips too ("Filtrat: Naftë", "Harta: Tiranë · 10 km" or "Harta: Kudo", "Rendit: Më të rejat"). The search, the filters and the map open over the results rather than pushing them down, a little under the row and as wide as it, the page dimmed under them; Escape, or a tap on the dim or on the row beside its buttons, folds them again (`filters_controller.js`). The map's card is a square under the row (below), and the filters' card takes at most the rest of the screen, scrolling inside if it needs more, between its top, its name and "Mbyll", and its foot, always in sight, with "Pastro" (when a filter is on) and a button that says how many results there are now ("Shfaq 1 234 njoftime", "Asnjë njoftim"), which folds the card and brings up the results' heading. Neither has a button to apply it: as soon as something changes (a box ticked, a choice picked, a typed field left or entered, a slider let go, the map stopped after a drag or a zoom) the page refreshes in place (`refresh.js`: Turbo fetches the address and morphs the page into the new one, keeping the scroll, the address replaced rather than added to the history), the card staying open as it is while the results, their count and the chips follow under it; closing the filters applies whatever was typed but not left. Each card has the listing's number as its id, so the refresh keeps the cards that stay. The orders open in a menu under their pill, like the in-page dropdowns, closing on a tap outside or Escape (`dropdown_controller.js`); only one is open at a time: opening one says so (`section:open`) and the others fold. Filtrat says which filters are on but the place, which the map's button says, and which has its own chip. Over the listings, a heading names the order they come in ("Më të rejat", "Më të lirat", "Më të shtrenjtat", "Më afër"), for screen readers: the order's pill says it to the eye. The view's pill, at the right end of the order's line (on a line of its own if the line is full), opens a menu like the order's, its menu growing leftward, with three views, each with its icon, the one now ticked (`Cards` in `browse.rs`); a pick switches the cards at once and folds the menu (`cards_controller.js`). Small cards, as many to a line as fit; a list, a line each, no wider than 46rem: the photo square at its left (128 px, 176 px from 720 px up), beside it the price, bold in the text's colour, the title on up to two lines, under it the start of the description, muted and a touch smaller than the details line, three lines at most, ending in "…" when it has more, the card itself opening the listing for the rest (a card carries the first 400 characters, its lines run together, cut at a word and ending in "…" when longer: `description` in `ListingCard`, `description_excerpt`), and level with the photo's bottom the details line with, at its right end, when it was posted or last brought up ("Sot", "Dje", "para 3 ditësh", the date from a week on: `ListingCard::when_text`), the heart at the top right in the text's colour with no circle (`.cards-list` in `page.css`, the photo's box giving its parts to the card's own grid); or big, one a line no wider than 40rem (two on a wide screen), the photo 4:3 with every photo of the listing to slide through (`slides_controller.js`: a finger slides them, arrows step them under a pointer, a photo in a fifth of a second, and dots say which shows), and over its top left corner, as the price is at the bottom, the seller, small so as not to cover the photo: their initial in a circle of 24 px, as on the listing page, their name at 12.8 px, and under it, at 11 px, since when they are a user ("Përdorues që nga tetor 2026"). A cookie keeps the choice (`cards=small`, `cards=big` or `cards=list`), so the next pages and the infinite scroll come the same way; Prona's pages and those of the categories under it show big cards until the visitor chooses another view there, and keep that choice apart (`cards-prona`, `BIG_CARDS_IN`), so that one made on other pages leaves them as they are; and the photos' `sizes` follow it, a WebP photo's `<source>` as well as its JPEG, so big cards fetch bigger copies and small ones and the list's the card-sized (`Cards::sizes`; see "Photos"). The map page's list, and "Më të klikuarat", stay small. The filters' card is drawn like Claude's, as the posting form's own fields are (the menus' hairline and shadow, rounder corners), and whichever is open has its opener on the accent's tint. "Filtrat" says which filters are on; first where, Qyteti (the towns by county, which replaces the map's point) with "Afër meje" beside it (the visitor's point, which replaces the town), then the price, then the filters buyers narrow by most: those the category asks of every listing or shows on its cards, and a few more (`PRIMARY` in `filters.rs`: a car's body type and gearbox, the makes and brands, a property's furnishing, a job's contract and place), a make with its models; the rest wait under "Më shumë filtra", folded unless one of them is on (`Filters::fields`; a category with six or fewer, or only two more, shows them all). A property's kind of listing (Shitje, Qira) comes first; elsewhere it waits with the rest. Their options are boxes like the posting form's toggles, picked on the accent's tint, under quiet grey names; the options apply by themselves (`filters_controller.js`; without JavaScript, with a button), and on wide screens its groups stand side by side; a make picked there narrows the models to its own at once, as in the posting form (`narrow_controller.js`). The price is typed, from and to, the currency beside them; under them one track with two ends, the lowest and the highest, which stop at each other (`price_range_controller.js`): its scale grows to the right, from 10 € to about a million (a hundred times as many Lekë), prices kept to two figures, and either end at its edge is no limit; and under the track four quick choices, each about a quarter of the category's listings by price in its usual currency ("deri 3 000 €", "3 000 – 6 000 €", "6 000 – 12 000 €", "nga 12 000 €"): the 25th, 50th and 75th percentiles of its prices, kept to round figures (`db::price_quarters`, `Filters::price_choices`), worked out again every six hours, and offered once 20 of its listings have a price. Each of the three moves the others, and the choice the prices are shows pressed; tapped again, it lets them go. A range whose unit has steps is picked from two dropdowns, from and to (`range_steps` in `filters.rs`): the years from next year back to 1970 (1950 for a building's), the kilometres, square metres, cm³, horsepower, inches and rim sizes in round numbers, a value from an older link standing in its place among them; other ranges are typed. A car's body types (Karroceria) take a line of their own, on every screen, each box with the car's side drawn over its name (`BODY_TYPES` in `properties.rs`). A listener in `application.js` drops empty fields from the URL before Turbo fetches it.
- Where to look is in the map's card, as two pills like the welcome page's categories: "Vendndodhja ime" (the browser says where the visitor is, once they let it, and the map moves there, its point applying) and beside it the towns' menu, which says the place now ("Gjithë Shqipëria", "Durrës", or the map's point, "Tiranë · 10 km") and opens the towns by county, folding under their county as the in-page lists do (`picker_controller.js`), all of Albania first (`Filters::town_menu`). A town puts the page in it, its filters kept, as the filters' Qyteti does, and all of Albania drops the place; the page refreshes in place (`refresh.js`, `refreshTo`), the card staying open and the map moving onto the town, without a circle until a point is set again by moving it (`map_controller.js`, `pickTown`). The filters' card has Qyteti and "Afër meje" too. The town the page is in is kept in a cookie for a year (`qyteti`, which `place_controller.js` sets from the page's element as it shows and as a refresh changes the town), so the next category or search opens in it (`Params::remembering_town`) unless its address names a place, a town or a point; all of Albania in the map's card, the town's chip, the filters' own "Gjithë Shqipëria" and "Kërko në gjithë Shqipërinë" forget it (`forgetTown`; the links carry `data-forget-town`, `application.js`, and are never fetched ahead, `data-turbo-prefetch="false"`, since that request would still carry the town). The map page has its own point.
- How many listings a page found is counted with the page up to 1 000 (`db::COUNT_LIMIT`); past that the count, by the order's pill, says "1 000+ njoftime" and, once the page shows, it asks its own address with `numri=1` for the count up to 10 000 (`db::PRECISE_COUNT_LIMIT`, `filters_controller.js`), which fills in that line and the filters' button. Counting that far with every page would slow them all: a town's cars take some 70 ms more to count to 10 000 than to 1 000.
- When nothing is found the page offers what would find more, a tap away: around the map's point, a wider circle ("Zgjero largësinë në 25 km", then 50), all of Albania with a place picked ("Kërko në gjithë Shqipërinë"), and with filters on, a word that the chips above each drop one.
- The map page (`/harta`) finds listings around a point. As on Facebook's, the pin stands fixed in the middle of the map and the map moves under it, starting in the middle of Tirana (`TIRANA` in `filters.rs`, which the map page's results follow until another point is set); "Vendos hartën", under the map, sets its middle as the point. Round the pin, a circle stays the same size in the middle of the map whatever the zoom (38% of the map's smaller side), and how far to look is what it covers: zooming in makes the distance smaller, zooming out bigger, from 1 to 50 km in whole kilometres, said over the map as it changes ("Largësia 9 km", `map_radius.html`); once the zoom has been still for 400 ms the nearest whole kilometre applies, the map staying as it is (snapping it to the zoom of exactly that many was a visible jump), and it zooms no further than 1 km in or 50 km out (any zoom, not only whole ones: `zoomSnap: 0`, and a tap on + or − is half a zoom, the distance about 1.4 times smaller or bigger). An address may name any whole distance up to 50 km, which the map starts on; and the results panel, a `<turbo-frame>`, reloads with what lies inside the circle, nearest first, each card saying how far away it is ("Tiranë · 850 m · 2+1 · 101 m²"). The address bar follows (`/harta?lat=41.32833&lng=19.81806&radius=10`), so a map search can be bookmarked and shared, and category and search pages take the same parameters. "Vendndodhja ime" moves the map to where the visitor is, if the browser may say, and the towns' dropdown beside it, by county, onto a town (its point in `locations`; the dropdown has no name, so it sends no town of its own); the distance and the place both wait for "Vendos hartën". The map has a dot for every result, up to 500 (`db::MAP_POINTS`), while the list loads 24 at a time. Zoomed out, results near each other show as one numbered bubble and a lone one as a small dot; neither takes taps, so they don't get in the way of moving the map. From zoom 15 (`CLOSE_ZOOM`, streets readable), or from the closest the circle goes if that is further out, each result is its own dot, and a tap on one opens its title and price. It is 3:2, three wide and two tall; the mouse wheel scrolls the page rather than zooming the map, and the + and − buttons, a pinch and a double tap zoom around the middle, so the pin stays where it is.
- Every category and search page has the same map over the results, in a card that always comes folded, its button saying where the results are from (above); the button opens it, and only then do Leaflet and the results' dots load (the dots from the same page with `harta=1`, which Turbo takes their `<turbo-frame>` out of). Its pin starts on the point set before, or on the town picked, or on Tirana (`Filters::map_start`), the circle round it from the first, before a point is set. The card is a square from the row's left edge, as wide as the row or as high as the screen has left under the row, whichever is less, and the map fills it, edge to edge, nothing above or below it: "Vendndodhja ime" and the towns' menu float over its top, the distance the circle covers is a badge at its bottom left ("10 km", following the zoom), Leaflet's zoom buttons sit at its bottom right, the mouse wheel zooms it too (around the middle, as everything does), and what the map has to say ("Po kërkoj vendndodhjen…") shows over it only while it has something. As soon as the map stops after a drag or a zoom, or "Vendndodhja ime" moves it, the page refreshes in place for the map's middle and the distance, in place of a town, so the heading, the subcategories, the filters and the sort stay, the card stays open with its map as it was, the dots follow the new results, and the point shows as a chip, whose cross drops it and searches everywhere again. The filter form and the category pills carry the point along. Nothing links to the map page (`/harta`) since the welcome page's link went, but its addresses still work.
- `map_controller.js` draws the map with [Leaflet](https://leafletjs.com/) 1.9.4 (in `static/vendor/` with its BSD licence, 46 KB gzipped), which it fetches only when a map shows: on the map page, or once a category page's map card is opened. The tiles come from OpenStreetMap's tile server (see "Before production"); the dark theme darkens them with a CSS filter.
- Anyone can post a listing ("+ Shto njoftim", `/shto`) with just a phone number or an e-mail address: no password, and no code. The listing is published at once, at its town's point on the map, and buyers reach the seller through the phone button, a phone in a coral circle named "Shfaq numrin" for screen readers (an envelope, "Shfaq email-in", for sellers who signed in by e-mail), which shows the number with a button beside it that copies it (`copy_controller.js`). Sellers edit their listings from the listing page ("Ndrysho", seen only by them) or from "Njoftimet e mia" (`/njoftimet-e-mia`); an edit goes through the same checks, keeps the listing's place in "newest first", and only the listing's own seller, while it is active or reserved, can make it. Sellers can also take a listing down for a while ("Çaktivizo", status `PAUSED`, migration 0011): it leaves the site but stays on "Njoftimet e mia" with "Aktivizo" to put it back where it was. "Fshi" deletes it for good (status `DELETED`, after the browser asks to be sure). Both are on the listing page too, for its seller, and the status history records who did it. The form asks for the chosen category's own properties too (a car's make, model, year, mileage, fuel...; `web/properties.rs`), which reload in a turbo-frame (`/shto/fushat?category=makina`) when the category changes; models narrow to the chosen make, required properties and numeric limits (`validation_json`, and next year at most for a year) are checked, and the values feed the cards, filters and search like the sample listings' do. Where a list can't be complete (car makes and models, phone brands: `attributes.allow_other`), it ends with "Tjetër", which opens a box to type the option in. A typed option is saved as an inactive option (`attribute_options.added_by_user_id` says who added it): the listing shows it and search finds it, but the form and the filters don't offer it until a moderator approves it. Moderators are the phone numbers and e-mail addresses `MODERATORS` in `.env` names; signed in with one, "Njoftimet e mia" links to "Për miratim" (`/moderim`, `web/moderation.rs`), which lists what waits, makes before their models, with how many listings have each and who typed it. "Mirato" approves one, its spelling put right if need be, and the lists offer it after the names that come before it alphabetically; "Bashko" merges it into the listed one the seller meant, chosen already when the names are alike (pg_trgm), and its listings take that one, along with a typed make's typed models. Anyone else gets "not found" there. Typing an option that exists ("bmw", or a make someone else typed) reuses it. Posts can have up to 8 photos (see "Photos").
- "Njoftimet e mia" tells sellers what their listings drew (`db::ListingStats`, `my_listings.html`). Over the list, three tiles add all of them up, each with its icon and its name, the count since the listings were posted, and the last seven days' under it ("+56 këtë javë", or "Asnjë këtë javë"): "Shikime", how many times their pages were seen (the views "Më të klikuarat" counts, the seller's own left out, in `listing_daily_stats.detail_views`); "Kontakte", how many times their number or e-mail address was shown ("Shfaq numrin", `phone_reveals`), which a line under the tiles explains; and "Ruajtje", how many people keep them with the heart (`saved_listings`; in the week, those kept in it). Under each listing's price, its own three counts by the same icons (the eye, the handset, the heart), each with a tooltip that gives the week's as well ("1 203 shikime, 3 këtë javë") and a name for screen readers. Views of the last minute show a minute later, when `views.rs` writes them; deleted listings and their counts are left out.
- The form asks for what the seller writes first, the title, the description and the photos, then the rest; and with the AI set up (`ANTHROPIC_API_KEY` in `.env`), "Plotëso pjesën tjetër me AI" between the two fills the rest in from them (`web/ai_fill.rs`, `ai_fill_controller.js`): the category, its properties (a car's make, model, year, mileage...; a flat's rooms, area, floor...), the kind of listing, the condition, the price and the town, and a property's zone. The server sends the title, the description and the first four photos (made 1024 px on their longer side) to Claude (`src/ai.rs`, Haiku 4.5 unless `ANTHROPIC_MODEL` names another), with instructions that list the form's own choices, built from the database, so it picks codes rather than inventing values; it answers by calling a tool, so its answer is data. Every value is checked as the form checks what a seller sends (`properties::check` and the like), and what doesn't fit is left out; a price only when the seller wrote one. The form then fills in as a seller would fill it: the category first, then its fields once they load, a make before its model, each picker following its select. A field the seller set themselves stays as they set it, and those the AI filled are tinted until changed; nothing is published until the seller publishes. A fill takes a few seconds and costs about a cent with three or four photos (the instructions, about 8 000 tokens, are kept in Anthropic's prompt cache between fills); a seller (their account, or their network when not signed in) gets 10 an hour, and the whole site 2 000 a day. The title, the description and the photos go to Anthropic to be read; say so in the privacy policy. Without a key, the button isn't there and the form is as it was.
- Accounts come by themselves: someone who posts without being signed in types a phone number or e-mail address with the listing, which goes live at once under the account that contact has, or one made there and then, active, under the name typed ("Përdorues" without one; `db::poster_account`). Posting signs no one in, though, and its page says once that it is up and that changing it later takes signing in (the `posted` cookie). Signing in takes a six-digit code sent to the contact, and typing it back signs the seller in for 30 days (a `session` cookie; only a SHA-256 hash of the token is stored, likewise for the codes); so typing someone else's number can put a listing under it, but never opens their account. The account button beside the theme toggle (`/llogaria`) signs in, or creates an account with a name, or leads a signed-in seller to their listings. (Listings posted before this waited as a `DRAFT` for the code, `login_codes.listing_id`; an account still `PENDING` from then becomes active when it posts again.)
- A property says its zone in the town, where the town has known ones (Tirana's Blloku, Komuna e Parisit, Don Bosko... 41 of them, and Durrës's and Vlorë's; migration 0019): the posting form's "Zona", after the town, shows for property only, with the chosen town's zones (`narrow_controller.js`), and isn't required. A zone is a place under its municipality (`location_type` 'AREA'), the listing's own place: the listing page says "Blloku, Tiranë" and has "Zona" first among its details, the card "Blloku, Tiranë" (only "Blloku" where the search is in Tirana already), filtering by Tirana takes in its zones, and search finds the listing by its zone. The zones have no point of their own yet, so their listings stand on the map at their town's.
- A property on sale says under its price what a square metre of it costs, from its area: "925 €/m²" for a house at 185 000 € with 200 m² (`ListingDetail::area_price`), in its own currency and to the whole euro or lek; rents, wanted ads and "price on request" don't. It is a bubble like "Llogarit kredinë", which opens how it was worked out (`area_price.html`): the price, the area, and the one over the other, rounded. With "Llogarit kredinë", the price takes a line of its own and the two bubbles stand under it.
- In a zone with enough sales (Blloku, Plazh, Lungomare...), that bubble also compares the property's square metre with what one costs there on average among the zone's sales in its category ("Mesatarja e zonës"): green when it is 200 € or more under the average, red when it is as much over it, yellow in between (`ZONE_MARGIN_EUR`), and its card says the average and by how much, in the listing's currency. The averages are kept in the database, in the materialized view `zone_area_prices` (migration 0020): for each zone and category, the mean of the euros a square metre the active sales with a price and an area ask, leaving out the cheapest and the dearest tenth so that a mistyped price or area doesn't pull it, where at least five listings make one. The app rebuilds it along with `search_words`, at startup and every 15 minutes. A starting price ("nga") isn't compared.
- Things on sale for at least 10 000 € or, about the same, 1 000 000 Lekë have "Llogarit kredinë" beside the price, or a property beside its price per square metre (`loan_controller.js`): a bubble like that one, which opens the calculator in a card under it the same way (on a phone, under the price's row and as wide as it). A down payment, years and yearly interest give the monthly payment of an ordinary bank loan (equal payments), with the loan, the interest and the total paid. It works in the page alone; rents, wanted ads and "price on request" don't have it (`ListingDetail::loan_amount`).
- Every listing page has its buttons under the price (the phone, the message, the share and "Raporto", a flag in a circle like the share's link, with "Blloko shitësin" under them for someone signed in; on a wide screen the photos run across the page, and the title, price, buttons, details and description sit under them in a column as wide as a line reads well). The seller comes after the description, over their other listings, in a card only as wide as it needs (the page's width on a phone): a small initial, the name, since when they are on the site and how many listings they have, linking to their profile with all their active listings (`/perdorues/{id}` for a private seller, `/biznes/{slug}` for a business), and beside them the same buttons small, so that the page is answered from its foot too, the number revealed there as under the price. The category's path sits above the photo as a category page's heading has it: quiet links with a fainter › between them, the listing's own category last in the text's colour ("Automjete › Makina").
- Under a listing, when its seller has others on the site, "Më shumë nga ky përdorues" ("… ky biznes" for a business) shows the newest four of them, as their profile lists them (`db::seller_listings`), with "Shiko të gjitha (7)" leading to that profile when they have more: at the end of the heading's line, or under the heading where the line is too short for both. The cards stand in one row, each as wide as a column of the grid under them, which slides sideways a card at a time when they don't all fit (two across on a phone), with a quarter of the next card showing at the edge and no scrollbar on any screen (`.seller-listings`): a finger slides the row, and a mouse grabs and drags it, flicking it on as a finger would, the click that ends a drag opening no card (`drag_row_controller.js`, which "Më të klikuarat" uses too). Space alone sets the two sections apart, with no line between.
- Under a listing, "Njoftime të ngjashme" shows up to eight listings like it from other sellers, at most two from any one, the most alike first (`db::related_listings`), as cards like the browse pages', and nothing when there are none. As the big sites do it, on a small scale: a few hundred candidates come cheaply off the category's indexes (the closest in price either way, the nearest, the newest, and those sharing a narrow choice such as the model), and a score ranks them by the choices they share (a model three, any other a half, so that many small likenesses such as the colour don't outweigh the price), the price (none at half or double), the place (none 50 km away) and how new they are (none two months on). The big sites add what visitors do: listings viewed in the same visits as this one, and learned likeness (embeddings) over titles and photos; both need the views recorded first.
- What keeps spam out without passwords: a listing posted without signing in belongs to the contact typed, which nothing proves (the price of posting without a code: anyone can type anyone's number, and buyers who tap the phone button see it), so a network (IP address) posts at most 20 listings an hour that way (in memory, `post.rs`); signing in proves the contact: a code works for 10 minutes and 5 guesses, and a contact gets one code a minute and five an hour, and a network twenty an hour, which also caps what a bot could spend on SMS; an account posts at most 5 listings a day, never the same title twice in a day, and at most 2 links per listing; a hidden field only bots fill in, and a form sent less than 3 seconds after it was shown, are refused; and forms posted from other sites are refused (`Origin` check, `SameSite=Lax` cookies). The limits are constants at the top of `account.rs` and `post.rs`.
- Anyone can report a listing ("Raporto", a flag in a circle among the buttons under the price; `/listing/{number}/raporto`, `report.html`): what is wrong with it, among seven reasons, each with what it covers ("Mashtrim", "Njoftim i rremë", "I ndaluar ose fyes", "Spam ose i përsëritur", "Nuk është më në dispozicion", "Të dhëna të gabuara", "Tjetër", which needs a word on what), and an explanation if they like (`web/safety.rs`, `db/safety.rs`). It takes being signed in: someone who isn't is asked to, and comes back to the report after (the `kthehu` cookie the hearts use), since reports take listings off the site and a stranger shouldn't be able to do that to someone else's. A person reports a listing once until a moderator decides on it, and 20 listings a day at most; its own seller can't. When three people (`HIDE_AFTER`) have reported a listing for a serious reason (a scam, fake, prohibited or offensive, spam: not "wrong details", which is for a moderator to see to), it comes off the site (`SUSPENDED`): gone from the pages and its link, but for its seller, whose "Njoftimet e mia" says "Në shqyrtim" and whose page says why, and the moderators; the status history and `moderation_actions` record it as the rule's doing. On `/moderim` the reported listings come first, those taken off the site first, each with how many reported it, the reasons with how many gave each, what they wrote, and two buttons: "Mbaje" dismisses its reports and puts it back as it was (active or reserved), "Hiqe" takes it down for good (`REJECTED`, "Hequr nga moderatorët" to its seller, who can still delete it). "Për miratim (N)" on a moderator's "Njoftimet e mia" counts these too.
- A signed-in visitor can block a seller ("Blloko shitësin" under the seller's buttons, after the browser asks to be sure; `/listing/{number}/blloko`, `user_blocks`): the account that posted the listing, and so all it posts, leaves their welcome page, category and search pages, the map, "Më të klikuarat", the newest listings' row and the listings under another, filtered in the queries (`ListingFilter::hidden_accounts`) or, for the rows read from memory, after (`safety::without_hidden`). A blocked seller's listing still opens from its link, saying "E ke bllokuar këtë shitës" with "Zhblloko shitësin", and "Njoftimet e mia" lists those blocked ("Ata që ke bllokuar"), each with "Zhblloko". It is the one block a conversation's "Blloko" makes too (`user_blocks`, `web/messages.rs`): whichever page it was made on, the blocked person's listings leave the blocker's pages, neither can write to the other, and either page lets them be again. The pages find the visitor's blocks with one query when there is a session and none otherwise (`blocked_for_session`).
- Signing in with a code proves the phone number or e-mail address it was sent to (`user_verifications`, type `PHONE` or `EMAIL`, set by `account::check_code`). A listing whose shown number (or address) is one the account that posted it proved has a shield beside the seller's name, in the green of a good price, which says "Numër i verifikuar" (or "Email i verifikuar") as a tooltip and to screen readers; a profile says it in words under the name, and a business's when one of its people proved its public number. A number changed since isn't verified. Since anyone can post under any number without a code, a listing posted that way under a proved number shows it too: it goes to that number's account, whose owner sees it on "Njoftimet e mia" and can delete it. In test mode (`SIGN_IN_CODES_ON_SCREEN`) anyone can prove any number.
- Every select is chosen from a list in the page, not the phone's own picker (a wheel on iPhones, a dialog on Android): categories, towns, the kind of listing, the condition, the price, the currency, the order, a car's make and model. `picker_controller.js` puts a button where the `<select>` was and opens a list under it, styled like the order's menu, groups under their headings (a town under its county). A list with groups folds, towns under their county and models under their make, as does a category list (`data-picker-tree-value`): a group's or a category's arrow opens it, the list opens with only the path to the choice open, and a list of one group (the models of the make chosen) comes open; a list wider than a narrow field grows from the field's side nearer the middle of the screen; where a category can be chosen itself (the welcome form, the filters, the map), its name chooses it and its arrow opens it. The posting form's groups nest by their path ("Automjete › Pjesë dhe aksesorë"), the other lists by the levels `category_choices` gives them. The select stays, unseen, and is what the form sends; choosing sets it and fires its `change`, and the button follows the select however a script sets it (the kind of listing a category takes, its currency, a model that no longer fits its make), the list being built anew each time it opens. It takes the arrow keys (right and left open and close a category), Enter, Escape and a first letter; without JavaScript a select is just a select.
- A category page says where it is and where to go next in pills. On a line of their own, small, each with its category's icon, after a round back button as tall as they are, which goes to the page before as the browser's does when that was one of the site's pages in this tab (Turbo numbers its history entries, `restorationIndex`), and a level up otherwise or without scripts (the welcome page from a main category; `back_controller.js`): the category above it, quieter, a way up, with no arrow of its own; and the page's own, filled as the row's buttons are when set, which is the heading and, tapped, shows the other categories at its level as small pills after it on its line, each with its icon, and hides them again on a second tap (`level` in `browse.rs`: those beside it, not itself; for a main category, the other main ones after "Të gjitha njoftimet"). The pill is a `<details>` holding only itself, and the pills stand after it (`.category-level`), its box giving way to them when it is open (`display: contents`), so that they take their places on the line as the pills before them do; it opens and closes without scripts, no tap outside closes it as one would a menu, and it stays open while the page refreshes in place (`disclosure_controller.js`). Under that line, its subcategories as pills like the welcome page's main categories, a touch smaller than those, when it has any. No breadcrumb, and a listing's page has the back button and the first two pills in its place above the photo (`listing.html`, `.listing-path`: the back button, leading to its category without scripts; the category above its own, quieter, and its own, filled, each a link to its page; `parent` and `category` in `listings.rs`, with `category_icon` from `browse.rs`): search engines get the page's trail as schema.org `BreadcrumbList` data instead (`seo.rs`), and the count stands on the order's pill's line, with room kept for the most it can say ("10 000+ njoftime"), so that the pills after it stand still as the count past 1 000 comes in. Under "Më të klikuarat", or under those pills where the page has none, the listings have a heading of their own, like that row's, with the count, the order's pill and the view's on its right (`.results-head`; on a phone, on the line under it, still on the right, the order's menu growing leftward so that it stays on the screen): their order, the category and, when one is chosen, their kind ("Njoftimet më të reja në Makina", "Njoftimet më të lira në Prona me qira"; `listings_heading` in `browse.rs`, the order as an adjective from `Filters::sort_adjective`), so that the list says what it is where it starts; a search across the categories and the page of every listing keep the order alone as their heading, for screen readers.
- The import map in `templates/base.html` loads the JavaScript modules directly, with no bundler. To upgrade Turbo or Stimulus, replace its file in `static/vendor/` and update the import map.
- The site follows the device's light or dark setting until the visitor picks a theme with the header toggle. The choice is kept in `localStorage`, and a small inline script in `base.html` applies it before the first paint, so pages never flash the wrong theme.
- Everything but the logo is set in one typeface, [Geist](https://vercel.com/font), Vercel's: headings, prices and text differ only in weight and colour. It is a variable font, every weight in one file (29 KB, Latin subset, which covers Albanian), in `static/vendor/fonts/` with its licence (`OFL-Geist.txt`). The site serves it itself, so visitors make no extra connection and their IPs don't go to Google or Vercel, and `base.html` preloads it so text rarely waits for it. `base.css` names the weights it uses: light (300) for the welcome page's intro line, regular (400) for body text, medium (500) for chips and detail values, semibold (600) for headings, labels, card titles and buttons, and bold (700) for prices. Its sizes are a scale of seven, `--text-xs` to `--text-3xl` (12.8, 14, 16, 18, 20, 25.6 and 36 pixels, in rem so that they shrink with the page on phones), from captions and labels through body text, leads, section headings and page titles to a listing's price; the only others are fixed where a pixel size matters (form fields at 16px at least on phones, which iPhones zoom into otherwise; the posting form's car fields at 14px; the map's bubbles at 11px), what follows a price, at half its size, and a card's title, at .95rem (13.3px on a phone, `--text-sm` in the welcome page's row of the newest), a step over the details line under it, whose town read nearly as large before; that line's pin is the size of its letters.
- Every page ends in a footer with the pages about the site (`web/pages.rs`): "Si funksionon", "Këshilla sigurie", "Kushtet", "Privatësia" and "Kontakt", where people look for them when deciding whether to trust a site. Each is a column of text written to be read, with its description and canonical address for search engines, and in the pages' sitemap. The privacy page says what the code keeps (the sign-in contact and name, hashed codes and sessions with the IP a code was asked from, listings, messages, the listings kept, blocks and reports, views counted by day without who saw them), the cookies, who else gets data (the code's channel, the photo store, OpenStreetMap's tiles, Anthropic for "Plotëso me AI"), and for how long: change it when that changes. The contact page gives the address `CONTACT_EMAIL` names.
- The site is called Pluribus. Its logo is a creature, spinning and changing shape (`thinking.svg`), in the buttons' coral, with its right half behind the p of the name in lowercase, "pluribus", in [Merriweather](https://github.com/EbenSorkin/Merriweather4) Bold, a serif, in the text's colour, white on the dark theme and charcoal on the light one (`.logo`): the font's Latin subset (48 KB, from Fontsource 5.3.0) is in `static/vendor/fonts/` with its licence (`OFL-Merriweather.txt`), and `base.html` preloads it as it does Geist; Georgia stands in until it comes. Turbo keeps the logo from page to page (`data-turbo-permanent`), so the creature goes on moving rather than starting over, and it stands still for visitors who ask for less motion (`application.js`). Its changing shape is redrawn by the main thread every frame (SVG's own animation, which the GPU can't take over): on Chrome's phone profile (the CPU four times slower) it keeps the main thread busy a third of the time while a listing's page sits still, and half of it on the welcome page; a second, large one behind the welcome page's search took that to two-thirds, and went. Titles and text say "Pluribus" ("Shto një njoftim | Pluribus", "Në Pluribus që nga tetor 2026"); the code, the crate and the database keep the name marketplace.
- Phones get everything smaller: the root size drops from 16px to 14px below 720px (13.5px at 360px and under), and everything sized in `rem` follows. Form fields keep 16px text, because iPhones zoom into a focused field with smaller text, and get less padding instead. A listing's page has a few things smaller still there, so that its top isn't all large type: the price 24.5px (1.75rem) rather than 31.5, a little over the title's 22.4 (36px from 720px up), the path to its category at the text's 14px rather than a heading's 17.5, and its detail pills at 13px with 16px icons. The pill over its photo says where and when on one line, the when in words within the week ("Sot", "Dje", "para 3 ditësh"; `format::when`) and as a date from a week on, both by Albania's clock (`format::date`, which follows Europe's summer time); its number ends the line under the title ("Në shitje · E përdorur · Numri i njoftimit: 1096191").
- Turbo, Stimulus, Leaflet and the fonts carry their version in the file name, and the site's own CSS and JavaScript a fingerprint of the files in their address (`base.css?v=8d6d43be`, and every module through the import map, which names each with it; from `web::asset_version()`), so browsers keep them all for a year without asking again (`immutable`, `web::cache_headers`). A page loaded in full, after the first, asks the server for none of its CSS and JavaScript, where it asked again about 33 of its 34 before painting and starting: on Chrome's phone profile (the CPU four times slower, 150 ms a request) the welcome page paints in 228 ms rather than 624, and its scripts are ready in 0.2 s rather than 1.3. A deploy that changes the files changes the fingerprint, and so their addresses; and since Turbo keeps the stylesheets and scripts of the first page a visitor opens while they click around, it tracks those tags (`data-turbo-track="reload"`): after a deploy, an open page reloads in full on its next click instead of running on with the old files. An address with another fingerprint, from a page loaded before a deploy, still gets the file, but checked on every use. Every module is preloaded with the page (`<link rel="modulepreload">`, `web::js_modules`), so a first visit fetches them all at once rather than `application.js` and only then what it imports. The fingerprint is worked out once per server start, so restart the server after changing CSS or JavaScript (`dev/deploy.sh` does).

### Templates

Askama templates in `templates/`, compiled into the binary, so a template change needs a build. `base.html` holds what every page shares; the rest extend it or are included in one another.

| Template | What it renders |
|---|---|
| `base.html` | Every page's frame: the head (the theme set before the first paint, view transitions, the fonts preloaded, the eight stylesheets, the import map with every module at its fingerprinted address, every module preloaded, `application.js`), the header with the logo and its creature, "+", the hearts, the messages, "Llogaria" and the theme toggle, and the footer with the pages about the site |
| `meta.html` | What a page says about itself in its head (`web::seo::PageMeta`): its description and canonical address, `noindex` on a search's results and a sold listing, the Open Graph tags of link previews, a listing's price and its schema.org data; the pages without it have `base.html`'s description of the site |
| `home.html` | The welcome page: the search pill, the main categories' pills, "Më të klikuarat", "Sapo u shtuan" and the newest listings |
| `browse.html` | A category or search page: a category's pills (the way up and the category itself, small, on a line, the other categories at its level that it shows as pills after it, and its subcategories under), or a search's heading and the subcategory pills under it, the row of buttons and what they open (the search, the map), the filters' card with its head and foot, "Më të klikuarat", the listings' heading with the smaller pills on its right (the count, the order and its menu, the cards' view and its menu; alone at the results' head where the heading is for screen readers) (on a category page their order, the category and their kind; elsewhere the order alone, for screen readers), the listings, and what to do when there are none |
| `filters.html` | The filter form, from the category's filterable properties: the place, the price (typed, its track, its quick choices), the filters buyers use most, the rest under "Më shumë filtra" |
| `filter_field.html` | One property's filter: a dropdown, boxes, a range typed or picked from steps, a year, a box to tick |
| `filter_type.html` | The kind of listing (Shitje, Qira...), first for property and under "Më shumë filtra" elsewhere |
| `map.html`, `map_radius.html`, `map_town.html` | The map page and its controls: the distance around the pin, which the zoom sets, and a town to move it to (a category's card has its own, in `browse.html`) |
| `listing_grid.html` | A grid of listing cards, and the frame of the next page |
| `listing_card.html` | A listing's card: its photos (one, or all on a big card), the price, by how much it dropped lately, the title, the start of its description (in the list alone), the details line, when it was posted (shown in the list alone), the heart, and its views on "Më të klikuarat" |
| `listing_frame.html`, `next_page.html` | The next page of cards, which Turbo loads as the frame scrolls into view |
| `newest_card.html`, `newest_stream.html` | A card of the newest listings' row, and the Turbo Stream that brings a new one |
| `heart.html` | The heart over a listing's photo |
| `listing.html` | A listing's page: the photos, the place and date over them, the views, the title, with what it is and its number under it, the price and its bubbles, the buttons under them, the details, the description, the seller's card with the buttons again, small, their other listings and the related ones; among the buttons, "Raporto", and under them, "Blloko shitësin"; beside the seller's name, the verified shield; why it is off the site, for its seller and the moderators |
| `report.html` | Reporting a listing: the reasons, each with what it covers, and an explanation; for someone not signed in, the way to sign in; once sent, thanks |
| `area_price.html` | A property's price per square metre, its card, and its zone's average |
| `phone_stream.html` | The seller's number or address, revealed in place under the price, with its copy button, and again at the seller's line |
| `listing_form.html` | The posting and edit form |
| `listing_properties.html`, `listing_properties_frame.html` | A category's properties in the form, and the frame that brings them when the category changes |
| `my_listings.html` | "Njoftimet e mia": what the listings drew, all together (views, contacts, saves, each with the last seven days' under it), then each listing with its price, its own counts and its buttons; those the user blocked, on a listing's page or in a conversation, each to let be again |
| `saved.html` | The listings the user keeps, under those whose price dropped since they last looked, from what to what |
| `messages.html` | The inbox: the visitor's conversations, those with messages unread in bold |
| `conversation.html` | A conversation, or the page that starts one: who with, the listing, the messages, the box to write in (with the quick first messages), blocking; in the `conversation` frame, which a listing's message sheet takes it into |
| `message.html` | One message in a conversation, the visitor's own on the right |
| `messages_stream.html` | New messages, as a Turbo Stream that adds them at a conversation's end |
| `price_drop.html` | After a listing's price, when it dropped lately: what it was, struck through, and by how much |
| `profile.html` | A seller's profile, with "Numër i verifikuar" when it is |
| `sign_in.html`, `sign_in_code.html` | Signing in: the contact, then the code |
| `moderation.html` | The moderators' page: the reported listings, each with who reported it, why and what they wrote, to keep or take down; then the names sellers typed in |
| `suggestions.html` | The search boxes' suggestions |
| `not_found.html` | The 404 page |
| `how_it_works.html`, `safety_tips.html`, `terms.html`, `privacy.html`, `contact.html` | The pages about the site (`web/pages.rs`), each a column of text; the terms and the privacy page say when they last changed |
| `thinking.svg`, `pin.svg`, `phone.svg`, `mail.svg` | Drawings included where they are used: the logo's creature, the location pin, the phone and the envelope |

### Stylesheets

`static/css/`, eight files that `base.html` loads in this order (`web::STYLESHEETS`). The cascade follows it, so a rule meets the others where it would in one file. Colours, sizes and weights are custom properties in `base.css` (`--accent`, `--button`, `--text-sm`, `--semibold`...), with the dark theme's values beside the light theme's.

| File | What it styles |
|---|---|
| `base.css` | The fonts, the colours of both themes, type sizes and weights, the page's base, the header and the logo |
| `page.css` | The page's frame and its footer, the listing cards (small and big, and the list's lines), their hearts and view counts, the subcategory pills' row, form fields, and the pages about the site (`.text-page`) |
| `welcome.css` | The welcome page: the search pill and its suggestions, the in-page lists that stand in for selects (pickers), the main categories' pills (which a category page's subcategories wear too), the newest listings' row and the live dot by "Sapo u shtuan", whose pulse is a copy of it that grows and fades (`transform` and `opacity`, which the GPU draws without the main thread) |
| `browse.css` | Category and search pages: the row of category pills and those the selected one shows after it, a search's heading, the row of buttons (round, or pills with words and a count, as big as the category pills) and what they open, the results' head (the heading, and the smaller pills on its right: the count, the order, the cards' view, its menu at the line's right end), the filters' card with its head and foot, the price's fields, track and quick choices, "Më shumë filtra", what to do when nothing is found |
| `map.css` | The map, on its own page and over a browse page's results, where it fills the card, a square as wide as the row or as high as the screen has left, its circle, the pills of "Vendndodhja ime" and the towns' menu and the distance badge over it, the order's menu |
| `posting.css` | Signing in, and the posting and edit forms: photos, a car's tiles, the AI's button |
| `listing.css` | A listing's page: its category's pills over the photo (styled by `browse.css`'s `.category-path`), the gallery, the price's bubbles, the buttons, the seller's card, the details' pills, the rows under it ("Më shumë nga ky përdorues", "Më të klikuarat"'s shared row style) |
| `messages.css` | Messages: the header's unread count, the inbox, a conversation's bubbles and the box to write in, a listing's message sheet |

### JavaScript

`static/js/application.js` starts Stimulus, registers the controllers below and holds the few behaviours every page shares: the view transitions skipped where the browser animated a back or forward itself, the photos of the page Turbo keeps for going back shown at once, a tapped card keeping its border while its listing loads, a listing's page reporting its view, and a link that drops the town forgetting it (`data-forget-town`). `controllers/refresh.js` is what the filters' and the map's controllers share: refreshing the page in place; `controllers/locate.js` asks the browser where the visitor is, for "Afër meje" and "Vendndodhja ime"; `controllers/signed_in.js` tells the hearts' and the messages' controllers whether someone is signed in (the `llogari` cookie), so that they ask the server for nothing otherwise. `application.js` also starts fetching a page as a finger touches its link (see "How the frontend works").

| Controller | What it does |
|---|---|
| `ai_fill_controller.js` | "Plotëso pjesën tjetër me AI": sends the form's title, description and photos, and fills in the answer as a seller would |
| `back_controller.js` | The round back button before a category's pills: the page before when it was the site's, in this tab; else its link, a level up |
| `cards_controller.js` | The view menu: small cards, big ones or the list, remembered in a cookie |
| `conversation_controller.js` | A conversation, on its page or in a listing's message sheet, which it scrolls in place of the page: opens at its end, reads what it shows, asks for new messages every 8 seconds; the box that grows, Enter to send, the quick messages |
| `copy_controller.js` | Copies the seller's number or address, a check for a moment |
| `disclosure_controller.js` | A `<details>` that shows more of the page in its place rather than a menu over it (a category page's own pill, the other categories at its level): kept open while the page refreshes in place |
| `drag_row_controller.js` | A mouse drags the rows that slide sideways ("Më të klikuarat", the seller's other listings, "Sapo u shtuan"), flicking them on, the click that ends a drag opening no card |
| `dropdown_controller.js` | A `<details>` menu closes on a tap outside or Escape; one of the browse row's, open one at a time |
| `filters_controller.js` | The filters' card: applies them as they change, keeps what is open while the page refreshes, fits the card under the row; its Qyteti and "Afër meje"; asks for the count past 1 000 for its button and the count by the order's pill |
| `gallery_controller.js` | A listing's photos: the counter and the arrows |
| `heart_controller.js` | The hearts: fills those of the listings kept, keeps one or lets it go at a tap; puts the dot on the header's heart while kept listings dropped their price unseen |
| `inbox_controller.js` | The header's count of conversations with messages unread |
| `listing_form_controller.js` | The listing form follows its category: its kinds of listing, condition, currency and properties |
| `loan_controller.js` | The loan calculator under a price |
| `map_controller.js` | The map: its pin in the middle, the circle, the same size whatever the zoom (in a category's card once a point is set), the distance the zoom sets, the dots, "Vendndodhja ime" |
| `message_sheet_controller.js` | A listing's message sheet: opens under the message button, the page scrolled to bring it near the top, and takes the rest of the screen; closes on "Mbyll", Escape or Turbo caching the page, emptying its frame; a response without the frame opens as a page |
| `narrow_controller.js` | A select that depends on another (a car's model on its make, a town's zones) offers only what fits |
| `newest_controller.js` | The newest listings' row: new cards come in one by one, and wait while the row is slid along |
| `panel_controller.js` | The browse row's search box, opened under the row |
| `photos_controller.js` | The listing form's photos: picked a few at a time, previews, "Hiq", shrunk before sending |
| `picker_controller.js` | A select chosen from a list in the page rather than the phone's own picker; its button and list stay through a refresh in place |
| `place_controller.js` | Keeps the town a browse page is in, in a cookie, for the next pages |
| `price_range_controller.js` | The price filter: typed, its track with two ends and its quick choices, each moving the others |
| `share_controller.js` | Shares a listing, or copies its link |
| `slides_controller.js` | A big card's photos: the dots, the arrows' quick steps |
| `suggest_controller.js` | The search boxes' suggestions as words are typed, and the keys that go through them |
| `theme_controller.js` | The light or dark theme, remembered |

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

Pjesë dhe aksesorë goes one level further, as on MerrJep: Pjesë këmbimi, Goma dhe disqe, Audio dhe video, GPS dhe navigatorë, Styling dhe pajisje sportive, Tapete makine, Rimorkio, Mbushës për makina elektrike and Të tjera (migration 0012; Pjesë këmbimi kept its address, `/c/pjese-kembimi`, and its listings). Nothing in the code counts levels: a category page's pills lead one level up and across its own, however deep it is, the posting form groups the kinds under "Automjete › Pjesë dhe aksesorë", and a category's page lists everything under it, however deep.

A category page shows its subcategories under its heading as pills like the welcome page's main categories, each with its icon where it has one (`icons::subcategory` in `icons.rs`: every subcategory has one, from Automjete's car from the front, motorbike, truck and wrench to the kinds of parts' nut and tyre, Prona's blocks of flats, Elektronikë's laptop and headphones, and Makineri dhe biznes's excavator and tractor; a subcategory added later shows its name alone until it has one), all at once on as many lines as they need. Links to a narrower category, the subcategory pills and the menu's entries under the page's category, keep the page's filters, since a subcategory has its parent's properties: "Audi, 2006" on Pjesë dhe aksesorë stays on in GPS dhe navigatorë (`Filters::narrowed_url`). Links up the tree or across it keep only the search words, the town and the map pin (`Filters::category_url`).

### Properties

Each category's properties are rows in the database, not code. A browse page builds its filters from the category's `filterable` properties, listing cards show the `show_in_card` ones after the city ("Tiranë · 2016 · 148 000 km · Naftë"), and the listing page shows the `show_in_details` ones, each with its icon, drawn as the welcome page's categories are (`web/icons.rs`): the category's own where it says more than the property's (a phone's brand is a phone, a dog's breed a dog), and the property's name beside it where the value alone wouldn't say what it is ("Kati 6", but "Euro 6"); a box ticked shows its name ("Pa aksident"). A new property needs its icon there, or a test fails. What `0003_category_properties.sql` sets up, and the migrations named after it; none of it is required but a car's make, model and year:

| Category | Properties |
|---|---|
| Makina | make, model (grouped by make), year, mileage, fuel, gearbox, body type, colour, options ("Opsionet"); customs ("E zhdoganuar"), plates, engine (cm³), power (HP), Euro class, drive, doors, seats, owners, and whether it is accident-free, has its service book, inspection and insurance (0023); the posting form shows them two to a line and in one size, the first eight with an icon before the name, the options last; its title is made from its make, model, body type and colour ("Mercedes-Benz S-Class Sedan, e zezë", `listings.generated_title`, kept by `listing_generated_title()` wherever `search_extra` is), the seller's own heading the description and the details leaving those four out |
| Motoçikleta, Automjete tregtare (0023) | make, from lists of their own, kind (scooter, naked, enduro, ATV...; van, pick-up, truck, minibus...), model, year, mileage, customs, plates, fuel; a motorcycle's engine ("Kubatura"), licence (AM, A1, A2, A) and colour; a van's power, gearbox, payload and seats. Their forms are plain ones, the make first, without a car's icons |
| Prona | for every kind: area and amenities (lift, parking, garage, terrace, sea view, title deed...); and since 0023, a flat: rooms (1+1, 2+1...), bathrooms, floor and the building's floors, furnishing, year built, condition, heating, orientation, and for letting it, deposit, bills, pets and students; a studio the same without rooms and bathrooms; a house: its plot's area and its floors rather than a floor; a shop: its window, floor, age, condition and heating; an office: its rooms and toilets besides a flat's floor, age and heating; a plot: what it is for, its frontage, a permit and a road; a garage: its spaces and floor. Each kind offers its own amenities. Rooms, floor and furnishing were every kind's before 0023, and central heating an amenity |
| Elektronikë (0021) | for every kind: brand, warranty, what comes with it (the box, the receipt...); a phone: model (grouped by brand), storage, RAM, battery (%), colour, 5G / Dual SIM / eSIM; a laptop: processor, RAM, storage, disk, screen (inches), graphics, system; a desktop: what it is (gaming PC, All-in-One...), the same and whether a monitor comes with it; a tablet: storage, screen, Wi-Fi or SIM; a TV: size ("Diagonalja", inches), resolution, panel, Smart TV; audio: what it is, wireless, noise cancelling; a camera: what it is (DSLR, mirrorless, lens, drone...), sensor, lens mount, shutter count; a console: the console (grouped by maker), edition, games, controllers |
| Shtëpi dhe kopsht (0023) | furniture: what it is, material, colour, size; appliances: which, brand, energy class, capacity, warranty; garden things: what, material; décor: what, material, colour |
| Punë | schedule; contract, where the work is done (on site, hybrid, from home), experience, education, languages and shift, and in tourism and building, board and meals (0023) |
| Shërbime (0023) | the areas served, whether they come to the client, give an invoice, years of experience, when; repairs, cleaning, photography and building work: what they do; lessons: subject, level, online or in person; transport: the vehicle, its passengers and load |
| Veshje dhe aksesorë | for whom, brand (each kind's own), colour; jackets and dresses: size, a child's size, material; shoes: size, EU 20 to 48, material; bags and accessories: material; watches: movement, material (0023; size was every kind's before) |
| Sport dhe hobi (0023) | a bicycle's kind, wheels, frame, brand, frame material and gears; fitness equipment; an instrument, its kind and brand; camping and fishing gear; winter sports gear, its length and a boot's size |
| Kafshë shtëpiake (0023) | dogs and cats: breed (each their own), age, sex, vaccinated, chip or passport, pedigree, neutered; birds: kind, age, sex; fish: kind; accessories: what, and for which pet |
| Makineri dhe biznes | year; brand, model and working hours, and for building, farm and energy machines what they are, power, weight, a tractor's drive, a generator's fuel and phase (0023) |
| Pjesë dhe aksesorë (0012) | the vehicle a part fits, none required: make and model from the cars' lists ("Marka e automjetit", "Modeli i automjetit"), and the years from and to; a spare part's number, a tyre's kind, rim, width, profile, season and how many, a trailer's capacity, a charger's power and connector (0023) |

The car makes and models are migration 0013's list: 42 makes common in Albania (Alfa Romeo to Volvo, Iveco and the Chinese BYD and MG included) with 556 models, in alphabetical order, numbers first. Their codes are the ones the posting form gives a name typed under "Tjetër" (`format::slug`, upper case, a model's after its make's: `VOLKSWAGEN_ID_BUZZ`), so a typed "id buzz" is the listed ID. Buzz. To add one, write a migration like 0013's, or approve one a seller typed.

To add a property, write a new migration:

```sql
INSERT INTO attributes (code, name, name_i18n, data_type, unit)
VALUES ('tv_refresh_rate', 'Frekuenca', '{"en": "Refresh rate"}', 'NUMBER', 'Hz');

INSERT INTO category_attributes (category_id, attribute_id, filterable, show_in_card, sort_order)
SELECT c.id, a.id, true, false, 45
FROM categories c, attributes a
WHERE c.slug = 'televizore' AND a.code = 'tv_refresh_rate';
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

- Subcategories inherit their parents' properties, from every level above them. The `effective_category_attributes` view resolves that, and where a subcategory configures a property its parent has too, the subcategory's settings win. A subcategory can't leave out one it inherits, so a property only some kinds have sits on those kinds: Prona's rooms on Apartamente and Vila dhe shtëpi, not on Prona, whose Truall has none.
- A category can offer only some of a property's options, in its own order (`category_attribute_options`, migration 0021): phones' brands aren't cameras', and a laptop's storage starts at 128 GB where a phone's starts at 32. The form, the filters and the generator's check all read `category_options(category, property)`, which gives a category's own list where it has one and otherwise every option, as a parent category's filters offer every brand. An option a seller typed and a moderator approves is offered in the categories it was typed in, before "Tjetër". Migration 0023's shared lists work the same way: shoes' brands aren't watches', nor a dog's breeds a cat's, and a garden's materials aren't a dress's.
- `category_attributes.label` is a category's own name for a property, where the property's name would mislead: on a part, "Marka" would read as the part's own brand, so there it is "Marka e automjetit". It is inherited like the other settings; NULL means the property's name.
- Property codes become URL parameters, so don't name one `q`, `category`, `city`, `lat`, `lng`, `radius`, `type`, `condition`, `price_min`, `price_max`, `currency`, `sort`, `after`, `harta` or `numri`; a year pair's filter takes `<x>_year` too.
- A property whose code ends in `_year` can't be later than next year, and in a year pair the second can't come before the first.
- `categories.listing_types` lists the kinds of listing a category takes (sale, wanted, rent...); with more than one, the filter offers them. `categories.uses_condition` is false where new or used means nothing: property, jobs, services and pets.
- Searchable property labels reach full-text search through `listings.search_extra`, followed by the names of the listing's category and of those above it, in Albanian and English, and of its place (migration 0017). After saving a listing's values, category or place, run `UPDATE listings SET search_extra = listing_search_extra(id) WHERE id = $1`; that is how "telefon" finds an iPhone, and "laptop shkoder" the laptops in Shkodër. Renaming a category or a place calls for the same for its listings.
- `format_attribute_number(value, unit)` formats numbers for display: grouped with the unit when there is one ("148 000 km"), as they are when there isn't (a year).

## Photos

Sellers add up to 8 photos to a listing, on the posting and edit forms; the first is the cover, which cards show. When editing, each photo has "Hiq" to remove it (the next one becomes the cover). On the listing's page, each photo shows whole in a 6:5 frame (on a wide screen, across the page and up to 70% of the screen's height), never cropped: where it doesn't fill the frame (beside a tall photo, above and below a wide one), the photo shows again behind it, blurred, from its tiny copy (below). Several photos make a strip to swipe through one at a time, with a counter in its corner ("2 / 5") and, on screens with a mouse, arrows (`gallery_controller.js`); one photo is just the photo.

- **In the browser** (`photos_controller.js`): photos can be picked a few at a time, as phones offer them: each pick adds to the ones before (a file input alone keeps only the last pick), each new one shows as a preview with "Hiq" to take it out, and big phone photos are made smaller before they go (longest side 1600 px, the size the server keeps, as JPEG), so posting doesn't send megabytes over mobile data.
- **On the server** (`src/media.rs`, `src/web/photos.rs`): each photo is decoded (JPEG, PNG or WebP; at most 15 MB as sent, 64 MB for the whole form), turned the way the camera held it, and written out again, which drops what phones keep inside photos, the GPS position they were taken at included. It is kept as WebP (libwebp, quality 80) in three sizes: 1600 px on its longer side for the listing page on big screens, 1200 px for phones and big cards (none for a photo no bigger), and 600 px for cards. Beside them: a JPEG of the full one (mozjpeg, progressive, quality 80), for link previews (`og:image`) and browsers without WebP; and a 16 px copy, about 130 characters as a `data:` URI, which goes in the pages themselves. A 1200 × 1600 phone photo the site kept as a 213 KB JPEG before is now 72 KB, 44 KB in the middle size and 16 KB on a card, and its JPEG 139 KB; making them all takes about a quarter of a second. `listing_media` keeps the key, every copy's URL, the tiny copy, the size and a SHA-256 of what was sent.
- **In the pages**: a photo kept as WebP is a `<picture>`: a WebP source with its three copies, each with its longer side as its width (`photos::srcset`), and the JPEG in the `<img>`, which browsers that show WebP never fetch. The page says how wide the photo shows (`sizes`: the cards' in `ListingPage::sizes`, the listing page's in `GALLERY_SIZES`) and the browser fetches the smallest copy that is sharp on its screen: on a phone the middle one on the listing page and the card's on a small card; on a laptop the full one. The tiny copy shows at once: on the listing page, blurred around the photo where it doesn't fill its frame and under it until it loads; on a card, under its first photo until it loads (`.card-photo`'s background). Photos kept before migration 0025 stay as they were: JPEGs, their card-sized copy blurred around them.
- **Storage**: an S3-compatible store. Locally that is [Garage](https://garagehq.deuxfleurs.fr/) in Docker (`compose.yaml`, `dev/garage.toml`, set up once by `dev/setup_garage.sh`); in production, Cloudflare R2. The app talks plain S3 to either (the `object_store` crate), with the `S3_*` settings in `.env`. Without them, photos are kept in memory, which is what the tests use. Each file is stored with its type and `Cache-Control: public, max-age=31536000, immutable`, which the store sends with it. A photo's files share its name: `photos/<32 hex digits>.webp` (its key in `listing_media`), `-middle.webp`, `-card.webp` and `.jpg`; removing a photo deletes them all.
- **Serving**: the site serves photos itself from `/media/photos/...`, with year-long caching (their names never change), so they work wherever the site does, through a tunnel included.
- **Through a CDN**: put the CDN in front of the whole site, and it keeps `/media/...` as their headers allow; or have it serve the bucket (an R2 bucket's public domain, `https://foto.example.al`) and set `MEDIA_URL` to that address: new photos' URLs point there, and their files bring their type and caching with them. Photos kept before keep the URLs they were stored with; to move them, after copying their files across:

```sql
UPDATE listing_media
SET public_url = regexp_replace(public_url, '^/media/', 'https://foto.example.al/'),
    middle_url = regexp_replace(middle_url, '^/media/', 'https://foto.example.al/'),
    thumbnail_url = regexp_replace(thumbnail_url, '^/media/', 'https://foto.example.al/'),
    jpeg_url = regexp_replace(jpeg_url, '^/media/', 'https://foto.example.al/')
WHERE storage_key LIKE 'photos/%';
```

Files stored before their type and caching were (before migration 0025) have neither in the store, so a CDN serving the bucket sends them with its defaults; browsers still show them.

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
psql -v ON_ERROR_STOP=1 --single-transaction -f migrations/0001_marketplace_schema.sql -f migrations/0002_marketplace_reference_data.sql -f migrations/0003_category_properties.sql -f migrations/0004_plate_codes.sql -f migrations/0005_map_search.sql -f migrations/0006_rename_patundshmeri_to_prona.sql -f migrations/0007_sign_in_codes.sql -f migrations/0008_subcategories.sql -f migrations/0009_seller_options.sql -f migrations/0010_post_before_sign_in.sql -f migrations/0011_paused_listings.sql -f migrations/0012_parts_and_accessories.sql -f migrations/0013_car_makes_and_models.sql -f migrations/0014_options_name.sql -f migrations/0015_generated_titles.sql -f migrations/0016_fuzzy_search.sql -f migrations/0017_search_categories_and_places.sql
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
- Search forgives typos and finds a word's other forms (migration 0016): "golff" finds the Golfs, "makinat" the listings that say "makinë", "apartamente" the "Apartament". `search_query(words)` turns the words into the tsquery listings are matched against: the websearch syntax as before (`-word`, `"a phrase"`, `or`), each word of four letters or more widened, where it stands, to its other forms (the words listings use with the same rough stem, `sq_stem`: makina, makine, makinat, makinave → makin) and, when few listings use it or none do, to the common word it most likely misspells (`search_correction`: spelt alike by `pg_trgm`, used by more than three times as many listings) with that word's forms. A word many listings use is never corrected, so "audi" stays apart from "audio". It also returns the corrections, which the page names under its heading ("Shfaqen edhe rezultatet për „golf“."). The first eight words are widened; at most 100 characters of search words are taken.
- A listing's category, those above it and its place are words of it too (`listing_search_extra`, migration 0017), so a search finds listings by them as by anything they say: "apartamente tirane" finds the flats in Tirana, "goma" what is under "Goma dhe disqe", "cars" the cars, "shkodra" and "shkoder" the listings in Shkodër (a town in -ër is kept with its article as well, as one ending off doesn't turn either into the other). As words rather than filters, they combine with the rest of a search: "makinë larëse" finds washing machines, which say "larëse", not cars, which only say "makinë". `search_query` leaves out filler words (në, me, për, dhe, e, i, të, një, nga, tek, që), which a listing would otherwise have to say as well: "apartament me qira në tiranë" searches apartament and tiranë, among listings for rent (below); a search of fillers only keeps them. "se" is no filler, for the iPhone SE. "ose" and "apo" mean or: "shtëpi ose vilë".
- Words that name a category, a kind of listing or a town put those filters on (`web/named.rs`): "apartament me qera ne tirane" goes to `/search?category=apartamente&city=tirane&type=RENT`, the flats for rent in Tirana, where each is a chip to take off if it was misread, and the words that name nothing stay searched there ("golf 7 në Durrës": golf 7, in Durrës). Searched as words, they found the listings that say them in any category, with none of the category's filters (rooms, floor, furnished) to narrow them. A town is named in any of its forms (Tirana, Tiranës, Durrësi, Shkodra, Kamza) or by its two or three words (Malësi e Madhe), one of three letters (Mat, Has) only after a word that leads to a place ("në Mat"; "bojë mat" is a colour), and words that name two towns name neither. A kind of listing by the words `search_kind` knows (below), when the category takes that kind and another. A category only in a search across every category, by the first word that is no filler, verb an ad starts with (kërkoj, jepet, ofrohet), kind or town, as the thing sought comes first ("kufje për telefon" is headphones); the word names the categories whose name is about it in any of its forms, a name being about its first word and those "dhe" joins to it ("Vila dhe shtëpi"; "Tapete makine" are mats, so "makinë" is Makina). Of several, the one that takes the kind named, then the one whose name has most of the search's words, most of them as typed, then the shortest name; two still alike name neither: "shtëpi" alone stays a word, as Vila dhe shtëpi and Shtëpi dhe kopsht are both about it, but "shtëpi me qira" is Vila dhe shtëpi, the one that takes rentals. A category's own page reads only the towns and the kinds in its search; the map's pin says where, so it reads none.
- A word that names a kind of listing finds the listings of that kind, as well as those that say it (`search_kind`, migration 0024): shitje, shitja, shitet, shes for sale, qira, qera for rent, blej for those who want to buy. Few cars for sale say "shitje", so "golf 7 në shitje" found nothing while every word had to be in a listing; now it finds the Golfs for sale, and "makina me qira" still finds a rental firm's listing that says "qira", whatever its kind. `search_query` returns such words apart from the rest (`kinds`, and `kind_query`, the words with their forms), and the listing queries take a listing of one of the kinds or one that matches `kind_query`; a word left out ("-shitje") stays a word left out.
- The words listings use are the materialized view `search_words`, from `ts_stat` over the active listings' search vectors. The app rebuilds it at startup and every 15 minutes (`REFRESH MATERIALIZED VIEW CONCURRENTLY`, which searches go on through; about 2 s for 100,000 listings), so a new listing's words reach typo correction within a quarter of an hour, while the listing itself is searchable at once. `tests/fixtures/listings.sql` and `dev/sample_listings.sql` end by refreshing it, and so should anything else that loads listings with psql: `REFRESH MATERIALIZED VIEW search_words;`. The zones' averages a square metre (`zone_area_prices`) are rebuilt along with it; after loading listings with psql, rebuild them too: `REFRESH MATERIALIZED VIEW zone_area_prices;`.
- Pages work a search out once, before their listing queries (`db::text_search`), and pass it to them as a parameter. Worked out inside them, it would be hidden from the planner, and a search that finds nothing would walk the whole newest-first index: 87 ms instead of 0.04 ms on 161,000 listings.
- The search vector weights the title (A) above searchable attribute labels (B, from `listings.search_extra`) and the description (C), so `ts_rank` puts title matches first.
- `sort_at = greatest(published_at, bumped_at)` is the "newest first" key. A bump lifts a listing to the top, and newer listings then push it down. Featured listings get their own slot rather than being sorted into every page.
- Browse and search indexes are partial (`WHERE status = 'ACTIVE'`). After a year most rows are expired or sold, and this keeps the hot indexes small.
- Listings are priced in ALL and EUR. Filter and sort on `price_normalized`, never on `price_amount`. It holds the price in euros, computed with `price_in_eur(amount, currency)` from the `exchange_rates` table; price filters convert their bounds with the same function, so the two always agree. Free listings count as 0; "price on request" has no price and sorts last.
- Trigram indexes on unaccented titles and location names support autocomplete ("korce" finds Korçë).
- Unfiltered, a page reads only its 24 listings off an index already in its order rather than sorting the category: newest first `idx_listings_active_sort`, by price `idx_listings_active_category_cheapest` and `_dearest` (on `coalesce(price_normalized, ±Infinity)`, the price order's key), nearest first `idx_listings_active_category_earth`, a GiST index of the category and the place that gives listings nearest first (KNN). `category_id = ANY(...)` can't use an index for an order, so by price or distance a page over several categories reads each one's first in its own index and merges them (`db::push_ordered`, a LATERAL scan per category); one index of every category's listings in price order would wade through the others' whenever they come first. A category's cheapest first went from 160 ms on half a million listings to 10 ms on a million, and the nearest within 10 km of Tirana from 630 ms to 23 ms.
- The site's connections plan every query for the values it runs with (`plan_cache_mode = force_custom_plan`, set as they open: `db::plan_for_values`). By default, once a connection has run a prepared query five times, Postgres may plan it once for any values (a generic plan), and a filter's values match a handful of listings or tens of thousands, which no one plan suits: on the dev database's million listings, one request in ten for the cars filtered to BMW took 107 ms or more, and half of those for diesel automatics 109 ms or more; planned for their values, they take 16 to 18 ms. Planning every query costs the other pages about a millisecond.
- Radius search ("within 10 km") uses the `cube` and `earthdistance` extensions rather than PostGIS. A listing's point is `listings.latitude`/`longitude`, and the partial GiST index `idx_listings_active_earth` covers the active listings that have one: a 5 km search around Tirana's centre takes about a millisecond (query below). Distances are the cube's `<->`, straight through the earth, which the index orders by; over the surface they would be under 15 cm longer at 50 km. Municipalities have points too, at their seats (from Wikidata, CC0).

This is enough for an MVP and a fairly large site: each example query below ran in under 10 ms against 200,000 listings. When traffic grows, mirror active listings into OpenSearch/Elasticsearch, fed from `outbox_events` so the index never misses a change.

### 5. Analytics are aggregated

`listing_daily_stats` stores daily counters rather than one database row per page impression. Buffer increments (in memory for now: `views.rs` writes the pages seen once a minute; Redis once there are several servers) and flush them periodically, because an UPDATE per page view turns popular listings into lock hot spots. High-volume raw events should go to Kafka/ClickHouse/BigQuery/etc. if needed.

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
- **Chat**: start conversations with `INSERT ... ON CONFLICT (listing_id, buyer_user_id)` (below; `db::start_conversation`). A message's sender must be a participant. A business's listing reaches whoever posted it; add the business's other members as participants when they open a thread, for a shared inbox.
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

Work the search out first, forgiving typos and finding other forms of the words, along with the words it searched in place of misspelt ones:

```sql
SELECT query::text, corrections, kinds, kind_query::text FROM search_query(:words);
-- golff: 'golff' | 'golf', {golf}, {}, NULL
-- golf në shitje: 'golf', {}, {SELL}, 'shitje' | 'shitjes'...
```

Then pass the query in as a parameter, so the planner sees it, with the kinds the words named (a word that names one finds listings of that kind, or that say it):

```sql
SELECT l.id, l.title, l.sort_at
FROM listings l
WHERE l.status = 'ACTIVE'
  AND l.search_vector @@ :query::tsquery
  AND (l.listing_type = ANY(:kinds) OR l.search_vector @@ :kind_query::tsquery)   -- where kinds were named
ORDER BY l.sort_at DESC, l.id DESC
LIMIT 30;
```

For best-match ordering, use `ORDER BY ts_rank(l.search_vector, :query::tsquery) DESC`. Searching the words exactly as typed is `websearch_to_tsquery('sq_unaccent', :words)`.

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
