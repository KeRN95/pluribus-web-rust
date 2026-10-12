//! Stores: "Krijo një dyqan" from "Cilësimet", a business of the user's own (db/stores.rs), which
//! their listings can be posted as (post.rs, "Posto si"), with its own page at /biznes/{slug}
//! (browse.rs, profile.html): its name and what it says of itself, its logo and its cover, its
//! public number (with WhatsApp or not), its e-mail address and website, its town and street, when
//! it is open, and its NIPT, for its own records. Its owner and its admins change it from its page.

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use serde::Deserialize;
use uuid::Uuid;

use super::{
    Result,
    account::{SignInPage, current_user},
    not_found,
    photos::Submission,
    render, saved,
};
use crate::{
    AppState,
    db::{self, City, ImageChange, SessionUser, StoreFields, StoreImage},
    domain::Role,
    format::{self, Contact, percent_encoded},
    i18n::{self, Lang, in_lang, t, t1, t2},
    media,
};

#[derive(Template)]
#[template(path = "store_form.html")]
struct StoreFormPage {
    /// "Krijo një dyqan" or "Ndrysho dyqanin", and where the form goes.
    heading: &'static str,
    action: String,
    /// The store being changed, its pictures shown; None for a new one.
    store: Option<db::Store>,
    form: StoreForm,
    errors: HashMap<String, String>,
    towns: Vec<City>,
}

impl StoreFormPage {
    fn error(&self, field: &str) -> Option<&str> {
        self.errors.get(field).map(String::as_str)
    }
}

/// The form's fields as typed.
#[derive(Debug, Default, Clone)]
struct StoreForm {
    name: String,
    description: String,
    phone: String,
    whatsapp: bool,
    email: String,
    website: String,
    city: String,
    address: String,
    hours: String,
    nipt: String,
    remove_logo: bool,
    remove_cover: bool,
}

impl StoreForm {
    fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        let mut form = Self::default();
        for (name, value) in pairs {
            match name.as_str() {
                "name" => form.name = value,
                "description" => form.description = value,
                "phone" => form.phone = value,
                "whatsapp" => form.whatsapp = value == "1",
                "email" => form.email = value,
                "website" => form.website = value,
                "city" => form.city = value,
                "address" => form.address = value,
                "hours" => form.hours = value,
                "nipt" => form.nipt = value,
                "remove_logo" => form.remove_logo = value == "1",
                "remove_cover" => form.remove_cover = value == "1",
                _ => {}
            }
        }
        form
    }

    /// A store's fields as its form shows them, its number written as people write it.
    fn of(store: &db::Store, towns: &[City]) -> Self {
        Self {
            name: store.display_name.clone(),
            description: store.description.clone().unwrap_or_default(),
            phone: store.public_phone.as_deref().map(crate::format::phone).unwrap_or_default(),
            whatsapp: store.whatsapp,
            email: store.public_email.clone().unwrap_or_default(),
            website: store.website_url.clone().unwrap_or_default(),
            city: store.location_id.and_then(|id| towns.iter().find(|town| town.id == id)).map(|town| town.slug.clone()).unwrap_or_default(),
            address: store.address.clone().unwrap_or_default(),
            hours: store.opening_hours.clone().unwrap_or_default(),
            nipt: store.registration_no.clone().unwrap_or_default(),
            remove_logo: false,
            remove_cover: false,
        }
    }
}

/// A store's public number as +355…: an Albanian one, a mobile's nine digits after 0 or a fixed
/// line's eight (04 222 3333), or a foreign one with its + (or 00); None for anything else.
pub fn store_phone(typed: &str) -> Option<String> {
    let digits: String = typed.chars().filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')' | '/')).collect();
    let e164 = if let Some(rest) = digits.strip_prefix('+') {
        format!("+{rest}")
    } else if let Some(rest) = digits.strip_prefix("00") {
        format!("+{rest}")
    } else {
        format!("+355{}", digits.strip_prefix('0')?)
    };
    let number = &e164[1..];
    let valid = number.chars().all(|c| c.is_ascii_digit())
        && (7..=15).contains(&number.len())
        && !number.starts_with('0')
        && match number.strip_prefix("355") {
            Some(national) => (8..=9).contains(&national.len()) && !national.starts_with('0'),
            None => true,
        };
    valid.then_some(e164)
}

