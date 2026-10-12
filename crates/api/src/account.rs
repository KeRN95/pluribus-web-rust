//! The account's settings, as the phone app changes them, each as "Cilësimet" (pluribus-web's
//! settings.rs) changes it and by the same functions: its name (`PATCH /me`); what it hears of
//! (`GET` and `PUT /me/notices`); the language of its e-mails (`PUT /me/language`); its phone
//! number and its e-mail address, either added or changed by a code sent to the new one, through
//! the core signing in goes by and within its limits (pluribus-services' sign_in.rs:
//! `POST /me/contacts`, `POST /me/contacts/verify`), and either taken off while the other stays
//! (`DELETE /me/contacts/{kind}`); the devices signed in to it, the app's and the pages' alike, one
//! table holding both, and signing out of all but the one asking (`GET /me/sessions`,
//! `DELETE /me/sessions/others`); and closing it for good (`DELETE /me`). Every route is signed in
//! (`ApiUser`); a change answers the user as `/me` gives them, as they are after it (`me_now`), or
//! 204. What is refused is said as the page says it, in the language the request asks for; a
//! code's refusals as signin.rs says them (`signin::refused`). The shapes they answer in are here,
//! beside their handlers.

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use pluribus_core::{
    format::Contact,
    i18n::{Lang, t},
};
use pluribus_db::{self as db, CodePurpose, Device, NoticePreferences, SessionUser};
use pluribus_services::{
    AppState, hash, media,
    sign_in::{self, NotJoining, Refused},
};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody, MeDto, auth,
    listing::ContactKind,
    openapi::{Internal, NotFound, RateLimited, Unauthorized, Validation, kinds},
    signin::{self, ClientIp, CodeSentDto},
};

/// The user as `/me` gives them (`MeDto`), as their account is now: after a change of theirs here,
/// which the session's row (`ApiUser`, read as the request came in) doesn't know of.
async fn me_now(state: &AppState, id: Uuid) -> Result<Json<MeDto>, ApiError> {
    let account = db::account_settings(&state.db, id).await?;
    let user = SessionUser { id, phone: account.phone.clone(), email: account.email.clone(), display_name: account.display_name.clone() };
    Ok(Json(MeDto::from((&user, &account))))
}

/// A validation error on `field` that says why in its own words, the envelope's message too: what
/// the account's state refuses (a contact it has, another account's, its last), rather than what
/// was typed wrong.
fn refusal(field: &'static str, message: &str) -> ApiError {
    ApiError::Validation(vec![(field, message.to_string())]).said(message)
}

/// `PATCH /me`'s body: the name to show by, as typed.
#[derive(Debug, Deserialize, ToSchema)]
pub struct RenameRequest {
    #[schema(examples("Ana Hoxha"))]
    pub name: String,
}

/// `PATCH /me` `{"name"}`: the name the account shows by, on its listings, its profile and in its
/// conversations (db::rename_account), tidied as "Cilësimet" tidies it (`sign_in::display_name`:
/// its spaces, and its first 60 characters, as many as the page's field holds); one with nothing
/// in it, a validation error on `name`, in the page's words. The user, renamed.
#[utoipa::path(
    patch,
    path = "/me",
    tag = "account",
    operation_id = "renameAccount",
    summary = "Rename the account",
    description = "The name the account shows by, on its listings, its profile and in its conversations, tidied as \"Cilësimet\" tidies it (its spaces, and its first 60 characters).",
    request_body = RenameRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The user, renamed", body = MeDto),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn rename(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<RenameRequest>,
) -> Result<Json<MeDto>, ApiError> {
    let Some(name) = sign_in::display_name(&body.name) else {
        return Err(ApiError::Validation(vec![("name", t("Shkruaj emrin që do të shohin të tjerët.").to_string())]));
    };
    db::rename_account(&state.db, user.id, &name).await?;
    me_now(&state, user.id).await
}

/// What the account hears of (`GET` and `PUT /me/notices`; db::NoticePreferences), named as
/// "Cilësimet" names its boxes: the messages left unread ("Mesazhet që nuk i ke lexuar"), the
/// prices that drop of the listings it keeps ("Çmimet që bien"), its saved searches' new listings
/// ("Kërkimet e ruajtura"), and whether by e-mail too ("Edhe me email"), which only an account
/// with an address has a box for. All on until it says otherwise.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct NoticesDto {
    pub messages: bool,
    pub price_drops: bool,
    pub searches: bool,
    pub email: bool,
}

impl From<NoticePreferences> for NoticesDto {
    fn from(preferences: NoticePreferences) -> Self {
        Self {
            messages: preferences.message_alerts,
            price_drops: preferences.price_drop_alerts,
            searches: preferences.saved_search_alerts,
            email: preferences.email_enabled,
        }
    }
}

