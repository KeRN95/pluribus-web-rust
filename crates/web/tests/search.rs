//! Searching by words (web/browse.rs's search, named.rs, suggestions.rs): accents, typos and other
//! forms of a word; the words that name a category, a kind of listing, a town or a zone, and put
//! those filters on, in English too; the best matches first; the categories a search found; and the
//! suggestions under the box.

mod common;

use axum::http::StatusCode;
use common::*;
use sqlx::PgPool;

/// The page at `uri`, or the one it sends the visitor on to: a search whose words name filters
/// goes to the page with them on.
async fn followed(pool: PgPool, uri: &str) -> TestResponse {
    let response = get(pool.clone(), uri).await;
    match (response.status, &response.location) {
        (StatusCode::SEE_OTHER, Some(to)) => get(pool, to).await,
        _ => response,
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_with_words_says_what_and_where_for_the_last_searches(pool: PgPool) {
    // What it searched and where, which application.js keeps among the last searches.
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<section class="results" data-controller="lines" aria-labelledby="results-heading" data-search-words="golf" data-search-address="/search?q=golf">"#), "{search}");
    let cars = html(pool.clone(), "/c/makina?q=golf&vehicle_fuel=DIESEL").await;
    assert!(cars.contains(r#"data-search-words="golf" data-search-address="/c/makina?q=golf&vehicle_fuel=DIESEL""#), "{cars}");
    // Words are the visitor's own: what would end the attribute or open a tag is escaped.
    let odd = html(pool.clone(), "/search?q=golf%22%3E%3Cb").await;
    let tag = &odd[odd.find(r#"<section class="results""#).unwrap()..];
    let tag = &tag[..tag.find('>').unwrap()];
    assert!(tag.contains("data-search-words=\"golf&#34;&#62;&#60;b\"") || tag.contains("data-search-words=\"golf&quot;&gt;&lt;b\""), "{tag}");
    // A page without words has nothing to keep.
    assert!(!html(pool.clone(), "/c/makina").await.contains("data-search-words"));
}

/// A search's listings, count and categories as one plan reads them: as Postgres plans them, or as
/// after a query ran past its first try (db::ListingFilter::matches), for few matches (through
/// the words' index) or many (in order).
async fn search_read(pool: &PgPool, words: &str, categories: Option<Vec<sqlx::types::Uuid>>, sort: pluribus_db::Sort, few: Option<bool>) -> (Vec<i64>, i64, Vec<(sqlx::types::Uuid, i64)>) {
    let search = pluribus_db::text_search(pool, Some(words)).await.unwrap();
    let filter = pluribus_db::ListingFilter { search, category_ids: categories, sort, ..Default::default() };
    if let Some(few) = few {
        filter.matches.set(few);
    }
    let cards = pluribus_db::listing_page(pool, &filter).await.unwrap();
    let count = pluribus_db::listing_count(pool, &filter).await.unwrap();
    let found_in = pluribus_db::listing_categories(pool, &filter, 8).await.unwrap();
    (cards.iter().map(|card| card.public_no).collect(), count, found_in)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_reads_the_same_listings_through_its_words_index(pool: PgPool) {
    use pluribus_db::Sort;
    let vehicles: Vec<sqlx::types::Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE sub AS (SELECT id FROM categories WHERE slug = 'automjete'
                                UNION ALL SELECT c.id FROM categories c JOIN sub ON c.parent_id = sub.id)
         SELECT id FROM sub",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // Words that meet often and seldom, alone and among the vehicles, a kind's word, in each order.
    for (words, categories) in [
        ("golf", None),
        ("tirane", None),
        ("golf", Some(vehicles.clone())),
        ("apartament", Some(vehicles.clone())),
        ("makina apartament", None),
        ("shitje", None),
    ] {
        for sort in [Sort::Relevance, Sort::Newest, Sort::PriceLow] {
            let planned = search_read(&pool, words, categories.clone(), sort, None).await;
            let indexed = search_read(&pool, words, categories.clone(), sort, Some(true)).await;
            let walked = search_read(&pool, words, categories.clone(), sort, Some(false)).await;
            assert_eq!(planned, indexed, "{words} {sort:?}");
            assert_eq!(planned, walked, "{words} {sort:?}");
        }
    }
    let (golf, ..) = search_read(&pool, "golf", Some(vehicles.clone()), Sort::Newest, Some(true)).await;
    assert!(golf.contains(&1001), "{golf:?}");
    let (none, count, found_in) = search_read(&pool, "apartament", Some(vehicles), Sort::Relevance, Some(true)).await;
    assert_eq!((none.len(), count, found_in.len()), (0, 0, 0));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_ignores_accents(pool: PgPool) {
    // "vlore" names the town, which the page then filters by, and "shtepi" is searched there.
    let response = followed(pool, "/search?q=shtepi+vlore").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Shtëpi private në shitje në Vlorë"));
    assert!(!response.body.contains("Apartament 2+1"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_matches_property_values(pool: PgPool) {
    // Neither the title nor the description says "telefon"; its type property does. (Searched
    // across every category, "telefon" names Telefona, which the page would filter by instead.)
    let body = get(pool, "/c/elektronike?q=telefon").await.body;
    assert!(body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(!body.contains("Laptop Lenovo ThinkPad T14"));
}

/// The listings a search finds, by number, on the page it goes to if its words name filters.
async fn found(pool: &PgPool, words: &str) -> Vec<i64> {
    card_numbers(&followed(pool.clone(), &format!("/search?q={words}")).await.body)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_that_name_a_kind_of_listing_find_that_kind(pool: PgPool) {
    // The Golf is for sale, though it doesn't say so: "në shitje", "shitet" and "shes" find it as
    // "golf" does, "me qira" doesn't.
    for words in ["golf+7+ne+shitje", "golf+7+n%C3%AB+shitje", "shitet+golf", "shes+golf+7"] {
        assert_eq!(found(&pool, words).await, [1001], "{words}");
    }
    assert!(found(&pool, "golf+me+qira").await.is_empty());
    // The flat is for rent, and "me qira" finds it; "në shitje" doesn't.
    assert_eq!(found(&pool, "apartament+me+qira").await, [1003]);
    assert!(found(&pool, "apartament+ne+shitje").await.is_empty());
    // A listing that says such a word is found by it, whatever its kind: the Mercedes, for sale,
    // can be rented too.
    sqlx::query("UPDATE listings SET title = 'Mercedes-Benz C 220, 2018, edhe me qira' WHERE public_no = 1002")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(found(&pool, "mercedes+me+qira").await, [1002]);
    // The kind alone finds every listing of it, and a kind left out stays out.
    assert!(found(&pool, "me+qira").await.contains(&1003));
    assert!(found(&pool, "golf+-shitje").await.contains(&1001));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn an_english_search_names_its_filters_in_english_words(pool: PgPool) {
    // On an English page, English's words name a category (by its English name, singular or
    // plural), a kind and a town, as Albanian's do.
    let found_in_english = |words: &'static str| {
        let pool = pool.clone();
        async move {
            let response = get_with_cookie(pool.clone(), &format!("/search?q={words}"), "gjuha=en").await;
            let page = match (response.status, &response.location) {
                (StatusCode::SEE_OTHER, Some(to)) => get_with_cookie(pool, to, "gjuha=en").await,
                _ => response,
            };
            card_numbers(&page.body)
        }
    };
    assert_eq!(found_in_english("flat+for+rent+in+tirana").await, [1003]);
    assert_eq!(found_in_english("apartments+to+rent").await, [1003]);
    assert!(found_in_english("flat+for+sale").await.is_empty());
    assert_eq!(found_in_english("house+for+sale+in+vlora").await, [1004]);
    assert_eq!(found_in_english("phones").await, [1005]);
    // The page it goes to has those filters on.
    let to = get_with_cookie(pool.clone(), "/search?q=flat+for+rent+in+tirana", "gjuha=en").await.location.unwrap();
    assert!(to.contains("category=apartamente") && to.contains("type=RENT") && to.contains("city=tirane"), "{to}");
    // An Albanian page reads Albanian's words alone: there "flat for rent" are words to search.
    assert!(found(&pool, "flat+for+rent").await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_forgives_typos_and_names_the_word_it_searched(pool: PgPool) {
    // No listing says "golff": the Golf is found, and the page says "golf" was searched too.
    let misspelt = get(pool.clone(), "/search?q=golff").await.body;
    assert_eq!(card_numbers(&misspelt), vec![1001]);
    assert!(misspelt.contains(r#"<p class="search-corrections">Shfaqen edhe rezultatet për „golf“.</p>"#));
    // A word listings use is searched as typed.
    let typed = get(pool, "/search?q=golf").await.body;
    assert!(typed.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!typed.contains("search-corrections"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_finds_other_forms_of_a_word(pool: PgPool) {
    // The flat's title says "Apartament" and the search the plural, "apartamente": the same word,
    // which needs no note. (In a category's page, as across every category it names Apartamente.)
    let body = get(pool, "/c/prona?q=apartamente").await.body;
    assert!(body.contains("Apartament 2+1 me qira në Tiranë"));
    assert!(!body.contains("search-corrections"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_word_left_out_stays_out_of_a_widened_search(pool: PgPool) {
    // The Golf runs on diesel (naftë): leaving that out leaves the Golf out, typo or not.
    let body = get(pool, "/search?q=golff+-naft%C3%AB").await.body;
    assert!(card_numbers(&body).is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_finds_listings_by_their_category_and_town(pool: PgPool) {
    // The laptop says neither "elektronikë" nor "Shkodër": the words name its category and its
    // town, which the page filters by, the town with its article too ("Shkodra").
    assert_eq!(card_numbers(&followed(pool.clone(), "/search?q=elektronike+shkoder").await.body), vec![1006]);
    assert_eq!(card_numbers(&followed(pool, "/search?q=laptop+shkodra").await.body), vec![1006]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_that_name_a_category_a_kind_and_a_town_put_those_filters_on(pool: PgPool) {
    // However typed, with its accents or without, the words in any of their forms and any order:
    // the flats for rent in Tirana, each filter a chip to take off if it was misread.
    let flats = "/search?category=apartamente&city=tirane&type=RENT";
    for words in ["Apartament+me+qera+ne+tirane", "apartamente+me+qira+n%C3%AB+Tiran%C3%AB", "qira+apartamenti+tirana"] {
        let sent = get(pool.clone(), &format!("/search?q={words}")).await;
        assert_eq!((sent.status, sent.location.as_deref()), (StatusCode::SEE_OTHER, Some(flats)), "{words}");
    }
    let page = html(pool.clone(), flats).await;
    assert_eq!(card_numbers(&page), [1003]);
    for chip in ["Apartamente", "Tiranë", "Me qira"] {
        assert!(page.contains(&format!(r#"aria-label="Hiq filtrin {chip}""#)), "{chip}");
    }

    let goes = |uri: &'static str| {
        let pool = pool.clone();
        async move { get(pool, uri).await.location }
    };
    // A town alone; and the words that name nothing stay searched there, the other filters kept.
    assert_eq!(goes("/search?q=tirane").await.as_deref(), Some("/search?city=tirane"));
    assert_eq!(
        goes("/search?q=golf+7+ne+durres&sort=price_asc").await.as_deref(),
        Some("/search?q=golf+7&city=durres&sort=price_asc")
    );
    // A short town only after the word that leads to it ("bojë mat" is a colour).
    assert_eq!(goes("/search?q=makina+ne+mat").await.as_deref(), Some("/search?category=makina&city=mat"));
    // A category's own page reads the town and the kind, the category being its own.
    assert_eq!(goes("/c/makina?q=me+qira+ne+tirane").await.as_deref(), Some("/c/makina?city=tirane&type=RENT"));
    // The thing sought comes first: headphones for a phone are headphones. A category's name is
    // about its first word: a car is in Makina, not in Tapete (mats, once "Tapete makine"); and the verb an ad
    // starts with comes before it.
    assert_eq!(goes("/search?q=kufje+per+telefon").await.as_deref(), Some("/search?q=per+telefon&category=audio-dhe-kufje"));
    assert_eq!(goes("/search?q=makine+me+nafte").await.as_deref(), Some("/search?q=me+nafte&category=makina"));
    assert_eq!(goes("/search?q=kerkoj+apartament+me+qira").await.as_deref(), Some("/search?category=apartamente&type=RENT"));
    // "shtëpi" is a house and the home alike, until the kind says which.
    assert_eq!(goes("/search?q=shtepi+me+qira").await.as_deref(), Some("/search?category=vila-dhe-shtepi&type=RENT"));
    // A category whose name became one word on 2026-10-11 is named by its old name's words too,
    // and by its new one's: "goma" is Rrota, once "Goma / disqe"; "kufje" Audio (above).
    assert_eq!(goes("/search?q=goma").await.as_deref(), Some("/search?category=goma-dhe-disqe"));
    assert_eq!(goes("/search?q=rrota").await.as_deref(), Some("/search?category=goma-dhe-disqe"));
    // Words that name nothing, a kind with no category to take it, a category two names share, or
    // two towns, are searched as they are.
    for uri in ["/search?q=golf", "/search?q=golf+ne+shitje", "/search?q=karikues+telefoni", "/search?q=shtepi",
        "/search?q=tirane+durres", "/search?q=boj%C3%AB+mat"]
    {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::OK, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_zone_named_in_a_property_search_puts_it_on(pool: PgPool) {
    in_zone(&pool, 1003, "durres", "plazh").await;
    let goes = |uri: &'static str| {
        let pool = pool.clone();
        async move { get(pool, uri).await.location }
    };
    // The flats in Plazh, which is in Durrës, "2+1" searched there.
    let plazh = "/search?q=2%2B1&category=apartamente&city=durres&zone=plazh";
    assert_eq!(goes("/search?q=apartament+2%2B1+ne+plazh").await.as_deref(), Some(plazh));
    let page = html(pool.clone(), plazh).await;
    assert_eq!(card_numbers(&page), [1003]);
    for chip in ["Apartamente", "Durrës", "Plazh"] {
        assert!(page.contains(&format!(r#"aria-label="Hiq filtrin {chip}""#)), "{chip}");
    }
    // Without the category, which its chip and "Pastro filtrat" take off, a zone is nothing: it goes.
    assert!(page.contains(r#"<a class="filter-chip" href="/search?q=2%2B1&city=durres" aria-label="Hiq filtrin Apartamente">"#));
    assert!(page.contains(r#"<a class="clear-filters" href="/search?q=2%2B1&city=durres">"#));
    // In its forms and any order; a zone of a few words, on a category's page.
    assert_eq!(goes("/search?q=plazhi+apartament").await.as_deref(), Some("/search?category=apartamente&city=durres&zone=plazh"));
    assert_eq!(goes("/c/prona?q=komuna+e+parisit").await.as_deref(), Some("/c/prona?city=tirane&zone=komuna-e-parisit"));
    // A zone named after a town is not that town: Rruga e Durrësit is in Tirana.
    assert_eq!(
        goes("/c/apartamente?q=rruga+e+durresit").await.as_deref(),
        Some("/c/apartamente?city=tirane&zone=rruga-e-durresit")
    );
    // Qendër is Tirana's, Durrës's and Vlora's: the one in the town named, or else in the page's.
    assert_eq!(goes("/c/apartamente?q=ne+qender+te+tiranes").await.as_deref(), Some("/c/apartamente?city=tirane&zone=qender"));
    assert_eq!(goes("/c/apartamente?city=vlore&q=qendra").await.as_deref(), Some("/c/apartamente?city=vlore&zone=qender"));
    // A zone in another town than the one named stays a word, the town kept.
    assert_eq!(goes("/c/apartamente?q=plazh+ne+vlore").await.as_deref(), Some("/c/apartamente?q=plazh&city=vlore"));
    // Outside property, or with no town to say which Qendër, the name is a word: a beach towel.
    for uri in ["/search?q=peshqir+plazhi", "/c/makina?q=golf+blloku", "/c/apartamente?q=qender"] {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::OK, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filler_words_needn_t_be_in_a_listing(pool: PgPool) {
    // The Mercedes is in Durrës, but nothing it says has "në" (which goes with the town, the
    // page's filter); nor does the Golf, for sale, have "në" or "shitje".
    assert_eq!(card_numbers(&followed(pool.clone(), "/search?q=mercedes+n%C3%AB+durr%C3%ABs").await.body), vec![1002]);
    assert_eq!(card_numbers(&get(pool, "/search?q=golf+n%C3%AB+shitje").await.body), vec![1001]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_across_categories_names_the_categories_it_found(pool: PgPool) {
    // Each chip narrows the search to its category, where that category's filters are.
    // After "Gjetur te", as the canvas has them, each with its category's icon.
    let found = html(pool.clone(), "/search?q=golf").await;
    assert!(found.contains(r#"<span class="categories-lead" aria-hidden="true">Gjetur te</span>"#));
    assert!(found.contains(r#"<a class="chip" href="/c/makina?q=golf"><svg class="chip-icon""#));
    assert!(found.contains(r#"</svg>Makina <span class="chip-count">1</span></a>"#));
    // A kind of listing, with no category to take it, stays words: the listings for sale are in
    // nine categories, of which the six with the most, the two cars' first.
    let selling = html(pool.clone(), "/search?q=shitje").await;
    assert!(selling.contains(r#"<a class="chip" href="/c/makina?q=shitje"><svg class="chip-icon""#), "{selling}");
    assert!(selling.contains(r#"</svg>Makina <span class="chip-count">2</span></a>"#), "{selling}");
    assert_eq!(selling.matches(r#"<span class="chip-count">"#).count(), 6);
    // A category's page has its subcategories instead.
    assert!(!html(pool, "/c/makina?q=golf").await.contains("chip-count"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn suggestions_finish_the_words_and_say_where_they_lead(pool: PgPool) {
    // The word finished as the listings have it, then the category it is found in, then the make
    // and model it names.
    let rows = html(pool.clone(), "/sugjerime?q=gol").await;
    let at = positions(&rows, &[
        r#"href="/search?q=golf""#,
        r#"href="/c/makina?q=golf""#,
        r#"href="/c/makina?vehicle_make=VOLKSWAGEN&vehicle_model=VOLKSWAGEN_GOLF""#,
    ]);
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{rows}");
    assert!(rows.contains(r#"<span class="suggestion-label">Volkswagen <strong>Gol</strong>f</span>"#), "{rows}");
    // A town the last word starts: on its own, all its listings; after other words, those there.
    let town = html(pool.clone(), "/sugjerime?q=durr").await;
    assert!(town.contains(r#"href="/search?city=durres""#) && town.contains("Njoftimet në <strong>Durr</strong>ës"), "{town}");
    let there = html(pool.clone(), "/sugjerime?q=golf%20durr").await;
    assert!(there.contains(r#"href="/search?q=golf&city=durres""#) && there.contains("në Durrës"), "{there}");
    // The word that joins the words to the town, "në" with its ë or without, is the town's to say:
    // the row searches the words alone, there; with no other words, it is all the town's listings.
    for typed in ["golf%20ne%20durr", "golf%20n%C3%AB%20durr", "golf%20te%20durr"] {
        let there = html(pool.clone(), &format!("/sugjerime?q={typed}")).await;
        assert!(there.contains(r#"href="/search?q=golf&city=durres""#), "{typed}: {there}");
        assert!(there.contains(r#"<span class="suggestion-label"><strong>golf</strong></span>"#), "{typed}: {there}");
    }
    let town_alone = html(pool.clone(), "/sugjerime?q=ne%20durr").await;
    assert!(town_alone.contains(r#"href="/search?city=durres""#) && town_alone.contains("Njoftimet <strong>në</strong> <strong>Durr</strong>ës"), "{town_alone}");
    // Words, then "në" (or "ne", or with a space after): the towns they are found in, a car in
    // Tirana and one in Durrës, rather than words starting "ne".
    for typed in ["makina%20ne", "makina%20n%C3%AB%20"] {
        let places = html(pool.clone(), &format!("/sugjerime?q={typed}")).await;
        for town in ["tirane", "durres"] {
            assert!(places.contains(&format!(r#"href="/search?q=makina&city={town}""#)), "{typed}: {places}");
        }
        assert!(!places.contains("q=makina+n"), "{typed}: {places}");
    }
    // A whole word misspelt: the word it most likely misspells.
    assert!(html(pool.clone(), "/sugjerime?q=mercedez").await.contains(r#"href="/search?q=mercedes""#));
    // A brand of the very name the words have: its row, which finds it alone, not the words
    // searched in its category too.
    let brand = html(pool.clone(), "/sugjerime?q=apple").await;
    assert!(brand.contains(r#"href="/c/telefona?electronics_brand=APPLE""#), "{brand}");
    assert!(!brand.contains(r#"href="/c/telefona?q=apple""#), "{brand}");
    // Too little to go on.
    assert!(!get(pool, "/sugjerime?q=g").await.body.contains("suggestion"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_find_the_best_matches_first(pool: PgPool) {
    // 260 in Të tjera that say "kaltërosh", a minute apart, the first the newest: every fifth in its
    // title, every third twice in its description, the others once. The newest 240 are ranked
    // (db::RANK_WINDOW); the 20 past them follow, newest first.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g,
                CASE WHEN g % 5 = 0 THEN 'Kaltërosh ' || g ELSE 'Njoftim shtesë ' || g END,
                CASE WHEN g % 3 = 0 THEN 'Një kaltërosh, dhe një kaltërosh tjetër.' ELSE 'Një kaltërosh.' END,
                1000, 'ALL', 'FIXED', 10, 'ACTIVE', now() - g * interval '1 minute'
         FROM generate_series(1, 260) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    pluribus_db::refresh_search_words(&pool).await.unwrap();
    assert_eq!(pluribus_db::RANK_WINDOW, 240);
    let in_order = |order: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT public_no FROM (
                     SELECT l.public_no, l.sort_at, l.id, ts_rank(l.search_vector, s.query) AS rank,
                            row_number() OVER (ORDER BY l.sort_at DESC, l.id DESC) AS newest
                     FROM listings l, search_query('kaltërosh') s
                     WHERE l.status = 'ACTIVE' AND l.search_vector @@ s.query) m
                 ORDER BY {order}"
            ))
            .fetch_all(&pool)
            .await
            .unwrap()
        }
    };
    let best = in_order("newest > 240, CASE WHEN newest <= 240 THEN rank END DESC, sort_at DESC, id DESC").await;
    let newest = in_order("sort_at DESC, id DESC").await;
    assert_eq!(best.len(), 260);
    // Among the ranked, those with the word in their title come first, those that say it twice
    // besides first of them; past the window, the rest newest first.
    let titled: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE title LIKE 'Kaltërosh %' AND split_part(title, ' ', 2)::int <= 240
         ORDER BY public_no",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(titled.len(), 48);
    let mut first = best[..48].to_vec();
    first.sort();
    assert_eq!(first, titled);
    let twice: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE title LIKE 'Kaltërosh %' AND description LIKE '%tjetër%'
           AND split_part(title, ' ', 2)::int <= 240 ORDER BY sort_at DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(best[..16], twice[..]);
    assert_eq!(best[240..], newest[240..]);
    assert_ne!(best, newest);

    // Words open the page in the best match first, which its order's menu names first, the newest
    // first after it; every page, as scrolling loads them, follows on in that order.
    let search = "/search?q=kalt%C3%ABrosh";
    let page = html(pool.clone(), search).await;
    let choices = positions(&page, &["Më të përshtatshmet", "Më të rejat", "Më të lirat"]);
    assert!(choices.is_sorted(), "{page}");
    assert_eq!(every_page(&pool, search).await, best);
    // Newest first when asked, every page too.
    assert_eq!(every_page(&pool, &format!("{search}&sort=newest")).await, newest);
    // Without words there is nothing to match: no such order on offer, nor taken from the address.
    let plain = html(pool.clone(), "/c/te-tjera?sort=relevance").await;
    assert!(!plain.contains("Më të përshtatshmet"), "{plain}");
    let shown = card_numbers(&plain);
    let newest_here: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
         ORDER BY sort_at DESC, id DESC LIMIT 24",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(shown, newest_here);
}
