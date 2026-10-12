//! Signing in from the phone app with a code, as the pages sign in, through the one core both go
//! by (pluribus-services' sign_in.rs), within the same limits: `POST /auth/code` sends a code to a
//! phone number or an e-mail address, `POST /auth/verify` takes it back and answers a bearer
//! token, given that once, for a session of the same table as the pages' cookie, a year long on a
//! phone (`sign_in::APP_SESSION_DAYS`), and `POST /auth/sign-out` ends that session. What the core
//! refuses is said in the envelope (`refused`), in the language the request asks for: asking again
//! too soon, or too often, `rate_limited` with `retry_after`; a wrong code `validation` on `code`,
//! saying how many tries are left; a code tried too often `rate_limited`; one that expired or was
//! used `not_found`; an account suspended `forbidden`.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    Json,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{HeaderMap, StatusCode, request::Parts},
};
use pluribus_core::{
    format::Contact,
    i18n::{self, t, t1},
};
use pluribus_db as db;
use pluribus_services::{
    AppState,
    sender::Channel,
    sign_in::{self, Client, Refused},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    ApiError, ApiUser, JsonBody, MeDto, auth,
    openapi::{Forbidden, Internal, NotFound, RateLimited, Unauthorized, Validation, kinds},
};

/// The address the request came from, for the limits, as the pages read theirs
/// (`sign_in::client_ip`): the connection's own, or behind a proxy on this machine (cloudflared's
/// tunnel) the last address it added to X-Forwarded-For.
pub struct ClientIp(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
        let forwarded = parts.headers.get("x-forwarded-for").and_then(|value| value.to_str().ok());
        Ok(Self(sign_in::client_ip(peer, forwarded)))
    }
}

/// `POST /auth/code`'s body: a phone number or an e-mail address, as people type them ("069 123
/// 4567", "Ana@Example.com"), and the name for an account it makes, if it makes one.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CodeRequest {
    #[schema(examples("069 123 4567"))]
    pub contact: String,
    #[serde(default)]
    #[schema(examples("Ana"))]
    pub name: Option<String>,
}

/// A code sent (`POST /auth/code`).
#[derive(Debug, Serialize, ToSchema)]
pub struct CodeSentDto {
    /// What the code waits under, which `/auth/verify` takes back with it.
    pub code_id: Uuid,
    /// Where it went, as the pages show it: "+355 69 *** **67", "a***@example.com".
    #[schema(examples("+355 69 *** **67"))]
    pub sent_to: String,
    /// How it went: "whatsapp", "email", or "log" where its channel isn't set up (as in
    /// development, where the server's log has it).
    #[schema(value_type = kinds::Channel)]
    pub via: &'static str,
    /// In test mode alone (`SIGN_IN_CODES_ON_SCREEN`), the code itself, as the pages show it
    /// then; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[schema(examples("123456"))]
    pub code: Option<String>,
}

/// `POST /auth/verify`'s body: the code's id and the code as typed (its digits are what count);
/// what is signing in, "ios", "android" or "web"; and the phone's name, if the app gives one,
/// kept with the session (its first 80 characters).
#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyRequest {
    pub code_id: Uuid,
    #[schema(examples("123456"))]
    pub code: String,
    #[schema(value_type = kinds::Client)]
    pub client: String,
    #[serde(default)]
    #[schema(examples("Pixel 9"))]
    pub device_name: Option<String>,
}

/// Signed in (`POST /auth/verify`).
#[derive(Debug, Serialize, ToSchema)]
pub struct SignedInDto {
    /// The bearer token, given this once: the database keeps its hash alone.
    pub token: String,
    /// Who signed in, as `/me` gives them.
    pub user: MeDto,
    /// Whether the account is new, made by this code.
    pub first_time: bool,
}