/// A website as a link: https:// before it when it has no scheme, a dot in its host, no spaces;
/// None for anything else.
pub fn website(typed: &str) -> Option<String> {
    let typed = typed.trim();
    let url = if typed.starts_with("http://") || typed.starts_with("https://") { typed.to_string() } else { format!("https://{typed}") };
    let host = url.split("://").nth(1)?.split(['/', '?', '#']).next()?;
    let valid = host.contains('.') && !host.starts_with('.') && !host.ends_with('.') && url.len() <= 200 && !url.chars().any(char::is_whitespace);
    valid.then_some(url)
}

/// How long each of the form's texts may be, in characters.
const NAME_CHARS: usize = 80;
const DESCRIPTION_CHARS: usize = 2000;
const ADDRESS_CHARS: usize = 200;
const HOURS_CHARS: usize = 300;

/// The form's fields checked, each problem under its field.
fn check(form: &StoreForm, towns: &[City], errors: &mut HashMap<String, String>) -> StoreFields {
    let mut fail = |field: &str, message: &str| {
        errors.entry(field.to_string()).or_insert_with(|| message.to_string());
    };
    let text = |value: &str| Some(value.trim().to_string()).filter(|value| !value.is_empty());
    let name = form.name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.chars().count() < 2 || name.chars().count() > NAME_CHARS {
        fail("name", t("Shkruaj emrin e dyqanit, nga 2 deri në 80 shkronja."));
    }
    let description = text(&form.description);
    if description.as_ref().is_some_and(|text| text.chars().count() > DESCRIPTION_CHARS) {
        fail("description", t("Përshkrimi mund të jetë deri në 2 000 shkronja."));
    }
    let phone = text(&form.phone).map(|typed| store_phone(&typed).ok_or(()));
    if matches!(phone, Some(Err(()))) {
        fail("phone", t("Shkruaj numrin si 069 123 4567 ose 04 222 3333."));
    }
    let email = text(&form.email).map(|typed| match Contact::parse(&typed) {
        Some(Contact::Email(email)) => Ok(email),
        _ => Err(()),
    });
    if matches!(email, Some(Err(()))) {
        fail("email", t("Shkruaj një adresë email-i, p.sh. info@dyqani.al."));
    }
    let site = text(&form.website).map(|typed| website(&typed).ok_or(()));
    if matches!(site, Some(Err(()))) {
        fail("website", t("Shkruaj adresën e faqes, p.sh. www.dyqani.al."));
    }
    let town = match form.city.as_str() {
        "" => None,
        slug => {
            let town = towns.iter().find(|town| town.slug == slug);
            if town.is_none() {
                fail("city", t("Zgjidh qytetin nga lista."));
            }
            town
        }
    };
    let address = text(&form.address);
    if address.as_ref().is_some_and(|text| text.chars().count() > ADDRESS_CHARS) {
        fail("address", t("Adresa mund të jetë deri në 200 shkronja."));
    }
    let hours = text(&form.hours);
    if hours.as_ref().is_some_and(|text| text.chars().count() > HOURS_CHARS) {
        fail("hours", t("Orari mund të jetë deri në 300 shkronja."));
    }
    let nipt = text(&form.nipt).map(|typed| typed.to_uppercase().replace(' ', ""));
    if nipt.as_ref().is_some_and(|nipt| nipt.len() != 10 || !nipt.chars().all(|c| c.is_ascii_alphanumeric())) {
        fail("nipt", t("NIPT-i ka 10 shenja, p.sh. L12345678A."));
    }
    let phone = phone.and_then(Result::ok);
    StoreFields {
        name,
        description,
        whatsapp: form.whatsapp && phone.is_some(),
        phone,
        email: email.and_then(Result::ok),
        website: site.and_then(Result::ok),
        location_id: town.map(|town| town.id),
        address,
        opening_hours: hours,
        registration_no: nipt,
    }
}

