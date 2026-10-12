//! Posting and editing listings: the form, the limits that keep one account from flooding the site,
//! and what the listing's rules (domain::listing) find wrong, said in the page's language. What the
//! JSON API's listings share with the form is pluribus-services' since C9: the properties' checks
//! (properties.rs), the photos' (photos.rs), and posting.rs's limits, review, poster, listing as the
//! database saves it, zone named by its words and the form's words for each problem.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use askama::Template;
use axum::{
    Form, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use pluribus_services::{
    filters::parse_number,
    photos::{check as check_photos, process as process_photos, store as store_photos, unstore},
    posting::{self, Poster, listing_to_save, message},
    push,
    sign_in::display_name,
};
use serde::Deserialize;

use super::{
    Result,
    account::{self, ClientIp, MIN_FORM_SECONDS, SignInPage, cookie_header, current_user, now, redirect_with_cookies},
    filters::{self, Choice, ChoiceGroup},
    not_found,
    photos::Submission,
    promotions::OnPromotionExt,
    properties::{self, PropertyField},
    render, saved,
};
use crate::{
    AppState,
    db::{self, Categories, Category, City, FormAttribute, ListingAction, MyListing, OwnListing, OwnPhoto, Photos, SessionUser},
    domain::{
        Condition, ListingType, PriceType, ReviewReason, Status,
        listing::{self, Checked, FieldErrors, ListingInput, distance_km, mapped, zoned},
    },
    format::{self, Contact},
    i18n::{t, t1, t2},
    media::{self, MAX_PHOTOS},
};

/// The kinds of price the form offers (listing::PRICE_TYPES, in its order), each with its label,
/// which the form shows in the page's language (t).
pub(super) const PRICE_TYPES: [(&str, &str); 4] = [
    (PriceType::Fixed.as_str(), "Çmim fiks"),
    (PriceType::Negotiable.as_str(), "I diskutueshëm"),
    (PriceType::Free.as_str(), "Falas"),
    (PriceType::OnRequest.as_str(), "Me marrëveshje"),
];
pub(super) const CONDITIONS: [&str; 4] = Condition::NAMES;
pub(super) const LISTING_TYPES: [&str; 6] = ListingType::NAMES;

/// What the form sent, kept as typed so the form can show it again next to its errors.
#[derive(Default)]
pub struct ListingForm {
    category: String,
    listing_type: String,
    condition: String,
    title: String,
    description: String,
    price: String,
    currency: String,
    price_type: String,
    city: String,
    /// A property's zone in the town, a zone's slug; empty for none.
    zone: String,
    name: String,
    /// Posting without being signed in: the phone number or e-mail address to post with.
    contact: String,
    /// "Posto si": the address of the store it is posted as (stores.rs); empty for the poster's own.
    si: String,
    /// A property's or a car's point on the map, from the form's map (map_pin_controller.js);
    /// empty for its town's.
    latitude: String,
    longitude: String,
    /// Hidden from people; see sign_in.html.
    website: String,
    started: Option<i64>,
    /// The category's properties, by code ("vehicle_year"); several values for a multi-select.
    properties: HashMap<String, Vec<String>>,
    /// The photos' order as the form arranged them (photos_controller.js): `saved:<id>` for one
    /// the listing has, `new:<n>` for the n-th sent with it.
    photo_order: Vec<String>,
    /// Whether the browser holds the photos picked, and puts them back on a form sent back with
    /// an error (photos_controller.js), which then needn't ask for them again.
    photos_kept: bool,
}

impl ListingForm {
    /// The form's fields as the browser sent them. Property fields are named after the property
    /// ("prop_vehicle_year"), which differ from category to category, hence pairs, not a struct.
    fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        let mut form = Self::default();
        for (name, value) in pairs {
            if let Some(code) = name.strip_prefix("prop_") {
                form.properties.entry(code.to_string()).or_default().push(value);
                continue;
            }
            let field = match name.as_str() {
                "category" => &mut form.category,
                "listing_type" => &mut form.listing_type,
                "condition" => &mut form.condition,
                "title" => &mut form.title,
                "description" => &mut form.description,
                "price" => &mut form.price,
                "currency" => &mut form.currency,
                "price_type" => &mut form.price_type,
                "city" => &mut form.city,
                "zone" => &mut form.zone,
                "name" => &mut form.name,
                "contact" => &mut form.contact,
                "si" => &mut form.si,
                "website" => &mut form.website,
                "latitude" => &mut form.latitude,
                "longitude" => &mut form.longitude,
                "started" => {
                    form.started = value.parse().ok();
                    continue;
                }
                "photo_order" => {
                    form.photo_order = value.split(',').map(str::trim).filter(|token| !token.is_empty()).map(String::from).collect();
                    continue;
                }
                "photos_kept" => {
                    form.photos_kept = value == "1";
                    continue;
                }
                _ => continue,
            };
            *field = value;
        }
        form
    }

    /// The edit form starts from the listing as it is.
    fn from_listing(listing: OwnListing, properties: Vec<(String, String)>) -> Self {
        let mut values: HashMap<String, Vec<String>> = HashMap::new();
        for (code, value) in properties {
            values.entry(code).or_default().push(value);
        }
        Self {
            category: listing.category,
            listing_type: listing.listing_type,
            condition: listing.condition.unwrap_or_default(),
            title: listing.title,
            description: listing.description,
            // Shown the way the seller would type it: "9500", "12,5".
            price: listing.price_amount.map(|price| price.normalize().to_string().replace('.', ",")).unwrap_or_default(),
            currency: listing.currency,
            price_type: listing.price_type,
            city: listing.city.unwrap_or_default(),
            zone: listing.zone.unwrap_or_default(),
            name: listing.contact_name.unwrap_or_default(),
            contact: String::new(),
            si: String::new(),
            latitude: String::new(),
            longitude: String::new(),
            website: String::new(),
            started: None,
            properties: values,
            photo_order: Vec::new(),
            photos_kept: false,
        }
    }

    /// The listing the form describes, for its rules (listing::check): each choice one of its
    /// kind's values or none, the price read as people type numbers (parse_number), the rest as
    /// typed.
    fn input(&self) -> ListingInput {
        ListingInput {
            category: self.category.clone(),
            listing_type: ListingType::parse(&self.listing_type),
            condition: Condition::parse(&self.condition),
            title: self.title.clone(),
            description: self.description.clone(),
            price_type: PriceType::parse(&self.price_type),
            price: parse_number(&self.price),
            currency: self.currency.clone(),
            city: self.city.clone(),
            zone: self.zone.clone(),
            name: self.name.clone(),
            latitude: self.latitude.clone(),
            longitude: self.longitude.clone(),
        }
    }
}

