//! Messages between buyers and sellers. A buyer writes to a listing's seller from its page
//! (/listing/{no}/mesazh), and the two go on in their conversation (/mesazhet/{id}), listed with
//! the others in the inbox (/mesazhet); the header's button says how many have messages unread
//! (inbox_controller.js). Spam is kept down by who may write and how much: only someone signed in,
//! which takes a code sent to a phone number or an e-mail address (account.rs); to a few new
//! sellers an hour and a day; until the other side answers, three messages at most and no links,
//! which is what spam and scams mostly send; and anyone can block whoever writes to them. Those
//! rules are domain::messaging's, whose refusals, codes, this module words in the page's language
//! (`reason`). A store's conversations are its people's, each of whom sees and answers them
//! (db/messages.rs).

use askama::Template;
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use time::{OffsetDateTime, macros::format_description};
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
    domain::{
        ListingType, Status,
        messaging::{self, Outgoing, Refusal, Writing},
    },
    format,
    i18n::{english, t, t1},
};

/// A message's length, in characters (domain::messaging), which the box to write in holds to
/// (conversation.html).
pub use crate::domain::messaging::MAX_CHARS;

/// The conversations the inbox shows, the latest first.
const INBOX_MOST: i64 = 200;

/// First messages a buyer can start with, a tap away (conversation_controller.js), in the page's
/// language where conversation.html shows them (crate::t, whose catalogue has each).
pub const QUICK: [&str; 3] = ["Përshëndetje! A është ende në dispozicion?", "Cili është çmimi i fundit?", "Kur mund ta shoh?"];

/// The listing a conversation is about, at the top of its page.
pub struct ListingHead {
    pub title: String,
    pub url: String,
    pub photo: Option<String>,
    pub price: String,
    /// What became of it, once it is no longer simply up: reserved, sold ("Dhënë me qira" for a
    /// rental) or off the site, so that neither side writes on about a thing gone.
    pub status: Option<&'static str>,
}

/// What a conversation's head says of its listing's status (`status`, of a listing of kind
/// `kind`): nothing while it is up (Status::conversation_label). A status the schema doesn't
/// allow would count as off the site, and a kind it doesn't as one on sale.
fn listing_status(status: Option<&str>, kind: Option<&str>) -> Option<&'static str> {
    let kind = kind.and_then(ListingType::parse).unwrap_or(ListingType::Sell);
    Status::parse(status?).unwrap_or(Status::Deleted).conversation_label(kind)
}