/// The logo or the cover the form sent (by its field's name), made and put in the photo store
/// under a new name; None when none was sent. A picture the site can't read, or too big, is a
/// problem under its field.
async fn store_image(state: &AppState, files: &[(String, Bytes)], field: &str, errors: &mut HashMap<String, String>) -> Result<Option<StoreImage>> {
    let Some((_, bytes)) = files.iter().find(|(name, _)| name == field) else {
        return Ok(None);
    };
    let unreadable = match field {
        "logo" => t("Logoja nuk u lexua. Zgjidh një foto JPG, PNG ose WebP."),
        _ => t("Kopertina nuk u lexua. Zgjidh një foto JPG, PNG ose WebP."),
    };
    if bytes.len() > media::MAX_UPLOAD_BYTES {
        errors.entry(field.to_string()).or_insert_with(|| t1("Fotoja mund të jetë deri në {} MB.", media::MAX_UPLOAD_BYTES / 1024 / 1024));
        return Ok(None);
    }
    let bytes = bytes.clone();
    let logo = field == "logo";
    let made = tokio::task::spawn_blocking(move || if logo { media::logo(&bytes) } else { media::store_cover(&bytes) }).await?;
    let Ok(made) = made else {
        errors.entry(field.to_string()).or_insert_with(|| unreadable.to_string());
        return Ok(None);
    };
    let key = format!("stores/{:032x}-{field}.webp", rand::random::<u128>());
    state.media.put(&key, made).await?;
    Ok(Some(StoreImage { url: state.media.url(&key), key }))
}

/// Takes files out of the photo store; one left behind only takes space.
async fn forget_files(state: &AppState, keys: &[String]) {
    for key in keys {
        if let Err(error) = state.media.delete(key).await {
            tracing::warn!(%error, "a store's old picture stays in the store");
        }
    }
}

/// The form again, as a 422, which Turbo shows.
fn again(page: StoreFormPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

/// GET /dyqani/krijo: a new store's form, for someone signed in who owns fewer than
/// db::STORES_MOST; for someone not signed in, the way to sign in.
pub async fn new_store(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    let towns = db::cities(&state.db).await?;
    let mut errors = HashMap::new();
    if db::stores_owned(&state.db, user.id).await? >= db::STORES_MOST {
        errors.insert("form".to_string(), t1("Ke {} dyqane, sa mund të kesh.", db::STORES_MOST));
    }
    let form = StoreForm { phone: user.phone.as_deref().map(crate::format::phone).unwrap_or_default(), ..Default::default() };
    render(StoreFormPage { heading: t("Krijo një dyqan"), action: "/dyqani/krijo".to_string(), store: None, form, errors, towns })
}

/// POST /dyqani/krijo: the store made, the user its owner, its pictures made and kept; to its page.
pub async fn create_store(State(state): State<AppState>, headers: HeaderMap, submission: Submission) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let towns = db::cities(&state.db).await?;
    let form = StoreForm::from_pairs(submission.pairs);
    let mut errors = HashMap::new();
    if db::stores_owned(&state.db, user.id).await? >= db::STORES_MOST {
        errors.insert("form".to_string(), t1("Ke {} dyqane, sa mund të kesh.", db::STORES_MOST));
    }
    let fields = check(&form, &towns, &mut errors);
    let (logo, cover) = if errors.is_empty() {
        (store_image(&state, &submission.files, "logo", &mut errors).await?, store_image(&state, &submission.files, "cover", &mut errors).await?)
    } else {
        (None, None)
    };
    if !errors.is_empty() {
        let made: Vec<String> = [&logo, &cover].into_iter().flatten().map(|image| image.key.clone()).collect();
        forget_files(&state, &made).await;
        return again(StoreFormPage { heading: t("Krijo një dyqan"), action: "/dyqani/krijo".to_string(), store: None, form, errors, towns });
    }
    let slug = db::create_store(&state.db, user.id, &fields, logo.as_ref(), cover.as_ref()).await?;
    Ok(Redirect::to(&format!("/biznes/{slug}")).into_response())
}

/// The store at this address, with the user and their role in it, for its owner and admins
/// (Role::manages); None for anyone else.
async fn managed(state: &AppState, headers: &HeaderMap, slug: &str) -> Result<Option<(SessionUser, db::Store, String)>> {
    let Some(user) = current_user(state, headers).await? else {
        return Ok(None);
    };
    let Some(store) = db::store_by_slug(&state.db, slug).await? else {
        return Ok(None);
    };
    match db::role_in(&state.db, store.id, user.id).await? {
        Some(role) if Role::parse(&role).is_some_and(Role::manages) => Ok(Some((user, store, role))),
        _ => Ok(None),
    }
}

