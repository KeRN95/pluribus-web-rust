//! Messages between buyers and sellers (web/messages.rs): writing to a seller and the answers,
//! signing in first, the notices of a message left unread, the limits on a buyer's first messages
//! and on how many sellers an account writes to, blocking a buyer, and a business's listing.

mod common;

use axum::http::StatusCode;
use common::*;
use sqlx::PgPool;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_buyer_writes_to_a_seller_who_answers(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let listing = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Others have the listing's button to write to its seller; she doesn't.
    assert!(get_with_cookie(pool.clone(), listing, &buyer).await.body.contains(r#"href="/listing/1001/mesazh""#));
    assert!(!get_with_cookie(pool.clone(), listing, &ana).await.body.contains("/listing/1001/mesazh"));
    // Signed in, the button under the price (the seller's card has none) opens the conversation
    // in the listing's sheet, in its frame; signed out, the button leads to the page, which asks
    // to sign in first, and there is no sheet.
    let theirs = get_with_cookie(pool.clone(), listing, &buyer).await.body;
    let to_sheet = r#"<a class="button message-button" href="/listing/1001/mesazh" data-turbo-frame="conversation" data-turbo-prefetch="false" data-action="message-sheet#open">"#;
    assert_eq!(theirs.matches(to_sheet).count(), 1, "{theirs}");
    // A wide pill that says its name, beside the phone's in the contact bar, filled in the accent as
    // the site's main buttons are; the phone's plain, on the grey of secondary controls, its icon
    // muted (each wore the accent's light tint for a moment on 2026-10-12, until the user asked
    // both back).
    assert!(theirs.contains(r#"<span class="button-label">Shkruaji shitësit</span></a>"#));
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    assert!(css.contains(".contact-bar .reveal-button .contact-icon{color:var(--muted);"), "the phone's icon muted");
    assert!(!css.contains(".contact-bar .button.reveal-button{background") && !css.contains(".contact-bar .message-button{background"), "no tint");
    assert!(theirs.contains(r#"<div class="message-sheet" id="message-sheet" role="dialog" aria-modal="true""#));
    assert!(theirs.contains(r#"<turbo-frame id="conversation" data-message-sheet-target="frame""#));
    let anyone = get(pool.clone(), listing).await.body;
    assert!(anyone.contains(r#"href="/listing/1001/mesazh""#) && !anyone.contains("data-turbo-frame=\"conversation\"") && !anyone.contains(r#"id="message-sheet""#));
    // The page to write on: to whom, about what, and quick messages to start with; in the frame
    // of the same name, for the sheet to take.
    let page = get_with_cookie(pool.clone(), "/listing/1001/mesazh", &buyer).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains(r#"<turbo-frame id="conversation" data-turbo-action="advance">"#));
    // Who with first, on a line of their own; then the listing, its title the heading.
    let head = positions(&page.body, &[r#"<header class="conversation-head">"#, r#"<p class="conversation-with"><span class="avatar" aria-hidden="true">A</span><span translate="no">Ana</span></p>"#, r#"<h1 class="conversation-title">"#, r#"<span class="conversation-price">"#]);
    assert!(head.is_sorted(), "{head:?}");
    assert!(page.body.contains("Ana") && page.body.contains("Volkswagen Golf") && page.body.contains("Cili është çmimi i fundit?"));
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/mesazh", &ana).await.location.as_deref(), Some(listing));

    let sent = write(&pool, "/listing/1001/mesazh", "  A është ende në shitje?\r\n", &buyer).await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER, "{}", sent.body);
    let url = sent.location.unwrap();
    assert!(url.starts_with("/mesazhet/"), "{url}");
    // Writing about the listing again goes on in the same conversation.
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/mesazh", &buyer).await.location.as_deref(), Some(url.as_str()));

    // The seller has it, unread: in the header's count, and in her inbox in bold.
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":1}"#);
    assert_eq!(unread(&pool, &buyer).await, r#"{"unread":0}"#);
    let inbox = get_with_cookie(pool.clone(), "/mesazhet", &ana).await.body;
    assert!(inbox.contains("is-unread") && inbox.contains("A është ende në shitje?") && inbox.contains("Përdorues"), "{inbox}");
    // Opening it shows it; once the page says it showed it (conversation_controller.js), it is read.
    let thread = get_with_cookie(pool.clone(), &url, &ana).await.body;
    assert!(thread.contains(r#"<p class="message-body">A është ende në shitje?</p>"#), "{thread}");
    let read = attribute(&thread, "data-conversation-read-url-value");
    assert_eq!(read, format!("{url}/lexuar"));
    let first = &message_ids(&thread)[0];
    assert_eq!(post_form(pool.clone(), &format!("{read}?deri={first}"), &[], Some(&ana)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":0}"#);

    // She answers; he sees both, his own on the right, and hers unread.
    assert_eq!(write(&pool, &url, "Po, është.", &ana).await.status, StatusCode::SEE_OTHER);
    let thread = get_with_cookie(pool.clone(), &url, &buyer).await.body;
    assert!(thread.find("A është ende në shitje?").unwrap() < thread.find("Po, është.").unwrap());
    assert!(thread.contains(r#"<li class="message mine""#));
    assert_eq!(unread(&pool, &buyer).await, r#"{"unread":1}"#);

    // What came after a message, for the page to add at its end (conversation_controller.js).
    let ids = message_ids(&thread);
    assert_eq!(ids.len(), 2);
    let newer = get_with_cookie(pool.clone(), &format!("{url}/te-rejat?pas={}", ids[0]), &buyer).await;
    assert_eq!(newer.content_type.as_deref(), Some("text/vnd.turbo-stream.html; charset=utf-8"));
    assert!(newer.body.contains(r#"<turbo-stream action="append" target="thread">"#));
    assert!(newer.body.contains("Po, është.") && !newer.body.contains("A është ende"));
    let none = get_with_cookie(pool.clone(), &format!("{url}/te-rejat?pas={}", ids[1]), &buyer).await;
    assert_eq!(none.status, StatusCode::NO_CONTENT);

    // Nobody else can read it or write in it.
    let besnik = session_for(&pool, BESNIK_ID).await;
    assert_eq!(get_with_cookie(pool.clone(), &url, &besnik).await.status, StatusCode::NOT_FOUND);
    assert_eq!(write(&pool, &url, "Hej", &besnik).await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), "/mesazhet/jo-nje-bisede", &besnik).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn writing_to_a_seller_takes_signing_in_first(pool: PgPool) {
    // The page to write on sends to sign in, and keeps itself for after (saved::after_sign_in).
    let page = get(pool.clone(), "/listing/1001/mesazh").await;
    assert_eq!(page.location.as_deref(), Some("/llogaria"));
    assert_eq!(page.cookie("kthehu"), Some("/listing/1001/mesazh"));
    let inbox = get(pool.clone(), "/mesazhet").await.body;
    assert!(inbox.contains("Hyr në llogari për t'u shkruar shitësve"));
    assert_eq!(get(pool.clone(), "/mesazhet/numri").await.body, r#"{"unread":0}"#);
    // Every page's rail leads to the conversations, the listing's page too, which a seller reads a
    // buyer's message from; on phones, which have no rail, the menu does, its button with the count
    // of those unread (the welcome page's round button in the row with its search went on
    // 2026-10-11).
    for page in ["/c/automjete", "/listing/1001-volkswagen-golf-7-2016-nafte", "/mesazhet", "/"] {
        let body = get(pool.clone(), page).await.body;
        assert!(body.contains(r#"<a class="rail-item" href="/mesazhet""#), "{page}");
        assert!(body.contains(r#"<a class="menu-row" href="/mesazhet">"#), "{page}");
    }
    assert!(!get(pool.clone(), "/").await.body.contains("inbox-shortcut"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_message_left_unread_brings_a_notice_at_most_once_an_hour(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let due = || pluribus_services::notices::send_due(&pool, "https://pluribus.example");
    let thread = write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await.location.unwrap();
    let conversation = thread.trim_start_matches("/mesazhet/").to_string();

    // Just written, nothing: Ana may be reading it. A few minutes on, still unread, she hears of
    // it, once: who wrote, about which listing, the message and where to answer it.
    assert_eq!(due().await.unwrap(), 0);
    minutes_pass(&pool, 4).await;
    assert_eq!(due().await.unwrap(), 1);
    let notices = message_notices(&pool).await;
    assert_eq!(notices.len(), 1);
    let (to, title, body, about) = &notices[0];
    assert_eq!((to.as_str(), body.as_str(), about.as_str()), (ANA_ID, "A është ende në shitje?", conversation.as_str()));
    assert!(title.starts_with("Përdorues të shkroi për „Volkswagen Golf"), "{title}");
    assert_eq!(due().await.unwrap(), 0);

    // More within the hour bring none; past it, the latest of those after the notice brings one.
    write(&pool, &thread, "Mund ta shoh nesër?", &buyer).await;
    minutes_pass(&pool, 10).await;
    assert_eq!(due().await.unwrap(), 0);
    minutes_pass(&pool, 60).await;
    assert_eq!(due().await.unwrap(), 1);
    assert_eq!(message_notices(&pool).await[1].2, "Mund ta shoh nesër?");

    // Read, a message brings nothing however long it waits; and Ana's answer, left unread, tells
    // the buyer.
    write(&pool, &thread, "Në orën 5?", &buyer).await;
    minutes_pass(&pool, 70).await;
    let page = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    let last = message_ids(&page).pop().unwrap();
    let read = attribute(&page, "data-conversation-read-url-value");
    post_form(pool.clone(), &format!("{read}?deri={last}"), &[], Some(&ana)).await;
    assert_eq!(due().await.unwrap(), 0);
    write(&pool, &thread, "Po, në orën 5.", &ana).await;
    minutes_pass(&pool, 4).await;
    assert_eq!(due().await.unwrap(), 1);
    let buyer_id: String = sqlx::query_scalar("SELECT buyer_user_id::text FROM conversations").fetch_one(&pool).await.unwrap();
    assert_eq!(message_notices(&pool).await[2].0, buyer_id);

    // "Cilësimet" says she hears of them, and lets her stop: then nothing, until she says yes
    // again. The form sends every box ticked, the others as they were.
    let checkbox = r#"<input type="checkbox" name="mesazhet" value="1""#;
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(settings.contains(r#"<form class="notice-settings settings-notices" method="post" action="/llogaria/njoftimet">"#));
    assert!(settings.contains(&format!("{checkbox} checked>")), "{settings}");
    let off = post_form(pool.clone(), "/llogaria/njoftimet", &[("cmimet", "1"), ("kerkimet", "1")], Some(&ana)).await;
    assert_eq!(off.location.as_deref(), Some("/llogaria/cilesimet?u=njoftimet"));
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=njoftimet", &ana).await.body;
    assert!(settings.contains(&format!("{checkbox}>")) && settings.contains("Zgjedhjet për njoftimet u ruajtën."), "{settings}");
    write(&pool, &thread, "A je aty?", &buyer).await;
    minutes_pass(&pool, 120).await;
    assert_eq!(due().await.unwrap(), 0);
    post_form(pool.clone(), "/llogaria/njoftimet", &[("mesazhet", "1"), ("cmimet", "1"), ("kerkimet", "1")], Some(&ana)).await;
    assert_eq!(due().await.unwrap(), 1);
    // Not signed in, the form leads to signing in.
    assert_eq!(post_form(pool.clone(), "/llogaria/njoftimet", &[], None).await.location.as_deref(), Some("/llogaria"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn until_the_seller_answers_a_buyer_sends_three_messages_and_no_links(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    // A first message with a link, empty, or too long goes back, as typed, with why.
    let refused = write(&pool, "/listing/1001/mesazh", "Shiko www.example.com", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Lidhjet mund t") && refused.body.contains(">Shiko www.example.com</textarea>"), "{}", refused.body);
    assert!(write(&pool, "/listing/1001/mesazh", " ", &buyer).await.body.contains("Shkruaj mesazhin."));
    assert!(write(&pool, "/listing/1001/mesazh", &"a".repeat(1001), &buyer).await.body.contains("deri në 1000 shkronja"));

    let url = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    for body in ["A është ende në shitje?", "Kur mund ta shoh?"] {
        assert_eq!(write(&pool, &url, body, &buyer).await.status, StatusCode::SEE_OTHER);
    }
    let fourth = write(&pool, &url, "Alo?", &buyer).await;
    assert_eq!(fourth.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fourth.body.contains("Prit që Ana të të përgjigjet"), "{}", fourth.body);
    // Once she answers, he writes on, links and all.
    assert_eq!(write(&pool, &url, "Po, nesër pasdite.", &ana).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Mirë. Ja ku jam: https://maps.example.com/x", &buyer).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Alo?", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn one_account_writes_to_a_few_new_sellers_an_hour_and_a_day(pool: PgPool) {
    let buyer = new_seller(&pool, "+355691112233").await;
    let started = |count: i32, ago: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO conversations (buyer_user_id, seller_user_id, created_at, last_message_at)
                 SELECT u.id, $1::uuid, now() - $3::interval, now() - $3::interval
                 FROM users u, generate_series(1, $2)
                 WHERE u.phone_e164 = '+355691112233'",
            )
            .bind(BESNIK_ID)
            .bind(count)
            .bind(ago)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    // Five sellers written to in the last hour: a sixth waits.
    started(5, "10 minutes").await;
    let refused = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("U ke shkruar 5 shitësve këtë orë"), "{}", refused.body);
    // Earlier in the day they count towards the day's fifteen.
    sqlx::query("UPDATE conversations SET created_at = now() - interval '3 hours'").execute(&pool).await.unwrap();
    started(10, "5 hours").await;
    let refused = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await;
    assert!(refused.body.contains("U ke shkruar 15 shitësve sot"), "{}", refused.body);
    // A day later, he writes again.
    sqlx::query("UPDATE conversations SET created_at = now() - interval '25 hours'").execute(&pool).await.unwrap();
    assert_eq!(write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_seller_blocks_a_buyer_and_lets_him_write_again(pool: PgPool) {
    let besnik = session_for(&pool, BESNIK_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let url = write(&pool, "/listing/1003/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    assert_eq!(post_form(pool.clone(), &format!("{url}/blloko"), &[], Some(&besnik)).await.status, StatusCode::SEE_OTHER);
    // He can't write to Besnik any more, here or about another of his listings.
    let refused = write(&pool, &url, "Alo?", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Besnik nuk pranon mesazhe nga ti."));
    assert!(get_with_cookie(pool.clone(), &url, &buyer).await.body.contains("Besnik nuk pranon mesazhe nga ti."));
    assert_eq!(write(&pool, "/listing/1004/mesazh", "Po kjo shtëpi?", &buyer).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Besnik sees he blocked him, and can let him write again.
    let page = get_with_cookie(pool.clone(), &url, &besnik).await.body;
    assert!(page.contains("E ke bllokuar këtë përdorues") && page.contains(&format!(r#"action="{url}/zhblloko""#)));
    assert_eq!(post_form(pool.clone(), &format!("{url}/zhblloko"), &[], Some(&besnik)).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Alo?", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_business_listing_reaches_whoever_posted_it(pool: PgPool) {
    let dritan = session_for(&pool, DRITAN_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let url = write(&pool, "/listing/1002/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    // The buyer writes to the business; whoever posted its listing answers for it.
    assert!(get_with_cookie(pool.clone(), &url, &buyer).await.body.contains("Auto Durrësi"));
    let inbox = get_with_cookie(pool.clone(), "/mesazhet", &dritan).await.body;
    assert!(inbox.contains("Përdorues") && inbox.contains("Mercedes-Benz"), "{inbox}");
    assert_eq!(write(&pool, &url, "Po, ejani nesër.", &dritan).await.status, StatusCode::SEE_OTHER);
}
