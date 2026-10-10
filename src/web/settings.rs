//! "Cilësimet", the account's settings (/llogaria/cilesimet, linked from every account page's row,
//! account_nav.html): its name; its phone number and its e-mail address, either added or changed
//! by a code sent to the new one (which then takes the place of the one of its kind, on the
//! account's listings too), and either taken off while the other stays; what it hears of, and
//! whether by e-mail; those it blocked; the devices signed in to it; signing out; and closing it
//! for good (db::delete_account). Each form comes back here, its line at the top saying what it
//! did, or with its own field saying why not.

use askama::Template;
use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    Result,
    account::{self, ClientIp, SignInPage, current_user, redirect_with_cookies, session_hash},
    photos, render,
};
use crate::{
    AppState,
    db::{self, SessionUser},
    format::{self, Contact},
};

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPage {
    settings: db::AccountSettings,
    preferences: db::NoticePreferences,
    blocked: Vec<db::BlockedSeller>,
    /// The stores the account is one of the people of (stores.rs), and whether it may make another.
    stores: Vec<db::MyStore>,
    more_stores: bool,
    /// How many devices are signed in, this one with them.
    sessions: i64,
    /// The notifications not seen yet, by "Lajmërimet" in the account's row.
    unread: i64,
    /// What the last form did, from the address it came back to (?u=...): "Emri u ruajt."
    done: Option<&'static str>,
    /// The fields as a form sent back typed them, and why it couldn't be done.
    name: String,
    name_error: Option<&'static str>,
    contact: String,
    contact_error: Option<&'static str>,
    close_error: Option<&'static str>,
}

impl SettingsPage {
    /// "tetor 2026", since when the account is on the site.
    fn since(&self) -> String {
        format::month_year(self.settings.created_at)
    }

    fn phone(&self) -> Option<String> {
        self.settings.phone.as_deref().map(format::phone)
    }

    /// Whether either can be taken off: an account keeps one at least, to sign in with.
    fn both(&self) -> bool {
        self.settings.phone.is_some() && self.settings.email.is_some()
    }
}

/// The page for this user, the fields as their account has them.
async fn settings_page(state: &AppState, user: &SessionUser, done: Option<&'static str>) -> Result<SettingsPage> {
    let (settings, preferences, blocked, sessions, unread, stores, owned) = tokio::try_join!(
        db::account_settings(&state.db, user.id),
        db::notice_preferences(&state.db, user.id),
        db::blocked_sellers(&state.db, user.id),
        db::session_count(&state.db, user.id),
        db::unread_notifications(&state.db, user.id),
        db::my_stores(&state.db, user.id),
        db::stores_owned(&state.db, user.id),
    )?;
    let name = settings.display_name.clone();
    Ok(SettingsPage {
        settings,
        preferences,
        blocked,
        stores,
        more_stores: owned < db::STORES_MOST,
        sessions,
        unread: unread.all,
        done,
        name,
        name_error: None,
        contact: String::new(),
        contact_error: None,
        close_error: None,
    })
}

/// A form sent back: the page again, as a 422, which Turbo shows.
fn again(page: SettingsPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

fn back(done: &str) -> Response {
    Redirect::to(&format!("/llogaria/cilesimet?u={done}")).into_response()
}

#[derive(Deserialize)]
pub struct Done {
    #[serde(default)]
    u: String,
}

/// What a form did, by the word the page came back with.
fn done_line(word: &str) -> Option<&'static str> {
    Some(match word {
        "emri" => "Emri u ruajt.",
        "kontakti" => "U ruajt te llogaria: hyn edhe me të tani.",
        "zene" => "Ndërkohë ky numër ose email u bë i një llogarie tjetër, ndaj nuk u shtua.",
        "hequr" => "U hoq nga llogaria.",
        "njoftimet" => "Zgjedhjet për njoftimet u ruajtën.",
        "pajisjet" => "Dole nga pajisjet e tjera.",
        "zhbllokuar" => "U zhbllokua.",
        _ => return None,
    })
}

/// GET /llogaria/cilesimet; for someone not signed in, the way to sign in.
pub async fn page(State(state): State<AppState>, headers: HeaderMap, Query(Done { u }): Query<Done>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    render(settings_page(&state, &user, done_line(&u)).await?)
}

#[derive(Deserialize)]
pub struct NameForm {
    #[serde(default)]
    emri: String,
}

/// POST /llogaria/emri: the name the account shows by, tidied as a new account's is
/// (account::display_name).
pub async fn rename(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<NameForm>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Some(name) = account::display_name(&form.emri) else {
        let mut page = settings_page(&state, &user, None).await?;
        page.name = form.emri;
        page.name_error = Some("Shkruaj emrin që do të shohin të tjerët.");
        return again(page);
    };
    db::rename_account(&state.db, user.id, &name).await?;
    Ok(back("emri"))
}

#[derive(Deserialize)]
pub struct ContactForm {
    #[serde(default)]
    kontakti: String,
}

