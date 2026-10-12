//! Messages between buyers and sellers, as the phone app has them, by what the pages have them by
//! (pluribus-web's messages.rs, db's messages.rs), signed in (`ApiUser`): the conversations, the
//! latest written in first, a store's among them for its people, marked with its name
//! (`GET /conversations`); one, with its messages, or only those after one, as the page asks for
//! them every few seconds (`GET /conversations/{id}`); writing to a listing's seller, which starts
//! the conversation, or goes on in the one there is (`POST /listings/{no}/messages`), and writing
//! in a conversation (`POST /conversations/{id}/messages`), within the limits the pages keep
//! (domain::messaging::refuse), each refusal said as the page says it (`refused`); reading it up to
//! a message (`POST /conversations/{id}/read`), as the page's script reads it once it showed them;
//! and blocking the other side, or letting them write again (`POST /conversations/{id}/block`,
//! `/unblock`). A message written here is one like any other: the other side reads it on the
//! pages, and hears of it left unread as of one written there (pluribus-services' notices.rs).
//! Whoever isn't one of a conversation's sides finds nothing there (404), as on the pages. The
//! shapes they answer in are here, beside their handlers.

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use pluribus_core::{
    domain::{
        listing::link_count,
        messaging::{self, Outgoing, Refusal, Writing},
    },
    i18n::{self, t},
};
use pluribus_db::{self as db, Conversation, InboxEntry, Message, SessionUser, Sent};
use pluribus_services::AppState;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody, browse,
    openapi::{Internal, NotFound, RateLimited, Unauthorized, Validation},
};

/// How many conversations the inbox gives, the latest first: as many as the pages' inbox shows
/// (pluribus-web's INBOX_MOST).
const INBOX_MOST: i64 = 200;

/// How long a message sent too fast waits, in seconds: the minute whose messages count
/// (messaging::PER_MINUTE), as its words say ("Prit një minutë").
const TOO_FAST_WAIT: u64 = 60;

/// The kind of every message a conversation shows (db::messages): what someone wrote.
const TEXT: &str = "TEXT";

/// The listing a conversation is about, as its head and the inbox show it (pluribus-web's
/// ListingHead).
#[derive(Debug, Serialize, ToSchema)]
pub struct ListingHeadDto {
    /// Its number, which its page goes by.
    #[schema(examples(1001))]
    pub no: i64,
    pub title: String,
    /// Its cover's card-sized copy, its address absolute; none for a listing without photos.
    pub photo: Option<String>,
    /// What became of it, once it is no longer simply up, in the language asked for: "Rezervuar",
    /// "Shitur" ("Dhënë me qira" for a rental), "Jo më në faqe" (messaging::listing_label); none
    /// while it is up.
    pub status_label: Option<String>,
}

impl ListingHeadDto {
    /// The head of a listing still there (`no`), from what its row has of it, its photo's address
    /// absolute from `origin`; none once it is gone for good.
    fn new(no: Option<i64>, title: Option<String>, photo: Option<String>, status: Option<&str>, kind: Option<&str>, origin: &str) -> Option<Self> {
        Some(Self {
            no: no?,
            title: title.unwrap_or_default(),
            photo: photo.map(|photo| browse::absolute(origin, &photo)),
            status_label: messaging::listing_label(status, kind).map(str::to_string),
        })
    }
}

/// The other side of a conversation.
#[derive(Debug, Serialize, ToSchema)]
pub struct OtherDto {
    /// A person's account, or a store's, by its id.
    pub id: Uuid,
    pub kind: OtherKind,
    /// Their name as the pages show it: as they gave it, or the one the site gives in the language
    /// asked for ("Përdorues", "User", until they give one); a store's own.
    #[schema(examples("Besnik"))]
    pub name: String,
    /// A store's address, its page's (/biznes/{slug}); none for a person.
    #[schema(examples("auto-durresi"))]
    pub slug: Option<String>,
}

/// Who the other side of a conversation is: a person (user), or a store (business).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum OtherKind {
    /// A person: the buyer, as the seller's side sees them, or a private seller.
    User,
    /// A store, as the buyer of its listing sees it, whichever of its people answers.
    Business,
}

