//! Posting and editing listings: the form, its checks, and the limits that keep one account from
//! flooding the site.

use std::collections::HashMap;

use askama::Template;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use rust_decimal::Decimal;

use super::{
    Result,
    account::{ClientIp, MIN_FORM_SECONDS, SignInPage, code_limit, current_user, display_name, now, send_code_and_ask},
    filters::{self, Choice, ChoiceGroup, parse_number},
    not_found,
    photos::{self, Submission},
    properties::{self, PropertyField},
    render,
};
use crate::{
    AppState,
    db::{
        self, Categories, Category, City, CodePurpose, FormAttribute, ListingAction, ListingProperty, MyListing, NewListing,
        OwnListing, OwnPhoto, SessionUser,
    },
    format::{self, Contact},
    media::MAX_PHOTOS,
};

/// At most this many listings a day from one account.
const POSTS_PER_DAY: i64 = 5;
/// Links in a listing; spam is mostly links.
const MAX_LINKS: usize = 2;

const PRICE_TYPES: [(&str, &str); 4] = [
    ("FIXED", "Çmim fiks"),
    ("NEGOTIABLE", "I diskutueshëm"),
    ("FREE", "Falas"),
    ("ON_REQUEST", "Me marrëveshje"),
];
const CONDITIONS: [&str; 4] = ["NEW", "LIKE_NEW", "USED", "FOR_PARTS"];
const LISTING_TYPES: [&str; 6] = ["SELL", "BUY", "RENT", "WANTED_RENT", "SERVICE", "JOB"];

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
    name: String,
    /// Posting without being signed in: the phone number or e-mail address to post with.
    contact: String,
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
                "name" => &mut form.name,
                "contact" => &mut form.contact,
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
            name: listing.contact_name.unwrap_or_default(),
            contact: String::new(),
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
        types: choices(LISTING_TYPES.iter().map(|code| (*code, format::listing_type(code))), &form.listing_type),
        conditions: choices(CONDITIONS.iter().map(|code| (*code, format::condition(code))), &form.condition),
        price_types: choices(PRICE_TYPES.into_iter(), if form.price_type.is_empty() { "FIXED" } else { &form.price_type }),
        currencies: choices([("EUR", "€"), ("ALL", "Lekë")].into_iter(), currency),
        fields: properties::fields(attributes, &form.properties, &errors),
        form,
        errors,
        started: now(),
        photos: Vec::new(),
    }
}

/// The categories listings go into under `parent`, at any depth, as the form's groups: one for
/// those right under it, labelled with its path ("Automjete"), then one for each subcategory that
/// has its own ("Automjete › Pjesë dhe aksesorë"). A select can't nest its groups, hence the path.
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

fn link_count(text: &str) -> usize {
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
    name: &'a str,
}

/// Who a listing is posted as: the account, and the contact buyers get.
struct Poster<'a> {
    id: uuid::Uuid,
    phone: Option<&'a str>,
    email: Option<&'a str>,
}

impl<'a> Poster<'a> {
    fn of(user: &'a SessionUser) -> Self {
        let phone = user.phone.as_deref();
        Self { id: user.id, phone, email: if phone.is_none() { user.email.as_deref() } else { None } }
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
            location_id: self.city.id,
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
    if !(5..=120).contains(&title.chars().count()) {
        fail(errors, "title", "Titulli duhet të ketë 5 deri në 120 shkronja.");
    } else if link_count(title) > 0 {
        fail(errors, "title", "Titulli nuk mund të ketë lidhje.");
    }
    if !(10..=5000).contains(&description.chars().count()) {
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
        name,
    })
}

/// The form for a new listing. A visitor who isn't signed in fills in a phone number or e-mail
/// address too, and confirms it with a code once the listing is written (`create_listing`).
pub async fn new_listing(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let user = current_user(&state, &headers).await?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    render(page(&Purpose::New, user.as_ref(), &categories, &cities, &[], ListingForm::default(), HashMap::new()))
}

/// The properties of the category the form names, if it is one listings go into.
async fn attributes_for(state: &AppState, categories: &Categories, slug: &str) -> Result<Vec<FormAttribute>> {
    match categories.by_slug(slug).filter(|category| !categories.has_children(category.id)) {
        Some(category) => Ok(db::form_attributes(&state.db, category.id).await?),
        None => Ok(Vec::new()),
    }
}