/// A conversation's page, or the page that starts one about a listing.
#[derive(Template)]
#[template(path = "conversation.html")]
struct ConversationPage {
    listing: Option<ListingHead>,
    other_name: String,
    /// For one of a store's people, the store whose conversation it is.
    store: Option<String>,
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
                status: None,
            }),
            other_name: crate::i18n::name(&target.seller_name).to_string(),
            store: None,
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
            status: listing_status(conversation.listing_status.as_deref(), conversation.listing_type.as_deref()),
        });
        Self {
            listing,
            other_name: crate::i18n::name(&conversation.other_name).to_string(),
            store: conversation.store.clone(),
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

/// When a message was sent, by Albania's clock, as of `now`: "14:32" today, "Dje, 14:32", then the
/// date (in English "Yesterday, 14:32", "8 Oct 2026, 14:32").
fn when(sent: OffsetDateTime, now: OffsetDateTime) -> String {
    let time = clock(sent);
    match days_ago(sent, now) {
        0 => time,
        1 => format!("{}, {time}", t("Dje")),
        _ => format!("{}, {time}", day(sent, true)),
    }
}

/// When a conversation's latest message came, as the inbox says it, as of `now`: the time today,
/// "Dje", the day and month within 300 days, or the date.
fn when_latest(sent: OffsetDateTime, now: OffsetDateTime) -> String {
    match days_ago(sent, now) {
        0 => clock(sent),
        1 => t("Dje").to_string(),
        days if days < 300 => day(sent, false),
        _ => day(sent, true),
    }
}

/// How many days before `now` a message was sent, by Tirana's calendar (format::tirana), not by
/// hours: 0 for today there and 1 for yesterday, whatever the hour in UTC.
fn days_ago(sent: OffsetDateTime, now: OffsetDateTime) -> i64 {
    (format::tirana(now).date() - format::tirana(sent).date()).whole_days()
}

/// The time of day a message was sent, by Albania's clock: "09:05".
fn clock(sent: OffsetDateTime) -> String {
    let sent = format::tirana(sent);
    format!("{:02}:{:02}", sent.hour(), sent.minute())
}

/// The day a message was sent, by Albania's clock, with its year or without: "08.10.2026",
/// "08.10"; in English "8 Oct 2026", "8 Oct", as format::date writes a date.
fn day(sent: OffsetDateTime, year: bool) -> String {
    let sent = format::tirana(sent);
    match (english(), year) {
        (false, true) => format!("{:02}.{:02}.{}", sent.day(), u8::from(sent.month()), sent.year()),
        (false, false) => format!("{:02}.{:02}", sent.day(), u8::from(sent.month())),
        (true, true) => sent.format(format_description!("[day padding:none] [month repr:short] [year]")).unwrap_or_default(),
        (true, false) => sent.format(format_description!("[day padding:none] [month repr:short]")).unwrap_or_default(),
    }
}

/// What the pages say of a message in its conversation: `Message`'s own methods in all but name, a
/// trait because the type is pluribus-db's (each method's doc is on its body, below).
pub trait MessageExt {
    fn when(&self) -> String;
}

impl MessageExt for Message {
    fn when(&self) -> String {
        when(self.created_at, OffsetDateTime::now_utc())
    }
}

/// What the pages say of a conversation in the inbox: `InboxEntry`'s own methods in all but name, a
/// trait because the type is pluribus-db's (each method's doc is on its body, below).
pub trait InboxEntryExt {
    fn when(&self) -> String;
    fn preview(&self) -> String;
}

impl InboxEntryExt for InboxEntry {
    /// When its latest message came (`when_latest`): the time today, "Dje", the day and month within
    /// 300 days, or the date.
    fn when(&self) -> String {
        self.last_message_at.map_or_else(String::new, |sent| when_latest(sent, OffsetDateTime::now_utc()))
    }

    /// The start of its latest message, on one line.
    fn preview(&self) -> String {
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

/// Why this message can't go, if it can't (messaging::refuse, given what its sender sent lately),
/// in the page's language: `other` is the other side's name, which some of the reasons say.
async fn refusal(state: &AppState, user: &SessionUser, writing: Writing, message: Outgoing<'_>, other: &str) -> Result<Option<String>> {
    let sent = db::sent_lately(&state.db, user.id).await?;
    Ok(messaging::refuse(writing, &message, &sent).map(|refusal| reason(refusal, other)))
}

/// What a refusal says, in the page's language, naming the other side (`other`) where it does.
fn reason(refusal: Refusal, other: &str) -> String {
    match refusal {
        Refusal::Blocked => t("E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.").to_string(),
        Refusal::BlockedBy => t1("{} nuk pranon mesazhe nga ti.", other),
        Refusal::Closed => t("Ky njoftim nuk pranon mesazhe tani.").to_string(),
        Refusal::Empty => t("Shkruaj mesazhin.").to_string(),
        Refusal::TooLong { most } => t1("Mesazhi mund të ketë deri në {} shkronja.", most),
        Refusal::TooManyInARow { .. } => t1("Prit që {} të të përgjigjet para se të shkruash më shumë.", other),
        Refusal::LinksBeforeAnswer => t1("Lidhjet mund t'i dërgosh pasi {} të të përgjigjet.", other),
        Refusal::TooFast { .. } => t("Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.").to_string(),
        Refusal::TooManyToday { .. } => t("Ke dërguar shumë mesazhe sot. Provo përsëri nesër.").to_string(),
        Refusal::NewSellersThisHour { most } => t1("U ke shkruar {} shitësve këtë orë. Prit pak para se t'i shkruash një tjetri.", most),
        Refusal::NewSellersToday { most } => t1("U ke shkruar {} shitësve sot. Mund t'u shkruash të tjerëve nesër.", most),
    }
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
    if target.own {
        return Ok(Redirect::to(&format!("/listing/{}-{}", target.public_no, target.slug)).into_response());
    }
    let (blocked, blocked_by) = db::blocks_with_seller(&state.db, user.id, &target).await?;
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
    if target.own {
        return Ok(Redirect::to(&format!("/listing/{}-{}", target.public_no, target.slug)).into_response());
    }
    let (blocked, blocked_by) = db::blocks_with_seller(&state.db, user.id, &target).await?;
    let writing = Writing::of(target.open, blocked, blocked_by);
    let body = tidy(&form.body);
    let message = Outgoing { body: &body, links: link_count(&body), unanswered: Some(0), new_seller: true };
    if let Some(error) = refusal(&state, &user, writing, message, &target.seller_name).await? {
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

/// The conversation with this id in the address, for someone who may see it (its buyer, its
/// seller, or one of the store's people).
async fn find(state: &AppState, user: &SessionUser, id: &str) -> Result<Option<Conversation>> {
    let Ok(id) = id.parse::<Uuid>() else {
        return Ok(None);
    };
    Ok(db::conversation(&state.db, id, user.id).await?)
}

/// GET /mesazhet/{id}: the conversation, for those who may see it; "not found" for anyone else.
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
    let message = Outgoing { body: &body, links: link_count(&body), unanswered, new_seller: false };
    if let Some(error) = refusal(state, user, writing, message, &conversation.other_name).await? {
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
    render_stream(NewMessages { other_name: crate::i18n::name(&conversation.other_name).to_string(), messages })
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
        && db::may_see(&state.db, id, user.id).await?
    {
        db::mark_read(&state.db, id, user.id, deri).await?;
    }
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// POST /mesazhet/{id}/blloko and /zhblloko: blocks the other side of the conversation, who can't
/// write to the visitor any more, nor start another conversation with them; or lets them again,
/// lifting a colleague's block too on a store's side.
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
    db::set_conversation_blocked(&state.db, conversation.id, user.id, conversation.other_id, on).await?;
    Ok(Redirect::to(&format!("/mesazhet/{}", conversation.id)).into_response())
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn times_say_today_yesterday_or_the_date() {
        // 9:05 on the 8th of October in Tirana, by its summer time (UTC+2).
        let sent = datetime!(2026-10-08 7:05 UTC);
        assert_eq!(when(sent, datetime!(2026-10-08 18:00 UTC)), "09:05");
        assert_eq!(when(sent, datetime!(2026-10-09 18:00 UTC)), "Dje, 09:05");
        assert_eq!(when(sent, datetime!(2026-10-11 18:00 UTC)), "08.10.2026, 09:05");
    }

    #[test]
    fn today_and_yesterday_are_tiranas_days() {
        // Half past midnight on the 10th in Tirana (UTC+2) is 22:30 UTC on the 9th.
        let now = datetime!(2026-10-09 22:30 UTC);
        // Ten past midnight there is today; ten to, forty minutes earlier, yesterday; and ten to
        // midnight on the 8th, under 25 hours before, two days ago.
        assert_eq!(when(datetime!(2026-10-09 22:10 UTC), now), "00:10");
        assert_eq!(when(datetime!(2026-10-09 21:50 UTC), now), "Dje, 23:50");
        assert_eq!(when(datetime!(2026-10-08 21:50 UTC), now), "08.10.2026, 23:50");
        // The inbox says the same days shorter.
        assert_eq!(when_latest(datetime!(2026-10-09 22:10 UTC), now), "00:10");
        assert_eq!(when_latest(datetime!(2026-10-09 21:50 UTC), now), "Dje");
        assert_eq!(when_latest(datetime!(2026-10-08 21:50 UTC), now), "08.10");
        // In winter (UTC+1), half past midnight on the 5th of December is 23:30 UTC on the 4th.
        let now = datetime!(2026-12-04 23:30 UTC);
        assert_eq!(when(datetime!(2026-12-04 23:05 UTC), now), "00:05");
        assert_eq!(when(datetime!(2026-12-04 22:59 UTC), now), "Dje, 23:59");
        // The inbox gives the year from 300 of Tirana's days before on: the 9th of February is
        // 299 days before the 5th of December, the 8th 300.
        assert_eq!(when_latest(datetime!(2026-02-09 12:00 UTC), now), "09.02");
        assert_eq!(when_latest(datetime!(2026-02-08 12:00 UTC), now), "08.02.2026");
    }

    #[test]
    fn messages_go_without_space_around_them() {
        assert_eq!(tidy("  Përshëndetje!\r\nA është ende?  \n"), "Përshëndetje!\nA është ende?");
    }

    #[test]
    fn each_refusal_says_why_in_the_pages_words() {
        let said = |refusal| reason(refusal, "Ana");
        assert_eq!(said(Refusal::Blocked), "E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.");
        assert_eq!(said(Refusal::BlockedBy), "Ana nuk pranon mesazhe nga ti.");
        assert_eq!(said(Refusal::Closed), "Ky njoftim nuk pranon mesazhe tani.");
        assert_eq!(said(Refusal::Empty), "Shkruaj mesazhin.");
        assert_eq!(said(Refusal::TooLong { most: MAX_CHARS }), "Mesazhi mund të ketë deri në 1000 shkronja.");
        assert_eq!(said(Refusal::TooManyInARow { most: 3 }), "Prit që Ana të të përgjigjet para se të shkruash më shumë.");
        assert_eq!(said(Refusal::LinksBeforeAnswer), "Lidhjet mund t'i dërgosh pasi Ana të të përgjigjet.");
        assert_eq!(said(Refusal::TooFast { most: 8 }), "Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.");
        assert_eq!(said(Refusal::TooManyToday { most: 200 }), "Ke dërguar shumë mesazhe sot. Provo përsëri nesër.");
        assert_eq!(
            said(Refusal::NewSellersThisHour { most: 5 }),
            "U ke shkruar 5 shitësve këtë orë. Prit pak para se t'i shkruash një tjetri."
        );
        assert_eq!(said(Refusal::NewSellersToday { most: 15 }), "U ke shkruar 15 shitësve sot. Mund t'u shkruash të tjerëve nesër.");
    }
}
