//! Posting and editing listings: the form, its checks, and the limits that keep one account from
//! flooding the site.

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
use rust_decimal::Decimal;
use serde::Deserialize;

use super::{
    Result,
    account::{ClientIp, MIN_FORM_SECONDS, SignInPage, cookie_header, current_user, display_name, now, redirect_with_cookies},
    filters::{self, Choice, ChoiceGroup, parse_number},
    named, not_found,
    photos::{self, Submission},
    properties::{self, PropertyField},
    render, saved,
};
use crate::{
    AppState,
    db::{
        self, Categories, Category, City, FormAttribute, ListingAction, ListingProperty, MyListing, NewListing,
        OwnListing, OwnPhoto, SessionUser, Zone,
    },
    format::{self, Contact},
    media::MAX_PHOTOS,
};

/// At most this many listings a day from one account.
const POSTS_PER_DAY: i64 = 5;
/// The longest title and description a listing takes, in characters.
const MAX_TITLE_CHARS: usize = 120;
const MAX_DESCRIPTION_CHARS: usize = 5000;
/// Links in a listing; spam is mostly links.
const MAX_LINKS: usize = 2;

pub(super) const PRICE_TYPES: [(&str, &str); 4] = [
    ("FIXED", "Çmim fiks"),
    ("NEGOTIABLE", "I diskutueshëm"),
    ("FREE", "Falas"),
    ("ON_REQUEST", "Me marrëveshje"),
];
pub(super) const CONDITIONS: [&str; 4] = ["NEW", "LIKE_NEW", "USED", "FOR_PARTS"];
pub(super) const LISTING_TYPES: [&str; 6] = ["SELL", "BUY", "RENT", "WANTED_RENT", "SERVICE", "JOB"];

/// The category whose listings can say their zone in the town (Blloku, Tiranë), and its
/// subcategories': property.
const ZONED: &str = "prona";

