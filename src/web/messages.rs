//! Messages between buyers and sellers. A buyer writes to a listing's seller from its page
//! (/listing/{no}/mesazh), and the two go on in their conversation (/mesazhet/{id}), listed with
//! the others in the inbox (/mesazhet); the header's button says how many have messages unread
//! (inbox_controller.js). Spam is kept down by who may write and how much: only someone signed in,
//! which takes a code sent to a phone number or an e-mail address (account.rs); to a few new
//! sellers an hour and a day; until the other side answers, three messages at most and no links,
//! which is what spam and scams mostly send; and anyone can block whoever writes to them.

use askama::Template;
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use time::PrimitiveDateTime;
use uuid::Uuid;

use super::{
    Result,
    account::{cookie_header, current_user, redirect_with_cookies},
    not_found,
    post::link_count,
    render, render_stream, saved,
};
use crate::{
    AppState,
    db::{self, Conversation, InboxEntry, Message, MessageTarget, SessionUser},
    format,
};

/// A message's length, in characters.
pub const MAX_CHARS: usize = 1000;
/// Messages one account sends: in a minute, which only a script outpaces, and in a day.
const PER_MINUTE: i64 = 8;
const PER_DAY: i64 = 200;
/// New sellers one account writes to (conversations it starts): in an hour, and in a day.
const NEW_PER_HOUR: i64 = 5;
const NEW_PER_DAY: i64 = 15;
/// Until the other side answers, how many messages in a row; and those carry no links.
const UNANSWERED: i64 = 3;
/// The conversations the inbox shows, the latest first.
const INBOX_MOST: i64 = 200;

/// First messages a buyer can start with, a tap away (conversation_controller.js).
pub const QUICK: [&str; 3] = ["Përshëndetje! A është ende në dispozicion?", "Cili është çmimi i fundit?", "Kur mund ta shoh?"];

/// What the conversation's page lets its visitor do: write, or why they can't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writing {
    Open,
    /// They blocked the other side, who can't write to them either; unblocking opens it again.
    Blocked,
    /// The other side blocked them.
    BlockedBy,
    /// A listing that takes no messages: down, sold, or with messages turned off.
    Closed,
}

impl Writing {
    fn of(open: bool, blocked: bool, blocked_by: bool) -> Self {
        if blocked {
            Writing::Blocked
        } else if blocked_by {
            Writing::BlockedBy
        } else if open {
            Writing::Open
        } else {
            Writing::Closed
        }
    }

    /// Why a message can't go, for a form sent anyway.
    fn refusal(self, other: &str) -> Option<String> {
        match self {
            Writing::Open => None,
            Writing::Blocked => Some("E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.".to_string()),
            Writing::BlockedBy => Some(format!("{other} nuk pranon mesazhe nga ti.")),
            Writing::Closed => Some("Ky njoftim nuk pranon mesazhe tani.".to_string()),
        }
    }
}

/// The listing a conversation is about, at the top of its page.
pub struct ListingHead {
    pub title: String,
    pub url: String,
    pub photo: Option<String>,
    pub price: String,
}

/// A conversation's page, or the page that starts one about a listing.
#[derive(Template)]
#[template(path = "conversation.html")]
struct ConversationPage {
    listing: Option<ListingHead>,
    other_name: String,
    /// The conversation, once there is one.
    id: Option<Uuid>,
    messages: Vec<Message>,
    /// Where the form goes: the listing's address to start one, the conversation's after.
    action: String,
    writing: Writing,
    error: Option<String>,
    /// The message as typed, kept when it goes back with an error.
    body: String,
    /// The first message, about to start the conversation: the quick ones are offered.
    first: bool,
}

impl ConversationPage {
    /// The page that starts a conversation about the listing.
    fn start(target: &MessageTarget, writing: Writing, error: Option<String>, body: String) -> Self {
        Self {
            listing: Some(ListingHead {
                title: target.title.clone(),
                url: format!("/listing/{}-{}", target.public_no, target.slug),
                photo: target.photo.clone(),
                price: format::price(target.price_amount, &target.currency, &target.price_type, &target.listing_type),
            }),
            other_name: target.seller_name.clone(),
            id: None,
            messages: Vec::new(),
            action: format!("/listing/{}/mesazh", target.public_no),
            writing,
            error,
            body,
            first: true,
        }
    }