/// `POST /auth/code`: a code to the contact, by the state's senders, as the pages send one; 422 on
/// `contact` for one that is neither a mobile number nor an e-mail address.
#[utoipa::path(
    post,
    path = "/auth/code",
    tag = "signin",
    operation_id = "requestCode",
    summary = "Send a sign-in code",
    description = "Sends a sign-in code to a phone number or an e-mail address, as the pages send one and within their limits.",
    request_body = CodeRequest,
    responses(
        (status = 200, description = "The code went out", body = CodeSentDto),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn code(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    JsonBody(body): JsonBody<CodeRequest>,
) -> Result<Json<CodeSentDto>, ApiError> {
    let Some(contact) = Contact::parse(&body.contact) else {
        let message = t("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.");
        return Err(ApiError::Validation(vec![("contact", message.to_string())]));
    };
    let requested = sign_in::request(&state, &contact, ip.as_deref(), body.name.as_deref()).await?.map_err(refused)?;
    Ok(Json(CodeSentDto { code_id: requested.code_id, sent_to: contact.masked(), via: channel(requested.via), code: requested.shown }))
}

/// `POST /auth/verify`: the code back. Right, the account of its contact is signed in, made if it
/// had none, for the client named (422 on `client` for another), and the language the request
/// asks for (lang.rs) is the account's now, its e-mails' too.
#[utoipa::path(
    post,
    path = "/auth/verify",
    tag = "signin",
    operation_id = "verifyCode",
    summary = "Sign in with the code",
    description = "Takes the code back and signs the account of its contact in, made if it had none, giving the bearer token once; the language the request asks for becomes the account's.",
    request_body = VerifyRequest,
    responses(
        (status = 200, description = "Signed in: the token, given this once, and the user", body = SignedInDto),
        (status = 403, response = Forbidden),
        (status = 404, response = NotFound),
        (status = 422, response = Validation),
        (status = 429, response = RateLimited),
        (status = 500, response = Internal),
    ),
)]
pub async fn verify(State(state): State<AppState>, JsonBody(body): JsonBody<VerifyRequest>) -> Result<Json<SignedInDto>, ApiError> {
    let Some(client) = Client::parse(&body.client) else {
        return Err(ApiError::Validation(vec![("client", t("Duhet të jetë „ios“, „android“ ose „web“.").to_string())]));
    };
    let lang = Some(i18n::lang());
    let signed = sign_in::complete(&state, body.code_id, &body.code, lang, client, body.device_name.as_deref())
        .await?
        .map_err(refused)?;
    let account = db::account_settings(&state.db, signed.user.id).await?;
    Ok(Json(SignedInDto { token: signed.token, user: MeDto::from((&signed.user, &account)), first_time: signed.first_time }))
}

/// `POST /auth/sign-out`, signed in: the session the bearer token names ends, as signing out of the
/// pages ends theirs: 204.
#[utoipa::path(
    post,
    path = "/auth/sign-out",
    tag = "signin",
    operation_id = "signOut",
    summary = "Sign out",
    description = "Ends the session the bearer token names, as signing out of the pages ends theirs: the token signs nothing in after.",
    security(("bearer" = [])),
    responses(
        (status = 204, description = "Signed out"),
        (status = 401, response = Unauthorized),
        (status = 500, response = Internal),
    ),
)]
pub async fn sign_out(State(state): State<AppState>, ApiUser(_): ApiUser, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    if let Some(token) = auth::bearer(&headers) {
        sign_in::sign_out(&state, token).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// What the core refused, in the envelope, said in the request's language: the limits on asking
/// as the pages say them, with the seconds to wait (`retry_after`); a code the channel refused
/// (`validation` on `contact`); and what checking one refuses. account.rs says so too, for the
/// codes that add a contact to the account (`/me/contacts`).
pub(crate) fn refused(refused: Refused) -> ApiError {
    match refused {
        Refused::TooSoon { retry_after } => ApiError::RateLimited
            .said(t("Të dërguam një kod pak më parë. Prit një minutë para se të kërkosh një tjetër."))
            .retry_after(retry_after),
        Refused::TooManyCodes { retry_after } => {
            ApiError::RateLimited.said(t("Shumë kërkesa për kode. Provo përsëri më vonë.")).retry_after(retry_after)
        }
        Refused::NotSent => {
            let message = t("Nuk arritëm ta dërgojmë kodin. Kontrollo numrin ose email-in dhe provo përsëri pas pak.");
            ApiError::Validation(vec![("contact", message.to_string())]).said(message)
        }
        Refused::NoSuchCode => ApiError::NotFound.said(t("Ky kod skadoi ose u përdor. Kërko një kod të ri.")),
        Refused::WrongCode { attempts_left } => {
            let message = match attempts_left {
                0 => t("Kodi nuk është i saktë, dhe kjo ishte prova e fundit. Kërko një kod të ri.").to_string(),
                1 => t("Kodi nuk është i saktë. Të ka mbetur edhe një provë.").to_string(),
                left => t1("Kodi nuk është i saktë. Të kanë mbetur {} prova.", left),
            };
            ApiError::Validation(vec![("code", message.clone())]).said(message)
        }
        Refused::TooManyAttempts => ApiError::RateLimited.said(t("Kodi u provua shumë herë. Kërko një kod të ri.")),
        Refused::Suspended => ApiError::Forbidden.said(t("Kjo llogari është pezulluar.")),
    }
}

/// How a code went, as the app reads it (here, and in account.rs).
pub(crate) fn channel(via: Channel) -> &'static str {
    match via {
        Channel::WhatsApp => "whatsapp",
        Channel::Email => "email",
        Channel::Log => "log",
    }
}
