//! The notifications and the badges' counts (crates/api/src/notifications.rs): the list, a page at
//! a time in the page's order, each title said whole in the language asked for from what its
//! payload keeps, as /lajmerimet shows it, and leading where the page leads; one kept before its
//! payload had its title's values saying the title it kept, worded as the page words it; those the
//! app showed read, and the counts dropping with them, each as the pages' own JSON counts it; and
//! none of it without a token. The notifications come from the code that writes them, over the
//! sample listings (crates/web/tests/fixtures/listings.sql), or straight into the table.

mod common;

use axum::{
    body::Body,
    http::{StatusCode, header},
};
use pluribus_core::format::Contact;
use pluribus_db::{Granted, Invited, NewSearch};
use pluribus_services::sender::Senders;
use serde_json::{Value, json};
use sqlx::PgPool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use common::*;

/// The fixtures' people: Ana and Besnik, private sellers, and Dritan, who owns Auto Durrësi.
const ANA: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK: &str = "22222222-2222-4222-8222-222222222222";
const DRITAN: &str = "33333333-3333-4333-8333-333333333333";
const AUTO_DURRESI: &str = "44444444-4444-4444-8444-444444444444";
/// Besnik's laptop (1006).
const LAPTOP: &str = "a0000000-0000-4000-8000-000000001006";
/// Where the links of what goes out start.
const ORIGIN: &str = "https://pluribus.example";
/// What an invitation to post a store's listings offers, in Albanian and in English.
const EDITOR_SQ: &str = "Si „Poston njoftime“: Poston dhe ndryshon njoftimet e dyqanit, dhe u përgjigjet mesazheve.";
const EDITOR_EN: &str = "As “Posts listings”: Posts and edits the store’s listings, and answers messages.";

fn id(text: &str) -> Uuid {
    Uuid::parse_str(text).unwrap()
}

/// `GET /api/v1/notifications` with `query`, signed in with `token`, in `language` (as
/// `Accept-Language`) where one is given.
async fn notifications(pool: &PgPool, token: &str, query: &str, language: Option<&str>) -> ApiResponse {
    let mut request = request("GET", &format!("/api/v1/notifications{query}"), Some(token));
    if let Some(language) = language {
        request = request.header(header::ACCEPT_LANGUAGE, language);
    }
    send(api(pool.clone()), request.body(Body::empty()).unwrap()).await
}

/// A page's notifications, from an answer that must be one.
fn items(answer: &ApiResponse) -> Vec<Value> {
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    answer.json()["items"].as_array().unwrap().clone()
}

/// What each says: its title, and what it says under it.
fn said(items: &[Value]) -> Vec<(String, Option<String>)> {
    items.iter().map(|item| (item["title"].as_str().unwrap().to_string(), item["body"].as_str().map(str::to_string))).collect()
}

fn titles(items: &[Value]) -> Vec<String> {
    items.iter().map(|item| item["title"].as_str().unwrap().to_string()).collect()
}

/// `GET /api/v1/counts`, signed in with `token`: its JSON, which is never kept.
async fn counts(pool: &PgPool, token: &str) -> Value {
    let answer = get(pool.clone(), "/api/v1/counts", Some(token)).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
    assert_eq!(answer.header("cache-control"), Some("no-store"));
    answer.json()
}

/// A line of /lajmerimet, as the page shows it: where it leads, its title and what it says under
/// it, and whether it is unread.
#[derive(Debug)]
struct Line {
    href: Option<String>,
    title: String,
    body: Option<String>,
    unread: bool,
}

/// What Askama escapes, as it was.
fn unescape(html: &str) -> String {
    html.replace("&#60;", "<").replace("&#62;", ">").replace("&#34;", "\"").replace("&#39;", "'").replace("&#38;", "&")
}