pub(super) fn zoned(categories: &Categories, category: &Category) -> bool {
    category.slug == ZONED || categories.ancestors(category).iter().any(|above| above.slug == ZONED)
}

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
    /// Hidden from people; see sign_in.html.
    website: String,
    started: Option<i64>,
    /// The category's properties, by code ("vehicle_year"); several values for a multi-select.
    properties: HashMap<String, Vec<String>>,
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
                "started" => {
                    form.started = value.parse().ok();
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
            website: String::new(),
            started: None,
            properties: values,
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

    /// How many more photos fit.
    fn photos_left(&self) -> usize {
        MAX_PHOTOS.saturating_sub(self.photos.len())
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
    // Listings go into the most specific category: under each heading, its subcategories.
    let option = |category: &Category| CategoryOption {
        value: category.slug.clone(),
        label: category.name.clone(),
        selected: category.slug == form.category,
        types: category.listing_types.join(","),
        uses_condition: category.uses_condition,
        currency: category.default_currency.clone(),
        zoned: zoned(categories, category),
    };
    let mut category_groups = Vec::new();
    for top in categories.top_level() {
        match categories.has_children(top.id) {
            true => add_category_groups(categories, top, top.name.clone(), &option, &mut category_groups),
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
    let zone_shown = categories.by_slug(&form.category).is_some_and(|category| zoned(categories, category))
        && zones.iter().any(|group| group.city == form.city);
    let (heading, action, submit_label, cancel_url) = match purpose {
        Purpose::New => ("Shto një njoftim", "/shto".to_string(), "Publiko njoftimin", None),
        Purpose::Edit { public_no, slug } => (
            "Ndrysho njoftimin",
            format!("/listing/{public_no}/ndrysho"),
            "Ruaj ndryshimet",
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
        price_types: choices(PRICE_TYPES.into_iter(), if form.price_type.is_empty() { "FIXED" } else { &form.price_type }),
        currencies: choices([("EUR", "€"), ("ALL", "Lekë")].into_iter(), currency),
        fields: properties::fields(attributes, &form.properties, &errors),
        form,
        errors,
        started: now(),
        photos: Vec::new(),
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
/// ({"city": "tirane", "zone": "blloku"}), or {} for none (named::zone_in), which the form picks
/// while the seller hasn't (listing_form_controller.js): in the town chosen, or, with none chosen,
/// in a town the words name too ("Blloku, Tiranë"). The title first: a description may name the
/// places nearby too. As much of the words as the form takes, and no more.
pub async fn guess_zone(State(state): State<AppState>, Form(words): Form<ZoneWords>) -> Result<Response> {
    let cities = db::cities(&state.db).await?;
    let town = cities.iter().find(|city| city.slug == words.city);
    let title: String = words.title.chars().take(MAX_TITLE_CHARS).collect();
    let description: String = words.description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    let found = [title, description].iter().find_map(|text| named::zone_in(text, town, &cities));
    let body = match found {
        Some((city, zone)) => serde_json::json!({ "city": city.slug, "zone": zone.slug }),
        None => serde_json::json!({}),
    };
    Ok(Json(body).into_response())
}

/// The categories listings go into under `parent`, at any depth, as the form's groups: one for
/// those right under it, labelled with its path ("Automjete"), then one for each subcategory that
/// has its own ("Automjete › Pjesë / aksesorë"). A select can't nest its groups, hence the path.
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
        add_category_groups(categories, child, format!("{path} \u{203a} {}", child.name), option, groups);
    }
}

/// The form shown again with its errors; Turbo renders it because of the 422 status.
fn form_again(page: ListingFormPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
}

/// The photos sent with a form that is otherwise right, decoded and resized; None when the form
/// goes back with errors. Browsers can't keep chosen files when it does, so the form says to pick
/// them again.
async fn read_photos(
    photos: &[axum::body::Bytes],
    errors: &mut HashMap<String, String>,
) -> Result<Option<Vec<crate::media::Processed>>> {
    let mut processed = Vec::new();
    if errors.is_empty() && !photos.is_empty() {
        match photos::process(photos.to_vec()).await? {
            Ok(done) => processed = done,
            Err(message) => fail(errors, "photos", message),
        }
    }
    if errors.is_empty() {
        return Ok(Some(processed));
    }
    if !photos.is_empty() {
        fail(errors, "photos", "Zgjidh fotot përsëri: formulari i harron kur ka një gabim.");
    }
    Ok(None)
}

/// The first message for a field is the one shown.
fn fail(errors: &mut HashMap<String, String>, field: &str, message: &str) {
    errors.entry(field.to_string()).or_insert_with(|| message.to_string());
}

pub(super) fn link_count(text: &str) -> usize {
    let text = text.to_lowercase();
    ["http://", "https://", "www."].iter().map(|marker| text.matches(marker).count()).sum()
}

/// A listing as the form describes it, once every field checks out.
struct Checked<'a> {
    category: &'a Category,
    listing_type: &'a str,
    condition: Option<&'static str>,
    title: &'a str,
    description: &'a str,
    price_type: &'static str,
    price: Option<Decimal>,
    currency: &'static str,
    city: &'a City,
    zone: Option<&'a Zone>,
    name: &'a str,
}

/// Who a listing is posted as: the account, the contact buyers get, and the store it is posted
/// for, if any.
struct Poster<'a> {
    id: uuid::Uuid,
    phone: Option<&'a str>,
    email: Option<&'a str>,
    business_id: Option<uuid::Uuid>,
}

impl<'a> Poster<'a> {
    fn of(user: &'a SessionUser) -> Self {
        let phone = user.phone.as_deref();
        Self { id: user.id, phone, email: if phone.is_none() { user.email.as_deref() } else { None }, business_id: None }
    }

    /// Posting for a store: buyers get its public number, or the poster's contact when it has none.
    fn for_store(user: &'a SessionUser, store: &'a db::MyStore) -> Self {
        match store.public_phone.as_deref() {
            Some(phone) => Self { id: user.id, phone: Some(phone), email: None, business_id: Some(store.id) },
            None => Self { business_id: Some(store.id), ..Self::of(user) },
        }
    }
}

impl<'a> Checked<'a> {
    fn listing(
        &self,
        poster: &Poster<'a>,
        slug: &'a str,
        properties: &'a [ListingProperty],
        publish: bool,
    ) -> NewListing<'a> {
        NewListing {
            user_id: poster.id,
            category_id: self.category.id,
            location_id: self.zone.map_or(self.city.id, |zone| zone.id),
            title: self.title,
            slug,
            description: self.description,
            listing_type: self.listing_type,
            condition: self.condition,
            price_amount: self.price,
            currency: self.currency,
            price_type: self.price_type,
            contact_name: (!self.name.is_empty()).then_some(self.name),
            phone: poster.phone,
            email: poster.email,
            properties,
            publish,
            business_id: poster.business_id,
        }
    }
}

/// The checks a new listing and an edit share. Problems go into `errors`, one per field.
fn check<'a>(
    form: &'a ListingForm,
    categories: &'a Categories,
    cities: &'a [City],
    errors: &mut HashMap<String, String>,
) -> Option<Checked<'a>> {
    let too_fast = form.started.is_none_or(|started| now() - started < MIN_FORM_SECONDS);
    if !form.website.is_empty() || too_fast {
        fail(errors, "form", "Prit një çast dhe provo përsëri.");
    }

    let category = categories.by_slug(&form.category);
    if category.is_none() {
        fail(errors, "category", "Zgjidh kategorinë.");
    }
    // A category with subcategories only groups them.
    let category = category.filter(|category| {
        let leaf = !categories.has_children(category.id);
        if !leaf {
            fail(errors, "category", "Zgjidh nënkategorinë, p.sh. Makina te Automjete.");
        }
        leaf
    });
    let listing_type = match category {
        Some(category) if category.listing_types.len() == 1 => Some(category.listing_types[0].as_str()),
        Some(category) => category.listing_types.iter().map(String::as_str).find(|t| *t == form.listing_type),
        None => None,
    };
    if category.is_some() && listing_type.is_none() {
        fail(errors, "listing_type", "Zgjidh llojin e njoftimit.");
    }
    let condition = match category {
        Some(category) if category.uses_condition => CONDITIONS.into_iter().find(|c| *c == form.condition),
        _ => None,
    };

    let title = form.title.trim();
    let description = form.description.trim();
    if !(5..=MAX_TITLE_CHARS).contains(&title.chars().count()) {
        fail(errors, "title", "Titulli duhet të ketë 5 deri në 120 shkronja.");
    } else if link_count(title) > 0 {
        fail(errors, "title", "Titulli nuk mund të ketë lidhje.");
    }
    if !(10..=MAX_DESCRIPTION_CHARS).contains(&description.chars().count()) {
        fail(errors, "description", "Përshkrimi duhet të ketë 10 deri në 5000 shkronja.");
    } else if link_count(description) > MAX_LINKS {
        fail(errors, "description", "Përshkrimi mund të ketë deri në 2 lidhje.");
    }

    let price_type = PRICE_TYPES.iter().map(|(code, _)| *code).find(|code| *code == form.price_type).unwrap_or("FIXED");
    let price = match price_type {
        "FIXED" | "NEGOTIABLE" => match parse_number(&form.price) {
            Some(price) if price > Decimal::ZERO => Some(price.round_dp(2)),
            _ => {
                fail(errors, "price", "Shkruaj çmimin, ose zgjidh \u{201e}Falas\u{201c} ose \u{201e}Me marrëveshje\u{201c}.");
                None
            }
        },
        _ => None,
    };
    let currency = match form.currency.as_str() {
        "ALL" => "ALL",
        _ => "EUR",
    };

    let city = cities.iter().find(|city| city.slug == form.city);
    if city.is_none() {
        fail(errors, "city", "Zgjidh qytetin.");
    }
    // A property may say its zone in the town; another town's is a mistake. Other listings have
    // none.
    let zone = match (category, city) {
        (Some(category), Some(city)) if !form.zone.is_empty() && zoned(categories, category) => {
            let zone = city.zones.iter().find(|zone| zone.slug == form.zone);
            if zone.is_none() {
                fail(errors, "zone", "Zgjidh një zonë të qytetit.");
            }
            zone
        }
        _ => None,
    };
    let name = form.name.trim();
    if name.chars().count() > 60 {
        fail(errors, "name", "Emri mund të ketë deri në 60 shkronja.");
    }

    Some(Checked {
        category: category?,
        listing_type: listing_type?,
        condition,
        title,
        description,
        price_type,
        price,
        currency,
        city: city?,
        zone,
        name,
    })
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