/// The categories under one heading in the form's select: a category's subcategories, or a
/// category without any on its own (no heading).
pub struct CategoryGroup {
    label: Option<String>,
    options: Vec<CategoryOption>,
}

/// A category in the form's select, with what the form adapts to it (listing_form_controller.js).
pub struct CategoryOption {
    value: String,
    label: String,
    selected: bool,
    types: String,
    uses_condition: bool,
    currency: String,
    /// Its listings can say their zone in the town.
    zoned: bool,
    /// Its listings can say where they are on the map (mapped).
    mapped: bool,
}

/// A town's zones in the form's "Zona" select, under the town, which narrow_controller.js shows
/// alone once the town is chosen.
pub struct ZoneGroup {
    city: String,
    label: String,
    choices: Vec<Choice>,
}

/// The form, for a new listing or for editing one.
#[derive(Template)]
#[template(path = "listing_form.html")]
struct ListingFormPage {
    heading: &'static str,
    action: String,
    submit_label: &'static str,
    /// Where "Anulo" goes when editing: back to the listing.
    cancel_url: Option<String>,
    /// The contact buyers get, masked; None when not signed in: the form then asks for one.
    signed_in_as: Option<String>,
    category_groups: Vec<CategoryGroup>,
    cities: Vec<ChoiceGroup>,
    /// Every town's zones, for those that have known ones.
    zones: Vec<ZoneGroup>,
    /// "Zona" shows: a property in a town with zones (listing_form_controller.js follows).
    zone_shown: bool,
    types: Vec<Choice>,
    conditions: Vec<Choice>,
    price_types: Vec<Choice>,
    currencies: Vec<Choice>,
    /// The chosen category's properties.
    fields: Vec<PropertyField>,
    form: ListingForm,
    errors: HashMap<String, String>,
    started: i64,
    /// When editing, the photos the listing has; each can be removed.
    photos: Vec<OwnPhoto>,
    /// Sent back with an error, photos the browser holds: it puts them back as they were.
    photos_again: bool,
    /// The listing's number on its edit form, which the AI looks at the photos of.
    editing: Option<i64>,
    /// Whether the category chosen can say where it is on the map (mapped), and each town's point
    /// by its slug, as JSON, which the form's map starts on (map_pin_controller.js).
    map_shown: bool,
    towns: String,
    /// "Plotëso pjesën tjetër me AI": on a new listing's form, when the AI is set up.
    ai_fill: bool,
    /// A new listing's form: "Zona" follows the zone its title or description names (guess_zone,
    /// listing_form_controller.js), and its foot names the terms it is posted under.
    new_listing: bool,
    /// The stores a signed-in poster may post for (stores.rs), offered as "Posto si" on a new
    /// listing's form; none, and the form doesn't ask.
    stores: Vec<db::MyStore>,
}

impl ListingFormPage {
    fn error(&self, field: &str) -> Option<&str> {
        self.errors.get(field).map(String::as_str)
    }

    /// How many more photos fit, for a form without the script, whose saved photos stay.
    fn photos_left(&self) -> usize {
        MAX_PHOTOS.saturating_sub(self.photos.len())
    }

    /// Whom buyers reach, as the lead says it: posting for a store ("Posto si", form.si) with a
    /// number of its own, that number; else the poster's contact, masked (signed_in_as), as for a
    /// store without one (Poster::for_store). None for someone not signed in.
    fn contact_shown(&self) -> Option<String> {
        let store = self.stores.iter().find(|store| !self.form.si.is_empty() && store.slug == self.form.si);
        match store {
            Some(store) => Some(self.store_contact(store)),
            None => self.signed_in_as.clone(),
        }
    }

    /// Whom buyers reach posting for `store`: its number, or the poster's own contact when it has
    /// none; for the lead, which the form's script changes as "Posto si" does.
    fn store_contact(&self, store: &db::MyStore) -> String {
        match store.public_phone.as_deref() {
            Some(phone) => format::phone(phone),
            None => self.signed_in_as.clone().unwrap_or_default(),
        }
    }
}

fn choices<'a>(items: impl Iterator<Item = (&'a str, &'a str)>, selected: &str) -> Vec<Choice> {
    items
        .map(|(value, label)| Choice { value: value.to_string(), label: label.to_string(), selected: value == selected })
        .collect()
}

fn signed_in_as(user: &SessionUser) -> String {
    match (&user.phone, &user.email) {
        (Some(phone), _) => Contact::Phone(phone.clone()).masked(),
        (None, Some(email)) => Contact::Email(email.clone()).masked(),
        (None, None) => user.display_name.clone(),
    }
}

/// What the form is for: posting, or editing the listing with this number and slug.
enum Purpose {
    New,
    Edit { public_no: i64, slug: String },
}

