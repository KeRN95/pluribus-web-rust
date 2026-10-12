//! Signing in without a password, on the pages: a phone number or an e-mail address, a six-digit
//! code sent there, and a session cookie once the code comes back. Every listing is tied to a
//! contact its seller proved is theirs. What signing in is apart from the pages, the limits that
//! keep one person or one network from asking for codes, or guessing them, by the thousand, the
//! code, its check and the session, is pluribus-services' sign_in.rs, which the JSON API signs in
//! by too; here are the pages' forms, their cookies and redirects, the listing posted before
//! signing in put up and the one kept, and what the core refuses said on them, as they always
//! said it.

use std::{
    collections::HashMap,
    convert::Infallible,
    net::SocketAddr,
    sync::{Mutex, OnceLock},
};

use askama::Template;
use axum::{
    Form,
    extract::{ConnectInfo, FromRequestParts, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
// Session tokens are kept as their SHA-256, as the API's bearer tokens are (pluribus-api's
// auth.rs) and the codes are (sign_in.rs): one function for all.
use pluribus_services::{
    hash,
    sign_in::{self, CODE_ATTEMPTS, CODE_MINUTES, Client, Refused, Requested, WEB_SESSION_DAYS},
};
use serde::Deserialize;
use time::OffsetDateTime;
use uuid::Uuid;

use super::{Result, cookie, render, saved, settings};
use crate::{
    AppState,
    db::{self, CodePurpose, SessionUser},
    format::{self, Contact},
    i18n::{Lang, t, t1},
    sender::{Channel, Senders},
};

/// A person takes longer than this to type a phone number or fill in a listing; most bots don't.
pub const MIN_FORM_SECONDS: i64 = 3;

const SESSION_COOKIE: &str = "session";
const LOGIN_COOKIE: &str = "login";

pub fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// The visitor's IP address, for the limits: the connection's own, or behind a proxy on this
/// machine (cloudflared's tunnel, a reverse proxy) the last address it added to X-Forwarded-For,
/// as pluribus-services' `sign_in::client_ip` reads it, which the API reads it by too.
pub struct ClientIp(pub Option<String>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> std::result::Result<Self, Infallible> {
        let peer = parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|info| info.0.ip());
        let forwarded = parts.headers.get("x-forwarded-for").and_then(|value| value.to_str().ok());
        Ok(ClientIp(sign_in::client_ip(peer, forwarded)))
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

/// The Set-Cookie that forgets which code was waiting (LOGIN_COOKIE), once it came back.
pub fn forget_login(headers: &HeaderMap) -> HeaderValue {
    cookie_header(headers, LOGIN_COOKIE, "", 0)
}

/// The Set-Cookie that ends the session in this browser.
pub fn forget_session(headers: &HeaderMap) -> HeaderValue {
    cookie_header(headers, SESSION_COOKIE, "", 0)
}

pub fn redirect_with_cookies(to: &str, cookies: Vec<HeaderValue>) -> Response {
    let mut response = Redirect::to(to).into_response();
    for value in cookies {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

/// The cookie the pages' scripts read to tell whether someone is signed in (signed_in.js): the
/// session's is HttpOnly, out of their reach, and for someone not signed in there is nothing to ask
/// the server for, neither the listings their hearts keep nor unread messages.
pub const SIGNED_IN_COOKIE: &str = "llogari";

/// Keeps SIGNED_IN_COOKIE in step with the session cookie: set on the answer to a request that has
/// a session and not it, which signing in leads to, and cleared on one that has it and no session,
/// after signing out. It says what the session cookie does, not whether the session is still
/// good: one past its time keeps it until the browser lets both go, and the scripts' questions get
/// the empty answers they always did. Static files are left alone, being kept by browsers.
pub async fn signed_in_cookie(request: Request, next: Next) -> Response {
    let session = cookie(request.headers(), SESSION_COOKIE).is_some();
    let marked = cookie(request.headers(), SIGNED_IN_COOKIE).is_some_and(|value| value == "1");
    let https = request.headers().get("x-forwarded-proto").is_some_and(|proto| proto == "https");
    let page = !request.uri().path().starts_with("/static/");
    let mut response = next.run(request).await;
    if session != marked && page {
        let (value, max_age) = if session { ("1", i64::from(WEB_SESSION_DAYS) * 86_400) } else { ("", 0) };
        let secure = if https { "; Secure" } else { "" };
        let cookie = format!("{SIGNED_IN_COOKIE}={value}; Path=/; Max-Age={max_age}; SameSite=Lax{secure}");
        response.headers_mut().append(header::SET_COOKIE, HeaderValue::from_str(&cookie).expect("ASCII"));
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

/// The codes waiting to be shown in test mode (SIGN_IN_CODES_ON_SCREEN=true in the settings,
/// `Config::sign_in_codes_on_screen`), by login code id; only the hash is in the database. Test
/// mode is for trying the site before an SMS or e-mail provider is connected: the code page shows
/// the code and fills it in, so anyone can sign in as any number; it is off unless switched on,
/// and never for a public site.
fn shown_codes() -> &'static Mutex<HashMap<Uuid, String>> {
    static CODES: OnceLock<Mutex<HashMap<Uuid, String>>> = OnceLock::new();
    CODES.get_or_init(Default::default)
}

/// "në WhatsApp", "me email" or nothing: how the code page says the code went out by `senders`
/// (the state's), in the page's language (sign_in_code.html puts it in its sentence).
fn sent_via(senders: &Senders, contact: &Contact) -> &'static str {
    match senders.channel(contact) {
        Channel::WhatsApp => t("në WhatsApp"),
        Channel::Email => t("me email"),
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
    /// "Hyr ose krijo një llogari", with a name, rather than signing in to go on somewhere.
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

/// Why no code can go to this contact now, if one can't: one a minute and a few an hour
/// (`sign_in::limit`). In the page's language, as its callers show it.
pub async fn code_limit(state: &AppState, contact: &Contact, ip: Option<&str>) -> Result<Option<&'static str>> {
    Ok(sign_in::limit(state, contact, ip).await?.map(not_sent))
}

/// Why no code went out (`sign_in::limit`, `sign_in::send`), in the page's language, as the pages
/// have always said it.
fn not_sent(refused: Refused) -> &'static str {
    match refused {
        Refused::TooSoon { .. } => t("Të dërguam një kod pak më parë. Prit një minutë para se të kërkosh një tjetër."),
        Refused::TooManyCodes { .. } => t("Shumë kërkesa për kode. Provo përsëri pas një ore."),
        // The channel refused it (`NotSent`): sending a code refuses nothing else.
        _ => t("Nuk arritëm ta dërgojmë kodin. Kontrollo numrin ose email-in dhe provo përsëri pas pak."),
    }
}

/// Sends a code to the contact by the state's senders (`sign_in::send`: e-mail, or WhatsApp for a
/// phone), its limits seen to by the caller, and goes to the page that asks for it (`ask_for`). If
/// the code can't go out, the message for the visitor instead, in the page's language.
pub async fn send_code_and_ask(
    state: &AppState,
    headers: &HeaderMap,
    contact: &Contact,
    ip: Option<&str>,
    purpose: &CodePurpose,
) -> Result<std::result::Result<Response, &'static str>> {
    Ok(sign_in::send(state, contact, ip, purpose).await?.map(|requested| ask_for(headers, requested)).map_err(not_sent))
}

/// The page that asks for a code sent, which shows the code too in test mode (the state's
/// settings, `sign_in_codes_on_screen`). Which code is waiting stays in a cookie rather than the
/// URL, so the contact never shows there.
fn ask_for(headers: &HeaderMap, requested: Requested) -> Response {
    if let Some(code) = requested.shown {
        shown_codes().lock().unwrap().insert(requested.code_id, code);
    }
    let login = cookie_header(headers, LOGIN_COOKIE, &requested.code_id.to_string(), i64::from(CODE_MINUTES) * 60);
    redirect_with_cookies("/hyr/kodi", vec![login])
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
        return again(t("Prit një çast dhe provo përsëri."));
    }
    let Some(contact) = Contact::parse(&form.contact) else {
        return again(t("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i."));
    };
    match sign_in::request(&state, &contact, ip.as_deref(), Some(form.name.as_str())).await? {
        Ok(requested) => Ok(ask_for(&headers, requested)),
        Err(refused) => again(not_sent(refused)),
    }
}

#[derive(Template)]
#[template(path = "sign_in_code.html")]
pub struct CodePage {
    sent_to: String,
    /// "në WhatsApp", "me email", or nothing while the channel isn't set up (sent_via).
    sent_via: &'static str,
    error: Option<String>,
    /// The code can no longer be used: the page offers a new one instead of the form.
    gone: bool,
    /// The code publishes a listing posted before signing in.
    publishes: bool,
    /// That listing waits for a moderator meanwhile, who may put it up without the code.
    reviewed: bool,
    /// The code adds its contact to the account signed in (settings.rs): it saves rather than
    /// signs in, and a new one is asked for from the settings.
    adds: bool,
    /// In test mode, the code itself.
    test_code: Option<String>,
}

impl CodePage {
    fn gone(sent_to: String, adds: bool) -> Self {
        Self {
            sent_to,
            sent_via: "",
            error: Some(t("Kodi skadoi ose u provua shumë herë. Kërko një kod të ri.").to_string()),
            gone: true,
            publishes: false,
            reviewed: false,
            adds,
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
    let Some((contact, publishes, reviewed, adds)) =
        pending.and_then(|pending| Some((Contact::parse(&pending.contact)?, pending.publishes, pending.reviewed, pending.adds)))
    else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let test_code = pending_id(&headers).and_then(|id| shown_codes().lock().unwrap().get(&id).cloned());
    render(CodePage { sent_to: contact.masked(), sent_via: sent_via(&state.senders, &contact), error: None, gone: false, publishes, reviewed, adds, test_code })
}

#[derive(Deserialize)]
pub struct CodeForm {
    #[serde(default)]
    code: String,
}

/// The code typed back, checked (`sign_in::check`): the contact is confirmed, so its account is
/// (created earlier, PENDING, it becomes active), the visitor is signed in (`sign_in::admit`), and
/// a listing posted before signing in goes live; or, for a contact being added to the account
/// signed in, it joins it (settings.rs).
pub async fn check_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(form): Form<CodeForm>,
) -> Result<Response> {
    let Some(id) = pending_id(&headers) else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Some(pending) = db::pending_login_code(&state.db, id, CODE_ATTEMPTS).await? else {
        return unprocessable(CodePage::gone(String::new(), false));
    };
    let sent_to = Contact::parse(&pending.contact).map(|contact| contact.masked()).unwrap_or_default();
    let proved = match sign_in::check(&state, id, &form.code).await? {
        Ok(proved) => proved,
        Err(Refused::WrongCode { attempts_left }) if attempts_left > 0 => {
            return unprocessable(CodePage {
                sent_via: Contact::parse(&pending.contact).map_or("", |contact| sent_via(&state.senders, &contact)),
                sent_to,
                error: Some(if attempts_left == 1 {
                    t("Kodi nuk është i saktë. Të ka mbetur edhe një provë.").to_string()
                } else {
                    t1("Kodi nuk është i saktë. Të kanë mbetur {} prova.", attempts_left)
                }),
                gone: false,
                publishes: pending.publishes,
                reviewed: pending.reviewed,
                adds: pending.adds,
                test_code: None,
            });
        }
        // Used up by that last guess, tried too often, expired or used: a new code is needed.
        Err(_) => return unprocessable(CodePage::gone(sent_to, pending.adds)),
    };
    shown_codes().lock().unwrap().remove(&id);
    if let Some(owner) = proved.purpose.adds_to {
        return settings::contact_confirmed(&state, &headers, owner, proved.contact.as_str()).await;
    }
    // A language chosen before signing in is the account's now, for its e-mails too (language.rs).
    let lang = cookie(&headers, super::language::LANG_COOKIE).and_then(|code| Lang::from_code(&code));
    // Its account suspended (`Refused::Suspended`) is all that admitting refuses.
    let Ok(signed) = sign_in::admit(&state, &proved, lang, Client::Web, None).await? else {
        return unprocessable(CodePage {
            sent_to,
            sent_via: "",
            error: Some(t("Kjo llogari është pezulluar.").to_string()),
            gone: true,
            publishes: false,
            reviewed: false,
            adds: false,
            test_code: None,
        });
    };
    let session = cookie_header(&headers, SESSION_COOKIE, &signed.token, i64::from(WEB_SESSION_DAYS) * 86_400);
    let login = cookie_header(&headers, LOGIN_COOKIE, "", 0);
    let published = match signed.listing {
        Some(listing) => db::publish_draft(&state.db, listing, signed.user.id).await?.map(|(no, slug)| format::listing_path(no, &slug)),
        None => None,
    };
    // A heart tapped before signing in (saved.rs): the listing is kept now, and the page it was
    // tapped on is where the visitor goes back to.
    let (back, done) = saved::after_sign_in(&state, &headers, signed.user.id).await?;
    let to = published.or(back).unwrap_or_else(|| "/njoftimet-e-mia".to_string());
    Ok(redirect_with_cookies(&to, [vec![session, login], done].concat()))
}

pub async fn sign_out(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    if let Some(token) = cookie(&headers, SESSION_COOKIE) {
        sign_in::sign_out(&state, &token).await?;
    }
    Ok(redirect_with_cookies("/", vec![cookie_header(&headers, SESSION_COOKIE, "", 0)]))
}