    fn of(conversation: &Conversation, messages: Vec<Message>, error: Option<String>, body: String) -> Self {
        let listing = conversation.public_no.zip(conversation.slug.as_ref()).map(|(public_no, slug)| ListingHead {
            title: conversation.title.clone().unwrap_or_default(),
            url: format!("/listing/{public_no}-{slug}"),
            photo: conversation.photo.clone(),
            price: format::price(
                conversation.price_amount,
                conversation.currency.as_deref().unwrap_or("ALL"),
                conversation.price_type.as_deref().unwrap_or("FIXED"),
                conversation.listing_type.as_deref().unwrap_or("SELL"),
            ),
        });
        Self {
            listing,
            other_name: conversation.other_name.clone(),
            id: Some(conversation.id),
            messages,
            action: format!("/mesazhet/{}", conversation.id),
            writing: Writing::of(true, conversation.blocked, conversation.blocked_by),
            error,
            body,
            first: false,
        }
    }

    /// The other side's initial, in a circle before their name, as on a listing's page.
    fn initial(&self) -> String {
        format::initial(&self.other_name)
    }
}

/// New messages in a conversation, added at the end of its page (conversation_controller.js).
#[derive(Template)]
#[template(path = "messages_stream.html")]
struct NewMessages {
    other_name: String,
    messages: Vec<Message>,
}

#[derive(Template)]
#[template(path = "messages.html")]
struct InboxPage {
    signed_in: bool,
    conversations: Vec<InboxEntry>,
}

/// When a message was sent, by Albania's clock: "14:32" today, "Dje, 14:32", then the date.
fn when(sent: PrimitiveDateTime, days_ago: i32) -> String {
    let time = format!("{:02}:{:02}", sent.hour(), sent.minute());
    match days_ago {
        0 => time,
        1 => format!("Dje, {time}"),
        _ => format!("{:02}.{:02}.{}, {time}", sent.day(), u8::from(sent.month()), sent.year()),
    }
}

impl Message {
    pub fn when(&self) -> String {
        when(self.sent, self.days_ago)
    }
}

impl InboxEntry {
    /// When its latest message came: the time today, "Dje", the day and month this year, or the date.
    pub fn when(&self) -> String {
        let (Some(sent), Some(days_ago)) = (self.last_sent, self.days_ago) else {
            return String::new();
        };
        match days_ago {
            0 => format!("{:02}:{:02}", sent.hour(), sent.minute()),
            1 => "Dje".to_string(),
            _ if days_ago < 300 => format!("{:02}.{:02}", sent.day(), u8::from(sent.month())),
            _ => format!("{:02}.{:02}.{}", sent.day(), u8::from(sent.month()), sent.year()),
        }
    }

    /// The start of its latest message, on one line.
    pub fn preview(&self) -> String {
        let text = self.last_body.as_deref().unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        if text.chars().count() > 90 { format!("{}…", text.chars().take(89).collect::<String>()) } else { text }
    }
}

#[derive(Deserialize)]
pub struct MessageForm {
    #[serde(default)]
    body: String,
}

/// The message as sent: its line breaks the same everywhere, without space around it.
fn tidy(body: &str) -> String {
    body.replace("\r\n", "\n").trim().to_string()
}

/// Why this message can't go, if it can't: what it says, then how much its sender sent lately.
/// `unanswered` is how many they sent in a row where the other side hasn't answered yet, and
/// `new` whether it starts a conversation.
async fn refusal(
    state: &AppState,
    user: &SessionUser,
    body: &str,
    unanswered: Option<i64>,
    new: bool,
    other: &str,
) -> Result<Option<String>> {
    let chars = body.chars().count();
    if chars == 0 {
        return Ok(Some("Shkruaj mesazhin.".to_string()));
    }
    if chars > MAX_CHARS {
        return Ok(Some(format!("Mesazhi mund të ketë deri në {MAX_CHARS} shkronja.")));
    }
    if let Some(in_a_row) = unanswered {
        if in_a_row >= UNANSWERED {
            return Ok(Some(format!("Prit që {other} të të përgjigjet para se të shkruash më shumë.")));
        }
        if link_count(body) > 0 {
            return Ok(Some(format!("Lidhjet mund t'i dërgosh pasi {other} të të përgjigjet.")));
        }
    }
    let sent = db::sent_lately(&state.db, user.id).await?;
    let message = if sent.last_minute >= PER_MINUTE {
        Some("Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.".to_string())
    } else if sent.last_day >= PER_DAY {
        Some("Ke dërguar shumë mesazhe sot. Provo përsëri nesër.".to_string())
    } else if new && sent.started_last_hour >= NEW_PER_HOUR {
        Some(format!("U ke shkruar {NEW_PER_HOUR} shitësve këtë orë. Prit pak para se t'i shkruash një tjetri."))
    } else if new && sent.started_last_day >= NEW_PER_DAY {
        Some(format!("U ke shkruar {NEW_PER_DAY} shitësve sot. Mund t'u shkruash të tjerëve nesër."))
    } else {
        None
    };
    Ok(message)
}