fn page(
    purpose: &Purpose,
    user: Option<&SessionUser>,
    categories: &Categories,
    cities: &[City],
    attributes: &[FormAttribute],
    form: ListingForm,
    errors: HashMap<String, String>,
) -> ListingFormPage {
    // Listings go into the most specific category: under each heading, its subcategories. Their
    // names in the page's language.
    let option = |category: &Category| CategoryOption {
        value: category.slug.clone(),
        label: t(&category.name).to_string(),
        selected: category.slug == form.category,
        types: category.listing_types.join(","),
        uses_condition: category.uses_condition,
        currency: category.default_currency.clone(),
        zoned: zoned(categories, category),
        mapped: mapped(categories, category),
    };
    let mut category_groups = Vec::new();
    for top in categories.top_level() {
        match categories.has_children(top.id) {
            true => add_category_groups(categories, top, t(&top.name).to_string(), &option, &mut category_groups),
            false => category_groups.push(CategoryGroup { label: None, options: vec![option(top)] }),
        }
    }
    let currency = match form.currency.as_str() {
        "" => categories.by_slug(&form.category).map_or("EUR", |c| c.default_currency.as_str()),
        chosen => chosen,
    };
    let zones: Vec<ZoneGroup> = cities
        .iter()
        .filter(|city| !city.zones.is_empty())
        .map(|city| ZoneGroup {
            city: city.slug.clone(),
            label: city.name.clone(),
            choices: city
                .zones
                .iter()
                .map(|zone| Choice {
                    value: zone.slug.clone(),
                    label: zone.name.clone(),
                    selected: city.slug == form.city && zone.slug == form.zone,
                })
                .collect(),
        })
        .collect();
    let map_shown = categories.by_slug(&form.category).is_some_and(|category| mapped(categories, category));
    let zone_shown = categories.by_slug(&form.category).is_some_and(|category| zoned(categories, category))
        && zones.iter().any(|group| group.city == form.city);
    let (heading, action, submit_label, cancel_url) = match purpose {
        Purpose::New => (t("Shto një njoftim"), "/shto".to_string(), t("Publiko njoftimin"), None),
        Purpose::Edit { public_no, slug } => (
            t("Ndrysho njoftimin"),
            format!("/listing/{public_no}/ndrysho"),
            t("Ruaj ndryshimet"),
            Some(format!("/listing/{public_no}-{slug}")),
        ),
    };
    ListingFormPage {
        heading,
        action,
        submit_label,
        cancel_url,
        signed_in_as: user.map(signed_in_as),
        category_groups,
        cities: filters::city_groups(cities, Some(form.city.as_str())),
        zones,
        zone_shown,
        types: choices(LISTING_TYPES.iter().map(|code| (*code, format::listing_type(code))), &form.listing_type),
        conditions: choices(CONDITIONS.iter().map(|code| (*code, format::condition(code))), &form.condition),
        price_types: choices(
            PRICE_TYPES.into_iter().map(|(code, label)| (code, t(label))),
            if form.price_type.is_empty() { "FIXED" } else { &form.price_type },
        ),
        currencies: choices([("EUR", "€"), ("ALL", t("Lekë"))].into_iter(), currency),
        fields: properties::fields(attributes, &form.properties, &errors),
        form,
        errors,
        started: now(),
        photos: Vec::new(),
        photos_again: false,
        editing: match purpose {
            Purpose::Edit { public_no, .. } => Some(*public_no),
            Purpose::New => None,
        },
        map_shown,
        towns: serde_json::Value::Object(
            cities.iter().map(|city| (city.slug.clone(), serde_json::json!([city.latitude, city.longitude]))).collect(),
        )
        .to_string(),
        ai_fill: false,
        new_listing: matches!(purpose, Purpose::New),
        stores: Vec::new(),
    }
}

/// What a new listing's form says when it asks for its zone (guess_zone): what the seller wrote,
/// and the town they chose, if any.
#[derive(Deserialize)]
pub struct ZoneWords {
    #[serde(default)]
    title: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    city: String,
}

/// POST /shto/zona: the zone a new listing's title or description names, with its town, as JSON
/// ({"city": "tirane", "zone": "blloku"}), or {} for none (pluribus-services' posting::zone_named,
/// which the API guesses by too), which the form picks while the seller hasn't
/// (listing_form_controller.js): in the town chosen, or, with none chosen, in a town the words
/// name too ("Blloku, Tiranë"). The title first: a description may name the places nearby too.
pub async fn guess_zone(State(state): State<AppState>, Form(words): Form<ZoneWords>) -> Result<Response> {
    let cities = db::cities(&state.db).await?;
    let found = posting::zone_named(&cities, &words.title, &words.description, &words.city);
    let body = match found {
        Some((city, zone)) => serde_json::json!({ "city": city.slug, "zone": zone.slug }),
        None => serde_json::json!({}),
    };
    Ok(Json(body).into_response())
}

/// The categories listings go into under `parent`, at any depth, as the form's groups: one for
/// those right under it, labelled with its path ("Automjete"), then one for each subcategory that
/// has its own ("Automjete › Pjesë"). A select can't nest its groups, hence the path,
/// its names in the page's language.
fn add_category_groups(
    categories: &Categories,
    parent: &Category,
    path: String,
    option: &dyn Fn(&Category) -> CategoryOption,
    groups: &mut Vec<CategoryGroup>,
) {
    let options: Vec<CategoryOption> = categories
        .children(parent.id)
        .filter(|child| !categories.has_children(child.id))
        .map(option)
        .collect();
    if !options.is_empty() {
        groups.push(CategoryGroup { label: Some(path.clone()), options });
    }
    for child in categories.children(parent.id).filter(|child| categories.has_children(child.id)) {
        add_category_groups(categories, child, format!("{path} \u{203a} {}", t(&child.name)), option, groups);
    }
}

