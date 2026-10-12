//! What a notification says, from what its payload keeps (notifications.payload): its title, and
//! under an invitation into a store's, the role it offers, in the language now. Its writer keeps
//! the title as a sentence in Albanian too (notifications.title, its first 180 characters), which
//! /lajmerimet words anew for an English reader by the texts it was written by
//! (web/notifications.rs, i18n::reworded); said from the payload, it is the same sentence in
//! either language, whole, for what has the data alone (an API). The texts are the catalogue's,
//! the very ones the titles are written by. Each kind's payload, as its writer keeps it:
//!
//! - PRICE_DROP (db/price_drops.rs): `listing_id`, `public_no` and `title`, the listing's; `was`,
//!   `price` and `currency`, what its price dropped from and to. "Çmimi ra: {title}".
//! - EXPIRING, EXPIRED (db/expiry.rs): `listing_id`, `public_no` and `title`; an EXPIRING's
//!   `expires_at` too, the expiry it warns of. "Njoftimi skadon së shpejti: {title}", "Njoftimi
//!   skadoi: {title}".
//! - NEW_MESSAGE (notices.rs, db/notices.rs): `conversation`; `writer`, who wrote (to a store's
//!   buyer, the store's name); `listing`, the title of the listing it is about, null once that is
//!   gone for good. "{writer} të shkroi për „{listing}“", or "{writer} të shkroi në Pluribus".
//! - SAVED_SEARCH (web/searches.rs, db/searches.rs): `search` and `name`, the saved search's;
//!   `count`, its new listings (more than NEW_MOST counted as one more); `url`, where to see them.
//!   "3 njoftime të reja: {name}".
//! - PROMOTED (db/promotions.rs): `listing_id`, `public_no` and `title`; `product`, the
//!   promotion's name, as promotion_products has it. "{product}: {title}".
//! - STORE_INVITE (web/stores.rs, db/stores.rs): `invitation`; `store`, the store's name; `role`,
//!   the one it offers (a domain::Role). "{store} të fton në dyqan", and under it "Si „{role}“:
//!   {what the role lets one do}".

use serde_json::Value;

use super::{NotificationKind, Role};
use crate::i18n::{name, t, t1, t2};

/// A saved search's new listings are counted up to this many: past it, "99+".
pub const NEW_MOST: i64 = 99;

/// "1 njoftim i ri", "12 njoftime të reja", "99+ njoftime të reja" (past NEW_MOST), in the language
/// now: a saved search's new listings, as /kerkimet-e-ruajtura, its alert's e-mail and its
/// notification's title say them (web/searches.rs); Albanian for the notification it keeps and for
/// its WhatsApp template.
pub fn new_listings(count: i64) -> String {
    match count {
        1 => t("1 njoftim i ri").to_string(),
        count if count > NEW_MOST => t1("{}+ njoftime të reja", NEW_MOST),
        count => t1("{} njoftime të reja", count),
    }
}

/// A notification's title, said from its payload in the language now: in Albanian, the sentence
/// its writer kept (of which notifications.title keeps the first 180 characters); in English, what
/// /lajmerimet words that into. What the payload lacks (a notification kept before its payload had
/// its title's values) reads as nothing.
pub fn title(kind: NotificationKind, payload: &Value) -> String {
    let listing = || text(payload, "title").unwrap_or_default();
    match kind {
        NotificationKind::PriceDrop => t1("Çmimi ra: {}", listing()),
        NotificationKind::Expiring => t1("Njoftimi skadon së shpejti: {}", listing()),
        NotificationKind::Expired => t1("Njoftimi skadoi: {}", listing()),
        NotificationKind::NewMessage => {
            // The names the site gives people, in the language now ("Përdorues", "User").
            let writer = name(text(payload, "writer").unwrap_or_default());
            match text(payload, "listing") {
                Some(listing) => t2("{} të shkroi për „{}“", writer, listing),
                None => t1("{} të shkroi në Pluribus", writer),
            }
        }
        NotificationKind::SavedSearch => {
            let count = payload.get("count").and_then(Value::as_i64).unwrap_or_default();
            format!("{}: {}", new_listings(count), text(payload, "name").unwrap_or_default())
        }
        // The product's name in the language now, which the catalogue has as the database does.
        NotificationKind::Promoted => format!("{}: {}", t(text(payload, "product").unwrap_or_default()), listing()),
        NotificationKind::StoreInvite => t1("{} të fton në dyqan", text(payload, "store").unwrap_or_default()),
    }
}

