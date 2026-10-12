//! Listings expire LISTING_DAYS after they are published, renewed or put back up (db/posting.rs),
//! so that the site doesn't fill with things long gone. Every CHECK_EVERY the server (one at a
//! time however many share the database: jobs.rs, which runs `tick`) takes off the site those
//! whose time is up (EXPIRED), and tells their sellers WARN_DAYS before and when it happens: a
//! notification each (EXPIRING, EXPIRED), and an e-mail where the seller signs in with
//! an address, or a WhatsApp message by its template where they sign in with a number
//! (WHATSAPP_EXPIRY_TEMPLATE), with the way to keep it ("Rifresko", on "Njoftimet e mia" and the listing's own
//! page, keeps it up LISTING_DAYS more, at the top of "newest first" again once a week at most:
//! db::renew_listing). The e-mail is in the language kept on the seller's account
//! (users.preferred_language); the notifications' titles, kept in Albanian, web/notifications.rs
//! words for whoever reads them, and a WhatsApp template's texts stay Albanian. Each look forgets
//! too, with their files, the listings posted signed out that their code never put up, and the
//! photos uploaded before their listing that none took within a day (`tick`).

use std::time::Duration;

use pluribus_core::{
    domain::NotificationKind,
    format::{self, Contact},
    i18n::{self, Lang, t1, t2},
};
use pluribus_db::{self as db, ExpiryNotice, LISTING_DAYS};
use sqlx::PgPool;

use crate::{
    media::Media,
    sender::{Alert, AlertTemplate, Senders},
};

/// How many days before a listing expires its seller hears of it.
pub const WARN_DAYS: i32 = 3;
/// How often the server looks for listings to expire or warn of.
pub const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);

/// Expires the listings whose time is up and warns of those WARN_DAYS from it, their sellers told
/// by `senders`; returns how many of each (warned, expired). The e-mails' links start with
/// `origin` (`Config::site_origin`).
pub async fn run(db: &PgPool, senders: &Senders, origin: &str) -> sqlx::Result<(usize, usize)> {
    // Expired first, so that one whose time is up isn't warned of as well.
    let expired = db::expire_listings(db).await?;
    for listing in &expired {
        let unsubscribe = unsubscribe_link(db, origin, listing).await?;
        let (subject, body) = in_their_language(listing, || {
            let subject = t1("Njoftimi yt skadoi: {}", &listing.title);
            let body = format!(
                "{}\n\n{}\n{}",
                t1("Njoftimi yt „{}“ skadoi dhe nuk shfaqet më në Pluribus.", &listing.title),
                t2("Për ta kthyer në faqe për {} ditë të tjera, rifreskoje te „Njoftimet e mia“: {}", LISTING_DAYS, format!("{origin}/njoftimet-e-mia")),
                no_more(&unsubscribe),
            );
            (subject, body)
        });
        let whatsapp = AlertTemplate::Expiry { title: listing.title.clone(), what: "skadoi".into() };
        tell(senders, listing, Alert { subject, body, whatsapp, unsubscribe }).await;
        crate::push::tell_expiry(db, senders, NotificationKind::Expired, listing).await;
    }
    let warned = db::warn_expiring(db, WARN_DAYS).await?;
    for listing in &warned {
        let unsubscribe = unsubscribe_link(db, origin, listing).await?;
        let (subject, body) = in_their_language(listing, || {
            let date = format::date(listing.expires_at);
            let subject = t2("Njoftimi yt skadon më {}: {}", &date, &listing.title);
            let body = format!(
                "{}\n\n{}\n{}",
                t2("Njoftimi yt „{}“ në Pluribus skadon më {}, dhe pastaj nuk shfaqet më.", &listing.title, &date),
                t2("Për ta mbajtur {} ditë të tjera, rifreskoje te „Njoftimet e mia“: {}", LISTING_DAYS, format!("{origin}/njoftimet-e-mia")),
                no_more(&unsubscribe),
            );
            (subject, body)
        });
        let date = format::date(listing.expires_at);
        let whatsapp = AlertTemplate::Expiry { title: listing.title.clone(), what: format!("skadon më {date}") };
        tell(senders, listing, Alert { subject, body, whatsapp, unsubscribe }).await;
        crate::push::tell_expiry(db, senders, NotificationKind::Expiring, listing).await;
    }
    Ok((warned.len(), expired.len()))
}

/// For an e-mail, the link that stops every notice by e-mail (web/unsubscribe.rs): these have no
/// box of their own on "Cilësimet".
async fn unsubscribe_link(db: &PgPool, origin: &str, listing: &ExpiryNotice) -> sqlx::Result<Option<String>> {
    if listing.email.is_none() || !listing.email_ok {
        return Ok(None);
    }
    Ok(Some(format!("{origin}/ndalo/{}/email", db::notice_token(db, listing.user_id).await?)))
}

