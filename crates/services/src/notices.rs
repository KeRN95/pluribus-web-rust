//! Notices of messages someone hasn't read, so that a seller hears a buyer wrote with the site
//! closed. Every minute (CHECK_EVERY) the server, one at a time however many share the database
//! (jobs.rs, which runs `tick`), looks for messages from the other side of a conversation that
//! their reader hasn't read NOTICE_AFTER after they came (db::due_notices), and tells them by
//! e-mail, or by WhatsApp with its notice template (sender.rs), about the latest with a link to
//! answer it: at most once per conversation in NOTICE_GAP, and only of messages
//! newer than the last notice about it. Someone who turned them off in "Cilësimet"
//! (notification_preferences.message_alerts) hears nothing, nor by e-mail someone who turned
//! e-mail off. Each notice is a notification (NEW_MESSAGE), recorded before it goes out, so that a
//! channel that fails doesn't make it go again every minute; without a channel set up, it goes to
//! the log, as sign-in codes do. The e-mail is in the language kept on the reader's account
//! (users.preferred_language); the notification's title is kept in Albanian, which
//! web/notifications.rs words for whoever reads it, and its payload has who wrote and about which
//! listing, from which domain::notices says it in either language.

use std::time::Duration;

use pluribus_core::{
    format::Contact,
    i18n::{self, Lang},
};
use pluribus_db as db;
use sqlx::PgPool;

use crate::sender::{Notice, Senders};

/// How long after a message came, unread, its reader hears of it: long enough that someone with
/// the conversation open (which reads what it shows) or about to answer hears nothing.
pub const NOTICE_AFTER: Duration = Duration::from_secs(3 * 60);
/// At most one notice per conversation in this long, however many messages come.
pub const NOTICE_GAP: Duration = Duration::from_secs(60 * 60);
/// How often the server looks for notices due.
pub const CHECK_EVERY: Duration = Duration::from_secs(60);
/// How much of the latest message a notice quotes, in characters.
const EXCERPT: usize = 300;
/// The notification's title is at most this long (notifications.title).
const TITLE: usize = 180;

/// Sends the notices due now by `senders`; returns how many went out. Their links start with
/// `origin`, the site's address (`Config::site_origin`).
pub async fn send_due(db: &PgPool, senders: &Senders, origin: &str) -> sqlx::Result<usize> {
    let due = db::due_notices(db, NOTICE_AFTER, NOTICE_GAP).await?;
    let mut sent = 0;
    for due in due {
        // An e-mail carries the link that stops these (web/unsubscribe.rs).
        let unsubscribe = match (&due.email, due.email_ok) {
            (Some(_), true) => Some(format!("{origin}/ndalo/{}/mesazhet", db::notice_token(db, due.user_id).await?)),
            _ => None,
        };
        let notice = Notice {
            writer: due.writer,
            listing: due.listing,
            excerpt: excerpt(&due.body),
            unread: due.unread,
            url: format!("{origin}/mesazhet/{}", due.conversation_id),
            unsubscribe,
        };
        let title: String = i18n::LANG.sync_scope(Lang::Sq, || notice.title()).chars().take(TITLE).collect();
        db::record_notice(db, due.user_id, due.conversation_id, &title, &notice.excerpt, &notice.writer, notice.listing.as_deref()).await?;
        sent += 1;
        // Their phones hear of it by push too, whichever channel reaches them (push.rs).
        crate::push::tell_notice(db, senders, due.user_id, &due.language, due.conversation_id, &notice).await;
        let contact = match (due.email, due.phone) {
            (Some(email), _) if due.email_ok => Contact::Email(email),
            (_, Some(phone)) => Contact::Phone(phone),
            _ => continue,
        };
        let lang = Lang::from_code(&due.language).unwrap_or_default();
        match i18n::in_lang(lang, senders.send_notice(&contact, &notice)).await {
            Ok(channel) => tracing::debug!(?channel, to = %contact.masked(), "message notice sent"),
            Err(error) => tracing::warn!(%error, to = %contact.masked(), "message notice not sent"),
        }
    }
    Ok(sent)
}

/// The start of a message, its lines kept, cut at EXCERPT characters with "…".
fn excerpt(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(EXCERPT) {
        Some((cut, _)) => format!("{}…", body[..cut].trim_end()),
        None => body.to_string(),
    }
}

/// One look for the notices due, sent by `senders` with links that start with `origin` (the
/// state's, `Config::site_origin`), a failure in the log: what the jobs run every CHECK_EVERY, as
/// the server starts and from then on, in the process that holds the notices' lock (jobs.rs,
/// `start_all`; `start` here, from `pluribus_web::state`, until D2).
pub async fn tick(db: &PgPool, senders: &Senders, origin: &str) {
    if let Err(error) = send_due(db, senders, origin).await {
        tracing::warn!(%error, "could not send the message notices");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_quotes_the_start_of_a_long_message() {
        assert_eq!(excerpt("  Po, është.\n"), "Po, është.");
        let long = "ë".repeat(EXCERPT + 20);
        let cut = excerpt(&long);
        assert_eq!(cut.chars().count(), EXCERPT + 1);
        assert!(cut.ends_with('…'));
    }
}