/// Someone not signed in: to sign in first, and back to `page` once they have
/// (saved::after_sign_in).
fn sign_in_first(headers: &HeaderMap, page: &str) -> Response {
    redirect_with_cookies("/llogaria", vec![cookie_header(headers, saved::BACK_COOKIE, page, saved::SIGN_IN_SECONDS)])
}

/// A form shown again with why it didn't go; Turbo renders it because of the 422 status.
fn unprocessable(page: ConversationPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

/// GET /listing/{public_no}/mesazh: the page that starts a conversation with the listing's seller,
/// or the conversation the visitor has about it already.
pub async fn compose(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(sign_in_first(&headers, &format!("/listing/{public_no}/mesazh")));
    };
    let Some(target) = db::message_target(&state.db, public_no, user.id).await? else {
        return not_found().await;
    };
    if let Some(id) = target.conversation_id {
        return Ok(Redirect::to(&format!("/mesazhet/{id}")).into_response());
    }
    if target.seller_side_id == user.id {
        return Ok(Redirect::to(&format!("/listing/{}-{}", target.public_no, target.slug)).into_response());
    }
    let (blocked, blocked_by) = db::blocks_between(&state.db, user.id, target.seller_side_id).await?;
    render(ConversationPage::start(&target, Writing::of(target.open, blocked, blocked_by), None, String::new()))
}

/// POST /listing/{public_no}/mesazh: the first message, which starts the conversation; into the
/// one the visitor has about the listing, if they have one already.
pub async fn start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Form(form): Form<MessageForm>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(sign_in_first(&headers, &format!("/listing/{public_no}/mesazh")));
    };
    let Some(target) = db::message_target(&state.db, public_no, user.id).await? else {
        return not_found().await;
    };
    if let Some(id) = target.conversation_id {
        return send_into(&state, &user, id, form).await;
    }
    if target.seller_side_id == user.id {
        return Ok(Redirect::to(&format!("/listing/{}-{}", target.public_no, target.slug)).into_response());
    }
    let (blocked, blocked_by) = db::blocks_between(&state.db, user.id, target.seller_side_id).await?;
    let writing = Writing::of(target.open, blocked, blocked_by);
    let body = tidy(&form.body);
    let error = match writing.refusal(&target.seller_name) {
        Some(reason) => Some(reason),
        None => refusal(&state, &user, &body, Some(0), true, &target.seller_name).await?,
    };
    if let Some(error) = error {
        return unprocessable(ConversationPage::start(&target, writing, Some(error), body));
    }
    let id = db::start_conversation(&state.db, &target, user.id, &body).await?;
    Ok(Redirect::to(&format!("/mesazhet/{id}")).into_response())
}

/// GET /mesazhet: the visitor's conversations, the latest first; for someone not signed in, the
/// way to sign in.
pub async fn inbox(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let (signed_in, conversations) = match current_user(&state, &headers).await? {
        Some(user) => (true, db::inbox(&state.db, user.id, INBOX_MOST).await?),
        None => (false, Vec::new()),
    };
    render(InboxPage { signed_in, conversations })
}

/// GET /mesazhet/numri: how many of the visitor's conversations have messages unread, as JSON, for
/// the header's button (inbox_controller.js); none for someone not signed in. `{"unread": 2}`.
pub async fn unread(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let unread = match current_user(&state, &headers).await? {
        Some(user) => db::unread_conversations(&state.db, user.id).await?,
        None => 0,
    };
    let body = serde_json::json!({ "unread": unread });
    Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(body)).into_response())
}

/// The conversation with this id in the address, for someone who takes part in it.
async fn find(state: &AppState, user: &SessionUser, id: &str) -> Result<Option<Conversation>> {
    let Ok(id) = id.parse::<Uuid>() else {
        return Ok(None);
    };
    Ok(db::conversation(&state.db, id, user.id).await?)
}