/// The store at this address, for someone who may change it (Role::manages).
async fn managed_store(state: &AppState, headers: &HeaderMap, slug: &str) -> Result<Option<db::Store>> {
    Ok(managed(state, headers, slug).await?.map(|(_, store, _)| store))
}

/// GET /dyqani/{slug}/ndrysho: the store's form, for its owner and its admins.
pub async fn edit_store(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>) -> Result<Response> {
    let Some(store) = managed_store(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let towns = db::cities(&state.db).await?;
    let form = StoreForm::of(&store, &towns);
    render(StoreFormPage {
        heading: t("Ndrysho dyqanin"),
        action: format!("/dyqani/{slug}/ndrysho"),
        store: Some(store),
        form,
        errors: HashMap::new(),
        towns,
    })
}

/// POST /dyqani/{slug}/ndrysho: the store's form saved, its logo and cover kept, replaced or taken
/// off (their old files out of the photo store); back to its page.
pub async fn update_store(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, submission: Submission) -> Result<Response> {
    let Some(store) = managed_store(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let towns = db::cities(&state.db).await?;
    let form = StoreForm::from_pairs(submission.pairs);
    let mut errors = HashMap::new();
    let fields = check(&form, &towns, &mut errors);
    let (logo, cover) = if errors.is_empty() {
        (store_image(&state, &submission.files, "logo", &mut errors).await?, store_image(&state, &submission.files, "cover", &mut errors).await?)
    } else {
        (None, None)
    };
    if !errors.is_empty() {
        let made: Vec<String> = [&logo, &cover].into_iter().flatten().map(|image| image.key.clone()).collect();
        forget_files(&state, &made).await;
        return again(StoreFormPage {
            heading: t("Ndrysho dyqanin"),
            action: format!("/dyqani/{slug}/ndrysho"),
            store: Some(store),
            form,
            errors,
            towns,
        });
    }
    let change = |image: Option<StoreImage>, remove: bool| match (image, remove) {
        (Some(image), _) => ImageChange::Replace(image),
        (None, true) => ImageChange::Remove,
        (None, false) => ImageChange::Keep,
    };
    let gone = db::update_store(&state.db, store.id, &fields, &change(logo, form.remove_logo), &change(cover, form.remove_cover)).await?;
    forget_files(&state, &gone).await;
    Ok(Redirect::to(&format!("/biznes/{slug}")).into_response())
}

/// A role's name, as the pages say it, in their language (Role::label, which an invitation's
/// notification says too, domain::notices); a text that is no role's, the least role's.
pub fn role_name(role: &str) -> &'static str {
    Role::parse(role).unwrap_or(Role::Viewer).label()
}

/// What a role lets someone do, as the invitation form and the invitation say it, in their
/// language (Role::does); a text that is no role's, what the least role does.
pub fn role_does(role: &str) -> &'static str {
    Role::parse(role).unwrap_or(Role::Viewer).does()
}

/// The roles someone of `role` gives, and so takes back: an owner any of db::GIVEN_ROLES, an admin
/// those under its own; nobody else any.
fn given_by(role: &str) -> &'static [&'static str] {
    match role {
        "OWNER" => &db::GIVEN_ROLES,
        "ADMIN" => &["EDITOR", "VIEWER"],
        _ => &[],
    }
}

/// How many invitations a store may have open at once: enough for a team, not for mailing lists.
const INVITATIONS_MOST: usize = 20;

/// A store's people (store_people.html), for its owner and admins: each with their role, those the
/// user may handle with the roles they give and "Hiq"; the open invitations, each to send again by
/// WhatsApp or as a link, or to withdraw; and the form that asks someone in.
#[derive(Template)]
#[template(path = "store_people.html")]
struct PeoplePage {
    store: db::Store,
    /// The user, and their role in the store, which says whom they handle.
    me: Uuid,
    role: String,
    members: Vec<db::Member>,
    invitations: Vec<db::Invitation>,
    /// The site's address, which an invitation's link starts with.
    origin: String,
    /// The invitation form as typed, and why it didn't go.
    contact: String,
    invite_role: String,
    error: Option<String>,
    /// What the last form did.
    done: Option<&'static str>,
}

