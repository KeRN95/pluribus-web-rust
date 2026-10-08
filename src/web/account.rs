//! Signing in without a password: a phone number or an e-mail address, a six-digit code sent
//! there, and a session cookie once the code comes back. Every listing is tied to a contact its
//! seller proved is theirs, and the limits below keep one person or one network from asking for
//! codes, or guessing them, by the thousand.

use std::{
    collections::HashMap,
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::{Mutex, OnceLock},
};

use askama::Template;
use axum::{
    Form,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Redirect, Response},
};
use rand::Rng;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Result, cookie, render, saved};
use crate::{
    AppState,
    db::{self, CodeCheck, CodePurpose, SessionUser},
    format::Contact,
    sender::{self, Channel},
};

/// A code works for ten minutes and for five guesses: a million possible codes, five guesses.
const CODE_MINUTES: i32 = 10;
const CODE_ATTEMPTS: i16 = 5;
/// One code a minute for a contact, five an hour; twenty an hour from one network. SMS cost
/// money, so these also cap what a bot can spend.
const CODE_INTERVAL_SECONDS: f64 = 60.0;
const CODES_PER_CONTACT_HOUR: i64 = 5;
const CODES_PER_IP_HOUR: i64 = 20;
/// A person takes longer than this to type a phone number or fill in a listing; most bots don't.
pub const MIN_FORM_SECONDS: i64 = 3;
const SESSION_DAYS: i32 = 30;

const SESSION_COOKIE: &str = "session";
const LOGIN_COOKIE: &str = "login";

pub fn hash(value: &str) -> Vec<u8> {
    Sha256::digest(value.as_bytes()).to_vec()
}

fn code_hash(contact: &str, code: &str) -> Vec<u8> {
    hash(&format!("{contact}:{code}"))
}

pub fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// The visitor's IP address, for the limits. Behind a proxy on this machine (ngrok's agent, a
/// reverse proxy) it is the last address the proxy added to X-Forwarded-For; a direct visitor
/// could write anything in that header, so for them it is the connection's own address.
pub struct ClientIp(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> std::result::Result<Self, Infallible> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
        let forwarded = || {
            let header = parts.headers.get("x-forwarded-for")?.to_str().ok()?;
            header.rsplit(',').next()?.trim().parse::<IpAddr>().ok()
        };
        let ip = match peer {
            Some(ip) if ip.is_loopback() => forwarded().or(Some(ip)),
            Some(ip) => Some(ip),
            None => forwarded(),
        };
        Ok(ClientIp(ip.map(|ip| ip.to_string())))
    }
}

/// A cookie only the server reads, sent over HTTPS only when the visitor came over HTTPS (behind
/// ngrok or a proxy), so plain local development keeps working.
pub fn cookie_header(headers: &HeaderMap, name: &str, value: &str, max_age: i64) -> HeaderValue {
    let https = headers.get("x-forwarded-proto").is_some_and(|proto| proto == "https");
    let secure = if https { "; Secure" } else { "" };
    HeaderValue::from_str(&format!("{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}"))
        .expect("cookie values are ASCII")
}

pub fn redirect_with_cookies(to: &str, cookies: Vec<HeaderValue>) -> Response {
    let mut response = Redirect::to(to).into_response();
    for value in cookies {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// The hash of the session the request carries, if any, as the database keeps it.
pub fn session_hash(headers: &HeaderMap) -> Option<Vec<u8>> {
    cookie(headers, SESSION_COOKIE).map(|token| hash(&token))
}

/// The signed-in user, if the request carries a live session.
pub async fn current_user(state: &AppState, headers: &HeaderMap) -> Result<Option<SessionUser>> {
    let Some(token) = cookie(headers, SESSION_COOKIE) else {
        return Ok(None);
    };
    Ok(db::session_user(&state.db, &hash(&token)).await?)
}

/// Test mode (SIGN_IN_CODES_ON_SCREEN=true in .env): the code page shows the code and fills it
/// in, for trying the site before an SMS or e-mail provider is connected. Anyone can then sign in
/// as any number, so it is off unless switched on, and never for a public site.
fn codes_on_screen() -> bool {
    std::env::var("SIGN_IN_CODES_ON_SCREEN").is_ok_and(|value| value == "true")
}

/// The codes waiting to be shown in test mode, by login code id; only the hash is in the database.
fn shown_codes() -> &'static Mutex<HashMap<Uuid, String>> {
    static CODES: OnceLock<Mutex<HashMap<Uuid, String>>> = OnceLock::new();
    CODES.get_or_init(Default::default)
}

/// " në WhatsApp", " me email" or nothing: how the code page says the code went out.
fn sent_via(contact: &Contact) -> &'static str {
    match sender::senders().channel(contact) {
        Channel::WhatsApp => " në WhatsApp",
        Channel::Email => " me email",
        Channel::Log => "",
    }
}