impl OtherDto {
    /// The other side as a row has them: their account and their name, or for the buyer of a
    /// store's listing, the store (`store`, its id and its address).
    fn new(id: Uuid, name: &str, store: Option<Uuid>, slug: Option<String>) -> Self {
        let name = i18n::name(name).to_string();
        match store {
            Some(store) => Self { id: store, kind: OtherKind::Business, name, slug },
            None => Self { id, kind: OtherKind::User, name, slug: None },
        }
    }
}

/// The store whose conversation it is, for one of its people: its address and its name.
#[derive(Debug, Serialize, ToSchema)]
pub struct StoreDto {
    #[schema(examples("auto-durresi"))]
    pub slug: String,
    #[schema(examples("Auto Durrësi"))]
    pub name: String,
}

impl StoreDto {
    fn new(name: Option<String>, slug: Option<String>) -> Option<Self> {
        name.zip(slug).map(|(name, slug)| Self { slug, name })
    }
}

/// A conversation as the inbox lists it (`GET /conversations`).
#[derive(Debug, Serialize, ToSchema)]
pub struct InboxDto {
    pub id: Uuid,
    /// The listing it is about; none once it is gone for good.
    pub listing: Option<ListingHeadDto>,
    pub other: OtherDto,
    /// For one of a store's people, the store whose conversation it is, which the inbox names
    /// beside the buyer; none otherwise.
    pub store: Option<StoreDto>,
    /// When its latest message came.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_message_at: Option<OffsetDateTime>,
    /// Its latest message as the inbox shows it: its start on one line (messaging::preview), after
    /// "Ti: " ("You: ") where the user wrote it, or a colleague's name where another of the store's
    /// people did.
    pub preview: String,
    /// How many messages from the other side the user hasn't read.
    pub unread: u32,
    /// Whether the user's side blocked the other.
    pub blocked: bool,
}

impl InboxDto {
    fn new(entry: InboxEntry, origin: &str) -> Self {
        let start = messaging::preview(entry.last_body.as_deref().unwrap_or_default());
        let preview = match (&entry.last_author, entry.last_mine) {
            (Some(author), _) => format!("{}: {start}", i18n::name(author)),
            (None, true) => format!("{}: {start}", t("Ti")),
            (None, false) => start,
        };
        let listing = ListingHeadDto::new(
            entry.public_no,
            entry.listing_title,
            entry.photo,
            entry.listing_status.as_deref(),
            entry.listing_type.as_deref(),
            origin,
        );
        Self {
            id: entry.id,
            listing,
            other: OtherDto::new(entry.other_id, &entry.other_name, entry.other_store_id, entry.other_store_slug),
            store: StoreDto::new(entry.store, entry.store_slug),
            last_message_at: entry.last_message_at,
            preview,
            unread: u32::try_from(entry.unread).unwrap_or(u32::MAX),
            blocked: entry.blocked,
        }
    }
}

/// A conversation, with its messages (`GET /conversations/{id}`).
#[derive(Debug, Serialize, ToSchema)]
pub struct ConversationDto {
    pub id: Uuid,
    /// The listing it is about; none once it is gone for good.
    pub listing: Option<ListingHeadDto>,
    pub other: OtherDto,
    /// For one of a store's people, the store whose conversation it is ("Për Auto Durrësi" on the
    /// page); none otherwise.
    pub store: Option<StoreDto>,
    /// Whether the user's side blocked the other, who can't write to them any more, nor the user to
    /// them until they let them again (`/unblock`)...
    pub blocked_by_me: bool,
    /// ...and whether the other side blocked the user's.
    pub blocked_me: bool,
    /// Whether the listing takes no new conversations now: down, sold, off the site, its messages
    /// turned off, or gone. This one goes on all the same, as on the pages, the listing's
    /// `status_label` saying what became of it.
    pub closed: bool,
    /// Whether the user may write in it now: neither side blocked the other...
    pub can_write: bool,
    /// ...and where they may not, why, as a message sent anyway would be refused: its code
    /// ("blocked", "blocked_by") and what it says, in the language asked for.
    pub why_not: Option<WhyNotDto>,
    /// Its messages, the oldest first: its latest 300, or with `after` those after that one.
    pub messages: Vec<MessageDto>,
}