impl PeoplePage {
    /// The roles the user gives.
    fn roles(&self) -> &'static [&'static str] {
        given_by(&self.role)
    }

    /// Whether the user may change or let go this one of the store's people: not themself, and
    /// only someone with a role they give (never the owner).
    fn handles(&self, member: &db::Member) -> bool {
        member.user_id != self.me && given_by(&self.role).contains(&member.role.as_str())
    }

    /// The role the invitation form starts at: the one typed, else someone who posts.
    fn picked(&self, role: &str) -> bool {
        if self.invite_role.is_empty() { role == "EDITOR" } else { self.invite_role == role }
    }

    /// The contact someone signs in with, as people write it.
    fn contact_of(&self, member: &db::Member) -> String {
        match (&member.phone, &member.email) {
            (Some(phone), _) => format::phone(phone),
            (None, email) => email.clone().unwrap_or_default(),
        }
    }

    /// Whom an invitation waits for, as people write it.
    fn invited(&self, invitation: &db::Invitation) -> String {
        match (&invitation.phone, &invitation.email) {
            (Some(phone), _) => format::phone(phone),
            (None, email) => email.clone().unwrap_or_default(),
        }
    }

    /// An invitation's page, to send by hand.
    fn link(&self, invitation: &db::Invitation) -> String {
        format!("{}/ftesa/{}", self.origin, invitation.id)
    }

    /// A chat in WhatsApp with the number an invitation waits for, its link written in it; None for
    /// an e-mail address, which hears of it by e-mail.
    fn whatsapp(&self, invitation: &db::Invitation) -> Option<String> {
        let digits = invitation.phone.as_deref()?.strip_prefix('+')?;
        let text = format!("Përshëndetje! Të ftoj në dyqanin „{}“ te Pluribus: {}", self.store.display_name, self.link(invitation));
        Some(format!("https://wa.me/{digits}?text={}", percent_encoded(&text)))
    }

    /// The day an invitation runs out.
    fn until(&self, invitation: &db::Invitation) -> String {
        format::date(invitation.expires_at)
    }
}

#[derive(Deserialize)]
pub struct PeopleDone {
    /// What the last form did (people_done).
    u: Option<String>,
}

/// The line at the top of the page of people saying what the last form did.
fn people_done(word: &str) -> Option<&'static str> {
    Some(match word {
        "ftuar" => t("Ftesa u ruajt. Dërgoja edhe vetë: me WhatsApp, ose kopjo lidhjen."),
        "anetar" => t("Ky numër ose email është tashmë i dyqanit."),
        "anuluar" => t("Ftesa u anulua."),
        "roli" => t("Roli u ndryshua."),
        "hequr" => t("U hoq nga dyqani."),
        _ => return None,
    })
}

/// The page of people, with what it shows besides its store.
async fn people_page(
    state: &AppState,
    me: Uuid,
    store: db::Store,
    role: String,
    (contact, invite_role): (String, String),
    error: Option<String>,
    done: Option<&'static str>,
) -> Result<PeoplePage> {
    let (members, invitations) = tokio::try_join!(db::members(&state.db, store.id), db::store_invitations(&state.db, store.id))?;
    Ok(PeoplePage { store, me, role, members, invitations, origin: state.config.site_origin(), contact, invite_role, error, done })
}

