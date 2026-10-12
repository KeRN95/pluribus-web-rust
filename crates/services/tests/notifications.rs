//! What a notification says, across the crates that write one: each writer keeps in its payload
//! what its title says (pluribus-db's writers, and the notices of unread messages, this crate's
//! notices.rs), and a message notice's title is its e-mail's subject (sender.rs); domain::notices
//! says both from the payload. These were pluribus-core's domain/notices.rs's and pluribus-db's
//! notifications.rs's tests until S2: this crate's tests are the ones that see the sender, the
//! notices and the database's writers together.

use pluribus_core::{
    domain::{
        NotificationKind, Role,
        notices::{self, title},
    },
    format::Contact,
};
use pluribus_db::{
    Granted, Invited, NewSearch, expire_listings, grant_promotion, invite, notifications, notify_price_drop,
    record_search_alert, save_search, warn_expiring,
};
use pluribus_services::sender::{Notice, Senders};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// A message notice's title is its e-mail's subject (sender.rs), which notices.rs keeps.
#[test]
fn a_message_notices_title_is_the_one_its_email_has() {
    for listing in [Some("Volkswagen Golf 7, 2016, naftë"), None] {
        let notice = Notice {
            writer: "Besa".to_string(),
            listing: listing.map(str::to_string),
            excerpt: "A është ende në shitje?".to_string(),
            unread: 1,
            url: "https://pluribus.example/mesazhet/0192f0c1-0000-7000-8000-00000000000c".to_string(),
            unsubscribe: None,
        };
        let payload = json!({"conversation": "0192f0c1-0000-7000-8000-00000000000c", "writer": "Besa", "listing": listing});
        assert_eq!(title(NotificationKind::NewMessage, &payload), notice.title());
    }
}

/// The fixtures' people (crates/web/tests/fixtures/listings.sql): Ana and Besnik, and Dritan, who
/// owns Auto Durrësi.
const ANA: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);
const BESNIK: Uuid = Uuid::from_u128(0x2222_2222_2222_4222_8222_2222_2222_2222);
const DRITAN: Uuid = Uuid::from_u128(0x3333_3333_3333_4333_8333_3333_3333_3333);
const AUTO_DURRESI: Uuid = Uuid::from_u128(0x4444_4444_4444_4444_8444_4444_4444_4444);

/// Each writer keeps in a notification's payload what its title says: said from the payload
/// (domain::notices), the title is the one it kept, whole (the column keeps its first 180
/// characters), and an invitation's body too; read back as /lajmerimet reads them.
#[sqlx::test(migrator = "pluribus_db::MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
async fn each_writer_keeps_what_its_notifications_title_says(pool: PgPool) -> sqlx::Result<()> {
    let golf: Uuid = sqlx::query_scalar("SELECT id FROM listings WHERE public_no = 1001").fetch_one(&pool).await?;
    // Besnik keeps Ana's Golf, whose price she lowers (posting.rs, update_listing): a price
    // drop for him.
    sqlx::query("INSERT INTO saved_listings (user_id, listing_id) VALUES ($1, $2)").bind(BESNIK).bind(golf).execute(&pool).await?;
    sqlx::query("UPDATE listing_price_history SET changed_at = changed_at - interval '1 hour' WHERE listing_id = $1")
        .bind(golf)
        .execute(&pool)
        .await?;
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE listings SET price_amount = 9000 WHERE id = $1").bind(golf).execute(&mut *tx).await?;
    notify_price_drop(&mut tx, golf).await?;
    tx.commit().await?;

    // He wrote to her about it ten minutes ago, and she hasn't read it: a notice (notices.rs).
    let conversation: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (listing_id, buyer_user_id, seller_user_id, last_message_at)
         VALUES ($1, $2, $3, now() - interval '10 minutes') RETURNING id",
    )
    .bind(golf)
    .bind(BESNIK)
    .bind(ANA)
    .fetch_one(&pool)
    .await?;
    sqlx::query("INSERT INTO conversation_participants (conversation_id, user_id) VALUES ($1, $2), ($1, $3)")
        .bind(conversation)
        .bind(BESNIK)
        .bind(ANA)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO messages (conversation_id, sender_user_id, body, created_at)
         VALUES ($1, $2, 'A është ende në shitje?', now() - interval '10 minutes')",
    )
    .bind(conversation)
    .bind(BESNIK)
    .execute(&pool)
    .await?;
    assert_eq!(pluribus_services::notices::send_due(&pool, &Senders::none(), "https://pluribus.example").await?, 1);

    // Her sofa's time is up, and her iPhone's two days off (expiry.rs).
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = 1007").execute(&pool).await?;
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = 1005").execute(&pool).await?;
    assert_eq!(expire_listings(&pool).await?.len(), 1);
    assert_eq!(warn_expiring(&pool, 3).await?.len(), 1);

    // A moderator features her bicycle (web/moderation.rs).
    assert!(matches!(grant_promotion(&pool, 1010, "FEATURED_7D", BESNIK).await?, Granted::Given(Some(_))));

    // His saved search has new listings: its alert, with the title web/searches.rs gives it.
    let name = "Makina në Tiranë";
    let search = NewSearch { name, category_id: None, words: None, path: "/c/makina", query: "city=tirane" };
    let search = save_search(&pool, BESNIK, &search).await?.expect("saved");
    let title = format!("{}: {name}", notices::new_listings(3));
    record_search_alert(&pool, BESNIK, search, &title, 3, &format!("https://pluribus.example/kerkimet/{search}")).await?;

    // Dritan's store asks him in to post its listings, in the words web/stores.rs gives it.
    let role = Role::Editor;
    let body = format!("Si „{}“: {}", role.label(), role.does());
    let contact = Contact::Phone("+355690000002".to_string());
    let invited = invite(&pool, AUTO_DURRESI, &contact, role.as_str(), DRITAN, "Auto Durrësi të fton në dyqan", &body).await?;
    assert!(matches!(invited, Invited::Sent(_)));

    let mut kinds = Vec::new();
    for user in [ANA, BESNIK] {
        for notification in notifications(&pool, user).await? {
            let kind = NotificationKind::parse(&notification.kind).expect("a kind the code writes");
            let said: String = notices::title(kind, &notification.payload).chars().take(180).collect();
            assert_eq!(said, notification.title, "{kind}: {}", notification.payload.0);
            let body = notices::body(kind, &notification.payload);
            match kind {
                NotificationKind::StoreInvite => assert_eq!(body, notification.body),
                _ => assert_eq!(body, None, "{kind}"),
            }
            kinds.push(kind);
        }
    }
    for kind in NotificationKind::ALL {
        assert_eq!(kinds.iter().filter(|written| **written == kind).count(), 1, "{kind}: {kinds:?}");
    }
    Ok(())
}
