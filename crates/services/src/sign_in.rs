//! Signing in with a code, once for the pages and the JSON API alike, with no cookie and no page of
//! its own: a phone number or an e-mail address, a six-digit code sent there by the state's senders
//! (`request`, `send`), and a session once the code comes back right (`check`, `admit`; both at
//! once, `complete`), whose token is given once, the database keeping its hash alone. The limits
//! keep one person or one network from asking for codes, or guessing them, by the thousand
//! (`limit`): one code a minute for a contact, five an hour, twenty an hour from one network, and a
//! code works for ten minutes and five guesses; WhatsApp messages and SMS cost money, so they also
//! cap what a bot can spend. What is refused is a code (`Refused`), which each edge says in its own
//! words: the pages (pluribus-web's account.rs) on their forms, as they always said it, with their
//! cookies, their redirects, the listing posted before signing in and the one kept; the API
//! (pluribus-api's signin.rs) in its envelope. A code also adds a contact to the account signed in,
//! or puts it in place of the one of its kind (the pages' "Cilësimet", the API's `/me/contacts`):
//! which contacts may join it (`may_join`), and the contact a code proved joining it (`join`).

use std::net::IpAddr;

use pluribus_core::{format::Contact, i18n::Lang};
use pluribus_db::{self as db, CodeCheck, CodePurpose, SessionUser};
use rand::Rng;
use uuid::Uuid;

use crate::{AppState, hash, sender::Channel};

/// A code works for ten minutes and for five guesses: a million possible codes, five guesses.
pub const CODE_MINUTES: i32 = 10;
pub const CODE_ATTEMPTS: i16 = 5;
/// One code a minute for a contact, five an hour; twenty an hour from one network.
pub const CODE_INTERVAL_SECONDS: f64 = 60.0;
pub const CODES_PER_CONTACT_HOUR: i64 = 5;
pub const CODES_PER_IP_HOUR: i64 = 20;
/// How long a session lasts: the pages' cookie a month, as it always has; the phone app's token a
/// year, which its owner signs in on their phone once for.
pub const WEB_SESSION_DAYS: i32 = 30;
pub const APP_SESSION_DAYS: i32 = 365;
/// The most characters of an account's name (`display_name`), and of a phone's as the app gives it,
/// which `sessions.device_name` holds (varchar(80), migration 0043).
const NAME_MOST: usize = 60;
const DEVICE_NAME_MOST: usize = 80;

/// What a session is for, as `sessions.client` keeps it (migration 0043): the pages, whose cookie
/// it is, or the phone app on an iPhone or an Android phone, whose bearer token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    Web,
    Ios,
    Android,
}

impl Client {
    /// The client as the app names it: "web", "ios" or "android"; none for anything else.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "web" => Some(Self::Web),
            "ios" => Some(Self::Ios),
            "android" => Some(Self::Android),
            _ => None,
        }
    }

    /// As the column spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Web => "web",
            Self::Ios => "ios",
            Self::Android => "android",
        }
    }

    /// How many days its sessions last: WEB_SESSION_DAYS for the pages, APP_SESSION_DAYS for the
    /// app.
    pub fn session_days(self) -> i32 {
        match self {
            Self::Web => WEB_SESSION_DAYS,
            Self::Ios | Self::Android => APP_SESSION_DAYS,
        }
    }
}

/// Why no code went out, or why a code typed back signed no one in: a code for each edge to say
/// in its own words (pluribus-web's account.rs on the pages, pluribus-api's signin.rs in the
/// envelope), with the figures those need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// A code went to this contact less than a minute ago: another may go in `retry_after` seconds.
    TooSoon { retry_after: u64 },
    /// The contact asked for CODES_PER_CONTACT_HOUR codes within the hour, or its network for
    /// CODES_PER_IP_HOUR: another may go in `retry_after` seconds.
    TooManyCodes { retry_after: u64 },
    /// The code couldn't go out: its channel refused it (a number WhatsApp can't reach, an address
    /// refused at once, the provider down). Nothing waits for it.
    NotSent,
    /// No code waits under that id: none was asked for, it was used, or its ten minutes are up.
    NoSuchCode,
    /// The code typed isn't the one sent: `attempts_left` more tries, none when this was the last,
    /// the code then used up.
    WrongCode { attempts_left: i16 },
    /// The code was tried CODE_ATTEMPTS times already: a new one is needed.
    TooManyAttempts,
    /// The code came back right, but its contact's account is suspended: no session.
    Suspended,
}

/// Why a contact can't join the account signed in, before any code goes to it (`may_join`): the
/// account has it already (`Own`), or another account has it (`Taken`), whose owner signs in with
/// it. Each edge says it in its own words, as the page has always said it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotJoining {
    Own,
    Taken,
}