/// A page of the site, for the user `token` signs in, by the same session's cookie, in English
/// with `english`.
async fn page(pool: &PgPool, path: &str, token: &str, english: bool) -> ApiResponse {
    let cookie = match english {
        true => format!("session={token}; gjuha=en"),
        false => format!("session={token}"),
    };
    send(api(pool.clone()), request("GET", path, None).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await
}

/// /lajmerimet's lines, in its order.
async fn page_lines(pool: &PgPool, token: &str, english: bool) -> Vec<Line> {
    let page = page(pool, "/lajmerimet", token, english).await;
    assert_eq!(page.status, StatusCode::OK);
    let after = |text: &str, start: &str| text.split_once(start).map(|(_, rest)| rest.to_string());
    let upto = |text: &str, end: &str| unescape(text.split_once(end).unwrap().0);
    page.body
        .split(r#"<li class="notification"#)
        .skip(1)
        .filter(|line| line.starts_with(r#"">"#) || line.starts_with(r#" is-unread">"#))
        .map(|line| {
            let title = after(line, r#"<span class="notification-title">"#).unwrap();
            // Unread, it says so to screen readers first.
            let hidden = title.strip_prefix(r#"<span class="visually-hidden">"#);
            let title = hidden.map_or(title.as_str(), |hidden| hidden.split_once("</span>").unwrap().1);
            Line {
                href: after(line, r#"<a class="notification-line" href=""#).map(|rest| upto(&rest, "\"")),
                title: upto(title, "</span>"),
                body: after(line, r#"<span class="notification-body">"#).map(|rest| upto(&rest, "</span>")),
                unread: line.starts_with(" is-unread"),
            }
        })
        .collect()
}

/// Holds the API's notifications to the page's lines, one for one, in their order: the same title
/// and body, read where the page shows them read, and leading where the page leads.
fn same_as_the_page(items: &[Value], lines: &[Line]) {
    assert_eq!(items.len(), lines.len(), "{items:#?}\n{lines:#?}");
    for (item, line) in items.iter().zip(lines) {
        assert_eq!(item["title"].as_str(), Some(line.title.as_str()), "{item}");
        assert_eq!(item["body"].as_str(), line.body.as_deref(), "{item}");
        assert_eq!(item["read"].as_bool(), Some(!line.unread), "{item}");
        let leads = match (item["listing_no"].as_i64(), item["conversation_id"].as_str(), item["search_id"].as_str()) {
            (Some(no), None, None) => Some(format!("/listing/{no}-")),
            (None, Some(id), None) => Some(format!("/mesazhet/{id}")),
            (None, None, Some(id)) => Some(format!("/kerkimet/{id}")),
            (None, None, None) => None,
            _ => panic!("leads to more than one thing: {item}"),
        };
        let href = line.href.as_deref();
        match (leads, item["kind"].as_str().unwrap()) {
            (Some(leads), _) => assert!(href.is_some_and(|href| href.starts_with(&leads)), "{item}: {href:?}"),
            // Where the app has nothing to open, the page leads to the saved searches, or to an
            // invitation (the stores are the site's alone for now).
            (None, "SAVED_SEARCH") => assert_eq!(href, Some("/kerkimet-e-ruajtura"), "{item}"),
            (None, "STORE_INVITE") => assert!(href.is_none_or(|href| href.starts_with("/ftesa/")), "{item}: {href:?}"),
            (None, _) => assert_eq!(href, None, "{item}"),
        }
    }
}

/// Moves what the message notices go by `minutes` into the past, as if that long went by.
async fn minutes_pass(pool: &PgPool, minutes: i32) {
    for (table, column) in [
        ("messages", "created_at"),
        ("notifications", "created_at"),
        ("conversation_participants", "last_read_at"),
        ("conversations", "last_message_at"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {column} = {column} - make_interval(mins => $1)"))
            .bind(minutes)
            .execute(pool)
            .await
            .unwrap();
    }
}

/// Besnik writes to Ana about her Golf (1001): the conversation's id.
async fn besnik_writes(pool: &PgPool) -> Uuid {
    let target = pluribus_db::message_target(pool, 1001, id(BESNIK)).await.unwrap().expect("her Golf");
    pluribus_db::start_conversation(pool, &target, id(BESNIK), "A është ende në shitje?").await.unwrap()
}

/// Ana keeps Besnik's laptop, whose price he lowers (as posting.rs does, in one transaction with
/// the change): a price drop for her. Its first price an hour old, so that the drop comes after it
/// whatever the clock does.
async fn laptop_drops(pool: &PgPool) {
    assert!(pluribus_db::set_saved(pool, id(ANA), 1006, true).await.unwrap());
    sqlx::query("UPDATE listing_price_history SET changed_at = changed_at - interval '1 hour' WHERE listing_id = $1")
        .bind(id(LAPTOP))
        .execute(pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE listings SET price_amount = 58000 WHERE id = $1").bind(id(LAPTOP)).execute(&mut *tx).await.unwrap();
    pluribus_db::notify_price_drop(&mut tx, id(LAPTOP)).await.unwrap();
    tx.commit().await.unwrap();
}

/// A search Ana keeps has two new listings: the search's id.
async fn search_alert(pool: &PgPool) -> Uuid {
    let search = NewSearch {
        name: "Telefona · Tiranë",
        category_id: None,
        words: None,
        path: "/c/telefona",
        query: "city=tirane",
    };
    let search = pluribus_db::save_search(pool, id(ANA), &search).await.unwrap().expect("kept");
    let url = format!("{ORIGIN}/kerkimet/{search}");
    pluribus_db::record_search_alert(pool, id(ANA), search, "2 njoftime të reja: Telefona · Tiranë", 2, &url).await.unwrap();
    search
}

/// A moderator features Ana's Golf for a week.
async fn golf_featured(pool: &PgPool) {
    let granted = pluribus_db::grant_promotion(pool, 1001, "FEATURED_7D", id(DRITAN)).await.unwrap();
    assert!(matches!(granted, Granted::Given(Some(_))));
}

/// Gives Ana a notification of every kind, each by the code that writes it, the oldest first:
/// Besnik's message about her Golf, left unread long enough to hear of it; a drop in the price of
/// his laptop, which she keeps; two new listings of a search she keeps; her sofa (1007) expired,
/// and her iPhone (1005) days from it; her Golf featured; and Auto Durrësi asking her in to post
/// its listings. The conversation's id and the search's.
async fn every_kind(pool: &PgPool) -> (Uuid, Uuid) {
    let conversation = besnik_writes(pool).await;
    minutes_pass(pool, 4).await;
    assert_eq!(pluribus_services::notices::send_due(pool, &Senders::none(), ORIGIN).await.unwrap(), 1);
    laptop_drops(pool).await;
    let search = search_alert(pool).await;
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = 1007").execute(pool).await.unwrap();
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = 1005").execute(pool).await.unwrap();
    assert_eq!(pluribus_services::expiry::run(pool, &Senders::none(), ORIGIN).await.unwrap(), (1, 1));
    golf_featured(pool).await;
    let (ana, title) = (Contact::Phone("+355690000001".to_string()), "Auto Durrësi të fton në dyqan");
    let invited = pluribus_db::invite(pool, id(AUTO_DURRESI), &ana, "EDITOR", id(DRITAN), title, EDITOR_SQ).await.unwrap();
    assert!(matches!(invited, Invited::Sent(_)));
    (conversation, search)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn every_kind_says_its_title_as_the_page_shows_it_in_either_language(pool: PgPool) {
    let token = token_for(&pool, ANA, "android").await;
    let (conversation, search) = every_kind(&pool).await;

    // Every kind, the latest first, all unread, each with when it came and what the app opens
    // from it: the listing, the search, the conversation.
    let albanian = notifications(&pool, &token, "", None).await;
    assert_eq!(albanian.json()["next"], Value::Null);
    let albanian = items(&albanian);
    let kinds: Vec<&str> = albanian.iter().map(|item| item["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["STORE_INVITE", "PROMOTED", "EXPIRING", "EXPIRED", "SAVED_SEARCH", "PRICE_DROP", "NEW_MESSAGE"]);
    let (conversation, search) = (conversation.to_string(), search.to_string());
    let leads: Vec<(Option<i64>, Option<&str>, Option<&str>)> = albanian
        .iter()
        .map(|item| (item["listing_no"].as_i64(), item["conversation_id"].as_str(), item["search_id"].as_str()))
        .collect();
    assert_eq!(
        leads,
        [
            (None, None, None),
            (Some(1001), None, None),
            (Some(1005), None, None),
            (Some(1007), None, None),
            (None, None, Some(search.as_str())),
            (Some(1006), None, None),
            (None, Some(conversation.as_str()), None),
        ]
    );
    for item in &albanian {
        assert_eq!(item["read"], false, "{item}");
        OffsetDateTime::parse(item["created_at"].as_str().unwrap(), &Rfc3339).unwrap();
    }

    // In Albanian, what each writer kept; in English, what an English page says.
    let sq = |title: &str, body: Option<&str>| (title.to_string(), body.map(str::to_string));
    assert_eq!(
        said(&albanian),
        [
            sq("Auto Durrësi të fton në dyqan", Some(EDITOR_SQ)),
            sq("I veçuar për 7 ditë: Volkswagen Golf 7, 2016, naftë", None),
            sq("Njoftimi skadon së shpejti: iPhone 13, 128 GB, si i ri", None),
            sq("Njoftimi skadoi: Divan qoshe ngjyrë gri", None),
            sq("2 njoftime të reja: Telefona · Tiranë", None),
            sq("Çmimi ra: Laptop Lenovo ThinkPad T14", None),
            sq("Besnik të shkroi për „Volkswagen Golf 7, 2016, naftë“", Some("A është ende në shitje?")),
        ]
    );
    let english = notifications(&pool, &token, "", Some("en")).await;
    assert!(english.vary().contains("Accept-Language"), "{}", english.vary());
    let english = items(&english);
    assert_eq!(
        said(&english),
        [
            sq("Auto Durrësi invites you to their store", Some(EDITOR_EN)),
            sq("Featured for 7 days: Volkswagen Golf 7, 2016, naftë", None),
            sq("Listing expires soon: iPhone 13, 128 GB, si i ri", None),
            sq("Listing expired: Divan qoshe ngjyrë gri", None),
            sq("2 new listings: Telefona · Tiranë", None),
            sq("Price drop: Laptop Lenovo ThinkPad T14", None),
            // What Besnik wrote, as he wrote it.
            sq("Besnik wrote to you about “Volkswagen Golf 7, 2016, naftë”", Some("A është ende në shitje?")),
        ]
    );
    // As the page shows them to her, in either language, leading where it leads.
    same_as_the_page(&albanian, &page_lines(&pool, &token, false).await);
    same_as_the_page(&english, &page_lines(&pool, &token, true).await);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn one_kept_before_its_payload_had_its_values_says_the_title_it_kept(pool: PgPool) {
    let token = token_for(&pool, ANA, "ios").await;
    // As they were kept before their payloads had their titles' values, one by each text the page
    // words them anew by: the payload empty, or with what the notification leads to alone.
    sqlx::query(
        "INSERT INTO notifications (user_id, notification_type, title, body, payload, created_at) VALUES
         ($1::uuid, 'PRICE_DROP', 'Çmimi ra: Laptop Lenovo ThinkPad T14', NULL,
          jsonb_build_object('listing_id', $2::uuid, 'was', 62000, 'price', 58000, 'currency', 'ALL'),
          now() - interval '9 minutes'),
         ($1::uuid, 'EXPIRING', 'Njoftimi skadon së shpejti: iPhone 13, 128 GB, si i ri', NULL, '{}', now() - interval '8 minutes'),
         ($1::uuid, 'EXPIRED', 'Njoftimi skadoi: Divan qoshe ngjyrë gri', NULL, '{}', now() - interval '7 minutes'),
         ($1::uuid, 'NEW_MESSAGE', 'Përdorues të shkroi për „Volkswagen Golf 7“', 'A është ende në shitje?', '{}',
          now() - interval '6 minutes'),
         ($1::uuid, 'NEW_MESSAGE', 'Auto Elbasani të shkroi në Pluribus', 'Po, është.', '{}', now() - interval '5 minutes'),
         ($1::uuid, 'SAVED_SEARCH', '3 njoftime të reja: Makina në Tiranë', NULL, '{}', now() - interval '4 minutes'),
         ($1::uuid, 'SAVED_SEARCH', '1 njoftim i ri: Makina në Durrës', NULL, '{}', now() - interval '3 minutes'),
         ($1::uuid, 'PROMOTED', 'I veçuar për 7 ditë: Volkswagen Golf 7', NULL, '{}', now() - interval '2 minutes'),
         ($1::uuid, 'STORE_INVITE', 'Auto Elbasani të fton në dyqan', $3, '{}', now() - interval '1 minute')",
    )
    .bind(ANA)
    .bind(LAPTOP)
    .bind(EDITOR_SQ)
    .execute(&pool)
    .await
    .unwrap();

    // Their kept titles, in English as the page words them, the names the site gives people and
    // its own words among them too.
    let albanian = items(&notifications(&pool, &token, "", None).await);
    let english = items(&notifications(&pool, &token, "", Some("en")).await);
    let sq = |title: &str, body: Option<&str>| (title.to_string(), body.map(str::to_string));
    assert_eq!(
        said(&albanian),
        [
            sq("Auto Elbasani të fton në dyqan", Some(EDITOR_SQ)),
            sq("I veçuar për 7 ditë: Volkswagen Golf 7", None),
            sq("1 njoftim i ri: Makina në Durrës", None),
            sq("3 njoftime të reja: Makina në Tiranë", None),
            sq("Auto Elbasani të shkroi në Pluribus", Some("Po, është.")),
            sq("Përdorues të shkroi për „Volkswagen Golf 7“", Some("A është ende në shitje?")),
            sq("Njoftimi skadoi: Divan qoshe ngjyrë gri", None),
            sq("Njoftimi skadon së shpejti: iPhone 13, 128 GB, si i ri", None),
            sq("Çmimi ra: Laptop Lenovo ThinkPad T14", None),
        ]
    );
    assert_eq!(
        said(&english),
        [
            sq("Auto Elbasani invites you to their store", Some(EDITOR_EN)),
            sq("Featured for 7 days: Volkswagen Golf 7", None),
            sq("1 new listing: Makina në Durrës", None),
            sq("3 new listings: Makina në Tiranë", None),
            // What people wrote, as they wrote it.
            sq("Auto Elbasani wrote to you on Pluribus", Some("Po, është.")),
            sq("User wrote to you about “Volkswagen Golf 7”", Some("A është ende në shitje?")),
            sq("Listing expired: Divan qoshe ngjyrë gri", None),
            sq("Listing expires soon: iPhone 13, 128 GB, si i ri", None),
            sq("Price drop: Laptop Lenovo ThinkPad T14", None),
        ]
    );
    // What one leads to it still does, by its payload: the laptop.
    let leads: Vec<Option<i64>> = albanian.iter().map(|item| item["listing_no"].as_i64()).collect();
    assert_eq!(leads, [None, None, None, None, None, None, None, None, Some(1006)]);
    same_as_the_page(&albanian, &page_lines(&pool, &token, false).await);
    same_as_the_page(&english, &page_lines(&pool, &token, true).await);

    // Said from its payload, a title is whole, where the one kept beside it, which the page shows,
    // is its first 180 characters.
    let words = "Laptop Lenovo ThinkPad T14 me 16 GB RAM, 512 GB SSD dhe ekran 14 inç, i ri në kuti ";
    let long: String = words.repeat(3).chars().take(175).collect();
    sqlx::query(
        "INSERT INTO notifications (user_id, notification_type, title, payload)
         VALUES ($1::uuid, 'PRICE_DROP', left('Çmimi ra: ' || $2, 180),
                 jsonb_build_object('listing_id', $3::uuid, 'public_no', 1006, 'title', $2::text,
                                    'was', 62000, 'price', 58000, 'currency', 'ALL'))",
    )
    .bind(ANA)
    .bind(&long)
    .bind(LAPTOP)
    .execute(&pool)
    .await
    .unwrap();
    let cut: String = long.chars().take(170).collect();
    for (language, english, says) in [(None, false, "Çmimi ra: "), (Some("en"), true, "Price drop: ")] {
        let listed = items(&notifications(&pool, &token, "", language).await);
        let lines = page_lines(&pool, &token, english).await;
        assert_eq!(listed[0]["title"].as_str(), Some(format!("{says}{long}").as_str()));
        assert_eq!(lines[0].title, format!("{says}{cut}"));
        same_as_the_page(&listed[1..], &lines[1..]);
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_list_goes_on_a_page_at_a_time_after_the_last_it_gave(pool: PgPool) {
    let user = new_user(&pool, "+355690000001", "Ana").await;
    let token = token_for(&pool, &user, "android").await;
    // 105 price drops, one a minute, each id sorting as it was made, as a UUIDv7 does; and one
    // older than the 90 days the list goes back.
    sqlx::query(
        "INSERT INTO notifications (id, user_id, notification_type, title, payload, created_at)
         SELECT format('0192f0c1-%s-7000-8000-000000000000', lpad(to_hex(1000 - i), 4, '0'))::uuid, $1::uuid, 'PRICE_DROP',
                'Çmimi ra: Njoftimi ' || i, jsonb_build_object('title', 'Njoftimi ' || i), now() - make_interval(mins => i)
         FROM generate_series(1, 105) i
         UNION ALL
         SELECT '0192f0c1-0000-7000-8000-000000000000'::uuid, $1::uuid, 'PRICE_DROP', 'Çmimi ra: I vjetër',
                '{\"title\": \"I vjetër\"}', now() - interval '91 days'",
    )
    .bind(&user)
    .execute(&pool)
    .await
    .unwrap();
    let numbered =
        |numbers: std::ops::RangeInclusive<i32>| numbers.map(|n| format!("Çmimi ra: Njoftimi {n}")).collect::<Vec<_>>();

    // The first page: as many as the page shows, the latest first, as it shows them; and where
    // the next starts, its last.
    let first = notifications(&pool, &token, "", None).await;
    let shown = items(&first);
    assert_eq!(titles(&shown), numbered(1..=100));
    let next = first.json()["next"].as_str().expect("a next page").to_string();
    assert_eq!(shown[99]["id"].as_str(), Some(next.as_str()));
    let lines = page_lines(&pool, &token, false).await;
    assert_eq!(lines.iter().map(|line| line.title.clone()).collect::<Vec<_>>(), titles(&shown));

    // After it, the rest, and no more: none twice, none missed, none older than 90 days.
    let second = notifications(&pool, &token, &format!("?after={next}"), None).await;
    assert_eq!(second.json()["next"], Value::Null);
    assert_eq!(titles(&items(&second)), numbered(101..=105));
    // Gone since (an unseen price drop replaced by a newer one), the last still marks the place.
    sqlx::query("DELETE FROM notifications WHERE id = $1::uuid").bind(&next).execute(&pool).await.unwrap();
    assert_eq!(titles(&items(&notifications(&pool, &token, &format!("?after={next}"), None).await)), numbered(101..=105));
    // In the language asked for, by the query too.
    let english = items(&notifications(&pool, &token, &format!("?after={next}&lang=en"), None).await);
    assert_eq!(english[0]["title"], "Price drop: Njoftimi 101");

    // An `after` that isn't a notification's id: the envelope, on `after`.
    for after in ["soon", "1001", "0192f0c1-03e7"] {
        let wrong = notifications(&pool, &token, &format!("?after={after}"), None).await;
        let envelope = json!({"error": {
            "code": "validation",
            "message": "Disa fusha nuk janë në rregull.",
            "fields": {"after": "Vazhdimi i listës nuk lexohet. Fillo përsëri nga faqja e parë."},
        }});
        assert_eq!((wrong.status, wrong.json()), (StatusCode::UNPROCESSABLE_ENTITY, envelope), "{after}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn what_the_app_showed_is_read_and_the_counts_drop_with_it(pool: PgPool) {
    // Without a token, none of it.
    let unauthorized = json!({"error": {"code": "unauthorized", "message": "Hyr në llogari për të vazhduar."}});
    for (method, path) in [("GET", "/api/v1/notifications"), ("POST", "/api/v1/notifications/seen"), ("GET", "/api/v1/counts")] {
        let body = Body::from(r#"{"up_to": "0192f0c1-0000-7000-8000-000000000000"}"#);
        let asked = request(method, path, None).header(header::CONTENT_TYPE, "application/json").body(body).unwrap();
        let answer = send(api(pool.clone()), asked).await;
        assert_eq!((answer.status, answer.json()), (StatusCode::UNAUTHORIZED, unauthorized.clone()), "{method} {path}");
    }

    let token = token_for(&pool, ANA, "android").await;
    let nothing = json!({"messages": 0, "notifications": 0, "own": 0, "price_drops": 0, "searches": 0});
    assert_eq!(counts(&pool, &token).await, nothing);
    // Besnik writes to her, which she leaves unread; the price of his laptop, which she keeps,
    // drops; a search she keeps has new listings; and her iPhone is days from expiring, one about
    // her own listings.
    besnik_writes(&pool).await;
    laptop_drops(&pool).await;
    search_alert(&pool).await;
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = 1005").execute(&pool).await.unwrap();
    assert_eq!(pluribus_services::expiry::run(&pool, &Senders::none(), ORIGIN).await.unwrap(), (1, 0));
    assert_eq!(counts(&pool, &token).await, json!({"messages": 1, "notifications": 3, "own": 1, "price_drops": 1, "searches": 1}));

    // The app shows them; one comes after it did, her Golf featured, another about her own.
    let shown = items(&notifications(&pool, &token, "", None).await);
    assert!(shown.iter().all(|item| item["read"] == false), "{shown:#?}");
    let latest = shown[0]["id"].as_str().unwrap().to_string();
    golf_featured(&pool).await;
    assert_eq!(counts(&pool, &token).await, json!({"messages": 1, "notifications": 4, "own": 2, "price_drops": 1, "searches": 1}));

    // It says it showed them, up to the latest: those are read, and the price drop and the
    // search's alert with them; not the one it didn't show, nor the conversation, which she reads
    // in it.
    let seen = post_json(pool.clone(), "/api/v1/notifications/seen", Some(&token), &json!({"up_to": latest}).to_string()).await;
    assert_eq!((seen.status, seen.body.as_str()), (StatusCode::NO_CONTENT, ""));
    assert_eq!(counts(&pool, &token).await, json!({"messages": 1, "notifications": 1, "own": 1, "price_drops": 0, "searches": 0}));
    let read: Vec<(String, bool)> = items(&notifications(&pool, &token, "", None).await)
        .iter()
        .map(|item| (item["kind"].as_str().unwrap().to_string(), item["read"].as_bool().unwrap()))
        .collect();
    let read: Vec<(&str, bool)> = read.iter().map(|(kind, read)| (kind.as_str(), *read)).collect();
    assert_eq!(read, [("PROMOTED", false), ("EXPIRING", true), ("SAVED_SEARCH", true), ("PRICE_DROP", true)]);

    // The pages count the same, each by its own JSON: the account button, the heart, the
    // messages button.
    let json_of = |answer: ApiResponse| serde_json::from_str::<Value>(&answer.body).unwrap();
    assert_eq!(json_of(page(&pool, "/lajmerimet/numri", &token, false).await), json!({"unread": 1, "own": 1}));
    let heart = json_of(page(&pool, "/te-ruajturat/numrat", &token, false).await);
    assert_eq!((heart["price_drops"].as_i64(), heart["searches"].as_i64()), (Some(0), Some(0)));
    assert_eq!(json_of(page(&pool, "/mesazhet/numri", &token, false).await), json!({"unread": 1}));

    // A body that names no notification: the envelope, on `body`.
    let wrong = post_json(pool.clone(), "/api/v1/notifications/seen", Some(&token), r#"{"up_to": "soon"}"#).await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(wrong.json()["error"]["fields"]["body"].is_string(), "{}", wrong.body);
}
