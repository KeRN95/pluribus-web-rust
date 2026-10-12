//! Stores (web/stores.rs): a store's page, and the listings its people post as it; its owner asking
//! people in, until they go; the conversations its people share; whom a store's listing tells of
//! its expiry; and closing an account that owns one.

mod common;

use axum::{body::Body, http::{Request, StatusCode, header}};
use common::*;
use sqlx::PgPool;

/// A form with named files as the browser sends it (a store's "logo" and "cover"): its content
/// type and its body.
fn multipart_files(fields: &[(&str, &str)], files: &[(&str, Vec<u8>)]) -> (String, Vec<u8>) {
    let boundary = "----marketplace-test-boundary";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    for (name, bytes) in files {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// A store's form sent, with its files, as `cookie`.
async fn post_store(pool: &PgPool, uri: &str, fields: &[(&str, &str)], files: &[(&str, Vec<u8>)], cookie: &str) -> TestResponse {
    let (content_type, body) = multipart_files(fields, files);
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap();
    send(pool.clone(), request).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_store_has_its_page_and_its_people_post_as_it(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let store_row = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
            "SELECT display_name, public_phone::text, logo_key, logo_url FROM businesses WHERE slug = 'auto-elbasani'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // Signed out, the form asks to sign in; signed in, it starts from the account's number.
    assert!(get(pool.clone(), "/dyqani/krijo").await.body.contains(r#"action="/hyr""#));
    let form = get_with_cookie(pool.clone(), "/dyqani/krijo", &ana).await.body;
    assert!(form.contains(r#"<input id="phone" name="phone" value="+355 69 111 2233""#), "{form}");

    // Its fields checked, each problem under its own.
    let refused = post_store(&pool, "/dyqani/krijo", &[("name", "A"), ("phone", "123"), ("website", "dyqani"), ("nipt", "123")], &[], &ana).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    for why in ["Shkruaj emrin e dyqanit", "Shkruaj numrin si", "Shkruaj adresën e faqes", "NIPT-i ka 10 shenja"] {
        assert!(refused.body.contains(why), "{why}");
    }

    // Made: to its page, at an address made from its name, with its logo and cover.
    let fields = [
        ("name", "Auto Elbasani"),
        ("description", "Makina të përdorura nga Gjermania.\n\nShërbim pas shitjes."),
        ("phone", "069 111 2233"),
        ("whatsapp", "1"),
        ("email", "info@autoelbasani.al"),
        ("website", "www.autoelbasani.al"),
        ("city", "elbasan"),
        ("address", "Rruga e Durrësit 10"),
        ("hours", "E hënë – e shtunë, 9:00 – 19:00"),
        ("nipt", "l12345678a"),
    ];
    let made = post_store(&pool, "/dyqani/krijo", &fields, &[("logo", jpeg(80, 60)), ("cover", jpeg(300, 100))], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"), "{}", made.body);
    let (_, phone, logo_key, logo_url) = store_row(pool.clone()).await;
    assert_eq!(phone.as_deref(), Some("+355691112233"));
    let logo_key = logo_key.expect("a logo");
    assert!(media().get(&logo_key).await.unwrap().is_some());
    let page = get_with_cookie(pool.clone(), "/biznes/auto-elbasani", &ana).await.body;
    assert!(page.contains(r#"<h1 translate="no">Auto Elbasani</h1>"#) && page.contains(r#"<img class="store-cover" src="/media/stores/"#), "{page}");
    assert!(page.contains(&format!(r#"<img class="avatar large avatar-logo" src="{}""#, logo_url.unwrap())));
    assert!(page.contains(r#"<p class="store-about">Makina të përdorura nga Gjermania.</p>"#) && page.contains(r#"<p class="store-about">Shërbim pas shitjes.</p>"#));
    assert!(page.contains("<dd>Rruga e Durrësit 10, Elbasan</dd>") && page.contains("<dd>E hënë – e shtunë, 9:00 – 19:00</dd>"));
    assert!(page.contains(r#"href="tel:+355691112233""#) && page.contains(r#"href="https://wa.me/355691112233""#));
    assert!(page.contains(r#"href="mailto:info@autoelbasani.al""#) && page.contains(r#"href="https://www.autoelbasani.al" rel="nofollow ugc noopener" target="_blank">www.autoelbasani.al</a>"#));
    assert!(page.contains(r#"<a class="button secondary small" href="/dyqani/auto-elbasani/ndrysho">Ndrysho dyqanin</a>"#));
    assert!(!get(pool.clone(), "/biznes/auto-elbasani").await.body.contains("Ndrysho dyqanin"));
    assert!(!page.contains("L12345678A"), "the NIPT stays on the form");
    // "Cilësimet" lists it, and makes another.
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(settings.contains(r#"<a href="/biznes/auto-elbasani">Auto Elbasani</a>"#) && settings.contains("Pronar") && settings.contains(r#"href="/dyqani/krijo""#));

    // Posted as the store: its seller the store, its number the store's; its card has the logo
    // and leads to the store, whose page lists it.
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    // Its choice carries whom buyers reach then, its number, which the lead names as it is picked
    // (listing_form_controller.js), as it does on a form sent back as the store (it named her own
    // contact once).
    assert!(form.contains(r#"<option value="auto-elbasani" data-contact="+355 69 111 2233">Auto Elbasani</option>"#), "{form}");
    let back = post_form(pool.clone(), "/shto", &[listing_fields(("title", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&ana)).await;
    assert_eq!(back.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(back.body.contains(r#"Blerësit të kontaktojnë te <strong id="poster-contact">+355 69 111 2233</strong>."#), "{}", back.body);
    let not_hers = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "nje-tjeter")]].concat(), Some(&ana)).await;
    assert_eq!(not_hers.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(not_hers.body.contains("Zgjidh një nga dyqanet e tua, ose veten."));
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&ana)).await;
    let url = posted.location.expect("posted");
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();
    let (seller_user, store_name, contact): (Option<String>, String, Option<String>) = sqlx::query_as(
        "SELECT l.seller_user_id::text, b.display_name, c.phone_e164::text
         FROM listings l JOIN businesses b ON b.id = l.seller_business_id JOIN listing_contacts c ON c.listing_id = l.id
         WHERE l.public_no = $1",
    )
    .bind(no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((seller_user, store_name.as_str(), contact.as_deref()), (None, "Auto Elbasani", Some("+355691112233")));
    let listing = get(pool.clone(), &url).await.body;
    assert!(listing.contains(r#"<img class="avatar avatar-logo"#) && listing.contains(r#"href="/biznes/auto-elbasani""#), "{listing}");
    assert!(card_numbers(&get(pool.clone(), "/biznes/auto-elbasani").await.body).contains(&no));

    // Changed by its owner, nobody else: its name, a fixed line for its number, which its listing
    // shows now, and its logo taken off, the file with it; its address stays.
    assert_eq!(get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/ndrysho", &besa).await.status, StatusCode::NOT_FOUND);
    let form = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/ndrysho", &ana).await.body;
    assert!(form.contains(r#"value="Auto Elbasani""#) && form.contains(r#"name="remove_logo""#) && form.contains(r#"value="L12345678A""#), "{form}");
    // The pictures' fields have ids of their own, which their two labels each name: the header's
    // logo is "logo".
    assert!(form.matches(r#"id="logo""#).count() == 1 && form.contains(r#"<input id="store-logo" name="logo" type="file""#), "{form}");
    assert!(form.matches(r#"for="store-logo""#).count() == 2 && form.matches(r#"for="store-cover""#).count() == 2, "{form}");
    let changed = [("name", "Auto Elbasani Plus"), ("phone", "054 222 333"), ("city", "elbasan"), ("remove_logo", "1")];
    let saved = post_store(&pool, "/dyqani/auto-elbasani/ndrysho", &changed, &[], &ana).await;
    assert_eq!(saved.location.as_deref(), Some("/biznes/auto-elbasani"), "{}", saved.body);
    let (name, phone, logo, _) = store_row(pool.clone()).await;
    assert_eq!((name.as_str(), phone.as_deref(), logo), ("Auto Elbasani Plus", Some("+35554222333"), None));
    assert!(media().get(&logo_key).await.unwrap().is_none());
    let contact: Option<String> = sqlx::query_scalar(
        "SELECT c.phone_e164::text FROM listing_contacts c JOIN listings l ON l.id = c.listing_id WHERE l.public_no = $1",
    )
    .bind(no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(contact.as_deref(), Some("+35554222333"));
    // Its page writes the landline as people do, the town's code apart.
    let page = get(pool.clone(), "/biznes/auto-elbasani").await.body;
    assert!(page.contains(r#"href="tel:+35554222333""#) && page.contains("</svg>+355 54 222 333</a>"), "{page}");
    assert_eq!(post_store(&pool, "/dyqani/auto-elbasani/ndrysho", &changed, &[], &besa).await.status, StatusCode::NOT_FOUND);

    // Another of the same name gets an address of its own.
    let second = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani Plus")], &[], &besa).await;
    assert_eq!(second.location.as_deref(), Some("/biznes/auto-elbasani-plus"));
    let third = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani Plus")], &[], &besa).await;
    assert_eq!(third.location.as_deref(), Some("/biznes/auto-elbasani-plus-2"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_owner_asks_people_in_who_post_for_it_until_they_go(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let carla = new_seller(&pool, "+355691112255").await;
    let made = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233"), ("city", "elbasan")], &[], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"));
    let invitations = |pool: PgPool| async move {
        sqlx::query_scalar::<_, String>("SELECT id::text FROM business_invitations ORDER BY created_at").fetch_all(&pool).await.unwrap()
    };

    // Its people: its owner alone, on a page for its owner and admins, linked from its page.
    let people = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit", &ana).await.body;
    assert!(people.contains("Përdorues (ti)") && people.contains("Pronar"), "{people}");
    assert!(get_with_cookie(pool.clone(), "/biznes/auto-elbasani", &ana).await.body.contains(r#"href="/dyqani/auto-elbasani/njerezit""#));
    assert_eq!(get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit", &besa).await.status, StatusCode::NOT_FOUND);

    // Asked in by number: the account with it hears of it in "Lajmërimet" and "Cilësimet"; the
    // page has WhatsApp's button with the invitation's link written in, and the link to copy.
    let refused = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "123"), ("roli", "EDITOR")], Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Shkruaj një numër celulari") && refused.body.contains(r#"value="123""#));
    let sent = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2244"), ("roli", "EDITOR")], Some(&ana)).await;
    assert_eq!(sent.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=ftuar"));
    let invitation = invitations(pool.clone()).await.remove(0);
    let people = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit?u=ftuar", &ana).await.body.replace("&#38;", "&");
    assert!(people.contains("Ftesat në pritje") && people.contains("+355 69 111 2244") && people.contains("Ftesa u ruajt."), "{people}");
    assert!(people.contains("https://wa.me/355691112244?text=") && people.contains(&format!("/ftesa/{invitation}")), "{people}");
    let heard = get_with_cookie(pool.clone(), "/lajmerimet", &besa).await.body;
    assert!(heard.contains("Auto Elbasani të fton në dyqan") && heard.contains(&format!(r#"href="/ftesa/{invitation}""#)), "{heard}");
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besa).await.body.contains(&format!(r#"action="/ftesa/{invitation}/prano""#)));

    // Its page: the store and the role. Someone else learns whom it is for, and can't take it;
    // someone signed out comes back to it after signing in.
    let theirs = get_with_cookie(pool.clone(), &format!("/ftesa/{invitation}"), &carla).await.body;
    assert!(theirs.contains("Auto Elbasani të fton") && theirs.contains("Poston njoftime") && theirs.contains("+355 69 *** **44"), "{theirs}");
    assert!(!theirs.contains("/prano"));
    assert_eq!(post_form(pool.clone(), &format!("/ftesa/{invitation}/prano"), &[], Some(&carla)).await.status, StatusCode::NOT_FOUND);
    let signed_out = get(pool.clone(), &format!("/ftesa/{invitation}")).await;
    assert!(signed_out.cookie("kthehu").is_some_and(|back| back.contains(&invitation)), "{:?}", signed_out.cookies);

    // Taken: the store is in Besa's "Cilësimet" with her role and the way out, not its people.
    let taken = post_form(pool.clone(), &format!("/ftesa/{invitation}/prano"), &[], Some(&besa)).await;
    assert_eq!(taken.location.as_deref(), Some("/llogaria/cilesimet?u=pranuar"));
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=pranuar", &besa).await.body;
    assert!(settings.contains(r#"<a href="/biznes/auto-elbasani">Auto Elbasani</a>"#) && settings.contains("Poston njoftime"), "{settings}");
    assert!(settings.contains(r#"action="/dyqani/auto-elbasani/largohu""#) && !settings.contains("/dyqani/auto-elbasani/njerezit"));
    assert!(invitations(pool.clone()).await.is_empty());

    // She posts as the store; its owner changes the listing too. Her own listings leave it out,
    // the store's pill lists it, for the store's people alone.
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&besa)).await;
    let url = posted.location.expect("posted");
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.status, StatusCode::OK);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&format!(r#"action="/listing/{no}/caktivizo""#)));
    let hers = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body;
    assert!(hers.contains(r#"href="/njoftimet-e-mia?dyqani=auto-elbasani""#) && !hers.contains(&format!("/listing/{no}-")), "{hers}");
    let stores = get_with_cookie(pool.clone(), "/njoftimet-e-mia?dyqani=auto-elbasani", &ana).await.body;
    assert!(stores.contains(&format!("/listing/{no}-")) && stores.contains("Njoftimet e <strong>Auto Elbasani</strong>"), "{stores}");
    assert_eq!(get_with_cookie(pool.clone(), "/njoftimet-e-mia?dyqani=auto-elbasani", &carla).await.status, StatusCode::NOT_FOUND);

    // Made one who answers its messages alone: no posting as it, no changing its listings.
    let besa_id = account_of(&besa);
    let changed = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}"), &[("roli", "VIEWER")], Some(&ana)).await;
    assert_eq!(changed.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=roli"));
    assert_eq!(get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/shto", &besa).await.body.contains(r#"value="auto-elbasani""#));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besa).await.body.contains("Vetëm mesazhet"));
    // Nobody takes the owner's role or lets the owner go, nor does the owner leave.
    let ana_id = account_of(&ana);
    assert_eq!(post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{ana_id}/hiq"), &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), "/dyqani/auto-elbasani/largohu", &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}"), &[("roli", "OWNER")], Some(&ana)).await.status, StatusCode::NOT_FOUND);

    // Asking in someone who is in already says so; an address hears of it by e-mail, and the
    // invitation can be withdrawn.
    let member = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2244"), ("roli", "ADMIN")], Some(&ana)).await;
    assert_eq!(member.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=anetar"));
    let by_mail = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "Carla@Example.test"), ("roli", "ADMIN")], Some(&ana)).await;
    assert_eq!(by_mail.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=ftuar"));
    let withdrawn = invitations(pool.clone()).await.remove(0);
    let cancelled = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/ftesat/{withdrawn}/anulo"), &[], Some(&ana)).await;
    assert_eq!(cancelled.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=anuluar"));
    assert!(invitations(pool.clone()).await.is_empty());
    assert_eq!(get_with_cookie(pool.clone(), &format!("/ftesa/{withdrawn}"), &carla).await.status, StatusCode::NOT_FOUND);

    // Turned down, it is gone; taken by Carla, who then leaves.
    post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2255"), ("roli", "EDITOR")], Some(&ana)).await;
    let declined = invitations(pool.clone()).await.remove(0);
    let turned = post_form(pool.clone(), &format!("/ftesa/{declined}/refuzo"), &[], Some(&carla)).await;
    assert_eq!(turned.location.as_deref(), Some("/llogaria/cilesimet?u=refuzuar"));
    assert!(invitations(pool.clone()).await.is_empty());
    post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2255"), ("roli", "EDITOR")], Some(&ana)).await;
    let again = invitations(pool.clone()).await.remove(0);
    post_form(pool.clone(), &format!("/ftesa/{again}/prano"), &[], Some(&carla)).await;
    let left = post_form(pool.clone(), "/dyqani/auto-elbasani/largohu", &[], Some(&carla)).await;
    assert_eq!(left.location.as_deref(), Some("/llogaria/cilesimet?u=larguar"));

    // Besa let go: the listing she posted stays the store's, and hers to change no more.
    let removed = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}/hiq"), &[], Some(&ana)).await;
    assert_eq!(removed.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=hequr"));
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM business_members m JOIN businesses b ON b.id = m.business_id WHERE b.slug = 'auto-elbasani'").fetch_one(&pool).await.unwrap();
    assert_eq!(members, 1);
    let paused = post_form(pool.clone(), &format!("/listing/{no}/caktivizo"), &[], Some(&besa)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "ACTIVE", "{:?}", paused.status);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn closing_an_account_closes_the_stores_it_owns(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let made = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233")], &[("logo", jpeg(80, 80))], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"));
    post_store(&pool, "/dyqani/krijo", &[("name", "Besa Shop")], &[], &besa).await;
    // Each posts for the other's store too.
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, role)
         SELECT b.id, $1::uuid, 'EDITOR' FROM businesses b WHERE b.slug = 'auto-elbasani'
         UNION ALL SELECT b.id, $2::uuid, 'EDITOR' FROM businesses b WHERE b.slug = 'besa-shop'",
    )
    .bind(account_of(&besa))
    .bind(account_of(&ana))
    .execute(&pool)
    .await
    .unwrap();
    let post_as = |cookie: String, store: &'static str| {
        let pool = pool.clone();
        async move {
            let posted = post_form(pool, "/shto", &[listing_fields(("", ""), &started()), vec![("si", store)]].concat(), Some(&cookie)).await;
            posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().parse::<i64>().unwrap()
        }
    };
    let ana_for_hers = post_as(besa.clone(), "auto-elbasani").await;
    let hers_by_ana = post_as(ana.clone(), "besa-shop").await;
    let logo: String = sqlx::query_scalar("SELECT logo_key FROM businesses WHERE slug = 'auto-elbasani'").fetch_one(&pool).await.unwrap();
    assert!(media().get(&logo).await.unwrap().is_some());

    let closed = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", "FSHI")], Some(&ana)).await;
    assert!(closed.body.contains("<h1>Llogaria u fshi</h1>"), "{}", closed.body);
    assert!(closed.body.contains("dyqanet që kishe krijuar u mbyllën bashkë me njoftimet e tyre"), "{}", closed.body);
    // Her store is closed, its listings with it, whoever posted them, and its logo's file.
    let status = |slug: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM businesses WHERE slug = $1").bind(slug).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(status("auto-elbasani").await, "DELETED");
    assert_eq!(get(pool.clone(), "/biznes/auto-elbasani").await.status, StatusCode::NOT_FOUND);
    let listing_status = |no: i64| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(listing_status(ana_for_hers).await, "DELETED");
    assert!(media().get(&logo).await.unwrap().is_none());
    // Besa's store stays, with the listing Ana posted for it, and Besa its only one.
    assert_eq!(status("besa-shop").await, "ACTIVE");
    assert_eq!(listing_status(hers_by_ana).await, "ACTIVE");
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM business_members m JOIN businesses b ON b.id = m.business_id WHERE b.slug IN ('auto-elbasani', 'besa-shop')").fetch_one(&pool).await.unwrap();
    assert_eq!(people, 1);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_people_share_its_conversations(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let carla = new_seller(&pool, "+355691112255").await;
    let dan = new_seller(&pool, "+355691112266").await;
    let erion = new_seller(&pool, "+355691112277").await;
    for (cookie, name) in [(&ana, "Ana"), (&besa, "Besa"), (&carla, "Carla"), (&dan, "Dan")] {
        sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1::uuid").bind(account_of(cookie)).bind(name).execute(&pool).await.unwrap();
    }
    post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233")], &[], &ana).await;
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, role)
         SELECT b.id, p.id::uuid, p.role FROM businesses b, (VALUES ($1, 'EDITOR'), ($2, 'VIEWER')) AS p (id, role)
         WHERE b.slug = 'auto-elbasani'",
    )
    .bind(account_of(&besa))
    .bind(account_of(&carla))
    .execute(&pool)
    .await
    .unwrap();
    // Besa posts for the store; its people don't write to their own store.
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&besa)).await;
    let no = posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    assert!(get_with_cookie(pool.clone(), &format!("/listing/{no}/mesazh"), &carla).await.location.is_some_and(|to| to.starts_with(&format!("/listing/{no}-"))));

    // Dan writes: every one of the store's people has it unread, in an inbox that names the store.
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &dan).await.location.unwrap();
    for cookie in [&ana, &besa, &carla] {
        assert_eq!(unread(&pool, cookie).await, r#"{"unread":1}"#);
        let inbox = get_with_cookie(pool.clone(), "/mesazhet", cookie).await.body;
        assert!(inbox.contains(r#"<span class="inbox-name" translate="no">Dan <span class="inbox-store">Auto Elbasani</span></span>"#), "{inbox}");
    }
    assert!(!get_with_cookie(pool.clone(), "/mesazhet", &erion).await.body.contains(&thread));
    assert_eq!(get_with_cookie(pool.clone(), &thread, &erion).await.status, StatusCode::NOT_FOUND);

    // Ana answers, though she didn't post it; Dan hears from the store. For Besa, Ana's answer is
    // the store's, on her side with Ana's name, and no news: Dan's message is still unread.
    let page = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    assert!(page.contains(r#"<span class="conversation-store">Për Auto Elbasani</span>"#) && page.contains("A është ende në shitje?"), "{page}");
    assert_eq!(write(&pool, &thread, "Po, është.", &ana).await.location.as_deref(), Some(thread.as_str()));
    let his = get_with_cookie(pool.clone(), &thread, &dan).await.body;
    assert!(his.contains(r#"<li class="message" id="message-"#) && !his.contains("message-author") && his.contains("Auto Elbasani"), "{his}");
    let hers = get_with_cookie(pool.clone(), &thread, &besa).await.body;
    assert!(hers.contains(r#"<span class="message-author">Ana</span>"#), "{hers}");
    assert_eq!(hers.matches(r#"<li class="message mine""#).count(), 1, "{hers}");
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":1}"#);
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":0}"#);
    assert!(get_with_cookie(pool.clone(), "/mesazhet", &besa).await.body.contains("Ana: Po, është."));
    // Read, Besa's count goes; Carla's stays hers.
    let read = attribute(&hers, "data-conversation-read-url-value");
    let last = message_ids(&hers).pop().unwrap();
    post_form(pool.clone(), &format!("{read}?deri={last}"), &[], Some(&besa)).await;
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":0}"#);
    assert_eq!(unread(&pool, &carla).await, r#"{"unread":1}"#);

    // Left unread a while, Dan's next message is news to those who took part, Besa who posted it
    // and Ana who answered, not to Carla; and the store's answer to Dan, under its name, while a
    // colleague's answer tells nobody on the store's side.
    let due = || pluribus_services::notices::send_due(&pool, "https://pluribus.example");
    write(&pool, &thread, "Sa është çmimi i fundit?", &dan).await;
    minutes_pass(&pool, 4).await;
    due().await.unwrap();
    let notices = message_notices(&pool).await;
    let mut told: Vec<&str> = notices.iter().map(|notice| notice.0.as_str()).collect();
    told.sort_unstable();
    let mut took_part = [account_of(&besa), account_of(&ana)];
    took_part.sort_unstable();
    assert_eq!(told, took_part, "{notices:?}");
    write(&pool, &thread, "32 000 lekë.", &carla).await;
    minutes_pass(&pool, 70).await;
    due().await.unwrap();
    let notices = message_notices(&pool).await;
    let last = notices.last().unwrap();
    assert_eq!(last.0, account_of(&dan));
    assert!(last.1.starts_with("Auto Elbasani"), "{notices:?}");

    // Besa blocks Dan: he can't write to the store; Ana sees it blocked, and lets him again.
    post_form(pool.clone(), &format!("{thread}/blloko"), &[], Some(&besa)).await;
    let refused = write(&pool, &thread, "Përshëndetje?", &dan).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(get_with_cookie(pool.clone(), &thread, &ana).await.body.contains(&format!(r#"action="{thread}/zhblloko""#)));
    post_form(pool.clone(), &format!("{thread}/zhblloko"), &[], Some(&ana)).await;
    assert_eq!(write(&pool, &thread, "Përshëndetje?", &dan).await.location.as_deref(), Some(thread.as_str()));

    // Let go, Besa sees it no more.
    sqlx::query("DELETE FROM business_members WHERE user_id = $1::uuid").bind(account_of(&besa)).execute(&pool).await.unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &thread, &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/mesazhet", &besa).await.body.contains(&thread));
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":0}"#);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_listing_tells_its_expiry_to_whoever_still_keeps_it(pool: PgPool) {
    // Besa posts for Dritan's store, Auto Durrësi.
    let besa = new_seller(&pool, "+355691112244").await;
    let store = "44444444-4444-4444-8444-444444444444";
    sqlx::query("INSERT INTO business_members (business_id, user_id, role) VALUES ($1::uuid, $2::uuid, 'EDITOR')")
        .bind(store)
        .bind(account_of(&besa))
        .execute(&pool)
        .await
        .unwrap();
    let no = post_listing(&pool, &besa).await;
    sqlx::query("UPDATE listings SET seller_user_id = NULL, seller_business_id = $2::uuid, expires_at = now() + interval '2 days' WHERE public_no = $1::bigint")
        .bind(&no)
        .bind(store)
        .execute(&pool)
        .await
        .unwrap();
    let run = || pluribus_services::expiry::run(&pool, "https://pluribus.example");
    let told = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, String)>(
            "SELECT user_id::text, notification_type FROM notifications
             WHERE notification_type IN ('EXPIRING', 'EXPIRED') ORDER BY created_at, notification_type",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    let expiring = |user: &str| (user.to_string(), "EXPIRING".to_string());

    // While she posts for it, Besa hears that it is about to expire.
    assert_eq!(run().await.unwrap(), (1, 0));
    assert_eq!(told(pool.clone()).await, [expiring(account_of(&besa))]);

    // Gone from the store, she hears no more of it: Dritan, its owner, who may renew it, does,
    // of the same expiry, and of its end.
    sqlx::query("DELETE FROM business_members WHERE user_id = $1::uuid").bind(account_of(&besa)).execute(&pool).await.unwrap();
    assert_eq!(run().await.unwrap(), (1, 0));
    assert_eq!(run().await.unwrap(), (0, 0));
    assert_eq!(told(pool.clone()).await, [expiring(account_of(&besa)), expiring(DRITAN_ID)]);
    let dritan = session_for(&pool, DRITAN_ID).await;
    assert!(get_with_cookie(pool.clone(), &format!("/listing/{no}-samsung-galaxy-s21-128-gb"), &dritan).await.body.contains(&format!(r#"<form method="post" action="/listing/{no}/rifresko">"#)));
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(run().await.unwrap(), (0, 1));
    assert_eq!(told(pool.clone()).await.last(), Some(&(DRITAN_ID.to_string(), "EXPIRED".to_string())));
}