/// The form shown again with its errors; Turbo renders it because of the 422 status.
fn form_again(page: ListingFormPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

/// The photos sent, checked and made into their copies, when nothing else is wrong; with an
/// error, none, and unless the browser holds them (`kept`, photos_controller.js), word to pick them
/// again, since a form sent back can't hold files.
async fn read_photos(
    photos: &[axum::body::Bytes],
    kept: bool,
    errors: &mut HashMap<String, String>,
) -> Result<Option<Vec<crate::media::Processed>>> {
    let mut processed = Vec::new();
    if errors.is_empty() && !photos.is_empty() {
        match process_photos(photos.to_vec()).await? {
            Ok(done) => processed = done,
            Err(message) => fail(errors, "photos", message),
        }
    }
    if errors.is_empty() {
        return Ok(Some(processed));
    }
    if !photos.is_empty() && !kept {
        fail(errors, "photos", t("Zgjidh fotot përsëri: formulari i harron kur ka një gabim."));
    }
    Ok(None)
}

/// The first message for a field is the one shown.
fn fail(errors: &mut HashMap<String, String>, field: &str, message: &str) {
    errors.entry(field.to_string()).or_insert_with(|| message.to_string());
}

/// What a listing's rules found wrong, each field's first, said in the page's language
/// (pluribus-services' posting::message, the form's words, which the API says too).
fn add_errors(errors: &mut HashMap<String, String>, found: FieldErrors) {
    for (field, error) in found {
        errors.entry(field.to_string()).or_insert_with(|| message(error));
    }
}

/// The checks a new listing and an edit share: that a person sent the form, neither too soon nor
/// with the field only scripts fill in (as the sign-in form checks), and the listing's own rules
/// (listing::check) on `input`, the form's listing. Problems go into `errors`, one per field.
fn check<'a>(
    form: &ListingForm,
    input: &'a ListingInput,
    categories: &'a Categories,
    cities: &'a [City],
    errors: &mut HashMap<String, String>,
) -> Option<Checked<'a>> {
    let too_fast = form.started.is_none_or(|started| now() - started < MIN_FORM_SECONDS);
    if !form.website.is_empty() || too_fast {
        fail(errors, "form", t("Prit një çast dhe provo përsëri."));
    }
    let (checked, found) = listing::check(input, categories, cities);
    add_errors(errors, found);
    checked
}

/// The form for a new listing. A visitor who isn't signed in fills in a phone number or e-mail
/// address too, which the listing goes under (`create_listing`).
pub async fn new_listing(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let user = current_user(&state, &headers).await?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let mut page = page(&Purpose::New, user.as_ref(), &categories, &cities, &[], ListingForm::default(), HashMap::new());
    page.ai_fill = state.ai.is_some();
    page.stores = posting_stores(&state, user.as_ref()).await?;
    render(page)
}

/// The stores this poster may post for (Role::posts, posting::stores_posting_for); none for someone
/// not signed in.
async fn posting_stores(state: &AppState, user: Option<&SessionUser>) -> Result<Vec<db::MyStore>> {
    let Some(user) = user else {
        return Ok(Vec::new());
    };
    Ok(posting::stores_posting_for(&state.db, user.id).await?)
}

/// The properties of the category the form names, if it is one listings go into
/// (posting::form_attributes_of, which the API's listings take too).
async fn attributes_for(state: &AppState, categories: &Categories, slug: &str) -> Result<Vec<FormAttribute>> {
    Ok(posting::form_attributes_of(&state.db, categories, slug).await?)
}