/// Why a message can't go (domain::messaging::Refusal): its code, the same in every language
/// (`Refusal::code`), and what it says (`Refusal::reason`).
#[derive(Debug, Serialize, ToSchema)]
pub struct WhyNotDto {
    #[schema(examples("blocked"))]
    pub code: &'static str,
    pub message: String,
}

impl ConversationDto {
    fn new(conversation: Conversation, messages: Vec<Message>, origin: &str) -> Self {
        // A conversation once started takes messages, whatever became of its listing (`closed`):
        // only a block stops them, as on the pages.
        let writing = Writing::of(true, conversation.blocked, conversation.blocked_by);
        let other = OtherDto::new(conversation.other_id, &conversation.other_name, conversation.other_store_id, conversation.other_store_slug);
        let why_not = writing.refusal().map(|refusal| WhyNotDto { code: refusal.code(), message: refusal.reason(&other.name) });
        let listing = ListingHeadDto::new(
            conversation.public_no.filter(|_| conversation.slug.is_some()),
            conversation.title,
            conversation.photo,
            conversation.listing_status.as_deref(),
            conversation.listing_type.as_deref(),
            origin,
        );
        Self {
            id: conversation.id,
            listing,
            other,
            store: StoreDto::new(conversation.store, conversation.store_slug),
            blocked_by_me: conversation.blocked,
            blocked_me: conversation.blocked_by,
            closed: !conversation.open,
            can_write: writing == Writing::Open,
            why_not,
            messages: messages.into_iter().map(MessageDto::from).collect(),
        }
    }
}

/// A message, as a conversation shows it.
#[derive(Debug, Serialize, ToSchema)]
pub struct MessageDto {
    /// A UUIDv7, which sorts as it was sent: what `after` and `up_to` name it by.
    pub id: Uuid,
    /// Whether the user's side sent it: they, or another of the store's people they are one of.
    pub mine: bool,
    /// What kind of message it is, as the database spells it: TEXT, what someone wrote, the only
    /// kind a conversation shows (the schema has room for others, SYSTEM among them, the site's
    /// own, which nothing writes yet).
    #[schema(examples("TEXT"))]
    pub kind: &'static str,
    /// For one of a store's people, the colleague who sent it, where it wasn't them; none
    /// otherwise.
    pub author: Option<String>,
    /// What it says, as it was sent.
    pub body: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
}

impl From<Message> for MessageDto {
    fn from(message: Message) -> Self {
        Self {
            id: message.id,
            mine: message.mine,
            kind: TEXT,
            author: message.author.map(|author| i18n::name(&author).to_string()),
            body: message.body,
            created_at: message.created_at,
        }
    }
}

impl MessageDto {
    /// The message the user just wrote, saying `body`.
    fn sent(sent: Sent, body: String) -> Self {
        Self { id: sent.id, mine: true, kind: TEXT, author: None, body, created_at: sent.created_at }
    }
}

/// A message written to a listing's seller (`POST /listings/{no}/messages`): the message's own
/// members, and the conversation it went into beside them.
#[derive(Debug, Serialize, ToSchema)]
pub struct SentDto {
    pub conversation_id: Uuid,
    #[serde(flatten)]
    pub message: MessageDto,
}

/// What the routes that write take: `{"body": "…"}`, the message, which goes without space
/// around it (messaging::tidy).
#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageBody {
    #[schema(examples("Përshëndetje, a është ende në shitje?"))]
    pub body: String,
}

/// What `POST /conversations/{id}/read` takes: the latest message the app showed, by its id.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ReadBody {
    pub up_to: Uuid,
}

/// The query of `GET /conversations/{id}`.
#[derive(Debug, Deserialize)]
pub struct ConversationQuery {
    /// The last message the app has, by its id: only those after it.
    pub after: Option<Uuid>,
}