/// What a notification says under its title, said from its payload in the language now: an
/// invitation's, the role it offers and what that lets one do ("Si „Poston njoftime“: …"), as its
/// writer kept it (notifications.body). None for the other kinds, a message notice's being the
/// start of the message, as its writer wrote it (notifications.body), and the rest having none;
/// nor for an invitation kept before its payload had its role.
pub fn body(kind: NotificationKind, payload: &Value) -> Option<String> {
    match kind {
        NotificationKind::StoreInvite => {
            let role = text(payload, "role").and_then(Role::parse)?;
            Some(t2("Si „{}“: {}", role.label(), role.does()))
        }
        NotificationKind::PriceDrop
        | NotificationKind::SavedSearch
        | NotificationKind::NewMessage
        | NotificationKind::Expiring
        | NotificationKind::Expired
        | NotificationKind::Promoted => None,
    }
}

/// The text the payload keeps under `key`, if it keeps one.
fn text<'a>(payload: &'a Value, key: &str) -> Option<&'a str> {
    payload.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::i18n::{LANG, Lang, reworded_by};

    /// The texts /lajmerimet words a kept title anew by, in their order, as web/notifications.rs's
    /// TITLES has them.
    const TITLES: [&str; 8] = [
        "Çmimi ra: {}",
        "Njoftimi skadoi: {}",
        "Njoftimi skadon së shpejti: {}",
        "{} të shkroi për „{}“",
        "{} të shkroi në Pluribus",
        "1 njoftim i ri: {}",
        "{} njoftime të reja: {}",
        "{} të fton në dyqan",
    ];

    /// What /lajmerimet shows of a kept title in the language now, as web/notifications.rs's
    /// shown_title words it: a promotion's, its product's name before the listing's; the rest by
    /// TITLES, the names the site gives people said anew among the values.
    fn shown(kind: NotificationKind, kept: &str) -> String {
        match (kind, kept.split_once(": ")) {
            (NotificationKind::Promoted, Some((product, listing))) => format!("{}: {listing}", t(product)),
            _ => reworded_by(kept, &TITLES, |value| name(value)).into_owned(),
        }
    }

    /// Each kind's payload as its writer keeps it, the title it keeps beside it, and what an English
    /// reader's /lajmerimet shows of that (tests/site.rs's values, where it has them).
    fn kept() -> Vec<(NotificationKind, Value, &'static str, &'static str)> {
        let listing = json!({"listing_id": "a0000000-0000-4000-8000-000000001001", "public_no": 1001, "title": "Volkswagen Golf 7"});
        let with = |more: Value| {
            let mut payload = listing.clone();
            payload.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            payload
        };
        let conversation = "0192f0c1-0000-7000-8000-00000000000c";
        let search = |count: i64| {
            json!({"search": "0192f0c1-0000-7000-8000-00000000000a", "name": "Makina në Tiranë", "count": count,
                   "url": "https://pluribus.example/kerkimet/0192f0c1-0000-7000-8000-00000000000a"})
        };
        vec![
            (
                NotificationKind::PriceDrop,
                with(json!({"was": 9500.0, "price": 9000.0, "currency": "EUR"})),
                "Çmimi ra: Volkswagen Golf 7",
                "Price drop: Volkswagen Golf 7",
            ),
            (
                NotificationKind::Expiring,
                with(json!({"expires_at": "2026-10-14T10:00:00+00:00"})),
                "Njoftimi skadon së shpejti: Volkswagen Golf 7",
                "Listing expires soon: Volkswagen Golf 7",
            ),
            (NotificationKind::Expired, listing.clone(), "Njoftimi skadoi: Volkswagen Golf 7", "Listing expired: Volkswagen Golf 7"),
            (
                NotificationKind::NewMessage,
                json!({"conversation": conversation, "writer": "Përdorues", "listing": "Volkswagen Golf 7"}),
                "Përdorues të shkroi për „Volkswagen Golf 7“",
                "User wrote to you about “Volkswagen Golf 7”",
            ),
            (
                NotificationKind::NewMessage,
                json!({"conversation": conversation, "writer": "Auto Elbasani", "listing": null}),
                "Auto Elbasani të shkroi në Pluribus",
                "Auto Elbasani wrote to you on Pluribus",
            ),
            (NotificationKind::SavedSearch, search(3), "3 njoftime të reja: Makina në Tiranë", "3 new listings: Makina në Tiranë"),
            (NotificationKind::SavedSearch, search(1), "1 njoftim i ri: Makina në Tiranë", "1 new listing: Makina në Tiranë"),
            (NotificationKind::SavedSearch, search(NEW_MOST + 1), "99+ njoftime të reja: Makina në Tiranë", "99+ new listings: Makina në Tiranë"),
            (
                NotificationKind::Promoted,
                with(json!({"product": "I veçuar për 7 ditë"})),
                "I veçuar për 7 ditë: Volkswagen Golf 7",
                "Featured for 7 days: Volkswagen Golf 7",
            ),
            (
                NotificationKind::StoreInvite,
                json!({"invitation": "0192f0c1-0000-7000-8000-00000000000b", "store": "Auto Elbasani", "role": "EDITOR"}),
                "Auto Elbasani të fton në dyqan",
                "Auto Elbasani invites you to their store",
            ),
        ]
    }

    #[test]
    fn a_title_said_from_its_payload_is_the_one_its_writer_kept() {
        let kept = kept();
        for kind in NotificationKind::ALL {
            assert!(kept.iter().any(|(of, ..)| *of == kind), "{kind}: no title to compare");
        }
        for (kind, payload, albanian, english) in &kept {
            // Outside a request, and in Albanian, the sentence its writer kept.
            assert_eq!(title(*kind, payload), *albanian, "{kind}");
            LANG.sync_scope(Lang::Sq, || assert_eq!(title(*kind, payload), *albanian, "{kind}"));
            // In English, what /lajmerimet words the kept one into today.
            LANG.sync_scope(Lang::En, || {
                assert_eq!(shown(*kind, albanian), *english, "{kind}: /lajmerimet");
                assert_eq!(title(*kind, payload), *english, "{kind}");
            });
        }
        // Kept before its payload had its title's values, a title has its places empty.
        assert_eq!(title(NotificationKind::PriceDrop, &json!({"listing_id": "a0000000-0000-4000-8000-000000001001"})), "Çmimi ra: ");
    }

    #[test]
    fn an_invitations_body_says_the_role_it_offers() {
        let payload = json!({"invitation": "0192f0c1-0000-7000-8000-00000000000b", "store": "Auto Elbasani", "role": "EDITOR"});
        let albanian = "Si „Poston njoftime“: Poston dhe ndryshon njoftimet e dyqanit, dhe u përgjigjet mesazheve.";
        assert_eq!(body(NotificationKind::StoreInvite, &payload).as_deref(), Some(albanian));
        LANG.sync_scope(Lang::En, || {
            // As /lajmerimet words the kept one, the role's words being the site's own.
            let english = "As “Posts listings”: Posts and edits the store’s listings, and answers messages.";
            assert_eq!(reworded_by(albanian, &["Si „{}“: {}"], |value| t(value)), english);
            assert_eq!(body(NotificationKind::StoreInvite, &payload).as_deref(), Some(english));
        });
        // None for an invitation kept before its payload had the role, nor for the other kinds.
        assert_eq!(body(NotificationKind::StoreInvite, &json!({"invitation": "0192f0c1-0000-7000-8000-00000000000b"})), None);
        for kind in NotificationKind::ALL.into_iter().filter(|kind| *kind != NotificationKind::StoreInvite) {
            assert_eq!(body(kind, &payload), None, "{kind}");
        }
    }
}
