//! The notifications centre, /lajmerimet ("Lajmërimet", linked from "Njoftimet e mia"): every
//! notification the user had in the last db::NOTIFICATIONS_DAYS, whatever its kind, the latest
//! first, each leading to what it is about, those unread marked until the page has been shown. And
//! what the account button's dot asks for (account_controller.js): how many are unread, and how
//! many of those are about the user's own listings, which no other button shows.

use askama::Template;
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Result, account::current_user, icons, render};
use crate::{
    AppState, db, format,
    i18n::{self, t},
};

/// The titles the site keeps, in Albanian, as the texts they were written by have them
/// (db/price_drops.rs, db/expiry.rs, notices.rs, searches.rs, stores.rs), which an English
/// reader's page words anew (i18n::reworded_by; their English is in mail.txt).
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

#[derive(Template)]
#[template(path = "notifications.html")]
struct NotificationsPage {
    signed_in: bool,
    rows: Vec<Row>,
    /// Where the page says, once shown, that it showed its unread notifications: up to the latest
    /// of them (application.js, data-seen-url). None while none is unread.
    seen_url: Option<String>,
}

/// A notification as the page lists it.
struct Row {
    /// Its kind's icon (icons.rs).
    icon: &'static str,
    title: String,
    /// What a message notice said of the message, under its title.
    body: Option<String>,
    /// When, in words ("Sot", "para 3 ditësh").
    when: String,
    /// Where it leads, while what it is about is still there.
    href: Option<String>,
    unread: bool,
}

impl Row {
    fn new(notification: db::Notification, now: OffsetDateTime) -> Self {
        let listing = notification.public_no.zip(notification.slug.as_ref()).map(|(no, slug)| format::listing_path(no, slug));
        let (icon, href) = match notification.kind.as_str() {
            "PRICE_DROP" => (icons::PRICE_TAG, listing),
            "SAVED_SEARCH" => (
                icons::BELL,
                Some(notification.search.map_or_else(|| "/kerkimet-e-ruajtura".to_string(), |id| format!("/kerkimet/{id}"))),
            ),
            "NEW_MESSAGE" => (icons::CHAT, notification.conversation.map(|id| format!("/mesazhet/{id}"))),
            "EXPIRING" => (icons::CLOCK, listing),
            "EXPIRED" => (icons::RENEW, listing),
            "PROMOTED" => (icons::MEGAPHONE, listing),
            "STORE_INVITE" => (icons::STORE, notification.invitation.map(|id| format!("/ftesa/{id}"))),
            _ => (icons::BELL, None),
        };
        Self {
            icon,
            title: shown_title(&notification.kind, &notification.title),
            body: notification.body.filter(|body| !body.trim().is_empty()).map(|body| match notification.kind.as_str() {
                // "Si „Poston njoftime“: …": the role's words are the site's own.
                "STORE_INVITE" => i18n::reworded_by(&body, &["Si „{}“: {}"], |value| t(value)).into_owned(),
                _ => body,
            }),
            when: format::when(notification.created_at, now),
            href,
            unread: !notification.read,
        }
    }
}

/// A notification's title, kept in Albanian, in the page's language: worded anew from the text it
/// was written by (TITLES), the names the site gives people in it too; a promotion's, its
/// product's name before the listing's ("Ngritje në krye: Golf 7").
fn shown_title(kind: &str, title: &str) -> String {
    match (kind, title.split_once(": ")) {
        ("PROMOTED", Some((promotion, listing))) => format!("{}: {listing}", t(promotion)),
        _ => i18n::reworded_by(title, &TITLES, |value| i18n::name(value)).into_owned(),
    }
}

/// GET /lajmerimet: the user's notifications; for someone not signed in, the way to sign in.
pub async fn page(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(NotificationsPage { signed_in: false, rows: Vec::new(), seen_url: None });
    };
    let notifications = db::notifications(&state.db, user.id).await?;
    let latest_unread = notifications.iter().filter(|notification| !notification.read).map(|notification| notification.id).max();
    let now = OffsetDateTime::now_utc();
    let rows = notifications.into_iter().map(|notification| Row::new(notification, now)).collect();
    render(NotificationsPage { signed_in: true, rows, seen_url: latest_unread.map(|id| format!("/lajmerimet/pare?deri={id}")) })
}

#[derive(Deserialize)]
pub struct Seen {
    /// The latest unread notification the page showed, by its id.
    deri: Uuid,
}

/// POST /lajmerimet/pare?deri=…: /lajmerimet was shown (application.js, the beacon), not just
/// fetched ahead under the pointer: the notifications it showed unread are read, and with them the
/// dots of the heart and the account (a price drop or a saved search's alert is read there too).
pub async fn seen(State(state): State<AppState>, headers: HeaderMap, Query(Seen { deri }): Query<Seen>) -> Result<Response> {
    if let Some(user) = current_user(&state, &headers).await? {
        db::notifications_seen(&state.db, user.id, deri).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// GET /lajmerimet/numri: for the account button (account_controller.js), as JSON, how many of the
/// user's notifications are unread, and how many of those are about their own listings:
/// `{"unread": 3, "own": 1}`; none for someone not signed in.
pub async fn counts(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let unread = match current_user(&state, &headers).await? {
        Some(user) => db::unread_notifications(&state.db, user.id).await?,
        None => db::Unread::default(),
    };
    let body = serde_json::json!({ "unread": unread.all, "own": unread.own });
    Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(body)).into_response())
}