/// `GET /me/notices`: what the account hears of (`NoticesDto`).
#[utoipa::path(
    get,
    path = "/me/notices",
    tag = "account",
    operation_id = "getNotices",
    summary = "What the account hears of",
    description = "What the account hears of, as \"Cilësimet\" names its boxes: the messages left unread, the prices that drop of the listings it keeps, its saved searches' new listings, and whether by e-mail too.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "What the account hears of", body = NoticesDto),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn notices(State(state): State<AppState>, ApiUser(user): ApiUser) -> Result<Json<NoticesDto>, ApiError> {
    Ok(Json(NoticesDto::from(db::notice_preferences(&state.db, user.id).await?)))
}

/// `PUT /me/notices`, the shape `GET` gives: what the account hears of, as the boxes say
/// (db::set_notice_preferences); `email` only where it has an address, as the page shows that box
/// only then, the setting otherwise staying as it was. 204.
#[utoipa::path(
    put,
    path = "/me/notices",
    tag = "account",
    operation_id = "setNotices",
    summary = "Set what the account hears of",
    description = "Sets what the account hears of, in the shape `GET` gives; `email` only for an account with an e-mail address, as the page shows that box only then.",
    request_body = NoticesDto,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Set"),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn set_notices(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(wanted): JsonBody<NoticesDto>,
) -> Result<StatusCode, ApiError> {
    let email_enabled = match user.email {
        Some(_) => wanted.email,
        None => db::notice_preferences(&state.db, user.id).await?.email_enabled,
    };
    let preferences = NoticePreferences {
        message_alerts: wanted.messages,
        price_drop_alerts: wanted.price_drops,
        saved_search_alerts: wanted.searches,
        email_enabled,
    };
    db::set_notice_preferences(&state.db, user.id, preferences).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `PUT /me/language`'s body: `sq` or `en`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct LanguageRequest {
    #[schema(value_type = kinds::Language)]
    pub lang: String,
}