/// `GET /conversations`: the user's conversations, the latest written in first, as /mesazhet
/// lists them (db::inbox), a store's among them for its people, INBOX_MOST at most.
#[utoipa::path(
    get,
    path = "/conversations",
    tag = "messages",
    operation_id = "listConversations",
    summary = "The conversations",
    description = "The user's conversations, the latest written in first, as /mesazhet lists them, a store's among them for its people, 200 at most.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The conversations", body = Vec<InboxDto>),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn inbox(State(state): State<AppState>, ApiUser(user): ApiUser, headers: HeaderMap) -> Result<Json<Vec<InboxDto>>, ApiError> {
    let origin = browse::origin(&state.config, &headers);
    let entries = db::inbox(&state.db, user.id, INBOX_MOST).await?;
    Ok(Json(entries.into_iter().map(|entry| InboxDto::new(entry, &origin)).collect()))
}

/// `GET /conversations/{id}?after=…`: the conversation, with its messages, or only those after
/// `after` (what the app asks for every few seconds, as the page does, the list empty when none
/// came). Showing it reads nothing: the app says what it showed (`/read`), as the page's script
/// does once it shows them. An `after` that isn't a message's id is a validation error on `after`.
#[utoipa::path(
    get,
    path = "/conversations/{id}",
    tag = "messages",
    operation_id = "getConversation",
    summary = "A conversation",
    description = "The conversation with its latest 300 messages, the oldest first, or only those after one, as the page asks for them every few seconds; showing it reads nothing.",
    params(
        ("id" = Uuid, Path, description = "The conversation's id"),
        ("after" = Option<Uuid>, Query, description = "The last message the app has, by its id: only those after it"),
    ),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The conversation", body = ConversationDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn conversation(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    id: Result<Path<Uuid>, PathRejection>,
    headers: HeaderMap,
    query: Result<Query<ConversationQuery>, QueryRejection>,
) -> Result<Json<ConversationDto>, ApiError> {
    let conversation = find(&state, &user, id).await?;
    let Ok(Query(ConversationQuery { after })) = query else {
        let message = t("Vazhdimi i listës nuk lexohet. Fillo përsëri nga faqja e parë.");
        return Err(ApiError::Validation(vec![("after", message.to_string())]));
    };
    let messages = db::messages(&state.db, conversation.id, user.id, after).await?;
    let origin = browse::origin(&state.config, &headers);
    Ok(Json(ConversationDto::new(conversation, messages, &origin)))
}

