//! An account (web/account.rs, settings.rs, notifications.rs, unsubscribe.rs): signing in with a
//! code, and its limits; the number signed in with, shown as verified; the cookie that tells the
//! scripts someone is signed in; "Cilësimet"; the notifications page, and the e-mails' links that
//! stop a kind of notice; and closing an account.

mod common;

use axum::{body::Body, http::{Request, StatusCode, header}};
use common::*;
use serde_json::Value;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn signing_in_takes_a_code_sent_to_the_phone(pool: PgPool) {
    // Not signed in, "Llogaria" asks for a name and a phone number or an e-mail address, and
    // says it signs in as well as makes an account.
    let page = get(pool.clone(), "/llogaria").await;
    assert!(page.body.contains("<h1>Hyr ose krijo një llogari</h1>"));
    // The tab's title says what the heading does ("një" left out once).
    assert!(page.body.contains("<title>Hyr ose krijo një llogari | Pluribus</title>"));
    assert!(page.body.contains(r#"<form class="account-form" action="/hyr" method="post">"#));

    let sent = post_form(
        pool.clone(),
        "/hyr",
        &[("name", "  Erion   Hoxha "), ("contact", "069 123 4567"), ("started", &started())],
        None,
    )
    .await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER);
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}", sent.cookie("login").unwrap());
    // The code itself isn't stored, only its hash; give it a known value to type in.
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let page = get_with_cookie(pool.clone(), "/hyr/kodi", &login).await;
    assert!(page.body.contains("<strong>+355 69 *** **67</strong>"));
    // The code in six boxes, as the canvas has them, over the one input it goes into, which
    // stays the field the browser fills in and the form sends (code_boxes_controller.js).
    assert!(page.body.contains(r#"<div class="code-boxes" data-controller="code-boxes">"#), "{}", page.body);
    assert_eq!(page.body.matches(r#"<span class="code-box" data-code-boxes-target="box"></span>"#).count(), 6);
    assert!(page.body.contains(r#"autocomplete="one-time-code""#) && page.body.contains(r#"class="code-input" autofocus"#));

    let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "000000")], Some(&login)).await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(wrong.body.contains("Kodi nuk është i saktë. Të kanë mbetur 4 prova."));

    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123 456")], Some(&login)).await;
    assert_eq!(signed.status, StatusCode::SEE_OTHER);
    assert_eq!(signed.location.as_deref(), Some("/njoftimet-e-mia"));
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert_eq!(signed.cookie("login"), Some(""), "the pending code's cookie is cleared");

    // Signed in: the form for the listing, with the contact buyers will get.
    let form = get_with_cookie(pool.clone(), "/shto", &session).await.body;
    assert!(form.contains("Publiko njoftimin"));
    assert!(form.contains(r#"<strong id="poster-contact">+355 69 *** **67</strong>"#));
    let (name, status): (String, String) =
        sqlx::query_as("SELECT display_name, status FROM users WHERE phone_e164 = '+355691234567'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), status.as_str()), ("Erion Hoxha", "ACTIVE"));
    // Signed in, "Llogaria" leads to one's own listings.
    let account = get_with_cookie(pool.clone(), "/llogaria", &session).await;
    assert_eq!(account.location.as_deref(), Some("/njoftimet-e-mia"));

    // A used code doesn't work twice, and signing out ends the session.
    let again = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    let out = post_form(pool.clone(), "/dil", &[], Some(&session)).await;
    assert_eq!(out.location.as_deref(), Some("/"));
    assert!(get_with_cookie(pool, "/llogaria", &session).await.body.contains("Hyr ose krijo një llogari"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sign_in_codes_are_limited(pool: PgPool) {
    let codes = |pool: PgPool| async move {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM login_codes").fetch_one(&pool).await.unwrap()
    };
    // Bots: the hidden field filled in, or the form sent the moment it was shown.
    let now = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
    let trapped =
        post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("website", "x"), ("started", &started())], None)
            .await;
    let hasty = post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("started", &now)], None).await;
    assert_eq!(trapped.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(hasty.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(codes(pool.clone()).await, 0);

    // Something that is neither a mobile number nor an e-mail address.
    let landline = post_form(pool.clone(), "/hyr", &[("contact", "04 222 3333"), ("started", &started())], None).await;
    assert!(landline.body.contains("Shkruaj një numër celulari"));

    // One code a minute for a contact.
    let first = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    assert_eq!(first.status, StatusCode::SEE_OTHER);
    let second = post_form(pool.clone(), "/hyr", &[("contact", "Ana@Example.com"), ("started", &started())], None).await;
    assert!(second.body.contains("Prit një minutë"));

    // Twenty an hour from one network.
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at)
         SELECT 'n' || g || '@example.com', decode('00', 'hex'), '203.0.113.7', now() + interval '10 minutes'
         FROM generate_series(1, 20) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = Request::post("/hyr")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::from(format!("contact=besa%40example.com&started={}", started())))
        .unwrap();
    assert!(send(pool.clone(), request).await.body.contains("Shumë kërkesa"));

    // Five wrong guesses use a code up.
    sqlx::query("UPDATE login_codes SET created_at = now() - interval '2 minutes' WHERE contact = 'ana@example.com'")
        .execute(&pool)
        .await
        .unwrap();
    let sent = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    for _ in 0..4 {
        let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
        assert!(wrong.body.contains("nuk është i saktë"));
    }
    let last = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
    assert!(last.body.contains("Kërko një kod të ri"));
}

/// Ana's choice of `column` in notification_preferences: on, where she has made none.
async fn ana_wants(pool: &PgPool, column: &str) -> bool {
    sqlx::query_scalar(&format!("SELECT coalesce((SELECT {column} FROM notification_preferences WHERE user_id = $1::uuid), true)"))
        .bind(ANA_ID)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn an_emails_link_stops_its_kind_without_signing_in(pool: PgPool) {
    // Ana signs in with an address as well: her notice goes by e-mail, with the link that stops
    // them, its token made as it went; the buyer, with a number alone, has none.
    let buyer = new_seller(&pool, "+355691112233").await;
    write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await;
    minutes_pass(&pool, 4).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    let tokens: Vec<(String, String)> = sqlx::query_as("SELECT id::text, notice_token FROM users WHERE notice_token IS NOT NULL").fetch_all(&pool).await.unwrap();
    assert_eq!(tokens.len(), 1);
    let (whose, token) = &tokens[0];
    assert_eq!((whose.as_str(), token.len()), (ANA_ID, 32));

    // Opened, the link only asks: a link checker following it stops nothing.
    let link = format!("/ndalo/{token}/mesazhet");
    let asked = get(pool.clone(), &link).await;
    assert_eq!(asked.status, StatusCode::OK);
    assert!(asked.body.contains("<h1>T'i ndalojmë?</h1>"), "{}", asked.body);
    assert!(asked.body.contains("Nuk do të të vijnë më email-e për mesazhet që nuk i ke lexuar. Mesazhet i gjen si gjithmonë te „Mesazhet“."), "{}", asked.body);
    assert!(asked.body.contains(&format!(r#"<form method="post" action="{link}">"#)));
    assert!(ana_wants(&pool, "message_alerts").await);
    // A mail program's one-click POST (List-Unsubscribe-Post), with no cookie and nothing more,
    // stops them, as the box on "Cilësimet" does; "Rikthe" takes it back.
    let stopped = post_form(pool.clone(), &link, &[("List-Unsubscribe", "One-Click")], None).await;
    assert_eq!(stopped.status, StatusCode::OK);
    assert!(stopped.body.contains("<h1>U ndalën</h1>"), "{}", stopped.body);
    assert!(stopped.body.contains(&format!(r#"<form method="post" action="{link}/rikthe">"#)));
    assert!(!ana_wants(&pool, "message_alerts").await);
    assert!(post_form(pool.clone(), &format!("{link}/rikthe"), &[], None).await.body.contains("<h1>U rikthyen</h1>"));
    assert!(ana_wants(&pool, "message_alerts").await);

    // A token or a kind it doesn't know is no page.
    for wrong in [format!("/ndalo/{}/mesazhet", "0".repeat(32)), format!("/ndalo/{token}/tjeter"), format!("/ndalo/{token}/kerkimi-jo")] {
        assert_eq!(get(pool.clone(), &wrong).await.status, StatusCode::NOT_FOUND, "{wrong}");
        assert_eq!(post_form(pool.clone(), &wrong, &[], None).await.status, StatusCode::NOT_FOUND, "{wrong}");
    }

    // A search's link stops that search's alerts, and only a search of Ana's own.
    for user in [ANA_ID, BESNIK_ID] {
        let session = session_for(&pool, user).await;
        post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona?city=tirane")], Some(&session)).await;
    }
    let searches: Vec<(String, String)> = sqlx::query_as("SELECT id::text, name FROM saved_searches ORDER BY user_id").fetch_all(&pool).await.unwrap();
    let ((hers, name), (his, _)) = (&searches[0], &searches[1]);
    let alerts = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>("SELECT alerts_enabled FROM saved_searches WHERE id = $1::uuid").bind(id).fetch_one(&pool).await.unwrap()
        }
    };
    let link = format!("/ndalo/{token}/kerkimi-{hers}");
    let asked = get(pool.clone(), &link).await.body;
    assert!(asked.contains(&format!("Nuk do të të vijnë më email-e për kërkimin „{name}“. Kërkimi mbetet te „Kërkimet e ruajtura“.")), "{asked}");
    post_form(pool.clone(), &link, &[("List-Unsubscribe", "One-Click")], None).await;
    assert!(!alerts(hers.clone()).await);
    post_form(pool.clone(), &format!("{link}/rikthe"), &[], None).await;
    assert!(alerts(hers.clone()).await);
    assert_eq!(get(pool.clone(), &format!("/ndalo/{token}/kerkimi-{his}")).await.status, StatusCode::NOT_FOUND);

    // The expiry warnings' link stops every notice by e-mail, the notifications staying on the
    // site; the other choices stay as they were.
    let link = format!("/ndalo/{token}/email");
    let stopped = post_form(pool.clone(), &link, &[], None).await.body;
    assert!(stopped.contains("Nuk do të të vijnë më lajmërimet me email nga Pluribus. Lajmërimet i gjen te faqja, te „Lajmërimet“."), "{stopped}");
    assert!(!ana_wants(&pool, "email_enabled").await && ana_wants(&pool, "message_alerts").await);

    // A suspended account's link is no page.
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    assert_eq!(get(pool.clone(), &link).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_notifications_page_lists_every_kind_and_reads_them_once_shown(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112244").await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let buyer_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    // What the account button asks for (account_controller.js): how many are unread, and how many
    // of those are about the user's own listings.
    let counts = |session: &str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/lajmerimet/numri", &session).await.body).unwrap();
            (answer["unread"].as_i64().unwrap(), answer["own"].as_i64().unwrap())
        }
    };
    // Shown, the page says so (application.js) at the address it names.
    let saw = |page: &str, session: &str| {
        let url = attribute(page, "data-seen-url");
        let pool = pool.clone();
        let request = Request::post(url).header(header::COOKIE, session).body(Body::empty()).unwrap();
        async move { assert_eq!(send(pool, request).await.status, StatusCode::NO_CONTENT) }
    };

    // Not signed in, the page asks to sign in, and there is nothing to count.
    assert!(get(pool.clone(), "/lajmerimet").await.body.contains("Hyr në llogari për të parë lajmërimet"));
    assert_eq!(counts("").await, (0, 0));
    // Nothing yet: the page says what will come there.
    let empty = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    assert!(empty.contains("Ende s'ke asnjë lajmërim.") && !empty.contains("data-seen-url"), "{empty}");
    // Every page's account button asks for its dot.
    assert!(empty.contains(r#"<a class="icon-button account-button" href="/llogaria" aria-label="Llogaria" title="Llogaria" data-controller="account">"#));

    // The buyer keeps Ana's phone, whose price she then lowers, and keeps a search that has new
    // listings; and writes to her about it, which she leaves unread. Her phone is days from expiring.
    let no = post_listing(&pool, &ana).await;
    // Its first price an hour old, so that the drop comes after it whatever the clock does.
    sqlx::query(
        "UPDATE listing_price_history SET changed_at = changed_at - interval '1 hour'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = $1::bigint)",
    )
    .bind(&no)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(send(pool.clone(), tap_heart(&no, "1", Some(&buyer))).await.status, StatusCode::NO_CONTENT);
    let edited = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("price", "29 000"), &started()), Some(&ana)).await;
    assert_eq!(edited.status, StatusCode::SEE_OTHER);
    post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona?city=tirane")], Some(&buyer)).await;
    let search: uuid::Uuid = sqlx::query_scalar("SELECT id FROM saved_searches").fetch_one(&pool).await.unwrap();
    marketplace::db::record_search_alert(&pool, buyer_id, search, "2 njoftime të reja: Telefona · Tiranë", 2, "https://pluribus.example/kerkimet")
        .await
        .unwrap();
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &buyer).await.location.unwrap();
    minutes_pass(&pool, 4).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(marketplace::expiry::run(&pool, "https://pluribus.example").await.unwrap(), (1, 0));

    // The buyer's: the search's alert, then the price drop, both new, each leading to what it is
    // about; nothing about listings of his own.
    assert_eq!(counts(&buyer).await, (2, 0));
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    let lines = positions(
        &page,
        &[
            &format!(r#"<a class="notification-line" href="/kerkimet/{search}">"#),
            r#"<span class="notification-title"><span class="visually-hidden">E re: </span>2 njoftime të reja: Telefona · Tiranë</span>"#,
            &format!(r#"<a class="notification-line" href="/listing/{no}-samsung-galaxy-s21-128-gb">"#),
            r#"<span class="notification-title"><span class="visually-hidden">E re: </span>Çmimi ra: Samsung Galaxy S21, 128 GB</span>"#,
        ],
    );
    assert!(lines.is_sorted(), "{page}");
    assert_eq!(page.matches(r#"<li class="notification is-unread">"#).count(), 2);
    assert!(page.contains(r#"<span class="notification-when">Sot</span>"#));
    // Fetched, the page reads nothing; shown, it reads them, and the heart loses its dot too.
    assert_eq!(counts(&buyer).await, (2, 0));
    saw(&page, &buyer).await;
    assert_eq!(counts(&buyer).await, (0, 0));
    let heart: Value = serde_json::from_str(&get_with_cookie(pool.clone(), "/te-ruajturat/numrat", &buyer).await.body).unwrap();
    assert_eq!((heart["price_drops"].as_i64(), heart["searches"].as_i64()), (Some(0), Some(0)));
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    assert!(!page.contains("is-unread") && !page.contains("data-seen-url") && page.contains("Çmimi ra: Samsung Galaxy S21, 128 GB"));

    // Ana's: her phone expiring, then the buyer's message, with what he wrote; one about her own
    // listing, for the account button's dot. "Njoftimet e mia" leads there, saying how many are new.
    assert_eq!(counts(&ana).await, (2, 1));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<nav class="account-nav" aria-label="Llogaria jote">"#), "{mine}");
    assert!(mine.contains(r#"<a href="/njoftimet-e-mia" aria-current="page">Njoftimet e mia</a>"#), "{mine}");
    assert!(mine.contains(r#"<a href="/lajmerimet">Lajmërimet · 2 të reja</a>"#), "{mine}");
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &ana).await.body;
    let lines = positions(
        &page,
        &[
            &format!(r#"<a class="notification-line" href="/listing/{no}-samsung-galaxy-s21-128-gb">"#),
            "Njoftimi skadon së shpejti: Samsung Galaxy S21, 128 GB</span>",
            &format!(r#"<a class="notification-line" href="{thread}">"#),
            r#"<span class="notification-body">A është ende në shitje?</span>"#,
        ],
    );
    assert!(lines.is_sorted(), "{page}");
    // Her phone expires after the page was made: not read with it, as when the browser shows its
    // copy again.
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(marketplace::expiry::run(&pool, "https://pluribus.example").await.unwrap(), (0, 1));
    saw(&page, &ana).await;
    assert_eq!(counts(&ana).await, (1, 1));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/lajmerimet">Lajmërimet · 1 e re</a>"#));
    // Renewing the listing reads what was said of its expiry.
    post_form(pool.clone(), &format!("/listing/{no}/rifresko"), &[], Some(&ana)).await;
    assert_eq!(counts(&ana).await, (0, 0));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/lajmerimet">Lajmërimet</a>"#));

    // Another message left unread an hour on brings another notice; reading the conversation reads it.
    write(&pool, &thread, "Mund ta shoh nesër?", &buyer).await;
    minutes_pass(&pool, 70).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    assert_eq!(counts(&ana).await, (1, 0));
    let conversation = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    let last = message_ids(&conversation).pop().unwrap();
    post_form(pool.clone(), &format!("{thread}/lexuar?deri={last}"), &[], Some(&ana)).await;
    assert_eq!(counts(&ana).await, (0, 0));
}

/// Adds a contact to the account signed in with `session`, as "Cilësimet" does: the form, then the
/// code sent to it (made known), typed back. Returns where that leads.
async fn add_contact(pool: &PgPool, session: &str, typed: &str, contact: &str) -> TestResponse {
    let sent = post_form(pool.clone(), "/llogaria/kontakti", &[("kontakti", typed)], Some(session)).await;
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"), "{}", sent.body);
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = $1")
        .bind(contact)
        .execute(pool)
        .await
        .unwrap();
    let page = get_with_cookie(pool.clone(), "/hyr/kodi", &format!("{session}; {login}")).await.body;
    assert!(page.contains("Shkruaje dhe shtohet te llogaria jote.") && page.contains(r#"<button type="submit" class="button">Ruaje</button>"#), "{page}");
    post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&format!("{session}; {login}"))).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_settings_change_the_name_the_contacts_what_one_hears_and_the_devices(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let ana_id: String = sqlx::query_scalar("SELECT id::text FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    let no = post_listing(&pool, &ana).await;
    let contact_of_listing = |pool: PgPool, no: String| async move {
        sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT c.phone_e164::text, c.email::text FROM listing_contacts c JOIN listings l ON l.id = c.listing_id WHERE l.public_no = $1::bigint",
        )
        .bind(no)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let account = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, Option<String>, Option<String>, String)>(
            "SELECT display_name, phone_e164::text, email::text, auth_subject FROM users WHERE phone_e164 = '+355691119999' OR display_name = 'Ana Kola'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // Signed out, the page asks to sign in; signed in, it is the account's, the row over its
    // heading marking it.
    assert!(get(pool.clone(), "/llogaria/cilesimet").await.body.contains(r#"action="/hyr""#));
    let page = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(page.contains(r#"<a href="/llogaria/cilesimet" aria-current="page">Cilësimet</a>"#), "{page}");
    assert!(page.contains(r#"<input id="emri" name="emri" value="Përdorues""#) && page.contains("<strong>+355 69 111 2233</strong>"));
    assert!(!page.contains("Hiqe") && !page.contains(r#"name="email""#), "one contact, and no e-mail to send to");
    assert!(page.contains("Llogaria jote është e hapur vetëm në këtë pajisje."));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/llogaria/cilesimet">Cilësimet</a>"#));

    // The name, tidied; an empty one goes back with why.
    let renamed = post_form(pool.clone(), "/llogaria/emri", &[("emri", "  Ana   Kola ")], Some(&ana)).await;
    assert_eq!(renamed.location.as_deref(), Some("/llogaria/cilesimet?u=emri"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=emri", &ana).await.body.contains("Emri u ruajt."));
    assert_eq!(account(pool.clone()).await.0, "Ana Kola");
    let empty = post_form(pool.clone(), "/llogaria/emri", &[("emri", "   ")], Some(&ana)).await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(empty.body.contains("Shkruaj emrin që do të shohin të tjerët."));

    // An e-mail address added, once the code sent to it comes back: verified, beside the number,
    // either of which can now be taken off.
    let added = add_contact(&pool, &ana, "Ana.Kola@Example.test", "ana.kola@example.test").await;
    assert_eq!(added.location.as_deref(), Some("/llogaria/cilesimet?u=kontakti"));
    let (_, phone, email, _) = account(pool.clone()).await;
    assert_eq!((phone.as_deref(), email.as_deref()), (Some("+355691112233"), Some("ana.kola@example.test")));
    let verified: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_verifications WHERE user_id = $1::uuid AND verification_type = 'EMAIL' AND verified_value = 'ana.kola@example.test')",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(verified);
    let page = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=kontakti", &ana).await.body;
    assert!(page.contains("U ruajt te llogaria") && page.contains("<strong>ana.kola@example.test</strong>") && page.matches(">Hiqe</button>").count() == 2, "{page}");
    // Not one the account has, nor another account's.
    for (typed, why) in [("069 111 2233", "Ky është tashmë i llogarisë sate."), ("069 111 2244", "Ky është i një llogarie tjetër")] {
        let refused = post_form(pool.clone(), "/llogaria/kontakti", &[("kontakti", typed)], Some(&ana)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(refused.body.contains(why) && refused.body.contains(&format!(r#"value="{typed}""#)), "{}", refused.body);
    }

    // A new number in place of the old: on the account, and on its listing, which showed the old;
    // the old one free, for whoever signs in with it next.
    let changed = add_contact(&pool, &ana, "069 111 9999", "+355691119999").await;
    assert_eq!(changed.location.as_deref(), Some("/llogaria/cilesimet?u=kontakti"));
    let (_, phone, _, subject) = account(pool.clone()).await;
    assert_eq!(phone.as_deref(), Some("+355691119999"));
    assert_eq!(subject, format!("user:{ana_id}"));
    assert_eq!(contact_of_listing(pool.clone(), no.clone()).await, (Some("+355691119999".to_string()), None));
    let someone_else = sign_in_with_code(&pool, "069 111 2233", "+355691112233").await;
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &someone_else).await.body.contains("<strong>+355 69 111 2233</strong>"));

    // The number taken off: the listing shows the address instead. The address, the last one
    // left, stays.
    let removed = post_form(pool.clone(), "/llogaria/kontakti/hiq", &[("hiq", "numri")], Some(&ana)).await;
    assert_eq!(removed.location.as_deref(), Some("/llogaria/cilesimet?u=hequr"));
    assert_eq!(account(pool.clone()).await.1, None);
    assert_eq!(contact_of_listing(pool.clone(), no.clone()).await, (None, Some("ana.kola@example.test".to_string())));
    post_form(pool.clone(), "/llogaria/kontakti/hiq", &[("hiq", "email")], Some(&ana)).await;
    assert_eq!(account(pool.clone()).await.2.as_deref(), Some("ana.kola@example.test"));

    // What it hears of, as the boxes say; the e-mail box now that there is an address.
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body.contains(r#"<input type="checkbox" name="email" value="1" checked>"#));
    let saved = post_form(pool.clone(), "/llogaria/njoftimet", &[("mesazhet", "1")], Some(&ana)).await;
    assert_eq!(saved.location.as_deref(), Some("/llogaria/cilesimet?u=njoftimet"));
    let preferences: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT message_alerts, price_drop_alerts, saved_search_alerts, email_enabled FROM notification_preferences WHERE user_id = $1::uuid",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preferences, (true, false, false, false));

    // Signed in on another device too: out of it from here, this one staying.
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES (sha256(convert_to('another-device', 'UTF8')), $1::uuid, now() + interval '1 day')",
    )
    .bind(&ana_id)
    .execute(&pool)
    .await
    .unwrap();
    let other = "session=another-device".to_string();
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body.contains("e hapur në 2 pajisje"));
    let out = post_form(pool.clone(), "/llogaria/pajisjet", &[], Some(&ana)).await;
    assert_eq!(out.location.as_deref(), Some("/llogaria/cilesimet?u=pajisjet"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &other).await.body.contains(r#"action="/hyr""#));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=pajisjet", &ana).await.body.contains("Dole nga pajisjet e tjera."));
    let _ = besa;
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn closing_an_account_takes_its_listings_photos_and_contacts_away(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let ana_id: String = sqlx::query_scalar("SELECT id::text FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    // Ana's phone with a photo, which Besa keeps and asks about; a search Ana keeps.
    let posted = post_with_photos(pool.clone(), "/shto", &listing_fields(("", ""), &started()), &[jpeg(40, 30)], &ana).await;
    let url = posted.location.expect("posted");
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    let photo: String = sqlx::query_scalar(
        "SELECT m.storage_key FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(media().get(&photo).await.unwrap().is_some());
    send(pool.clone(), tap_heart(&no, "1", Some(&besa))).await;
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &besa).await.location.unwrap();
    post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona")], Some(&ana)).await;

    // Not without "FSHI" typed: back, with why, and nothing gone.
    let refused = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", "po")], Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Shkruaj FSHI"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);

    // Closed: signed out, the page says what became of it.
    let closed = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", " fshi ")], Some(&ana)).await;
    assert_eq!(closed.status, StatusCode::OK);
    assert!(closed.body.contains("<h1>Llogaria u fshi</h1>"), "{}", closed.body);
    assert!(!closed.body.contains("dyqanet"), "{}", closed.body);
    assert_eq!(closed.cookie("session"), Some(""));
    // Its listing gone, its contact and its photo with it, the file out of the store.
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    let (status, contacts, photos): (String, i64, i64) = sqlx::query_as(
        "SELECT l.status, (SELECT count(*) FROM listing_contacts c WHERE c.listing_id = l.id), (SELECT count(*) FROM listing_media m WHERE m.listing_id = l.id)
         FROM listings l WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status.as_str(), contacts, photos), ("DELETED", 0, 0));
    assert!(media().get(&photo).await.unwrap().is_none());
    // The account anonymised, what only it had gone.
    let (name, phone, subject, user_status): (String, Option<String>, String, String) =
        sqlx::query_as("SELECT display_name, phone_e164::text, auth_subject, status FROM users WHERE id = $1::uuid")
            .bind(&ana_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), phone, subject, user_status.as_str()), ("Përdorues i fshirë", None, format!("deleted:{ana_id}"), "DELETED"));
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM sessions WHERE user_id = $1::uuid) + (SELECT count(*) FROM saved_searches WHERE user_id = $1::uuid)",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"action="/hyr""#));
    assert_eq!(get(pool.clone(), &format!("/perdorues/{ana_id}")).await.status, StatusCode::NOT_FOUND);
    // Besa keeps her message, Ana shown as a deleted user; the listing she kept says it went.
    let conversation = get_with_cookie(pool.clone(), &thread, &besa).await.body;
    assert!(conversation.contains("Përdorues i fshirë") && conversation.contains("A është ende në shitje?"), "{conversation}");
    let kept = get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body;
    assert!(section(&kept, "saved-gone-heading").contains(r#"<span class="badge">U fshi</span>"#), "{kept}");
    // The number is free again: signing in with it opens a new account.
    let again = sign_in_with_code(&pool, "069 111 2233", "+355691112233").await;
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &again).await.body.contains("<strong>+355 69 111 2233</strong>"));
}

/// Signs in with a code sent to this contact, typed as `typed`, which is `contact` once read: the
/// session's cookie.
async fn sign_in_with_code(pool: &PgPool, typed: &str, contact: &str) -> String {
    let sent = post_form(pool.clone(), "/hyr", &[("contact", typed), ("started", &started())], None).await;
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = $1")
        .bind(contact)
        .execute(pool)
        .await
        .unwrap();
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    format!("session={}", signed.cookie("session").expect("signed in"))
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_number_a_seller_signed_in_with_shows_as_verified(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Nobody signed in with a code yet: nothing is verified.
    assert!(!html(pool.clone(), golf).await.contains("verifikuar"));
    // Ana signs in with a code sent to her number: her listings say so beside her name, and her
    // profile under it.
    sign_in_with_code(&pool, "069 000 0001", "+355690000001").await;
    // On every listing, a car's too, a check beside her name, which says what it means as a
    // tooltip and to screen readers; on her profile, the words under her name.
    let page = html(pool.clone(), golf).await;
    assert!(page.contains(r#"<span class="seller-name"><span translate="no">Ana</span><span class="verified" title="Numër i verifikuar">"#), "{page}");
    assert!(!page.contains("</svg>Numër i verifikuar</span>"));
    let iphone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(iphone.contains(r#"<span class="seller-name"><span translate="no">Ana</span><span class="verified" title="Numër i verifikuar">"#), "{iphone}");
    assert!(iphone.contains(r#"<span class="visually-hidden">Numër i verifikuar</span></span></span>"#));
    assert!(html(pool.clone(), "/perdorues/11111111-1111-4111-8111-111111111111").await.contains("Numër i verifikuar</p>"));
    let (kind, value): (String, String) =
        sqlx::query_as("SELECT verification_type, verified_value FROM user_verifications WHERE status = 'VERIFIED'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), value.as_str()), ("PHONE", "+355690000001"));
    // Besnik never did: his flat says nothing of it.
    assert!(!html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await.contains("verifikuar"));
    // A shop's number is verified once one of its people signs in with it: Dritan signs in with
    // his own, which isn't the one the Mercedes shows.
    sign_in_with_code(&pool, "069 000 0003", "+355690000003").await;
    assert!(!html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.contains("verifikuar"));
    // A listing showing another number than the one verified says nothing either.
    sqlx::query("UPDATE listing_contacts SET phone_e164 = '+355690000011' WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1001)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!html(pool.clone(), golf).await.contains("verifikuar"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_scripts_can_tell_whether_someone_is_signed_in(pool: PgPool) {
    // The session cookie is out of scripts' reach; a cookie beside it says there is one, so a page
    // for someone not signed in doesn't ask for their counts. Set when a session comes without it,
    // as after signing in; cleared when it stays after the session has gone, as after signing out.
    let with = |cookie: &'static str, uri: &'static str| {
        let pool = pool.clone();
        async move { send(pool, Request::get(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await }
    };
    assert_eq!(with("session=abc", "/").await.cookie("llogari"), Some("1"));
    assert_eq!(with("llogari=1", "/").await.cookie("llogari"), Some(""));
    assert_eq!(with("session=abc; llogari=1", "/").await.cookie("llogari"), None);
    assert_eq!(get(pool.clone(), "/").await.cookie("llogari"), None);
    // Not on a static file, which browsers keep.
    assert_eq!(with("session=abc", "/static/css/base.css").await.cookie("llogari"), None);
}