/// The e-mail's last line, with the link that stops them: none without one.
fn no_more(unsubscribe: &Option<String>) -> String {
    unsubscribe
        .as_ref()
        .map(|link| format!("\n{}\n", t1("Për të mos marrë më lajmërime me email nga Pluribus, me një klik: {}", link)))
        .unwrap_or_default()
}

/// What `say` says, in the language kept on the seller's account; the WhatsApp template's texts,
/// outside it, stay Albanian.
fn in_their_language<R>(listing: &ExpiryNotice, say: impl FnOnce() -> R) -> R {
    i18n::LANG.sync_scope(Lang::from_code(&listing.language).unwrap_or_default(), say)
}

/// The alert to the listing's seller: by e-mail where they sign in with an address and allow
/// e-mail, by WhatsApp where they sign in with a number and its template is set up; else the log
/// has it (sender.rs).
async fn tell(senders: &Senders, listing: &ExpiryNotice, alert: Alert) {
    let contact = match (&listing.email, &listing.phone) {
        (Some(email), _) if listing.email_ok => Contact::Email(email.clone()),
        (_, Some(phone)) => Contact::Phone(phone.clone()),
        _ => return,
    };
    match senders.send_alert(&contact, &alert).await {
        Ok(channel) => tracing::debug!(?channel, listing = listing.public_no, "expiry notice sent"),
        Err(error) => tracing::warn!(%error, listing = listing.public_no, "expiry notice not sent"),
    }
}

/// One look for listings to expire or warn of (`run`), their sellers told by `senders` with links
/// that start with `origin` (the state's, `Config::site_origin`); for listings posted without
/// signing in that their code never put up, which go with their photos in `media`
/// (db::forget_drafts); and for photos the phone app uploaded before their listing (C9) that no
/// listing took within photos::UPLOADS_KEPT, a day, which go with their files (db::forget_uploads,
/// here since D2b). Each failure in the log, and the others done all the same. What the jobs run
/// every CHECK_EVERY, as the server starts and from then on, in the process that holds the
/// expiry's lock (jobs.rs, `start_all`; `start` here, from `pluribus_web::state`, until D2).
pub async fn tick(db: &PgPool, media: &Media, senders: &Senders, origin: &str) {
    if let Err(error) = run(db, senders, origin).await {
        tracing::warn!(%error, "could not expire the listings whose time is up");
    }
    match db::forget_drafts(db).await {
        Ok(files) => crate::media::delete_photos(media, &files).await,
        Err(error) => tracing::warn!(%error, "could not forget the listings their code never put up"),
    }
    match db::forget_uploads(db, crate::photos::UPLOADS_KEPT).await {
        Ok(files) => crate::media::delete_photos(media, &files).await,
        Err(error) => tracing::warn!(%error, "could not forget the photos no listing took"),
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    /// Ana, one of the fixtures' people (crates/web/tests/fixtures/listings.sql).
    const ANA: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);

    /// A photo uploaded before its listing that no listing took within photos::UPLOADS_KEPT goes at
    /// the next look, every file of it with it; one uploaded since stays, its files too.
    #[sqlx::test(migrator = "pluribus_db::MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
    async fn a_look_forgets_the_uploads_no_listing_took(db: PgPool) {
        let media = Media::in_memory();
        let files = ["photos/old.webp", "photos/old-card.webp", "photos/old.jpg", "photos/new.webp", "photos/new-card.webp"];
        for key in files {
            media.put(key, b"webp".to_vec()).await.unwrap();
        }
        // One uploaded a minute longer ago than uploads are kept, the other a minute less.
        sqlx::query(
            "INSERT INTO photo_uploads (user_id, storage_key, created_at)
             VALUES ($1, 'photos/old.webp', now() - make_interval(secs => $2 + 60)),
                    ($1, 'photos/new.webp', now() - make_interval(secs => $2 - 60))",
        )
        .bind(ANA)
        .bind(crate::photos::UPLOADS_KEPT.as_secs_f64())
        .execute(&db)
        .await
        .unwrap();

        tick(&db, &media, &Senders::none(), "https://pluribus.example").await;
        let left: Vec<String> = sqlx::query_scalar("SELECT storage_key FROM photo_uploads").fetch_all(&db).await.unwrap();
        assert_eq!(left, ["photos/new.webp"]);
        for key in files {
            let kept = media.get(key).await.unwrap().is_some();
            assert_eq!(kept, key.starts_with("photos/new"), "{key}");
        }
    }
}
