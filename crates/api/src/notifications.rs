//! The notifications, as the phone app shows them (`GET /notifications`): those /lajmerimet lists
//! (pluribus-web's notifications.rs), in its order and as many a page as it shows, a page at a
//! time; each with its title said whole in the language the request asks for, from what its
//! payload keeps (domain::notices), and with what it is about while that is there to open, as the
//! page links it: the listing, the conversation, the saved search. Those the app showed, read
//! (`POST /notifications/seen`), as the page's beacon reads them. And what the app's badges count,
//! in one call (`GET /counts`), each by the function the pages' own JSON counts it with. The
//! shapes they answer in are here, beside their handlers.

use axum::{
    Json,
    extract::{Query, State, rejection::QueryRejection},
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use pluribus_core::{
    domain::{NotificationKind, notices},
    i18n::{self, t},
};
use pluribus_db::{self as db, Notification};
use pluribus_services::AppState;
use serde::{Deserialize, Serialize};
use sqlx::types::JsonValue;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{ApiError, ApiUser, JsonBody, Page};

/// How many notifications a page holds: as many as /lajmerimet shows.
const PAGE: usize = db::NOTIFICATIONS_MOST as usize;

/// The texts /lajmerimet words a kept title anew by, in its language (pluribus-web's
/// notifications.rs, TITLES; their English is in mail.txt): those the titles were written by,
/// which a notification kept before its payload had its title's values says alone.
const KEPT_TITLES: [&str; 8] = [
    "Çmimi ra: {}",
    "Njoftimi skadoi: {}",
    "Njoftimi skadon së shpejti: {}",
    "{} të shkroi për „{}“",
    "{} të shkroi në Pluribus",
    "1 njoftim i ri: {}",
    "{} njoftime të reja: {}",
    "{} të fton në dyqan",
];

/// A notification, as the app lists it (`GET /notifications`).
#[derive(Debug, Serialize)]
pub struct NotificationDto {
    /// A UUIDv7, which sorts as it was made: what `after` and `up_to` name it by.
    pub id: Uuid,
    /// What it is about: PRICE_DROP, SAVED_SEARCH, NEW_MESSAGE, EXPIRING, EXPIRED, PROMOTED or
    /// STORE_INVITE.
    pub kind: NotificationKind,
    /// What it says, whole, in the language asked for: "Çmimi ra: Laptop Lenovo ThinkPad T14",
    /// "Price drop: Laptop Lenovo ThinkPad T14".
    pub title: String,
    /// What it says under its title: a message notice's start of the message, as its writer wrote
    /// it; an invitation into a store's, the role it offers and what that lets one do, in the
    /// language asked for; none for the rest.
    pub body: Option<String>,
    /// The listing it is about (a price drop, an expiry, a promotion), by its number, while the
    /// user may open its page: up or sold, or theirs and not deleted.
    pub listing_no: Option<i64>,
    /// The conversation a message notice is about.
    pub conversation_id: Option<Uuid>,
    /// The saved search whose new listings it tells of, while the user keeps it; where they keep
    /// it no more, the page leads to their saved searches.
    pub search_id: Option<Uuid>,
    /// Whether the user has read it: shown it, here or on /lajmerimet, or seen what it is about
    /// (a conversation read, a price drop on /te-ruajturat, a listing renewed).
    pub read: bool,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl NotificationDto {
    /// The notification `row` keeps, said in the language now; none for one of a kind the code
    /// doesn't write (notifications.notification_type has no CHECK), which the page shows with
    /// its kept title alone.
    fn of(row: Notification) -> Option<Self> {
        let Some(kind) = NotificationKind::parse(&row.kind) else {
            tracing::warn!(id = %row.id, kind = %row.kind, "a notification of a kind the code doesn't write, left out");
            return None;
        };
        let payload = &row.payload.0;
        let title = match says_its_title(kind, payload) {
            true => notices::title(kind, payload),
            false => kept_title(kind, &row.title),
        };
        let body = notices::body(kind, payload).or_else(|| {
            let body = row.body.filter(|body| !body.trim().is_empty())?;
            Some(match kind {
                // Kept before its payload had the role: "Si „Poston njoftime“: …", the role's
                // words the site's own, as the page words them.
                NotificationKind::StoreInvite => i18n::reworded_by(&body, &["Si „{}“: {}"], |value| t(value)).into_owned(),
                _ => body,
            })
        });
        Some(Self {
            id: row.id,
            kind,
            title,
            body,
            listing_no: row.public_no,
            conversation_id: row.conversation,
            search_id: row.search,
            read: row.read,
            created_at: row.created_at,
        })
    }
}

/// Whether `payload` keeps the values a title of `kind` is said from (domain::notices::title), as
/// each writer keeps them since its payload has them (B4, 2026-10-12); before, a payload kept
/// what the notification leads to alone, and its title is the one kept beside it. A message's
/// listing is null once that is gone for good, which its title says too.
fn says_its_title(kind: NotificationKind, payload: &JsonValue) -> bool {
    let text = |key: &str| payload.get(key).is_some_and(JsonValue::is_string);
    match kind {
        NotificationKind::PriceDrop | NotificationKind::Expiring | NotificationKind::Expired => text("title"),
        NotificationKind::Promoted => text("product") && text("title"),
        NotificationKind::NewMessage => {
            text("writer") && payload.get("listing").is_some_and(|listing| listing.is_string() || listing.is_null())
        }
        NotificationKind::SavedSearch => text("name") && payload.get("count").is_some_and(JsonValue::is_i64),
        NotificationKind::StoreInvite => text("store"),
    }
}

/// A title kept in Albanian (notifications.title, its first 180 characters), as /lajmerimet shows
/// it in the language now (pluribus-web's notifications.rs, `shown_title`): worded anew by the
/// text it was written by (KEPT_TITLES), the names the site gives people said anew among its
/// values; a promotion's, its product's name before the listing's ("Ngritje në krye: Golf 7").
fn kept_title(kind: NotificationKind, title: &str) -> String {
    match (kind, title.split_once(": ")) {
        (NotificationKind::Promoted, Some((product, listing))) => format!("{}: {listing}", t(product)),
        _ => i18n::reworded_by(title, &KEPT_TITLES, |value| i18n::name(value)).into_owned(),
    }
}

/// The query of `GET /notifications`.
#[derive(Debug, Deserialize)]
pub struct NotificationsQuery {
    /// The last notification of the page before, by its id: the `next` that page gave.
    pub after: Option<Uuid>,
}

/// `GET /notifications?after=…`: the user's notifications of the last db::NOTIFICATIONS_DAYS, the
/// latest first, as /lajmerimet lists them, PAGE a page: the first those the page shows, and
/// `next`, while more follow, the id of its last, which sent back as `after` gives those after it
/// (db::notifications_after). An `after` that isn't an id is a validation error on `after`.
pub async fn list(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    query: Result<Query<NotificationsQuery>, QueryRejection>,
) -> Result<Json<Page<NotificationDto>>, ApiError> {
    let Ok(Query(NotificationsQuery { after })) = query else {
        let message = t("Vazhdimi i listës nuk lexohet. Fillo përsëri nga faqja e parë.");
        return Err(ApiError::Validation(vec![("after", message.to_string())]));
    };
    let mut rows = db::notifications_after(&state.db, user.id, after, db::NOTIFICATIONS_MOST + 1).await?;
    let more = rows.len() > PAGE;
    rows.truncate(PAGE);
    let next = rows.last().filter(|_| more).map(|last| last.id.to_string());
    Ok(Json(Page { items: rows.into_iter().filter_map(NotificationDto::of).collect(), next }))
}

/// What `POST /notifications/seen` reads.
#[derive(Debug, Deserialize)]
pub struct SeenBody {
    /// The latest of the unread notifications the app showed, by its id.
    pub up_to: Uuid,
}

/// `POST /notifications/seen` `{"up_to": "…"}`: the app showed the user's notifications, up to
/// the latest unread one it showed: those are read, and none that came after, which it didn't
/// show (db::notifications_seen, as /lajmerimet's beacon reads them); and with them what the
/// badges count (`GET /counts`), a price drop or a saved search's alert among them. 204.
pub async fn seen(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(SeenBody { up_to }): JsonBody<SeenBody>,
) -> Result<StatusCode, ApiError> {
    db::notifications_seen(&state.db, user.id, up_to).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// What the app's badges count (`GET /counts`), each as the pages' own JSON counts it, by the same
/// function.
#[derive(Debug, Serialize)]
pub struct CountsDto {
    /// The conversations with messages the user hasn't read (db::unread_conversations), as the
    /// messages button counts them (/mesazhet/numri).
    pub messages: i64,
    /// Their notifications of the last db::NOTIFICATIONS_DAYS they haven't read
    /// (db::unread_notifications), as the account button counts them (/lajmerimet/numri)...
    pub notifications: i64,
    /// ...and of those, the ones about their own listings (expiring, expired, promoted:
    /// NotificationKind::OWN_LISTING), which the account button's dot stands for.
    pub own: i64,
    /// The price drops of the listings they keep they haven't seen (db::price_drops, 20 at most),
    /// as the heart counts them (/te-ruajturat/numrat)...
    pub price_drops: i64,
    /// ...and their saved searches with alerts they haven't seen (db::unseen_search_alerts).
    pub searches: i64,
}

/// `GET /counts`: what the app's badges count (`CountsDto`), in one call; never kept
/// (`Cache-Control: no-store`), as the pages' counts aren't.
pub async fn counts(State(state): State<AppState>, ApiUser(user): ApiUser) -> Result<impl IntoResponse, ApiError> {
    let (messages, unread, drops, searches) = tokio::try_join!(
        db::unread_conversations(&state.db, user.id),
        db::unread_notifications(&state.db, user.id),
        db::price_drops(&state.db, user.id),
        db::unseen_search_alerts(&state.db, user.id),
    )?;
    let counts = CountsDto { messages, notifications: unread.all, own: unread.own, price_drops: drops.len() as i64, searches };
    Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(counts)))
}

#[cfg(test)]
mod tests {
    use pluribus_core::i18n::{LANG, Lang};
    use serde_json::json;

    use super::*;

    /// What the payloads name.
    const LISTING: &str = "a0000000-0000-4000-8000-000000001001";
    const CONVERSATION: &str = "0192f0c1-0000-7000-8000-00000000000c";
    const SEARCH: &str = "0192f0c1-0000-7000-8000-00000000000a";
    const INVITATION: &str = "0192f0c1-0000-7000-8000-00000000000b";
    const URL: &str = "https://pluribus.example/kerkimet";

    #[test]
    fn a_title_is_said_from_a_payload_that_keeps_its_values() {
        let listing = json!({"listing_id": LISTING, "public_no": 1001, "title": "Golf 7"});
        let with = |more: JsonValue| {
            let mut payload = listing.clone();
            payload.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
            payload
        };
        let message = |listing: JsonValue| json!({"conversation": CONVERSATION, "writer": "Besnik", "listing": listing});
        // Each kind's payload as its writer keeps it (domain::notices).
        let kept = [
            (NotificationKind::PriceDrop, with(json!({"was": 9500.0, "price": 9000.0, "currency": "EUR"}))),
            (NotificationKind::Expiring, with(json!({"expires_at": "2026-10-14T10:00:00+00:00"}))),
            (NotificationKind::Expired, listing.clone()),
            (NotificationKind::Promoted, with(json!({"product": "I veçuar për 7 ditë"}))),
            (NotificationKind::NewMessage, message(json!("Golf 7"))),
            // About a listing gone for good.
            (NotificationKind::NewMessage, message(JsonValue::Null)),
            (NotificationKind::SavedSearch, json!({"search": SEARCH, "name": "Makina në Tiranë", "count": 3, "url": URL})),
            (NotificationKind::StoreInvite, json!({"invitation": INVITATION, "store": "Auto Elbasani", "role": "EDITOR"})),
        ];
        for kind in NotificationKind::ALL {
            assert!(kept.iter().any(|(of, _)| *of == kind), "{kind}: no payload");
            assert!(!says_its_title(kind, &json!({})), "{kind}: an empty payload");
        }
        for (kind, payload) in &kept {
            assert!(says_its_title(*kind, payload), "{kind}: {payload}");
        }
        // As the writers kept them before: what the notification leads to, and its numbers.
        let before = [
            (NotificationKind::PriceDrop, json!({"listing_id": LISTING, "was": 9500.0, "price": 9000.0, "currency": "EUR"})),
            (NotificationKind::Expiring, json!({"listing_id": LISTING, "expires_at": "2026-10-14T10:00:00+00:00"})),
            (NotificationKind::Promoted, json!({"listing_id": LISTING, "title": "Golf 7"})),
            (NotificationKind::NewMessage, json!({"conversation": CONVERSATION})),
            (NotificationKind::NewMessage, json!({"conversation": CONVERSATION, "writer": "Besnik"})),
            (NotificationKind::SavedSearch, json!({"search": SEARCH, "count": 3, "url": URL})),
            (NotificationKind::StoreInvite, json!({"invitation": INVITATION})),
        ];
        for (kind, payload) in &before {
            assert!(!says_its_title(*kind, payload), "{kind}: {payload}");
        }
    }

    #[test]
    fn a_kept_title_is_worded_as_the_page_words_it() {
        let kept = [
            (NotificationKind::PriceDrop, "Çmimi ra: Golf 7", "Price drop: Golf 7"),
            (NotificationKind::NewMessage, "Përdorues të shkroi për „Golf 7“", "User wrote to you about “Golf 7”"),
            (NotificationKind::SavedSearch, "1 njoftim i ri: Makina në Tiranë", "1 new listing: Makina në Tiranë"),
            (NotificationKind::Promoted, "I veçuar për 7 ditë: Golf 7", "Featured for 7 days: Golf 7"),
            (NotificationKind::StoreInvite, "Auto Elbasani të fton në dyqan", "Auto Elbasani invites you to their store"),
            // A title no text of the site's fits stays as it was kept.
            (NotificationKind::PriceDrop, "Diçka tjetër", "Diçka tjetër"),
        ];
        for (kind, albanian, english) in kept {
            assert_eq!(kept_title(kind, albanian), albanian, "{kind}");
            LANG.sync_scope(Lang::En, || assert_eq!(kept_title(kind, albanian), english, "{kind}"));
        }
    }
}