/// `PUT /me/language` `{"lang"}`: the account's language, which its e-mails are written in, as the
/// language row of "Cilësimet" keeps it on the account (db::set_preferred_language), and which
/// `/me` gives (`preferred_language`); the API answers each request in the language it asks for
/// (lang.rs). Another than `sq` or `en`, a validation error on `lang`. 204.
#[utoipa::path(
    put,
    path = "/me/language",
    tag = "account",
    operation_id = "setLanguage",
    summary = "Set the account's language",
    description = "The account's language, which its e-mails are written in and `/me` gives as `preferred_language`; the API answers each request in the language it asks for, whatever the account's.",
    request_body = LanguageRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Set"),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn language(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<LanguageRequest>,
) -> Result<StatusCode, ApiError> {
    let Some(lang) = Lang::from_code(&body.lang) else {
        return Err(ApiError::Validation(vec![("lang", t("Duhet të jetë „sq“ ose „en“.").to_string())]));
    };
    db::set_preferred_language(&state.db, user.id, lang.code()).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /me/contacts`' body: a phone number or an e-mail address, as people type them.
#[derive(Debug, Deserialize, ToSchema)]
pub struct ContactRequest {
    #[schema(examples("ana@example.com"))]
    pub contact: String,
}

/// `POST /me/contacts` `{"contact"}`: a contact to add to the account, or to put in place of the
/// one of its kind, as "Cilësimet" adds one: not one the account has already, nor another
/// account's (`sign_in::may_join`); within the limits signing in has (`sign_in::limit`), a code
/// goes to it, by the state's senders (`sign_in::send`), carrying the account it adds to
/// (`CodePurpose::adds_to`), which signs no one in (`/auth/verify` finds none) and puts the contact
/// on the account once it comes back (`verify_contact`). Answered as `/auth/code` answers
/// (`CodeSentDto`). A contact that is neither a mobile number nor an e-mail address, the account's
/// own or another account's, a validation error on `contact`, in the page's words; a code asked
/// for too soon or too often, or one its channel refused, as `/auth/code` says it.
#[utoipa::path(
    post,
    path = "/me/contacts",
    tag = "account",
    operation_id = "addContact",
    summary = "Add a phone number or an e-mail address",
    description = "Sends a code to a phone number or an e-mail address to add to the account, or to put in place of the one of its kind, as \"Cilësimet\" does, within the limits signing in has; the code signs no one in.",
    request_body = ContactRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The code went out", body = CodeSentDto),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn add_contact(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<ContactRequest>,
) -> Result<Json<CodeSentDto>, ApiError> {
    let Some(contact) = Contact::parse(&body.contact) else {
        let message = t("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.");
        return Err(ApiError::Validation(vec![("contact", message.to_string())]));
    };
    if let Err(not) = sign_in::may_join(&state, &user, &contact).await? {
        return Err(refusal(
            "contact",
            match not {
                NotJoining::Own => t("Ky është tashmë i llogarisë sate."),
                NotJoining::Taken => t("Ky është i një llogarie tjetër: hyr me të për ta përdorur."),
            },
        ));
    }
    if let Some(refused) = sign_in::limit(&state, &contact, ip.as_deref()).await? {
        return Err(signin::refused(refused));
    }
    let purpose = CodePurpose { adds_to: Some(user.id), ..Default::default() };
    let requested = sign_in::send(&state, &contact, ip.as_deref(), &purpose).await?.map_err(signin::refused)?;
    Ok(Json(CodeSentDto { code_id: requested.code_id, sent_to: contact.masked(), via: signin::channel(requested.via), code: requested.shown }))
}

/// `POST /me/contacts/verify`'s body: the code's id, as `/me/contacts` gave it, and the code as
/// typed (its digits are what count).
#[derive(Debug, Deserialize, ToSchema)]
pub struct ContactCodeRequest {
    pub code_id: Uuid,
    #[schema(examples("123456"))]
    pub code: String,
}

/// `POST /me/contacts/verify` `{"code_id", "code"}`: the code back (`sign_in::check`), refused as
/// `/auth/verify` refuses one (a wrong one with the tries left, one tried out, one expired or
/// used). Right, its contact joins the account (`sign_in::join`): in place of the one of its kind,
/// and on the account's listings that showed that one, verified as signing in verifies one, as the
/// page's code page puts it on. A code that wasn't asked for here, by this account, adds nothing:
/// as good as none, used up, as `/auth/verify` takes one of these. A contact another account took
/// since, a validation error on `contact`, in the page's words. The user, with it.
#[utoipa::path(
    post,
    path = "/me/contacts/verify",
    tag = "account",
    operation_id = "verifyContact",
    summary = "Confirm a contact by its code",
    description = "Takes the code `/me/contacts` sent back, refused as `/auth/verify` refuses one, and puts its contact on the account, verified, in place of the one of its kind.",
    request_body = ContactCodeRequest,
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The user, with the contact", body = MeDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn verify_contact(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<ContactCodeRequest>,
) -> Result<Json<MeDto>, ApiError> {
    let proved = sign_in::check(&state, body.code_id, &body.code).await?.map_err(signin::refused)?;
    if proved.purpose.adds_to != Some(user.id) {
        return Err(signin::refused(Refused::NoSuchCode));
    }
    if !sign_in::join(&state, user.id, &proved.contact).await? {
        return Err(refusal("contact", t("Ndërkohë ky numër ose email u bë i një llogarie tjetër, ndaj nuk u shtua.")));
    }
    me_now(&state, user.id).await
}

/// `DELETE /me/contacts/{kind}`, `phone` or `email`: takes the account's phone number or its
/// e-mail address off it while it keeps the other, its listings that showed the one taken off
/// showing the other (db::remove_contact), as "Cilësimet" does. A kind that is neither, or one the
/// account hasn't, is not found; the only contact it has, which signing in needs, a validation
/// error on `kind`. The user, without it.
#[utoipa::path(
    delete,
    path = "/me/contacts/{kind}",
    tag = "account",
    operation_id = "removeContact",
    summary = "Take a contact off the account",
    description = "Takes the account's phone number or its e-mail address off it while it keeps the other, its listings that showed the one taken off showing the other.",
    params(("kind" = ContactKind, Path, description = "Which: `phone` or `email`")),
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The user, without it", body = MeDto),
        (status = 401, response = Unauthorized),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn remove_contact(State(state): State<AppState>, ApiUser(user): ApiUser, Path(kind): Path<String>) -> Result<Json<MeDto>, ApiError> {
    let (phone, has) = match kind.as_str() {
        "phone" => (true, user.phone.is_some()),
        "email" => (false, user.email.is_some()),
        _ => return Err(ApiError::NotFound),
    };
    if !has {
        return Err(ApiError::NotFound);
    }
    if !db::remove_contact(&state.db, user.id, phone).await? {
        return Err(refusal("kind", t("Llogaria jote duhet të ketë të paktën një numër ose email, që të hysh me të.")));
    }
    me_now(&state, user.id).await
}

/// A device signed in to the account (`GET /me/sessions`): one of its sessions still good, as
/// "Cilësimet" counts them.
#[derive(Debug, Serialize, ToSchema)]
pub struct SessionDto {
    /// What signed it in: `web`, the pages, or the phone app, `ios` or `android`.
    #[schema(value_type = kinds::Client)]
    pub client: String,
    /// The phone's name, as the app gave it when it signed in; none for the pages.
    #[schema(examples("Pixel 9"))]
    pub device_name: Option<String>,
    /// When it signed in.
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    /// When it was last used, within ten minutes (it moves once ten minutes old); none until a
    /// request used it.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_used_at: Option<OffsetDateTime>,
    /// Whether it is the one asking: the session the bearer token names.
    pub current: bool,
}

impl From<Device> for SessionDto {
    fn from(device: Device) -> Self {
        Self {
            client: device.client,
            device_name: device.device_name,
            created_at: device.created_at,
            last_used_at: device.last_used_at,
            current: device.current,
        }
    }
}

/// The session the request is signed in by, as the database keeps it: its bearer token's hash.
fn this_session(headers: &HeaderMap) -> Result<Vec<u8>, ApiError> {
    auth::bearer(headers).map(hash).ok_or(ApiError::Unauthorized)
}

/// `GET /me/sessions`: the devices signed in to the account (db::devices), the one asking first,
/// then the latest used first.
#[utoipa::path(
    get,
    path = "/me/sessions",
    tag = "account",
    operation_id = "listSessions",
    summary = "The devices signed in",
    description = "The devices signed in to the account, the pages' and the app's alike, the one asking first, then the latest used first.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "The devices signed in", body = Vec<SessionDto>),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn sessions(State(state): State<AppState>, ApiUser(user): ApiUser, headers: HeaderMap) -> Result<Json<Vec<SessionDto>>, ApiError> {
    let here = this_session(&headers)?;
    let devices = db::devices(&state.db, user.id, &here).await?;
    Ok(Json(devices.into_iter().map(SessionDto::from).collect()))
}

/// How many devices `DELETE /me/sessions/others` signed out.
#[derive(Debug, Serialize, ToSchema)]
pub struct EndedDto {
    #[schema(examples(2))]
    pub ended: u64,
}

/// `DELETE /me/sessions/others`: signs the account out everywhere but on the device asking, as
/// "Dil nga pajisjet e tjera" in "Cilësimet" does (db::end_other_sessions): the pages' sessions and
/// the app's alike, so that the page's button ends the app's too. `{"ended"}`, how many were still
/// signed in.
#[utoipa::path(
    delete,
    path = "/me/sessions/others",
    tag = "account",
    operation_id = "endOtherSessions",
    summary = "Sign out everywhere else",
    description = "Signs the account out everywhere but on the device asking, as \"Dil nga pajisjet e tjera\" in \"Cilësimet\" does, the pages' sessions and the app's alike.",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "How many devices were still signed in", body = EndedDto),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn end_other_sessions(State(state): State<AppState>, ApiUser(user): ApiUser, headers: HeaderMap) -> Result<Json<EndedDto>, ApiError> {
    let here = this_session(&headers)?;
    let ended = db::end_other_sessions(&state.db, user.id, &here).await?;
    Ok(Json(EndedDto { ended }))
}

/// `DELETE /me`'s body: the word that says it is meant.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CloseRequest {
    /// FSHI, in any case, or the word of the request's language (DELETE in English).
    #[schema(examples("FSHI"))]
    pub confirm: String,
}

/// `DELETE /me` `{"confirm": "FSHI"}`: closes the account for good, as "Cilësimet" closes it once
/// FSHI is typed (db::delete_account, README's "Users are anonymised, not deleted": its listings
/// deleted with their photos, the stores it owns closed, what only it had gone, its sessions among
/// them, this one too, and the account itself anonymised), its listings' photos taken out of the
/// store after (media::delete_photos). `confirm` is read as the page reads its box: FSHI in any
/// case, spaces aside, or the word the request's language says for it (DELETE in English, as an
/// English page asks for it); anything else, a validation error on `confirm`, nothing closed. 204,
/// and the token signs nothing in after.
#[utoipa::path(
    delete,
    path = "/me",
    tag = "account",
    operation_id = "closeAccount",
    summary = "Close the account",
    description = "Closes the account for good once FSHI is sent, as \"Cilësimet\" closes it: its listings deleted with their photos, the stores it owns closed, its sessions ended (this one too) and the account anonymised.",
    request_body = CloseRequest,
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Closed: the token signs nothing in after"),
        (status = 401, response = Unauthorized),
        (status = 422, response = Validation),
        (status = 500, response = Internal),
    ),
)]
pub async fn close(
    State(state): State<AppState>,
    ApiUser(user): ApiUser,
    JsonBody(body): JsonBody<CloseRequest>,
) -> Result<StatusCode, ApiError> {
    let typed = body.confirm.trim();
    if !typed.eq_ignore_ascii_case("FSHI") && !typed.eq_ignore_ascii_case(t("FSHI")) {
        return Err(ApiError::Validation(vec![("confirm", t("Shkruaj FSHI, që të dimë se e do vërtet.").to_string())]));
    }
    let photos = db::delete_account(&state.db, user.id).await?;
    media::delete_photos(&state.media, &photos).await;
    tracing::info!(user = %user.id, photos = photos.len(), "an account closed itself, from the app");
    Ok(StatusCode::NO_CONTENT)
}
