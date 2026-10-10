//! Stores: "Krijo një dyqan" from "Cilësimet", a business of the user's own (db/stores.rs), which
//! their listings can be posted as (post.rs, "Posto si"), with its own page at /biznes/{slug}
//! (browse.rs, profile.html): its name and what it says of itself, its logo and its cover, its
//! public number (with WhatsApp or not), its e-mail address and website, its town and street, when
//! it is open, and its NIPT, for its own records. Its owner and its admins change it from its page.

use std::collections::HashMap;

use askama::Template;
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};

use super::{
    Result,
    account::{SignInPage, current_user},
    not_found,
    photos::Submission,
    render,
};
use crate::{
    AppState,
    db::{self, City, ImageChange, StoreFields, StoreImage},
    format::Contact,
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
        fail("name", "Shkruaj emrin e dyqanit, nga 2 deri në 80 shkronja.");
    }
    let description = text(&form.description);
    if description.as_ref().is_some_and(|text| text.chars().count() > DESCRIPTION_CHARS) {
        fail("description", "Përshkrimi mund të jetë deri në 2 000 shkronja.");
    }
    let phone = text(&form.phone).map(|typed| store_phone(&typed).ok_or(()));
    if matches!(phone, Some(Err(()))) {
        fail("phone", "Shkruaj numrin si 069 123 4567 ose 04 222 3333.");
    }
    let email = text(&form.email).map(|typed| match Contact::parse(&typed) {
        Some(Contact::Email(email)) => Ok(email),
        _ => Err(()),
    });
    if matches!(email, Some(Err(()))) {
        fail("email", "Shkruaj një adresë email-i, p.sh. info@dyqani.al.");
    }
    let site = text(&form.website).map(|typed| website(&typed).ok_or(()));
    if matches!(site, Some(Err(()))) {
        fail("website", "Shkruaj adresën e faqes, p.sh. www.dyqani.al.");
    }
    let town = match form.city.as_str() {
        "" => None,
        slug => {
            let town = towns.iter().find(|town| town.slug == slug);
            if town.is_none() {
                fail("city", "Zgjidh qytetin nga lista.");
            }
            town
        }
    };
    let address = text(&form.address);
    if address.as_ref().is_some_and(|text| text.chars().count() > ADDRESS_CHARS) {
        fail("address", "Adresa mund të jetë deri në 200 shkronja.");
    }
    let hours = text(&form.hours);
    if hours.as_ref().is_some_and(|text| text.chars().count() > HOURS_CHARS) {
        fail("hours", "Orari mund të jetë deri në 300 shkronja.");
    }
    let nipt = text(&form.nipt).map(|typed| typed.to_uppercase().replace(' ', ""));
    if nipt.as_ref().is_some_and(|nipt| nipt.len() != 10 || !nipt.chars().all(|c| c.is_ascii_alphanumeric())) {
        fail("nipt", "NIPT-i ka 10 shenja, p.sh. L12345678A.");
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
        "logo" => "Logoja nuk u lexua. Zgjidh një foto JPG, PNG ose WebP.",
        _ => "Kopertina nuk u lexua. Zgjidh një foto JPG, PNG ose WebP.",
    };
    if bytes.len() > media::MAX_UPLOAD_BYTES {
        errors.entry(field.to_string()).or_insert_with(|| format!("Fotoja mund të jetë deri në {} MB.", media::MAX_UPLOAD_BYTES / 1024 / 1024));
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
        errors.insert("form".to_string(), format!("Ke {} dyqane, sa mund të kesh.", db::STORES_MOST));
    }
    let form = StoreForm { phone: user.phone.as_deref().map(crate::format::phone).unwrap_or_default(), ..Default::default() };
    render(StoreFormPage { heading: "Krijo një dyqan", action: "/dyqani/krijo".to_string(), store: None, form, errors, towns })
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
        errors.insert("form".to_string(), format!("Ke {} dyqane, sa mund të kesh.", db::STORES_MOST));
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
        return again(StoreFormPage { heading: "Krijo një dyqan", action: "/dyqani/krijo".to_string(), store: None, form, errors, towns });
    }
    let slug = db::create_store(&state.db, user.id, &fields, logo.as_ref(), cover.as_ref()).await?;
    Ok(Redirect::to(&format!("/biznes/{slug}")).into_response())
}

/// The store at this address, for someone who may change it (db::MANAGING_ROLES).
async fn managed_store(state: &AppState, headers: &HeaderMap, slug: &str) -> Result<Option<db::Store>> {
    let Some(user) = current_user(state, headers).await? else {
        return Ok(None);
    };
    let manages = db::my_stores(&state.db, user.id).await?.iter().any(|store| store.slug == slug && store.manages());
    if !manages {
        return Ok(None);
    }
    Ok(db::store_by_slug(&state.db, slug).await?)
}

/// GET /dyqani/{slug}/ndrysho: the store's form, for its owner and its admins.
pub async fn edit_store(State(state): State<AppState>, headers: HeaderMap, Path(slug): Path<String>) -> Result<Response> {
    let Some(store) = managed_store(&state, &headers, &slug).await? else {
        return not_found().await;
    };
    let towns = db::cities(&state.db).await?;
    let form = StoreForm::of(&store, &towns);
    render(StoreFormPage {
        heading: "Ndrysho dyqanin",
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
            heading: "Ndrysho dyqanin",
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