/// `POST /listings/{no}/messages` `{"body": "…"}`: writes to the listing's seller, as the page that
/// starts a conversation does (pluribus-web's messages.rs, start): into the conversation the user
/// has about it already, within that one's limits; else, within a new one's, to a few new sellers
/// an hour and a day too, starting it (db::start_conversation). 201, with the message and its
/// conversation's id. A listing that takes no messages, the user's own or one off the site, is
/// refused as `Refusal::Closed`; a number no listing has, or a listing deleted, is not found.
#[utoipa::path(
    post,
    path = "/listings/{no}/messages",
    tag = "messages",
    operation_id = "startConversation",
    summary = "Write to a listing's seller",
    description = "Writes to the listing's seller, into the conversation the user has about it already, else starting one, within the limits the pages keep, each refusal said as the page says it.",
    params(("no" = i64, Path, description = "The listing's number", example = 1001)),
    request_body = MessageBody,
    security(("bearer" = [])),
    responses(
        (status = 201, description = "The message, with its conversation's id", body = SentDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn start(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    no: Result<Path<i64>, PathRejection>,
    JsonBody(MessageBody { body }): JsonBody<MessageBody>,
) -> Result<(StatusCode, Json<SentDto>), ApiError> {
    let Path(no) = no.map_err(|_| ApiError::NotFound)?;
    let target = db::message_target(&state.db, no, user.id).await?.ok_or(ApiError::NotFound)?;
    let body = messaging::tidy(&body);
    if let Some(id) = target.conversation_id {
        let conversation = db::conversation(&state.db, id, user.id).await?.ok_or(ApiError::NotFound)?;
        let sent = send_into(&state, &user, &conversation, &body).await?;
        return Ok(written(sent, body));
    }
    let seller = i18n::name(&target.seller_name);
    if target.own {
        // Theirs, or their store's: to answer, not to write to.
        return Err(refused(Refusal::Closed, seller));
    }
    let (blocked, blocked_by) = db::blocks_with_seller(&state.db, user.id, &target).await?;
    let writing = Writing::of(target.open, blocked, blocked_by);
    let outgoing = Outgoing { body: &body, links: link_count(&body), unanswered: Some(0), new_seller: true };
    check(&state, &user, writing, &outgoing, seller).await?;
    let sent = db::start_conversation(&state.db, &target, user.id, &body).await?;
    Ok(written(sent, body))
}

/// `POST /conversations/{id}/messages` `{"body": "…"}`: writes in the conversation, as the page
/// does (pluribus-web's messages.rs, reply), within its limits. 201, with the message.
#[utoipa::path(
    post,
    path = "/conversations/{id}/messages",
    tag = "messages",
    operation_id = "sendMessage",
    summary = "Write in a conversation",
    description = "Writes in the conversation, as the page does, within the same limits, refused alike.",
    params(("id" = Uuid, Path, description = "The conversation's id")),
    request_body = MessageBody,
    security(("bearer" = [])),
    responses(
        (status = 201, description = "The message", body = MessageDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn reply(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    id: Result<Path<Uuid>, PathRejection>,
    JsonBody(MessageBody { body }): JsonBody<MessageBody>,
) -> Result<(StatusCode, Json<MessageDto>), ApiError> {
    let conversation = find(&state, &user, id).await?;
    let body = messaging::tidy(&body);
    let sent = send_into(&state, &user, &conversation, &body).await?;
    Ok((StatusCode::CREATED, Json(MessageDto::sent(sent, body))))
}

/// `POST /conversations/{id}/read` `{"up_to": "…"}`: the app showed the conversation's messages up
/// to this one: they are read (db::mark_read), as the page's script reads them, and with them, once
/// nothing from the other side is left unread, the notifications of them; the badges' count of
/// conversations (`/counts`' `messages`) drops with it. 204; a message that isn't the
/// conversation's reads nothing.
#[utoipa::path(
    post,
    path = "/conversations/{id}/read",
    tag = "messages",
    operation_id = "markConversationRead",
    summary = "The messages were shown",
    description = "The app showed the conversation's messages up to this one: they are read, as the page's script reads them, and once nothing from the other side is left unread, the notifications of them too.",
    params(("id" = Uuid, Path, description = "The conversation's id")),
    request_body = ReadBody,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Read"),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn read(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    id: Result<Path<Uuid>, PathRejection>,
    JsonBody(ReadBody { up_to }): JsonBody<ReadBody>,
) -> Result<StatusCode, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    if !db::may_see(&state.db, id, user.id).await? {
        return Err(ApiError::NotFound);
    }
    db::mark_read(&state.db, id, user.id, up_to).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /conversations/{id}/block`: blocks the other side, as the page's "Blloko" does: they can't
/// write to the user any more, here or about another of their listings, nor start another
/// conversation with them, nor the user write to them until they let them again. 204.
#[utoipa::path(
    post,
    path = "/conversations/{id}/block",
    tag = "messages",
    operation_id = "blockConversation",
    summary = "Block the other side",
    description = "Blocks the other side, as the page's \"Blloko\" does: they can't write to the user any more, here or about another of their listings, nor the user to them.",
    params(("id" = Uuid, Path, description = "The conversation's id")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Blocked"),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn block(state: State<AppState>, user: ApiUser, id: Result<Path<Uuid>, PathRejection>) -> Result<StatusCode, ApiError> {
    set_blocked(state, user, id, true).await
}

/// `POST /conversations/{id}/unblock`: lets them write again, as the page's "Zhblloko" does,
/// whoever on a store's side blocked them. 204.
#[utoipa::path(
    post,
    path = "/conversations/{id}/unblock",
    tag = "messages",
    operation_id = "unblockConversation",
    summary = "Let the other side write again",
    description = "Lets the other side write again, as the page's \"Zhblloko\" does, whoever on a store's side blocked them.",
    params(("id" = Uuid, Path, description = "The conversation's id")),
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Let write again"),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 500, response = Internal),
    ),
)]
pub async fn unblock(state: State<AppState>, user: ApiUser, id: Result<Path<Uuid>, PathRejection>) -> Result<StatusCode, ApiError> {
    set_blocked(state, user, id, false).await
}

async fn set_blocked(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    id: Result<Path<Uuid>, PathRejection>,
    on: bool,
) -> Result<StatusCode, ApiError> {
    let conversation = find(&state, &user, id).await?;
    db::set_conversation_blocked(&state.db, conversation.id, user.id, conversation.other_id, on).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The conversation the path names, for someone who may see it (its buyer, its seller, one of the
/// store's people: db::conversation); not found for anyone else, nor for an id that isn't one.
async fn find(state: &AppState, user: &SessionUser, id: Result<Path<Uuid>, PathRejection>) -> Result<Conversation, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::NotFound)?;
    db::conversation(&state.db, id, user.id).await?.ok_or(ApiError::NotFound)
}

/// Writes `body` into the conversation, within its limits, as the page does (pluribus-web's
/// messages.rs, send_into): a block either way first; until the other side answers, three in a
/// row and no links; then what the user sent lately.
async fn send_into(state: &AppState, user: &SessionUser, conversation: &Conversation, body: &str) -> Result<Sent, ApiError> {
    let writing = Writing::of(true, conversation.blocked, conversation.blocked_by);
    let unanswered = (!conversation.answered).then_some(conversation.in_a_row);
    let outgoing = Outgoing { body, links: link_count(body), unanswered, new_seller: false };
    check(state, user, writing, &outgoing, i18n::name(&conversation.other_name)).await?;
    Ok(db::send_message(&state.db, conversation.id, user.id, body).await?)
}

/// The message refused, if it can't go (messaging::refuse, given what the user sent lately:
/// db::sent_lately), naming the other side (`other`, their name as shown) where its words do.
async fn check(state: &AppState, user: &SessionUser, writing: Writing, outgoing: &Outgoing<'_>, other: &str) -> Result<(), ApiError> {
    let sent = db::sent_lately(&state.db, user.id).await?;
    match messaging::refuse(writing, outgoing, &sent) {
        Some(refusal) => Err(refused(refusal, other)),
        None => Ok(()),
    }
}

/// A refusal in the envelope, its message what the page says (`Refusal::reason`): the limits on
/// how much one account sends `rate_limited` (429), a minute's with the minute to wait
/// (`retry_after`); the rest `validation` (422), an empty or too long message on `body` too.
fn refused(refusal: Refusal, other: &str) -> ApiError {
    let message = refusal.reason(other);
    match refusal {
        Refusal::TooFast { .. } => ApiError::RateLimited.said(message).retry_after(TOO_FAST_WAIT),
        Refusal::TooManyToday { .. } | Refusal::NewSellersThisHour { .. } | Refusal::NewSellersToday { .. } => {
            ApiError::RateLimited.said(message)
        }
        Refusal::Empty | Refusal::TooLong { .. } => ApiError::Validation(vec![("body", message.clone())]).said(message),
        Refusal::Blocked | Refusal::BlockedBy | Refusal::Closed | Refusal::TooManyInARow { .. } | Refusal::LinksBeforeAnswer => {
            ApiError::Validation(Vec::new()).said(message)
        }
    }
}

/// 201, with the message just written and its conversation's id.
fn written(sent: Sent, body: String) -> (StatusCode, Json<SentDto>) {
    (StatusCode::CREATED, Json(SentDto { conversation_id: sent.conversation_id, message: MessageDto::sent(sent, body) }))
}