/// A code that went out: the id it waits under, which it comes back with, and the channel it went
/// by; in test mode (`Config::sign_in_codes_on_screen`) the code itself too, for the page that
/// asks for it to show, and the API to give.
#[derive(Debug)]
pub struct Requested {
    pub code_id: Uuid,
    pub via: Channel,
    pub shown: Option<String>,
}

/// What a code typed back right proved: that its contact is the typer's; and what it was asked
/// for, a new account's name, a listing to put up once it came back, or an account the contact
/// joins (the pages' "Cilësimet", the API's `/me/contacts`; `join`).
#[derive(Debug)]
pub struct Proved {
    pub contact: Contact,
    pub purpose: CodePurpose,
}

/// Someone signed in by a code.
pub struct Completed {
    /// The session's token, for the pages' cookie or the app's bearer token: given here once, the
    /// database keeping its hash alone.
    pub token: String,
    pub user: SessionUser,
    /// Whether the account is new: made by this code, its contact confirmed for the first time.
    pub first_time: bool,
    /// The listing posted before signing in that the code puts up, which the pages put up
    /// (pluribus-web's account.rs).
    pub listing: Option<Uuid>,
}

/// A code as the database keeps it: SHA-256 of the contact and the code together, so that a copy
/// of the table signs no one in, and a code is good for its own contact alone.
fn code_hash(contact: &str, code: &str) -> Vec<u8> {
    hash(&format!("{contact}:{code}"))
}

/// A name as typed for an account: spaces tidied, at most 60 letters; none if empty. The posting
/// form and "Cilësimet" tidy a name so too (pluribus-web's account.rs gives it them).
pub fn display_name(typed: &str) -> Option<String> {
    tidied(typed, NAME_MOST)
}

/// `typed` with its spaces tidied, at most `most` characters of it; none if nothing is left.
fn tidied(typed: &str, most: usize) -> Option<String> {
    let tidy = typed.split_whitespace().collect::<Vec<_>>().join(" ");
    (!tidy.is_empty()).then(|| tidy.chars().take(most).collect())
}

/// The address the limits count a request from: the connection's own (`peer`); or, behind a proxy
/// on this machine (cloudflared's tunnel, a reverse proxy), which connects from the loopback, the
/// last address that proxy added to X-Forwarded-For (`forwarded_for`), since a visitor connecting
/// directly could write anything in that header; or, with no connection's address (a test's
/// request), the header's. The pages' `ClientIp` and the API's read it so.
pub fn client_ip(peer: Option<IpAddr>, forwarded_for: Option<&str>) -> Option<String> {
    let forwarded = || forwarded_for?.rsplit(',').next()?.trim().parse::<IpAddr>().ok();
    let ip = match peer {
        Some(ip) if ip.is_loopback() => forwarded().or(Some(ip)),
        Some(ip) => Some(ip),
        None => forwarded(),
    };
    ip.map(|ip| ip.to_string())
}

/// Why no code can go to this contact, from the network `ip`, now, if one can't: one went to it
/// under a minute ago (`TooSoon`), or it, or its network, asked for as many as the hour allows
/// (`TooManyCodes`); each with the seconds until one may go, the later of the two where both are
/// at theirs.
pub async fn limit(state: &AppState, contact: &Contact, ip: Option<&str>) -> sqlx::Result<Option<Refused>> {
    let requests = db::code_requests(&state.db, contact.as_str(), ip, CODES_PER_CONTACT_HOUR, CODES_PER_IP_HOUR).await?;
    if let Some(since) = requests.seconds_since_last.filter(|since| *since < CODE_INTERVAL_SECONDS) {
        return Ok(Some(Refused::TooSoon { retry_after: whole_seconds(CODE_INTERVAL_SECONDS - since) }));
    }
    let waits = requests.contact_waits.into_iter().chain(requests.network_waits).reduce(f64::max);
    Ok(waits.map(|wait| Refused::TooManyCodes { retry_after: whole_seconds(wait) }))
}

/// A wait in whole seconds, rounded up, one at least: once it is over, the limit has let go.
fn whole_seconds(wait: f64) -> u64 {
    // A wait is within the hour, and at least a second here: as a u64, whole.
    wait.ceil().max(1.0) as u64
}

/// Sends a sign-in code to the contact, asked for from the network `ip`, an account it makes
/// taking `name` (as `display_name` tidies it): the limits first (`limit`), then the code
/// (`send`).
pub async fn request(state: &AppState, contact: &Contact, ip: Option<&str>, name: Option<&str>) -> sqlx::Result<Result<Requested, Refused>> {
    if let Some(refused) = limit(state, contact, ip).await? {
        return Ok(Err(refused));
    }
    let purpose = CodePurpose { display_name: name.and_then(display_name), ..Default::default() };
    send(state, contact, ip, &purpose).await
}

