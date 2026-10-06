//! Posting and editing listings: the form, its checks, and the limits that keep one account from
//! flooding the site.

use std::collections::HashMap;

use askama::Template;
use axum::{
    Form,
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
    properties::{self, PropertyField},
    render,
};
use crate::{
    AppState,
    db::{
        self, Categories, Category, City, CodePurpose, FormAttribute, ListingProperty, MyListing, NewListing, OwnListing,
        SessionUser,
    },
    format::{self, Contact},
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
}

impl ListingFormPage {
    fn error(&self, field: &str) -> Option<&str> {
        self.errors.get(field).map(String::as_str)
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
    let category_groups = categories
        .top_level()
        .map(|top| match categories.has_children(top.id) {
            true => CategoryGroup { label: Some(top.name.clone()), options: categories.children(top.id).map(option).collect() },
            false => CategoryGroup { label: None, options: vec![option(top)] },
        })
        .collect();
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
    }
}

/// The form shown again with its errors; Turbo renders it because of the 422 status.
fn form_again(page: ListingFormPage) -> Result<Response> {
    Ok((StatusCode::UNPROCESSABLE_ENTITY, render(page)?).into_response())
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
    Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Response> {
    let form = ListingForm::from_pairs(pairs);
    let user = current_user(&state, &headers).await?;
    let (categories, cities) = tokio::try_join!(db::categories(&state.db), db::cities(&state.db))?;
    let attributes = attributes_for(&state, &categories, &form.category).await?;
    let mut errors = HashMap::new();
    let checked = check(&form, &categories, &cities, &mut errors);
    let properties = properties::check(&attributes, &form.properties, &mut errors);

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
    let (Some(checked), Some(poster), true) = (checked, poster, errors.is_empty()) else {
        return form_again(page(&Purpose::New, user.as_ref(), &categories, &cities, &attributes, form, errors));
    };
    let slug = format::slug(checked.title);
    let publish = user.is_some();
    let (listing_id, public_no) = db::create_listing(&state.db, &checked.listing(&poster, &slug, &properties, publish)).await?;
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
    let form = ListingForm::from_listing(listing, values);
    render(page(&purpose, Some(&user), &categories, &cities, &attributes, form, HashMap::new()))
}

pub async fn update_listing(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(public_no): Path<i64>,
    Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Response> {
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
    let Some(checked) = checked.filter(|_| errors.is_empty()) else {
        return form_again(page(&purpose, Some(&user), &categories, &cities, &attributes, form, errors));
    };
    let slug = format::slug(checked.title);
    let listing = checked.listing(&Poster::of(&user), &slug, &properties, true);
    if !db::update_listing(&state.db, public_no, &listing).await? {
        return not_found().await;
    }
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
            _ => Some("Jo aktiv"),
        }
    }

    pub fn editable(&self) -> bool {
        matches!(self.status.as_str(), "ACTIVE" | "RESERVED")
    }
}

/// "Njoftimet e mia": the signed-in seller's listings, each with a way to edit it.
pub async fn my_listings(State(state): State<AppState>, headers: HeaderMap) -> Result<Response> {
    let Some(user) = current_user(&state, &headers).await? else {
        return render(SignInPage::new());
    };
    let listings = db::my_listings(&state.db, user.id).await?;
    render(MyListingsPage { signed_in_as: signed_in_as(&user), listings })
}