/// Publishes a listing at once, signed in or not: without signing in, under the account of the
/// phone number or e-mail address typed with it, made with the listing if there is none (below).
/// The same checks and limits apply either way.
pub async fn create_listing(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Submission { pairs, photos, .. }: Submission,
) -> Result<Response> {
    let form = ListingForm::from_pairs(pairs);
    let user = current_user(&state, &headers).await?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let attributes = attributes_for(&state, &categories, &form.category).await?;
    let mut errors = HashMap::new();
    let input = form.input();
    let checked = check(&form, &input, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);
    if let Some(message) = check_photos(&photos, 0) {
        fail(&mut errors, "photos", &message);
    }
    // "Posto si": a store the poster may post for, or their own.
    let stores = posting_stores(&state, user.as_ref()).await?;
    let store = match form.si.as_str() {
        "" => None,
        slug => {
            let store = stores.iter().find(|store| store.slug == slug);
            if store.is_none() {
                fail(&mut errors, "si", t("Zgjidh një nga dyqanet e tua, ose veten."));
            }
            store
        }
    };

    // Not signed in: the listing goes under the phone number or e-mail address typed with it. One
    // no account has makes one, under the name typed, and the listing goes up at once, no code
    // asked: posting is as quick as it gets. One an account has already gets a code, and the
    // listing waits for it (DRAFT): typing it signs the account in and puts the listing up
    // (account.rs, check_code), so that nobody posts under someone else's account, nor signs in
    // to it, by typing their number.
    let contact = match user {
        Some(_) => None,
        None => {
            let contact = Contact::parse(&form.contact);
            if contact.is_none() {
                fail(&mut errors, "contact", t("Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i."));
            }
            contact
        }
    };
    let existing = match &contact {
        Some(contact) => db::account_by_contact(&state.db, contact.phone(), contact.email()).await?,
        None => None,
    };
    if existing.as_ref().is_some_and(|account| account.status == "SUSPENDED") {
        fail(&mut errors, "contact", t("Kjo llogari është pezulluar."));
    }

    // Limits per account: a few listings a day, and the same one only once (posting::may_post).
    let account_id = user.as_ref().map(|user| user.id).or(existing.as_ref().map(|account| account.id));
    if let (true, Some(account_id)) = (errors.is_empty(), account_id)
        && let Err(refused) = posting::may_post(&state.db, account_id, &form.title).await?
    {
        add_errors(&mut errors, vec![(refused.field, refused.error)]);
    }
    // And per network without signing in, since no code stands in a script's way.
    if user.is_none() && errors.is_empty() && !unsigned_post_allowed(ip.as_deref()) {
        fail(&mut errors, "form", t("Shumë njoftime nga ky rrjet brenda një ore. Provo përsëri më vonë."));
    }
    // A contact an account has: the code's own limits, before anything is kept.
    let waits = user.is_none() && existing.is_some();
    if waits
        && errors.is_empty()
        && let Some(contact) = &contact
        && let Some(limit) = account::code_limit(&state, contact, ip.as_deref()).await?
    {
        fail(&mut errors, "form", limit);
    }
    let kept = form.photos_kept && !photos.is_empty();
    let processed = read_photos(&photos, form.photos_kept, &mut errors).await?;
    let (Some(checked), Some(processed)) = (checked, processed) else {
        let mut again = page(&Purpose::New, user.as_ref(), &categories, &cities, &attributes, form, errors);
        again.ai_fill = state.ai.is_some();
        again.stores = stores;
        again.photos_again = kept;
        return form_again(again);
    };

    // All checked: the account of someone not signed in, the one their contact has (whose code
    // the listing waits for), or one made now.
    let poster = match (&user, &contact, &existing) {
        (Some(user), _, _) => match store {
            Some(store) => Poster::for_store(user, store),
            None => Poster::of(user),
        },
        (None, Some(contact), Some(account)) => Poster { id: account.id, phone: contact.phone(), email: contact.email(), business_id: None },
        (None, Some(contact), None) => {
            let name = display_name(&form.name);
            let account = db::poster_account(&state.db, contact.phone(), contact.email(), name.as_deref()).await?;
            if account.status == "PENDING" {
                // Waiting for a code from when posting needed one: it needs none now.
                db::confirm_account(&state.db, account.id, name.as_deref()).await?;
            }
            Poster { id: account.id, phone: contact.phone(), email: contact.email(), business_id: None }
        }
        (None, None, _) => unreachable!("without a contact, the form went back with an error"),
    };
    let slug = format::slug(checked.title);
    // On a site with moderators, what waits for them first (posting::review_for): one with a
    // photo another seller's listing has, the very same file; and, posted without signing in, the
    // first listing of an account made with it, its contact confirmed by no code.
    let hashes: Vec<Vec<u8>> = processed.iter().map(|photo| photo.sha256.to_vec()).collect();
    let review = posting::review_for(&state, &hashes, poster.seller(), user.is_none() && existing.is_none()).await?;
    let mut listing = listing_to_save(&checked, &poster, &slug, &properties, !waits);
    listing.review = review;
    // The photos' files go into the store first, then the listing, with its point and its photos,
    // in one transaction (db::create_listing); should that fail, the files go again, and nothing
    // is left of it.
    let stored = store_photos(&state.media, processed).await?;
    let (listing_id, public_no) = match db::create_listing(&state.db, &listing, checked.point, Photos::Stored(&stored)).await {
        Ok(created) => created,
        Err(error) => {
            unstore(&state.media, &stored).await;
            return Err(error.into());
        }
    };
    let to = format!("/listing/{public_no}-{slug}");
    if user.is_some() {
        // Its page says it is up, once, with ways to share it (listings.rs, just_published).
        let posted = cookie_header(&headers, POSTED_COOKIE, &public_no.to_string(), 600);
        return Ok(redirect_with_cookies(&to, vec![posted]));
    }
    record_unsigned_post(ip.as_deref());
    if let Some(contact) = &contact
        && (waits || review == Some(ReviewReason::NewAccount))
    {
        // To the code page, which says the listing goes up once the code is typed: for a contact
        // an account has, the only way (a code that couldn't go out leaves the listing waiting,
        // to go a day on, db::forget_drafts); for a new account's first listing, the quick way,
        // a moderator's the other, which a code too soon or too many leaves it to.
        let limited = if waits { None } else { account::code_limit(&state, contact, ip.as_deref()).await? };
        if limited.is_none() {
            let purpose = db::CodePurpose { listing_id: Some(listing_id), ..Default::default() };
            match account::send_code_and_ask(&state, &headers, contact, ip.as_deref(), &purpose).await? {
                Ok(response) => return Ok(response),
                Err(message) if waits => {
                    let mut errors = HashMap::new();
                    fail(&mut errors, "form", message);
                    let mut again = page(&Purpose::New, None, &categories, &cities, &attributes, form, errors);
                    again.ai_fill = state.ai.is_some();
                    return form_again(again);
                }
                Err(_) => {}
            }
        }
    }
    // Its page says it is up (or waits for a moderator), once, and how to change it later: by
    // signing in (listings.rs).
    let posted = cookie_header(&headers, POSTED_COOKIE, &public_no.to_string(), 600);
    Ok(redirect_with_cookies(&to, vec![posted]))
}

/// The listing just posted without signing in, by number: its page says it is up.
pub const POSTED_COOKIE: &str = "posted";

/// Listings posted without signing in from one network (IP address) in an hour. With no code to
/// confirm a contact, this keeps a script from filling the site from one place; mobile networks
/// put many people behind one address, hence not fewer.
const UNSIGNED_POSTS_PER_IP_HOUR: usize = 20;

/// When listings were posted without signing in, by network, over the last hour. In memory: a
/// restart forgets them, which costs no more than an hour's limit.
fn unsigned_posts() -> &'static Mutex<HashMap<String, Vec<Instant>>> {
    static POSTS: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();
    POSTS.get_or_init(Default::default)
}