/// Publishes a listing. Signed in, at once. Otherwise the listing waits as a draft, under an
/// account for the contact typed in (created if new, see db::account_for_contact), and a code goes
/// to that contact; typing it back confirms the contact and publishes the listing
/// (account::check_code). The same checks and limits apply either way.
pub async fn create_listing(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Submission { pairs, photos }: Submission,
) -> Result<Response> {
    let form = ListingForm::from_pairs(pairs);
    let user = current_user(&state, &headers).await?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let attributes = attributes_for(&state, &categories, &form.category).await?;
    let mut errors = HashMap::new();
    let checked = check(&form, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);
    photos::check(&photos, 0, &mut errors);

    // Not signed in: the contact to confirm, if a code may go to it now.
    let contact = match user {
        Some(_) => None,
        None => {
            let contact = Contact::parse(&form.contact);
            match &contact {
                None => fail(&mut errors, "contact", "Shkruaj një numër celulari (p.sh. 069 123 4567) ose një adresë email-i."),
                Some(contact) if errors.is_empty() => {
                    if let Some(limit) = code_limit(&state, contact, ip.as_deref()).await? {
                        fail(&mut errors, "form", limit);
                    }
                }
                Some(_) => {}
            }
            contact
        }
    };
    let poster = match (&user, &contact) {
        (Some(user), _) => Some(Poster::of(user)),
        (None, Some(contact)) if errors.is_empty() => {
            let name = display_name(&form.name);
            let account = db::account_for_contact(&state.db, contact.phone(), contact.email(), name.as_deref()).await?;
            if account.status == "SUSPENDED" {
                fail(&mut errors, "contact", "Kjo llogari është pezulluar.");
            }
            Some(Poster { id: account.id, phone: contact.phone(), email: contact.email() })
        }
        _ => None,
    };

    // Limits per account: a few listings a day, and the same one only once.
    if let (true, Some(poster)) = (errors.is_empty(), &poster) {
        let recent = db::recent_posts(&state.db, poster.id, form.title.trim()).await?;
        if recent.count >= POSTS_PER_DAY {
            fail(&mut errors, "form", &format!("Ke publikuar {POSTS_PER_DAY} njoftime sot. Mund të publikosh të tjera nesër."));
        } else if recent.same_title {
            fail(&mut errors, "title", "Ke publikuar tashmë një njoftim me këtë titull sot.");
        }
    }
    let processed = read_photos(&photos, &mut errors).await?;
    let (Some(checked), Some(poster), Some(processed)) = (checked, poster, processed) else {
        return form_again(page(&Purpose::New, user.as_ref(), &categories, &cities, &attributes, form, errors));
    };
    let stored = photos::store(&state.media, processed).await?;
    let slug = format::slug(checked.title);
    let publish = user.is_some();
    let (listing_id, public_no) = db::create_listing(&state.db, &checked.listing(&poster, &slug, &properties, publish)).await?;
    db::add_listing_photos(&state.db, listing_id, &stored).await?;
    match &contact {
        Some(contact) if !publish => {
            let purpose = CodePurpose { listing_id: Some(listing_id), display_name: display_name(&form.name) };
            match send_code_and_ask(&state, &headers, contact, ip.as_deref(), &purpose).await? {
                Ok(response) => Ok(response),
                Err(message) => {
                    let mut errors = HashMap::new();
                    fail(&mut errors, "contact", message);
                    form_again(page(&Purpose::New, user.as_ref(), &categories, &cities, &attributes, form, errors))
                }
            }
        }
        _ => Ok(Redirect::to(&format!("/listing/{public_no}-{slug}")).into_response()),
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
    Submission { pairs, photos }: Submission,
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
}

impl MyListing {
    pub fn url(&self) -> String {
        format!("/listing/{}-{}", self.public_no, self.slug)
    }

    pub fn price_display(&self) -> String {
        format::price(self.price_amount, &self.currency, &self.price_type, &self.listing_type)
    }

    /// Listings that are no longer up say so; they can't be edited.
    pub fn status_label(&self) -> Option<&'static str> {
        match self.status.as_str() {
            "ACTIVE" => None,
            "RESERVED" => Some("Rezervuar"),
            "SOLD" => Some("Shitur"),
            "EXPIRED" => Some("Skaduar"),
            "PENDING_REVIEW" | "DRAFT" => Some("Në pritje"),
            "PAUSED" => Some("Çaktivizuar"),
            _ => Some("Jo aktiv"),
        }
    }

    pub fn editable(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED")
    }

    /// Up on the site, so it can be taken down for a while ("Çaktivizo").
    pub fn pausable(&self) -> bool {
        self.editable()
    }

    /// Taken down by its seller, so it can be put back ("Aktivizo").
    pub fn paused(&self) -> bool {
        self.status == "PAUSED"
    }
}

/// "Çaktivizo": the seller takes a listing down for a while; only they still see it, on
/// "Njoftimet e mia".
pub async fn pause_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Pause).await
}

/// "Aktivizo": a paused listing goes back up, where it was in "newest first".
pub async fn resume_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Resume).await
}

/// "Fshi": the listing is gone for good, from the site and from "Njoftimet e mia".
pub async fn delete_listing(State(state): State<AppState>, headers: HeaderMap, Path(public_no): Path<i64>) -> Result<Response> {
    change_listing(&state, &headers, public_no, ListingAction::Delete).await
}

/// Only the listing's own seller may; anyone else gets "not found", as for a listing that doesn't
/// exist. Back to "Njoftimet e mia" after.
async fn change_listing(state: &AppState, headers: &HeaderMap, public_no: i64, action: ListingAction) -> Result<Response> {
    let Some(user) = current_user(state, headers).await? else {
        return render(SignInPage::new());
    };
    if !db::change_own_listing(&state.db, public_no, user.id, action).await? {
        return not_found().await;
    }
    Ok(Redirect::to("/njoftimet-e-mia").into_response())
}

/// "Njoftimet e mia": the signed-in seller's listings, each with a way to edit it.
pub async fn my_listings(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    let listings = db::my_listings(&state.db, user.id).await?;
    render(MyListingsPage { signed_in_as: signed_in_as(&user), listings })
}