/// Sends a code to the contact for `purpose`, by the state's senders (sender.rs: e-mail, or
/// WhatsApp for a phone, or the log where neither is set up), its limits seen to by the caller
/// (`request` does; the pages' posting form asks before it keeps a listing that waits for the
/// code): the code kept as its hash, for CODE_MINUTES, from the network `ip`. One its channel
/// refuses is gone again, and `NotSent`.
pub async fn send(state: &AppState, contact: &Contact, ip: Option<&str>, purpose: &CodePurpose) -> sqlx::Result<Result<Requested, Refused>> {
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000));
    let code_id = db::create_login_code(&state.db, contact.as_str(), &code_hash(contact.as_str(), &code), ip, CODE_MINUTES, purpose).await?;
    let via = match state.senders.send(contact, &code).await {
        Ok(via) => via,
        Err(error) => {
            tracing::error!(%error, to = %contact.masked(), "the sign-in code couldn't be sent");
            db::delete_login_code(&state.db, code_id).await?;
            return Ok(Err(Refused::NotSent));
        }
    };
    let shown = state.config.sign_in_codes_on_screen.then_some(code);
    Ok(Ok(Requested { code_id, via, shown }))
}

/// Checks the code typed back for the code waiting under `code_id`, its digits alone ("123 456" is
/// 123456), each wrong guess counted: right, it is used up and proves its contact (`Proved`);
/// otherwise why not, `WrongCode` with the tries left, `TooManyAttempts`, or `NoSuchCode`.
pub async fn check(state: &AppState, code_id: Uuid, typed: &str) -> sqlx::Result<Result<Proved, Refused>> {
    let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
    let checked = db::check_login_code(&state.db, code_id, |contact| code_hash(contact, &digits), CODE_ATTEMPTS).await?;
    Ok(match checked {
        CodeCheck::Signed { contact, purpose } => match Contact::parse(&contact) {
            Some(contact) => Ok(Proved { contact, purpose }),
            // Never so: a code goes only to a contact read as one, kept as it was read.
            None => Err(Refused::NoSuchCode),
        },
        CodeCheck::Wrong(attempts_left) => Err(Refused::WrongCode { attempts_left }),
        CodeCheck::TriedOut => Err(Refused::TooManyAttempts),
        CodeCheck::Gone => Err(Refused::NoSuchCode),
    })
}

/// Signs in the account of the contact a code proved, for `client` (on the phone `device_name`,
/// its first 80 characters): the account the contact has, or one made for it under the code's name
/// (`Suspended` for one suspended); confirmed, so that a new one is active; the contact verified
/// (`user_verifications`: listings showing it, and the profile, say so); the language the visitor
/// chose (`lang`; none, as the pages have it without their language's cookie, leaves the
/// account's) its language now, for its e-mails too; and a new session of the client's length,
/// its token made at random and kept as its hash alone.
pub async fn admit(state: &AppState, proved: &Proved, lang: Option<Lang>, client: Client, device_name: Option<&str>) -> sqlx::Result<Result<Completed, Refused>> {
    let (phone, email) = (proved.contact.phone(), proved.contact.email());
    let name = proved.purpose.display_name.as_deref();
    let account = db::account_for_contact(&state.db, phone, email, name).await?;
    if account.status == "SUSPENDED" {
        return Ok(Err(Refused::Suspended));
    }
    db::confirm_account(&state.db, account.id, name).await?;
    db::verify_contact(&state.db, account.id, phone, email).await?;
    if let Some(lang) = lang {
        db::set_preferred_language(&state.db, account.id, lang.code()).await?;
    }
    let token: String = rand::rng().random::<[u8; 32]>().iter().map(|byte| format!("{byte:02x}")).collect();
    let device_name = device_name.and_then(|name| tidied(name, DEVICE_NAME_MOST));
    db::create_session(&state.db, &hash(&token), account.id, client.session_days(), client.as_str(), device_name.as_deref()).await?;
    // The session just made: who it signs in, as every request after it finds them.
    let user = db::session_user(&state.db, &hash(&token)).await?.ok_or(sqlx::Error::RowNotFound)?;
    Ok(Ok(Completed { token, user, first_time: account.status == "PENDING", listing: proved.purpose.listing_id }))
}

/// A code typed back, checked (`check`) and, right, signing its contact's account in (`admit`),
/// as the API signs in. A code that adds its contact to the account signed in (the pages'
/// "Cilësimet", the API's `/me/contacts`) signs no one in: here it is as good as none
/// (`NoSuchCode`), used up.
pub async fn complete(
    state: &AppState,
    code_id: Uuid,
    typed: &str,
    lang: Option<Lang>,
    client: Client,
    device_name: Option<&str>,
) -> sqlx::Result<Result<Completed, Refused>> {
    let proved = match check(state, code_id, typed).await? {
        Ok(proved) => proved,
        Err(refused) => return Ok(Err(refused)),
    };
    if proved.purpose.adds_to.is_some() {
        return Ok(Err(Refused::NoSuchCode));
    }
    admit(state, &proved, lang, client, device_name).await
}