/// Whether this network may post one more listing without signing in; an unknown one may.
fn unsigned_post_allowed(ip: Option<&str>) -> bool {
    let Some(ip) = ip else { return true };
    let hour_ago = Instant::now().checked_sub(Duration::from_secs(3600));
    let mut posts = unsigned_posts().lock().unwrap();
    posts.retain(|_, times| {
        times.retain(|time| hour_ago.is_none_or(|hour_ago| *time > hour_ago));
        !times.is_empty()
    });
    posts.get(ip).map_or(0, Vec::len) < UNSIGNED_POSTS_PER_IP_HOUR
}

fn record_unsigned_post(ip: Option<&str>) {
    if let Some(ip) = ip {
        unsigned_posts().lock().unwrap().entry(ip.to_string()).or_default().push(Instant::now());
    }
}

/// The edit form, for the listing's own seller only; anyone else gets "not found", which tells
/// them nothing about whose listing it is.
pub async fn edit_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    let Some(listing) = db::own_listing(&state.db, public_no, user.id).await? else {
        return not_found().await;
    };
    let (categories, cities, values) = tokio::try_join!(
        db::categories(&state.db),
        db::cities(&state.db),
        db::listing_property_values(&state.db, listing.id),
    )?;
    let attributes = attributes_for(&state, &categories, &listing.category).await?;
    let purpose = Purpose::Edit { public_no, slug: listing.slug.clone() };
    let listing_photos = db::own_listing_photos(&state.db, listing.id).await?;
    // Where it is on the map, when its seller put it there rather than at its town's point.
    let point = db::listing_point(&state.db, listing.id).await?;
    let town = listing.city.as_deref().and_then(|slug| cities.iter().find(|city| city.slug == slug)).map(|town| (town.latitude, town.longitude));
    let mut form = ListingForm::from_listing(listing, values);
    if let (Some(point), Some(town)) = (point, town)
        && distance_km(point, town) > 0.05
    {
        (form.latitude, form.longitude) = (point.0.to_string(), point.1.to_string());
    }
    let mut page = page(&purpose, Some(&user), &categories, &cities, &attributes, form, HashMap::new());
    page.photos = listing_photos;
    page.ai_fill = state.ai.is_some();
    render(page)
}

pub async fn update_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Submission { pairs, photos, .. }: Submission,
) -> Result<Response> {
    // Photos ticked "Hiq"; only the listing's own count, whatever the form says.
    let removed: Vec<uuid::Uuid> = pairs
        .iter()
        .filter(|(name, _)| name == "remove_photo")
        .filter_map(|(_, value)| value.parse().ok())
        .collect();
    let form = ListingForm::from_pairs(pairs);
    let Some(user) = current_user(&state, &headers).await? else {
        return Ok(Redirect::to("/shto").into_response());
    };
    let Some(listing) = db::own_listing(&state.db, public_no, user.id).await? else {
        return not_found().await;
    };
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let purpose = Purpose::Edit { public_no, slug: listing.slug };
    let attributes = attributes_for(&state, &categories, &form.category).await?;
    let mut errors = HashMap::new();
    let input = form.input();
    let checked = check(&form, &input, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);
    let listing_photos = db::own_listing_photos(&state.db, listing.id).await?;
    let removed: Vec<uuid::Uuid> = removed.into_iter().filter(|id| listing_photos.iter().any(|photo| photo.id == *id)).collect();
    if let Some(message) = check_photos(&photos, listing_photos.len() - removed.len()) {
        fail(&mut errors, "photos", &message);
    }
    let kept = form.photos_kept;
    let order = form.photo_order.clone();
    let processed = read_photos(&photos, kept, &mut errors).await?;
    let (Some(checked), Some(processed)) = (checked, processed) else {
        let mut page = page(&purpose, Some(&user), &categories, &cities, &attributes, form, errors);
        page.photos = listing_photos;
        page.photos_again = kept;
        page.ai_fill = state.ai.is_some();
        return form_again(page);
    };
    let slug = format::slug(checked.title);
    let changed = listing_to_save(&checked, &Poster::of(&user), &slug, &properties, true);
    // The new photos' files go into the store first, then the edit, with its point and its photos
    // taken out, added and put in order, in one transaction (db::update_listing_with_drops);
    // should that fail, or the listing no longer be theirs to edit, the new files go again and the
    // listing stays as it was.
    let stored = store_photos(&state.media, processed).await?;
    // In the order the form arranged: a saved photo by its id (one of the listing's, not taken
    // out), a new one by its place among those sent, the first the cover.
    let order: Vec<String> = order
        .iter()
        .filter_map(|token| match token.split_once(':') {
            Some(("saved", id)) => {
                let id: uuid::Uuid = id.parse().ok()?;
                (listing_photos.iter().any(|photo| photo.id == id) && !removed.contains(&id)).then(|| id.to_string())
            }
            Some(("new", place)) => stored.get(place.parse::<usize>().ok()?).map(|photo| photo.storage_key.clone()),
            _ => None,
        })
        .collect();
    let updated = match db::update_listing_with_drops(&state.db, public_no, &changed, checked.point, &removed, Photos::Stored(&stored), &order).await {
        Ok(Some(updated)) => updated,
        Ok(None) => {
            unstore(&state.media, &stored).await;
            return not_found().await;
        }
        Err(error) => {
            unstore(&state.media, &stored).await;
            return Err(error.into());
        }
    };
    // The files of the photos taken out, now that the listing names them no more; and the phones of
    // those it told of a lower price, now that it went through (push.rs), which the page doesn't
    // wait on.
    media::delete_photos(&state.media, &updated.gone).await;
    push::after_price_drops(&state, updated.drops);
    Ok(Redirect::to(&format!("/listing/{public_no}-{slug}")).into_response())
}