/// The stores this poster may post for (db::POSTING_ROLES); none for someone not signed in.
async fn posting_stores(state: &AppState, user: Option<&SessionUser>) -> Result<Vec<db::MyStore>> {
    let Some(user) = user else {
        return Ok(Vec::new());
    };
    Ok(db::my_stores(&state.db, user.id).await?.into_iter().filter(db::MyStore::posts).collect())
}

/// The properties of the category the form names, if it is one listings go into.
async fn attributes_for(state: &AppState, categories: &Categories, slug: &str) -> Result<Vec<FormAttribute>> {
    match categories.by_slug(slug).filter(|category| !categories.has_children(category.id)) {
        Some(category) => Ok(db::form_attributes(&state.db, category.id).await?),
        None => Ok(Vec::new()),
    }
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
    let checked = check(&form, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);
    photos::check(&photos, 0, &mut errors);
    // "Posto si": a store the poster may post for, or their own.
    let stores = posting_stores(&state, user.as_ref()).await?;
    let store = match form.si.as_str() {
        "" => None,
        slug => {
            let store = stores.iter().find(|store| store.slug == slug);
            if store.is_none() {
                fail(&mut errors, "si", "Zgjidh një nga dyqanet e tua, ose veten.");
            }
            store
        }
    };

    // Not signed in: the listing goes under the phone number or e-mail address typed with it, and
    // the account that has it, or one made with the listing under the name typed. No code, so
    // posting is as quick as it gets; and it signs no one in, which takes a code (account.rs), so
    // typing someone's number never opens their account.
    let contact = match user {
        Some(_) => None,
        None => {
            let contact = Contact::parse(&form.contact);
            if contact.is_none() {
                fail(&mut errors, "contact", "Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i.");
            }
            contact
        }
    };
    let existing = match &contact {
        Some(contact) => db::account_by_contact(&state.db, contact.phone(), contact.email()).await?,
        None => None,
    };
    if existing.as_ref().is_some_and(|account| account.status == "SUSPENDED") {
        fail(&mut errors, "contact", "Kjo llogari është pezulluar.");
    }

    // Limits per account: a few listings a day, and the same one only once.
    let account_id = user.as_ref().map(|user| user.id).or(existing.as_ref().map(|account| account.id));
    if let (true, Some(account_id)) = (errors.is_empty(), account_id) {
        let recent = db::recent_posts(&state.db, account_id, form.title.trim()).await?;
        if recent.count >= POSTS_PER_DAY {
            fail(&mut errors, "form", &format!("Ke publikuar {POSTS_PER_DAY} njoftime sot. Mund të publikosh të tjera nesër."));
        } else if recent.same_title {
            fail(&mut errors, "title", "Ke publikuar tashmë një njoftim me këtë titull sot.");
        }
    }
    // And per network without signing in, since no code stands in a script's way.
    if user.is_none() && errors.is_empty() && !unsigned_post_allowed(ip.as_deref()) {
        fail(&mut errors, "form", "Shumë njoftime nga ky rrjet brenda një ore. Provo përsëri më vonë.");
    }
    let processed = read_photos(&photos, &mut errors).await?;
    let (Some(checked), Some(processed)) = (checked, processed) else {
        let mut again = page(&Purpose::New, user.as_ref(), &categories, &cities, &attributes, form, errors);
        again.ai_fill = state.ai.is_some();
        again.stores = stores;
        return form_again(again);
    };

    // All checked: the account of someone not signed in, made now if their contact has none.
    let poster = match (&user, &contact) {
        (Some(user), _) => match store {
            Some(store) => Poster::for_store(user, store),
            None => Poster::of(user),
        },
        (None, Some(contact)) => {
            let name = display_name(&form.name);
            let account = db::poster_account(&state.db, contact.phone(), contact.email(), name.as_deref()).await?;
            if account.status == "PENDING" {
                // Waiting for a code from when posting needed one: it needs none now.
                db::confirm_account(&state.db, account.id, name.as_deref()).await?;
            }
            Poster { id: account.id, phone: contact.phone(), email: contact.email(), business_id: None }
        }
        (None, None) => unreachable!("without a contact, the form went back with an error"),
    };
    let stored = photos::store(&state.media, processed).await?;
    let slug = format::slug(checked.title);
    let (listing_id, public_no) = db::create_listing(&state.db, &checked.listing(&poster, &slug, &properties, true)).await?;
    db::add_listing_photos(&state.db, listing_id, &stored).await?;
    let to = format!("/listing/{public_no}-{slug}");
    if user.is_some() {
        return Ok(Redirect::to(&to).into_response());
    }
    record_unsigned_post(ip.as_deref());
    // Its page says it is up, once, and how to change it later: by signing in (listings.rs).
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
    let form = ListingForm::from_listing(listing, values);
    let mut page = page(&purpose, Some(&user), &categories, &cities, &attributes, form, HashMap::new());
    page.photos = listing_photos;
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
    let checked = check(&form, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);
    let listing_photos = db::own_listing_photos(&state.db, listing.id).await?;
    let removed: Vec<uuid::Uuid> = removed.into_iter().filter(|id| listing_photos.iter().any(|photo| photo.id == *id)).collect();
    photos::check(&photos, listing_photos.len() - removed.len(), &mut errors);
    let processed = read_photos(&photos, &mut errors).await?;
    let (Some(checked), Some(processed)) = (checked, processed) else {
        let mut page = page(&purpose, Some(&user), &categories, &cities, &attributes, form, errors);
        page.photos = listing_photos;
        return form_again(page);
    };
    let slug = format::slug(checked.title);
    let changed = checked.listing(&Poster::of(&user), &slug, &properties, true);
    if !db::update_listing(&state.db, public_no, &changed).await? {
        return not_found().await;
    }
    let gone = db::remove_listing_photos(&state.db, listing.id, &removed).await?;
    photos::delete(&state.media, &gone).await;
    let stored = photos::store(&state.media, processed).await?;
    db::add_listing_photos(&state.db, listing.id, &stored).await?;
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
}

