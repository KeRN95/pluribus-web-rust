//! Listings expire LISTING_DAYS after they are published, renewed or put back up (db/posting.rs),
//! so that the site doesn't fill with things long gone. Every CHECK_EVERY the server takes off the
//! site those whose time is up (EXPIRED), and tells their sellers WARN_DAYS before and when it
//! happens: a notification each (EXPIRING, EXPIRED), and an e-mail where the seller signs in with
//! an address, with the way to keep it ("Rifresko", on "Njoftimet e mia" and the listing's own
//! page, keeps it up LISTING_DAYS more, at the top of "newest first" again once a week at most:
//! db::renew_listing).

use std::time::Duration;

use sqlx::PgPool;

use crate::{
    db::{self, ExpiryNotice, LISTING_DAYS},
    format::{self, Contact},
    sender,
};

/// How many days before a listing expires its seller hears of it.
pub const WARN_DAYS: i32 = 3;
/// How often the server looks for listings to expire or warn of.
pub const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);

/// Expires the listings whose time is up and warns of those WARN_DAYS from it; returns how many of
/// each (warned, expired). The e-mails' links start with `origin` (web::site_origin).
pub async fn run(db: &PgPool, origin: &str) -> sqlx::Result<(usize, usize)> {
    // Expired first, so that one whose time is up isn't warned of as well.
    let expired = db::expire_listings(db).await?;
    for listing in &expired {
        let subject = format!("Njoftimi yt skadoi: {}", listing.title);
        let body = format!(
            "Njoftimi yt „{}“ skadoi dhe nuk shfaqet më në Pluribus.\n\n\
             Për ta kthyer në faqe për {LISTING_DAYS} ditë të tjera, rifreskoje te „Njoftimet e mia“: {origin}/njoftimet-e-mia\n",
            listing.title
        );
        tell(listing, &subject, &body).await;
    }
    let warned = db::warn_expiring(db, WARN_DAYS).await?;
    for listing in &warned {
        let date = format::date(listing.expires_at);
        let subject = format!("Njoftimi yt skadon më {date}: {}", listing.title);
        let body = format!(
            "Njoftimi yt „{}“ në Pluribus skadon më {date}, dhe pastaj nuk shfaqet më.\n\n\
             Për ta mbajtur {LISTING_DAYS} ditë të tjera, rifreskoje te „Njoftimet e mia“: {origin}/njoftimet-e-mia\n",
            listing.title
        );
        tell(listing, &subject, &body).await;
    }
    Ok((warned.len(), expired.len()))
}

/// An e-mail to the listing's seller, where they sign in with an address and allow e-mail; else
/// the log has it (sender.rs), as for a phone number, which has no WhatsApp template for these.
async fn tell(listing: &ExpiryNotice, subject: &str, body: &str) {
    let contact = match (&listing.email, &listing.phone) {
        (Some(email), _) if listing.email_ok => Contact::Email(email.clone()),
        (_, Some(phone)) => Contact::Phone(phone.clone()),
        _ => return,
    };
    match sender::senders().send_text(&contact, subject, body).await {
        Ok(channel) => tracing::debug!(?channel, listing = listing.public_no, "expiry notice sent"),
        Err(error) => tracing::warn!(%error, listing = listing.public_no, "expiry notice not sent"),
    }
}

/// Looks for listings to expire or warn of every CHECK_EVERY, from now on, in the background.
pub fn start(db: PgPool) {
    tokio::spawn(async move {
        let origin = crate::web::site_origin();
        let mut every = tokio::time::interval(CHECK_EVERY);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            if let Err(error) = run(&db, &origin).await {
                tracing::warn!(%error, "could not expire the listings whose time is up");
            }
        }
    });
}