#[derive(Template)]
#[template(path = "sign_in.html")]
pub struct SignInPage {
    pub contact: String,
    pub name: String,
    pub error: Option<String>,
    pub started: i64,
    /// "Krijo një llogari", with a name, rather than signing in to go on somewhere.
    pub create: bool,
}

impl SignInPage {
    pub fn new() -> Self {
        Self { contact: String::new(), name: String::new(), error: None, started: now(), create: false }
    }
}

#[derive(Deserialize)]
pub struct SignInForm {
    #[serde(default)]
    contact: String,
    #[serde(default)]
    name: String,
    /// Hidden from people (see sign_in.html); a bot filling in every field fills this one too.
    #[serde(default)]
    website: String,
    started: Option<i64>,
}

/// A form shown again with a message; Turbo renders it because of the 422 status.
fn unprocessable(page: impl Template) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

/// A name as typed for an account: spaces tidied, at most 60 letters; None if empty.
pub fn display_name(typed: &str) -> Option<String> {
    let name = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty()).then(|| name.chars().take(60).collect())
}

/// Why no code can go to this contact now, if one can't: one a minute and a few an hour.
pub async fn code_limit(state: &AppState, contact: &Contact, ip: Option<&str>) -> Result<Option<&'static str>> {
    let requests = db::code_requests(&state.db, contact.as_str(), ip).await?;
    if requests.seconds_since_last.is_some_and(|seconds| seconds < CODE_INTERVAL_SECONDS) {
        return Ok(Some("Të dërguam një kod pak më parë. Prit një minutë para se të kërkosh një tjetër."));
    }
    if requests.for_contact >= CODES_PER_CONTACT_HOUR || requests.from_ip >= CODES_PER_IP_HOUR {
        return Ok(Some("Shumë kërkesa për kode. Provo përsëri pas një ore."));
    }
    Ok(None)
}

/// Sends a code to the contact (sender.rs: e-mail, or WhatsApp for a phone) and goes to the page
/// that asks for it. Which code is waiting stays in a cookie rather than the URL, so the contact
/// never shows there. If the code can't go out, the message for the visitor instead.
pub async fn send_code_and_ask(
    state: &AppState,
    headers: &HeaderMap,
    contact: &Contact,
    ip: Option<&str>,
    purpose: &CodePurpose,
) -> Result<std::result::Result<Response, &'static str>> {
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000));
    let id = db::create_login_code(
        &state.db,
        contact.as_str(),
        &code_hash(contact.as_str(), &code),
        ip,
        CODE_MINUTES,
        purpose,
    )
    .await?;
    if let Err(error) = sender::senders().send(contact, &code).await {
        tracing::error!(%error, to = %contact.masked(), "the sign-in code couldn't be sent");
        db::delete_login_code(&state.db, id).await?;
        return Ok(Err("Nuk arritëm ta dërgojmë kodin. Kontrollo numrin ose email-in dhe provo përsëri pas pak."));
    }
    if codes_on_screen() {
        shown_codes().lock().unwrap().insert(id, code);
    }
    let login = cookie_header(headers, LOGIN_COOKIE, &id.to_string(), i64::from(CODE_MINUTES) * 60);
    Ok(Ok(redirect_with_cookies("/hyr/kodi", vec![login])))
}

/// "Llogaria", the button beside the theme toggle: creating an account (or signing in to one),
/// or, for a signed-in visitor, their listings.
pub async fn account_page(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if current_user(&state, &headers).await?.is_some() {
        return Ok(Redirect::to("/njoftimet-e-mia").into_response());
    }
    render(SignInPage { create: true, ..SignInPage::new() })
}

pub async fn request_code(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<SignInForm>,
) -> Result<Response> {
    let again = |error: &str| {
        unprocessable(SignInPage {
            contact: form.contact.clone(),
            name: form.name.clone(),
            error: Some(error.to_string()),
            started: now(),
            create: !form.name.is_empty(),
        })
    };
    let too_fast = form.started.is_none_or(|started| now() - started < MIN_FORM_SECONDS);
    if !form.website.is_empty() || too_fast {
        return again("Prit një çast dhe provo përsëri.");
    }
    let Some(contact) = Contact::parse(&form.contact) else {
        return again("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.");
    };
    if let Some(limit) = code_limit(&state, &contact, ip.as_deref()).await? {
        return again(limit);
    }
    let purpose = CodePurpose { listing_id: None, display_name: display_name(&form.name) };
    match send_code_and_ask(&state, &headers, &contact, ip.as_deref(), &purpose).await? {
        Ok(response) => Ok(response),
        Err(message) => again(message),
    }
}

#[derive(Template)]
#[template(path = "sign_in_code.html")]
pub struct CodePage {
    sent_to: String,
    /// " në WhatsApp", " me email", or nothing while the channel isn't set up.
    sent_via: &'static str,
    error: Option<String>,
    /// The code can no longer be used: the page offers a new one instead of the form.
    gone: bool,
    /// The code publishes a listing posted before signing in.
    publishes: bool,
    /// In test mode, the code itself.
    test_code: Option<String>,
}