#[derive(Deserialize)]
pub struct WhoseListings {
    /// The store whose listings to show, by its address; the user's own without it.
    dyqani: Option<String>,
}

impl MyListing {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// Listings that are no longer simply up say so.
    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "ACTIVE" => None,
            "RESERVED" => Some("Rezervuar"),
            "SOLD" => Some(format::sold(&self.listing_type)),
            "EXPIRED" => Some("Skaduar"),
            "PENDING_REVIEW" | "DRAFT" => Some("Në pritje"),
            "PAUSED" => Some("Çaktivizuar"),
            "SUSPENDED" => Some("Në shqyrtim"),
            "REJECTED" => Some("Hequr nga moderatorët"),
            _ => Some("Jo aktiv"),
        }
    }

    /// Up, or taken down by its seller for a while: it can be edited.
    pub fn editable(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED" | "PAUSED")
    }

    /// Up on the site, so it can be taken down for a while ("Çaktivizo").
    pub fn pausable(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED")
    }

    /// Taken down by its seller, or sold, so it can be put back up ("Aktivizo").
    pub fn resumable(&self) -> bool {
        matches!(self.status.as_str(), "PAUSED" | "SOLD")
    }

    /// Up, and on sale or for rent, so it can be said to be sold ("U shit", "U dha me qira") or
    /// reserved.
    pub fn sellable(&self) -> bool {
        self.pausable() && db::SELLABLE_KINDS.contains(&self.listing_type.as_str())
    }

    /// The button that says it is sold: "U shit", or "U dha me qira" for a rental.
    pub fn sell_action(&self) -> &'static str {
        format::sell_action(&self.listing_type)
    }

    /// Up or reserved and expiring within a week: "Skadon pas 3 ditësh" (format::expires_soon).
    pub fn expires_soon(&self) -> Option<String> {
        match self.status.as_str() {
            "ACTIVE" | "RESERVED" => format::expires_soon(self.expires_at?, time::OffsetDateTime::now_utc()),
            _ => None,
        }
    }

    /// Expired, or expiring within a week: "Rifresko" keeps it up db::LISTING_DAYS more.
    pub fn renewable(&self) -> bool {
        self.status == "EXPIRED" || self.expires_soon().is_some()
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
            let (options, reported) = tokio::try_join!(db::pending_option_count(&state.db), db::reported_count(&state.db))?;
            Some(options + reported)
        }
        false => None,
    };
    let totals = listings.iter().map(|listing| listing.stats).sum();
    render(MyListingsPage { signed_in_as: signed_in_as(&user), listings, totals, pending, notifications, stores, store })
}