/// GET /mesazhet/{id}: the conversation, for those who take part in it; "not found" for anyone else.
pub async fn conversation(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        let back = id.parse::<Uuid>().map_or_else(|_| "/mesazhet".to_string(), |id| format!("/mesazhet/{id}"));
        return Ok(sign_in_first(&headers, &back));
    };
    let Some(conversation) = find(&state, &user, &id).await? else {
        return not_found().await;
    };
    let messages = db::messages(&state.db, conversation.id, user.id, None).await?;
    render(ConversationPage::of(&conversation, messages, None, String::new()))
}

/// POST /mesazhet/{id}: a message in the conversation.
pub async fn reply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Form(form): Form<MessageForm>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(sign_in_first(&headers, "/mesazhet"));
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return not_found().await;
    };
    send_into(&state, &user, id, form).await
}

/// Sends the message into the conversation, after the limits; back to its page either way.
async fn send_into(state: &AppState, user: &SessionUser, id: Uuid, form: MessageForm) -> Result<Response> {
    let Some(conversation) = db::conversation(&state.db, id, user.id).await? else {
        return not_found().await;
    };
    let body = tidy(&form.body);
    let writing = Writing::of(true, conversation.blocked, conversation.blocked_by);
    let unanswered = (!conversation.answered).then_some(conversation.in_a_row);
    let error = match writing.refusal(&conversation.other_name) {
        Some(reason) => Some(reason),
        None => refusal(state, user, &body, unanswered, false, &conversation.other_name).await?,
    };
    if let Some(error) = error {
        let messages = db::messages(&state.db, id, user.id, None).await?;
        return unprocessable(ConversationPage::of(&conversation, messages, Some(error), body));
    }
    db::send_message(&state.db, id, user.id, &body).await?;
    Ok(Redirect::to(&format!("/mesazhet/{id}")).into_response())
}

#[derive(Deserialize)]
pub struct After {
    /// The last message the page has.
    pas: Option<Uuid>,
}

/// GET /mesazhet/{id}/te-rejat?pas=…: the messages after the page's last, as a Turbo Stream that
/// adds them at its end (conversation_controller.js asks every few seconds); 204 when none came.
pub async fn newer(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(After { pas }): Query<After>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    };
    let Some(conversation) = find(&state, &user, &id).await? else {
        return not_found().await;
    };
    let messages = db::messages(&state.db, conversation.id, user.id, pas).await?;
    if messages.is_empty() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    render_stream(NewMessages { other_name: conversation.other_name, messages })
}

#[derive(Deserialize)]
pub struct Upto {
    /// The last message the page showed.
    deri: Uuid,
}

/// POST /mesazhet/{id}/lexuar?deri=…: the conversation's page showed its messages up to this one
/// (conversation_controller.js, as it shows and as more come): they are read, and the header's
/// count goes down. Not when Turbo only fetched the page ahead, under the pointer.
pub async fn read(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(Upto { deri }): Query<Upto>,
) -> Result<Response> {
    if let (Some(user), Ok(id)) = (current_user(&state, &headers).await?, id.parse::<Uuid>())
        && db::takes_part(&state.db, id, user.id).await?
    {
        db::mark_read(&state.db, id, user.id, deri).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// POST /mesazhet/{id}/blloko and /zhblloko: blocks the other side of the conversation, who can't
/// write to the visitor any more, nor start another conversation with them; or lets them again.
pub async fn block(state: State<AppState>, headers: HeaderMap, id: Path<String>) -> Result<Response> {
    set_blocked(state, headers, id, true).await
}

pub async fn unblock(state: State<AppState>, headers: HeaderMap, id: Path<String>) -> Result<Response> {
    set_blocked(state, headers, id, false).await
}

async fn set_blocked(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>, on: bool) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(sign_in_first(&headers, "/mesazhet"));
    };
    let Some(conversation) = find(&state, &user, &id).await? else {
        return not_found().await;
    };
    db::set_user_blocked(&state.db, user.id, conversation.other_id, on).await?;
    Ok(Redirect::to(&format!("/mesazhet/{}", conversation.id)).into_response())
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn times_say_today_yesterday_or_the_date() {
        let sent = datetime!(2026-10-08 9:05);
        assert_eq!(when(sent, 0), "09:05");
        assert_eq!(when(sent, 1), "Dje, 09:05");
        assert_eq!(when(sent, 3), "08.10.2026, 09:05");
    }

    #[test]
    fn messages_go_without_space_around_them() {
        assert_eq!(tidy("  Përshëndetje!\r\nA është ende?  \n"), "Përshëndetje!\nA është ende?");
    }
}