/// POST /llogaria/kontakti: a phone number or an e-mail address to add to the account, or to take
/// the place of the one of its kind: a code goes to it (account::send_code_and_ask, its limits as
/// signing in has them), and the code page, once it comes back, puts it on the account
/// (contact_confirmed). Not one the account has already, nor another account's.
pub async fn add_contact(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Form(form): Form<ContactForm>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let refuse = |error: &'static str| {
        let (state, user, typed) = (state.clone(), &user, form.kontakti.clone());
        async move {
            let mut page = settings_page(&state, user, None).await?;
            page.contact = typed;
            page.contact_error = Some(error);
            again(page)
        }
    };
    let Some(contact) = Contact::parse(&form.kontakti) else {
        return refuse("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.").await;
    };
    let own = match &contact {
        Contact::Phone(phone) => user.phone.as_deref() == Some(phone.as_str()),
        Contact::Email(email) => user.email.as_deref().is_some_and(|own| own.eq_ignore_ascii_case(email)),
    };
    if own {
        return refuse("Ky është tashmë i llogarisë sate.").await;
    }
    if db::account_by_contact(&state.db, contact.phone(), contact.email()).await?.is_some_and(|other| other.id != user.id) {
        return refuse("Ky është i një llogarie tjetër: hyr me të për ta përdorur.").await;
    }
    if let Some(limit) = account::code_limit(&state, &contact, ip.as_deref()).await? {
        return refuse(limit).await;
    }
    let purpose = db::CodePurpose { adds_to: Some(user.id), ..Default::default() };
    match account::send_code_and_ask(&state, &headers, &contact, ip.as_deref(), &purpose).await? {
        Ok(response) => Ok(response),
        Err(message) => refuse(message).await,
    }
}

/// The code a contact being added was sent came back right (account::check_code): the contact
/// joins the account if that account is still the one signed in here, verified as signing in
/// verifies one; then back to the settings. The code is used up either way.
pub async fn contact_confirmed(state: &AppState, headers: &HeaderMap, owner: Uuid, contact: &str) -> Result<Response> {
    let forget = account::forget_login(headers);
    let user = current_user(state, headers).await?.filter(|user| user.id == owner);
    let (Some(user), Some(contact)) = (user, Contact::parse(contact)) else {
        return Ok(redirect_with_cookies("/llogaria", vec![forget]));
    };
    if !db::set_contact(&state.db, user.id, &contact).await? {
        return Ok(redirect_with_cookies("/llogaria/cilesimet?u=zene", vec![forget]));
    }
    db::verify_contact(&state.db, user.id, contact.phone(), contact.email()).await?;
    Ok(redirect_with_cookies("/llogaria/cilesimet?u=kontakti", vec![forget]))
}

#[derive(Deserialize)]
pub struct RemoveForm {
    /// "numri" or "email".
    #[serde(default)]
    hiq: String,
}

/// POST /llogaria/kontakti/hiq: takes the phone number or the e-mail address off the account, while
/// it keeps the other (db::remove_contact).
pub async fn remove_contact(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<RemoveForm>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let phone = match form.hiq.as_str() {
        "numri" => true,
        "email" => false,
        _ => return Ok(back("")),
    };
    if !db::remove_contact(&state.db, user.id, phone).await? {
        return Ok(back(""));
    }
    Ok(back("hequr"))
}

#[derive(Deserialize)]
pub struct NoticesForm {
    /// Each "1" while ticked; a box left unticked sends nothing.
    #[serde(default)]
    mesazhet: String,
    #[serde(default)]
    cmimet: String,
    #[serde(default)]
    kerkimet: String,
    #[serde(default)]
    email: String,
}

/// POST /llogaria/njoftimet: what the account hears of (db::NoticePreferences), as the boxes say.
/// One without an e-mail address has no box for e-mail, whose setting stays as it was.
pub async fn notices(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<NoticesForm>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let before = db::notice_preferences(&state.db, user.id).await?;
    let preferences = db::NoticePreferences {
        message_alerts: form.mesazhet == "1",
        price_drop_alerts: form.cmimet == "1",
        saved_search_alerts: form.kerkimet == "1",
        email_enabled: if user.email.is_some() { form.email == "1" } else { before.email_enabled },
    };
    db::set_notice_preferences(&state.db, user.id, preferences).await?;
    Ok(back("njoftimet"))
}

/// POST /llogaria/pajisjet: signs the account out everywhere but here.
pub async fn other_devices(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let (Some(user), Some(here)) = (current_user(&state, &headers).await?, session_hash(&headers)) else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    db::end_other_sessions(&state.db, user.id, &here).await?;
    Ok(back("pajisjet"))
}

#[derive(Template)]
#[template(path = "account_closed.html")]
struct ClosedPage;

#[derive(Deserialize)]
pub struct CloseForm {
    #[serde(default)]
    konfirmo: String,
}

/// POST /llogaria/fshi: closes the account for good once "FSHI" is typed (db::delete_account), its
/// listings' photos taken out of the store after; signed out, the page that says so.
pub async fn close(State(state): State<AppState>, headers: HeaderMap, Form(form): Form<CloseForm>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    if !form.konfirmo.trim().eq_ignore_ascii_case("FSHI") {
        let mut page = settings_page(&state, &user, None).await?;
        page.close_error = Some("Shkruaj FSHI në kutinë sipër butonit, që të dimë se e do vërtet.");
        return again(page);
    }
    let photos = db::delete_account(&state.db, user.id).await?;
    photos::delete(&state.media, &photos).await;
    tracing::info!(user = %user.id, photos = photos.len(), "an account closed itself");
    let mut response = render(ClosedPage)?;
    response.headers_mut().append(axum::http::header::SET_COOKIE, account::forget_session(&headers));
    Ok(response)
}
