//! Moderators, and what readers report (web/moderation.rs, safety.rs): the makes and models sellers
//! typed, listings waiting for review, promotions, suspended accounts; a reported listing, which
//! comes off the site until a moderator decides; and a blocked seller's listings.

mod common;

use axum::http::StatusCode;
use common::*;
use sqlx::PgPool;

/// Whether the option named so is offered, approved.
async fn option_active(pool: &PgPool, label: &str) -> Option<bool> {
    sqlx::query_scalar("SELECT active FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_approve_or_merge_what_sellers_typed(pool: PgPool) {
    let started = started();
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let moderator = new_seller(&pool, MODERATOR).await;
    // Typed in: a make the list lacks, with its model; a listed make spelt short, with its model;
    // and a model of a listed make, spelt long.
    let post = |seller: String, make: &'static str, model: &'static str, title: &'static str| {
        let pool = pool.clone();
        let started = started.clone();
        async move {
            let mut fields = typed_car(make, model, &started);
            fields.iter_mut().find(|field| field.0 == "title").unwrap().1 = title;
            let posted = post_form(pool, "/shto", &fields, Some(&seller)).await;
            assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
            posted.location.unwrap()
        }
    };
    let yugo = post(ana.clone(), "Zastava", "Yugo", "Zastava Yugo 45, 1988").await;
    let mercedes = post(besa.clone(), "Mercedes", "C 220", "Mercedes C 220, 2012").await;
    let golf = post(ana.clone(), "volkswagen", "Golf IV", "VW Golf IV 1.9 TDI").await;
    let zastava_id = option_id(&pool, "Zastava").await;
    let yugo_id = option_id(&pool, "Yugo").await;
    let golf_iv_id = option_id(&pool, "Golf IV").await;
    let mercedes_id = option_id(&pool, "Mercedes").await;

    // Only for moderators: anyone else finds nothing, and changes nothing.
    assert_eq!(get(pool.clone(), "/moderim").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), "/moderim", &ana).await.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", "Zastava")], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &option_id(&pool, "Golf").await)], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    assert_eq!(option_active(&pool, "Zastava").await, Some(false));
    assert_eq!(option_active(&pool, "Golf IV").await, Some(false));
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains("/moderim"));

    // The moderator's own page links to it, with how many wait.
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains(r#"<a class="button secondary small" href="/moderim">Për miratim (5)</a>"#));
    // Each, with its make, how many listings have it, who typed it; the likeliest listed one
    // comes chosen for a merge; a model whose make waits waits for it.
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await;
    assert_eq!(page.status, StatusCode::OK);
    let page = page.body;
    assert!(page.contains(r#"<strong class="pending-label">Zastava</strong>"#));
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Zastava</span>"#));
    assert!(page.contains("Mirato ose bashko më parë markën, „Zastava“."));
    assert!(page.contains(r#"<a href="/search?q=Golf+IV">1 njoftim</a> · shtuar nga Përdorues"#));
    let golf_id = option_id(&pool, "Golf").await;
    assert!(page.contains(&format!(r#"<option value="{golf_id}" selected>Golf</option>"#)));
    let mercedes_benz_id = option_id(&pool, "Mercedes-Benz").await;
    assert!(page.contains(&format!(r#"<option value="{mercedes_benz_id}" selected>Mercedes-Benz</option>"#)));
    // The model of a make that waits can't be approved yet.
    let early = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Yugo")], Some(&moderator)).await;
    assert_eq!(early.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Yugo").await, Some(false));

    // Approved, the make is offered, in its alphabetical place: after Volvo.
    let approved = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", " Zastava ")], Some(&moderator)).await;
    assert_eq!((approved.status, approved.location.as_deref()), (StatusCode::SEE_OTHER, Some("/moderim")));
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    let places = positions(&frame, &[">Volkswagen<", ">Volvo<", ">Zastava<"]);
    assert!(places.is_sorted(), "{places:?}");
    // Its model, with its name put right: "Koral", which search finds it by now too.
    let approved = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Koral")], Some(&moderator)).await;
    assert_eq!(approved.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Koral").await, Some(true));
    assert!(get(pool.clone(), &yugo).await.body.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Zastava Koral, 2017</span> · "#));
    assert!(get(pool.clone(), "/search?q=koral").await.body.contains(&yugo));
    assert!(get(pool.clone(), "/c/makina").await.body.contains(">Zastava<"));

    // A name the list has already, or one that can't be a name, is not approved.
    for (label, message) in [("golf", "„golf“ është tashmë në listë: bashkoje me të."), ("x", "Shkruaj vetëm emrin, me 2 deri në 40 shkronja.")] {
        let refused = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/mirato"), &[("label", label)], Some(&moderator)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{label}");
        assert!(refused.body.contains(message), "{label}");
        assert_eq!(option_active(&pool, "Golf IV").await, Some(false), "{label}");
    }

    // Merged, the listing has the listed model, and the typed one is gone.
    let merged = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Golf IV").await, None);
    let page = get(pool.clone(), &golf).await.body;
    assert!(page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf, 2017</span> · "#) && !page.contains(">Golf IV<"));
    // A typed make merged into the listed one takes its typed model along, under the listed make.
    let merged = post_form(pool.clone(), &format!("/moderim/{mercedes_id}/bashko"), &[("into", &mercedes_benz_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), &mercedes).await.body.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Mercedes-Benz C 220, 2017</span> · "#));
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Mercedes-Benz</span>"#));
    assert!(page.contains(r#"<strong class="pending-label">C 220</strong>"#));
    assert!(!page.contains("më parë markën"));
    // No listed model is like it, so the merge asks which, and won't go without one.
    assert!(page.contains(r#"<option value="" selected>Bashkoje me…</option>"#));
    let c220_id = option_id(&pool, "C 220").await;
    let unchosen = post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", "")], Some(&moderator)).await;
    assert_eq!(unchosen.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(unchosen.body.contains("Zgjidh me cilin emër të listës ta bashkosh."));
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    // Not into a model of another make.
    post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains("Për miratim (1)"));
}

/// A moderator gives listing `number` the promotion `product` (/moderim).
async fn promote(pool: &PgPool, moderator: &str, number: &str, product: &str) -> TestResponse {
    post_form(pool.clone(), "/moderim/promovime", &[("njoftimi", number), ("produkti", product)], Some(moderator)).await
}

/// The listings a promotion features at the top of a page's results, by their numbers, in their
/// order (browse.html, ListingPage::featured).
fn featured_numbers(html: &str) -> Vec<i64> {
    html.match_indices(r#"" id="featured-"#)
        .map(|(start, matched)| {
            let rest = &html[start + matched.len()..];
            rest[..rest.find('"').unwrap()].parse().unwrap()
        })
        .collect()
}

/// The listings in a page's results, by their numbers, past the rows over them and the promoted
/// ones at their top.
fn result_numbers(html: &str) -> Vec<i64> {
    let results = &html[html.find(r#"<section class="results""#).unwrap()..];
    let mut numbers = card_numbers(results);
    numbers.drain(..featured_numbers(results).len());
    numbers
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_promote_listings_which_show_more_until_their_time(pool: PgPool) {
    let moderator = new_seller(&pool, MODERATOR).await;
    let ana = session_for(&pool, ANA_ID).await;
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let updated = |pool: PgPool| async move {
        sqlx::query_scalar::<_, time::OffsetDateTime>("SELECT updated_at FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap()
    };
    let golf_updated = updated(pool.clone()).await;

    // Only a moderator gives one; their page offers those on sale, the number from a listing's page
    // already in its form.
    assert_eq!(promote(&pool, &ana, "1001", "FEATURED_7D").await.status, StatusCode::NOT_FOUND);
    let page = get_with_cookie(pool.clone(), "/moderim?njoftimi=1001", &moderator).await.body;
    assert!(page.contains(r#"<h2 class="moderation-heading" id="promovime">Promovime</h2>"#), "{page}");
    assert!(page.contains(r#"<input id="promotion-number" name="njoftimi" value="1001""#), "{page}");
    for option in [
        r#"<option value="BUMP_ONCE">Ngritje në krye</option>"#,
        r#"<option value="FEATURED_7D">I veçuar për 7 ditë</option>"#,
        r#"<option value="HIGHLIGHT_7D">I theksuar për 7 ditë</option>"#,
        r#"<option value="CATEGORY_TOP_7D">Në krye të kategorisë për 7 ditë</option>"#,
    ] {
        assert!(page.contains(option), "{option}");
    }
    assert!(page.contains("Asnjë promovim tani."));
    // The seller's page leads to one, a moderator's to giving one.
    assert!(get_with_cookie(pool.clone(), golf, &ana).await.body.contains(r#"<p class="promotion-note"><a href="/promovo?njoftimi=1001">Promovoje</a> që ta shohin më shumë.</p>"#));
    assert!(get_with_cookie(pool.clone(), golf, &moderator).await.body.contains(r#"<a href="/moderim?njoftimi=1001#promovime">Jep një promovim</a>"#));
    assert!(!get(pool.clone(), golf).await.body.contains("promotion-note"));
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(featured_numbers(&cars).is_empty() && !cars.contains("is-promoted") && !cars.contains("I promovuar"));

    // Featured: first of the results of its category and of the searches it matches, in their own
    // look, its price marked in the accent as the listing page's is (screen readers hearing "I
    // promovuar" before it), and not again among them (the user's asks of 2026-10-11: a row of
    // its own over the results until then, "Të veçuara", and a pill saying "I promovuar" over the
    // photo after it); nowhere else; on /moderim with until when; its seller hears of it and sees
    // it.
    let given = promote(&pool, &moderator, "1001", "FEATURED_7D").await;
    assert_eq!(given.location.as_deref(), Some("/moderim#promovime"));
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert_eq!(featured_numbers(&cars), vec![1001]);
    let open = r#"<article class="card is-promoted" id="featured-1001">"#;
    let at = positions(&cars, &[r#"<section class="results""#, r#"<h2 class="results-heading" id="results-heading">"#, r#"<div class="grid">"#, open, r#"<article class="card" id="listing-1002">"#]);
    assert!(at.is_sorted(), "{at:?}");
    let promoted = &cars[cars.find(open).unwrap()..];
    let promoted = &promoted[..promoted.find("</article>").unwrap()];
    assert!(promoted.contains(r#"<p class="card-price"><span class="visually-hidden">I promovuar: </span><span class="card-amount"><span class="card-figure">9"#), "{promoted}");
    assert!(!cars.contains("card-promoted"));
    assert!(!cars.contains("featured-row") && !cars.contains("Të veçuara") && !cars.contains("row-note"));
    assert_eq!(result_numbers(&cars), vec![1002]);
    assert_eq!(featured_numbers(&get(pool.clone(), "/search?q=golf").await.body), vec![1001]);
    assert!(featured_numbers(&get(pool.clone(), "/search?q=mercedes").await.body).is_empty());
    assert!(featured_numbers(&get(pool.clone(), "/c/elektronike").await.body).is_empty());
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(r#"<span class="pending-attribute">I veçuar për 7 ditë</span>"#) && page.contains(&format!(r#"<a class="pending-label" href="{golf}">"#)), "{page}");
    assert!(page.contains("Nr. 1001 · deri më "));
    let mine = get_with_cookie(pool.clone(), golf, &ana).await.body;
    assert!(mine.contains(r#"<p class="promotion-note">I veçuar për 7 ditë, deri më "#), "{mine}");
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<span class="badge promoted">I veçuar për 7 ditë, deri më "#));
    assert!(get_with_cookie(pool.clone(), "/lajmerimet", &ana).await.body.contains("I veçuar për 7 ditë: Volkswagen Golf 7, 2016, naftë"));
    // Given again, its time adds up.
    promote(&pool, &moderator, "1001", "FEATURED_7D").await;
    let days: f64 = sqlx::query_scalar("SELECT extract(epoch FROM featured_until - now())::float8 / 86400 FROM listings WHERE public_no = 1001")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!((13.99..=14.0).contains(&days), "{days}");

    // Ended by a moderator, it gives no more; the other of its kind on it still does, until it is
    // ended too.
    let featured: Vec<String> = sqlx::query_scalar(
        "SELECT p.id::text FROM listing_promotions p JOIN promotion_products pr ON pr.id = p.promotion_product_id
         WHERE pr.code = 'FEATURED_7D' AND p.status = 'ACTIVE' ORDER BY p.ends_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(featured.len(), 2);
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(&format!(r#"<form method="post" action="/moderim/promovime/{}/ndalo""#, featured[1])));
    let ended = post_form(pool.clone(), &format!("/moderim/promovime/{}/ndalo", featured[1]), &[], Some(&moderator)).await;
    assert_eq!(ended.location.as_deref(), Some("/moderim#promovime"));
    assert_eq!(featured_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001]);
    post_form(pool.clone(), &format!("/moderim/promovime/{}/ndalo", featured[0]), &[], Some(&moderator)).await;
    assert!(featured_numbers(&get(pool.clone(), "/c/makina").await.body).is_empty());
    let left: Option<time::OffsetDateTime> = sqlx::query_scalar("SELECT featured_until FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap();
    assert_eq!(left, None);

    // Highlighted: its card stands out wherever it shows.
    promote(&pool, &moderator, "1002", "HIGHLIGHT_7D").await;
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(cars.contains(r#"<article class="card is-highlighted" id="listing-1002">"#), "{cars}");
    assert!(cars.contains(r#"<article class="card" id="listing-1001">"#));

    // A bump takes it to the top of "newest first" at once, spent there and then.
    promote(&pool, &moderator, "1002", "BUMP_ONCE").await;
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1002, 1001]);
    // At the top of its category for a week: there now, and back once a day has gone by.
    promote(&pool, &moderator, "1001", "CATEGORY_TOP_7D").await;
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001, 1002]);
    // Giving and ending them moved no listing's updated_at, which the sitemaps give search
    // engines.
    assert_eq!(updated(pool.clone()).await, golf_updated);
    sqlx::query("UPDATE listings SET published_at = now() - interval '3 days', bumped_at = now() - interval '25 hours' WHERE public_no = 1001").execute(&pool).await.unwrap();
    let golf_updated = updated(pool.clone()).await;
    sqlx::query("UPDATE listings SET bumped_at = now() - interval '1 hour' WHERE public_no = 1002").execute(&pool).await.unwrap();
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1002, 1001]);
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (0, 1));
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001, 1002]);
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (0, 0));

    // Its time up, it ends.
    sqlx::query(
        "UPDATE listing_promotions p SET starts_at = now() - interval '8 days', ends_at = now() - interval '1 minute'
         FROM promotion_products pr WHERE pr.id = p.promotion_product_id AND pr.code = 'HIGHLIGHT_7D'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE listings SET highlighted_until = now() - interval '1 minute' WHERE public_no = 1002").execute(&pool).await.unwrap();
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("is-highlighted"));
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (1, 0));

    // A number no listing up has, or no promotion chosen, is said beside the form.
    let refused = promote(&pool, &moderator, "999999", "FEATURED_7D").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Asnjë njoftim në faqe nuk ka numrin „999999“.") && refused.body.contains(r#"value="999999" inputmode="numeric" required aria-invalid="true""#), "{}", refused.body);
    let refused = promote(&pool, &moderator, "1001", "GJITHCKA").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh një nga promovimet."));

    // The moderators' record keeps each, and neither the ends nor the day's top moved updated_at.
    let actions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT action_type, count(*) FROM moderation_actions WHERE action_type LIKE '%PROMOTION' GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, vec![("END_PROMOTION".to_string(), 2), ("GRANT_PROMOTION".to_string(), 5)]);
    assert_eq!(updated(pool.clone()).await, golf_updated);

    // /promovo says what each does and costs, and how to get one.
    let about = get(pool.clone(), "/promovo?njoftimi=1001").await.body;
    assert!(about.contains("<strong>I veçuar për 7 ditë</strong>") && about.contains("8,99\u{a0}€"), "{about}");
    assert!(about.contains("Që ta shohin më shumë: mbi njoftimet e tjera me çmimin e shënuar me ngjyrë, ") && !about.contains("Të veçuara"), "{about}");
    assert!(about.contains("<span>Del mbi njoftimet e kategorisë së tij dhe të kërkimeve ku përputhet, me çmimin e shënuar me ngjyrë, me radhë me të tjerët.</span>"), "{about}");
    assert!(about.contains(r#"<a href="/kontakt">Kontakt</a> me numrin e njoftimit (<strong>1001</strong>)"#), "{about}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_review_listings_with_anothers_photos_and_suspend_accounts(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let moderator = new_seller(&pool, MODERATOR).await;
    let started = started();
    let status = |title: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE title = $1").bind(title).fetch_one(&pool).await.unwrap() }
    };
    let post = |cookie: String, title: &'static str, photo: Vec<u8>| {
        let pool = pool.clone();
        let started = started.clone();
        async move {
            let fields = listing_fields(("title", title), &started);
            let posted = post_with_photos(pool, "/shto", &fields, &[photo], &cookie).await;
            posted.location.unwrap_or_else(|| panic!("{title}: {}", posted.body))
        }
    };

    // Ana's photo, then the very same file on Besa's listing: hers waits for a moderator, which
    // she sees on its page and "Njoftimet e mia"; nobody else sees it. Ana posting it again is
    // nothing new.
    let photo = jpeg(64, 48);
    let first = post(ana.clone(), "Samsung Galaxy S21, i pari", photo.clone()).await;
    let copied = post(besa.clone(), "Samsung Galaxy S21, i kopjuari", photo.clone()).await;
    post(ana.clone(), "Samsung Galaxy S21, përsëri", photo.clone()).await;
    assert_eq!(status("Samsung Galaxy S21, i pari").await, "ACTIVE");
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "PENDING_REVIEW");
    assert_eq!(status("Samsung Galaxy S21, përsëri").await, "ACTIVE");
    assert!(get_with_cookie(pool.clone(), &copied, &besa).await.body.contains("Njoftimi yt pret miratimin e një moderatori"));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body.contains("Në pritje"));
    assert_eq!(get(pool.clone(), &copied).await.status, StatusCode::NOT_FOUND);
    // Besa's other listing, with a photo of her own, goes up.
    let laptop = post(besa.clone(), "Laptop i Besës", jpeg(70, 50)).await;
    assert_eq!(status("Laptop i Besës").await, "ACTIVE");

    // The moderators see why it waits, and the listing it took the photo from; refused, Besa
    // reads why. Nobody else may.
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    let first_no = first.trim_start_matches("/listing/").split('-').next().unwrap();
    assert!(queue.contains("Foto të njëjta me njoftimin e një shitësi tjetër") && queue.contains(&format!(">nr. {first_no}</a>")), "{queue}");
    let no = copied.trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    assert_eq!(post_form(pool.clone(), &format!("/moderim/njoftim/{no}/refuzo"), &[("arsye", "foto")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    post_form(pool.clone(), &format!("/moderim/njoftim/{no}/refuzo"), &[("arsye", "foto")], Some(&moderator)).await;
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "REJECTED");
    let told = get_with_cookie(pool.clone(), &copied, &besa).await.body;
    assert!(told.contains("Moderatorët e hoqën këtë njoftim nga faqja: Fotot janë marrë nga një njoftim tjetër."), "{told}");

    // Suspended, Besa is signed out and her listing leaves the site; the moderators have her
    // with "Rikthe", which brings it back, and leaves the refused one refused.
    let besa_id = account_of(&besa).to_string();
    assert_eq!(post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/pezullo"), &[("arsye", "mashtrim")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/pezullo"), &[("arsye", "mashtrim")], Some(&moderator)).await;
    assert_eq!(status("Laptop i Besës").await, "SUSPENDED");
    // Its page says why to the moderators, the account's suspension rather than reports.
    let seen = get_with_cookie(pool.clone(), &laptop, &moderator).await.body;
    assert!(seen.contains("Ky njoftim nuk shfaqet në faqe: llogaria e shitësit është pezulluar."), "{seen}");
    assert!(!seen.contains("disa njerëz e raportuan"));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body.contains(r#"action="/hyr""#));
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(queue.contains("Llogari të pezulluara") && queue.contains("Mashtrim · ") && queue.contains(&format!("/moderim/perdorues/{besa_id}/rikthe")), "{queue}");
    post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/rikthe"), &[], Some(&moderator)).await;
    assert_eq!(status("Laptop i Besës").await, "ACTIVE");
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "REJECTED");
    let account: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1::uuid").bind(&besa_id).fetch_one(&pool).await.unwrap();
    assert_eq!(account, "ACTIVE");
    // A moderator doesn't suspend themself.
    let own = account_of(&moderator).to_string();
    post_form(pool.clone(), &format!("/moderim/perdorues/{own}/pezullo"), &[("arsye", "spam")], Some(&moderator)).await;
    let theirs: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1::uuid").bind(&own).fetch_one(&pool).await.unwrap();
    assert_eq!(theirs, "ACTIVE");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_reported_listing_comes_off_the_site_until_a_moderator_decides(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Anyone may report a listing from its page. Someone not signed in is asked to sign in first,
    // and comes back to the report after; its own seller can't.
    assert!(html(pool.clone(), golf).await.contains(r#"href="/listing/1001/raporto""#));
    let anonymous = get(pool.clone(), "/listing/1001/raporto").await;
    assert!(anonymous.body.contains("Hyr në llogari që ta raportosh."), "{}", anonymous.body);
    assert_eq!(anonymous.cookie("kthehu"), Some("/listing/1001/raporto"));
    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &ana).await.status, StatusCode::NOT_FOUND);

    let mut reporters = Vec::new();
    for phone in ["+355691110001", "+355691110002", "+355691110003", "+355691110004"] {
        reporters.push(new_seller(&pool, phone).await);
    }
    let report = |session: &str, reason: &'static str, details: &'static str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move { post_form(pool, "/listing/1001/raporto", &[("reason", reason), ("details", details)], Some(&session)).await }
    };
    // The form offers its reasons, and needs one; "Tjetër" needs what is wrong.
    let form = get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[0]).await.body;
    assert_eq!(form.matches(r#"<input type="radio" name="reason""#).count(), 7);
    // The listing at the page's top, as the canvas has it: a small card, its photo beside its
    // title and its price and town under it, all one link to it.
    let card = &form[form.find(r#"<a class="report-listing" href="/listing/1001-volkswagen-golf-7-2016-nafte">"#).expect("the listing's card")..];
    let card = &card[..card.find("</a>").unwrap()];
    assert!(card.contains(r#"<img src="https://images.unsplash.com/"#), "{card}");
    assert!(card.contains("<span class=\"report-listing-meta\">9\u{a0}500\u{a0}€ · <span translate=\"no\">Tiranë</span></span>"), "{card}");
    let none = report(&reporters[0], "", "").await;
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(none.body.contains("Zgjidh çfarë nuk shkon me njoftimin."));
    assert!(report(&reporters[0], "OTHER", "  ").await.body.contains("Shkruaj shkurt çfarë nuk shkon me njoftimin."));

    // Reported for the wrong details, then for a scam and as fake: it stays up, as wrong details
    // are for a moderator to see to, not reason to hide it. Thanked; a second report is the first.
    let sent = report(&reporters[0], "WRONG_DETAILS", "Është në kategorinë e gabuar.").await;
    assert_eq!(sent.location.as_deref(), Some("/listing/1001/raporto"));
    assert!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[0]).await.body.contains("Faleminderit!"));
    report(&reporters[0], "SCAM", "").await;
    report(&reporters[1], "SCAM", "Kërkon para përpara, pa e parë makinën.").await;
    report(&reporters[2], "FAKE", "").await;
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM reports").fetch_one(&pool).await.unwrap();
    assert_eq!(reports, 3);
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::OK);

    // A third for a serious reason takes it off the site: gone for others, the cards with it, and
    // the report page but for whoever reported it.
    report(&reporters[3], "SCAM", "").await;
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::NOT_FOUND);
    assert!(!card_numbers(&html(pool.clone(), "/c/makina").await).contains(&1001));
    assert!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[3]).await.body.contains("Faleminderit!"));
    // Its seller still sees it, told why, and so do the moderators.
    let own = get_with_cookie(pool.clone(), golf, &ana).await.body;
    assert!(own.contains("Ky njoftim nuk shfaqet në faqe: disa njerëz e raportuan, dhe moderatorët do ta shqyrtojnë."));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<span class="badge danger">Në shqyrtim</span>"#));
    let moderator = new_seller(&pool, MODERATOR).await;
    assert_eq!(get_with_cookie(pool.clone(), golf, &moderator).await.status, StatusCode::OK);
    // On /moderim, the hidden first: who reported it and why, and what they wrote.
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body.contains("Për miratim (1)"));
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    for text in ["4 veta e raportuan", r#"<span class="badge danger">Fshehur</span>"#, "<li>Mashtrim (2)</li>", "<li>Njoftim i rremë</li>",
                 "<li>Të dhëna të gabuara</li>", "Kërkon para përpara, pa e parë makinën."] {
        assert!(queue.contains(text), "{text}: {queue}");
    }

    // Kept: back on the site, its reports dismissed, both steps on record.
    let kept = post_form(pool.clone(), "/moderim/njoftim/1001/mbaje", &[], Some(&moderator)).await;
    assert_eq!(kept.location.as_deref(), Some("/moderim"));
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::OK);
    assert!(get_with_cookie(pool.clone(), "/moderim", &moderator).await.body.contains("Nuk ka njoftime të raportuara."));
    let actions: Vec<(String, bool)> = sqlx::query_as(
        "SELECT action_type, moderator_user_id IS NOT NULL FROM moderation_actions ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, [("SUSPEND_LISTING".to_string(), false), ("RESTORE_LISTING".to_string(), true)]);
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE status = 'OPEN'").fetch_one(&pool).await.unwrap();
    assert_eq!(open, 0);

    // Another taken down for good: off the site, and its seller told so; deleting it is theirs.
    let iphone = "/listing/1005-iphone-13-128-gb-si-i-ri";
    post_form(pool.clone(), "/listing/1005/raporto", &[("reason", "PROHIBITED"), ("details", "")], Some(&reporters[0])).await;
    assert_eq!(get(pool.clone(), iphone).await.status, StatusCode::OK);
    let removed = post_form(pool.clone(), "/moderim/njoftim/1005/hiq", &[], Some(&moderator)).await;
    assert_eq!(removed.location.as_deref(), Some("/moderim"));
    assert_eq!(get(pool.clone(), iphone).await.status, StatusCode::NOT_FOUND);
    assert!(get_with_cookie(pool.clone(), iphone, &ana).await.body.contains("Moderatorët e hoqën këtë njoftim nga faqja."));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge danger">Hequr nga moderatorët</span>"#));
    let deleted = post_form(pool.clone(), "/listing/1005/fshi", &[], Some(&ana)).await;
    assert_eq!(deleted.location.as_deref(), Some("/njoftimet-e-mia"));
    // Nobody else decides.
    assert_eq!(post_form(pool.clone(), "/moderim/njoftim/1001/hiq", &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_blocked_sellers_listings_leave_the_pages_of_whoever_blocked_them(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let mercedes = "/listing/1002-mercedes-benz-c-220-2018-automatike";
    // Blocking takes being signed in.
    assert!(!html(pool.clone(), golf).await.contains("Blloko shitësin"));
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    // The listing's page offers no block: the report's thanks do, once Besnik has reported it.
    assert!(!get_with_cookie(pool.clone(), golf, &besnik).await.body.contains("/listing/1001/blloko"));
    let report = "/listing/1001/raporto";
    assert!(!get_with_cookie(pool.clone(), report, &besnik).await.body.contains("Blloko shitësin"));
    post_form(pool.clone(), report, &[("reason", "SPAM"), ("details", "")], Some(&besnik)).await;
    let thanks = get_with_cookie(pool.clone(), report, &besnik).await.body;
    assert!(thanks.contains("Faleminderit!") && thanks.contains("Nëse s'do ta shohësh më këtë shitës, bllokoje:"), "{thanks}");
    assert!(thanks.contains(r#"<form method="post" action="/listing/1001/blloko">"#) && thanks.contains("</svg>Blloko shitësin</button>"));

    // Besnik blocks Ana, who sells the Golf, the iPhone and three more: the thanks say so then.
    let blocked = post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&besnik)).await;
    assert_eq!(blocked.location.as_deref(), Some("/listing/1001/raporto"));
    let thanks = get_with_cookie(pool.clone(), report, &besnik).await.body;
    assert!(thanks.contains("E ke bllokuar edhe shitësin") && !thanks.contains("Blloko shitësin"), "{thanks}");
    // Her listings leave his pages: the welcome page, a category's, a search, those under another
    // listing; others still see them.
    let anas = [1001, 1005, 1007, 1010, 1012];
    let his = |uri: &'static str| {
        let (pool, besnik) = (pool.clone(), besnik.clone());
        async move { card_numbers(&get_with_cookie(pool, uri, &besnik).await.body.replace("&#38;", "&")) }
    };
    let home = his("/").await;
    assert!(!home.is_empty() && home.iter().all(|number| !anas.contains(number)), "{home:?}");
    assert!(card_numbers(&html(pool.clone(), "/").await).contains(&1001));
    assert!(!his("/c/makina").await.contains(&1001));
    assert!(!his("/search?q=golf").await.contains(&1001));
    assert!(card_numbers(&html(pool.clone(), mercedes).await).contains(&1001), "the Golf is like the Mercedes");
    assert!(!his(mercedes).await.contains(&1001));
    // Her listing's own page, which he can still open, says so, and where to let her be again.
    let page = get_with_cookie(pool.clone(), golf, &besnik).await.body;
    assert!(page.contains("E ke bllokuar këtë shitës") && page.contains(r#"E zhbllokon te <a href="/llogaria/cilesimet">Cilësimet</a>."#));
    assert!(!page.contains("Zhblloko shitësin") && !page.contains("listing-safety"));

    // "Cilësimet" lists her, to let be again, and says so after.
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besnik).await.body;
    assert!(
        settings.contains(r#"<a href="/perdorues/11111111-1111-4111-8111-111111111111"><span class="avatar" aria-hidden="true">A</span><span class="blocked-seller-name">Ana</span></a>"#),
        "{settings}"
    );
    let unblocked = post_form(pool.clone(), "/llogaria/zhblloko/11111111-1111-4111-8111-111111111111", &[], Some(&besnik)).await;
    assert_eq!(unblocked.location.as_deref(), Some("/llogaria/cilesimet?u=zhbllokuar"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=zhbllokuar", &besnik).await.body.contains("U zhbllokua."));
    assert!(his("/c/makina").await.contains(&1001));

    // Nobody blocks themselves.
    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
}