impl CodePage {
    fn gone(sent_to: String) -> Self {
        Self {
            sent_to,
            sent_via: "",
            error: Some("Kodi skadoi ose u provua shumë herë. Kërko një kod të ri.".to_string()),
            gone: true,
            publishes: false,
            test_code: None,
        }
    }
}

fn pending_id(headers: &HeaderMap) -> Option<Uuid> {
    cookie(headers, LOGIN_COOKIE)?.parse().ok()
}

pub async fn code_page(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let pending = match pending_id(&headers) {
        Some(id) => db::pending_login_code(&state.db, id, CODE_ATTEMPTS).await?,
        None => None,
    };
    let Some((contact, publishes)) =
        pending.and_then(|pending| Some((Contact::parse(&pending.contact)?, pending.publishes)))
    else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let test_code = pending_id(&headers).and_then(|id| shown_codes().lock().unwrap().get(&id).cloned());
    render(CodePage { sent_to: contact.masked(), sent_via: sent_via(&contact), error: None, gone: false, publishes, test_code })
}

#[derive(Deserialize)]
pub struct CodeForm {
    #[serde(default)]
    code: String,
}

/// The code typed back: the contact is confirmed, so its account is (created earlier, PENDING,
/// it becomes active), the visitor is signed in, and a listing posted before signing in goes live.
pub async fn check_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CodeForm>,
) -> Result<Response> {
    let Some(id) = pending_id(&headers) else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Some(pending) = db::pending_login_code(&state.db, id, CODE_ATTEMPTS).await? else {
        return unprocessable(CodePage::gone(String::new()));
    };
    let sent_to = Contact::parse(&pending.contact).map(|contact| contact.masked()).unwrap_or_default();
    let typed: String = form.code.chars().filter(char::is_ascii_digit).collect();
    match db::check_login_code(&state.db, id, &code_hash(&pending.contact, &typed), CODE_ATTEMPTS).await? {
        CodeCheck::Signed { contact, purpose } => {
            shown_codes().lock().unwrap().remove(&id);
            let (phone, email) = match Contact::parse(&contact) {
                Some(Contact::Phone(phone)) => (Some(phone), None),
                Some(Contact::Email(email)) => (None, Some(email)),
                None => return Ok(Redirect::to("/llogaria").into_response()),
            };
            let name = purpose.display_name.as_deref();
            let account = db::account_for_contact(&state.db, phone.as_deref(), email.as_deref(), name).await?;
            if account.status == "SUSPENDED" {
                return unprocessable(CodePage {
                    sent_to,
                    sent_via: "",
                    error: Some("Kjo llogari është pezulluar.".to_string()),
                    gone: true,
                    publishes: false,
                    test_code: None,
                });
            }
            db::confirm_account(&state.db, account.id, name).await?;
            // The code came to this number or address, so it is theirs: listings showing it, and
            // their profile, say it is verified.
            db::verify_contact(&state.db, account.id, phone.as_deref(), email.as_deref()).await?;
            let token: String = rand::rng().random::<[u8; 32]>().iter().map(|byte| format!("{byte:02x}")).collect();
            db::create_session(&state.db, &hash(&token), account.id, SESSION_DAYS).await?;
            let session = cookie_header(&headers, SESSION_COOKIE, &token, i64::from(SESSION_DAYS) * 86_400);
            let login = cookie_header(&headers, LOGIN_COOKIE, "", 0);
            let published = match purpose.listing_id {
                Some(listing) => db::publish_draft(&state.db, listing, account.id).await?,
                None => None,
            };
            // A heart tapped before signing in (saved.rs): the listing is kept now, and the page it
            // was tapped on is where the visitor goes back to.
            let (back, done) = saved::after_sign_in(&state, &headers, account.id).await?;
            let to = published.or(back).unwrap_or_else(|| "/njoftimet-e-mia".to_string());
            Ok(redirect_with_cookies(&to, [vec![session, login], done].concat()))
        }
        CodeCheck::Wrong(left) => unprocessable(CodePage {
            sent_via: Contact::parse(&pending.contact).map_or("", |contact| sent_via(&contact)),
            sent_to,
            error: Some(format!(
                "Kodi nuk është i saktë. {}",
                if left == 1 { "Të ka mbetur edhe një provë.".to_string() } else { format!("Të kanë mbetur {left} prova.") }
            )),
            gone: false,
            publishes: pending.publishes,
            test_code: None,
        }),
        CodeCheck::Gone => unprocessable(CodePage::gone(sent_to)),
    }
}

pub async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = cookie(&headers, SESSION_COOKIE) {
        db::delete_session(&state.db, &hash(&token)).await?;
    }
    Ok(redirect_with_cookies("/", vec![cookie_header(&headers, SESSION_COOKIE, "", 0)]))
}