/// What a listing drew, or all of a seller's listings together, as "Njoftimet e mia" says it.
impl db::ListingStats {
    /// A listing's views, for the tooltip over their count: "1 234 shikime, 56 këtë javë".
    pub fn views_title(&self) -> String {
        let views = match self.views {
            1 => "1 shikim".to_string(),
            views => format!("{} shikime", format::count(views)),
        };
        format!("{views}, {} këtë javë", format::count(self.views_week))
    }

    /// "12 kontakte, 3 këtë javë": how many times the number or address was shown.
    pub fn contacts_title(&self) -> String {
        let contacts = match self.contacts {
            1 => "1 kontakt".to_string(),
            contacts => format!("{} kontakte", format::count(contacts)),
        };
        format!("{contacts}, {} këtë javë", format::count(self.contacts_week))
    }

    /// "5 ruajtje, 1 këtë javë".
    pub fn saves_title(&self) -> String {
        format!("{} ruajtje, {} këtë javë", format::count(self.saves), format::count(self.saves_week))
    }
}

/// The last seven days' under a total: "+56 këtë javë", or "Asnjë këtë javë".
pub fn this_week(count: impl std::borrow::Borrow<i64>) -> String {
    match *count.borrow() {
        0 => "Asnjë këtë javë".to_string(),
        count => format!("+{} këtë javë", format::count(count)),
    }
}