#[derive(Template)]
#[template(path = "my_listings.html")]
struct MyListingsPage {
    signed_in_as: String,
    listings: Vec<MyListing>,
    /// What they drew, all of them together.
    totals: db::ListingStats,
    /// For a moderator, how many typed options and reported listings wait on /moderim.
    pending: Option<i64>,
    /// How many of their notifications they haven't seen, by "Lajmërimet" in the account's row.
    notifications: db::Unread,
    /// The stores they post for, each a pill after "Të miat", and the one shown, if one is.
    stores: Vec<db::MyStore>,
    store: Option<db::MyStore>,
    /// The promotions on the listings shown (db/promotions.rs).
    promotions: Vec<db::OnPromotion>,
    /// Each listing's first photo, by its number, at its row's start as the canvas has it.
    photos: HashMap<i64, String>,
}

impl MyListingsPage {
    /// The promotions on the listing with this number now.
    pub fn promoted(&self, public_no: &i64) -> Vec<&db::OnPromotion> {
        self.promotions.iter().filter(|promotion| promotion.public_no == *public_no).collect()
    }

    /// The listing with this number's first photo, where it has one.
    pub fn photo(&self, public_no: &i64) -> Option<&str> {
        self.photos.get(public_no).map(String::as_str)
    }
}

#[derive(Deserialize)]
pub struct WhoseListings {
    /// The store whose listings to show, by its address; the user's own without it.
    dyqani: Option<String>,
}

/// What the pages say of a listing on "Njoftimet e mia": `MyListing`'s own methods in all but name,
/// a trait because the type is pluribus-db's (each method's doc is on its body, below).
pub trait MyListingExt {
    fn url(&self) -> String;
    fn price_parts(&self) -> (String, String);
    fn status(&self) -> Status;
    fn kind(&self) -> Option<ListingType>;
    fn status_label(&self) -> Option<&'static str>;
    fn status_tone(&self) -> Option<&'static str>;
    fn editable(&self) -> bool;
    fn pausable(&self) -> bool;
    fn resumable(&self) -> bool;
    fn sellable(&self) -> bool;
    fn sell_action(&self) -> &'static str;
    fn expires_soon(&self) -> Option<String>;
    fn renewable(&self) -> bool;
}

impl MyListingExt for MyListing {
    fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    /// Its price and what follows it ("/muaj (i diskutueshëm)"), which its row says quieter.
    fn price_parts(&self) -> (String, String) {
        format::price_parts(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// Where it stands. The schema's CHECK allows no status Status lacks (its test holds it to the
    /// migrations); one would count as deleted here: "Jo aktiv", and no button.
    fn status(&self) -> Status {
        Status::parse(&self.status).unwrap_or(Status::Deleted)
    }

    /// Its kind, likewise: one ListingType lacked couldn't be sold, and would be spoken of as one
    /// on sale.
    fn kind(&self) -> Option<ListingType> {
        ListingType::parse(&self.listing_type)
    }

    /// Listings that are no longer simply up say so (Status::seller_label).
    fn status_label(&self) -> Option<&'static str> {
        self.status().seller_label(self.kind().unwrap_or(ListingType::Sell))
    }

    /// The tone its badge takes (Status::tone): a reservation in the warm yellow, a listing expired
    /// or taken off in red's tint; the rest grey (waiting for review, paused, sold).
    fn status_tone(&self) -> Option<&'static str> {
        self.status().tone()
    }

    /// Up, or taken down by its seller for a while: it can be edited.
    fn editable(&self) -> bool {
        self.status().editable()
    }

    /// Up on the site, so it can be taken down for a while ("Çaktivizo").
    fn pausable(&self) -> bool {
        self.status().pausable()
    }

    /// Taken down by its seller, or sold, so it can be put back up ("Aktivizo").
    fn resumable(&self) -> bool {
        self.status().resumable()
    }

    /// Up, and on sale or for rent, so it can be said to be sold ("U shit", "U dha me qira") or
    /// reserved.
    fn sellable(&self) -> bool {
        self.kind().is_some_and(|kind| self.status().sellable(kind))
    }

    /// The button that says it is sold: "U shit", or "U dha me qira" for a rental.
    fn sell_action(&self) -> &'static str {
        format::sell_action(&self.listing_type)
    }

    /// Up or reserved and expiring within a week: "Skadon pas 3 ditësh" (format::expires_soon).
    fn expires_soon(&self) -> Option<String> {
        if !self.status().up() {
            return None;
        }
        format::expires_soon(self.expires_at?, time::OffsetDateTime::now_utc())
    }

    /// Expired, or expiring within a week: "Rifresko" keeps it up db::LISTING_DAYS more.
    fn renewable(&self) -> bool {
        self.status().renewable(self.expires_soon().is_some())
    }
}

/// "Çaktivizo": the seller takes a listing down for a while; only they still see it, on
/// "Njoftimet e mia".
pub async fn pause_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Pause).await
}

/// "Aktivizo": a paused or sold listing goes back up, where it was in "newest first".
pub async fn resume_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Resume).await
}

/// "U shit" ("U dha me qira" for a rental): the listing is sold. Its page stays, saying so, for
/// links shared and kept; it leaves the lists, and no one can ask about it any more.
pub async fn sell_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Sell).await
}

/// "Rezervo": someone is taking it. It stays up, saying so, and can still be asked about.
pub async fn reserve_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Reserve).await
}

/// "Hiq rezervimin": the reservation is off, the listing up as before.
pub async fn unreserve_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Unreserve).await
}

/// "Rifresko": the listing stays up db::LISTING_DAYS more, an expired one back up, and comes back
/// to the top of "newest first" if it was last brought up a week ago or more (db::renew_listing).
/// Back to the page the button was on, or "Njoftimet e mia".
pub async fn renew_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    if !db::renew_listing(&state.db, public_no, user.id).await? {
        return not_found().await;
    }
    let back = saved::referring_page(&headers).unwrap_or_else(|| "/njoftimet-e-mia".to_string());
    Ok(Redirect::to(&back).into_response())
}