/// GET /dyqani/{slug}/njerezit: the store's people, for its owner and admins.
pub async fn people(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Query(done): Query<PeopleDone>) -> Result<Response> {
    let Some((user, store, role)) = managed(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let done = done.u.as_deref().and_then(people_done);
    render(people_page(&state, user.id, store, role, Default::default(), None, done).await?)
}

#[derive(Deserialize)]
pub struct InviteForm {
    #[serde(default)]
    kontakti: String,
    #[serde(default)]
    roli: String,
}

/// POST /dyqani/{slug}/njerezit: asks someone into the store by their number or address, with one
/// of the roles the user gives. An account with that contact has a notification of it; an address
/// hears of it by e-mail; for a number, the page has WhatsApp's button to send it by hand.
pub async fn invite(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>, Form(form): Form<InviteForm>) -> Result<Response> {
    let Some((user, store, role)) = managed(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let contact = Contact::parse(&form.kontakti);
    let error = if contact.is_none() {
        Some(t("Shkruaj një numër celulari (069 123 4567) ose një email."))
    } else if !given_by(&role).contains(&form.roli.as_str()) {
        Some(t("Zgjidh një nga rolet."))
    } else if db::store_invitations(&state.db, store.id).await?.len() >= INVITATIONS_MOST {
        Some(t("Dyqani ka 20 ftesa në pritje. Anulo disa para se të dërgosh të tjera."))
    } else {
        None
    };
    let (Some(contact), None) = (contact, error) else {
        let error = error.map(str::to_string);
        let page = people_page(&state, user.id, store, role, (form.kontakti, form.roli), error, None).await?;
        return Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response());
    };
    let title: String = format!("{} të fton në dyqan", store.display_name).chars().take(180).collect();
    // The notification and the e-mail speak Albanian for now, whatever the page's language (their
    // English is a later step): the role's words in Albanian too.
    let body = in_lang(Lang::Sq, async { format!("Si „{}“: {}", role_name(&form.roli), role_does(&form.roli)) }).await;
    let id = match db::invite(&state.db, store.id, &contact, &form.roli, user.id, &title, &body).await? {
        db::Invited::Member => return Ok(Redirect::to(&format!("/dyqani/{slug}/njerezit?u=anetar")).into_response()),
        db::Invited::Sent(id) => id,
    };
    if let Contact::Email(email) = &contact {
        // The e-mail in the language of the account the address signs in to, if one does; else
        // Albanian.
        let lang = db::language_of_email(&state.db, email).await?.and_then(|code| Lang::from_code(&code)).unwrap_or_default();
        let (subject, text) = i18n::LANG.sync_scope(lang, || {
            let invites = t1("{} të fton në dyqan", &store.display_name);
            let role = t2("Si „{}“: {}", role_name(&form.roli), role_does(&form.roli));
            (invites, format!("{role}\n\n{}\n{}/ftesa/{id}", t("Për ta pranuar, hyr në Pluribus me këtë email:"), state.config.site_origin()))
        });
        if let Err(error) = state.senders.send_text(&contact, &subject, &text).await {
            tracing::warn!(%error, to = %contact.masked(), "store invitation not sent");
        }
    }
    Ok(Redirect::to(&format!("/dyqani/{slug}/njerezit?u=ftuar")).into_response())
}

#[derive(Deserialize)]
pub struct RoleForm {
    #[serde(default)]
    roli: String,
}

/// One of the store's people the user handles (PeoplePage::handles): their id; None otherwise.
async fn handled(state: &AppState, store: &db::Store, user: &SessionUser, role: &str, member: &str) -> Result<Option<Uuid>> {
    let Ok(member) = member.parse::<Uuid>() else {
        return Ok(None);
    };
    let theirs = db::role_in(&state.db, store.id, member).await?;
    let handles = member != user.id && theirs.is_some_and(|theirs| given_by(role).contains(&theirs.as_str()));
    Ok(handles.then_some(member))
}

/// POST /dyqani/{slug}/njerezit/{user_id}: gives one of the store's people another role, one the
/// user gives, if the user handles them.
pub async fn set_role(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((slug, member)): Path<(String, String)>,
    Form(form): Form<RoleForm>,
) -> Result<Response> {
    let Some((user, store, role)) = managed(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let Some(member) = handled(&state, &store, &user, &role, &member).await? else {
        return not_found().await;
    };
    if !given_by(&role).contains(&form.roli.as_str()) {
        return not_found().await;
    }
    db::set_member_role(&state.db, store.id, member, &form.roli).await?;
    Ok(Redirect::to(&format!("/dyqani/{slug}/njerezit?u=roli")).into_response())
}

/// POST /dyqani/{slug}/njerezit/{user_id}/hiq: lets one of the store's people go, if the user
/// handles them; the listings they posted for it stay its own.
pub async fn remove(State(state): State<AppState>, headers: HeaderMap, Path((slug, member)): Path<(String, String)>) -> Result<Response> {
    let Some((user, store, role)) = managed(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let Some(member) = handled(&state, &store, &user, &role, &member).await? else {
        return not_found().await;
    };
    db::remove_member(&state.db, store.id, member).await?;
    Ok(Redirect::to(&format!("/dyqani/{slug}/njerezit?u=hequr")).into_response())
}

/// POST /dyqani/{slug}/ftesat/{id}/anulo: withdraws one of the store's invitations.
pub async fn cancel(State(state): State<AppState>, headers: HeaderMap, Path((slug, id)): Path<(String, String)>) -> Result<Response> {
    let Some((_, store, _)) = managed(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return not_found().await;
    };
    db::cancel_invitation(&state.db, store.id, id).await?;
    Ok(Redirect::to(&format!("/dyqani/{slug}/njerezit?u=anuluar")).into_response())
}

/// POST /dyqani/{slug}/largohu: someone who isn't the store's owner leaves it; to "Cilësimet".
pub async fn leave(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Some(store) = db::store_by_slug(&state.db, &slug).await? else {
        return not_found().await;
    };
    if !db::remove_member(&state.db, store.id, user.id).await? {
        return not_found().await;
    }
    Ok(Redirect::to("/llogaria/cilesimet?u=larguar").into_response())
}

/// Who looks at an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    SignedOut,
    /// Signed in with the contact it waits for.
    Invited,
    /// Signed in with other contacts.
    Other,
}

/// An invitation into a store (store_invitation.html): the store, the role it gives and what that
/// lets one do; for the one it is for, "Prano" and "Refuzo"; for anyone else, the contact to sign
/// in with.
#[derive(Template)]
#[template(path = "store_invitation.html")]
struct InvitationPage {
    invitation: db::Invitation,
    viewer: Viewer,
    /// The contact it waits for, its middle hidden ("+355 69 *** **33").
    masked: String,
}

impl InvitationPage {
    /// The day it runs out.
    fn until(&self) -> String {
        format::date(self.invitation.expires_at)
    }
}

/// Whether the invitation waits for one of the user's contacts.
fn invites(user: &SessionUser, invitation: &db::Invitation) -> bool {
    let phone = invitation.phone.is_some() && invitation.phone == user.phone;
    let email = match (&invitation.email, &user.email) {
        (Some(invited), Some(theirs)) => invited.eq_ignore_ascii_case(theirs),
        _ => false,
    };
    phone || email
}

/// GET /ftesa/{id}: an invitation, while it is open. Someone not signed in comes back here
/// after signing in (saved::back_after_sign_in).
pub async fn invitation(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let invitation = match id.parse::<Uuid>() {
        Ok(id) => db::invitation(&state.db, id).await?,
        Err(_) => None,
    };
    let Some(invitation) = invitation else {
        return not_found().await;
    };
    let user = current_user(&state, &headers).await?;
    let viewer = match &user {
        None => Viewer::SignedOut,
        Some(user) if invites(user, &invitation) => Viewer::Invited,
        Some(_) => Viewer::Other,
    };
    let page = format!("/ftesa/{}", invitation.id);
    let masked = invitation.contact().masked();
    let mut response = render(InvitationPage { invitation, viewer, masked })?;
    if user.is_none() {
        response.headers_mut().append(header::SET_COOKIE, saved::back_after_sign_in(&headers, &page));
    }
    Ok(response)
}

/// POST /ftesa/{id}/prano: the user takes the invitation, if it is theirs; to "Cilësimet",
/// which lists the store now.
pub async fn accept(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return not_found().await;
    };
    match db::accept_invitation(&state.db, id, user.id).await? {
        Some(_) => Ok(Redirect::to("/llogaria/cilesimet?u=pranuar").into_response()),
        None => not_found().await,
    }
}

/// POST /ftesa/{id}/refuzo: the user turns the invitation down, if it is theirs.
pub async fn decline(State(state): State<AppState>, headers: HeaderMap, Path(id): Path<String>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/llogaria").into_response());
    };
    let Ok(id) = id.parse::<Uuid>() else {
        return not_found().await;
    };
    if !db::decline_invitation(&state.db, id, user.id).await? {
        return not_found().await;
    }
    Ok(Redirect::to("/llogaria/cilesimet?u=refuzuar").into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stores_number_and_website() {
        assert_eq!(store_phone("069 123 4567").as_deref(), Some("+355691234567"));
        assert_eq!(store_phone("04 222 3333").as_deref(), Some("+35542223333"));
        assert_eq!(store_phone("+39 06 1234 5678").as_deref(), Some("+390612345678"));
        assert_eq!(store_phone("0042"), None);
        assert_eq!(store_phone("12345"), None);
        assert_eq!(website("www.dyqani.al").as_deref(), Some("https://www.dyqani.al"));
        assert_eq!(website("http://dyqani.al/rreth").as_deref(), Some("http://dyqani.al/rreth"));
        assert_eq!(website("dyqani"), None);
        assert_eq!(website("dyqani .al"), None);
    }
}