/// Ends the session whose token this is, the pages' cookie's or the app's.
pub async fn sign_out(state: &AppState, token: &str) -> sqlx::Result<()> {
    db::delete_session(&state.db, &hash(token)).await
}

/// Whether a contact may join `user`'s account, beside the one of the other kind or in place of the
/// one of its kind, by a code sent to it (the pages' "Cilësimet", the API's `POST /me/contacts`,
/// each asking `limit` after this, then sending the code with `CodePurpose::adds_to`): not one the
/// account has already (`NotJoining::Own`; an e-mail address in any case), nor one another account
/// has (`NotJoining::Taken`).
pub async fn may_join(state: &AppState, user: &SessionUser, contact: &Contact) -> sqlx::Result<Result<(), NotJoining>> {
    let own = match contact {
        Contact::Phone(phone) => user.phone.as_deref() == Some(phone.as_str()),
        Contact::Email(email) => user.email.as_deref().is_some_and(|own| own.eq_ignore_ascii_case(email)),
    };
    if own {
        return Ok(Err(NotJoining::Own));
    }
    let holder = db::account_by_contact(&state.db, contact.phone(), contact.email()).await?;
    Ok(match holder {
        Some(other) if other.id != user.id => Err(NotJoining::Taken),
        _ => Ok(()),
    })
}

/// The contact a code proved joins the account the code was asked for (`CodePurpose::adds_to`), as
/// the pages' code page and the API's `POST /me/contacts/verify` put it on: in place of the one of
/// its kind, and on the account's listings that showed that one (`db::set_contact`), verified as
/// signing in verifies one (`db::verify_contact`). False, and nothing changed, when another account
/// has had it since the code was asked for.
pub async fn join(state: &AppState, user_id: Uuid, contact: &Contact) -> sqlx::Result<bool> {
    if !db::set_contact(&state.db, user_id, contact).await? {
        return Ok(false);
    }
    db::verify_contact(&state.db, user_id, contact.phone(), contact.email()).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_address_is_the_connections_but_behind_a_proxy_on_this_machine() {
        let ip = |peer: Option<&str>, forwarded: Option<&str>| client_ip(peer.map(|peer| peer.parse().unwrap()), forwarded);
        assert_eq!(ip(Some("203.0.113.7"), None).as_deref(), Some("203.0.113.7"));
        // A visitor connecting directly writes what they like in the header: never read.
        assert_eq!(ip(Some("203.0.113.7"), Some("198.51.100.1")).as_deref(), Some("203.0.113.7"));
        // Behind the tunnel, the last address it added.
        assert_eq!(ip(Some("127.0.0.1"), Some("10.0.0.1, 198.51.100.1")).as_deref(), Some("198.51.100.1"));
        assert_eq!(ip(Some("::1"), Some(" 2001:db8::5 ")).as_deref(), Some("2001:db8::5"));
        assert_eq!(ip(Some("127.0.0.1"), Some("not an address")).as_deref(), Some("127.0.0.1"));
        assert_eq!(ip(Some("127.0.0.1"), None).as_deref(), Some("127.0.0.1"));
        // No connection's address: the header's.
        assert_eq!(ip(None, Some("198.51.100.1")).as_deref(), Some("198.51.100.1"));
        assert_eq!(ip(None, None), None);
    }

    #[test]
    fn a_name_is_tidied_and_cut_and_a_wait_is_whole_seconds() {
        assert_eq!(display_name("  Erion   Hoxha ").as_deref(), Some("Erion Hoxha"));
        assert_eq!(display_name(" \t "), None);
        assert_eq!(display_name(&"ë".repeat(70)).map(|name| name.chars().count()), Some(60));
        assert_eq!(tidied(&"x".repeat(90), DEVICE_NAME_MOST).map(|name| name.len()), Some(80));
        assert_eq!((whole_seconds(41.2), whole_seconds(0.004), whole_seconds(3599.5)), (42, 1, 3600));
    }

    #[test]
    fn a_client_is_named_as_the_column_spells_it_and_the_apps_last_a_year() {
        for client in [Client::Web, Client::Ios, Client::Android] {
            assert_eq!(Client::parse(client.as_str()), Some(client));
        }
        assert_eq!(Client::parse("iOS"), None);
        assert_eq!(Client::parse("windows"), None);
        assert_eq!((Client::Web.session_days(), Client::Ios.session_days(), Client::Android.session_days()), (30, 365, 365));
    }
}