/// "Fshi": the listing is gone for good, from the site and from "Njoftimet e mia".
pub async fn delete_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Delete).await
}

/// Only the listing's own seller may; anyone else gets "not found", as for a listing that doesn't
/// exist, and so does an action the listing's state or kind doesn't take. Back to the page the
/// button was on, the listing's own or "Njoftimet e mia", or to "Njoftimet e mia" without one;
/// after deleting, always there, as the listing's page is gone.
async fn change_listing(state: &AppState, headers: &HeaderMap, public_no: i64, action: ListingAction) -> Result<Response> {
    let Some(user) = current_user(state, headers).await? else {
        return render(SignInPage::new());
    };
    if !db::change_own_listing(&state.db, public_no, user.id, action).await? {
        return not_found().await;
    }
    let back = match action {
        ListingAction::Delete => None,
        _ => saved::referring_page(headers),
    };
    Ok(Redirect::to(back.as_deref().unwrap_or("/njoftimet-e-mia")).into_response())
}

/// "Njoftimet e mia": the signed-in seller's listings, each with a way to edit it; with
/// `?dyqani=` those of a store they post for (stores.rs), whoever of its people posted them.
pub async fn my_listings(State(state): State<AppState>, headers: HeaderMap, Query(whose): Query<WhoseListings>) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    let stores = posting_stores(&state, Some(&user)).await?;
    let store = match whose.dyqani.as_deref() {
        None | Some("") => None,
        Some(slug) => match stores.iter().find(|store| store.slug == slug) {
            Some(store) => Some(store.clone()),
            None => return not_found().await,
        },
    };
    let (listings, notifications) = tokio::try_join!(
        db::my_listings(&state.db, user.id, store.as_ref().map(|store| store.id)),
        db::unread_notifications(&state.db, user.id)
    )?;
    let pending = match state.moderators.include(&user) {
        true => {
            let (options, reported, waiting) = tokio::try_join!(
                db::pending_option_count(&state.db),
                db::reported_count(&state.db),
                db::pending_listing_count(&state.db)
            )?;
            Some(options + reported + waiting)
        }
        false => None,
    };
    let totals = listings.iter().map(|listing| listing.stats).sum();
    let numbers: Vec<i64> = listings.iter().map(|listing| listing.public_no).collect();
    let (promotions, photos) =
        tokio::try_join!(db::listing_promotions(&state.db, &numbers), db::first_photos(&state.db, &numbers))?;
    render(MyListingsPage { signed_in_as: signed_in_as(&user), listings, totals, pending, notifications, stores, store, promotions, photos })
}

/// What the pages say of what a listing drew: `ListingStats`'s own methods in all but name, a trait
/// because the type is pluribus-db's (each method's doc is on its body, below).
pub trait ListingStatsExt {
    fn views_title(&self) -> String;
    fn contacts_title(&self) -> String;
    fn saves_title(&self) -> String;
}

/// What a listing drew, or all of a seller's listings together, as "Njoftimet e mia" says it, in
/// the page's language.
impl ListingStatsExt for db::ListingStats {
    /// A listing's views, for the tooltip over their count: "1 234 shikime, 56 këtë javë".
    fn views_title(&self) -> String {
        let views = match self.views {
            1 => t("1 shikim").to_string(),
            views => t1("{} shikime", format::count(views)),
        };
        t2("{}, {} këtë javë", views, format::count(self.views_week))
    }

    /// "12 kontakte, 3 këtë javë": how many times the number or address was shown.
    fn contacts_title(&self) -> String {
        let contacts = match self.contacts {
            1 => t("1 kontakt").to_string(),
            contacts => t1("{} kontakte", format::count(contacts)),
        };
        t2("{}, {} këtë javë", contacts, format::count(self.contacts_week))
    }

    /// "5 ruajtje, 1 këtë javë"; one alone has a text of its own for English ("1 save").
    fn saves_title(&self) -> String {
        let saves = match self.saves {
            1 => t("1 ruajtje").to_string(),
            saves => t1("{} ruajtje", format::count(saves)),
        };
        t2("{}, {} këtë javë", saves, format::count(self.saves_week))
    }
}

/// The last seven days' under a total: "+56 këtë javë", or "Asnjë këtë javë".
pub fn this_week(count: impl std::borrow::Borrow<i64>) -> String {
    match *count.borrow() {
        0 => t("Asnjë këtë javë").to_string(),
        count => t1("+{} këtë javë", format::count(count)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::listing::FieldError;

    /// The form offers the kinds of price the rules take, in their order.
    #[test]
    fn the_form_offers_what_the_listing_rules_take() {
        assert_eq!(PRICE_TYPES.map(|(code, _)| code), PriceType::names(listing::PRICE_TYPES));
    }

    /// Each problem the rules find is said as the form said it before they had codes.
    #[test]
    fn each_problem_is_said_in_the_forms_words() {
        assert_eq!(message(FieldError::TitleLength { chars: 3 }), "Titulli duhet të ketë 5 deri në 120 shkronja.");
        assert_eq!(message(FieldError::PointFar { km: 80.0 }), "Pika në hartë është larg qytetit që zgjodhe: vendose te qyteti, ose hiqe.");
        assert_eq!(message(FieldError::TooManyToday), "Ke publikuar 5 njoftime sot. Mund të publikosh të tjera nesër.");
        let mut errors = HashMap::from([("title".to_string(), "Më parë.".to_string())]);
        add_errors(&mut errors, vec![("title", FieldError::TitleLinks { links: 1 }), ("price", FieldError::NoPrice), ("price", FieldError::NoCity)]);
        assert_eq!(errors["title"], "Më parë.");
        assert_eq!(errors["price"], "Shkruaj çmimin, ose zgjidh „Falas“ ose „Me marrëveshje“.");
    }
}
