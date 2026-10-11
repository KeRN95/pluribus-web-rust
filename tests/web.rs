//! Requests through the full router. Each test gets a fresh database with the migrations applied,
//! plus the fixtures named in its attribute.

use std::sync::{Arc, Mutex, OnceLock};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use marketplace::{Moderators, ai::Ai, media::Media};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

/// The migrations, embedded once for every test (`migrator = "MIGRATOR"` in each attribute):
/// without it, `#[sqlx::test]` reads and embeds all of them again for each test, which took three
/// quarters of this file's build.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!();

/// The tests' moderator (MODERATORS in .env): whoever signs in with this number.
const MODERATOR: &str = "+355690000099";

/// The app, with photos kept in memory, one store for all the tests: their photos' names never
/// collide, and a photo posted in one request is there for the next. MODERATOR moderates.
fn app(pool: PgPool) -> Router {
    marketplace::app_with(pool, media(), Moderators::new([MODERATOR]))
}

fn media() -> Media {
    static MEDIA: OnceLock<Media> = OnceLock::new();
    MEDIA.get_or_init(Media::in_memory).clone()
}

/// The app with an AI to fill in the listing form, which the server at `url` stands in for
/// (ai_stand_in).
fn app_with_ai(pool: PgPool, url: &str) -> Router {
    let ai = Ai::new("test-key", url, "claude-haiku-4-5-20251001");
    marketplace::app_with_ai(pool, media(), Moderators::new([MODERATOR]), Some(ai))
}

struct TestResponse {
    status: StatusCode,
    location: Option<String>,
    content_type: Option<String>,
    /// Set-Cookie headers, as "name=value" without their attributes.
    cookies: Vec<String>,
    headers: axum::http::HeaderMap,
    body: String,
}

impl TestResponse {
    fn cookie(&self, name: &str) -> Option<&str> {
        self.cookies.iter().find_map(|cookie| cookie.strip_prefix(&format!("{name}=")))
    }
}

async fn send(pool: PgPool, request: Request<Body>) -> TestResponse {
    send_to(app(pool), request).await
}

async fn send_to(app: Router, request: Request<Body>) -> TestResponse {
    let response = app.oneshot(request).await.unwrap();
    let header = |name| {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap().to_string())
    };
    let (status, location, content_type) = (
        response.status(),
        header(header::LOCATION),
        header(header::CONTENT_TYPE),
    );
    let cookies = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap().split(';').next().unwrap().to_string())
        .collect();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    TestResponse { status, location, content_type, cookies, headers, body: String::from_utf8_lossy(&body).into_owned() }
}

async fn get(pool: PgPool, uri: &str) -> TestResponse {
    send(pool, Request::get(uri).body(Body::empty()).unwrap()).await
}

/// Posts a form, as a browser would, with the cookie given ("name=value") if any.
async fn post_form(pool: PgPool, uri: &str, fields: &[(&str, &str)], cookie: Option<&str>) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let mut request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    send(pool, request.body(Body::from(body)).unwrap()).await
}

/// A form sent from this network: the address a proxy in front of the app says it came from.
async fn post_form_from(pool: PgPool, uri: &str, fields: &[(&str, &str)], ip: &str) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", ip);
    send(pool, request.body(Body::from(body)).unwrap()).await
}

/// The listing form as the browser sends it with photos: multipart, the photos as files.
async fn post_with_photos(
    pool: PgPool,
    uri: &str,
    fields: &[(&str, &str)],
    photos: &[Vec<u8>],
    cookie: &str,
) -> TestResponse {
    let (content_type, body) = multipart(fields, photos);
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap();
    send(pool, request).await
}

/// A form with photos as the browser sends it: its content type and its body.
fn multipart(fields: &[(&str, &str)], photos: &[Vec<u8>]) -> (String, Vec<u8>) {
    let boundary = "----marketplace-test-boundary";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    for (i, photo) in photos.iter().enumerate() {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"photos\"; filename=\"photo{i}.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(photo);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// A form with named files as the browser sends it (a store's "logo" and "cover"): its content
/// type and its body.
fn multipart_files(fields: &[(&str, &str)], files: &[(&str, Vec<u8>)]) -> (String, Vec<u8>) {
    let boundary = "----marketplace-test-boundary";
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    for (name, bytes) in files {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

/// A store's form sent, with its files, as `cookie`.
async fn post_store(pool: &PgPool, uri: &str, fields: &[(&str, &str)], files: &[(&str, Vec<u8>)], cookie: &str) -> TestResponse {
    let (content_type, body) = multipart_files(fields, files);
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap();
    send(pool.clone(), request).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_store_has_its_page_and_its_people_post_as_it(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let store_row = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
            "SELECT display_name, public_phone::text, logo_key, logo_url FROM businesses WHERE slug = 'auto-elbasani'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // Signed out, the form asks to sign in; signed in, it starts from the account's number.
    assert!(get(pool.clone(), "/dyqani/krijo").await.body.contains(r#"action="/hyr""#));
    let form = get_with_cookie(pool.clone(), "/dyqani/krijo", &ana).await.body;
    assert!(form.contains(r#"<input id="phone" name="phone" value="+355 69 111 2233""#), "{form}");

    // Its fields checked, each problem under its own.
    let refused = post_store(&pool, "/dyqani/krijo", &[("name", "A"), ("phone", "123"), ("website", "dyqani"), ("nipt", "123")], &[], &ana).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    for why in ["Shkruaj emrin e dyqanit", "Shkruaj numrin si", "Shkruaj adresën e faqes", "NIPT-i ka 10 shenja"] {
        assert!(refused.body.contains(why), "{why}");
    }

    // Made: to its page, at an address made from its name, with its logo and cover.
    let fields = [
        ("name", "Auto Elbasani"),
        ("description", "Makina të përdorura nga Gjermania.\n\nShërbim pas shitjes."),
        ("phone", "069 111 2233"),
        ("whatsapp", "1"),
        ("email", "info@autoelbasani.al"),
        ("website", "www.autoelbasani.al"),
        ("city", "elbasan"),
        ("address", "Rruga e Durrësit 10"),
        ("hours", "E hënë – e shtunë, 9:00 – 19:00"),
        ("nipt", "l12345678a"),
    ];
    let made = post_store(&pool, "/dyqani/krijo", &fields, &[("logo", jpeg(80, 60)), ("cover", jpeg(300, 100))], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"), "{}", made.body);
    let (_, phone, logo_key, logo_url) = store_row(pool.clone()).await;
    assert_eq!(phone.as_deref(), Some("+355691112233"));
    let logo_key = logo_key.expect("a logo");
    assert!(media().get(&logo_key).await.unwrap().is_some());
    let page = get_with_cookie(pool.clone(), "/biznes/auto-elbasani", &ana).await.body;
    assert!(page.contains(r#"<h1 translate="no">Auto Elbasani</h1>"#) && page.contains(r#"<img class="store-cover" src="/media/stores/"#), "{page}");
    assert!(page.contains(&format!(r#"<img class="avatar large avatar-logo" src="{}""#, logo_url.unwrap())));
    assert!(page.contains(r#"<p class="store-about">Makina të përdorura nga Gjermania.</p>"#) && page.contains(r#"<p class="store-about">Shërbim pas shitjes.</p>"#));
    assert!(page.contains("<dd>Rruga e Durrësit 10, Elbasan</dd>") && page.contains("<dd>E hënë – e shtunë, 9:00 – 19:00</dd>"));
    assert!(page.contains(r#"href="tel:+355691112233""#) && page.contains(r#"href="https://wa.me/355691112233""#));
    assert!(page.contains(r#"href="mailto:info@autoelbasani.al""#) && page.contains(r#"href="https://www.autoelbasani.al" rel="nofollow ugc noopener" target="_blank">www.autoelbasani.al</a>"#));
    assert!(page.contains(r#"<a class="button secondary small" href="/dyqani/auto-elbasani/ndrysho">Ndrysho dyqanin</a>"#));
    assert!(!get(pool.clone(), "/biznes/auto-elbasani").await.body.contains("Ndrysho dyqanin"));
    assert!(!page.contains("L12345678A"), "the NIPT stays on the form");
    // "Cilësimet" lists it, and makes another.
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(settings.contains(r#"<a href="/biznes/auto-elbasani">Auto Elbasani</a>"#) && settings.contains("Pronar") && settings.contains(r#"href="/dyqani/krijo""#));

    // Posted as the store: its seller the store, its number the store's; its card has the logo
    // and leads to the store, whose page lists it.
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    // Its choice carries whom buyers reach then, its number, which the lead names as it is picked
    // (listing_form_controller.js), as it does on a form sent back as the store (it named her own
    // contact once).
    assert!(form.contains(r#"<option value="auto-elbasani" data-contact="+355 69 111 2233">Auto Elbasani</option>"#), "{form}");
    let back = post_form(pool.clone(), "/shto", &[listing_fields(("title", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&ana)).await;
    assert_eq!(back.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(back.body.contains(r#"Blerësit të kontaktojnë te <strong id="poster-contact">+355 69 111 2233</strong>."#), "{}", back.body);
    let not_hers = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "nje-tjeter")]].concat(), Some(&ana)).await;
    assert_eq!(not_hers.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(not_hers.body.contains("Zgjidh një nga dyqanet e tua, ose veten."));
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&ana)).await;
    let url = posted.location.expect("posted");
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();
    let (seller_user, store_name, contact): (Option<String>, String, Option<String>) = sqlx::query_as(
        "SELECT l.seller_user_id::text, b.display_name, c.phone_e164::text
         FROM listings l JOIN businesses b ON b.id = l.seller_business_id JOIN listing_contacts c ON c.listing_id = l.id
         WHERE l.public_no = $1",
    )
    .bind(no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((seller_user, store_name.as_str(), contact.as_deref()), (None, "Auto Elbasani", Some("+355691112233")));
    let listing = get(pool.clone(), &url).await.body;
    assert!(listing.contains(r#"<img class="avatar avatar-logo"#) && listing.contains(r#"href="/biznes/auto-elbasani""#), "{listing}");
    assert!(card_numbers(&get(pool.clone(), "/biznes/auto-elbasani").await.body).contains(&no));

    // Changed by its owner, nobody else: its name, a fixed line for its number, which its listing
    // shows now, and its logo taken off, the file with it; its address stays.
    assert_eq!(get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/ndrysho", &besa).await.status, StatusCode::NOT_FOUND);
    let form = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/ndrysho", &ana).await.body;
    assert!(form.contains(r#"value="Auto Elbasani""#) && form.contains(r#"name="remove_logo""#) && form.contains(r#"value="L12345678A""#), "{form}");
    let changed = [("name", "Auto Elbasani Plus"), ("phone", "054 222 333"), ("city", "elbasan"), ("remove_logo", "1")];
    let saved = post_store(&pool, "/dyqani/auto-elbasani/ndrysho", &changed, &[], &ana).await;
    assert_eq!(saved.location.as_deref(), Some("/biznes/auto-elbasani"), "{}", saved.body);
    let (name, phone, logo, _) = store_row(pool.clone()).await;
    assert_eq!((name.as_str(), phone.as_deref(), logo), ("Auto Elbasani Plus", Some("+35554222333"), None));
    assert!(media().get(&logo_key).await.unwrap().is_none());
    let contact: Option<String> = sqlx::query_scalar(
        "SELECT c.phone_e164::text FROM listing_contacts c JOIN listings l ON l.id = c.listing_id WHERE l.public_no = $1",
    )
    .bind(no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(contact.as_deref(), Some("+35554222333"));
    assert_eq!(post_store(&pool, "/dyqani/auto-elbasani/ndrysho", &changed, &[], &besa).await.status, StatusCode::NOT_FOUND);

    // Another of the same name gets an address of its own.
    let second = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani Plus")], &[], &besa).await;
    assert_eq!(second.location.as_deref(), Some("/biznes/auto-elbasani-plus"));
    let third = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani Plus")], &[], &besa).await;
    assert_eq!(third.location.as_deref(), Some("/biznes/auto-elbasani-plus-2"));
}

/// The account behind a test session's cookie (session_for).
fn account_of(cookie: &str) -> &str {
    cookie.trim_start_matches("session=test-token-")
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_owner_asks_people_in_who_post_for_it_until_they_go(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let carla = new_seller(&pool, "+355691112255").await;
    let made = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233"), ("city", "elbasan")], &[], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"));
    let invitations = |pool: PgPool| async move {
        sqlx::query_scalar::<_, String>("SELECT id::text FROM business_invitations ORDER BY created_at").fetch_all(&pool).await.unwrap()
    };

    // Its people: its owner alone, on a page for its owner and admins, linked from its page.
    let people = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit", &ana).await.body;
    assert!(people.contains("Përdorues (ti)") && people.contains("Pronar"), "{people}");
    assert!(get_with_cookie(pool.clone(), "/biznes/auto-elbasani", &ana).await.body.contains(r#"href="/dyqani/auto-elbasani/njerezit""#));
    assert_eq!(get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit", &besa).await.status, StatusCode::NOT_FOUND);

    // Asked in by number: the account with it hears of it in "Lajmërimet" and "Cilësimet"; the
    // page has WhatsApp's button with the invitation's link written in, and the link to copy.
    let refused = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "123"), ("roli", "EDITOR")], Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Shkruaj një numër celulari") && refused.body.contains(r#"value="123""#));
    let sent = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2244"), ("roli", "EDITOR")], Some(&ana)).await;
    assert_eq!(sent.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=ftuar"));
    let invitation = invitations(pool.clone()).await.remove(0);
    let people = get_with_cookie(pool.clone(), "/dyqani/auto-elbasani/njerezit?u=ftuar", &ana).await.body.replace("&#38;", "&");
    assert!(people.contains("Ftesat në pritje") && people.contains("+355 69 111 2244") && people.contains("Ftesa u ruajt."), "{people}");
    assert!(people.contains("https://wa.me/355691112244?text=") && people.contains(&format!("/ftesa/{invitation}")), "{people}");
    let heard = get_with_cookie(pool.clone(), "/lajmerimet", &besa).await.body;
    assert!(heard.contains("Auto Elbasani të fton në dyqan") && heard.contains(&format!(r#"href="/ftesa/{invitation}""#)), "{heard}");
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besa).await.body.contains(&format!(r#"action="/ftesa/{invitation}/prano""#)));

    // Its page: the store and the role. Someone else learns whom it is for, and can't take it;
    // someone signed out comes back to it after signing in.
    let theirs = get_with_cookie(pool.clone(), &format!("/ftesa/{invitation}"), &carla).await.body;
    assert!(theirs.contains("Auto Elbasani të fton") && theirs.contains("Poston njoftime") && theirs.contains("+355 69 *** **44"), "{theirs}");
    assert!(!theirs.contains("/prano"));
    assert_eq!(post_form(pool.clone(), &format!("/ftesa/{invitation}/prano"), &[], Some(&carla)).await.status, StatusCode::NOT_FOUND);
    let signed_out = get(pool.clone(), &format!("/ftesa/{invitation}")).await;
    assert!(signed_out.cookie("kthehu").is_some_and(|back| back.contains(&invitation)), "{:?}", signed_out.cookies);

    // Taken: the store is in Besa's "Cilësimet" with her role and the way out, not its people.
    let taken = post_form(pool.clone(), &format!("/ftesa/{invitation}/prano"), &[], Some(&besa)).await;
    assert_eq!(taken.location.as_deref(), Some("/llogaria/cilesimet?u=pranuar"));
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=pranuar", &besa).await.body;
    assert!(settings.contains(r#"<a href="/biznes/auto-elbasani">Auto Elbasani</a>"#) && settings.contains("Poston njoftime"), "{settings}");
    assert!(settings.contains(r#"action="/dyqani/auto-elbasani/largohu""#) && !settings.contains("/dyqani/auto-elbasani/njerezit"));
    assert!(invitations(pool.clone()).await.is_empty());

    // She posts as the store; its owner changes the listing too. Her own listings leave it out,
    // the store's pill lists it, for the store's people alone.
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&besa)).await;
    let url = posted.location.expect("posted");
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.status, StatusCode::OK);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&format!(r#"action="/listing/{no}/caktivizo""#)));
    let hers = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body;
    assert!(hers.contains(r#"href="/njoftimet-e-mia?dyqani=auto-elbasani""#) && !hers.contains(&format!("/listing/{no}-")), "{hers}");
    let stores = get_with_cookie(pool.clone(), "/njoftimet-e-mia?dyqani=auto-elbasani", &ana).await.body;
    assert!(stores.contains(&format!("/listing/{no}-")) && stores.contains("Njoftimet e <strong>Auto Elbasani</strong>"), "{stores}");
    assert_eq!(get_with_cookie(pool.clone(), "/njoftimet-e-mia?dyqani=auto-elbasani", &carla).await.status, StatusCode::NOT_FOUND);

    // Made one who answers its messages alone: no posting as it, no changing its listings.
    let besa_id = account_of(&besa);
    let changed = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}"), &[("roli", "VIEWER")], Some(&ana)).await;
    assert_eq!(changed.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=roli"));
    assert_eq!(get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/shto", &besa).await.body.contains(r#"value="auto-elbasani""#));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besa).await.body.contains("Vetëm mesazhet"));
    // Nobody takes the owner's role or lets the owner go, nor does the owner leave.
    let ana_id = account_of(&ana);
    assert_eq!(post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{ana_id}/hiq"), &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), "/dyqani/auto-elbasani/largohu", &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}"), &[("roli", "OWNER")], Some(&ana)).await.status, StatusCode::NOT_FOUND);

    // Asking in someone who is in already says so; an address hears of it by e-mail, and the
    // invitation can be withdrawn.
    let member = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2244"), ("roli", "ADMIN")], Some(&ana)).await;
    assert_eq!(member.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=anetar"));
    let by_mail = post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "Carla@Example.test"), ("roli", "ADMIN")], Some(&ana)).await;
    assert_eq!(by_mail.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=ftuar"));
    let withdrawn = invitations(pool.clone()).await.remove(0);
    let cancelled = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/ftesat/{withdrawn}/anulo"), &[], Some(&ana)).await;
    assert_eq!(cancelled.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=anuluar"));
    assert!(invitations(pool.clone()).await.is_empty());
    assert_eq!(get_with_cookie(pool.clone(), &format!("/ftesa/{withdrawn}"), &carla).await.status, StatusCode::NOT_FOUND);

    // Turned down, it is gone; taken by Carla, who then leaves.
    post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2255"), ("roli", "EDITOR")], Some(&ana)).await;
    let declined = invitations(pool.clone()).await.remove(0);
    let turned = post_form(pool.clone(), &format!("/ftesa/{declined}/refuzo"), &[], Some(&carla)).await;
    assert_eq!(turned.location.as_deref(), Some("/llogaria/cilesimet?u=refuzuar"));
    assert!(invitations(pool.clone()).await.is_empty());
    post_form(pool.clone(), "/dyqani/auto-elbasani/njerezit", &[("kontakti", "069 111 2255"), ("roli", "EDITOR")], Some(&ana)).await;
    let again = invitations(pool.clone()).await.remove(0);
    post_form(pool.clone(), &format!("/ftesa/{again}/prano"), &[], Some(&carla)).await;
    let left = post_form(pool.clone(), "/dyqani/auto-elbasani/largohu", &[], Some(&carla)).await;
    assert_eq!(left.location.as_deref(), Some("/llogaria/cilesimet?u=larguar"));

    // Besa let go: the listing she posted stays the store's, and hers to change no more.
    let removed = post_form(pool.clone(), &format!("/dyqani/auto-elbasani/njerezit/{besa_id}/hiq"), &[], Some(&ana)).await;
    assert_eq!(removed.location.as_deref(), Some("/dyqani/auto-elbasani/njerezit?u=hequr"));
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM business_members m JOIN businesses b ON b.id = m.business_id WHERE b.slug = 'auto-elbasani'").fetch_one(&pool).await.unwrap();
    assert_eq!(members, 1);
    let paused = post_form(pool.clone(), &format!("/listing/{no}/caktivizo"), &[], Some(&besa)).await;
    let status: String = sqlx::query_scalar("SELECT status FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "ACTIVE", "{:?}", paused.status);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn closing_an_account_closes_the_stores_it_owns(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let made = post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233")], &[("logo", jpeg(80, 80))], &ana).await;
    assert_eq!(made.location.as_deref(), Some("/biznes/auto-elbasani"));
    post_store(&pool, "/dyqani/krijo", &[("name", "Besa Shop")], &[], &besa).await;
    // Each posts for the other's store too.
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, role)
         SELECT b.id, $1::uuid, 'EDITOR' FROM businesses b WHERE b.slug = 'auto-elbasani'
         UNION ALL SELECT b.id, $2::uuid, 'EDITOR' FROM businesses b WHERE b.slug = 'besa-shop'",
    )
    .bind(account_of(&besa))
    .bind(account_of(&ana))
    .execute(&pool)
    .await
    .unwrap();
    let post_as = |cookie: String, store: &'static str| {
        let pool = pool.clone();
        async move {
            let posted = post_form(pool, "/shto", &[listing_fields(("", ""), &started()), vec![("si", store)]].concat(), Some(&cookie)).await;
            posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().parse::<i64>().unwrap()
        }
    };
    let ana_for_hers = post_as(besa.clone(), "auto-elbasani").await;
    let hers_by_ana = post_as(ana.clone(), "besa-shop").await;
    let logo: String = sqlx::query_scalar("SELECT logo_key FROM businesses WHERE slug = 'auto-elbasani'").fetch_one(&pool).await.unwrap();
    assert!(media().get(&logo).await.unwrap().is_some());

    let closed = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", "FSHI")], Some(&ana)).await;
    assert!(closed.body.contains("<h1>Llogaria u fshi</h1>"), "{}", closed.body);
    assert!(closed.body.contains("dyqanet që kishe krijuar u mbyllën bashkë me njoftimet e tyre"), "{}", closed.body);
    // Her store is closed, its listings with it, whoever posted them, and its logo's file.
    let status = |slug: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM businesses WHERE slug = $1").bind(slug).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(status("auto-elbasani").await, "DELETED");
    assert_eq!(get(pool.clone(), "/biznes/auto-elbasani").await.status, StatusCode::NOT_FOUND);
    let listing_status = |no: i64| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE public_no = $1").bind(no).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(listing_status(ana_for_hers).await, "DELETED");
    assert!(media().get(&logo).await.unwrap().is_none());
    // Besa's store stays, with the listing Ana posted for it, and Besa its only one.
    assert_eq!(status("besa-shop").await, "ACTIVE");
    assert_eq!(listing_status(hers_by_ana).await, "ACTIVE");
    let people: i64 = sqlx::query_scalar("SELECT count(*) FROM business_members m JOIN businesses b ON b.id = m.business_id WHERE b.slug IN ('auto-elbasani', 'besa-shop')").fetch_one(&pool).await.unwrap();
    assert_eq!(people, 1);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_sellers_card_says_how_quickly_they_answer_and_when_they_were_here(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let listing = "/listing/1001-volkswagen-golf-7-2016-nafte";
    assert!(!get(pool.clone(), listing).await.body.contains("seller-activity"));
    let seen = |pool: PgPool| async move {
        sqlx::query_as::<_, (Option<time::OffsetDateTime>, time::OffsetDateTime)>("SELECT last_seen_at, updated_at FROM users WHERE id = $1::uuid")
            .bind(ANA_ID)
            .fetch_one(&pool)
            .await
            .unwrap()
    };

    // Signed in, Ana is on the site: today. The moment holds for ten minutes, and the account's
    // own updated_at stays as it was.
    let (_, updated) = seen(pool.clone()).await;
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (first, still) = seen(pool.clone()).await;
    assert!(first.is_some());
    assert_eq!(updated, still);
    sqlx::query("UPDATE users SET last_seen_at = now() - interval '5 minutes' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (kept, _) = seen(pool.clone()).await;
    assert!(kept.unwrap() < time::OffsetDateTime::now_utc() - time::Duration::minutes(4));
    sqlx::query("UPDATE users SET last_seen_at = now() - interval '20 minutes' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await;
    let (moved, _) = seen(pool.clone()).await;
    assert!(moved.unwrap() > time::OffsetDateTime::now_utc() - time::Duration::minutes(1));
    assert!(get(pool.clone(), listing).await.body.contains("<span class=\"seller-activity\">Aktiv\u{a0}sot</span>"));

    // Three buyers wrote, each answered a quarter of an hour on: she answers within the hour. Two
    // wouldn't have been enough to say so.
    for (i, phone) in ["+355691112233", "+355691112244", "+355691112255"].into_iter().enumerate() {
        let buyer = new_seller(&pool, phone).await;
        let thread = write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await.location.unwrap();
        write(&pool, &thread, "Po, është.", &ana).await;
        sqlx::query(
            "UPDATE messages m SET created_at = m.created_at - interval '15 minutes'
             FROM conversations c WHERE c.id = m.conversation_id AND m.sender_user_id = c.buyer_user_id",
        )
        .execute(&pool)
        .await
        .unwrap();
        marketplace::db::refresh_response_times(&pool).await.unwrap();
        let page = get(pool.clone(), listing).await.body;
        assert_eq!(page.contains("Përgjigjet zakonisht"), i == 2, "{i}");
    }
    let page = get(pool.clone(), listing).await.body;
    assert!(page.contains("<span class=\"seller-activity\">Përgjigjet zakonisht brenda 1 ore ·\u{a0}Aktiv\u{a0}sot</span>"), "{page}");
    let profile = get(pool.clone(), &format!("/perdorues/{ANA_ID}")).await.body;
    assert!(profile.contains("<p class=\"profile-activity\">Përgjigjet zakonisht brenda 1 ore ·\u{a0}Aktiv\u{a0}sot</p>"), "{profile}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_people_share_its_conversations(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let carla = new_seller(&pool, "+355691112255").await;
    let dan = new_seller(&pool, "+355691112266").await;
    let erion = new_seller(&pool, "+355691112277").await;
    for (cookie, name) in [(&ana, "Ana"), (&besa, "Besa"), (&carla, "Carla"), (&dan, "Dan")] {
        sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1::uuid").bind(account_of(cookie)).bind(name).execute(&pool).await.unwrap();
    }
    post_store(&pool, "/dyqani/krijo", &[("name", "Auto Elbasani"), ("phone", "069 111 2233")], &[], &ana).await;
    sqlx::query(
        "INSERT INTO business_members (business_id, user_id, role)
         SELECT b.id, p.id::uuid, p.role FROM businesses b, (VALUES ($1, 'EDITOR'), ($2, 'VIEWER')) AS p (id, role)
         WHERE b.slug = 'auto-elbasani'",
    )
    .bind(account_of(&besa))
    .bind(account_of(&carla))
    .execute(&pool)
    .await
    .unwrap();
    // Besa posts for the store; its people don't write to their own store.
    let posted = post_form(pool.clone(), "/shto", &[listing_fields(("", ""), &started()), vec![("si", "auto-elbasani")]].concat(), Some(&besa)).await;
    let no = posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    assert!(get_with_cookie(pool.clone(), &format!("/listing/{no}/mesazh"), &carla).await.location.is_some_and(|to| to.starts_with(&format!("/listing/{no}-"))));

    // Dan writes: every one of the store's people has it unread, in an inbox that names the store.
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &dan).await.location.unwrap();
    for cookie in [&ana, &besa, &carla] {
        assert_eq!(unread(&pool, cookie).await, r#"{"unread":1}"#);
        let inbox = get_with_cookie(pool.clone(), "/mesazhet", cookie).await.body;
        assert!(inbox.contains(r#"<span class="inbox-name" translate="no">Dan <span class="inbox-store">Auto Elbasani</span></span>"#), "{inbox}");
    }
    assert!(!get_with_cookie(pool.clone(), "/mesazhet", &erion).await.body.contains(&thread));
    assert_eq!(get_with_cookie(pool.clone(), &thread, &erion).await.status, StatusCode::NOT_FOUND);

    // Ana answers, though she didn't post it; Dan hears from the store. For Besa, Ana's answer is
    // the store's, on her side with Ana's name, and no news: Dan's message is still unread.
    let page = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    assert!(page.contains(r#"<span class="conversation-store">Për Auto Elbasani</span>"#) && page.contains("A është ende në shitje?"), "{page}");
    assert_eq!(write(&pool, &thread, "Po, është.", &ana).await.location.as_deref(), Some(thread.as_str()));
    let his = get_with_cookie(pool.clone(), &thread, &dan).await.body;
    assert!(his.contains(r#"<li class="message" id="message-"#) && !his.contains("message-author") && his.contains("Auto Elbasani"), "{his}");
    let hers = get_with_cookie(pool.clone(), &thread, &besa).await.body;
    assert!(hers.contains(r#"<span class="message-author">Ana</span>"#), "{hers}");
    assert_eq!(hers.matches(r#"<li class="message mine""#).count(), 1, "{hers}");
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":1}"#);
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":0}"#);
    assert!(get_with_cookie(pool.clone(), "/mesazhet", &besa).await.body.contains("Ana: Po, është."));
    // Read, Besa's count goes; Carla's stays hers.
    let read = attribute(&hers, "data-conversation-read-url-value");
    let last = message_ids(&hers).pop().unwrap();
    post_form(pool.clone(), &format!("{read}?deri={last}"), &[], Some(&besa)).await;
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":0}"#);
    assert_eq!(unread(&pool, &carla).await, r#"{"unread":1}"#);

    // Left unread a while, Dan's next message is news to those who took part, Besa who posted it
    // and Ana who answered, not to Carla; and the store's answer to Dan, under its name, while a
    // colleague's answer tells nobody on the store's side.
    let due = || marketplace::notices::send_due(&pool, "https://pluribus.example");
    write(&pool, &thread, "Sa është çmimi i fundit?", &dan).await;
    minutes_pass(&pool, 4).await;
    due().await.unwrap();
    let notices = message_notices(&pool).await;
    let mut told: Vec<&str> = notices.iter().map(|notice| notice.0.as_str()).collect();
    told.sort_unstable();
    let mut took_part = [account_of(&besa), account_of(&ana)];
    took_part.sort_unstable();
    assert_eq!(told, took_part, "{notices:?}");
    write(&pool, &thread, "32 000 lekë.", &carla).await;
    minutes_pass(&pool, 70).await;
    due().await.unwrap();
    let notices = message_notices(&pool).await;
    let last = notices.last().unwrap();
    assert_eq!(last.0, account_of(&dan));
    assert!(last.1.starts_with("Auto Elbasani"), "{notices:?}");

    // Besa blocks Dan: he can't write to the store; Ana sees it blocked, and lets him again.
    post_form(pool.clone(), &format!("{thread}/blloko"), &[], Some(&besa)).await;
    let refused = write(&pool, &thread, "Përshëndetje?", &dan).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(get_with_cookie(pool.clone(), &thread, &ana).await.body.contains(&format!(r#"action="{thread}/zhblloko""#)));
    post_form(pool.clone(), &format!("{thread}/zhblloko"), &[], Some(&ana)).await;
    assert_eq!(write(&pool, &thread, "Përshëndetje?", &dan).await.location.as_deref(), Some(thread.as_str()));

    // Let go, Besa sees it no more.
    sqlx::query("DELETE FROM business_members WHERE user_id = $1::uuid").bind(account_of(&besa)).execute(&pool).await.unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &thread, &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/mesazhet", &besa).await.body.contains(&thread));
    assert_eq!(unread(&pool, &besa).await, r#"{"unread":0}"#);
}

/// What the AI stand-in was asked: the key each call came with, and the request.
type Asked = Arc<Mutex<Vec<(Option<String>, Value)>>>;

/// A stand-in for Anthropic's Messages API, on a port of its own: it answers every call by calling
/// the listing form's tool with `input`, or, with none, fails as an overloaded API does; and keeps
/// what it was asked. Its address, and what it was asked.
async fn ai_stand_in(input: Option<Value>) -> (String, Asked) {
    let asked: Asked = Arc::default();
    let kept = asked.clone();
    let answer = move |headers: axum::http::HeaderMap, axum::Json(request): axum::Json<Value>| {
        let (kept, input) = (kept.clone(), input.clone());
        async move {
            let key = headers.get("x-api-key").map(|key| key.to_str().unwrap().to_string());
            kept.lock().unwrap().push((key, request));
            match input {
                Some(input) => (
                    StatusCode::OK,
                    axum::Json(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{"type": "tool_use", "id": "toolu_test", "name": "fill_listing_form", "input": input}],
                        "stop_reason": "tool_use",
                        "usage": {"input_tokens": 2000, "output_tokens": 120}
                    })),
                ),
                None => (
                    StatusCode::from_u16(529).unwrap(),
                    axum::Json(json!({"type": "error", "error": {"type": "overloaded_error", "message": "Overloaded"}})),
                ),
            }
        }
    };
    let stand_in = Router::new().route("/v1/messages", axum::routing::post(answer));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, stand_in).await.unwrap() });
    (url, asked)
}

/// Asks `app` to fill in the listing form, as the form's script does, from the network `ip`: the
/// answer's status and its JSON.
async fn ai_fill(app: Router, fields: &[(&str, &str)], photos: &[Vec<u8>], ip: &str) -> (StatusCode, Value) {
    let (content_type, body) = multipart(fields, photos);
    let request = Request::post("/shto/ploteso")
        .header(header::CONTENT_TYPE, content_type)
        .header("x-forwarded-for", ip)
        .body(Body::from(body))
        .unwrap();
    let response = send_to(app, request).await;
    (response.status, serde_json::from_str(&response.body).unwrap_or(Value::Null))
}

/// A plain JPEG of that size, as a camera might send.
fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut out)
        .encode_image(&image::DynamicImage::new_rgb8(width, height).to_rgb8())
        .unwrap();
    out
}

async fn get_with_cookie(pool: PgPool, uri: &str, cookie: &str) -> TestResponse {
    send(pool, Request::get(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await
}

/// When a form was shown: long enough ago for a person to have filled it in.
fn started() -> String {
    (time::OffsetDateTime::now_utc().unix_timestamp() - 30).to_string()
}

/// Signs the user in by writing a session straight into the database; returns the cookie.
async fn session_for(pool: &PgPool, user_id: &str) -> String {
    let token = format!("test-token-{user_id}");
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES (sha256(convert_to($1, 'UTF8')), $2::uuid, now() + interval '1 day')",
    )
    .bind(&token)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
    format!("session={token}")
}

/// A seller who hasn't posted anything yet, signed in with this phone number; returns the cookie.
async fn new_seller(pool: &PgPool, phone: &str) -> String {
    let id: String = sqlx::query_scalar(
        "INSERT INTO users (auth_subject, phone_e164, display_name) VALUES ('phone:' || $1, $1, 'Përdorues')
         RETURNING id::text",
    )
    .bind(phone)
    .fetch_one(pool)
    .await
    .unwrap();
    session_for(pool, &id).await
}

/// The page's HTML with `&` unescaped (Askama writes it as &#38;), so URLs can be compared.
async fn html(pool: PgPool, uri: &str) -> String {
    get(pool, uri).await.body.replace("&#38;", "&")
}

/// The page at `uri`, or the one it sends the visitor on to: a search whose words name filters
/// goes to the page with them on.
async fn followed(pool: PgPool, uri: &str) -> TestResponse {
    let response = get(pool.clone(), uri).await;
    match (response.status, &response.location) {
        (StatusCode::SEE_OTHER, Some(to)) => get(pool, to).await,
        _ => response,
    }
}

/// The id and URL of the lazy frame that loads the next page, if there is one.
fn next_frame(html: &str) -> Option<(String, String)> {
    html.match_indices("<turbo-frame id=\"").find_map(|(start, _)| {
        let tag = &html[start..start + html[start..].find('>')?];
        let attribute = |name: &str| {
            let value_start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
            Some(tag[value_start..value_start + tag[value_start..].find('"')?].to_string())
        };
        Some((attribute("id")?, attribute("src")?))
    })
}

/// The listing numbers of the cards on a page, in order.
fn card_numbers(html: &str) -> Vec<i64> {
    html.match_indices(r#"class="card-link" href="/listing/"#)
        .map(|(start, matched)| {
            let rest = &html[start + matched.len()..];
            rest[..rest.find('-').unwrap()].parse().unwrap()
        })
        .collect()
}

/// A section of cards under a listing, named by its heading's id: from its opening tag to its end.
fn section<'a>(html: &'a str, heading_id: &str) -> &'a str {
    let start = html
        .find(&format!(r#"aria-labelledby="{heading_id}">"#))
        .unwrap_or_else(|| panic!("no section {heading_id}"));
    let rest = &html[start..];
    &rest[..rest.find("</section>").unwrap()]
}

/// The listing numbers of the cards on a page and on every page after it, as scrolling loads them.
async fn every_page(pool: &PgPool, uri: &str) -> Vec<i64> {
    let mut page = html(pool.clone(), uri).await;
    let mut numbers = card_numbers(&page);
    while let Some((frame_id, src)) = next_frame(&page) {
        let request = Request::get(&src).header("turbo-frame", &frame_id).body(Body::empty()).unwrap();
        page = send(pool.clone(), request).await.body.replace("&#38;", "&");
        numbers.extend(card_numbers(&page));
    }
    numbers
}

/// Positions of the texts in the page, which must all be there.
fn positions(html: &str, texts: &[&str]) -> Vec<usize> {
    texts.iter().map(|text| html.find(text).unwrap_or_else(|| panic!("missing {text}"))).collect()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn pages_that_find_nothing_offer_ways_on(pool: PgPool) {
    // Words that find nothing: how to search for more, and a wanted ad for what no one sells yet.
    let page = get(pool.clone(), "/search?q=xyzzyqqq").await.body;
    assert!(page.contains("Nuk u gjet asnjë njoftim.") && page.contains("Kontrollo si i ke shkruar fjalët"), "{page}");
    assert!(page.contains(r#"<a class="button secondary" href="/shto"><svg"#) && page.contains("</svg>Shto një njoftim „Kërkoj të blej“</a>"));
    // An empty category, with no words searched, says neither.
    let empty = get(pool.clone(), "/c/aparate-fotografike").await.body;
    assert!(empty.contains("Nuk u gjet asnjë njoftim.") && !empty.contains("Kontrollo si i ke shkruar") && !empty.contains("Shto një njoftim „Kërkoj të blej“"));
    // An address with nothing: why that may be, a search in every category, and the ways home and
    // to posting, in the site's frame.
    let missing = get(pool.clone(), "/kjo-faqe-nuk-ekziston").await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    assert!(missing.body.contains("<h1>Faqja nuk u gjet</h1>"));
    assert!(missing.body.contains(r#"<form class="search not-found-search" action="/search" method="get" role="search">"#));
    assert!(missing.body.contains(r#"<a class="button" href="/">Faqja kryesore</a>"#) && missing.body.contains(r#"<a class="button secondary" href="/shto">Shto një njoftim</a>"#));
    // A request that fails: the site's frame too, with what to do and the way home, never the error.
    use axum::response::IntoResponse;
    let response = marketplace::web::AppError::from(std::io::Error::other("the database went away")).into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
    assert!(body.contains("<h1>Diçka shkoi keq</h1>") && body.contains(r#"<header class="site-header">"#), "{body}");
    assert!(body.contains(r#"<a href="/kontakt">Kontakt</a>"#) && !body.contains("database"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn home_page_lists_categories(pool: PgPool) {
    let response = get(pool, "/").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Automjete"));
    assert!(response.body.contains("Nuk u gjet asnjë njoftim."));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn welcome_page_asks_for_words_and_offers_the_categories(pool: PgPool) {
    let body = get(pool.clone(), "/").await.body;
    // The question is for screen readers alone, with no line under it; the creature that was the
    // site's icon is on no page (it was the logo's too, and once stood behind the search).
    assert!(body.contains(r#"<h1 class="visually-hidden">Çfarë po kërkon?</h1>"#));
    assert!(!body.contains("Shkruaj atë që të duhet"));
    assert!(!body.contains(r#"<svg class="thinking""#));
    // Words only: no city, no category; the results' filters narrow them. Suggestions open under
    // the words as they are typed.
    assert!(body.contains(r#"<form class="welcome-form" action="/search" method="get" role="search" data-controller="suggest""#));
    assert!(body.contains(r#"<input id="welcome-q" type="search" name="q""#));
    assert!(body.contains(r#"<div class="suggestions" id="welcome-suggestions" role="listbox""#));
    // While the box has words, a cross empties it (suggest_controller.js shows it).
    assert!(body.contains(r#"<button type="button" class="search-clear" aria-label="Pastro" title="Pastro" data-suggest-target="clear" data-action="suggest#clear" hidden>"#));
    // The search button is a magnifier at the end of the field's pill, in the accent; the field
    // asks the page's question.
    assert!(body.contains(r#"<button type="submit" class="icon-button welcome-search" aria-label="Kërko""#));
    assert!(body.contains(r#"placeholder="Çfarë po kërkon?""#));
    // The search leads a row of its own over the categories, with, for phones (the rail has it
    // from 720px up), "Posto", its plus with its word kept for screen readers; no button opens the
    // search, which stands open. The listings kept and the conversations, round buttons after
    // "Posto" until 2026-10-11, went at the user's word (the menu has them); the theme is the
    // menu's too.
    let nav = body.find(r#"<nav class="shortcuts" aria-label="Kategoritë">"#).unwrap();
    let row = positions(&body, &[
        r#"<div class="welcome-tools">"#,
        r#"<form class="welcome-form""#,
        r#"<a class="welcome-post" href="/shto">"#,
    ]);
    assert!(row.is_sorted() && row[2] < nav, "{row:?} before {nav}");
    assert!(!body.contains("circle-shortcut") && !body.contains("saved-shortcut") && !body.contains("inbox-shortcut"), "{body}");
    assert!(body.contains(r#"<span class="welcome-post-word">Posto</span>"#));
    assert!(!body.contains("theme-shortcut"));
    assert!(!body.contains("search-shortcut") && !body.contains("welcome-search-panel") && !body.contains("welcome-scrim"));
    // Then every category's pill straight in the list, no group round any of them, so that they
    // wrap wherever the line ends: Automjete, Prona, Elektronikë and Veshje / aksesorë first, then
    // every other category in its order.
    let list = &body[nav..];
    let list = &list[..list.find("</nav>").unwrap()];
    assert!(!list.contains("<div") && !body.contains("welcome-main"), "{list}");
    let order = positions(list, &[
        r#"<a class="shortcut" href="/c/automjete">"#,
        r#"<a class="shortcut" href="/c/prona">"#,
        r#"<a class="shortcut" href="/c/elektronike">"#,
        r#"<a class="shortcut" href="/c/veshje-dhe-aksesore">"#,
        r#"<a class="shortcut" href="/c/shtepi-dhe-kopsht">"#,
        r#"<a class="shortcut" href="/c/sport-dhe-hobi">"#,
        r#"<a class="shortcut" href="/c/kafshe-shtepiake">"#,
        r#"<a class="shortcut" href="/c/te-tjera">"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!list.contains("circle-shortcut"), "{list}");
    // Every page's top bar has the account alone at its right end, as the user asked on 2026-10-11,
    // the welcome page's too: the text's size and the theme are the menu's, posting, the listings
    // kept and the conversations the rail's (on phones the welcome page's row has posting, and the
    // menu the others).
    let cars = get(pool.clone(), "/c/automjete").await.body;
    for page in [&body, &cars] {
        assert!(!page.contains("post-button"));
        // The typeface panel ("Ff") stands there only while TYPE_TRIAL is on (.env).
        let trial = std::env::var("TYPE_TRIAL").is_ok_and(|value| value == "true");
        assert_eq!(page.contains(r#"<details class="type-menu" data-controller="dropdown type-trial""#), trial);
        let bar = &page[page.find(r#"<header class="site-header">"#).unwrap()..page.find("</header>").unwrap()];
        assert_eq!(bar.matches("icon-button").count(), 1 + usize::from(trial), "{bar}");
        assert!(bar.contains(r#"<a class="icon-button account-button" href="/llogaria""#));
        for gone in ["theme-toggle", "text-size-menu", "saved-button", "inbox-button"] {
            assert!(!bar.contains(gone), "{gone} in {bar}");
        }
    }
    assert!(!body.contains("saved-button") && !body.contains("inbox-button"));
    assert!(!cars.contains("circle-shortcut"));
    assert!(!body.contains("data-panel-target") && !body.contains("<noscript>"));
    assert!(!body.contains(r#"name="city""#) && !body.contains(r#"name="category""#));
    // A shortcut to every main category: Automjete, Prona, Elektronikë and Veshje / aksesorë
    // first, then the others in their order, each its icon before its name; no more listing-like
    // cards.
    let main = [
        ("automjete", "Automjete"),
        ("prona", "Prona"),
        ("elektronike", "Elektronikë"),
        ("veshje-dhe-aksesore", "Veshje / aksesorë"),
        ("shtepi-dhe-kopsht", "Shtëpi / kopsht"),
        ("pune", "Punë"),
        ("sherbime", "Shërbime"),
        ("sport-dhe-hobi", "Sport / hobi"),
        ("kafshe-shtepiake", "Kafshë shtëpiake"),
        ("makineri-dhe-biznes", "Makineri / biznes"),
        ("te-tjera", "Të tjera"),
    ];
    let links: Vec<String> =
        main.iter().map(|(slug, _)| format!(r#"<a class="shortcut" href="/c/{slug}">"#)).collect();
    let shortcuts = positions(&body, &links.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(shortcuts.is_sorted(), "{shortcuts:?}");
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), main.len());
    for (_, name) in main {
        assert!(body.contains(&format!("</svg>{name}</a>")), "{name}");
    }
    // Each with its icon.
    assert_eq!(body.matches(r#"<a class="shortcut""#).count(), body.matches(r#"<svg class="shortcut-icon""#).count());
    assert!(!body.contains(r#"class="tile""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn welcome_page_ignores_the_city_it_used_to_remember(pool: PgPool) {
    // The welcome page no longer asks for a city, so the one an older visit left in a cookie
    // would narrow the categories unseen: it is ignored.
    let request = Request::get("/")
        .header(header::COOKIE, "consent=1; city=vlore")
        .body(Body::empty())
        .unwrap();
    let body = send(pool, request).await.body;
    assert!(body.contains(r#"<a class="shortcut" href="/c/automjete">"#));
    assert!(!body.contains("city=vlore"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_welcome_page_shows_the_listings_seen_lately(pool: PgPool) {
    // None seen, no such row.
    assert!(!html(pool.clone(), "/").await.contains("Shikuar së fundmi"));
    // A listing's page gives its number, which application.js puts first in the cookie as the page
    // shows; its seller's own page gives none.
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    assert!(html(pool.clone(), golf).await.contains(r#" data-listing-number="1001""#));
    let ana = session_for(&pool, ANA_ID).await;
    assert!(!get_with_cookie(pool.clone(), golf, &ana).await.body.contains("data-listing-number"));
    // The cookie's listings, the latest first and each once, those still on the site: a paused
    // one, a number no listing has and what isn't a number are left out.
    sqlx::query("UPDATE listings SET status = 'PAUSED' WHERE public_no = 1002").execute(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), "/", "shikuar=1005.1002.999999.x.1001.1005").await.body;
    let row = section(&page, "seen-heading");
    assert_eq!(card_numbers(row), [1005, 1001]);
    assert!(row.contains(r#"<h2 id="seen-heading">Shikuar së fundmi</h2>"#) && row.contains(r#"data-action="recent#forget">Pastro</button>"#), "{row}");
    // Over the newest listings, its cards with ids of their own, the first photo the page's lead.
    assert!(page.find(r#"id="seen-heading""#) < page.find(r#"id="latest-heading""#));
    assert!(row.contains(r#"id="seen-1005""#) && row.contains(r#"fetchpriority="high""#), "{row}");
    // A sold one stays, saying so over its photo.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1005").execute(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), "/", "shikuar=1005.1001").await.body;
    let row = section(&page, "seen-heading");
    assert_eq!(card_numbers(row), [1005, 1001]);
    assert!(row.contains(r#"<p class="card-status is-sold">Shitur</p>"#), "{row}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_with_words_says_what_and_where_for_the_last_searches(pool: PgPool) {
    // What it searched and where, which application.js keeps among the last searches.
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<section class="results" aria-labelledby="results-heading" data-search-words="golf" data-search-address="/search?q=golf">"#), "{search}");
    let cars = html(pool.clone(), "/c/makina?q=golf&vehicle_fuel=DIESEL").await;
    assert!(cars.contains(r#"data-search-words="golf" data-search-address="/c/makina?q=golf&vehicle_fuel=DIESEL""#), "{cars}");
    // Words are the visitor's own: what would end the attribute or open a tag is escaped.
    let odd = html(pool.clone(), "/search?q=golf%22%3E%3Cb").await;
    let tag = &odd[odd.find(r#"<section class="results""#).unwrap()..];
    let tag = &tag[..tag.find('>').unwrap()];
    assert!(tag.contains("data-search-words=\"golf&#34;&#62;&#60;b\"") || tag.contains("data-search-words=\"golf&quot;&gt;&lt;b\""), "{tag}");
    // A page without words has nothing to keep.
    assert!(!html(pool.clone(), "/c/makina").await.contains("data-search-words"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn bumped_listing_comes_first(pool: PgPool) {
    let body = get(pool, "/").await.body;
    let house = body.find("Shtëpi private në shitje në Vlorë").unwrap();
    let phone = body.find("iPhone 13, 128 GB, si i ri").unwrap();
    assert!(house < phone, "the house was bumped after the phone was published");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_welcome_page_goes_from_its_categories_to_the_newest_listings(pool: PgPool) {
    let body = html(pool.clone(), "/").await;
    // Under the categories, with no listing seen yet, the newest listings straight away, in the
    // grid of the browse pages' small cards: no row of them over it, nor a stream of new ones.
    assert!(!body.contains("Sapo u shtuan") && !body.contains("/te-rejat") && !body.contains("data-cursor"), "{body}");
    let latest = section(&body, "latest-heading");
    assert!(latest.contains(&format!(r#"sizes="{SMALL_SIZES}""#)), "{latest}");
    // Its first photo comes before anything else on the page: on a phone, the biggest thing on the
    // first screen.
    assert_eq!(body.matches(r#"fetchpriority="high""#).count(), 1);
    let first = &latest[latest.find(r#"<article class="card""#).unwrap()..];
    assert!(first[..first.find("</article>").unwrap()].contains(r#"fetchpriority="high""#), "{latest}");
    // The stream's address is gone.
    assert_eq!(get(pool, "/te-rejat").await.status, StatusCode::NOT_FOUND);
}

/// The heart over listing `number`'s photo.
fn heart(number: i64) -> String {
    format!(r#"<form class="heart" method="post" action="/listing/{number}/ruaj" data-controller="heart" data-heart-number-value="{number}""#)
}

/// Taps the heart of listing `number` as heart_controller.js does: `keep` "1" or "0".
fn tap_heart(number: &str, keep: &str, cookie: Option<&str>) -> Request<Body> {
    let mut request = Request::post(format!("/listing/{number}/ruaj"))
        .header(header::ACCEPT, "application/json")
        .header(header::HOST, "localhost")
        .header(header::REFERER, "http://localhost/c/automjete?vehicle_fuel=DIESEL")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    request.body(Body::from(format!("ruaj={keep}"))).unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_listing_has_a_heart_over_its_photo(pool: PgPool) {
    // On every card, the newest listings' row's too, and on the listing's page; every page's rail
    // leads to the listings kept, and the menu does (the welcome page's round button in the row
    // with its search, on phones, went on 2026-10-11).
    let home = get(pool.clone(), "/").await.body;
    assert_eq!(home.matches(r#"<form class="heart""#).count(), home.matches(r#"<article class="card"#).count());
    assert!(home.contains(r#"<a class="menu-row" href="/te-ruajturat">"#) && !home.contains("saved-shortcut"));
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(cars.contains(r#"<a class="rail-item rail-saved" href="/te-ruajturat">"#));
    assert!(cars.contains(&heart(1001)));
    assert_eq!(cars.matches(r#"<form class="heart""#).count(), cars.matches(r#"<article class="card"#).count());
    let listing = get(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    assert!(listing.contains(&heart(1001)));
    assert!(listing.contains(r#"<button type="submit" class="heart-button" aria-pressed="false" aria-label="Ruaje""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_heart_keeps_a_listing_for_a_signed_in_user(pool: PgPool) {
    let session = new_seller(&pool, "+355690000011").await;
    let kept = |pool: PgPool| {
        let session = session.clone();
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
            let mut numbers: Vec<i64> = serde_json::from_value(answer["numbers"].clone()).unwrap();
            numbers.sort();
            numbers
        }
    };
    for number in ["1001", "1001", "1005"] {
        assert_eq!(send(pool.clone(), tap_heart(number, "1", Some(&session))).await.status, StatusCode::NO_CONTENT);
    }
    assert_eq!(kept(pool.clone()).await, [1001, 1005]);
    // Found again on their page, the last kept first.
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &session).await.body;
    assert!(page.contains("<h1>Të ruajturat</h1>"));
    assert_eq!(card_numbers(&page), [1005, 1001]);
    // Let go.
    assert_eq!(send(pool.clone(), tap_heart("1005", "0", Some(&session))).await.status, StatusCode::NO_CONTENT);
    assert_eq!(kept(pool.clone()).await, [1001]);
    assert_eq!(card_numbers(&get_with_cookie(pool.clone(), "/te-ruajturat", &session).await.body), [1001]);
    // No such listing.
    assert_eq!(send(pool.clone(), tap_heart("999999", "1", Some(&session))).await.status, StatusCode::NOT_FOUND);
    // Without JavaScript, the form keeps the listing and goes back to its page.
    let request = Request::post("/listing/1003/ruaj")
        .header(header::HOST, "localhost")
        .header(header::REFERER, "http://localhost/c/automjete")
        .header(header::COOKIE, &session)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("ruaj=1"))
        .unwrap();
    assert_eq!(send(pool.clone(), request).await.location.as_deref(), Some("/c/automjete"));
    assert_eq!(kept(pool.clone()).await, [1001, 1003]);
    // Nobody signed in keeps nothing, and their page says how to.
    assert_eq!(get(pool.clone(), "/te-ruajturat/numrat").await.body, r#"{"numbers":[],"price_drops":0,"searches":0}"#);
    let page = get(pool, "/te-ruajturat").await.body;
    assert!(page.contains(r#"<a class="button" href="/llogaria">Hyr</a>"#) && card_numbers(&page).is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_heart_tapped_before_signing_in_keeps_the_listing_once_signed_in(pool: PgPool) {
    // Not signed in: the heart says so, and the listing and the page wait in cookies.
    let tapped = send(pool.clone(), tap_heart("1001", "1", None)).await;
    assert_eq!(tapped.status, StatusCode::UNAUTHORIZED);
    let waiting = format!("ruaj={}; kthehu={}", tapped.cookie("ruaj").unwrap(), tapped.cookie("kthehu").unwrap());
    assert_eq!(waiting, "ruaj=1001; kthehu=/c/automjete?vehicle_fuel=DIESEL");

    let sent = post_form(pool.clone(), "/hyr", &[("contact", "069 123 4567"), ("started", &started())], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&format!("{login}; {waiting}"))).await;
    // Signed in: back to the page, with the listing kept, and the cookies gone.
    assert_eq!(signed.location.as_deref(), Some("/c/automjete?vehicle_fuel=DIESEL"));
    assert_eq!((signed.cookie("ruaj"), signed.cookie("kthehu")), (Some(""), Some("")));
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert_eq!(get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body, r#"{"numbers":[1001],"price_drops":0,"searches":0}"#);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn category_page_includes_subcategories(pool: PgPool) {
    // Automjete has no listings of its own; the cars sit in its Makina subcategory.
    let response = get(pool, "/c/automjete").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("iPhone 13"));
    assert!(response.body.contains(r#"<p class="result-count" id="result-count" data-show="Shfaq 2 njoftime">2 njoftime</p>"#));
    // Cards show the cover photo's thumbnail.
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=480"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn card_details_start_with_the_city(pool: PgPool) {
    // The city, with no pin before it, in a span of its own (the list hides it), then the
    // category's card properties.
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(!cars.contains(r#"<p class="card-summary"><svg"#));
    assert!(cars.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2+1 · 85\u{a0}m²</p>"));
    // Rents are per month.
    assert!(property.contains("450\u{a0}€<span class=\"card-note\">/muaj</span>"));
    // Without properties, the line is just the city.
    let other = get(pool.clone(), "/c/te-tjera").await.body;
    assert!(other.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Berat</span></p>"));
    // A phone shows its storage on the card; its brand only on its page.
    let phones = get(pool.clone(), "/c/elektronike").await.body;
    assert!(phones.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>128 GB</p>"));
    // Under the price, for the list, where it is: the pin and the town, in every card, in a
    // search in that town too.
    assert!(phones.contains(r#"<p class="card-place" translate="no"><svg class="pin-icon" width="14" height="14""#));
    // Phosphor's map pin (icons.rs, PIN), as every icon of the site is since 2026-10-11.
    assert!(phones.contains(r#"aria-hidden="true"><g transform="scale(.09375)" stroke="none"><path d="M128,24a80,80,0,0,0-80,80"#));
    assert!(phones.contains("</g></svg>Tiranë</p>"));
    // Searching in Tirana, the Tirana cards leave the city out and show the brand instead...
    let in_tirana = get(pool.clone(), "/c/elektronike?city=tirane").await.body;
    assert!(in_tirana.contains(r#"<p class="card-summary">Apple · 128 GB</p>"#));
    assert!(in_tirana.contains("</g></svg>Tiranë</p>"));
    // ...and a car the first property its title doesn't name: not the make, nor the model.
    let cars = get(pool, "/c/automjete?city=tirane").await.body;
    assert!(cars.contains("<p class=\"card-summary\">Hatchback · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_links_to_its_listing_through_the_title(pool: PgPool) {
    let body = get(pool, "/").await.body;
    // The title link is stretched over the whole card (see .card-link in page.css).
    assert!(body.contains(
        r#"<h2 class="card-title"><a class="card-link" href="/listing/1001-volkswagen-golf-7-2016-nafte">"#
    ));
    // Every listing once in the grid.
    assert_eq!(body.matches(r#"<article class="card" id="listing-"#).count(), 12);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_map_pin_finds_the_listings_within_its_radius_nearest_first(pool: PgPool) {
    // Tirana's centre, where the fixture puts the Tirana listings. Durrës is 31 km away.
    let body = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=10").await;
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>0\u{a0}m · 2016 · 148\u{a0}000\u{a0}km · Naftë</p>"));
    assert!(body.contains(r#"<a class="filter-chip" href="/search" aria-label="Hiq filtrin Brenda 10 km nga pika">"#));
    assert!(body.contains(r#"<input class="visually-hidden" type="radio" name="sort" value="" form="filter-form" data-action="filters#submit" checked>Më afër</label>"#));

    // From Durrës, 50 km reaches Tirana: Durrës's listings first, then Tirana's.
    let body = get(pool, "/search?lat=41.31333&lng=19.44583&radius=50").await.body;
    let order = positions(&body, &[
        "Mercedes-Benz C 220, 2018, automatike",
        "Volkswagen Golf 7, 2016, naftë",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(body.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>31\u{a0}km · 2016"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn nearest_pages_continue_where_the_last_one_ended(pool: PgPool) {
    // 40 more in Të tjera, north of Tirana's centre at nine distances, so many are tied and the
    // id has to break the ties.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, latitude, longitude, status,
                               published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 41.328333 + g % 9 * 0.01, 19.818056, 'ACTIVE', now()
         FROM generate_series(1, 40) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let mut seen = Vec::new();
    let mut uri = "/c/te-tjera?lat=41.32833&lng=19.81806&radius=25".to_string();
    let mut frame_id = None;
    loop {
        let mut request = Request::get(&uri);
        if let Some(id) = &frame_id {
            request = request.header("turbo-frame", id);
        }
        let page = send(pool.clone(), request.body(Body::empty()).unwrap()).await.body;
        let page = page.replace("&#38;", "&");
        seen.extend(card_numbers(&page));
        let Some((id, src)) = next_frame(&page) else { break };
        (frame_id, uri) = (Some(id), src);
    }
    let expected: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings
         WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
           AND earth_distance(ll_to_earth(41.32833, 19.81806), ll_to_earth(latitude::float8, longitude::float8)) <= 25000
         ORDER BY earth_distance(ll_to_earth(41.32833, 19.81806), ll_to_earth(latitude::float8, longitude::float8)), id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(expected.len(), 40, "the book listing in Berat is 70 km away");
    assert_eq!(seen, expected);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_map_page_reloads_only_its_results(pool: PgPool) {
    let page = get(pool.clone(), "/harta").await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains(r#"data-controller="map""#));
    assert!(page.body.contains(r#"<turbo-frame id="map-results""#));
    // The button that folds the map away (map_controller.js).
    assert!(page.body.contains(r#"data-action="map#toggleMap""#) && page.body.contains("Fshih hartën</button>"));
    // Until the map is moved, its pin is in the middle of Tirana, and so are the results: not the
    // Mercedes in Durrës, 31 km away. The map is the pin here, so no chip offers to drop it.
    assert!(page.body.contains(r#"data-map-latitude-value="41.3275" data-map-longitude-value="19.8187""#));
    assert!(page.body.contains(r#"<input type="hidden" name="lat" value="41.32750" data-map-target="latitude">"#));
    assert!(page.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!page.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!page.body.contains("Hiq filtrin Brenda"));
    // A dot for each result, with what its popup shows.
    assert!(page.body.contains(
        "<li data-lat=\"41.328333\" data-lng=\"19.818056\" data-url=\"/listing/1001-volkswagen-golf-7-2016-nafte\" \
         data-price=\"9\u{a0}500\u{a0}€\">Volkswagen Golf 7, 2016, naftë</li>"
    ));

    // A pin and a category: what's within the circle, in the category.
    let body = html(pool.clone(), "/harta?lat=41.32833&lng=19.81806&radius=5&category=makina").await;
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("iPhone 13"));
    assert!(body.contains(r#"data-map-latitude-value="41.32833""#));
    assert!(body.contains(r#"<input type="hidden" name="lat" value="41.32833" data-map-target="latitude">"#));

    // The results frame gets the whole page, which Turbo takes the frame out of.
    let request = Request::get("/harta?lat=41.32833&lng=19.81806&radius=5")
        .header("turbo-frame", "map-results")
        .body(Body::empty())
        .unwrap();
    assert!(send(pool.clone(), request).await.body.contains("<html"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_map_page_sets_its_map_from_a_box_beside_it(pool: PgPool) {
    // As the canvas lays it out: the map, then the box of what sets it, one under another: how far,
    // the category, a town to move the map onto, "Vendndodhja ime", then "Vendos hartën" and the
    // button that folds the map; the results under both, after how many there are.
    let page = html(pool.clone(), "/harta").await;
    let order = positions(&page, &[
        r#"<div class="map-layout">"#,
        r#"<div class="map-canvas" id="map-canvas""#,
        r#"<form id="map-form" class="map-form map-controls""#,
        r#"<div class="field map-radius">"#,
        r#"<label class="field-label" for="map-category">Kategoria</label>"#,
        r#"<label class="field-label" for="map-town">Qyteti</label>"#,
        r#"<button type="button" class="button secondary map-locate" data-action="map#locate">"#,
        r#"<div class="map-actions">"#,
        r#"<button type="button" class="button map-confirm" data-action="map#confirm">Vendos hartën</button>"#,
        r#"<turbo-frame id="map-results""#,
        r#"<div class="map-count">"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    // The town moves the map, and says so until one is picked.
    assert!(page.contains(r#"<select id="map-town" aria-label="Qyteti" data-map-target="town" data-action="map#goToTown">"#));
    assert!(page.contains(r#"<option value="">Zgjidh qytetin</option>"#));
    // "Vendndodhja ime" has the crosshair (Phosphor's, its ring the tone), here and in a category's
    // map card.
    let crosshair = r#"<path d="M160,128a32,32,0,1,1-32-32A32,32,0,0,1,160,128Z"/>"#;
    assert!(page[page.find("map-locate").unwrap()..].contains(crosshair));
    let cars = html(pool, "/c/makina").await;
    assert!(cars[cars.find(r#"class="map-pill map-locate""#).unwrap()..].contains(crosshair));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn any_whole_distance_up_to_50_km_can_be_picked(pool: PgPool) {
    let seven = html(pool.clone(), "/search?lat=41.32833&lng=19.81806&radius=7").await;
    assert!(seven.contains("Brenda 7 km nga pika"));
    // Out of range or not a whole number: the usual 10 km.
    for radius in ["0", "51", "2.5"] {
        let page = html(pool.clone(), &format!("/search?lat=41.32833&lng=19.81806&radius={radius}")).await;
        assert!(page.contains("Brenda 10 km nga pika"), "{radius}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filters_map_and_order_stand_side_by_side(pool: PgPool) {
    // Filtrat, Harta and Rendit stand in one row over the results (the search is the top bar's),
    // the orders opening in a menu; opening one says so (section:open) and the others fold, so only
    // one is open at a time. What they open floats over the results, the page dimmed under it;
    // Escape, or a tap on the dim or on the row beside its buttons, folds it again.
    // The filters' card comes after the row and the places under it: a column beside the results
    // on a wide screen (browse.css).
    let cars = html(pool.clone(), "/c/makina").await;
    let order = positions(&cars, &[
        // The heading's line first: the page's name, how many, and "Ruaj kërkimin".
        r#"<h1 class="browse-title">Makina</h1>"#,
        r#"<p class="result-count" id="result-count""#,
        r#"<form class="save-search" method="post" action="/kerkimet/ruaj">"#,
        r#"<div class="browse-tools" data-filters-target="tools" data-action="click->filters#foldFromRow">"#,
        r#"<button type="button" class="button secondary round-button labelled filters-open""#,
        r#"<section class="map-card map-hidden""#,
        // After the map, how the cards show, round, opening its menu; last, the order.
        r#"<details class="sort-menu cards-menu" data-controller="dropdown" data-cards-target="menu""#,
        r#"<summary class="button secondary round-button cards-button" aria-label="Pamja: Kartela të vogla""#,
        r#"<summary class="sort-button" aria-label="Rendit: Më të rejat" title="Rendit: Më të rejat">Më të rejat<svg"#,
        r#"<aside class="filters" id="filters" aria-labelledby="filters-heading" data-filters-target="panel">"#,
        r#"<div class="browse-scrim" aria-hidden="true" data-filters-target="scrim" data-action="click->filters#foldAll"></div>"#,
        // Then the listings, under their heading, their order.
        r#"<section class="results""#,
        r#"<div class="results-head">"#,
        r#"<h2 class="results-heading" id="results-heading">Njoftimet më të reja</h2>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    for listener in ["filters#foldFor", "map#foldFor", "dropdown#foldFor", "filters#fit"] {
        assert!(cars.contains(&format!("section:open@document->{listener}")), "{listener}");
    }
    assert!(cars.contains("keydown.esc@document->filters#foldAll"));
    // The filters and the map apply as soon as something changes, the filters also as they close,
    // the page refreshing in place, which keeps what is open as it is and each listing's card;
    // only without JavaScript do the filters have a button.
    assert!(cars.contains(r#"data-action="change->filters#changed keydown.enter->filters#enter">"#));
    assert!(cars.contains(r#"<noscript><div class="filter-actions">"#));
    for keep in [
        "turbo:before-morph-element->filters#keep",
        "turbo:before-morph-attribute->filters#keepAttribute",
        "turbo:before-morph-element->map#keep",
        "turbo:before-morph-attribute->map#keepOpen",
        "turbo:morph@document->map#reloadPoints",
    ] {
        assert!(cars.contains(keep), "{keep}");
    }
    assert!(cars.contains(r#"<article class="card" id="listing-1001">"#));
    // The filters' and the map's buttons say their names and what is on: "Filtrat", with how many
    // (none yet), and the map's place, "Harta" with none; the cards' view has its icon alone.
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button labelled map-button" data-map-target="toggle""#));
    assert!(cars.contains(r#"<span class="button-label">Harta</span>"#));
    assert!(cars.contains(r#"<button type="button" class="button secondary round-button labelled filters-open" aria-controls="filters" aria-expanded="false""#));
    assert!(cars.contains(r#"<span class="button-label">Filtrat</span>"#) && !cars.contains("button-count"));
    assert!(!cars.contains("round-button search-button"));
    // The order is the row's last button, its words and a chevron, the orders in a menu under it
    // (open with no JavaScript); the listings' heading says it: "Njoftimet më të reja" on a
    // category page.
    let row = &cars[cars.find(r#"<div class="browse-tools""#).unwrap()..cars.find(r#"<aside class="filters""#).unwrap()];
    assert_eq!(row.matches("cards-button").count(), 1);
    assert!(row.contains(r#"<fieldset class="sort-options">"#));
    assert!(!cars[cars.find(r#"<div class="results-head">"#).unwrap()..].contains("sort-menu"));
    let cheap = html(pool.clone(), "/c/makina?sort=price_asc").await;
    assert!(cheap.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të lira</h2>"#), "{cheap}");
    assert!(cars.contains("toggle->dropdown#announce"));
    // A search's listings are headed by the order alone, the best match first for words.
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<summary class="sort-button" aria-label="Rendit: Më të përshtatshmet" title="Rendit: Më të përshtatshmet">Më të përshtatshmet<svg"#));
    assert!(search.contains(r#"<h2 class="results-heading" id="results-heading">Më të përshtatshmet</h2>"#));

    // The search is the top bar's (base.html, browse.html), the row has none: it searches the
    // page's category and keeps its filters.
    let header = &cars[..cars.find("</header>").unwrap()];
    assert!(header.contains(r#"name="q""#) && !row.contains(r#"name="q""#));
    let fuel = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&sort=price_asc").await;
    assert!(fuel.contains(r#"<span class="button-label">Filtrat</span><span class="button-count" aria-hidden="true">1</span>"#));
    let search = &fuel[fuel.find(r#"<form class="top-search" action="/c/makina""#).expect("the bar's search")..];
    let search = &search[..search.find("</form>").unwrap()];
    assert!(search.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(search.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(search.contains(r#"<input type="search" name="q" value="""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn prona_shows_big_cards_until_the_visitor_chooses_small_ones_there(pool: PgPool) {
    let big = r#"<div class="browse cards-big" data-controller="filters cards place""#;
    let small = r#"<div class="browse" data-controller="filters cards place""#;
    // Prona and the categories under it: big cards, their own cookie for the view button...
    let prona = get(pool.clone(), "/c/prona").await.body;
    assert!(prona.contains(big) && prona.contains(r#"data-cards-cookie-value="cards-prona""#), "{prona}");
    assert!(prona.contains(r#"aria-label="Pamja: Kartela të mëdha""#));
    assert!(get(pool.clone(), "/c/apartamente").await.body.contains(big));
    // ...which a choice on other pages leaves as they are; one made there doesn't.
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards=small").await.body.contains(big));
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards-prona=small").await.body.contains(small));
    // Other categories: small cards, unless the visitor chose big ones.
    let cars = get(pool.clone(), "/c/automjete").await.body;
    assert!(cars.contains(small) && cars.contains(r#"data-cards-cookie-value="cards""#));
    assert!(get_with_cookie(pool.clone(), "/c/automjete", "cards-prona=big").await.body.contains(small));
    assert!(get_with_cookie(pool, "/c/automjete", "cards=big").await.body.contains(big));
}

/// How wide the cards' photos show, for the browser to pick a copy (Cards::sizes in browse.rs): a
/// column of the grid, a big card, the list's photo, and a card in a row that slides sideways
/// (ROW_SIZES).
const SMALL_SIZES: &str = "(min-width: 1100px) 255px, (min-width: 960px) calc((100vw - 80px) / 4), (min-width: 724px) calc((100vw - 64px) / 3), (min-width: 720px) calc((100vw - 48px) / 2), calc((100vw - 38px) / 2)";
const BIG_SIZES: &str = "(min-width: 1100px) 522px, (min-width: 960px) calc((100vw - 56px) / 2), (min-width: 720px) 640px, (min-width: 588px) 560px, calc(100vw - 28px)";
const LIST_SIZES: &str = "(min-width: 720px) 240px, 116px";
const ROW_SIZES: &str = "(min-width: 720px) 236px, 180px";

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn big_cards_slide_through_every_photo(pool: PgPool) {
    // Two more photos for the Golf, after its cover.
    sqlx::query(
        "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url, sort_order)
         SELECT l.id, 'IMAGE', 'READY', 'extra/' || n, 'https://example.com/full-' || n || '.jpg',
                'https://example.com/card-' || n || '.jpg', n
         FROM listings l, generate_series(1, 2) n WHERE l.public_no = 1001",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Small cards unless the visitor chose big ones: a small card shows its first photo (page.css),
    // the copy fetched for a column of the grid.
    let small = get(pool.clone(), "/c/makina").await.body;
    assert!(small.contains(r#"<div class="browse" data-controller="filters cards place""#));
    assert!(small.contains(r#"aria-label="Pamja: Kartela të vogla""#));
    assert!(small.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));

    // Big ones, from the cookie the view menu sets: every photo, the cover first, each with its
    // full copy for the browser to pick, and the dots and arrows to slide through them.
    let request = Request::get("/c/makina").header("cookie", "cards=big").body(Body::empty()).unwrap();
    let big = send(pool.clone(), request).await.body;
    assert!(big.contains(r#"<div class="browse cards-big" data-controller="filters cards place""#));
    assert!(big.contains(r#"aria-label="Pamja: Kartela të mëdha""#));
    assert!(!big.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
    assert_eq!(big.matches(r#"<div class="card-media" data-controller="slides">"#).count(), 1, "only the Golf has more than one");
    let golf = &big[big.find(r#"<div class="card-media" data-controller="slides">"#).unwrap()..];
    let golf = &golf[..golf.find("</article>").unwrap()];
    assert!(golf.contains(r#"href="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    assert_eq!(golf.matches(r#"<a class="card-slide""#).count(), 3);
    let cover = golf.find("photo-1591230740238").unwrap();
    assert!(cover < golf.find("card-1.jpg").unwrap() && golf.find("card-1.jpg").unwrap() < golf.find("card-2.jpg").unwrap());
    assert!(golf.contains(&format!(r#"srcset="https://example.com/card-2.jpg 600w, https://example.com/full-2.jpg 1600w" sizes="{BIG_SIZES}""#)));
    assert_eq!(golf.matches(r#"<span class="is-current"></span><span></span><span></span>"#).count(), 1, "a dot for each photo");
    // Over the photo's top left, who sells it, their initial in a circle, and since when they are
    // a user.
    assert!(golf.contains(r#"<p class="card-seller"><span class="avatar" aria-hidden="true">A</span><span class="card-seller-text"><span class="card-seller-name" translate="no">Ana</span><span>Përdorues që nga "#));
    assert!(golf.contains(r#"aria-label="Fotoja tjetër" hidden>"#));

    // The next pages come the same way, and the map page's list stays small.
    let pinned = Request::get("/harta").header("cookie", "cards=big").body(Body::empty()).unwrap();
    assert!(send(pool, pinned).await.body.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_list_shows_a_line_for_each_listing(pool: PgPool) {
    // The view menu: small cards, big ones or the list, the one now ticked and its icon on the
    // pill; picking one switches the cards at once (cards_controller.js).
    let cars = get(pool.clone(), "/c/makina").await.body;
    let menu = &cars[cars.find(r#"<details class="sort-menu cards-menu""#).unwrap()..];
    let menu = &menu[..menu.find("</details>").unwrap()];
    let options = positions(menu, &[
        r#"value="small" data-action="change->cards#choose" checked>"#,
        r#"value="big" data-action="change->cards#choose">"#,
        r#"value="list" data-action="change->cards#choose">"#,
    ]);
    assert!(options.is_sorted(), "{options:?}");
    assert!(menu.contains("</svg>Kartela të vogla</label>") && menu.contains("</svg>Listë</label>"));
    assert!(cars.contains(&format!(r#"data-cards-list-value="{LIST_SIZES}""#)));

    // Chosen, from its cookie: the page says so, the photos fetched for the line's left, each
    // card saying when it was posted after its details line (page.css leaves that out of cards).
    let request = Request::get("/c/makina").header("cookie", "cards=list").body(Body::empty()).unwrap();
    let list = send(pool.clone(), request).await.body;
    assert!(list.contains(r#"<div class="browse cards-list" data-controller="filters cards place""#));
    assert!(list.contains(r#"aria-label="Pamja: Listë""#));
    assert!(list.contains(r#"value="list" data-action="change->cards#choose" checked>"#));
    assert!(list.contains(&format!(r#"sizes="{LIST_SIZES}""#)));
    let golf = &list[list.find(r#"<article class="card" id="listing-1001">"#).unwrap()..];
    let golf = &golf[..golf.find("</article>").unwrap()];
    assert!(golf.contains("Naftë</p>\n    <p class=\"card-when\">"));

    // Prona keeps its own: the list chosen elsewhere leaves its big cards; the map's list is small.
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards=list").await.body.contains(r#"<div class="browse cards-big""#));
    assert!(get_with_cookie(pool.clone(), "/c/prona", "cards-prona=list").await.body.contains(r#"<div class="browse cards-list""#));
    let map = Request::get("/harta").header("cookie", "cards=list").body(Body::empty()).unwrap();
    assert!(send(pool, map).await.body.contains(&format!(r#"sizes="{SMALL_SIZES}""#)));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn category_pages_keep_their_map_folded(pool: PgPool) {
    // Every category page has its map, in a card folded to its header; with no point, the
    // results are from everywhere. Nothing else opens it.
    let cars = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await;
    assert!(cars.contains(r#"<section class="map-card map-hidden" data-controller="map" data-map-lazy-value="true""#));
    assert!(cars.contains(r#"aria-controls="map-card-body" aria-expanded="false""#));
    assert!(cars.contains(r#"aria-label="Harta: Kudo" title="Harta: Kudo""#));
    // With no point, it starts on Tirana, its circle round the pin all the same (map_controller.js).
    assert!(!cars.contains("data-map-latitude-value") && !cars.contains("data-map-ring-value"));
    assert!(!cars.contains("map-open") && !cars.contains("Brenda 10 km nga pika"));
    // Its dots come when it first opens: the same page with harta=1, whose frame has them.
    assert!(cars.contains(r#"<turbo-frame id="map-points" data-map-target="frame" data-src="/c/makina?vehicle_fuel=DIESEL&harta=1">"#));
    assert!(!cars.contains(r#"data-url="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    let dots = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&harta=1").await;
    assert!(dots.contains(r#"data-url="/listing/1001-volkswagen-golf-7-2016-nafte""#));
    // Its form sets the point, keeping the filters, by itself once the map stops: the card has no
    // buttons, and the point's chip drops it.
    assert!(cars.contains(r#"<form class="map-form" action="/c/makina" method="get" data-map-target="form">"#));
    assert!(cars.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(!cars.contains("map#confirm") && !cars.contains("map-everywhere"));
    // Beside "Vendndodhja ime", a pill, the towns' menu, by county, all of Albania first, each the
    // page in it with its point, which the map moves onto; it has no name, so the map's form sends
    // no town of its own.
    assert!(cars.contains(r#"<button type="button" class="map-pill map-locate" data-action="map#locate">"#));
    assert!(cars.contains(r#"<select aria-label="Qyteti" data-map-target="town" data-action="map#pickTown">"#));
    assert!(cars.contains(r#"<option value="tirane" data-lat="41.328333" data-lng="19.818056" data-url="/c/makina?city=tirane&vehicle_fuel=DIESEL">Tiranë</option>"#));

    // With a point, the page stays the category's, with its heading and filters, the card still
    // folded, saying how far around the point; the filter form and the category menu keep it.
    let pinned = html(pool.clone(), "/c/makina?lat=41.32833&lng=19.81806&radius=5&sort=price_asc").await;
    assert!(pinned.contains(r#"<h1 class="browse-title">Makina</h1>"#));
    assert!(pinned.contains(r#"<section class="map-card map-hidden""#));
    assert!(pinned.contains(r#"data-map-latitude-value="41.32833""#));
    // The map's button says where, the point's nearest town and how far, and is marked as set;
    // the filters' summary leaves the point to it, and to its chip. Its circle shows from the
    // start.
    assert!(pinned.contains(r#"aria-label="Harta: Tiranë · 5 km" title="Harta: Tiranë · 5 km""#));
    assert!(pinned.contains(r#"<span class="button-label">Tiranë · 5 km</span>"#));
    assert!(pinned.contains(r#"class="button secondary round-button labelled map-button is-set""#));
    assert!(pinned.contains(r#"aria-label="Filtrat: Asnjë filtër""#));
    // The distance is what the circle covers at the map's zoom (map_controller.js), said beside its
    // name and carried by the radius field: no slider.
    assert!(pinned.contains(r#"<p class="map-distance"><span class="visually-hidden">Largësia </span><output data-map-target="radiusLabel">5 km</output></p>"#));
    assert!(!pinned.contains("Zmadho hartën") && !pinned.contains("map-radius"));
    assert!(pinned.contains(r#"<input type="hidden" name="radius" value="5" data-map-target="radius">"#));
    assert!(!pinned.contains(r#"id="map-radius""#) && !pinned.contains("map-radius-steps"));
    // Any whole distance from a link, the zoom showing it as it is.
    let seven = html(pool.clone(), "/c/makina?lat=41.32833&lng=19.81806&radius=7").await;
    assert!(seven.contains(r#"<output data-map-target="radiusLabel">7 km</output>"#));
    for (name, value) in [("lat", "41.32833"), ("lng", "19.81806"), ("radius", "5")] {
        assert!(pinned.contains(&format!(r#"<input type="hidden" name="{name}" value="{value}">"#)), "{name}");
    }
    // A new pin or radius keeps the order picked.
    let map_form = &pinned[pinned.find(r#"<form class="map-form""#).unwrap()..];
    let map_form = &map_form[..map_form.find("</form>").unwrap()];
    assert!(map_form.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(pinned.contains(r#"<a class="breadcrumb-up" href="/c/automjete?lat=41.32833&lng=19.81806&radius=5">Automjete</a>"#), "{pinned}");
    // The point's chip searches everywhere again, keeping the rest.
    assert!(pinned.contains(r#"<a class="filter-chip" href="/c/makina?sort=price_asc" aria-label="Hiq filtrin Brenda 5 km nga pika">"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_maps_card_picks_a_town_or_all_of_albania(pool: PgPool) {
    // In the map's card, beside "Vendndodhja ime", the towns' menu: all of Albania first (chosen,
    // there being no place), then the towns by county, each the same page in it, its filters kept.
    // No town pills under the row any more: there, the order and the cards' size alone.
    let cars = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await;
    let menu = &cars[cars.find(r#"<div class="picker map-town" data-controller="picker">"#).expect("the towns' menu")..];
    let menu = &menu[..menu.find("</select>").unwrap()];
    let order = positions(menu, &[
        r#"<option value="" data-url="/c/makina?vehicle_fuel=DIESEL" selected>Gjithë Shqipëria</option>"#,
        r#"<optgroup label="#,
        r#"data-url="/c/makina?city=tirane&vehicle_fuel=DIESEL">Tiranë</option>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(menu.contains(r#"data-url="/c/makina?city=durres&vehicle_fuel=DIESEL">Durrës</option>"#));
    assert!(!cars.contains("place-pills") && !cars.contains("place-near"));
    let pills = &cars[cars.find(r#"<div class="results-head">"#).unwrap()..cars.find(r#"<div class="grid">"#).unwrap()];
    // No pill under the row: the heading says the order, and the row's last button the view.
    assert_eq!(pills.matches(r#"class="small-pill"#).count(), 0);
    // The page's town is kept in a cookie for the next pages (place_controller.js), on the page's
    // own element.
    assert!(cars.contains(r#"data-controller="filters cards place""#) && cars.contains(r#"data-place-town-value="""#));

    // A town: chosen in the menu, which says it; kept in the cookie; its chip forgets it. The
    // map's button says it, and the map starts there.
    let durres = html(pool.clone(), "/c/makina?city=durres").await;
    assert!(durres.contains(r#"data-place-town-value="durres""#));
    assert!(durres.contains(r#"data-url="/c/makina?city=durres" selected>Durrës</option>"#));
    assert!(durres.contains(r#"<option value="" data-url="/c/makina">Gjithë Shqipëria</option>"#));
    assert!(durres.contains(r#"<a class="filter-chip" href="/c/makina" aria-label="Hiq filtrin Durrës" data-forget-town data-turbo-prefetch="false">"#));
    assert!(durres.contains(r#"aria-label="Harta: Durrës" title="Harta: Durrës""#));
    assert!(durres.contains(r#"data-map-latitude-value="41.313333" data-map-longitude-value="19.445833""#));

    // The map's point, in place of a town: the menu says it, after all of Albania, which drops it,
    // as its chip does.
    let pinned = html(pool, "/c/makina?lat=41.32833&lng=19.81806&radius=5").await;
    let order = positions(&pinned, &[
        r#"<option value="" data-url="/c/makina">Gjithë Shqipëria</option>"#,
        r#"<option value="pika" selected>Tiranë · 5 km</option>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(pinned.contains(r#"<p class="field-hint">Tani nga harta: Tiranë · 5 km.</p>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_town_picked_last_opens_the_next_category_in_it(pool: PgPool) {
    // The cookie the page keeps the town in (qyteti): a category whose address names no place
    // opens in it, as if picked there.
    let durres = get_with_cookie(pool.clone(), "/c/automjete", "qyteti=durres").await.body;
    assert!(durres.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!durres.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(durres.contains(r#"data-url="/c/automjete?city=durres" selected>Durrës</option>"#));
    // A place in the address wins: a town, or the map's point.
    let tirana = get_with_cookie(pool.clone(), "/c/automjete?city=tirane", "qyteti=durres").await.body;
    assert!(tirana.contains("Volkswagen Golf 7, 2016, naftë") && !tirana.contains("Mercedes-Benz C 220"));
    let point = get_with_cookie(pool.clone(), "/c/automjete?lat=41.32833&lng=19.81806&radius=10", "qyteti=durres").await.body;
    assert!(point.contains("Volkswagen Golf 7, 2016, naftë") && !point.contains("Mercedes-Benz C 220"));
    // The map page has its point; a town that isn't one counts for nothing.
    let map = get_with_cookie(pool.clone(), "/harta", "qyteti=durres").await.body;
    assert!(map.contains("Volkswagen Golf 7, 2016, naftë"));
    let nowhere = get_with_cookie(pool, "/c/automjete", "qyteti=atlantis").await.body;
    assert!(nowhere.contains("Mercedes-Benz C 220") && nowhere.contains("Volkswagen Golf 7"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_count_goes_on_past_a_thousand_once_the_page_shows(pool: PgPool) {
    // With numri=1, only the count, as far as 10 000, for the heading's line and the button at
    // the foot of the filters ("Shfaq 2 njoftime"), which the page asks for when its own count
    // stopped (filters_controller.js); never kept by the browser.
    let few = get(pool.clone(), "/c/makina?numri=1").await;
    assert_eq!(few.content_type.as_deref(), Some("application/json"));
    assert_eq!(few.body, r#"{"count":2,"show":"Shfaq 2 njoftime","text":"2 njoftime"}"#);
    let page = html(pool.clone(), "/c/makina").await;
    assert!(page.contains(r#"<button type="button" class="button filters-show" data-filters-target="show" data-action="filters#showResults">Shfaq 2 njoftime</button>"#));
    assert!(!page.contains("data-capped"));
    let none = get(pool.clone(), "/c/makina?city=shkoder&numri=1").await;
    assert_eq!(none.body, r#"{"count":0,"show":"Asnjë njoftim","text":"0 njoftime"}"#);

    // 1 000 more in Të tjera: the page counts to 1 000 and says it stopped there; asked, the count
    // goes on.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 'ACTIVE', now()
         FROM generate_series(1, 1000) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let many = html(pool.clone(), "/c/te-tjera").await;
    assert!(many.contains("<p class=\"result-count\" id=\"result-count\" data-show=\"Shfaq 1k+ njoftime\" data-capped>1k+ njoftime</p>"));
    let counted = get(pool, "/c/te-tjera?numri=1").await;
    assert_eq!(counted.body, "{\"count\":1001,\"show\":\"Shfaq 1k njoftime\",\"text\":\"1k njoftime\"}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_filters_buyers_use_most_come_first(pool: PgPool) {
    // First the place and the price, then what the category asks of every listing or shows on its
    // cards, and a few more (Karroceria, Kambio); the rest under "Më shumë filtra", folded.
    let filter_form = |page: &str| {
        let form = &page[page.find(r#"<form id="filter-form""#).unwrap()..];
        form[..form.find("</form>").unwrap()].to_string()
    };
    let cars = filter_form(&html(pool.clone(), "/c/makina").await);
    let more = cars.find(r#"<details class="more-filters">"#).expect("Më shumë filtra");
    let at = |name: &str| cars.find(&format!(r#"name="{name}""#)).unwrap_or_else(|| panic!("{name}"));
    for name in [
        "city", "price_min", "vehicle_make", "vehicle_model", "vehicle_body_type", "vehicle_year_min",
        "vehicle_mileage_max", "vehicle_fuel", "vehicle_transmission",
    ] {
        assert!(at(name) < more, "{name} comes first");
    }
    for name in ["vehicle_engine_min", "vehicle_power_min", "vehicle_color", "vehicle_features", "vehicle_no_accident"] {
        assert!(at(name) > more, "{name} waits under Më shumë filtra");
    }
    // It opens with the page while one of them is on.
    let white = html(pool.clone(), "/c/makina?vehicle_color=WHITE").await;
    assert!(white.contains(r#"<details class="more-filters" open>"#));
    // A category with few has them all at once: a part's make, model and year.
    assert!(!html(pool.clone(), "/c/pjese-dhe-aksesore").await.contains("more-filters"));
    // The kind of listing comes first for property (Shitje, Qira), before the price.
    let flats = filter_form(&html(pool, "/c/apartamente").await);
    assert!(flats.find(r#"name="type""#).unwrap() < flats.find(r#"name="price_min""#).unwrap());
    let more = flats.find("more-filters").unwrap();
    assert!(flats.find(r#"name="property_rooms""#).unwrap() < more);
    assert!(flats.find(r#"name="property_heating""#).unwrap() > more);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn years_and_kilometres_are_picked_from_steps(pool: PgPool) {
    // A range whose unit has steps offers them in two dropdowns, from and to: the years from next
    // year back to 1970, the kilometres in round numbers. A value from an older link, not a step,
    // stands in its place among them.
    let cars = html(pool.clone(), "/c/makina?vehicle_year_min=2015&vehicle_mileage_max=123456").await;
    assert!(cars.contains(r#"<select name="vehicle_year_min" aria-label="Viti nga">"#));
    assert!(cars.contains(r#"<option value="">nga</option>"#) && cars.contains(r#"<option value="">deri</option>"#));
    assert!(cars.contains(r#"<option value="2015" selected>2015</option>"#));
    assert!(cars.contains(r#"<option value="1970">1970</option>"#) && !cars.contains(r#"<option value="1969">"#));
    assert!(cars.contains(r#"<select name="vehicle_mileage_max" aria-label="Kilometrazhi deri">"#));
    assert!(cars.contains(
        "<option value=\"100000\">100\u{a0}000</option><option value=\"123456\" selected>123\u{a0}456</option><option value=\"125000\">125\u{a0}000</option>"
    ));
    // A range without steps is typed.
    let phones = html(pool, "/c/telefona").await;
    assert!(phones.contains(r#"<input type="text" inputmode="numeric" name="phone_battery_min" value="" placeholder="nga""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn nothing_found_offers_what_would_find_more(pool: PgPool) {
    // Around a point in Shkodër there are no cars: a wider circle, a tap away, then a wider one,
    // and all of Albania; the filters' chips each drop one.
    let shkoder = "/c/makina?vehicle_fuel=DIESEL&lat=42.06813&lng=19.51214&radius=10";
    let page = html(pool.clone(), shkoder).await;
    assert!(page.contains("Nuk u gjet asnjë njoftim."));
    assert!(page.contains(r#"<p class="empty">Provo të heqësh ndonjë filtër nga lart.</p>"#));
    assert!(page.contains(r#"<a class="button secondary" href="/c/makina?lat=42.06813&lng=19.51214&radius=25&vehicle_fuel=DIESEL">Zgjero largësinë në 25 km</a>"#));
    assert!(page.contains(r#"<a class="button secondary" href="/c/makina?vehicle_fuel=DIESEL" data-forget-town data-turbo-prefetch="false"><svg"#));
    assert!(page.contains("</svg>Kërko në gjithë Shqipërinë</a>"));
    let wider = html(pool.clone(), "/c/makina?lat=42.06813&lng=19.51214&radius=25").await;
    assert!(wider.contains("Zgjero largësinë në 50 km</a>") && !wider.contains("Provo të heqësh"));
    let widest = html(pool.clone(), "/c/makina?lat=42.06813&lng=19.51214&radius=50").await;
    assert!(!widest.contains("Zgjero largësinë") && widest.contains("Kërko në gjithë Shqipërinë</a>"));
    // In a town with none: all of Albania. Everywhere already: only the words.
    let town = html(pool.clone(), "/c/makina?city=shkoder").await;
    assert!(!town.contains("Zgjero largësinë") && town.contains("Kërko në gjithë Shqipërinë</a>"));
    let everywhere = html(pool, "/c/makina?vehicle_fuel=ELECTRIC").await;
    assert!(everywhere.contains("Provo të heqësh") && !everywhere.contains("Kërko në gjithë Shqipërinë"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_map_has_a_dot_for_every_result_up_to_its_limit(pool: PgPool) {
    // 501 more in Të tjera, north of Tirana's centre at nine distances, 1.1 km apart.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, latitude, longitude, status,
                               published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                100, 'ALL', 41.328333 + g % 9 * 0.01, 19.818056, 'ACTIVE', now()
         FROM generate_series(1, 501) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let dots = |html: &str| html.matches("<li data-lat=").count();

    // Within 5 km, 279 of them: all on the map, while the list shows them 24 at a time.
    let page = html(pool.clone(), "/harta?category=te-tjera&lat=41.32833&lng=19.81806&radius=5").await;
    assert_eq!(dots(&page), 279);
    assert_eq!(card_numbers(&page).len(), 24);
    assert!(!page.contains("map-note"));
    // The list's next pages bring cards only.
    let (id, uri) = next_frame(&page).unwrap();
    let request = Request::get(uri).header("turbo-frame", id).body(Body::empty()).unwrap();
    let next = send(pool.clone(), request).await.body;
    assert_eq!(card_numbers(&next).len(), 24);
    assert_eq!(dots(&next), 0);

    // Within 10 km, all 501: the map shows the nearest 500, and says so.
    let page = html(pool.clone(), "/harta?category=te-tjera&lat=41.32833&lng=19.81806&radius=10").await;
    assert_eq!(dots(&page), 500);
    assert!(page.contains(r#"<p class="map-note">Në hartë shfaqen 500 njoftimet e para të listës.</p>"#));
    // A category's card says nothing of it: its map is all map (browse.html).
    let card = html(pool, "/c/te-tjera?lat=41.32833&lng=19.81806&radius=10&harta=1").await;
    assert_eq!(dots(&card), 500);
    assert!(!card.contains("map-note"));
}

/// A search's listings, count and categories as one plan reads them: as Postgres plans them, or as
/// after a query ran past its first try (db::ListingFilter::matches), for few matches (through
/// the words' index) or many (in order).
async fn search_read(pool: &PgPool, words: &str, categories: Option<Vec<sqlx::types::Uuid>>, sort: marketplace::db::Sort, few: Option<bool>) -> (Vec<i64>, i64, Vec<(sqlx::types::Uuid, i64)>) {
    let search = marketplace::db::text_search(pool, Some(words)).await.unwrap();
    let filter = marketplace::db::ListingFilter { search, category_ids: categories, sort, ..Default::default() };
    if let Some(few) = few {
        filter.matches.set(few);
    }
    let cards = marketplace::db::listing_page(pool, &filter).await.unwrap();
    let count = marketplace::db::listing_count(pool, &filter).await.unwrap();
    let found_in = marketplace::db::listing_categories(pool, &filter, 8).await.unwrap();
    (cards.iter().map(|card| card.public_no).collect(), count, found_in)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_reads_the_same_listings_through_its_words_index(pool: PgPool) {
    use marketplace::db::Sort;
    let vehicles: Vec<sqlx::types::Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE sub AS (SELECT id FROM categories WHERE slug = 'automjete'
                                UNION ALL SELECT c.id FROM categories c JOIN sub ON c.parent_id = sub.id)
         SELECT id FROM sub",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    // Words that meet often and seldom, alone and among the vehicles, a kind's word, in each order.
    for (words, categories) in [
        ("golf", None),
        ("tirane", None),
        ("golf", Some(vehicles.clone())),
        ("apartament", Some(vehicles.clone())),
        ("makina apartament", None),
        ("shitje", None),
    ] {
        for sort in [Sort::Relevance, Sort::Newest, Sort::PriceLow] {
            let planned = search_read(&pool, words, categories.clone(), sort, None).await;
            let indexed = search_read(&pool, words, categories.clone(), sort, Some(true)).await;
            let walked = search_read(&pool, words, categories.clone(), sort, Some(false)).await;
            assert_eq!(planned, indexed, "{words} {sort:?}");
            assert_eq!(planned, walked, "{words} {sort:?}");
        }
    }
    let (golf, ..) = search_read(&pool, "golf", Some(vehicles.clone()), Sort::Newest, Some(true)).await;
    assert!(golf.contains(&1001), "{golf:?}");
    let (none, count, found_in) = search_read(&pool, "apartament", Some(vehicles), Sort::Relevance, Some(true)).await;
    assert_eq!((none.len(), count, found_in.len()), (0, 0, 0));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_ignores_accents(pool: PgPool) {
    // "vlore" names the town, which the page then filters by, and "shtepi" is searched there.
    let response = followed(pool, "/search?q=shtepi+vlore").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Shtëpi private në shitje në Vlorë"));
    assert!(!response.body.contains("Apartament 2+1"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_matches_property_values(pool: PgPool) {
    // Neither the title nor the description says "telefon"; its type property does. (Searched
    // across every category, "telefon" names Telefona, which the page would filter by instead.)
    let body = get(pool, "/c/elektronike?q=telefon").await.body;
    assert!(body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(!body.contains("Laptop Lenovo ThinkPad T14"));
}

/// The listings a search finds, by number, on the page it goes to if its words name filters.
async fn found(pool: &PgPool, words: &str) -> Vec<i64> {
    card_numbers(&followed(pool.clone(), &format!("/search?q={words}")).await.body)
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_that_name_a_kind_of_listing_find_that_kind(pool: PgPool) {
    // The Golf is for sale, though it doesn't say so: "në shitje", "shitet" and "shes" find it as
    // "golf" does, "me qira" doesn't.
    for words in ["golf+7+ne+shitje", "golf+7+n%C3%AB+shitje", "shitet+golf", "shes+golf+7"] {
        assert_eq!(found(&pool, words).await, [1001], "{words}");
    }
    assert!(found(&pool, "golf+me+qira").await.is_empty());
    // The flat is for rent, and "me qira" finds it; "në shitje" doesn't.
    assert_eq!(found(&pool, "apartament+me+qira").await, [1003]);
    assert!(found(&pool, "apartament+ne+shitje").await.is_empty());
    // A listing that says such a word is found by it, whatever its kind: the Mercedes, for sale,
    // can be rented too.
    sqlx::query("UPDATE listings SET title = 'Mercedes-Benz C 220, 2018, edhe me qira' WHERE public_no = 1002")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(found(&pool, "mercedes+me+qira").await, [1002]);
    // The kind alone finds every listing of it, and a kind left out stays out.
    assert!(found(&pool, "me+qira").await.contains(&1003));
    assert!(found(&pool, "golf+-shitje").await.contains(&1001));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn an_english_search_names_its_filters_in_english_words(pool: PgPool) {
    // On an English page, English's words name a category (by its English name, singular or
    // plural), a kind and a town, as Albanian's do.
    let found_in_english = |words: &'static str| {
        let pool = pool.clone();
        async move {
            let response = get_with_cookie(pool.clone(), &format!("/search?q={words}"), "gjuha=en").await;
            let page = match (response.status, &response.location) {
                (StatusCode::SEE_OTHER, Some(to)) => get_with_cookie(pool, to, "gjuha=en").await,
                _ => response,
            };
            card_numbers(&page.body)
        }
    };
    assert_eq!(found_in_english("flat+for+rent+in+tirana").await, [1003]);
    assert_eq!(found_in_english("apartments+to+rent").await, [1003]);
    assert!(found_in_english("flat+for+sale").await.is_empty());
    assert_eq!(found_in_english("house+for+sale+in+vlora").await, [1004]);
    assert_eq!(found_in_english("phones").await, [1005]);
    // The page it goes to has those filters on.
    let to = get_with_cookie(pool.clone(), "/search?q=flat+for+rent+in+tirana", "gjuha=en").await.location.unwrap();
    assert!(to.contains("category=apartamente") && to.contains("type=RENT") && to.contains("city=tirane"), "{to}");
    // An Albanian page reads Albanian's words alone: there "flat for rent" are words to search.
    assert!(found(&pool, "flat+for+rent").await.is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_forgives_typos_and_names_the_word_it_searched(pool: PgPool) {
    // No listing says "golff": the Golf is found, and the page says "golf" was searched too.
    let misspelt = get(pool.clone(), "/search?q=golff").await.body;
    assert_eq!(card_numbers(&misspelt), vec![1001]);
    assert!(misspelt.contains(r#"<p class="search-corrections">Shfaqen edhe rezultatet për „golf“.</p>"#));
    // A word listings use is searched as typed.
    let typed = get(pool, "/search?q=golf").await.body;
    assert!(typed.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!typed.contains("search-corrections"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_finds_other_forms_of_a_word(pool: PgPool) {
    // The flat's title says "Apartament" and the search the plural, "apartamente": the same word,
    // which needs no note. (In a category's page, as across every category it names Apartamente.)
    let body = get(pool, "/c/prona?q=apartamente").await.body;
    assert!(body.contains("Apartament 2+1 me qira në Tiranë"));
    assert!(!body.contains("search-corrections"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_word_left_out_stays_out_of_a_widened_search(pool: PgPool) {
    // The Golf runs on diesel (naftë): leaving that out leaves the Golf out, typo or not.
    let body = get(pool, "/search?q=golff+-naft%C3%AB").await.body;
    assert!(card_numbers(&body).is_empty());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_finds_listings_by_their_category_and_town(pool: PgPool) {
    // The laptop says neither "elektronikë" nor "Shkodër": the words name its category and its
    // town, which the page filters by, the town with its article too ("Shkodra").
    assert_eq!(card_numbers(&followed(pool.clone(), "/search?q=elektronike+shkoder").await.body), vec![1006]);
    assert_eq!(card_numbers(&followed(pool, "/search?q=laptop+shkodra").await.body), vec![1006]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_that_name_a_category_a_kind_and_a_town_put_those_filters_on(pool: PgPool) {
    // However typed, with its accents or without, the words in any of their forms and any order:
    // the flats for rent in Tirana, each filter a chip to take off if it was misread.
    let flats = "/search?category=apartamente&city=tirane&type=RENT";
    for words in ["Apartament+me+qera+ne+tirane", "apartamente+me+qira+n%C3%AB+Tiran%C3%AB", "qira+apartamenti+tirana"] {
        let sent = get(pool.clone(), &format!("/search?q={words}")).await;
        assert_eq!((sent.status, sent.location.as_deref()), (StatusCode::SEE_OTHER, Some(flats)), "{words}");
    }
    let page = html(pool.clone(), flats).await;
    assert_eq!(card_numbers(&page), [1003]);
    for chip in ["Apartamente", "Tiranë", "Me qira"] {
        assert!(page.contains(&format!(r#"aria-label="Hiq filtrin {chip}""#)), "{chip}");
    }

    let goes = |uri: &'static str| {
        let pool = pool.clone();
        async move { get(pool, uri).await.location }
    };
    // A town alone; and the words that name nothing stay searched there, the other filters kept.
    assert_eq!(goes("/search?q=tirane").await.as_deref(), Some("/search?city=tirane"));
    assert_eq!(
        goes("/search?q=golf+7+ne+durres&sort=price_asc").await.as_deref(),
        Some("/search?q=golf+7&city=durres&sort=price_asc")
    );
    // A short town only after the word that leads to it ("bojë mat" is a colour).
    assert_eq!(goes("/search?q=makina+ne+mat").await.as_deref(), Some("/search?category=makina&city=mat"));
    // A category's own page reads the town and the kind, the category being its own.
    assert_eq!(goes("/c/makina?q=me+qira+ne+tirane").await.as_deref(), Some("/c/makina?city=tirane&type=RENT"));
    // The thing sought comes first: headphones for a phone are headphones. A category's name is
    // about its first word: a car is in Makina, not in Tapete makine (mats); and the verb an ad
    // starts with comes before it.
    assert_eq!(goes("/search?q=kufje+per+telefon").await.as_deref(), Some("/search?q=per+telefon&category=audio-dhe-kufje"));
    assert_eq!(goes("/search?q=makine+me+nafte").await.as_deref(), Some("/search?q=me+nafte&category=makina"));
    assert_eq!(goes("/search?q=kerkoj+apartament+me+qira").await.as_deref(), Some("/search?category=apartamente&type=RENT"));
    // "shtëpi" is a house and the home alike, until the kind says which.
    assert_eq!(goes("/search?q=shtepi+me+qira").await.as_deref(), Some("/search?category=vila-dhe-shtepi&type=RENT"));
    // Words that name nothing, a kind with no category to take it, a category two names share, or
    // two towns, are searched as they are.
    for uri in ["/search?q=golf", "/search?q=golf+ne+shitje", "/search?q=karikues+telefoni", "/search?q=shtepi",
        "/search?q=tirane+durres", "/search?q=boj%C3%AB+mat"]
    {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::OK, "{uri}");
    }
}

/// Moves a listing into one of a town's zones.
async fn in_zone(pool: &PgPool, number: i64, town: &str, zone: &str) {
    sqlx::query(
        "UPDATE listings SET location_id = (SELECT z.id FROM locations z JOIN locations t ON t.id = z.parent_id
                                            WHERE t.slug = $1 AND z.slug = $2 AND z.location_type = 'AREA')
         WHERE public_no = $3",
    )
    .bind(town)
    .bind(zone)
    .bind(number)
    .execute(pool)
    .await
    .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn property_narrows_to_a_zone_of_its_town(pool: PgPool) {
    in_zone(&pool, 1003, "tirane", "komuna-e-parisit").await;
    // In its zone, the flat; in another of Tirana's, nothing, and a way to the whole town.
    let zone = html(pool.clone(), "/c/apartamente?city=tirane&zone=komuna-e-parisit").await;
    assert_eq!(card_numbers(&zone), [1003]);
    let other = html(pool.clone(), "/c/apartamente?city=tirane&zone=blloku").await;
    assert!(card_numbers(&other).is_empty());
    assert!(other.contains(r#"<a class="button secondary" href="/c/apartamente?city=tirane">Kërko në gjithë qytetin</a>"#));
    // Zona has every town's zones under it, those of the town picked showing, the zone selected.
    assert!(zone.contains(r#"<optgroup label="Tiranë" data-parent="tirane">"#));
    assert!(zone.contains(r#"<option value="komuna-e-parisit" selected>Komuna e Parisit</option>"#));
    assert!(zone.contains(r#"<optgroup label="Durrës" data-parent="durres" hidden>"#));
    // The zone's chip leaves the town; the town's takes the zone with it.
    assert!(zone.contains(r#"<a class="filter-chip" href="/c/apartamente?city=tirane" aria-label="Hiq filtrin Komuna e Parisit">"#));
    assert!(zone.contains(r#"<a class="filter-chip" href="/c/apartamente" aria-label="Hiq filtrin Tiranë" data-forget-town"#));
    // A town without zones keeps Zona hidden, and a category whose listings have none has none.
    let shkoder = html(pool.clone(), "/c/apartamente?city=shkoder").await;
    assert!(shkoder.contains(r#"<div class="filter-group filter-zone" data-filters-target="zone" hidden>"#));
    assert!(!html(pool.clone(), "/c/makina?city=tirane").await.contains("filter-zone"));
    // A zone goes with its town: without it, in another town or outside property, it is nothing.
    let lone = html(pool.clone(), "/c/apartamente?zone=komuna-e-parisit").await;
    assert!(card_numbers(&lone).contains(&1003) && !lone.contains("Hiq filtrin Komuna e Parisit"));
    assert!(!html(pool.clone(), "/c/apartamente?city=durres&zone=komuna-e-parisit").await.contains("Hiq filtrin Komuna"));
    let cars = html(pool, "/c/makina?city=tirane&zone=blloku").await;
    assert_eq!(card_numbers(&cars), [1001]);
    assert!(!cars.contains("Hiq filtrin Blloku"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_zone_named_in_a_property_search_puts_it_on(pool: PgPool) {
    in_zone(&pool, 1003, "durres", "plazh").await;
    let goes = |uri: &'static str| {
        let pool = pool.clone();
        async move { get(pool, uri).await.location }
    };
    // The flats in Plazh, which is in Durrës, "2+1" searched there.
    let plazh = "/search?q=2%2B1&category=apartamente&city=durres&zone=plazh";
    assert_eq!(goes("/search?q=apartament+2%2B1+ne+plazh").await.as_deref(), Some(plazh));
    let page = html(pool.clone(), plazh).await;
    assert_eq!(card_numbers(&page), [1003]);
    for chip in ["Apartamente", "Durrës", "Plazh"] {
        assert!(page.contains(&format!(r#"aria-label="Hiq filtrin {chip}""#)), "{chip}");
    }
    // Without the category, which its chip and "Pastro filtrat" take off, a zone is nothing: it goes.
    assert!(page.contains(r#"<a class="filter-chip" href="/search?q=2%2B1&city=durres" aria-label="Hiq filtrin Apartamente">"#));
    assert!(page.contains(r#"<a class="clear-filters" href="/search?q=2%2B1&city=durres">"#));
    // In its forms and any order; a zone of a few words, on a category's page.
    assert_eq!(goes("/search?q=plazhi+apartament").await.as_deref(), Some("/search?category=apartamente&city=durres&zone=plazh"));
    assert_eq!(goes("/c/prona?q=komuna+e+parisit").await.as_deref(), Some("/c/prona?city=tirane&zone=komuna-e-parisit"));
    // A zone named after a town is not that town: Rruga e Durrësit is in Tirana.
    assert_eq!(
        goes("/c/apartamente?q=rruga+e+durresit").await.as_deref(),
        Some("/c/apartamente?city=tirane&zone=rruga-e-durresit")
    );
    // Qendër is Tirana's, Durrës's and Vlora's: the one in the town named, or else in the page's.
    assert_eq!(goes("/c/apartamente?q=ne+qender+te+tiranes").await.as_deref(), Some("/c/apartamente?city=tirane&zone=qender"));
    assert_eq!(goes("/c/apartamente?city=vlore&q=qendra").await.as_deref(), Some("/c/apartamente?city=vlore&zone=qender"));
    // A zone in another town than the one named stays a word, the town kept.
    assert_eq!(goes("/c/apartamente?q=plazh+ne+vlore").await.as_deref(), Some("/c/apartamente?q=plazh&city=vlore"));
    // Outside property, or with no town to say which Qendër, the name is a word: a beach towel.
    for uri in ["/search?q=peshqir+plazhi", "/c/makina?q=golf+blloku", "/c/apartamente?q=qender"] {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::OK, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filler_words_needn_t_be_in_a_listing(pool: PgPool) {
    // The Mercedes is in Durrës, but nothing it says has "në" (which goes with the town, the
    // page's filter); nor does the Golf, for sale, have "në" or "shitje".
    assert_eq!(card_numbers(&followed(pool.clone(), "/search?q=mercedes+n%C3%AB+durr%C3%ABs").await.body), vec![1002]);
    assert_eq!(card_numbers(&get(pool, "/search?q=golf+n%C3%AB+shitje").await.body), vec![1001]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_search_across_categories_names_the_categories_it_found(pool: PgPool) {
    // Each chip narrows the search to its category, where that category's filters are.
    // After "Gjetur te", as the canvas has them, each with its category's icon.
    let found = html(pool.clone(), "/search?q=golf").await;
    assert!(found.contains(r#"<span class="categories-lead" aria-hidden="true">Gjetur te</span>"#));
    assert!(found.contains(r#"<a class="chip" href="/c/makina?q=golf"><svg class="chip-icon""#));
    assert!(found.contains(r#"</svg>Makina <span class="chip-count">1</span></a>"#));
    // A kind of listing, with no category to take it, stays words: the listings for sale are in
    // nine categories, of which the six with the most, the two cars' first.
    let selling = html(pool.clone(), "/search?q=shitje").await;
    assert!(selling.contains(r#"<a class="chip" href="/c/makina?q=shitje"><svg class="chip-icon""#), "{selling}");
    assert!(selling.contains(r#"</svg>Makina <span class="chip-count">2</span></a>"#), "{selling}");
    assert_eq!(selling.matches(r#"<span class="chip-count">"#).count(), 6);
    // A category's page has its subcategories instead.
    assert!(!html(pool, "/c/makina?q=golf").await.contains("chip-count"));
}

/// The names on the row's filter dropdowns, in their order (filter_drop.html).
fn dropdown_names(page: &str) -> Vec<&str> {
    let pill = r#"<summary class="button secondary round-button labelled filter-pill">"#;
    page.match_indices(pill)
        .map(|(at, _)| {
            let name = &page[at + pill.len()..];
            &name[..name.find("<svg").unwrap()]
        })
        .collect()
}

/// One of the row's filter dropdowns, from its pill to its end.
fn dropdown<'a>(page: &'a str, name: &str) -> &'a str {
    let start = page.find(&format!(r#"labelled filter-pill">{name}<svg"#)).unwrap_or_else(|| panic!("no {name}"));
    &page[start..start + page[start..].find("</details>").unwrap()]
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_row_offers_the_filters_buyers_use_most_as_dropdowns(pool: PgPool) {
    // After Filtrat, as the canvas has them: a car's make, model and year, the price fourth, and
    // its fuel; its body types (with icons) and kilometres (a range in a unit) are the sheet's.
    let cars = html(pool.clone(), "/c/makina").await;
    assert_eq!(dropdown_names(&cars), ["Marka", "Modeli", "Viti", "Çmimi", "Karburanti"]);
    // A main category has the condition, the price and the kind of listing; a search across every
    // category starts with the town.
    assert_eq!(dropdown_names(&html(pool.clone(), "/c/automjete").await), ["Gjendja", "Çmimi", "Lloji i njoftimit"]);
    assert_eq!(dropdown_names(&html(pool.clone(), "/search?q=golf").await), ["Qyteti", "Çmimi", "Gjendja"]);
    // A filter that is on is a chip in place of its dropdown, and the row has five at most, chips
    // and dropdowns together: with the fuel on, four dropdowns; with a price, none for the price;
    // with five filters on, none.
    let diesel = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&sort=price_asc").await;
    assert_eq!(dropdown_names(&diesel), ["Marka", "Modeli", "Çmimi", "Viti"]);
    assert_eq!(dropdown_names(&html(pool.clone(), "/c/makina?price_max=9000").await), ["Marka", "Modeli", "Viti", "Karburanti"]);
    let five = "/c/makina?city=tirane&vehicle_fuel=DIESEL&vehicle_transmission=AUTOMATIC&condition=USED&price_max=9000";
    assert!(dropdown_names(&html(pool.clone(), five).await).is_empty());
    // With room for two, the price comes after the lone other.
    let three = "/c/makina?vehicle_make=AUDI&vehicle_fuel=PETROL&vehicle_fuel=HYBRID";
    assert_eq!(dropdown_names(&html(pool.clone(), three).await), ["Modeli", "Çmimi"]);

    // Each opens a card under it, a form of its own that carries the rest of the page along, its
    // order too: a make picked from its list (the picker, inline, "Të gjitha" there for a page
    // without scripts) applies at once, with no button.
    let make = dropdown(&diesel, "Marka");
    assert!(make.contains(r#"<form class="drop-card filter-form" action="/c/makina" method="get""#));
    assert!(make.contains(r#"<input type="hidden" name="vehicle_fuel" value="DIESEL">"#));
    assert!(make.contains(r#"<input type="hidden" name="sort" value="price_asc">"#));
    assert!(make.contains(r#"<div class="picker drop-list" data-picker-inline-value="true" data-controller="picker">"#));
    assert!(make.contains(r#"<select name="vehicle_make" aria-label="Marka" data-action="change->filter-drop#picked">"#));
    assert!(!make.contains("drop-apply") && !make.contains(r#"type="hidden" name="vehicle_make""#));
    // The models come under their makes.
    assert!(dropdown(&diesel, "Modeli").contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    // The price's card is the sheet's price, with a button at its foot; it carries the fuel, and
    // no price but its own.
    let price = dropdown(&diesel, "Çmimi");
    assert!(price.contains(r#"<fieldset class="filter-group price-filter" data-controller="price-range">"#));
    assert!(price.contains(r#"<button type="submit" class="button small drop-apply">Shfaq rezultatet</button>"#));
    assert!(price.contains(r#"name="vehicle_fuel" value="DIESEL""#) && !price.contains(r#"type="hidden" name="price_max""#));
    // The year's two lists, from and to, and its button.
    let year = dropdown(&diesel, "Viti");
    assert_eq!(year.matches("<select").count(), 2);
    assert!(year.contains("drop-apply"));
    // Opening one folds the row's others, and each applies through its own controller.
    assert!(diesel.contains(r#"<details class="filter-drop" data-controller="dropdown filter-drop""#));
    assert!(diesel.contains("toggle->filter-drop#toggled section:open@document->dropdown#foldFor"));
    // Without JavaScript they are gone, the sheet's form in the page.
    assert!(cars.contains(".filters-open, .filter-drop, .map-card,"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn suggestions_finish_the_words_and_say_where_they_lead(pool: PgPool) {
    // The word finished as the listings have it, then the category it is found in, then the make
    // and model it names.
    let rows = html(pool.clone(), "/sugjerime?q=gol").await;
    let at = positions(&rows, &[
        r#"href="/search?q=golf""#,
        r#"href="/c/makina?q=golf""#,
        r#"href="/c/makina?vehicle_make=VOLKSWAGEN&vehicle_model=VOLKSWAGEN_GOLF""#,
    ]);
    assert!(at.windows(2).all(|pair| pair[0] < pair[1]), "{rows}");
    assert!(rows.contains(r#"<span class="suggestion-label">Volkswagen <strong>Gol</strong>f</span>"#), "{rows}");
    // A town the last word starts: on its own, all its listings; after other words, those there.
    let town = html(pool.clone(), "/sugjerime?q=durr").await;
    assert!(town.contains(r#"href="/search?city=durres""#) && town.contains("Njoftimet në <strong>Durr</strong>ës"), "{town}");
    let there = html(pool.clone(), "/sugjerime?q=golf%20durr").await;
    assert!(there.contains(r#"href="/search?q=golf&city=durres""#) && there.contains("në Durrës"), "{there}");
    // The word that joins the words to the town, "në" with its ë or without, is the town's to say:
    // the row searches the words alone, there; with no other words, it is all the town's listings.
    for typed in ["golf%20ne%20durr", "golf%20n%C3%AB%20durr", "golf%20te%20durr"] {
        let there = html(pool.clone(), &format!("/sugjerime?q={typed}")).await;
        assert!(there.contains(r#"href="/search?q=golf&city=durres""#), "{typed}: {there}");
        assert!(there.contains(r#"<span class="suggestion-label"><strong>golf</strong></span>"#), "{typed}: {there}");
    }
    let town_alone = html(pool.clone(), "/sugjerime?q=ne%20durr").await;
    assert!(town_alone.contains(r#"href="/search?city=durres""#) && town_alone.contains("Njoftimet <strong>në</strong> <strong>Durr</strong>ës"), "{town_alone}");
    // Words, then "në" (or "ne", or with a space after): the towns they are found in, a car in
    // Tirana and one in Durrës, rather than words starting "ne".
    for typed in ["makina%20ne", "makina%20n%C3%AB%20"] {
        let places = html(pool.clone(), &format!("/sugjerime?q={typed}")).await;
        for town in ["tirane", "durres"] {
            assert!(places.contains(&format!(r#"href="/search?q=makina&city={town}""#)), "{typed}: {places}");
        }
        assert!(!places.contains("q=makina+n"), "{typed}: {places}");
    }
    // A whole word misspelt: the word it most likely misspells.
    assert!(html(pool.clone(), "/sugjerime?q=mercedez").await.contains(r#"href="/search?q=mercedes""#));
    // A brand of the very name the words have: its row, which finds it alone, not the words
    // searched in its category too.
    let brand = html(pool.clone(), "/sugjerime?q=apple").await;
    assert!(brand.contains(r#"href="/c/telefona?electronics_brand=APPLE""#), "{brand}");
    assert!(!brand.contains(r#"href="/c/telefona?q=apple""#), "{brand}");
    // Too little to go on.
    assert!(!get(pool, "/sugjerime?q=g").await.body.contains("suggestion"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn city_filter_keeps_to_that_city(pool: PgPool) {
    let body = get(pool, "/search?city=durres").await.body;
    assert!(body.contains(r#"<h1 class="browse-title">Njoftimet në Durrës</h1>"#));
    assert!(body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(body.contains("Kërkohet kamarier/e me eksperiencë"));
    assert!(!body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(body.contains(r#"<p class="result-count" id="result-count" data-show="Shfaq 2 njoftime">2 njoftime</p>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn welcome_form_narrows_to_a_category_and_city(pool: PgPool) {
    let body = html(pool, "/search?category=automjete&city=tirane").await;
    assert!(body.contains(r#"<h1 class="browse-title">Automjete</h1>"#));
    assert!(body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!body.contains("Mercedes-Benz C 220, 2018, automatike"), "the Mercedes is in Durrës");
    assert!(!body.contains("iPhone 13"), "the iPhone is in another category");
    // Both are filters here, so both can be removed.
    assert!(body.contains(r#"<a class="filter-chip" href="/search?city=tirane""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/search?category=automjete""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn price_filter_converts_currencies(pool: PgPool) {
    // 55 000 Lekë is 550 € and 62 000 Lekë is 620 €, at the placeholder rate of 100 Lekë per euro.
    let body = get(pool.clone(), "/c/elektronike?price_max=600&currency=EUR").await.body;
    assert!(body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(!body.contains("Laptop Lenovo ThinkPad T14"));
    let body = get(pool, "/c/elektronike?price_min=60.000&currency=ALL").await.body;
    assert!(!body.contains("iPhone 13, 128 GB, si i ri"));
    assert!(body.contains("Laptop Lenovo ThinkPad T14"));
    assert!(body.contains(">nga 60\u{a0}000\u{a0}Lekë<"), "the chip shows the bound");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_bank_of_albanias_euro_rate_reprices_the_listings_in_leke(pool: PgPool) {
    use marketplace::db;
    use rust_decimal::Decimal;
    let number = |text: &str| text.parse::<Decimal>().unwrap();
    let day = time::Date::from_calendar_date(2026, time::Month::October, 9).unwrap();
    let iphone = "iPhone 13, 128 GB, si i ri";
    let euros = |pool: PgPool, no: i64| async move {
        sqlx::query_scalar::<_, Decimal>("SELECT price_normalized FROM listings WHERE public_no = $1")
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    // The placeholder: 100 lekë a euro, every price worked out at it.
    let seeded = db::euro_rate(&pool).await.unwrap();
    assert_eq!((seeded.lek_per_eur, seeded.rate_date, seeded.reprice_due), (number("100.00"), None, false));
    assert_eq!(marketplace::rates::reprice(&pool).await.unwrap(), 0);
    // Every listing last changed a day ago (which the trigger lets a statement say only so).
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.keep_updated_at', 'on', true)").execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE listings SET updated_at = now() - interval '1 day'").execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();

    // The official 91.65 is taken; the listings priced in lekë are due to be worked out again.
    assert!(db::take_euro_rate(&pool, number("91.65"), day).await.unwrap());
    let taken = db::euro_rate(&pool).await.unwrap();
    assert_eq!((taken.eur_rate, taken.lek_per_eur, taken.rate_date, taken.reprice_due), (number("0.01091107"), number("91.65"), Some(day), true));

    // Worked out again: every one priced in lekë (the iPhone's 55 000 Lekë is 600,11 € now, not
    // 550), those in euros as they were, and none of them changed as far as its updated_at, which
    // the sitemaps give, says.
    let in_leke: i64 = sqlx::query_scalar("SELECT count(*) FROM listings WHERE currency = 'ALL' AND price_amount IS NOT NULL")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(marketplace::rates::reprice(&pool).await.unwrap(), in_leke as u64);
    assert_eq!(euros(pool.clone(), 1005).await, number("600.11"));
    assert_eq!(euros(pool.clone(), 1001).await, number("9500.00"));
    let untouched: bool = sqlx::query_scalar("SELECT bool_and(updated_at < now() - interval '23 hours') FROM listings").fetch_one(&pool).await.unwrap();
    assert!(untouched);
    assert!(!db::euro_rate(&pool).await.unwrap().reprice_due);
    assert_eq!(marketplace::rates::reprice(&pool).await.unwrap(), 0);
    // The price filter in euros goes by it.
    assert!(!get(pool.clone(), "/c/elektronike?price_max=600&currency=EUR").await.body.contains(iphone));
    assert!(get(pool.clone(), "/c/elektronike?price_max=601&currency=EUR").await.body.contains(iphone));

    // A rate a few hundredths off isn't worth rewriting the prices for; half a percent off is.
    assert!(!db::take_euro_rate(&pool, number("91.40"), day).await.unwrap());
    assert!(db::take_euro_rate(&pool, number("91.10"), day).await.unwrap());
    marketplace::rates::reprice(&pool).await.unwrap();
    assert_eq!(euros(pool.clone(), 1005).await, number("603.73"));
    // A listing edited meanwhile changes as ever.
    sqlx::query("UPDATE listings SET title = title WHERE public_no = 1005").execute(&pool).await.unwrap();
    let edited: bool = sqlx::query_scalar("SELECT updated_at > now() - interval '1 minute' FROM listings WHERE public_no = 1005")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(edited);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_price_is_a_slider_with_two_ends(pool: PgPool) {
    // Without a price, the ends stand at the edges: no limit either way, and the fields to type
    // the prices in are empty.
    let any = html(pool.clone(), "/c/elektronike").await;
    assert!(any.contains(r#"<legend class="filter-label">Çmimi <output data-price-range-target="label">Çdo çmim</output></legend>"#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="0" aria-label="Çmimi nga, në shkallë""#));
    assert!(any.contains(r#"<input type="range" min="0" max="1000" step="1" value="1000" aria-label="Çmimi deri, në shkallë""#));
    assert!(any.contains(r#"<input type="text" inputmode="numeric" name="price_min" value="" placeholder="nga" aria-label="Çmimi nga""#));
    // 1 000 € is two of the scale's five decades from 10 €, so 400 of 1 000; 20 000 € is
    // log10(2 000) = 3.30 of them, so 660. The typed fields carry the prices themselves.
    let some = html(pool, "/c/elektronike?price_min=1000&price_max=20000&currency=EUR").await;
    assert!(some.contains(r#"value="400" aria-label="Çmimi nga, në shkallë""#));
    assert!(some.contains(r#"value="660" aria-label="Çmimi deri, në shkallë""#));
    assert!(some.contains(r#"<input type="text" inputmode="numeric" name="price_min" value="1000" placeholder="nga" aria-label="Çmimi nga" autocomplete="off""#));
    assert!(some.contains(r#"data-price-range-target="min" data-action="change->price-range#typed">"#));
    assert!(some.contains("<output data-price-range-target=\"label\">1\u{a0}000 – 20\u{a0}000\u{a0}€</output>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_price_has_quick_choices_by_the_categorys_prices(pool: PgPool) {
    // Too few prices to go by: no quick choices. (Worked out once for some hours, so not asked of
    // Të tjera before its prices come.)
    assert!(!html(pool.clone(), "/c/makina").await.contains("price-choices"));
    // 24 more in Të tjera, at 1 000 to 24 000 Lekë: a quarter of them up to 6 000, half up to
    // 12 000, three quarters up to 18 000, each kept to round figures (20 000).
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                g * 1000, 'ALL', 'FIXED', g * 10, 'ACTIVE', now()
         FROM generate_series(1, 24) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let page = html(pool.clone(), "/c/te-tjera").await;
    // A choice's button up to its label, its whitespace one space whatever the template's line
    // endings, and as it should be.
    let button = |page: &str, label: &str| {
        let end = page.find(&format!(">{label}</button>")).unwrap_or_else(|| panic!("{label}"));
        let start = page[..end].rfind("<button").unwrap();
        page[start..end].split_whitespace().collect::<Vec<_>>().join(" ")
    };
    let choice = |min: &str, max: &str, pressed: bool| {
        format!(
            r#"<button type="button" class="price-choice" aria-pressed="{pressed}" data-price-range-target="choice" data-action="price-range#pick" data-price-range-min-param="{min}" data-price-range-max-param="{max}" data-price-range-currency-param="ALL""#
        )
    };
    let labels = [
        "deri 6\u{a0}000\u{a0}Lekë",
        "6\u{a0}000 – 12\u{a0}000\u{a0}Lekë",
        "12\u{a0}000 – 20\u{a0}000\u{a0}Lekë",
        "nga 20\u{a0}000\u{a0}Lekë",
    ];
    for (label, (min, max)) in labels.into_iter().zip([("", "6000"), ("6000", "12000"), ("12000", "20000"), ("20000", "")]) {
        assert_eq!(button(&page, label), choice(min, max, false), "{label}");
    }
    let ends = labels.map(|label| format!(">{label}</button>"));
    let order = positions(&page, &ends.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(order.is_sorted(), "{order:?}");
    // The one the prices are shows pressed.
    let picked = html(pool, "/c/te-tjera?price_min=6000&price_max=12000&currency=ALL").await;
    assert_eq!(button(&picked, labels[1]), choice("6000", "12000", true));
    assert_eq!(button(&picked, labels[0]), choice("", "6000", false));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sorts_by_price_with_unpriced_listings_last(pool: PgPool) {
    let body = get(pool.clone(), "/search?sort=price_asc").await.body;
    let order = positions(&body, &[
        "Dhurohen libra shkollorë", // free
        "Xhaketë dimri për meshkuj, masa M",
        "Biçikletë malore 29 inç",
        "Shtëpi private në shitje në Vlorë",
        "Kërkohet kamarier/e me eksperiencë", // salary on request
    ]);
    assert!(order.is_sorted(), "{order:?}");

    let body = get(pool, "/search?sort=price_desc").await.body;
    let order = positions(&body, &[
        "Shtëpi private në shitje në Vlorë",
        "Mercedes-Benz C 220, 2018, automatike",
        "Biçikletë malore 29 inç",
        "Dhurohen libra shkollorë",
        "Kërkohet kamarier/e me eksperiencë",
    ]);
    assert!(order.is_sorted(), "{order:?}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn price_pages_continue_where_the_last_one_ended(pool: PgPool) {
    // 40 more in Të tjera: 30 with prices, many of them equal so the id has to break ties, and
    // 10 with the price on request.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g, '',
                CASE WHEN g <= 30 THEN g % 7 * 1000 END, 'ALL',
                CASE WHEN g <= 30 THEN 'FIXED' ELSE 'ON_REQUEST' END,
                CASE WHEN g <= 30 THEN g % 7 * 10 END,
                'ACTIVE', now() - g * interval '1 minute'
         FROM generate_series(1, 40) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    for (sort, order) in [("price_asc", "ASC"), ("price_desc", "DESC")] {
        let mut seen = Vec::new();
        let mut uri = format!("/c/te-tjera?sort={sort}");
        let mut frame_id = None;
        loop {
            let mut request = Request::get(&uri);
            if let Some(id) = &frame_id {
                request = request.header("turbo-frame", id);
            }
            let page = send(pool.clone(), request.body(Body::empty()).unwrap()).await.body;
            let page = page.replace("&#38;", "&");
            seen.extend(card_numbers(&page));
            let Some((id, src)) = next_frame(&page) else { break };
            (frame_id, uri) = (Some(id), src);
        }
        let expected: Vec<i64> = sqlx::query_scalar(&format!(
            "SELECT public_no FROM listings
             WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
             ORDER BY price_normalized {order} NULLS LAST, id {order}"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(expected.len(), 41);
        assert_eq!(seen, expected, "{sort}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn words_find_the_best_matches_first(pool: PgPool) {
    // 260 in Të tjera that say "kaltërosh", a minute apart, the first the newest: every fifth in its
    // title, every third twice in its description, the others once. The newest 240 are ranked
    // (db::RANK_WINDOW); the 20 past them follow, newest first.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, price_type, price_normalized,
                               status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g,
                CASE WHEN g % 5 = 0 THEN 'Kaltërosh ' || g ELSE 'Njoftim shtesë ' || g END,
                CASE WHEN g % 3 = 0 THEN 'Një kaltërosh, dhe një kaltërosh tjetër.' ELSE 'Një kaltërosh.' END,
                1000, 'ALL', 'FIXED', 10, 'ACTIVE', now() - g * interval '1 minute'
         FROM generate_series(1, 260) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    marketplace::db::refresh_search_words(&pool).await.unwrap();
    assert_eq!(marketplace::db::RANK_WINDOW, 240);
    let in_order = |order: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(&format!(
                "SELECT public_no FROM (
                     SELECT l.public_no, l.sort_at, l.id, ts_rank(l.search_vector, s.query) AS rank,
                            row_number() OVER (ORDER BY l.sort_at DESC, l.id DESC) AS newest
                     FROM listings l, search_query('kaltërosh') s
                     WHERE l.status = 'ACTIVE' AND l.search_vector @@ s.query) m
                 ORDER BY {order}"
            ))
            .fetch_all(&pool)
            .await
            .unwrap()
        }
    };
    let best = in_order("newest > 240, CASE WHEN newest <= 240 THEN rank END DESC, sort_at DESC, id DESC").await;
    let newest = in_order("sort_at DESC, id DESC").await;
    assert_eq!(best.len(), 260);
    // Among the ranked, those with the word in their title come first, those that say it twice
    // besides first of them; past the window, the rest newest first.
    let titled: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE title LIKE 'Kaltërosh %' AND split_part(title, ' ', 2)::int <= 240
         ORDER BY public_no",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(titled.len(), 48);
    let mut first = best[..48].to_vec();
    first.sort();
    assert_eq!(first, titled);
    let twice: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE title LIKE 'Kaltërosh %' AND description LIKE '%tjetër%'
           AND split_part(title, ' ', 2)::int <= 240 ORDER BY sort_at DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(best[..16], twice[..]);
    assert_eq!(best[240..], newest[240..]);
    assert_ne!(best, newest);

    // Words open the page in the best match first, which its order's menu names first, the newest
    // first after it; every page, as scrolling loads them, follows on in that order.
    let search = "/search?q=kalt%C3%ABrosh";
    let page = html(pool.clone(), search).await;
    let choices = positions(&page, &["Më të përshtatshmet", "Më të rejat", "Më të lirat"]);
    assert!(choices.is_sorted(), "{page}");
    assert_eq!(every_page(&pool, search).await, best);
    // Newest first when asked, every page too.
    assert_eq!(every_page(&pool, &format!("{search}&sort=newest")).await, newest);
    // Without words there is nothing to match: no such order on offer, nor taken from the address.
    let plain = html(pool.clone(), "/c/te-tjera?sort=relevance").await;
    assert!(!plain.contains("Më të përshtatshmet"), "{plain}");
    let shown = card_numbers(&plain);
    let newest_here: Vec<i64> = sqlx::query_scalar(
        "SELECT public_no FROM listings WHERE category_id = '10000000-0000-0000-0000-000000000011' AND status = 'ACTIVE'
         ORDER BY sort_at DESC, id DESC LIMIT 24",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(shown, newest_here);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filter_form_offers_the_category_properties(pool: PgPool) {
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(cars.contains(r#"<legend class="filter-label">Karburanti</legend>"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_fuel" value="DIESEL">"#));
    assert!(cars.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#), "models are grouped by make");
    assert!(cars.contains("Gjendja"));
    // A few boxes share their line with the next field (Karburanti, five, beside Motorri); more
    // take one of their own, the colours' seven, as do the body types (Karroceria), whose boxes
    // have the car's side over each name.
    let fuel = cars.find(r#"<legend class="filter-label">Karburanti</legend>"#).unwrap();
    assert!(cars[..fuel].trim_end().ends_with(r#"<fieldset class="filter-group filter-options filter-half">"#));
    let body = cars.find(r#"<legend class="filter-label">Karroceria</legend>"#).unwrap();
    assert!(cars[..body].trim_end().ends_with(r#"<fieldset class="filter-group filter-options filter-icons">"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_body_type" value="WAGON"><svg class="check-icon""#));
    assert_eq!(cars.matches(r#"<svg class="check-icon""#).count(), 7, "one for each body type, none for the others");
    let colour = cars.find(r#"<legend class="filter-label">Ngjyra</legend>"#).unwrap();
    assert!(cars[..colour].trim_end().ends_with(r#"<fieldset class="filter-group filter-options">"#));
    assert!(cars.contains(r#"<input type="checkbox" name="vehicle_color" value="WHITE"> E bardhë</label>"#));
    assert!(!cars.contains(r#"<select id="filter-vehicle_color""#));
    // The options, many boxes, take a line of their own too...
    let features = cars.find(r#"<legend class="filter-label">Opsionet</legend>"#).unwrap();
    assert!(cars[..features].trim_end().ends_with(r#"<fieldset class="filter-group filter-options">"#));
    // ...as does a small field left without another beside it: a motorcycle's make, before the
    // eight boxes of its kinds.
    let motorcycles = get(pool.clone(), "/c/motocikleta").await.body;
    let make = motorcycles.find(r#"<label class="filter-label" for="filter-moto_make">"#).expect("the make");
    assert!(motorcycles[..make].trim_end().ends_with(r#"<div class="filter-group">"#));

    let property = get(pool.clone(), "/c/prona").await.body;
    assert!(property.contains(r#"<h1 class="browse-title">Prona</h1>"#));
    assert!(property.contains(r#"<input type="radio" name="type" value="RENT">"#));
    assert!(!property.contains("Gjendja"), "property has no condition");
    // Rooms are a flat's and a house's, not every property's: a plot has none.
    assert!(!property.contains(r#"name="property_rooms""#));
    assert!(get(pool.clone(), "/c/apartamente").await.body.contains(r#"value="R21""#));

    // Kinds of electronics are subcategories now, not a filter.
    let electronics = get(pool.clone(), "/c/elektronike").await.body;
    assert!(electronics.contains(r#"<a class="shortcut" href="/c/laptope"><svg class="shortcut-icon""#));
    assert!(!electronics.contains(r#"name="electronics_type""#));
    // A subcategory has its parent's properties: brand and storage for phones.
    let phones = get(pool.clone(), "/c/telefona").await.body;
    assert!(phones.contains("iPhone 13, 128 GB, si i ri"));
    assert!(phones.contains(r#"name="electronics_brand""#));

    // A chosen make narrows the models to its own: the others' wait, hidden, for the make to
    // change in the form (narrow_controller.js).
    let bmw = get(pool.clone(), "/c/makina?vehicle_make=BMW").await.body;
    assert!(bmw.contains(r#"<select id="filter-vehicle_model" name="vehicle_model" data-depends-on="vehicle_make">"#));
    assert!(bmw.contains(r#"<optgroup label="BMW" data-parent="BMW">"#));
    assert!(bmw.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN" hidden>"#));
    // A model of another make filters nothing, nor shows as chosen.
    let mixed = get(pool, "/c/makina?vehicle_make=MERCEDES_BENZ&vehicle_model=VOLKSWAGEN_GOLF").await.body;
    assert!(mixed.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!mixed.contains(r#"<option value="VOLKSWAGEN_GOLF" selected>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn filters_on_properties(pool: PgPool) {
    let golf = "Volkswagen Golf 7, 2016, naftë";
    let mercedes = "Mercedes-Benz C 220, 2018, automatike";

    // SELECT: any of the chosen options.
    let body = get(pool.clone(), "/c/makina?vehicle_transmission=AUTOMATIC").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    // NUMBER: a range, typed the way people type numbers.
    let body = get(pool.clone(), "/c/makina?vehicle_mileage_max=100+000").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    // MULTISELECT: all of the chosen options.
    let body = get(pool.clone(), "/c/makina?vehicle_features=ABS&vehicle_features=NAVIGATION").await.body;
    assert!(body.contains(mercedes) && !body.contains(golf));
    let body = get(pool.clone(), "/c/makina?vehicle_features=ABS").await.body;
    assert!(body.contains(mercedes) && body.contains(golf));
    // The kind of listing.
    let body = get(pool, "/c/prona?type=RENT").await.body;
    assert!(body.contains("Apartament 2+1 me qira në Tiranë"));
    assert!(!body.contains("Shtëpi private në shitje në Vlorë"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn chosen_filters_can_be_removed_one_by_one(pool: PgPool) {
    let body = html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&city=durres&sort=price_desc").await;
    // Each chip links to the page without its own filter, in a fixed parameter order.
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?vehicle_fuel=DIESEL&sort=price_desc""#));
    assert!(body.contains(r#"<a class="filter-chip" href="/c/makina?city=durres&sort=price_desc""#));
    // "Pastro filtrat", and the card's "Pastro", drop the filters but keep the place, which has
    // its pills.
    assert!(body.contains(r#"<a class="clear-filters" href="/c/makina?city=durres&sort=price_desc">"#));
    assert!(body.contains(r#"<a class="button secondary filters-clear" href="/c/makina?city=durres&sort=price_desc">Pastro</a>"#));
    // The town's chip forgets the town kept for the next pages (application.js).
    assert!(body.contains(r#"href="/c/makina?vehicle_fuel=DIESEL&sort=price_desc" aria-label="Hiq filtrin Durrës" data-forget-town data-turbo-prefetch="false">"#));
    // The filters' button says which are on, to screen readers and as its tooltip, and how many;
    // the place is the pills' and the map's to say.
    assert!(body.contains(r#"aria-label="Filtrat: Naftë" title="Filtrat: Naftë""#));
    assert!(body.contains(r#"<span class="button-count" aria-hidden="true">1</span>"#));
    // The town is the filter form's first field, Qyteti, by county, with "Afër meje" beside it.
    // The form has no field for the search words, so it carries them along: filtering keeps them,
    // and the row's search changes them.
    let filter_form = |page: &str| {
        let form = &page[page.find(r#"<form id="filter-form""#).unwrap()..];
        form[..form.find("</form>").unwrap()].to_string()
    };
    let form = filter_form(&body);
    assert!(form.contains(r#"<select id="filter-city" name="city" data-action="filters#placed">"#));
    assert!(form.contains(r#"<option value="durres" selected>Durrës</option>"#));
    assert!(form.contains(r#"<button type="button" class="button secondary filter-locate" data-action="filters#locate">"#));
    assert!(!form.contains(r#"<input type="hidden" name="city""#) && !body.contains(r#"id="filter-q""#));
    let words = html(pool, "/c/makina?q=golf").await;
    assert!(filter_form(&words).contains(r#"<input type="hidden" name="q" value="golf">"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn unknown_filter_values_are_ignored(pool: PgPool) {
    let uri = "/c/makina?vehicle_fuel=NUCLEAR&price_min=abc&sort=sideways&city=atlantis&type=JOB&x=1";
    let response = get(pool, uri).await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(response.body.contains("Mercedes-Benz C 220, 2018, automatike"));
    assert!(!response.body.contains("filter-chip"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_detail_has_its_icon_its_categorys_where_that_says_more(pool: PgPool) {
    // A phone's brand is drawn as a phone, as its category is; a flat's rooms and furnishing say
    // what they are beside their icons, its floor after its name.
    let phone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(phone.contains(concat!(
        r#"<dd class="attribute" title="Marka"><svg class="attribute-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">"#,
        r#"<g transform="scale(.09375)" stroke="none"><path d="M192,56V200H64V56Z"/>"#,
    )));
    // A flat's page is laid out as a car's, as the canvas has it: its rooms, area, floor and
    // furnishing are its key facts in a row under its title, each with its icon, its name over its
    // value (they were pills once, among its details).
    let flat = html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    let start = flat.find(r#"<dl class="key-facts">"#).unwrap();
    let facts = &flat[start..start + flat[start..].find("</dl>").unwrap()];
    assert_eq!(facts.matches(r#"<div class="key-fact">"#).count(), 4, "{facts}");
    assert_eq!(facts.matches(r#"<svg class="key-fact-icon""#).count(), 4, "{facts}");
    for value in ["<dd>2+1</dd>", "<dd>85\u{a0}m²</dd>", "<dd>4</dd>", "<dd>I mobiluar</dd>"] {
        assert!(facts.contains(value), "{value} in {facts}");
    }
    assert!(!flat.contains(r#"<dl class="attributes">"#));
    // Beside a photo, among the pills, the category's key facts, those its cards show, are lit.
    assert!(phone.contains(r#"<dd class="attribute is-key""#), "{phone}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listing_page_shows_price_and_attributes(pool: PgPool) {
    let response = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert_eq!(response.status, StatusCode::OK);
    assert!(response.body.contains("9\u{a0}500\u{a0}€"));
    // Each detail a pill, its icon before its value; its name for screen readers and as a
    // tooltip. The options, several, a pill each.
    // Where and when it went up, on a car's page in the line under the title, as the canvas has
    // it (over the photo's corner on every other page): published two hours ago, "Sot"
    // (format::when), or "Dje" in the first two hours of the day; its number shorter at the end of
    // that line.
    let now = time::OffsetDateTime::now_utc();
    let when = marketplace::format::when(now - time::Duration::hours(2), now);
    assert!(response.body.contains(&format!(r#" · <span translate="no">Tiranë</span> · <span class="visually-hidden">Publikuar </span>{when} · <span class="listing-number">Nr. 1001</span></p>"#)), "{}", response.body);
    assert!(!response.body.contains(r#"<p class="listing-meta">"#) && !response.body.contains("Numri i njoftimit: 1001"));
    // One photo: no mosaic, the slider alone.
    assert!(!response.body.contains("mosaic-side"));
    // What kind of listing it is, and its condition, over the price, not among the details.
    assert!(response.body.contains(r#"<p class="card-meta listing-kind">Në shitje · E përdorur · <span translate="no">Tiranë</span>"#));
    assert!(!response.body.contains(">Gjendja</dt>"));
    // A car's page, laid out as FINN's: its key facts, the year, the mileage, the gearbox and the
    // fuel, in a row under the title, each with its icon and its name over its value; the rest of
    // its details in a grid after the description, a name with its icon over its value, the facts
    // left out; and its options in a list of their own, each with a check. No pills.
    let facts = &response.body[response.body.find(r#"<dl class="key-facts">"#).expect("the key facts")..];
    let facts = &facts[..facts.find("</dl>").unwrap()];
    let order = positions(facts, &["<dd>2016</dd>", "<dd>148\u{a0}000\u{a0}km</dd>", "<dd>Manuale</dd>", "<dd>Naftë</dd>"]);
    assert!(order.is_sorted(), "{order:?}");
    assert_eq!(facts.matches(r#"<div class="key-fact">"#).count(), 4, "{facts}");
    assert_eq!(facts.matches(r#"<svg class="key-fact-icon""#).count(), 4, "{facts}");
    assert!(facts.contains("<dt>Kilometrazhi</dt><dd>148\u{a0}000\u{a0}km</dd>"), "{facts}");
    assert!(response.body.find("</header>").unwrap() < response.body.find(r#"<dl class="key-facts">"#).unwrap());
    let specs = &response.body[response.body.find(r#"<dl class="spec-grid">"#).expect("the details' grid")..];
    let specs = &specs[..specs.find("</dl>").unwrap()];
    assert!(specs.contains("Karroceria</dt><dd>Hatchback</dd>"), "{specs}");
    assert!(specs.contains(r#"<div class="spec">"#) && specs.contains(r#"<dt><svg class="spec-icon""#), "{specs}");
    assert!(!specs.contains("Kilometrazhi") && !specs.contains("Naftë"), "the facts leave the details: {specs}");
    // The map under its own heading, "Lokacioni", on a car's page.
    let order = positions(&response.body, &[r#"<section class="listing-map-section""#, "<h2>Lokacioni</h2>", r#"<div class="listing-map""#]);
    assert!(order.is_sorted(), "{order:?}");
    // What qualifies the price on a line of its own under it, without its brackets.
    let mercedes = html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await;
    assert!(mercedes.contains(r#"<span class="price-note">i diskutueshëm</span>"#), "{mercedes}");
    assert!(!response.body.contains(r#"<dd class="attribute"#), "no pills on a car's page");
    assert!(response.body.contains("</svg>ABS</li>") && response.body.contains("</svg>Kondicioner</li>"));
    let order = positions(&response.body, &["<h2>Përshkrimi</h2>", "<h2>Detajet</h2>", r#"<section class="listing-options">"#, r#"<ul class="options">"#]);
    assert!(order.is_sorted(), "{order:?}");
    // Where it is, as a category page says it and the canvas draws it: a line of words, the way
    // back first, leading to its category without scripts, and no house (the logo leads to the
    // welcome page), then the category above its own, quieter, and its own, each a link to its
    // page.
    let path = positions(&response.body, &[
        r#"<nav class="breadcrumb listing-path" aria-label="Kategoria e njoftimit">"#,
        r#"<a class="breadcrumb-back" href="/c/makina" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#,
        r#"<a class="breadcrumb-up" href="/c/automjete">Automjete</a>"#,
        r#"<span class="breadcrumb-sep" aria-hidden="true">›</span>"#,
        r#"<a class="breadcrumb-here" href="/c/makina">Makina</a>"#,
    ]);
    assert!(path.is_sorted(), "{path:?}");
    assert!(!response.body.contains("Faqja kryesore"));
    assert!(!response.body.contains("breadcrumbs"));
    let html = response.body.replace("&#38;", "&");
    assert!(html.contains("photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=1200"));
    // Three levels down, the category above its own and its own, as on a category page.
    sqlx::query("UPDATE listings SET category_id = (SELECT id FROM categories WHERE slug = 'gps-dhe-navigatore') WHERE public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    let gps = get(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let path = positions(&gps, &[
        r#"<a class="breadcrumb-back" href="/c/gps-dhe-navigatore" aria-label="Kthehu""#,
        r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">Pjesë / aksesorë</a>"#,
        r#"<a class="breadcrumb-here" href="/c/gps-dhe-navigatore">GPS / navigatorë</a>"#,
    ]);
    assert!(path.is_sorted(), "{path:?}");
    assert!(!gps.contains("Faqja kryesore"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_photos_are_a_mosaic(pool: PgPool) {
    // Four photos: from 960 px up the slider, and the next two beside it, each going to its photo
    // in the slider, the second saying how many more there are and showing the filmstrip too.
    for (at, name) in ["two", "three", "four"].into_iter().enumerate() {
        sqlx::query(
            "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, thumbnail_url, width_px, height_px, sort_order)
             SELECT id, 'IMAGE', 'READY', $1, $2, $3, 1200, 900, $4 FROM listings WHERE public_no = 1001",
        )
        .bind(format!("test/{name}"))
        .bind(format!("https://example.com/{name}.jpg"))
        .bind(format!("https://example.com/{name}-card.jpg"))
        .bind(at as i32 + 1)
        .execute(&pool)
        .await
        .unwrap();
    }
    let page = html(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let side = &page[page.find(r#"<div class="mosaic-side">"#).expect("the mosaic's side")..];
    let side = &side[..side.find("</div>").unwrap()];
    assert_eq!(side.matches(r#"class="mosaic-tile""#).count(), 2, "{side}");
    assert!(side.contains(r#"<button type="button" class="mosaic-tile" data-action="slides#tile" data-slides-index-param="1" aria-label="Foto 2 nga 4">"#), "{side}");
    assert!(side.contains(r#"data-slides-index-param="2" data-slides-more-param="true" aria-label="Foto 3 nga 4">"#), "{side}");
    assert!(side.contains(r#"<span class="mosaic-more" aria-hidden="true">+1</span>"#), "{side}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_steps_to_its_neighbours_on_its_categorys_page(pool: PgPool) {
    // At the right end of the path, after its pills, three round buttons like the back one: to
    // the category's list, to the listing before this one on it and to the one after it, in the
    // list's own order, newest first. The Golf is Makina's newest, so it has nothing before it,
    // the button there greyed and no link, and the Mercedes after it; the Mercedes has the Golf
    // before it and nothing after.
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let mercedes = "/listing/1002-mercedes-benz-c-220-2018-automatike";
    let off_before = r#"<span class="back-button step-button is-off" aria-hidden="true"><svg data-icon="chevron-left""#;
    let off_after = r#"<span class="back-button step-button is-off" aria-hidden="true"><svg data-icon="chevron-right""#;
    let to_golf = format!(r#"<a class="back-button step-button" href="{golf}" aria-label="Njoftimi i mëparshëm" title="Njoftimi i mëparshëm"><svg data-icon="chevron-left""#);
    let to_mercedes = format!(r#"<a class="back-button step-button" href="{mercedes}" aria-label="Njoftimi tjetër" title="Njoftimi tjetër"><svg data-icon="chevron-right""#);
    let page = html(pool.clone(), golf).await;
    // The path at the page's top, as the canvas starts every listing (a banner's room stood over
    // it once).
    let top = positions(&page, &[r#"<article class="listing"#, r#"<div class="listing-tags">"#]);
    assert!(top.is_sorted(), "{top:?}");
    assert!(!page.contains("listing-banner"));
    let steps = positions(&page, &[
        "Makina</a>",
        r#"<nav class="listing-steps" aria-label="Njoftimet e tjera">"#,
        r#"<a class="back-button list-button" href="/c/makina" aria-label="Lista e njoftimeve" title="Lista e njoftimeve"><svg data-icon="list""#,
        off_before,
        &to_mercedes,
    ]);
    assert!(steps.is_sorted(), "{steps:?}");
    assert!(!page.contains("Njoftimi i mëparshëm"));
    let page = html(pool.clone(), mercedes).await;
    let steps = positions(&page, &[&to_golf, off_after]);
    assert!(steps.is_sorted(), "{steps:?}");
    assert!(!page.contains("Njoftimi tjetër"));
    // Sold, the Golf is off the list, so the Mercedes has nothing before it; the Golf's own page
    // still steps to those either side of where it was.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001").execute(&pool).await.unwrap();
    let page = html(pool.clone(), mercedes).await;
    assert!(page.contains(off_before) && !page.contains(&to_golf));
    assert!(html(pool.clone(), golf).await.contains(&to_mercedes));
    sqlx::query("UPDATE listings SET status = 'ACTIVE', sold_at = NULL WHERE public_no = 1001").execute(&pool).await.unwrap();
    // Bumped, the Mercedes goes first: the Golf then has it before, and nothing after.
    sqlx::query("UPDATE listings SET bumped_at = now() WHERE public_no = 1002").execute(&pool).await.unwrap();
    let page = html(pool.clone(), golf).await;
    let mercedes_before = to_mercedes.replace("Njoftimi tjetër", "Njoftimi i mëparshëm").replace("chevron-right", "chevron-left");
    assert!(page.contains(&mercedes_before) && page.contains(off_after));
    // For Besnik, who blocked Ana, her Golf isn't on the list: the Mercedes has nothing after it.
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&besnik)).await;
    let golf_after = to_golf.replace("Njoftimi i mëparshëm", "Njoftimi tjetër").replace("chevron-left", "chevron-right");
    assert!(html(pool.clone(), mercedes).await.contains(&golf_after));
    assert!(get_with_cookie(pool.clone(), mercedes, &besnik).await.body.contains(off_after));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn costly_things_on_sale_offer_a_loan_calculator(pool: PgPool) {
    let car = html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await;
    // A bubble beside the price, like the price per square metre's, which opens the calculator in a
    // card under it the same way: the years and the interest, and what they come to, the down
    // payment a set share of the price (what the banks ask, LOAN_DOWN_PERCENT), the monthly
    // payment last. On a car's page too, in the price's card at the right.
    let start = r#"<details class="bubble loan" data-controller="dropdown loan" data-loan-price-value="21500.00" data-loan-currency-value="€" data-loan-down-percent-value="35">"#;
    let loan = &car[car.find(start).expect("the loan calculator's bubble")..];
    let loan = &loan[..loan.find("</details>").unwrap()];
    assert!(loan[..loan.find("</summary>").unwrap()].contains("Llogarit kredinë"));
    assert!(!loan.contains("loan-down\""), "no field for the down payment");
    let card = positions(loan, &[
        r#"<div class="bubble-card loan-card">"#,
        r#"<input id="loan-years""#,
        r#"<input id="loan-rate""#,
        "<dt>Paradhënia (35%)</dt>",
        "<dt>Kredia</dt>",
        "<dt>Kësti mujor</dt>",
    ]);
    assert!(card.is_sorted(), "{card:?}");
    assert!(car.find(r#"<div class="price-box">"#).unwrap() < car.find(start).unwrap() && !car.contains("listing-loan"));
    // Anything on sale for more than 500 € has it, the Golf at 9 500 € too; a 4 500 Lekë jacket
    // doesn't.
    assert!(html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.contains("Llogarit kredinë"));
    assert!(!html(pool.clone(), "/listing/1012-xhakete-dimri-per-meshkuj-masa-m").await.contains("data-loan-price-value"));
    // In Lekë, the limit is about the same as in euros: 50 000 Lekë isn't more than it, 50 001 is.
    for (price, offered) in [(50_000, false), (50_001, true)] {
        sqlx::query("UPDATE listings SET price_amount = $1::numeric, currency = 'ALL', price_type = 'FIXED' WHERE public_no = 1005")
            .bind(price)
            .execute(&pool)
            .await
            .unwrap();
        let page = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
        assert_eq!(page.contains("Llogarit kredinë"), offered, "{price}");
    }
    // A rent is paid monthly already.
    let rent = html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    // The month follows the price, smaller, off the band of the accent across the figure's foot
    // (as Tradera marks its prices, 2026-10-11); a rent says nothing of square metres.
    assert!(rent.contains("<p class=\"listing-price\"><span class=\"price-amount\">450\u{a0}€</span><span class=\"price-note\">/muaj</span></p>"));
    assert!(!rent.contains("Llogarit kredinë"));
    assert!(!rent.contains("price-per-area"));
    // A house on sale says under its price what a square metre costs, 185 000 € for 200 m², in a
    // bubble before the loan calculator's, which opens how it was worked out.
    let house = html(pool, "/listing/1004-shtepi-private-ne-shitje-ne-vlore").await;
    let row = &house[house.find(r#"<div class="price-row with-area">"#).expect("the price's row")..];
    let row = &row[..row.find(r#"<details class="bubble loan""#).unwrap()];
    assert!(row.contains("<p class=\"listing-price\"><span class=\"price-amount\">185\u{a0}000\u{a0}€</span></p>"));
    let bubble = &row[row.find(r#"<details class="bubble area-price" data-controller="dropdown">"#).expect("the bubble")..];
    assert!(bubble[..bubble.find("</summary>").unwrap()].contains("925\u{a0}€/m²"));
    for line in ["<dd>185\u{a0}000\u{a0}€</dd>", "<dd>200\u{a0}m²</dd>", "<dd>925\u{a0}€/m²</dd>", "rrumbullakuar në euro."] {
        assert!(bubble.contains(line), "{line}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn outdated_slug_redirects_to_the_current_one(pool: PgPool) {
    let response = get(pool, "/listing/1001-old-title").await;
    assert_eq!(response.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.location.as_deref(),
        Some("/listing/1001-volkswagen-golf-7-2016-nafte")
    );
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_parent_category_pages_in_order_across_its_subcategories(pool: PgPool) {
    // Thirty more each in Makina and Motoçikleta, both under Automjete, their prices and their
    // distances from Tirana's middle taking turns between the two. A page over Automjete reads
    // each subcategory's first in order and merges them (db::push_ordered); page after page, the
    // cards come as one ORDER BY over all of them would have them.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, price_normalized, currency, status,
                               published_at, latitude, longitude)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                (SELECT id FROM categories WHERE slug = CASE WHEN g % 2 = 0 THEN 'makina' ELSE 'motocikleta' END),
                'rradhe-' || g, 'Rradhë ' || g, '', 1000 + g * 37 % 60 * 10, 1000 + g * 37 % 60 * 10,
                'EUR', 'ACTIVE', now() - g * interval '1 hour', 41.3275 + g * 0.001, 19.8187
         FROM generate_series(1, 60) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let subtree = "WITH RECURSIVE tree AS (SELECT id FROM categories WHERE slug = 'automjete'
                                          UNION ALL
                                          SELECT c.id FROM categories c JOIN tree ON c.parent_id = tree.id)
                   SELECT id FROM tree";
    let distance = "(ll_to_earth(l.latitude::float8, l.longitude::float8) <-> ll_to_earth(41.3275, 19.8187))";
    for (uri, condition, order) in [
        ("/c/automjete", "true".to_string(), "l.sort_at DESC, l.id DESC".to_string()),
        ("/c/automjete?sort=price_asc", "true".to_string(), "coalesce(l.price_normalized, 'Infinity'), l.id".to_string()),
        (
            "/c/automjete?sort=price_desc",
            "true".to_string(),
            "coalesce(l.price_normalized, '-Infinity') DESC, l.id DESC".to_string(),
        ),
        (
            "/c/automjete?lat=41.3275&lng=19.8187&radius=50",
            format!("l.latitude IS NOT NULL AND {distance} <= 50000"),
            format!("{distance}, l.id"),
        ),
    ] {
        let expected: Vec<i64> = sqlx::query_scalar(&format!(
            "SELECT l.public_no FROM listings l
             WHERE l.status = 'ACTIVE' AND l.category_id IN ({subtree}) AND {condition}
             ORDER BY {order}"
        ))
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(expected.len() > 2 * 24, "{uri}: more than two pages");
        assert_eq!(every_page(&pool, uri).await, expected, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_shows_listings_like_it(pool: PgPool) {
    // Three more Golfs from Besnik at about the Golf's price, Ana's own second Golf, and an Audi
    // from Dritan, all in Makina in Tirana.
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title, description,
                               price_amount, price_normalized, currency, status, published_at,
                               latitude, longitude)
         SELECT v.seller::uuid, v.seller::uuid, (SELECT id FROM categories WHERE slug = 'makina'),
                v.slug, v.title, '', v.price, v.price, 'EUR', 'ACTIVE', now() - interval '1 hour',
                41.3275, 19.8187
         FROM (VALUES
             ('22222222-2222-4222-8222-222222222222', 'golf-a', 'Golf A', 9400),
             ('22222222-2222-4222-8222-222222222222', 'golf-b', 'Golf B', 9700),
             ('22222222-2222-4222-8222-222222222222', 'golf-c', 'Golf C', 9900),
             ('11111111-1111-4111-8111-111111111111', 'golf-d', 'Golf D', 9500),
             ('33333333-3333-4333-8333-333333333333', 'audi-a3', 'Audi A3', 9600)
         ) AS v(seller, slug, title, price)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_option_id)
         SELECT l.id, a.id, a.data_type, o.id
         FROM listings l
         CROSS JOIN (VALUES ('vehicle_make', 'VOLKSWAGEN'), ('vehicle_model', 'VOLKSWAGEN_GOLF')) AS v(code, value_code)
         JOIN attributes a ON a.code = v.code
         JOIN attribute_options o ON o.attribute_id = a.id AND o.value_code = v.value_code
         WHERE l.slug LIKE 'golf-_'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let number = |slug: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT public_no FROM listings WHERE slug = $1")
                .bind(slug)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let (golf_a, golf_b, audi) = (number("golf-a").await, number("golf-b").await, number("audi-a3").await);

    // Under the Golf: the other sellers' cars, the same model first, the closest in price before;
    // two of Besnik's three Golfs, none of Ana's own, not the Golf itself, and no flats. The
    // Mercedes comes last: it shares only the fuel, costs twice as much, and is in Durrës.
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let related = &page[page.find(r#"<section class="related" aria-labelledby="related-heading">"#).expect("related")..];
    let related = &related[..related.find("</section>").unwrap()];
    assert!(related.contains(r#"<h2 id="related-heading">Njoftime të ngjashme</h2>"#));
    assert_eq!(card_numbers(related), vec![golf_a, golf_b, audi, 1002]);
    // Four being all of them, no frame asks for more. The frame's answer, from the third on, in
    // the frame the page would have named, with no frame after it either; from the first, all
    // four. Asked for as a page, it leads back to the listing.
    assert!(!related.contains("page-related-"), "{related}");
    let frame = |from: i64| {
        let pool = pool.clone();
        async move {
            let request = Request::get(format!("/listing/1001/te-ngjashme?nga={from}"))
                .header("turbo-frame", format!("page-related-{from}"))
                .body(Body::empty())
                .unwrap();
            send(pool, request).await.body
        }
    };
    let rest = frame(2).await;
    assert!(rest.starts_with(r#"<turbo-frame id="page-related-2" target="_top">"#), "{rest}");
    assert_eq!(card_numbers(&rest), vec![audi, 1002]);
    assert!(!rest.contains("page-related-4"));
    assert_eq!(card_numbers(&frame(0).await), vec![golf_a, golf_b, audi, 1002]);
    let page = get(pool.clone(), "/listing/1001/te-ngjashme?nga=2").await;
    assert_eq!(page.status, StatusCode::SEE_OTHER);
    assert_eq!(page.location.as_deref(), Some("/listing/1001"));

    // A listing alone in its category has nothing under it.
    let dog = html(pool, "/listing/1009-kelysh-labrador-3-muajsh").await;
    assert!(!dog.contains("related-heading"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_can_say_where_it_is_on_the_map(pool: PgPool) {
    let seller = new_seller(&pool, "+355692223355").await;
    let started = started();
    let listing = |category: &'static str, latitude: &'static str, longitude: &'static str| {
        let mut fields = listing_fields(("category", category), &started);
        if category == "apartamente" {
            fields.retain(|field| field.0 != "condition");
            fields.push(("prop_property_area", "90"));
            fields.retain(|field| field.0 != "title");
            fields.push(("title", "Apartament 2+1 te Blloku"));
        }
        fields.extend([("latitude", latitude), ("longitude", longitude)]);
        fields
    };
    let point = |pool: PgPool, no: String| async move {
        sqlx::query_as::<_, (f64, f64)>("SELECT latitude::float8, longitude::float8 FROM listings WHERE public_no = $1::bigint")
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
    };
    let number = |posted: &TestResponse| posted.location.clone().expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().to_string();

    // The form offers a map for property and cars, whose pages show one, starting on each town's
    // point.
    let form = get_with_cookie(pool.clone(), "/shto", &seller).await.body;
    let option = |slug: &str| {
        let start = form.find(&format!(r#"<option value="{slug}""#)).unwrap();
        form[start..start + form[start..].find('>').unwrap()].to_string()
    };
    assert!(option("apartamente").contains("data-mapped") && option("makina").contains("data-mapped"));
    assert!(!option("telefona").contains("data-mapped"));
    assert!(form.contains(r#"<div class="field map-pin" data-controller="map-pin" data-map-pin-towns-value="{&#34;"#), "{form}");
    assert!(form.contains("&#34;tirane&#34;:[41.3"));

    // Put in Blloku, it stands there, to about a hundred metres.
    let posted = post_form(pool.clone(), "/shto", &listing("apartamente", "41.318762", "19.817612"), Some(&seller)).await;
    let flat = number(&posted);
    assert_eq!(point(pool.clone(), flat.clone()).await, (41.319, 19.818));
    let page = get(pool.clone(), posted.location.as_deref().unwrap()).await.body;
    assert!(page.contains(r#"data-listing-map-latitude-value="41.319"#), "{page}");

    // In Shkodër while the town is Tirana it is refused, as is a point it can't read.
    let far = post_form(pool.clone(), "/shto", &listing("apartamente", "42.0693", "19.5126"), Some(&seller)).await;
    assert_eq!(far.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(far.body.contains("Pika në hartë është larg qytetit që zgjodhe: vendose te qyteti, ose hiqe."), "{}", far.body);
    let garbled = post_form(pool.clone(), "/shto", &listing("apartamente", "e-ra", "19.8"), Some(&seller)).await;
    assert!(garbled.body.contains("Pika në hartë nuk u lexua: vendose përsëri."));

    // A phone's is let go: it stands at its town's point.
    let phone = number(&post_form(pool.clone(), "/shto", &listing("telefona", "41.318762", "19.817612"), Some(&seller)).await);
    let tirana: (f64, f64) = sqlx::query_as("SELECT latitude::float8, longitude::float8 FROM locations WHERE slug = 'tirane' AND location_type = 'MUNICIPALITY'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(point(pool.clone(), phone).await, tirana);

    // Its edit starts from the point, and moves it.
    let edit = get_with_cookie(pool.clone(), &format!("/listing/{flat}/ndrysho"), &seller).await.body;
    assert!(edit.contains(r#"<input type="hidden" name="latitude" value="41.319" data-map-pin-target="latitude">"#), "{edit}");
    assert!(edit.contains(r#"data-map-pin-target="clear" data-action="map-pin#clear">Hiqe pikën</button>"#));
    post_form(pool.clone(), &format!("/listing/{flat}/ndrysho"), &listing("apartamente", "41.3301", "19.8201"), Some(&seller)).await;
    assert_eq!(point(pool.clone(), flat).await, (41.33, 19.82));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_says_its_zone(pool: PgPool) {
    // A seller new today: the fixtures' have posted their five for the day.
    let seller = new_seller(&pool, "+355692223344").await;
    let started = started();
    let fields = |category: &'static str, city: &'static str, zone: &'static str, title: &'static str| {
        let mut fields = listing_fields(("city", city), &started);
        for field in &mut fields {
            match field.0 {
                "category" => field.1 = category,
                "title" => field.1 = title,
                _ => {}
            }
        }
        fields.retain(|field| field.0 != "condition");
        fields.push(("zone", zone));
        fields.push(("prop_property_area", "90"));
        fields
    };

    // The form offers Tirana's zones under Tirana, for property only.
    let form = get_with_cookie(pool.clone(), "/shto", &seller).await.body;
    assert!(form.contains(r#"<optgroup label="Tiranë" data-parent="tirane">"#));
    assert!(form.contains(r#"<option value="blloku">Blloku</option>"#));
    let option = |slug: &str| {
        let start = form.find(&format!(r#"<option value="{slug}""#)).unwrap();
        form[start..start + form[start..].find('>').unwrap()].to_string()
    };
    assert!(option("apartamente").contains("data-zoned"));
    assert!(!option("telefona").contains("data-zoned"));

    // Another town's zone is a mistake.
    let durres = post_form(pool.clone(), "/shto", &fields("apartamente", "tirane", "plazh", "Apartament 2+1 me ashensor"), Some(&seller)).await;
    assert_eq!(durres.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(durres.body.contains("Zgjidh një zonë të qytetit."));

    // In Blloku: the listing sits there, at Tirana's point on the map.
    let posted = post_form(pool.clone(), "/shto", &fields("apartamente", "tirane", "blloku", "Apartament 2+1 me ashensor"), Some(&seller)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let (number, zone, at_tirana): (i64, String, bool) = sqlx::query_as(
        "SELECT l.public_no, loc.name, l.latitude = town.latitude AND l.longitude = town.longitude
         FROM listings l JOIN locations loc ON loc.id = l.location_id
         JOIN locations town ON town.slug = 'tirane' AND town.location_type = 'MUNICIPALITY'
         WHERE l.title = 'Apartament 2+1 me ashensor'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(zone, "Blloku");
    assert!(at_tirana);

    // Its page says so, in its place and first among its details (its area is a key fact over
    // them); its card too, and filtering by Tirana or searching for Blloku finds it.
    let page = html(pool.clone(), &format!("/listing/{number}-apartament-2-1-me-ashensor")).await;
    assert!(page.contains("Blloku, Tiranë · "));
    let details = &page[page.find(r#"<dl class="spec-grid">"#).unwrap()..];
    assert!(details[..details.find("</dt>").unwrap()].ends_with("Zona"), "{details}");
    assert!(details.contains("<dd>Blloku</dd>"), "{details}");
    let card = format!(r#"href="/listing/{number}-apartament-2-1-me-ashensor""#);
    let flats = html(pool.clone(), "/c/apartamente").await;
    assert!(flats.contains(&card) && flats.contains("Blloku, Tiranë"));
    let in_tirana = html(pool.clone(), "/c/apartamente?city=tirane").await;
    assert!(in_tirana.contains(&card));
    assert!(html(pool.clone(), "/search?q=blloku").await.contains(&card));

    // The edit form has it chosen, and leaves it to the seller: only a new listing's form picks
    // the zone its words name (a_new_listings_zone_follows_its_words).
    let edit = get_with_cookie(pool.clone(), &format!("/listing/{number}/ndrysho"), &seller).await.body;
    assert!(edit.contains(r#"<option value="blloku" selected>Blloku</option>"#));
    assert!(!edit.contains("data-listing-form-guess-value"));

    // A phone has no zone: it sits in the town.
    let phone = post_form(pool.clone(), "/shto", &fields("telefona", "tirane", "blloku", "Samsung Galaxy S21, 128 GB"), Some(&seller)).await;
    assert_eq!(phone.status, StatusCode::SEE_OTHER, "{}", phone.body);
    let place: String = sqlx::query_scalar(
        "SELECT loc.location_type FROM listings l JOIN locations loc ON loc.id = l.location_id
         WHERE l.title = 'Samsung Galaxy S21, 128 GB'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(place, "MUNICIPALITY");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_new_listings_zone_follows_its_words(pool: PgPool) {
    // The form asks for it as the seller writes (listing_form_controller.js).
    assert!(get(pool.clone(), "/shto").await.body.contains(r#"data-listing-form-guess-value="true""#));
    let guess = |fields: Vec<(&'static str, &'static str)>| {
        let pool = pool.clone();
        async move {
            let answer = post_form(pool, "/shto/zona", &fields, None).await;
            assert_eq!(answer.status, StatusCode::OK);
            serde_json::from_str::<Value>(&answer.body).unwrap()
        }
    };
    // In the town chosen, the zone the title names, in any of its forms...
    let words = |title, description, city| vec![("title", title), ("description", description), ("city", city)];
    assert_eq!(guess(words("Apartament 2+1 në Bllok", "Me ashensor, i mobiluar.", "tirane")).await, json!({"city": "tirane", "zone": "blloku"}));
    // ...or else the description, the title first.
    assert_eq!(guess(words("Apartament 2+1", "Ndodhet te Komuna e Parisit.", "tirane")).await, json!({"city": "tirane", "zone": "komuna-e-parisit"}));
    assert_eq!(guess(words("Apartament në Plazh", "Larg qendrës.", "durres")).await, json!({"city": "durres", "zone": "plazh"}));
    // With no town chosen, a zone with its town, which is picked too.
    assert_eq!(guess(words("Shitet apartament 2+1", "Ndodhet në Plazh, Durrës.", "")).await, json!({"city": "durres", "zone": "plazh"}));
    // A zone with no town to say whose, another town's, or none at all: nothing.
    assert_eq!(guess(words("Apartament pranë plazhit", "Me pamje nga deti.", "")).await, json!({}));
    assert_eq!(guess(words("Apartament në Bllok", "Me ashensor.", "durres")).await, json!({}));
    assert_eq!(guess(words("Volkswagen Golf 7", "Naftë, 2016.", "tirane")).await, json!({}));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_or_a_car_shows_where_it_is_on_a_map_under_its_seller(pool: PgPool) {
    // The flat and the Golf, in Tirana: a map round their point, the town's, the town named over it,
    // under the seller's card.
    for uri in ["/listing/1003-apartament-2-1-me-qira-ne-tirane", "/listing/1001-volkswagen-golf-7-2016-nafte"] {
        let page = html(pool.clone(), uri).await;
        let map = page.find(r#"<section class="listing-map-section" aria-label="Vendndodhja""#).unwrap_or_else(|| panic!("{uri}"));
        assert!(page.find(r#"<section class="seller-section""#).unwrap() < map, "{uri}");
        let section = &page[map..map + page[map..].find("</section>").unwrap()];
        assert!(section.contains(r#"<div class="listing-map" data-controller="listing-map""#), "{section}");
        assert!(section.contains(r#"data-listing-map-latitude-value="41.328333" data-listing-map-longitude-value="19.818056""#), "{section}");
        assert!(section.contains(r#"<p class="listing-map-place" translate="no">Tiranë</p>"#), "{section}");
    }
    // A phone's page has none.
    assert!(!html(pool, "/listing/1005-iphone-13-128-gb-si-i-ri").await.contains("listing-map"));
}

/// Golfs up for sale, as 1001 is but for their years and their prices in euros.
async fn golfs(pool: &PgPool, cars: &[(i32, i64)]) {
    for (year, price) in cars {
        sqlx::query(
            "WITH l AS (
                 INSERT INTO listings (seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                                       listing_type, condition, price_amount, currency, price_type, price_normalized, status, published_at)
                 SELECT seller_user_id, created_by_user_id, category_id, location_id, slug, title, description,
                        listing_type, condition, $2::numeric, 'EUR', 'FIXED', $2::numeric, 'ACTIVE', now() - interval '3 days'
                 FROM listings WHERE public_no = 1001
                 RETURNING id
             )
             INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_text, value_number, value_boolean, value_date, value_option_id)
             SELECT l.id, v.attribute_id, v.data_type, v.value_text,
                    CASE WHEN a.code = 'vehicle_year' THEN $1::numeric ELSE v.value_number END,
                    v.value_boolean, v.value_date, v.value_option_id
             FROM l, listing_attribute_values v JOIN attributes a ON a.id = v.attribute_id
             WHERE v.listing_id = 'a0000000-0000-4000-8000-000000001001'",
        )
        .bind(year)
        .bind(price)
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_price_is_compared_with_what_cars_of_its_model_and_year_ask(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let refresh = || marketplace::db::refresh_car_prices(&pool);
    // Its price, now, with no history of it: a drop would have its own pill on the card.
    let priced = |price: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query("UPDATE listings SET price_amount = $1::numeric, price_normalized = $1::numeric WHERE public_no = 1001")
                .bind(price)
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("DELETE FROM listing_price_history WHERE listing_id = 'a0000000-0000-4000-8000-000000001001'").execute(&pool).await.unwrap();
        }
    };
    let hint = |query: &'static str| {
        let pool = pool.clone();
        async move { get(pool, &format!("/shto/cmimi?{query}")).await }
    };

    // Too few Golfs to say anything.
    refresh().await.unwrap();
    assert!(!get(pool.clone(), golf).await.body.contains("car-price"));
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.status, StatusCode::NO_CONTENT);

    // Nine more, from 2014 to 2018: ten, whose median is 10 250 €, the middle half's 9 500 to
    // 11 375; its 9 500 € is about it.
    golfs(&pool, &[(2014, 8000), (2015, 9000), (2015, 9500), (2016, 10000), (2016, 10500), (2017, 11000), (2017, 11500), (2018, 12000), (2018, 12500)]).await;
    refresh().await.unwrap();
    let page = get(pool.clone(), golf).await.body;
    assert!(page.contains(r#"<details class="bubble car-price price-near" data-controller="dropdown">"#), "{page}");
    assert!(page.contains("Rreth mesatares"));
    // A circle beside "Llogarit kredinë", after it, its tag alone, on the verdict's colour (a light
    // one under our black on the light theme, a deeper one under our white on the dark), the
    // verdict said to screen readers and under the pointer, and first in its card: the user's ask
    // of 2026-10-11.
    assert!(page.contains(r#"<div class="price-row with-car">"#), "{page}");
    assert!(page.find(r#"<details class="bubble loan""#).unwrap() < page.find(r#"<details class="bubble car-price"#).unwrap());
    assert!(page.contains(r#"<summary title="Si krahasohet: Rreth mesatares">"#), "{page}");
    assert!(page.contains(r#"<span class="visually-hidden">Rreth mesatares</span>"#), "{page}");
    assert!(page.contains(r#"<p class="car-price-verdict">Rreth mesatares</p>"#), "{page}");
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    assert!(css.contains(".car-price:is(.price-below,.price-near,.price-above) > summary{background:var(--tone-fill);color:var(--text);}"));
    assert!(css.contains("--near-fill:#f8e3a0;") && css.contains("--near-fill:#85660a;"));
    assert!(page.contains("<div><dt>Mesatarja</dt><dd>10\u{a0}300\u{a0}€</dd></div>"), "{page}");
    assert!(page.contains("<div><dt>Shumica kërkojnë</dt><dd>9\u{a0}500–11\u{a0}400\u{a0}€</dd></div>"), "{page}");
    assert!(page.contains("Nga 10 Volkswagen Golf 2014–2018 në shitje, pa makinat për pjesë."), "{page}");
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("card-bargain"));

    // At 8 000 €, against the median as it stands (no rebuild), a good price, on its card too; at
    // 13 000 €, over it; at 4 000 €, so far under it that it says nothing.
    priced(8000).await;
    let page = get(pool.clone(), golf).await.body;
    assert!(page.contains(r#"<details class="bubble car-price price-below""#) && page.contains("Çmim i mirë"), "{page}");
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert_eq!(cars.matches(r#"<span class="card-bargain">Çmim i mirë</span>"#).count(), 1, "{cars}");
    priced(13000).await;
    assert!(get(pool.clone(), golf).await.body.contains(r#"<details class="bubble car-price price-above""#));
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("card-bargain"));
    priced(4000).await;
    assert!(!get(pool.clone(), golf).await.body.contains("car-price"));

    // The listing form's hint: what such cars ask, and how the price typed will compare, in the
    // currency chosen; too few cars, or no model, and it says nothing.
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=8%20000").await.body).unwrap();
    assert_eq!(answer["text"], "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9\u{a0}500–11\u{a0}400\u{a0}€, mesatarja 10\u{a0}300\u{a0}€.");
    assert_eq!(answer["verdict"], "Te faqja e makinës, çmimi yt del „Çmim i mirë“.");
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=900").await.body).unwrap();
    assert_eq!(answer["verdict"], "Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?");
    let answer: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=EUR").await.body).unwrap();
    assert_eq!(answer["verdict"], Value::Null);
    let lek: Value = serde_json::from_str(&hint("model=VOLKSWAGEN_GOLF&year=2016&currency=ALL&price=1000000").await.body).unwrap();
    assert!(lek["text"].as_str().unwrap().ends_with("0\u{a0}Lekë."), "{lek}");
    assert_eq!(lek["verdict"], "Te faqja e makinës, çmimi yt del „Rreth mesatares“.");
    assert_eq!(hint("model=VOLKSWAGEN_GOLF&year=2025&currency=EUR").await.status, StatusCode::NO_CONTENT);
    assert_eq!(hint("model=BMW_3_SERIES&year=2016&currency=EUR").await.status, StatusCode::NO_CONTENT);
    assert_eq!(hint("year=2016").await.status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_property_in_a_zone_compares_its_square_metre_with_the_zones(pool: PgPool) {
    // Eleven flats on sale in Blloku, 100 m² each: ten from 700 to 1 200 € a square metre, and one
    // at 5 000 € as a mistyped price would have it. The zone's average leaves out the cheapest and
    // the dearest tenth, so the mistake doesn't pull it: it is 1 000 €/m².
    sqlx::query(
        "INSERT INTO listings (public_no, seller_user_id, created_by_user_id, category_id, location_id, slug,
                               title, description, listing_type, price_amount, price_normalized, currency,
                               price_type, status, published_at)
         OVERRIDING SYSTEM VALUE
         SELECT 2000 + n, '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                (SELECT id FROM categories WHERE slug = 'apartamente'),
                (SELECT id FROM locations WHERE slug = 'blloku' AND location_type = 'AREA'),
                'apartament-' || n, 'Apartament ' || n, 'Apartament në Bllok.', 'SELL',
                price, price, 'EUR', 'FIXED', 'ACTIVE', now()
         FROM (VALUES (1, 70000), (2, 80000), (3, 89000), (4, 100000), (5, 100000), (6, 100000),
                      (7, 100000), (8, 100000), (9, 111000), (10, 120000), (11, 500000)) AS v (n, price)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO listing_attribute_values (listing_id, attribute_id, data_type, value_number)
         SELECT l.id, a.id, a.data_type, 100
         FROM listings l JOIN attributes a ON a.code = 'property_area'
         WHERE l.public_no BETWEEN 2001 AND 2011",
    )
    .execute(&pool)
    .await
    .unwrap();
    marketplace::db::refresh_zone_prices(&pool).await.unwrap();

    let page = |n: i64| format!("/listing/{n}-apartament-{}", n - 2000);
    // The bubble under the price, from its opening tag to its end.
    let bubble = |html: String| {
        let start = html.find(r#"<details class="bubble area-price"#).expect("the bubble");
        html[start..start + html[start..].find("</details>").unwrap()].to_string()
    };

    // 200 € a square metre under the average, or more: green, and the card says the average, by
    // how much, and what the average is worked out from.
    let cheap = bubble(html(pool.clone(), &page(2002)).await);
    assert!(cheap.starts_with(r#"<details class="bubble area-price price-below" data-controller="dropdown">"#), "{cheap}");
    for line in [
        "<dd>800\u{a0}€/m²</dd>",
        "<dt>Mesatarja e zonës</dt><dd>1\u{a0}000\u{a0}€/m²</dd>",
        "Nën mesataren: 200\u{a0}€/m² më lirë",
        "Mesatarja: 11 njoftime në shitje në Blloku (Apartamente), pa çmimet më të ulëta e më të larta.",
    ] {
        assert!(cheap.contains(line), "{line}");
    }
    // Less than 200 € from it either way: yellow.
    let near = bubble(html(pool.clone(), &page(2003)).await);
    assert!(near.starts_with(r#"<details class="bubble area-price price-near""#));
    assert!(near.contains("Afër mesatares: 110\u{a0}€/m² më lirë"));
    let even = bubble(html(pool.clone(), &page(2004)).await);
    assert!(even.starts_with(r#"<details class="bubble area-price price-near""#));
    assert!(even.contains("Njësoj si mesatarja"));
    // 200 € over it, or more: red.
    let dear = bubble(html(pool.clone(), &page(2010)).await);
    assert!(dear.starts_with(r#"<details class="bubble area-price price-above""#));
    assert!(dear.contains("Mbi mesataren: 200\u{a0}€/m² më shtrenjtë"));

    // A price in Lekë meets the average in Lekë: 1 000 € is 100 000 Lekë.
    sqlx::query("UPDATE listings SET price_amount = 10000000, currency = 'ALL' WHERE public_no = 2004")
        .execute(&pool)
        .await
        .unwrap();
    let in_lek = bubble(html(pool.clone(), &page(2004)).await);
    assert!(in_lek.contains("<dt>Mesatarja e zonës</dt><dd>100\u{a0}000\u{a0}Lekë/m²</dd>"), "{in_lek}");
    assert!(in_lek.contains("Njësoj si mesatarja"));

    // A starting price says too little to compare.
    sqlx::query("UPDATE listings SET price_type = 'FROM' WHERE public_no = 2010").execute(&pool).await.unwrap();
    let from = bubble(html(pool.clone(), &page(2010)).await);
    assert!(from.starts_with(r#"<details class="bubble area-price" data-controller="dropdown">"#), "{from}");
    assert!(!from.contains("Mesatarja"));

    // Fewer than five sales in the zone make no average.
    sqlx::query("DELETE FROM listings WHERE public_no BETWEEN 2001 AND 2008").execute(&pool).await.unwrap();
    marketplace::db::refresh_zone_prices(&pool).await.unwrap();
    let alone = bubble(html(pool.clone(), &page(2009)).await);
    assert!(alone.starts_with(r#"<details class="bubble area-price" data-controller="dropdown">"#), "{alone}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_listing_form_names_its_fields_in_white_boxes(pool: PgPool) {
    // Signed out, as the canvas has it: what is sold, its category, its price, where it is and how
    // to reach the seller, each in a white box, each field named over it, the photos taken by a
    // zone, then the button across the column (in three sections under headings once, with tips
    // beside them).
    let form = get(pool.clone(), "/shto").await.body;
    let order = positions(
        &form,
        &[
            r#"<div class="form-box">"#,
            r#"<label class="field-label" for="title">Titulli</label>"#,
            r#"<label class="field-label" for="description">Përshkrimi</label>"#,
            r#"<span class="field-label" id="photos-label">Fotot</span>"#,
            r#"<label class="photo-drop" data-photos-target="zone">"#,
            r#"<div class="form-row category-row">"#,
            r#"<label class="field-label" for="category">Kategoria</label>"#,
            r#"<div class="price-fields">"#,
            r#"<label class="field-label" for="price_type">Çmimi</label>"#,
            r#"<label class="field-label" for="city">Qyteti</label>"#,
            r#"<label class="field-label" for="name">Emri <span class="optional">(s'është i detyrueshëm)</span></label>"#,
            r#"<label class="field-label" for="contact">Celulari ose email-i</label>"#,
            r#"<button type="submit" class="button">Publiko njoftimin</button>"#,
        ],
    );
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
    assert_eq!(form.matches(r#"<div class="form-box">"#).count(), 5, "{form}");
    assert!(!form.contains("listing-form-tips") && !form.contains(r#"<h2 id="section-"#), "{form}");
    assert!(form.contains(r#"placeholder="p.sh. Golf 7, 2016, naftë""#));
    // The currency a two-part toggle, as the filters' price has it (a list once).
    assert!(form.contains(r#"<div class="segmented currency-choice" role="radiogroup" aria-label="Monedha" data-listing-form-target="currency">"#), "{form}");
    assert_eq!(form.matches(r#"<input type="radio" name="currency""#).count(), 2, "{form}");
    // A listing begun and left in this browser is offered back (draft_controller.js), on a new
    // listing's form alone.
    assert!(form.contains(r#"<div class="notice draft-note" data-draft-target="note" role="status" hidden>"#), "{form}");

    // Signed in, no box for the contact: the name and the contact are the account's.
    let ana = session_for(&pool, ANA_ID).await;
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    assert_eq!(form.matches(r#"<div class="form-box">"#).count(), 4, "{form}");
    assert!(!form.contains(r#"id="name""#) && !form.contains(r#"id="contact""#));
    // An edit: "Anulo" before its button, as wide as its word; no draft.
    let edit = get_with_cookie(pool.clone(), "/listing/1001/ndrysho", &ana).await.body;
    let cancel = edit.find(">Anulo</a>").expect("Anulo");
    assert!(cancel < edit.find(r#"<button type="submit" class="button">Ruaj ndryshimet</button>"#).expect("its button"), "{edit}");
    assert!(!edit.contains(r#"id="name""#) && !edit.contains("draft-note") && !edit.contains(" draft\""));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_ai_on_an_edit_sees_the_listings_own_photos(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let started = started();
    let posted = post_with_photos(pool.clone(), "/shto", &listing_fields(("", ""), &started), &[jpeg(800, 600), jpeg(600, 800)], &ana).await;
    let no = posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    let (url, asked) = ai_stand_in(Some(json!({"category": "telefona", "listing_type": "SELL"}))).await;
    let fill = |cookie: String| {
        let (pool, url, no) = (pool.clone(), url.clone(), no.clone());
        async move {
            let fields = [("title", "Samsung Galaxy S21, 128 GB"), ("description", "Telefon në gjendje shumë të mirë, me kuti."), ("listing", no.as_str())];
            let (content_type, body) = multipart(&fields, &[jpeg(1000, 500)]);
            let request = Request::post("/shto/ploteso")
                .header(header::CONTENT_TYPE, content_type)
                .header(header::COOKIE, cookie)
                .body(Body::from(body))
                .unwrap();
            send_to(app_with_ai(pool, &url), request).await.status
        }
    };
    // The photos the AI was asked about, each by its width and height, in order.
    let shown = |request: &Value| -> Vec<(u32, u32)> {
        use base64::Engine;
        request["messages"][0]["content"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|block| block["type"] == "image")
            .map(|block| {
                let photo = base64::engine::general_purpose::STANDARD.decode(block["source"]["data"].as_str().unwrap()).unwrap();
                let photo = image::load_from_memory(&photo).unwrap();
                (photo.width(), photo.height())
            })
            .collect()
    };

    // Its seller asking from its edit form: its two photos, the cover first, then the one picked.
    assert_eq!(fill(ana).await, StatusCode::OK);
    // Someone else naming it: the photo they picked alone.
    assert_eq!(fill(besa).await, StatusCode::OK);
    let asked = asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(shown(&asked[0].1), vec![(800, 600), (600, 800), (1000, 500)]);
    assert_eq!(shown(&asked[1].1), vec![(1000, 500)]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_listing_form_asks_for_the_writing_first_and_offers_ai_when_it_is_set_up(pool: PgPool) {
    // The title, the description and the photos first, then the category, the price and the town.
    let form = html(pool.clone(), "/shto").await;
    let order = positions(&form, &[r#"id="title""#, r#"id="description""#, r#"name="photos""#, r#"id="category""#, r#"id="price_type""#, r#"id="city""#]);
    assert!(order.is_sorted(), "{order:?}");
    // Without the AI, nothing offers it, and asking for it says to fill the form in by hand.
    assert!(!form.contains("Plotëso pjesën tjetër me AI"));
    let words = [("title", "Volkswagen Golf 7, 2016"), ("description", "Makina në gjendje shumë të mirë.")];
    let (status, answer) = ai_fill(app(pool.clone()), &words, &[], "198.51.100.10").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(answer["message"].as_str().unwrap().contains("Plotëso vetë"), "{answer}");

    // With it, a new listing's form has the button, between the photos and the category; the
    // edit form too, for what is missing (the_ai_on_an_edit_sees_the_listings_own_photos).
    let (url, _) = ai_stand_in(None).await;
    let new = send_to(app_with_ai(pool.clone(), &url), Request::get("/shto").body(Body::empty()).unwrap()).await.body;
    let order = positions(&new, &[r#"name="photos""#, "Plotëso pjesën tjetër me AI", r#"id="category""#]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(new.contains(r##"data-controller="listing-form narrow price-hint draft ai-fill" data-ai-fill-photos-outlet="#listing-photos""##));
    assert!(new.contains(r#"<div class="field photos-field" id="listing-photos" data-controller="photos""#));
    let session = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    let request = Request::get("/listing/1001/ndrysho").header(header::COOKIE, &session).body(Body::empty()).unwrap();
    let edit = send_to(app_with_ai(pool, &url), request).await.body;
    assert!(edit.contains(r#"id="category""#) && edit.contains("Plotëso pjesën tjetër me AI") && edit.contains(r#"data-ai-fill-listing-value="1001""#), "{edit}");
    assert!(edit.contains("AI plotëson detajet që mungojnë. Ato që ke vënë nuk i prek."));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn ai_fills_in_the_rest_of_the_form_from_the_writing_and_the_photos(pool: PgPool) {
    // The AI's answer, with what the form can't take mixed in: a colour and an option it doesn't
    // list, a flat's area, and a zone, which a car doesn't say.
    let (url, asked) = ai_stand_in(Some(json!({
        "category": "makina",
        "listing_type": "SELL",
        "condition": "USED",
        "properties": {
            "vehicle_make": "VOLKSWAGEN",
            "vehicle_model": "VOLKSWAGEN_GOLF",
            "vehicle_year": 2016,
            "vehicle_mileage": 148000,
            "vehicle_fuel": "DIESEL",
            "vehicle_transmission": "MANUAL",
            "vehicle_color": "TURQUOISE",
            "vehicle_features": ["ABS", "JETPACK"],
            "property_area": 85
        },
        "price": 9500,
        "currency": "EUR",
        "price_type": "NEGOTIABLE",
        "city": "tirane",
        "zone": "blloku"
    })))
    .await;
    let words = [
        ("title", "Volkswagen Golf 7, 2016, naftë"),
        ("description", "148 000 km, kambio manuale, me ABS. Çmimi 9 500 €, i diskutueshëm. Ndodhet në Tiranë."),
    ];
    let (status, filled) = ai_fill(app_with_ai(pool.clone(), &url), &words, &[jpeg(3000, 2000)], "198.51.100.20").await;
    assert_eq!(status, StatusCode::OK, "{filled}");
    assert_eq!(filled["fields"], json!({
        "category": "makina", "listing_type": "SELL", "condition": "USED",
        "price_type": "NEGOTIABLE", "price": "9500", "currency": "EUR", "city": "tirane"
    }));
    assert_eq!(filled["properties"], json!({
        "prop_vehicle_make": ["VOLKSWAGEN"], "prop_vehicle_model": ["VOLKSWAGEN_GOLF"],
        "prop_vehicle_year": ["2016"], "prop_vehicle_mileage": ["148000"],
        "prop_vehicle_fuel": ["DIESEL"], "prop_vehicle_transmission": ["MANUAL"],
        "prop_vehicle_features": ["ABS"]
    }));

    // What the AI was asked: with the key and the model, to call the form's tool, following
    // instructions that list the form's choices, about the photo, made smaller, and the writing.
    let asked = asked.lock().unwrap();
    let [(key, request)] = asked.as_slice() else { panic!("{} calls", asked.len()) };
    assert_eq!(key.as_deref(), Some("test-key"));
    assert_eq!(request["model"], "claude-haiku-4-5-20251001");
    assert_eq!(request["tool_choice"], json!({"type": "tool", "name": "fill_listing_form"}));
    let instructions = request["system"][0]["text"].as_str().unwrap();
    for choice in ["makina: Automjete › Makina [SELL, BUY, RENT] (condition)", "VOLKSWAGEN_GOLF Golf", "tirane Tiranë", "tirane: 21-dhjetori 21 Dhjetori"] {
        assert!(instructions.contains(choice), "{choice}");
    }
    let content = request["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content.len(), 2);
    assert_eq!(content[0]["source"]["media_type"], "image/jpeg");
    use base64::Engine;
    let photo = base64::engine::general_purpose::STANDARD.decode(content[0]["source"]["data"].as_str().unwrap()).unwrap();
    let photo = image::load_from_memory(&photo).unwrap();
    assert_eq!(photo.width().max(photo.height()), 1024);
    assert!(content[1]["text"].as_str().unwrap().starts_with("Titulli: Volkswagen Golf 7, 2016, naftë"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn ai_fill_takes_the_properties_put_beside_the_other_fields(pool: PgPool) {
    // Haiku's answer for some photos of a Golf: its properties beside the category rather than in
    // "properties", where they are asked for. The form got none of them.
    let (url, asked) = ai_stand_in(Some(json!({
        "category": "makina", "city": "tirane", "condition": "USED", "currency": "EUR", "listing_type": "SELL",
        "price": 8900, "price_type": "NEGOTIABLE", "vehicle_doors": "D4", "vehicle_engine": 1600,
        "vehicle_features": ["AIR_CONDITIONING", "PARKING_SENSORS"], "vehicle_fuel": "DIESEL",
        "vehicle_make": "VOLKSWAGEN", "vehicle_mileage": 142000, "vehicle_model": "VOLKSWAGEN_GOLF",
        "vehicle_no_accident": true, "vehicle_service_book": true, "vehicle_transmission": "MANUAL", "vehicle_year": 2016
    })))
    .await;
    let words = [("title", "Shes golf 7"), ("description", "Golf 7 1.6 TDI, viti 2016, 142,000 km të bëra. Manual, 5 porta.")];
    let (status, filled) = ai_fill(app_with_ai(pool, &url), &words, &[], "198.51.100.21").await;
    assert_eq!(status, StatusCode::OK, "{filled}");
    assert_eq!(filled["properties"], json!({
        "prop_vehicle_doors": ["D4"], "prop_vehicle_engine": ["1600"],
        "prop_vehicle_features": ["AIR_CONDITIONING", "PARKING_SENSORS"], "prop_vehicle_fuel": ["DIESEL"],
        "prop_vehicle_make": ["VOLKSWAGEN"], "prop_vehicle_mileage": ["142000"], "prop_vehicle_model": ["VOLKSWAGEN_GOLF"],
        "prop_vehicle_no_accident": ["1"], "prop_vehicle_service_book": ["1"], "prop_vehicle_transmission": ["MANUAL"],
        "prop_vehicle_year": ["2016"]
    }));
    // The tool takes nothing beside its fields, which steers the AI to put them where they go.
    let asked = asked.lock().unwrap();
    assert_eq!(asked[0].1["tools"][0]["input_schema"]["additionalProperties"], json!(false));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn ai_fill_wants_the_writing_first_has_limits_and_says_when_it_fails(pool: PgPool) {
    let (url, asked) = ai_stand_in(Some(json!({"category": "makina"}))).await;
    let words = [("title", "Volkswagen Golf 7, 2016"), ("description", "Makina në gjendje shumë të mirë.")];
    // Too little to go on: the AI isn't asked.
    let (status, answer) = ai_fill(app_with_ai(pool.clone(), &url), &[("title", "Golf"), ("description", "")], &[], "198.51.100.30").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(answer["message"].as_str().unwrap().starts_with("Shkruaj së pari titullin"), "{answer}");
    assert!(asked.lock().unwrap().is_empty());
    // Ten fills an hour from one network, then no more for a while; another network still may.
    for _ in 0..10 {
        let (status, answer) = ai_fill(app_with_ai(pool.clone(), &url), &words, &[], "198.51.100.31").await;
        assert_eq!((status, answer["fields"]["category"].as_str()), (StatusCode::OK, Some("makina")));
    }
    let (status, answer) = ai_fill(app_with_ai(pool.clone(), &url), &words, &[], "198.51.100.31").await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert!(answer["message"].as_str().unwrap().contains("brenda një ore"), "{answer}");
    assert_eq!(asked.lock().unwrap().len(), 10);
    assert_eq!(ai_fill(app_with_ai(pool.clone(), &url), &words, &[], "198.51.100.32").await.0, StatusCode::OK);
    // An AI that doesn't answer: the seller fills the form in themselves.
    let (down, _) = ai_stand_in(None).await;
    let (status, answer) = ai_fill(app_with_ai(pool, &down), &words, &[], "198.51.100.33").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(answer["message"].as_str().unwrap().contains("Plotëso vetë"), "{answer}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn electronics_subcategories_have_properties_of_their_own(pool: PgPool) {
    // A phone's form: phones' brands (Huawei, not Canon) in their own order, the models under
    // them, storage from 32 GB, RAM, battery, colour, features, a warranty and what comes with it;
    // no screen size.
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    for name in ["phone_model", "electronics_storage", "electronics_ram", "phone_battery", "electronics_color",
                 "electronics_features", "electronics_warranty", "electronics_included"] {
        assert!(phones.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(!phones.contains("prop_electronics_screen"));
    assert!(phones.contains(r#"<option value="HUAWEI">Huawei</option>"#));
    assert!(!phones.contains(r#"<option value="CANON">"#));
    let brands = positions(&phones, &[r#"<option value="APPLE">"#, r#"<option value="SAMSUNG">"#,
                                      r#"<option value="HUAWEI">"#, r#"<option value="OTHER">"#]);
    assert!(brands.is_sorted(), "{brands:?}");
    assert!(phones.contains(r#"<optgroup label="Apple" data-parent="APPLE">"#));
    assert!(phones.contains(r#"<option value="APPLE_IPHONE_13">iPhone 13</option>"#));
    assert!(phones.contains(r#"<option value="GB32">32 GB</option>"#) && !phones.contains(r#"value="TB2""#));
    // A number's unit inside its field at its end, and in its name for screen readers.
    assert!(phones.contains(r#"Bateria<span class="visually-hidden"> (%)</span></label>"#), "{phones}");
    assert!(phones.contains(r#"<span class="unit" aria-hidden="true">%</span>"#), "{phones}");

    // A TV's: its size as "Diagonalja", in inches, resolution, panel and Smart TV, TV brands, and
    // its remote among what comes with it; no storage, no pen.
    let tvs = get(pool.clone(), "/shto/fushat?category=televizore").await.body;
    assert!(tvs.contains(r#"Diagonalja<span class="visually-hidden"> (inç)</span></label>"#), "{tvs}");
    assert!(tvs.contains(r#"<span class="unit" aria-hidden="true">inç</span>"#), "{tvs}");
    for name in ["tv_resolution", "tv_panel", "tv_smart"] {
        assert!(tvs.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(!tvs.contains("prop_electronics_storage"));
    assert!(tvs.contains(r#"<option value="TCL">TCL</option>"#) && !tvs.contains(r#"<option value="APPLE">"#));
    assert!(tvs.contains(r#"value="REMOTE""#) && !tvs.contains(r#"value="PEN""#));

    // A laptop's: processor, RAM from 4 GB to 64 and more, disk, screen, graphics and system.
    let laptops = get(pool.clone(), "/shto/fushat?category=laptope").await.body;
    for name in ["computer_cpu", "electronics_ram", "computer_disk", "electronics_screen", "computer_gpu", "computer_os"] {
        assert!(laptops.contains(&format!(r#"name="prop_{name}""#)), "{name}");
    }
    assert!(laptops.contains(r#"value="GB64""#) && !laptops.contains(r#"value="GB3""#));

    // Posted with them, a phone says them on its page, and the filters find it by them.
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = listing_fields(("title", "iPhone 13, 128 GB"), &started);
    fields.extend_from_slice(&[
        ("prop_electronics_brand", "APPLE"),
        ("prop_phone_model", "APPLE_IPHONE_13"),
        ("prop_electronics_storage", "GB128"),
        ("prop_electronics_ram", "GB4"),
        ("prop_phone_battery", "89"),
        ("prop_electronics_color", "BLUE"),
        ("prop_electronics_features", "FIVE_G"),
        ("prop_electronics_features", "ESIM"),
        ("prop_electronics_warranty", "1"),
        ("prop_electronics_included", "BOX"),
    ]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let page = get(pool.clone(), &posted.location.unwrap()).await.body;
    // RAM's pill says "4 GB RAM", not "RAM 4 GB RAM".
    assert!(!page.contains(r#"<span class="attribute-label" aria-hidden="true">RAM</span>"#));
    for value in [">iPhone 13</dd>", "</svg>4 GB RAM</dd>", ">Blu</dd>", ">5G</dd>", ">eSIM</dd>",
                  ">Kutia origjinale</dd>", r#"title="Me garanci""#] {
        assert!(page.contains(value), "{value}");
    }
    assert!(page.contains("89\u{a0}%") || page.contains("89 %"), "the battery");
    for (uri, found) in [
        ("/c/telefona?electronics_ram=GB4", 1),
        ("/c/telefona?electronics_ram=GB8", 0),
        ("/c/telefona?electronics_brand=APPLE&phone_model=APPLE_IPHONE_13", 1),
        ("/c/telefona?phone_battery_min=85", 1),
        ("/c/telefona?phone_battery_min=90", 0),
        ("/c/telefona?electronics_features=ESIM", 1),
    ] {
        assert_eq!(card_numbers(&html(pool.clone(), uri).await).len(), found, "{uri}");
    }
    // The phones' filters offer phones' brands; a TV page filters by size, resolution and panel.
    let phone_page = html(pool.clone(), "/c/telefona").await;
    assert!(phone_page.contains(r#"value="HUAWEI""#) && !phone_page.contains(r#"value="CANON""#));
    let tv_page = html(pool.clone(), "/c/televizore").await;
    for name in ["electronics_screen_min", "tv_resolution", "tv_panel", "tv_smart"] {
        assert!(tv_page.contains(&format!(r#"name="{name}""#)), "{name}");
    }
    assert!(!tv_page.contains(r#"name="electronics_storage""#));

    // A brand phones don't offer is refused, as one that doesn't exist is.
    let mut canon = listing_fields(("title", "Telefon Canon"), &started);
    canon.push(("prop_electronics_brand", "CANON"));
    let refused = post_form(pool.clone(), "/shto", &canon, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh një nga mundësitë."));

    // A brand typed in for a phone, once approved, is offered for phones, before Tjetër, and not
    // for TVs.
    let mut typed = listing_fields(("title", "Fairphone 5, 256 GB"), &started);
    typed.extend_from_slice(&[("prop_electronics_brand", "__other__"), ("prop_electronics_brand_other", "Fairphone")]);
    let posted = post_form(pool.clone(), "/shto", &typed, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let moderator = new_seller(&pool, MODERATOR).await;
    let fairphone = option_id(&pool, "Fairphone").await;
    let approved = post_form(pool.clone(), &format!("/moderim/{fairphone}/mirato"), &[("label", "Fairphone")], Some(&moderator)).await;
    assert_eq!(approved.status, StatusCode::SEE_OTHER);
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    let order = positions(&phones, &[r#"<option value="NOKIA">"#, r#"<option value="FAIRPHONE">Fairphone</option>"#,
                                     r#"<option value="OTHER">"#]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!get(pool.clone(), "/shto/fushat?category=televizore").await.body.contains("Fairphone"));
}

/// The listing form's fields for a category: the frame it loads when the category changes.
async fn form_fields(pool: &PgPool, category: &str) -> String {
    get(pool.clone(), &format!("/shto/fushat?category={category}")).await.body
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn each_kind_of_thing_asks_for_what_fits_it(pool: PgPool) {
    let has = |fields: &str, name: &str| fields.contains(&format!(r#"name="prop_{name}""#));
    // A flat: its rooms and bathrooms, floor, condition, heating, and what a tenant asks. Central
    // heating is a kind of heating, not one of its amenities.
    let flat = form_fields(&pool, "apartamente").await;
    for name in ["property_rooms", "property_bathrooms", "property_floor", "property_condition", "property_heating", "rent_pets"] {
        assert!(has(&flat, name), "{name}");
    }
    assert!(flat.contains(r#"<option value="CENTRAL">Qendrore</option>"#));
    assert!(!flat.contains(r#"value="HEATING""#));
    // A plot: what the land is for, its permit and its road; no rooms, floor or furnishing.
    let plot = form_fields(&pool, "truall").await;
    for name in ["land_type", "land_permit", "land_road"] {
        assert!(has(&plot, name), "{name}");
    }
    for name in ["property_rooms", "property_floor", "property_furnished"] {
        assert!(!has(&plot, name), "{name}");
    }
    // Shoes: their size, EU 20 to 48, and shoes' brands; a jacket clothes' sizes and children's;
    // a bag no size.
    let shoes = form_fields(&pool, "kepuce").await;
    assert!(shoes.contains(r#"<option value="EU42">42</option>"#) && !has(&shoes, "fashion_size"));
    assert!(shoes.contains(r#"<option value="NIKE">Nike</option>"#) && !shoes.contains(r#"<option value="ROLEX">"#));
    let jackets = form_fields(&pool, "xhaketa-dhe-pallto").await;
    assert!(has(&jackets, "fashion_size") && has(&jackets, "kids_size"));
    let bags = form_fields(&pool, "canta").await;
    assert!(has(&bags, "material") && !has(&bags, "fashion_size") && !has(&bags, "shoe_size"));
    // A dog: its breed, of dogs' breeds, its age, sex and vaccines.
    let dogs = form_fields(&pool, "qen").await;
    for name in ["pet_breed", "pet_age", "pet_sex", "pet_vaccinated"] {
        assert!(has(&dogs, name), "{name}");
    }
    assert!(dogs.contains(r#"<option value="LABRADOR">Labrador</option>"#) && !dogs.contains(r#"value="PERSIAN""#));
    // A job: its contract, where it is done, the experience and languages it asks.
    let job = form_fields(&pool, "pune-it").await;
    for name in ["job_contract", "job_place", "job_experience", "job_languages"] {
        assert!(has(&job, name), "{name}");
    }
    // A motorcycle's form starts with its own make, with none of a car's tiles: its year after.
    let motorcycle = form_fields(&pool, "motocikleta").await;
    assert!(!motorcycle.contains("tile"));
    let order = positions(&motorcycle, &[r#"for="prop_moto_make""#, r#"for="prop_moto_type""#, r#"for="prop_model_name""#,
                                         r#"for="prop_vehicle_year""#]);
    assert!(order.is_sorted(), "{order:?}");
    // A car's: its main fields first, then its papers, engine and history, its options last; no
    // tiles with icons any more, each field its name over it as the rest of the form's.
    let car = form_fields(&pool, "makina").await;
    assert!(!car.contains("tile"), "{car}");
    let order = positions(&car, &[r#"for="prop_vehicle_color""#, r#"for="prop_vehicle_customs""#, r#"for="prop_vehicle_engine""#,
                                  r#"name="prop_vehicle_no_accident""#, r#"<span class="field-label">Opsionet</span>"#]);
    assert!(order.is_sorted(), "{order:?}");

    // Posted with them, a car says them on its page, and the filters find it by them.
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = car_fields(("", ""), &started);
    fields.extend_from_slice(&[
        ("prop_vehicle_customs", "CLEARED"),
        ("prop_vehicle_plates", "ALBANIAN"),
        ("prop_vehicle_engine", "1598"),
        ("prop_vehicle_power", "115"),
        ("prop_vehicle_no_accident", "1"),
        ("prop_vehicle_owners", "1"),
    ]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let page = get(pool.clone(), &posted.location.unwrap()).await.body;
    for value in [">E zhdoganuar</dd>", ">Shqiptare</dd>", "1\u{a0}598\u{a0}cm³</dd>", "115\u{a0}HP</dd>", "Pa aksident</dt>"] {
        assert!(page.contains(value), "{value}");
    }
    // On a car's page the details stand in a grid, each its name with its icon over its value; a
    // box ticked says its name, not "Po".
    let spec = |label: &str| {
        let end = page.find(&format!("{label}</dt>")).unwrap_or_else(|| panic!("{label}"));
        let start = page[..end].rfind("<dt>").unwrap();
        page[start..end + page[end..].find("</dd>").unwrap()].to_string()
    };
    for label in ["Dogana", "Targat", "Motorri", "Fuqia", "Pronarë", "Pa aksident"] {
        assert!(spec(label).starts_with(r#"<dt><svg class="spec-icon""#), "{label}: {}", spec(label));
    }
    assert!(spec("Dogana").ends_with("Dogana</dt><dd>E zhdoganuar"));
    assert!(spec("Pronarë").ends_with("Pronarë</dt><dd>1"));
    assert!(spec("Pa aksident").ends_with("Pa aksident</dt><dd>Pa aksident"));
    for (uri, found) in [
        ("/c/makina?vehicle_customs=CLEARED", 1),
        ("/c/makina?vehicle_customs=NOT_CLEARED", 0),
        ("/c/makina?vehicle_engine_min=1500&vehicle_engine_max=2000", 1),
        ("/c/makina?vehicle_power_min=150", 0),
        ("/c/makina?vehicle_no_accident=1", 1),
    ] {
        assert_eq!(card_numbers(&html(pool.clone(), uri).await).len(), found, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_site_is_called_pluribus(pool: PgPool) {
    // The logo is the name in lowercase, the word alone, in Clash Display drawn as shapes
    // (logo.svg): the letters in the text's colour, the i's dot apart in the accent with an edge
    // inside it, and the word for screen readers. The site's one typeface, Geist, comes with the
    // page, the only font that does: the headings' Inter and the logo's Playfair Display went with
    // their files. The pages' titles and the footer say the name.
    let home = html(pool.clone(), "/").await;
    let logo = &home[home.find(r#"<a class="logo" id="logo" href="/" data-turbo-permanent translate="no"><svg class="logo-mark" "#).expect("the logo")..];
    let logo = &logo[..logo.find("</a>").unwrap()];
    assert!(logo.contains(r#" viewBox="0 0 4126 1140" aria-hidden="true"><path fill="currentColor" d="M190 1060"#), "{logo}");
    assert!(logo.contains(r#"<rect class="logo-dot" x="2086" y="220" width="150" height="118"/>"#), "{logo}");
    assert!(logo.contains(r#"<rect class="logo-dot-edge" x="2097" y="231" width="128" height="96" fill="none" stroke-width="22"/></svg>"#), "{logo}");
    assert!(logo.ends_with(r#"</svg><span class="visually-hidden">pluribus</span>"#), "{logo}");
    // The dot takes the accent, and its edge the unread dots' edge, which only lime has, and only
    // on the light theme.
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    assert!(css.contains(".logo-dot{fill:var(--highlight);") && css.contains(".logo-dot-edge{stroke:var(--logo-edge);"));
    assert!(css.contains("--logo-edge:rgb(27 27 27 / 45%);") && css.contains("--dot-edge:inset 0 0 0 1.5px rgb(27 27 27 / 45%);"));
    assert!(!home.contains("thinking"));
    assert!(home.contains(r#"<title>Pluribus</title>"#) && home.contains("© 2026 Pluribus"));
    assert!(home.contains(
        r#"<link rel="preload" href="/static/vendor/fonts/geist-v5-latin.woff2" as="font" type="font/woff2" crossorigin>"#
    ));
    assert_eq!(home.matches(r#"<link rel="preload" href="/static/vendor/fonts/"#).count(), 1);
    for gone in ["inter-5.3.0-latin-wght.woff2", "playfair-display-5.3.0-latin-600.woff2"] {
        assert!(!home.contains(gone), "{gone}");
        assert_eq!(get(pool.clone(), &format!("/static/vendor/fonts/{gone}")).await.status, StatusCode::NOT_FOUND, "{gone}");
    }
    let listing = html(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert!(listing.contains(" | Pluribus</title>") && !listing.contains("Marketplace"), "{listing}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn unknown_pages_are_404(pool: PgPool) {
    for uri in ["/listing/999999-nothing", "/c/does-not-exist", "/nowhere"] {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_footer_leads_to_the_pages_about_the_site(pool: PgPool) {
    // Every page's footer links the five; each is a page of its own, with its description and
    // canonical address for search engines, in the pages' sitemap, and open to crawlers.
    let home = html(pool.clone(), "/").await;
    let pages = [
        ("/si-funksionon", "Si funksionon", "Si funksionon Pluribus"),
        ("/siguria", "Këshilla sigurie", "Këshilla sigurie"),
        ("/kushtet", "Kushtet", "Kushtet e përdorimit"),
        ("/privatesia", "Privatësia", "Privatësia"),
        ("/kontakt", "Kontakt", "Kontakt"),
    ];
    for (path, link, heading) in pages {
        assert!(home.contains(&format!(r#"<a href="{path}">{link}</a>"#)), "{path}");
        let page = get(pool.clone(), path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert!(page.body.contains(&format!("<h1>{heading}</h1>")), "{path}");
        assert!(page.body.contains(r#"<meta name="description" content=""#), "{path}");
        assert!(page.body.contains(&format!(r#"<link rel="canonical" href="http://127.0.0.1:3000{path}">"#)), "{path}");
        assert!(!page.body.contains("noindex"), "{path}");
    }
    let sitemap = get(pool.clone(), "/sitemaps/faqet.xml").await.body;
    for (path, ..) in pages {
        assert!(sitemap.contains(&format!("<loc>http://127.0.0.1:3000{path}</loc>")), "{path}");
    }
    let robots = get(pool, "/robots.txt").await.body;
    assert!(!pages.iter().any(|(path, ..)| robots.contains(&format!("Disallow: {path}"))));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_privacy_page_says_what_the_law_asks_and_the_site_keeps_to_it(pool: PgPool) {
    // Who is responsible, on what ground each use rests, who else gets the data and where, for how
    // long, and the rights, a complaint to the Commissioner among them.
    let privacy = html(pool.clone(), "/privatesia").await;
    for said in [
        "<h2>Kush i mban</h2>",
        "<h2>Pse, dhe mbi çfarë baze</h2>",
        "Faqja shërbehet përmes Cloudflare",
        "jashtë Shqipërisë, kryesisht në Shtetet e Bashkuara",
        "fshihet pas një dite",
        "Të përgjigjemi brenda 30 ditëve.",
        r#"(<a href="https://www.idp.al" rel="noopener">idp.al</a>)"#,
        "duhet të jesh të paktën 18 vjeç",
    ] {
        assert!(privacy.contains(said), "{said}");
    }
    // Where people hand over what it keeps, a line says the terms they accept and where the
    // privacy page is: signing in and a new listing, not an edit.
    let note = r#"pranon <a href="/kushtet">kushtet e përdorimit</a>. Si i mbajmë të dhënat e tua: <a href="/privatesia">Privatësia</a>."#;
    assert!(html(pool.clone(), "/llogaria").await.contains(note));
    assert!(html(pool.clone(), "/shto").await.contains(note));

    // The sign-in codes asked for over a day ago go, with the contacts and addresses they were
    // asked from; those of today stay, for the limits.
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at, created_at)
         VALUES ('+355690000101', '\\x00', '203.0.113.9', now() - interval '47 hours', now() - interval '2 days'),
                ('+355690000102', '\\x00', '203.0.113.9', now() + interval '10 minutes', now())",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(marketplace::db::forget_old_codes(&pool).await.unwrap(), 1);
    let left: Vec<String> = sqlx::query_scalar("SELECT contact FROM login_codes").fetch_all(&pool).await.unwrap();
    assert_eq!(left, ["+355690000102"]);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn next_page_loads_as_a_turbo_frame(pool: PgPool) {
    sqlx::query(
        "INSERT INTO listings (seller_user_id, created_by_user_id, category_id, slug, title,
                               description, price_amount, currency, status, published_at)
         SELECT '11111111-1111-4111-8111-111111111111', '11111111-1111-4111-8111-111111111111',
                '10000000-0000-0000-0000-000000000011', 'njoftim-' || g, 'Njoftim shtesë ' || g,
                '', 100, 'ALL', 'ACTIVE', now() - g * interval '1 day'
         FROM generate_series(1, 30) g",
    )
    .execute(&pool)
    .await
    .unwrap();

    let first = html(pool.clone(), "/").await;
    let (frame_id, src) = next_frame(&first).expect("a next-page frame");
    assert!(src.starts_with("/?after="), "{src}");

    let request = Request::get(&src)
        .header("turbo-frame", &frame_id)
        .body(Body::empty())
        .unwrap();
    let next = send(pool.clone(), request).await;
    assert_eq!(next.status, StatusCode::OK);
    assert!(next.body.starts_with(&format!(r#"<turbo-frame id="{frame_id}""#)));
    assert!(!next.body.contains("<html"));
    // 42 listings: 24 on the first page, the remaining 18 here, and no further page.
    assert_eq!(next.body.matches(r#"class="card""#).count(), 18);
    assert!(!next.body.contains("Shfaq më shumë"));

    // The same address, asked for without the frame (a search engine, a link opened anew), is the
    // whole page: both answers say they differ with Turbo-Frame, so that a cache keeps them apart.
    let vary = |response: &TestResponse| {
        response.headers.get_all(header::VARY).iter().map(|value| value.to_str().unwrap().to_ascii_lowercase()).collect::<Vec<_>>()
    };
    assert!(vary(&next).iter().any(|value| value.contains("turbo-frame")), "{:?}", vary(&next));
    let whole = get(pool, &src).await;
    assert_eq!(whole.status, StatusCode::OK);
    assert!(whole.body.contains("<html") && whole.body.contains(r#"<link rel="canonical""#));
    assert!(vary(&whole).iter().any(|value| value.contains("turbo-frame")), "{:?}", vary(&whole));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn static_files_have_cache_headers(pool: PgPool) {
    let cache_control = |uri: &'static str| {
        let pool = pool.clone();
        async move {
            let response = app(pool)
                .oneshot(Request::get(uri).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{uri}");
            response.headers()[header::CACHE_CONTROL].to_str().unwrap().to_string()
        }
    };
    assert_eq!(
        cache_control("/static/vendor/turbo-8.0.23.js").await,
        "public, max-age=31536000, immutable"
    );
    assert_eq!(cache_control("/static/css/base.css").await, "no-cache");
    // The site's own files, with the fingerprint the pages give them, are kept for good: their
    // address changes when they do. With another, from a page loaded before a deploy, they aren't.
    let version = marketplace::web::asset_version();
    let kept = "public, max-age=31536000, immutable";
    for (uri, policy) in [
        (format!("/static/css/base.css?v={version}"), kept),
        (format!("/static/css/site.css?v={version}"), kept),
        (format!("/static/js/controllers/heart_controller.js?v={version}"), kept),
        ("/static/css/base.css?v=00000000".to_string(), "no-cache"),
        ("/static/css/site.css".to_string(), "no-cache"),
    ] {
        let response = app(pool.clone()).oneshot(Request::get(uri.as_str()).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], policy, "{uri}");
    }
    assert_eq!(
        cache_control("/static/vendor/leaflet-1.9.4/leaflet.js").await,
        "public, max-age=31536000, immutable"
    );

    let uri = "/static/vendor/fonts/geist-v5-latin.woff2";
    let font = app(pool)
        .oneshot(
            Request::get(uri)
                .header(header::ACCEPT_ENCODING, "br, gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(font.status(), StatusCode::OK, "{uri}");
    assert_eq!(font.headers()[header::CONTENT_TYPE], "font/woff2");
    assert_eq!(font.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert!(!font.headers().contains_key(header::CONTENT_ENCODING), "WOFF2 is compressed already");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn scripts_come_minified(pool: PgPool) {
    // Every script a page loads comes minified (web::script): Turbo and Stimulus, kept as written,
    // in a fraction of their bytes, with their licence notices; the site's own modules without
    // their comments, with their imports and exports as they were, so the import map and Stimulus
    // find what they look for.
    let version = marketplace::web::asset_version();
    let written = |file: &str| std::fs::read_to_string(file).unwrap();
    let turbo = get(pool.clone(), "/static/vendor/turbo-8.0.23.js").await;
    assert_eq!((turbo.status, turbo.content_type.as_deref()), (StatusCode::OK, Some("text/javascript; charset=utf-8")));
    let source = written("static/vendor/turbo-8.0.23.js");
    assert!(turbo.body.len() < source.len() / 2, "{} of {}", turbo.body.len(), source.len());
    assert!(turbo.body.starts_with("/*!\nTurbo 8.0.23\n"), "{}", &turbo.body[..40]);
    assert!(turbo.body.contains("FrameElement") && turbo.body.contains("customElements.define("), "{}", &turbo.body[..200]);
    let stimulus = get(pool.clone(), "/static/vendor/stimulus-3.2.2.js").await;
    let source = written("static/vendor/stimulus-3.2.2.js");
    assert!(stimulus.body.len() < source.len() * 3 / 5, "{} of {}", stimulus.body.len(), source.len());
    assert!(stimulus.body.starts_with("/*\nStimulus 3.2.1\n"), "{}", &stimulus.body[..40]);
    assert!(stimulus.body.contains("Controller"), "{}", &stimulus.body[..200]);
    let application = get(pool.clone(), &format!("/static/js/application.js?v={version}")).await;
    assert_eq!(application.content_type.as_deref(), Some("text/javascript; charset=utf-8"));
    let source = written("static/js/application.js");
    assert!(application.body.len() < source.len() * 2 / 3, "{} of {}", application.body.len(), source.len());
    assert!(!application.body.contains("Turbo turns links"), "{}", application.body);
    for kept in ["from\"@hotwired/turbo\"", "from\"@hotwired/stimulus\"", "\"controllers/heart_controller.js\"", "_controller.js`"] {
        assert!(application.body.contains(kept), "{kept} in {}", application.body);
    }
    let heart = get(pool.clone(), &format!("/static/js/controllers/heart_controller.js?v={version}")).await;
    assert!(heart.body.contains("export default class extends ") && heart.body.contains("static values={number:Number}"), "{}", heart.body);
    assert!(heart.body.contains("from\"controllers/signed_in.js\"") && heart.body.contains("connect(){"), "{}", heart.body);
    // Not a file above static/js, nor any but a script.
    for uri in ["/static/js/nope.js", "/static/js/../../static/js/application.js", "/static/js/application.js.map", "/static/js/../vendor/turbo-8.0.23.js"] {
        assert_eq!(get(pool.clone(), uri).await.status, StatusCode::NOT_FOUND, "{uri}");
    }
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn open_pages_reload_in_full_when_the_css_or_javascript_changes(pool: PgPool) {
    let version = marketplace::web::asset_version();
    assert_eq!(version.len(), 8);
    let body = get(pool.clone(), "/").await.body;
    // One stylesheet, the site's eight in their order, which the cascade follows, minified: no
    // comments, no lines.
    assert_eq!(body.matches(r#"<link rel="stylesheet""#).count(), 1);
    assert!(body.contains(&format!(r#"<link rel="stylesheet" href="/static/css/site.css?v={version}" data-turbo-track="reload">"#)));
    // Named in a Link header too, to preload: Cloudflare sends it as Early Hints, before the page.
    let page = get(pool.clone(), "/").await;
    assert_eq!(
        page.headers.get("link").and_then(|value| value.to_str().ok()),
        Some(format!("</static/css/site.css?v={version}>; rel=preload; as=style").as_str())
    );
    let site = get(pool.clone(), &format!("/static/css/site.css?v={version}")).await;
    assert_eq!((site.status, site.content_type.as_deref()), (StatusCode::OK, Some("text/css; charset=utf-8")));
    assert!(!site.body.contains("/*") && !site.body.contains('\n'));
    // Each file's first rule, by its selector, after the one before's.
    let mut at = 0;
    for sheet in marketplace::web::STYLESHEETS {
        let css = std::fs::read_to_string(format!("static/css/{sheet}.css")).unwrap();
        let first = css.lines().find(|line| line.contains('{') && !line.starts_with(['/', '*', ' '])).unwrap();
        let selector = format!("{}{{", first.split('{').next().unwrap().trim());
        at += site.body[at..].find(&selector).unwrap_or_else(|| panic!("{sheet}: {selector}"));
    }
    assert!(body.contains(&format!(
        r#"<script type="module" src="/static/js/application.js?v={version}" data-turbo-track="reload"></script>"#
    )));
    // Every module at its address with the fingerprint, which browsers keep; those the page uses
    // preloaded at once, rather than application.js first and what it imports after: application.js
    // and what it imports, and the controllers the page names, with theirs. No others.
    let modules = marketplace::web::js_modules();
    assert_eq!(modules[0], "application.js");
    for module in modules.iter().filter(|module| module.starts_with("controllers/")) {
        assert!(body.contains(&format!(r#""{module}": "/static/js/{module}?v={version}""#)), "{module}");
    }
    let preloads = |body: &str| -> Vec<String> {
        body.split(r#"<link rel="modulepreload" href="/static/js/"#)
            .skip(1)
            .map(|rest| rest.split('?').next().unwrap().to_string())
            .collect()
    };
    let home = preloads(&body);
    for used in [
        "application.js",
        "controllers/heart_controller.js",
        "controllers/signed_in.js",
        "controllers/place_controller.js",
        "controllers/refresh.js",
        "controllers/inbox_controller.js",
        "controllers/theme_controller.js",
    ] {
        assert!(home.iter().any(|module| module == used), "{used} in {home:?}");
    }
    // Nor, with no listing seen yet and so no row of the most seen, the row's.
    for unused in ["controllers/map_controller.js", "controllers/leaflet.js", "controllers/filters_controller.js", "controllers/drag_row_controller.js"] {
        assert!(!home.iter().any(|module| module == unused), "{unused} in {home:?}");
    }
    assert_eq!(home.len(), home.iter().collect::<std::collections::HashSet<_>>().len(), "{home:?}");
    let form = preloads(&get(pool.clone(), "/shto").await.body);
    for used in ["controllers/listing_form_controller.js", "controllers/photos_controller.js", "controllers/picker_controller.js"] {
        assert!(form.iter().any(|module| module == used), "{used} in {form:?}");
    }
    assert!(body.contains(r#"<link rel="modulepreload" href="/static/vendor/turbo-8.0.23.js">"#));
    // The fingerprint only changes the URL; the file is served as before.
    let css = get(pool, &format!("/static/css/browse.css?v={version}")).await;
    assert_eq!(css.status, StatusCode::OK);
    assert!(css.body.contains(".browse-heading"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_site_has_its_icon(pool: PgPool) {
    let body = get(pool.clone(), "/").await.body;
    for link in [
        r#"<link rel="icon" href="/favicon.ico" sizes="32x32">"#,
        r#"<link rel="icon" href="/static/img/icon.svg" type="image/svg+xml">"#,
        r#"<link rel="apple-touch-icon" href="/static/img/apple-touch-icon.png">"#,
        r#"<link rel="manifest" href="/static/site.webmanifest">"#,
    ] {
        assert!(body.contains(link), "{link}");
    }
    // Where browsers look without asking the page.
    let favicon = get(pool.clone(), "/favicon.ico").await;
    assert_eq!(favicon.status, StatusCode::OK);
    assert_eq!(favicon.content_type.as_deref(), Some("image/x-icon"));
    assert_eq!(favicon.headers[header::CACHE_CONTROL], "no-cache");
    // The manifest's icons are there.
    let manifest = get(pool.clone(), "/static/site.webmanifest").await;
    assert_eq!(manifest.status, StatusCode::OK);
    let manifest: Value = serde_json::from_str(&manifest.body).unwrap();
    for icon in manifest["icons"].as_array().unwrap() {
        let src = icon["src"].as_str().unwrap();
        let response = get(pool.clone(), src).await;
        assert_eq!(response.status, StatusCode::OK, "{src}");
        assert_eq!(response.content_type.as_deref(), Some("image/png"), "{src}");
    }
    for icon in ["/static/img/icon.svg", "/static/img/apple-touch-icon.png"] {
        assert_eq!(get(pool.clone(), icon).await.status, StatusCode::OK, "{icon}");
    }
    // The icon is the logo's p, light on an eerie black tile, with the i's square in the accent
    // there is unless another is picked, mint (lime until 2026-10-11).
    let svg = get(pool.clone(), "/static/img/icon.svg").await.body;
    assert!(svg.contains(r##"rx="264" fill="#1b1b1b"/>"##), "{svg}");
    assert!(svg.contains(r##"<path fill="#f7f7f7" d="M190 1060L40 1060"##), "{svg}");
    assert!(svg.contains(r##"<rect x="479" y="220" width="150" height="118" fill="#2de38c"/>"##), "{svg}");
    assert!(!svg.contains("#c8ff00"), "{svg}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn responses_tell_browsers_how_to_keep_the_site_safe(pool: PgPool) {
    use base64::Engine;
    use sha2::Digest;

    for uri in [
        "/",
        "/listing/1001-volkswagen-golf-7-2016-nafte",
        "/shto",
        "/nuk-ka-faqe",
        "/static/css/base.css",
        "/favicon.ico",
    ] {
        let response = get(pool.clone(), uri).await;
        let headers = &response.headers;
        assert_eq!(headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff", "{uri}");
        assert_eq!(headers[header::X_FRAME_OPTIONS], "DENY", "{uri}");
        assert_eq!(headers[header::REFERRER_POLICY], "strict-origin-when-cross-origin", "{uri}");
        assert_eq!(headers["cross-origin-opener-policy"], "same-origin", "{uri}");
        let permissions = headers["permissions-policy"].to_str().unwrap();
        assert!(permissions.contains("camera=()") && permissions.contains("geolocation=(self)"), "{uri}");
        let policy = headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        for directive in ["default-src 'self'", "object-src 'none'", "frame-ancestors 'none'", "form-action 'self'"] {
            assert!(policy.contains(directive), "{uri}: {directive}");
        }
        if !response.content_type.as_deref().unwrap_or_default().starts_with("text/html") {
            continue;
        }
        // Every script in the page itself runs by its hash; scripts' files come from the site. JSON
        // for search engines isn't run, and needs none.
        let script_src = policy.split(';').find(|directive| directive.trim().starts_with("script-src")).unwrap();
        assert!(!script_src.contains("unsafe-inline") && !script_src.contains("unsafe-eval"), "{script_src}");
        let mut inline = 0;
        for script in response.body.split("<script").skip(1) {
            let (attributes, rest) = script.split_once('>').unwrap();
            if attributes.contains("src=") || attributes.contains("application/ld+json") {
                continue;
            }
            let text = rest.split("</script>").next().unwrap();
            let hash = base64::engine::general_purpose::STANDARD.encode(sha2::Sha256::digest(text));
            assert!(script_src.contains(&format!("'sha256-{hash}'")), "{uri}: <script{attributes}> isn't allowed");
            inline += 1;
        }
        assert_eq!(inline, 2, "{uri}: the theme and the import map");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn revealing_a_phone_returns_a_turbo_stream_and_counts_it(pool: PgPool) {
    let request = Request::post("/listing/1001/phone").body(Body::empty()).unwrap();
    let response = send(pool.clone(), request).await;
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(
        response.content_type.as_deref(),
        Some("text/vnd.turbo-stream.html; charset=utf-8")
    );
    assert!(response.body.contains(r#"<turbo-stream action="replace" target="phone-reveal">"#));
    assert!(response.body.contains(r#"href="tel:+355690000001" translate="no"><svg class="contact-icon""#));
    assert!(response.body.contains("</svg>+355 69 000 0001</a>"));
    // Under the price alone: the seller's card at the foot has no buttons of its own.
    assert_eq!(response.body.matches("<turbo-stream ").count(), 1);
    // Beside it, a button that copies it, as a phone dials it.
    assert!(response.body.contains(r#"data-controller="copy" data-copy-text-value="+355690000001""#));
    assert!(response.body.contains(r#"aria-label="Kopjo numrin""#));
    // Then a circle that opens a WhatsApp chat with it, in a new tab or the app, a first message
    // naming the listing and its address typed in, percent-encoded (a space %20, not "+"). The
    // number was in no page until the reveal, which counted it.
    let whatsapp = r#"<a class="button secondary icon-circle whatsapp-button" href="https://wa.me/355690000001?text=P%C3%ABrsh%C3%ABndetje%21%20Ju%20shkruaj%20p%C3%ABr%20njoftimin%20%E2%80%9E"#;
    assert!(response.body.contains(whatsapp), "{}", response.body);
    assert!(response.body.contains(r#"%E2%80%9C%20n%C3%AB%20Pluribus%3A%20http%3A%2F%2F"#), "{}", response.body);
    assert!(response.body.contains(r#"%2Flisting%2F1001-volkswagen-golf-7-2016-nafte" target="_blank" rel="noopener" aria-label="Shkruaj në WhatsApp" title="Shkruaj në WhatsApp">"#), "{}", response.body);
    assert!(response.body.find("copy-button").unwrap() < response.body.find("whatsapp-button").unwrap());

    let reveals: i64 = sqlx::query_scalar("SELECT phone_reveals FROM listing_daily_stats")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reveals, 1);

    // An Albanian landline (+355 4..., Tirana's) has no WhatsApp: the number and its copy alone.
    sqlx::query(
        "UPDATE listing_contacts SET phone_e164 = '+35542234567'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1003)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = Request::post("/listing/1003/phone").body(Body::empty()).unwrap();
    let landline = send(pool, request).await.body;
    assert!(landline.contains(r#"href="tel:+35542234567""#) && landline.contains("copy-button"), "{landline}");
    assert!(!landline.contains("whatsapp"), "{landline}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_listing_offers_its_sellers_phone(pool: PgPool) {
    let listings: Vec<(i64, String)> =
        sqlx::query_as("SELECT public_no, slug FROM listings ORDER BY public_no")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(listings.len(), 12);
    for (public_no, slug) in &listings {
        let page = get(pool.clone(), &format!("/listing/{public_no}-{slug}")).await.body;
        let button = format!(r#"<form id="phone-reveal" method="post" action="/listing/{public_no}/phone">"#);
        assert!(page.contains(&button), "listing {public_no} has no phone button");
    }
    // A business's listing gives the business's number.
    let request = Request::post("/listing/1002/phone").body(Body::empty()).unwrap();
    assert!(send(pool.clone(), request).await.body.contains("+355 69 000 0004"));

    // No button where the seller hides the number, or once the listing is sold.
    sqlx::query(
        "UPDATE listing_contacts SET show_phone = false
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1003)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001")
        .execute(&pool)
        .await
        .unwrap();
    for uri in ["/listing/1003-apartament-2-1-me-qira-ne-tirane", "/listing/1001-volkswagen-golf-7-2016-nafte"] {
        let page = get(pool.clone(), uri).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(!page.body.contains("phone-reveal"), "{uri} still offers the phone");
    }
    let request = Request::post("/listing/1001/phone").body(Body::empty()).unwrap();
    assert_eq!(send(pool, request).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn signing_in_takes_a_code_sent_to_the_phone(pool: PgPool) {
    // Not signed in, "Llogaria" asks for a name and a phone number or an e-mail address, and
    // says it signs in as well as makes an account.
    let page = get(pool.clone(), "/llogaria").await;
    assert!(page.body.contains("<h1>Hyr ose krijo një llogari</h1>"));
    // The tab's title says what the heading does ("një" left out once).
    assert!(page.body.contains("<title>Hyr ose krijo një llogari | Pluribus</title>"));
    assert!(page.body.contains(r#"<form class="account-form" action="/hyr" method="post">"#));

    let sent = post_form(
        pool.clone(),
        "/hyr",
        &[("name", "  Erion   Hoxha "), ("contact", "069 123 4567"), ("started", &started())],
        None,
    )
    .await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER);
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}", sent.cookie("login").unwrap());
    // The code itself isn't stored, only its hash; give it a known value to type in.
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();

    let page = get_with_cookie(pool.clone(), "/hyr/kodi", &login).await;
    assert!(page.body.contains("<strong>+355 69 *** **67</strong>"));
    // The code in six boxes, as the canvas has them, over the one input it goes into, which
    // stays the field the browser fills in and the form sends (code_boxes_controller.js).
    assert!(page.body.contains(r#"<div class="code-boxes" data-controller="code-boxes">"#), "{}", page.body);
    assert_eq!(page.body.matches(r#"<span class="code-box" data-code-boxes-target="box"></span>"#).count(), 6);
    assert!(page.body.contains(r#"autocomplete="one-time-code""#) && page.body.contains(r#"class="code-input" autofocus"#));

    let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "000000")], Some(&login)).await;
    assert_eq!(wrong.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(wrong.body.contains("Kodi nuk është i saktë. Të kanë mbetur 4 prova."));

    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123 456")], Some(&login)).await;
    assert_eq!(signed.status, StatusCode::SEE_OTHER);
    assert_eq!(signed.location.as_deref(), Some("/njoftimet-e-mia"));
    let session = format!("session={}", signed.cookie("session").unwrap());
    assert_eq!(signed.cookie("login"), Some(""), "the pending code's cookie is cleared");

    // Signed in: the form for the listing, with the contact buyers will get.
    let form = get_with_cookie(pool.clone(), "/shto", &session).await.body;
    assert!(form.contains("Publiko njoftimin"));
    assert!(form.contains(r#"<strong id="poster-contact">+355 69 *** **67</strong>"#));
    let (name, status): (String, String) =
        sqlx::query_as("SELECT display_name, status FROM users WHERE phone_e164 = '+355691234567'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), status.as_str()), ("Erion Hoxha", "ACTIVE"));
    // Signed in, "Llogaria" leads to one's own listings.
    let account = get_with_cookie(pool.clone(), "/llogaria", &session).await;
    assert_eq!(account.location.as_deref(), Some("/njoftimet-e-mia"));

    // A used code doesn't work twice, and signing out ends the session.
    let again = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    let out = post_form(pool.clone(), "/dil", &[], Some(&session)).await;
    assert_eq!(out.location.as_deref(), Some("/"));
    assert!(get_with_cookie(pool, "/llogaria", &session).await.body.contains("Hyr ose krijo një llogari"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sign_in_codes_are_limited(pool: PgPool) {
    let codes = |pool: PgPool| async move {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM login_codes").fetch_one(&pool).await.unwrap()
    };
    // Bots: the hidden field filled in, or the form sent the moment it was shown.
    let now = time::OffsetDateTime::now_utc().unix_timestamp().to_string();
    let trapped =
        post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("website", "x"), ("started", &started())], None)
            .await;
    let hasty = post_form(pool.clone(), "/hyr", &[("contact", "0691234567"), ("started", &now)], None).await;
    assert_eq!(trapped.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(hasty.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(codes(pool.clone()).await, 0);

    // Something that is neither a mobile number nor an e-mail address.
    let landline = post_form(pool.clone(), "/hyr", &[("contact", "04 222 3333"), ("started", &started())], None).await;
    assert!(landline.body.contains("Shkruaj një numër celulari"));

    // One code a minute for a contact.
    let first = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    assert_eq!(first.status, StatusCode::SEE_OTHER);
    let second = post_form(pool.clone(), "/hyr", &[("contact", "Ana@Example.com"), ("started", &started())], None).await;
    assert!(second.body.contains("Prit një minutë"));

    // Twenty an hour from one network.
    sqlx::query(
        "INSERT INTO login_codes (contact, code_hash, client_ip, expires_at)
         SELECT 'n' || g || '@example.com', decode('00', 'hex'), '203.0.113.7', now() + interval '10 minutes'
         FROM generate_series(1, 20) g",
    )
    .execute(&pool)
    .await
    .unwrap();
    let request = Request::post("/hyr")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::from(format!("contact=besa%40example.com&started={}", started())))
        .unwrap();
    assert!(send(pool.clone(), request).await.body.contains("Shumë kërkesa"));

    // Five wrong guesses use a code up.
    sqlx::query("UPDATE login_codes SET created_at = now() - interval '2 minutes' WHERE contact = 'ana@example.com'")
        .execute(&pool)
        .await
        .unwrap();
    let sent = post_form(pool.clone(), "/hyr", &[("contact", "ana@example.com"), ("started", &started())], None).await;
    let login = format!("login={}", sent.cookie("login").unwrap());
    for _ in 0..4 {
        let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
        assert!(wrong.body.contains("nuk është i saktë"));
    }
    let last = post_form(pool.clone(), "/hyr/kodi", &[("code", "999999")], Some(&login)).await;
    assert!(last.body.contains("Kërko një kod të ri"));
}

/// A complete listing form, with one field replaced.
fn listing_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = vec![
        ("category", "telefona"),
        ("listing_type", "SELL"),
        ("condition", "USED"),
        ("title", "Samsung Galaxy S21, 128 GB"),
        ("description", "Telefon në gjendje shumë të mirë, me kuti dhe karikues."),
        ("price_type", "FIXED"),
        ("price", "32 000"),
        ("currency", "ALL"),
        ("city", "tirane"),
        ("name", "Ana"),
        ("website", ""),
        ("started", started),
    ];
    for field in &mut fields {
        if field.0 == replace.0 {
            field.1 = replace.1;
        }
    }
    fields
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_signed_in_seller_posts_a_listing(pool: PgPool) {
    let started = started();
    // Not signed in, a listing needs a phone number or an e-mail address (see
    // a_listing_posted_without_signing_in_goes_live_at_once).
    let anonymous = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), None).await;
    assert_eq!(anonymous.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(anonymous.body.contains("Shkruaj një numër celulari"));

    // The fixture sellers have posted more than five listings today already.
    let ana = new_seller(&pool, "+355691112233").await;
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    assert!(url.ends_with("-samsung-galaxy-s21-128-gb"), "{url}");

    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains("<h1>Samsung Galaxy S21, 128 GB</h1>"));
    assert!(page.contains("32\u{a0}000\u{a0}Lekë"));
    // A wide pill that says its name, in the contact bar under the price.
    assert!(page.contains(r#"<aside class="contact-bar" aria-label="Kontakti" data-controller="sticky">"#));
    assert!(page.contains(r#"<button type="submit" class="button secondary reveal-button"><svg class="contact-icon""#));
    assert!(page.contains(r#"</svg><span class="button-label">Shfaq numrin</span></button>"#));
    let public_no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let reveal = post_form(pool.clone(), &format!("/listing/{public_no}/phone"), &[], None).await;
    assert!(reveal.body.contains("+355 69 111 2233"), "the seller's own number");

    // Published at once, in search, and on the map at Tirana's point.
    assert!(get(pool.clone(), "/search?q=galaxy").await.body.contains("Samsung Galaxy S21"));
    let (status, on_map): (String, bool) = sqlx::query_as(
        "SELECT l.status, l.latitude = loc.latitude FROM listings l JOIN locations loc ON loc.id = l.location_id
         WHERE l.title = 'Samsung Galaxy S21, 128 GB'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status.as_str(), on_map), ("ACTIVE", true));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listing_forms_are_checked(pool: PgPool) {
    let started = started();
    // The fixture sellers have posted more than five listings today already.
    let ana = new_seller(&pool, "+355691112233").await;
    let post = |field: (&str, &str)| {
        let fields: Vec<(String, String)> =
            listing_fields(field, &started).into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let (pool, ana) = (pool.clone(), ana.clone());
        async move {
            let fields: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            post_form(pool, "/shto", &fields, Some(&ana)).await
        }
    };
    for (field, message) in [
        (("price", ""), "Shkruaj çmimin"),
        (("title", "Tel"), "Titulli duhet të ketë 5 deri në 120 shkronja."),
        (("title", "Shiko www.spam.example"), "Titulli nuk mund të ketë lidhje."),
        (("listing_type", "RENT"), "Zgjidh llojin e njoftimit."),
        (("city", "atlantis"), "Zgjidh qytetin."),
        (("website", "http://bot.example"), "Prit një çast"),
        (("description", "Bli https://a.example https://b.example https://c.example"), "deri në 2 lidhje"),
    ] {
        let response = post(field).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{field:?}");
        assert!(response.body.contains(message), "{field:?}: {message}");
        // What was typed comes back.
        assert!(response.body.contains("Samsung Galaxy") || field.0 == "title", "{field:?}");
    }

    // Free needs no price; the same title twice in a day is refused.
    assert_eq!(post(("price_type", "FREE")).await.status, StatusCode::SEE_OTHER);
    let twice = post(("", "")).await;
    assert!(twice.body.contains("Ke publikuar tashmë një njoftim me këtë titull sot."));

    // Five a day.
    for n in 2..=5 {
        let title = format!("Njoftim prove numër {n}");
        assert_eq!(post(("title", &title)).await.status, StatusCode::SEE_OTHER);
    }
    let sixth = post(("title", "Njoftimi i gjashtë sot")).await;
    assert!(sixth.body.contains("Ke publikuar 5 njoftime sot."));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn sellers_who_sign_in_by_email_get_an_email_button(pool: PgPool) {
    let id: String = sqlx::query_scalar(
        "INSERT INTO users (auth_subject, email, display_name)
         VALUES ('email:besa@example.com', 'besa@example.com', 'Besa')
         RETURNING id::text",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let besa = session_for(&pool, &id).await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&besa)).await;
    let url = posted.location.unwrap();
    assert!(get(pool.clone(), &url).await.body.contains(r#"</svg><span class="button-label">Shfaq email-in</span></button>"#));
    let public_no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let reveal = post_form(pool, &format!("/listing/{public_no}/phone"), &[], None).await;
    assert!(reveal.body.contains(r#"href="mailto:besa@example.com" translate="no"><svg class="contact-icon""#));
    assert!(reveal.body.contains("</svg>besa@example.com</a>"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn forms_posted_from_other_sites_are_refused(pool: PgPool) {
    let post = |origin: &'static str| {
        let pool = pool.clone();
        async move {
            let request = Request::post("/dil")
                .header(header::HOST, "marketplace.example")
                .header(header::ORIGIN, origin)
                .body(Body::empty())
                .unwrap();
            send(pool, request).await.status
        }
    };
    assert_eq!(post("https://evil.example").await, StatusCode::FORBIDDEN);
    assert_eq!(post("https://marketplace.example").await, StatusCode::SEE_OTHER);
}

/// Posts the standard test listing as this seller; returns its number.
async fn post_listing(pool: &PgPool, seller: &str) -> String {
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(seller)).await;
    let url = posted.location.expect("posted");
    url.trim_start_matches("/listing/").split('-').next().unwrap().to_string()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_just_posted_says_so_once_with_ways_to_share_it(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&ana)).await;
    let url = posted.location.clone().expect("posted");
    let number = posted.cookie("posted").expect("the cookie that says it was just posted").to_string();
    assert!(url.starts_with(&format!("/listing/{number}-")));

    // Its page says it is up, with the share sheet's button and a WhatsApp message with its title
    // and address, and leads to the seller's own listings.
    let page = get_with_cookie(pool.clone(), &url, &format!("{ana}; posted={number}")).await;
    assert!(page.body.contains("<p><strong>Njoftimi yt u publikua.</strong> Ndaje, që ta shohin më shumë njerëz.</p>"), "{}", page.body);
    assert!(page.body.contains(r#"<button type="button" class="button small" data-share-target="button" data-action="share#share" title="Ndaje">"#));
    assert!(page.body.contains(r#"href="https://wa.me/?text=Samsung%20Galaxy%20S21%2C%20128%20GB%20http"#), "{}", page.body);
    assert!(page.body.contains(r#"<a href="/njoftimet-e-mia">Njoftimet e mia</a>"#));
    // Once: the cookie goes with the answer, and the page says nothing of it after.
    assert_eq!(page.cookie("posted"), Some(""));
    assert!(!get_with_cookie(pool.clone(), &url, &ana).await.body.contains("Njoftimi yt u publikua"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sellers_edit_their_own_listings(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");

    // The seller sees "Ndrysho" on their listing, with its pencil; others don't.
    let edit_link = format!(r#"<a class="button secondary" href="/listing/{no}/ndrysho"><svg class="button-icon""#);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&edit_link));
    assert!(!get(pool.clone(), &url).await.body.contains(&edit_link));

    // The form starts from the listing as it is.
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"value="Samsung Galaxy S21, 128 GB""#));
    assert!(form.contains(r#"value="32000""#));
    assert!(form.contains(r#"<option value="tirane" selected>Tiranë</option>"#));
    assert!(form.contains("Ruaj ndryshimet"));

    // A new title, price and town.
    let started = started();
    let mut fields = listing_fields(("title", "Samsung Galaxy S21, 256 GB"), &started);
    for field in &mut fields {
        match field.0 {
            "price" => field.1 = "29 000",
            "city" => field.1 = "durres",
            _ => {}
        }
    }
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &fields, Some(&ana)).await;
    assert_eq!(saved.location, Some(format!("/listing/{no}-samsung-galaxy-s21-256-gb")));
    let page = get(pool.clone(), saved.location.as_deref().unwrap()).await.body;
    assert!(page.contains("<h1>Samsung Galaxy S21, 256 GB</h1>"));
    assert!(page.contains("29\u{a0}000\u{a0}Lekë"));
    // The old address leads to the new one, and the listing moved to Durrës on the map.
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::PERMANENT_REDIRECT);
    let moved: bool = sqlx::query_scalar(
        "SELECT l.latitude = loc.latitude FROM listings l JOIN locations loc ON loc.slug = 'durres' AND loc.location_type = 'MUNICIPALITY'
         WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(moved);

    // The same checks as for a new listing.
    let bad = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("title", "Tel"), &started), Some(&ana)).await;
    assert_eq!(bad.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(bad.body.contains("Titulli duhet të ketë 5 deri në 120 shkronja."));

    // "Njoftimet e mia" lists it, with its edit link; the page ends in signing out.
    let mine = get_with_cookie(pool, "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains("Samsung Galaxy S21, 256 GB"));
    assert!(mine.contains(&format!(r#"href="/listing/{no}/ndrysho""#)));
    let out = mine.find(r#"<form class="sign-out" action="/dil" method="post">"#).expect("signing out");
    assert!(mine[out..].contains(r#"<button type="submit" class="button secondary"><svg"#) && mine[out..].contains("</svg>Dil nga llogaria</button>"));
    assert!(out > mine.find(r#"href="/shto""#).unwrap(), "last on the page");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn nobody_else_can_edit_a_listing(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let edit = format!("/listing/{no}/ndrysho");

    assert_eq!(get_with_cookie(pool.clone(), &edit, &besa).await.status, StatusCode::NOT_FOUND);
    let started = started();
    let fields = listing_fields(("title", "Shitet nga dikush tjetër"), &started);
    assert_eq!(post_form(pool.clone(), &edit, &fields, Some(&besa)).await.status, StatusCode::NOT_FOUND);
    // Not signed in: the sign-in comes first, and a post goes back to it.
    assert!(get(pool.clone(), &edit).await.body.contains(r#"action="/hyr""#));
    assert_eq!(post_form(pool.clone(), &edit, &fields, None).await.location.as_deref(), Some("/shto"));

    let title: String = sqlx::query_scalar("SELECT title FROM listings WHERE public_no = $1::bigint")
        .bind(&no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(title, "Samsung Galaxy S21, 128 GB");

    // A sold listing can't be edited any more, even by its seller.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = $1::bigint")
        .bind(&no)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(get_with_cookie(pool.clone(), &edit, &ana).await.status, StatusCode::NOT_FOUND);
    let mine = get_with_cookie(pool, "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge">Shitur</span>"#));
    assert!(!mine.contains(&edit));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_part_of_the_site_has_the_one_main_colour(pool: PgPool) {
    // The five parts of the site lost their colours of their own (the redesign's first step): no
    // page names its main category on its body any more, and the stylesheet has no rules for
    // them.
    for page in ["/", "/c/elektronike", "/c/shtepi-dhe-kopsht", "/c/makina", "/harta?category=elektronike", "/listing/1005-iphone-13-128-gb-si-i-ri"] {
        let page_html = html(pool.clone(), page).await;
        assert!(page_html.contains(r#"<body data-controller="menu">"#) && !page_html.contains("data-section"), "{page}");
    }
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    assert!(!css.contains("data-section"));
    // The greys and the main colour, the text's, in either theme; the old names point at them
    // until the pages have all moved over.
    let light = &css[css.find(":root{color-scheme:light;").expect("the light theme")..];
    let light = &light[..light.find('}').unwrap()];
    for token in ["--bg:#f7f7f7;", "--surface:#fff", "--surface-muted:#ececec;", "--border:#dedede;", "--text:#1b1b1b;", "--muted:#707070;", "--main:#1b1b1b;", "--choice-mint:#2de38c;", "--choice-lime:#c8ff00;"] {
        assert!(light.contains(token), "{token} in {light}");
    }
    let dark = &css[css.find(r#":root[data-theme="dark"]{"#).expect("the dark theme")..];
    let dark = &dark[..dark.find('}').unwrap()];
    for token in ["--bg:#1b1b1b;", "--surface:#242424;", "--surface-muted:#2c2c2c;", "--text:#e8e8e8;", "--muted:#a3a3a3;", "--main:#e8e8e8;"] {
        assert!(dark.contains(token), "{token} in {dark}");
    }
    for token in ["--accent:var(--main)", "--button:var(--main)", "--on-button:var(--on-main)", "--badge:var(--surface-muted)"] {
        assert!(css.contains(token), "{token}");
    }
    // A kept listing's heart is filled with the site's one red, in every part of the site, lined
    // thinly in white over a photo; the welcome page's round heart and conversations buttons went
    // on 2026-10-11, and their rules with them.
    assert!(!css.contains("circle-shortcut") && !css.contains("saved-shortcut") && !css.contains("inbox-shortcut"));
    let kept = &css[css.find(r#".heart-button[aria-pressed="true"] svg{"#).expect("the kept heart")..];
    let kept = &kept[..kept.find('}').unwrap()];
    assert!(kept.contains("fill:var(--red)") && kept.contains("color:var(--on-photo)"), "{kept}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn buttons_are_pills_in_the_accent_or_the_grey(pool: PgPool) {
    // The redesign's buttons (listing.css): pills with no outline, 48px tall, their words
    // semibold; the one that does a page's thing in the accent with dark words, a shade off under
    // the pointer (the user's ask of 2026-10-11: the main colour before), a secondary one on the
    // grey of secondary controls, a danger one that grey with red words, a small one 40px tall.
    // "Posto", the rail's and the welcome page's, is the accent in both themes, as they are.
    // (The stylesheet comes minified: web::site_css, a rule right after the one before it.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let button = rule(".button");
    for part in ["min-height:var(--size-button)", "border-radius:var(--radius-pill)", "background:var(--highlight)", "color:var(--on-highlight)", "font-weight:var(--semibold)"] {
        assert!(button.contains(part), "{part} in {button}");
    }
    assert!(rule(".button:hover").contains("background:var(--highlight-hover)"));
    assert!(rule(".button.secondary").contains("background:var(--surface-muted)"));
    assert!(rule(".button.danger").contains("color:var(--danger)"));
    assert!(rule(".button.small").contains("min-height:var(--size-button-sm)"));
    assert!(rule(".welcome-post").contains("background:var(--post)"));
    let light = &css[css.find(":root{color-scheme:light;").expect("the light theme")..];
    let light = &light[..light.find('}').unwrap()];
    for token in ["--post:var(--highlight);", "--on-post:var(--on-highlight);", "--post-hover:var(--highlight-hover);", "--highlight-hover:color-mix(in srgb,var(--highlight) 88%,#1b1b1b);"] {
        assert!(light.contains(token), "{token} in {light}");
    }
    // A shade darker under the pointer on the light theme, lighter on the dark one.
    let dark = &css[css.find(r#":root[data-theme="dark"]{"#).expect("the dark theme")..];
    let dark = &dark[..dark.find('}').unwrap()];
    assert!(dark.contains("--highlight-hover:color-mix(in srgb,var(--highlight) 85%,#fff") && !dark.contains("--post:"), "{dark}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_tapped_card_or_category_shows_it_in_the_accent(pool: PgPool) {
    // A card tapped or clicked underlines its title in the accent until its listing shows, with no
    // box round it in the main colour, and a category's pill fills its icon with the accent and
    // underlines its name in it, under the pointer too: the user's asks of 2026-10-11
    // (application.js marks either .pressed). No blue tap box of the browser's own anywhere: a
    // link's words, held, are underlined in the accent, the menu's rows' too.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".card:active .card-link,.card.pressed .card-link{text-decoration:underline 2px var(--highlight);text-underline-offset:.14em;}",
        ".card:has(.heart:active) .card-link{text-decoration:none;}",
        ".shortcut:active .shortcut-icon,.shortcut.pressed .shortcut-icon{fill:var(--highlight);}",
        ".shortcut:hover .shortcut-icon{fill:var(--highlight);}",
        ".shortcut:active,.shortcut.pressed{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        ".shortcut:hover{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        "html{-webkit-tap-highlight-color:transparent;}",
        "a:not([class]):active{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        "a.menu-row:active .menu-row-word{text-decoration:underline 2px var(--highlight);text-underline-offset:.2em;}",
        // Under the pointer a card's title too, but not while the pointer is on its heart.
        ".card:hover .card-link{text-decoration:underline 2px var(--highlight);text-underline-offset:.14em;}",
        ".card:has(.heart:hover) .card-link{text-decoration:none;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(".card.pressed{") && !css.contains("outline-offset:4px"), "no box round a pressed card");
    // After a finger's tap the next page waits a moment before it is drawn, so that the mark shows
    // (the page was fetched as the finger touched it). (The script comes minified too:
    // web::minified_js.)
    let script = get(pool, &format!("/static/js/application.js?v={}", marketplace::web::asset_version())).await.body;
    for part in ["closest(`a.shortcut`)", "`.card.pressed, .shortcut.pressed`", "addEventListener(`turbo:before-render`", ".detail.resume()"] {
        assert!(script.contains(part), "{part} in {script}");
    }
    // iPhones and iPads alone are kept from zooming into a field as it takes the focus (the page's
    // size can make its words smaller than 16px), after every page Turbo shows.
    for part in ["iPhone|iPad|iPod", ", maximum-scale=1", "`turbo:load`"] {
        assert!(script.contains(part), "{part} in {script}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_way_back_is_a_chevron_level_with_its_words(pool: PgPool) {
    // The breadcrumb's way back is Phosphor's chevron, its Regular weight's (the Duotone's caret is
    // a triangle), moved into the middle of its box as every caret is (icons.rs), and the
    // breadcrumb's words stand at the font's own line height, so that their capitals sit level
    // with it: the user found it off the middle of its line on 2026-10-11.
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let back = &page[page.find(r#"class="breadcrumb-back""#).expect("the way back")..];
    let back = &back[..back.find("</a>").unwrap()];
    assert!(back.contains(r#"<g transform="translate(.75 0) scale(.09375)" stroke="none"><path fill="currentColor" d="M165.66,202.34"#), "{back}");
    assert!(!page.contains("M160,48V208L80,128Z") && !page.contains("M176,128,96,208V48Z"), "no caret drawn as a triangle");
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let crumb = &css[css.find(".breadcrumb{").expect("the breadcrumb")..];
    let crumb = &crumb[..crumb.find('}').unwrap()];
    assert!(crumb.contains("line-height:normal"), "{crumb}");
    // "Ruaj kërkimin"'s bookmark is plain until the pointer or a finger is on it, when it fills
    // with the accent as a category's pill does (the user's ask of the same day).
    for rule in [
        ".save-search-button > svg{flex:none;fill:none;}",
        ".save-search-button:active > svg{fill:var(--highlight);}",
        ".save-search-button:hover > svg{fill:var(--highlight);}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains("--highlight-knob"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn pills_chips_and_badges_are_grey_or_the_main_colour(pool: PgPool) {
    // The redesign's pills (the categories', the path's), chips and badges: grey with no outline,
    // what is picked in the main colour, a filter that is on in the accent toned down (2026-10-11),
    // a badge's tone in its tint.
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    assert!(rule(".shortcut").contains("background:var(--surface-muted)"));
    assert!(rule(".breadcrumb-here").contains("color:var(--text)"));
    assert!(rule(".filter-chip").contains("background:var(--filter-on)"));
    let badge = rule(".badge");
    for part in ["height:1.5rem", "background:var(--surface-muted)", "color:var(--muted)"] {
        assert!(badge.contains(part), "{part} in {badge}");
    }
    assert!(rule(".badge.danger").contains("background:var(--danger-bg)"));
    assert!(rule(".badge.state").contains("background:var(--main)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn fields_checkboxes_and_radios_are_drawn_as_the_canvas_has_them(pool: PgPool) {
    // Form fields 48px tall with the hairline and 12px corners, focused with a soft ring rather
    // than the outline; checkboxes and radios 22px, checked in the main colour; a field's name in
    // the text's colour. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let fields = &css[css.find(".picker-button{width:100%").expect("the fields' rule")..];
    let fields = &fields[..fields.find('}').unwrap()];
    for part in ["min-height:var(--size-field)", "border:1px solid var(--border)", "border-radius:var(--radius-field)", "background-color:var(--surface)"] {
        assert!(fields.contains(part), "{part} in {fields}");
    }
    assert!(css.contains("box-shadow:var(--focus-field)") && css.contains("--focus-field:0 0 0 3px rgb(27 27 27 / 14%)"));
    assert!(rule(r#"input[type="checkbox"]:checked"#).contains("background:var(--main)"));
    assert!(rule(r#"input[type="radio"]:checked"#).contains("box-shadow:inset 0 0 0 4px var(--surface)"));
    assert!(rule(".field-label").contains("color:var(--text)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn cards_are_drawn_as_the_canvas_has_them(pool: PgPool) {
    // A card's heart a white outline 22px with a soft shadow and no circle behind it; its price
    // white and bold over a shade along the photo's bottom edge; a kept listing's status in a dark
    // pill over the photo. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let heart = rule(".heart-button svg");
    assert!(heart.contains("width:22px") && heart.contains("drop-shadow(0 1px 3px var(--photo-shade))"), "{heart}");
    assert!(!rule(".heart-button::before").contains("background:"));
    let shade = rule(".card-media::after");
    assert!(shade.contains("linear-gradient(") && shade.contains("var(--photo-shade)"), "{shade}");
    let price = rule(".card-media .card-price");
    assert!(price.contains("color:var(--on-photo)") && price.contains("font-weight:var(--bold)"), "{price}");
    assert!(rule(".card-status").contains("background:var(--photo-label)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_header_is_drawn_as_the_canvas_has_it(pool: PgPool) {
    // The bar over every page: white on the light theme and the page's colour on the dark one (the
    // header token), a hairline under it, as tall as the design system has it; the account alone
    // at its right end, a bare round button of 40px; what is unread in the accent, a dot ringed in
    // the bar's colour with its edge on the light theme, the conversations' count with its number
    // on the accent. (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let header = rule(".site-header");
    assert!(header.contains("background:var(--header)") && header.contains("border-bottom:1px solid var(--border)"), "{header}");
    assert!(rule(".header-inner").contains("height:var(--size-header)"));
    let account = rule(".account-button");
    assert!(account.contains("width:var(--size-button-sm)") && account.contains("margin-left:auto") && account.contains("border:0"), "{account}");
    let dot = rule(".account-button.has-news::before");
    assert!(dot.contains("background:var(--highlight)") && dot.contains("box-shadow:0 0 0 2px var(--header),var(--dot-edge)"), "{dot}");
    let count = rule(".inbox-count");
    assert!(count.contains("background:var(--highlight)") && count.contains("color:var(--on-highlight)"), "{count}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_menu_leads_everywhere(pool: PgPool) {
    // Every page has the menu (base.html): its lines ("Menyja" for screen readers) at the bar's
    // left, before the logo and the account, opening a dialog that leads to posting, the account's
    // pages, every main category in the welcome page's order (its pills', from the database, which
    // browse::menu_categories keeps), the look (the text's three sizes, the dark theme's switch,
    // the five accents), the language through /gjuha, and the pages about the site.
    let home = get(pool.clone(), "/").await.body;
    for page in [home.clone(), get(pool.clone(), "/c/automjete").await.body] {
        assert!(page.contains(r#"<body data-controller="menu">"#));
        let bar = &page[page.find(r#"<header class="site-header">"#).unwrap()..page.find("</header>").unwrap()];
        let order = positions(bar, &[r#"<button type="button" class="menu-button""#, r#"<a class="logo" id="logo""#, r#"<a class="icon-button account-button""#]);
        assert!(order.is_sorted(), "{order:?}");
        assert!(bar.contains(r#"<span class="menu-button-word">Menyja</span>"#));
        let menu = &page[page.find(r#"<dialog class="menu" id="menu" aria-label="Menyja""#).expect("the menu")..];
        let menu = &menu[..menu.find("</dialog>").unwrap()];
        for href in ["/shto", "/njoftimet-e-mia", "/te-ruajturat", "/kerkimet-e-ruajtura", "/lajmerimet", "/mesazhet", "/llogaria/cilesimet", "/kontakt"] {
            assert!(menu.contains(&format!(r#"href="{href}""#)), "{href}");
        }
        // The text's four sizes, the smallest added on 2026-10-11, and the five accents, mint first,
        // the one there is unless another is picked (lime before it).
        let sizes: Vec<String> = ["smaller", "small", "normal", "large"].iter().map(|size| format!(r#"data-look-size-param="{size}""#)).collect();
        let order = positions(menu, &sizes.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(order.is_sorted(), "{order:?}");
        assert!(menu.contains(r#"<button type="button" class="text-size-smaller" aria-label="Shkrim më i vogël" aria-pressed="false""#));
        // And the page's size, everything drawn smaller (2026-10-11): 70, 80, 90 and the whole page,
        // each a frame's corners a size bigger, after the text's size.
        let pages: Vec<String> = ["70", "80", "90", "100"].iter().map(|page| format!(r#"aria-label="Faqja {page}%" title="{page}%""#)).collect();
        let order = positions(menu, &[sizes.last().unwrap().as_str(), r#"<span id="menu-page-size">Madhësia e faqes</span>"#, pages[0].as_str(), pages[1].as_str(), pages[2].as_str(), pages[3].as_str()]);
        assert!(order.is_sorted(), "{order:?}");
        assert!(menu.contains(r#"title="100%" aria-pressed="true" data-look-target="page" data-look-page-param="100" data-action="look#page">"#));
        let swatches: Vec<String> = ["mint", "lime", "coral", "orange", "blue"].iter().map(|accent| format!(r#"<input type="radio" class="swatch" name="accent" value="{accent}""#)).collect();
        let order = positions(menu, &swatches.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(order.is_sorted(), "{order:?}");
        assert!(menu.contains(r#"value="mint" aria-label="Nenexhik" checked "#) && menu.matches(" checked ").count() == 1);
        assert!(menu.contains(r#"<input type="checkbox" class="switch" role="switch" data-controller="theme""#));
        assert!(menu.contains(r#"<form class="segmented" method="post" action="/gjuha""#));
        assert!(menu.contains(r#"<button type="submit" name="gjuha" value="sq" lang="sq" aria-pressed="true">Shqip</button>"#));
        // A row's words stand in a span of their own, closed inside its link, and a count is a span
        // after them, hidden until there is something to count (a "</span>" inside the count's tag
        // once showed its attributes as words, in the accent).
        assert!(!menu.contains("</a></span>"), "{menu}");
        for target in ["notifications", "messages"] {
            assert!(menu.contains(&format!(r#"</span><span class="menu-count" data-menu-target="{target}" hidden>"#)), "{target}");
        }
    }
    // Its categories are the welcome page's pills, in their order.
    let slugs = |part: &str| -> Vec<String> {
        part.match_indices(r#"href="/c/"#).map(|(at, _)| part[at + 9..].split('"').next().unwrap().to_string()).collect()
    };
    let pills = &home[home.find(r#"<nav class="shortcuts" aria-label="Kategoritë">"#).unwrap()..];
    let pills = &pills[..pills.find("</nav>").unwrap()];
    let menu = &home[home.find(r#"<h2 id="menu-categories">"#).unwrap()..];
    let menu = &menu[..menu.find("</ul>").unwrap()];
    assert_eq!(slugs(menu).len(), 11);
    assert_eq!(slugs(menu), slugs(pills));
    // The look the visitor picked comes back before the first paint (web::THEME_SCRIPT): the
    // theme, the text's size and the accent, from the browser's storage.
    for part in ["root.dataset.theme = theme", "root.dataset.textSize = size", "root.dataset.accent = accent"] {
        assert!(home.contains(part), "{part}");
    }
    assert!(home.contains(r#"if (["smaller", "small", "large"].includes(size)) root.dataset.textSize = size;"#));
    assert!(home.contains(r#"if (["90", "80", "70"].includes(page)) root.dataset.pageSize = page;"#));
    assert!(home.contains(r#"if (["lime", "coral", "orange", "blue"].includes(accent)) root.dataset.accent = accent;"#));
    // Drawn as the canvas has it: white over the page dimmed, its counts in the accent, a link's
    // icon filled with the accent under the pointer; the text's size scales every size in rem.
    // (The stylesheet comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    for rule in [
        ".menu::backdrop{background:var(--scrim);}",
        r#":root[data-text-size="smaller"]{--text-scale:.8125;}"#,
        r#":root[data-text-size="large"]{--text-scale:1.1875;}"#,
        // The page's size: the whole page zoomed, the maps at their own scale, the frame's column
        // as wide on the screen as before.
        r#":root[data-page-size="70"]{--page-scale:.7;}"#,
        "html{zoom:var(--page-scale);}",
        ":is(.map-canvas,.map-pin-canvas){zoom:calc(1",
        "--content-width:calc(1068px",
        "html{font-size:calc(100% * var(--text-scale));}",
        "a.menu-row:hover > svg{fill:var(--highlight);}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    let count = &css[css.find("}.menu-count{").expect("the menu's counts") + 1..];
    assert!(count[..count.find('}').unwrap()].contains("background:var(--highlight)"));
    // Every accent as bright as lime (2026-10-11), the same in both themes and dark words on each,
    // mint when none is picked (the user's pick, the same day), a filter that is on taking as much
    // of it as each needs; a pick from before, a darker one, comes back as its bright one.
    for rule in [
        "--choice-mint:#2de38c;",
        "--choice-lime:#c8ff00;",
        "--choice-coral:#ff6b6b;",
        "--choice-orange:#ff9f1c;",
        "--choice-blue:#4dabff;",
        ":root{--highlight:var(--choice-mint);--on-highlight:#1b1b1b;--filter-share-light:38%;--filter-share-dark:27%;}",
        r#":root[data-accent="lime"]{--highlight:var(--choice-lime);--filter-share-light:45%;--filter-share-dark:24%;}"#,
        r#":root[data-accent="coral"]{--highlight:var(--choice-coral);--filter-share-light:28%;--filter-share-dark:38%;}"#,
        "--filter-on:color-mix(in srgb,var(--highlight) var(--filter-share),var(--bg));",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(!css.contains(r#"[data-accent="mint"]"#) && !css.contains("--lime:"));
    assert_eq!(css.matches("--on-highlight:").count(), 1);
    // A listing's price on a band of it across the figure's foot, as Tradera marks its prices
    // (2026-10-11): a thick underline under the letters, in the tint a filter that is on takes, as
    // quiet as the user asked.
    for rule in [
        "--price-mark:var(--filter-on);",
        ".price-amount{text-decoration-line:underline;text-decoration-color:var(--price-mark);text-decoration-thickness:.34em;text-decoration-skip-ink:none;text-underline-offset:-.2em;}",
    ] {
        assert!(css.contains(rule), "{rule}");
    }
    assert!(home.contains(r#"const older = { darkred: "coral", terracotta: "orange", olive: "mint" };"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_rail_and_the_bars_search_lead_everywhere(pool: PgPool) {
    // The rail, the menu folded (base.html, rail.rs), as the user drew it on 2026-10-11: posting,
    // then the front page, the search, the listings kept and the conversations, in that order on
    // every page, the one the page is marked as it (a category's listings are the search's).
    for (path, current) in [
        ("/", r#"<a class="rail-item" href="/" aria-current="page">"#),
        ("/c/makina", r#"<a class="rail-item" href="/search" aria-current="page">"#),
        ("/search", r#"<a class="rail-item" href="/search" aria-current="page">"#),
        ("/te-ruajturat", r#"<a class="rail-item rail-saved" href="/te-ruajturat" aria-current="page">"#),
        ("/mesazhet", r#"<a class="rail-item" href="/mesazhet" aria-current="page" data-controller="inbox">"#),
        ("/listing/1001-volkswagen-golf-7-2016-nafte", ""),
    ] {
        let page = html(pool.clone(), path).await;
        let rail = &page[page.find(r#"<nav class="rail" aria-label="Faqet kryesore">"#).unwrap_or_else(|| panic!("the rail on {path}"))..];
        let rail = &rail[..rail.find("</nav>").unwrap()];
        let order = positions(rail, &[
            r#"<a class="rail-post" href="/shto" aria-label="Posto njoftim""#,
            r#"<a class="rail-item" href="/""#,
            r#"<a class="rail-item" href="/search""#,
            r#"<a class="rail-item rail-saved" href="/te-ruajturat""#,
            r#"<a class="rail-item" href="/mesazhet""#,
        ]);
        assert!(order.is_sorted(), "{path}: {order:?}");
        assert_eq!(rail.matches(r#"aria-current="page""#).count(), usize::from(!current.is_empty()), "{path}: {rail}");
        assert!(rail.contains(current), "{path}: {rail}");
        assert!(rail.contains(r#"<span class="inbox-count" data-inbox-target="count" hidden></span>"#));
    }
    // The bar's search, on every page but the front page, whose own is its first thing: in every
    // category and town ("Kërko njoftime"), but on a category's page, which it searches with its
    // filters kept, saying which; the row's own search is gone.
    let home = html(pool.clone(), "/").await;
    assert!(!home.contains(r#"<form class="top-search""#));
    let listing = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert!(listing.contains(r#"<form class="top-search" action="/search" method="get" role="search" data-controller="suggest""#));
    assert!(listing.contains(r#"placeholder="Kërko njoftime" aria-label="Kërko njoftime""#));
    let cars = html(pool.clone(), "/c/makina?sort=price_asc").await;
    let bar = &cars[cars.find(r#"<header class="site-header">"#).unwrap()..cars.find("</header>").unwrap()];
    assert!(bar.contains(r#"<form class="top-search" action="/c/makina" method="get" role="search" data-controller="suggest""#), "{bar}");
    assert!(bar.contains(r#"placeholder="Kërko në Makina" aria-label="Kërko në Makina""#), "{bar}");
    assert!(bar.contains(r#"<input type="hidden" name="sort" value="price_asc">"#), "{bar}");
    assert!(!cars.contains("search-card") && !cars.contains("search-panel"));
    // Drawn as the user drew them: posting's square in "Posto"'s colours, the current place on a
    // grey pill; the searches grey pills, the front page's button the accent's. (The stylesheet
    // comes minified: web::site_css.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    assert!(rule(".rail-post").contains("background:var(--post)"));
    assert!(rule(r#".rail-item[aria-current="page"] .rail-icon"#).contains("background:var(--surface-muted)"));
    assert!(rule(".top-search").contains("background:var(--surface-muted)"));
    assert!(rule(".welcome-form").contains("background:var(--surface-muted)"));
    assert!(rule(".welcome-search").contains("background:var(--search-knob)"));
    // The bar's search stands in the middle of the page beside the rail: the bar's start and end
    // grow alike around it. A filter that is on is in the accent, toned down (2026-10-11).
    assert!(bar.contains(r#"<div class="header-start">"#) && bar.contains(r#"<div class="header-end">"#), "{bar}");
    assert!(css.contains(".header-start,.header-end{flex:1 1 0;min-width:max-content;}"));
    assert!(rule(".filter-chip").contains("background:var(--filter-on)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_footer_is_drawn_as_the_canvas_has_it(pool: PgPool) {
    // The footer: the links in the muted grey at the left and the copyright at the right, under a
    // hairline; the other language underlined; the page a column at least a screen tall, so that
    // the footer stands at its foot under a short page too. (The stylesheet comes minified.)
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let rule = |selector: &str| {
        let at = css.find(&format!("}}{selector}{{")).unwrap_or_else(|| panic!("{selector}"));
        let rule = &css[at + 1..];
        rule[..rule.find('}').unwrap()].to_string()
    };
    let footer = rule(".site-footer");
    for part in ["justify-content:space-between", "border-top:1px solid var(--border)", "color:var(--muted)"] {
        assert!(footer.contains(part), "{part} in {footer}");
    }
    assert!(rule(".footer-links a").contains("min-height:var(--size-button-sm)"));
    assert!(rule(".language-switch button").contains("text-decoration:underline"));
    let body = rule("body");
    // At least the screen's height, the page's own size divided out (data-page-size).
    assert!(body.contains("flex-direction:column") && body.contains("min-height:calc(100svh"), "{body}");
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_short_descriptions_fold_button_stays_hidden(pool: PgPool) {
    // The button that unfolds a long description waits with the hidden attribute until the
    // controller finds the description long (fold_controller.js). Its own display rule would
    // override the attribute and show it under a two-line description too, so the stylesheet
    // hides it again by the attribute.
    let css = get(pool, &format!("/static/css/site.css?v={}", marketplace::web::asset_version())).await.body;
    let shown = css.find(".description-more{").expect("the button's rule");
    let hidden = css.find(".description-more[hidden]{display:none;}").expect("the hidden rule");
    assert!(shown < hidden, "the hidden rule comes after the button's, to win");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn parts_have_their_kinds_a_level_down(pool: PgPool) {
    // Automjete lists its four subcategories, each with its icon; the kinds of parts sit under the
    // fourth.
    let vehicles = html(pool.clone(), "/c/automjete").await;
    for (slug, name) in [
        ("makina", "Makina"),
        ("motocikleta", "Motoçikleta"),
        ("automjete-tregtare", "Automjete tregtare"),
        ("pjese-dhe-aksesore", "Pjesë / aksesorë"),
    ] {
        let pill = &vehicles[vehicles.find(&format!(r#"<a class="shortcut" href="/c/{slug}">"#)).expect(slug)..];
        assert!(pill.starts_with(&format!(r#"<a class="shortcut" href="/c/{slug}"><svg class="shortcut-icon""#)), "{slug}");
        assert!(pill[..pill.find("</a>").unwrap()].ends_with(&format!("</svg>{name}")), "{slug}");
    }
    assert!(!vehicles.contains(r#"<a class="shortcut" href="/c/gps-dhe-navigatore">"#));
    // Every main category's subcategories have icons of their own too.
    for slug in ["prona", "elektronike", "shtepi-dhe-kopsht", "pune", "sherbime", "veshje-dhe-aksesore", "sport-dhe-hobi",
                 "kafshe-shtepiake", "makineri-dhe-biznes"] {
        let page = html(pool.clone(), &format!("/c/{slug}")).await;
        // The subcategories' pills, under the small ones, which carry icons of their own.
        let nav = &page[page.find(r#"<nav class="subcategories" aria-label="Nënkategoritë">"#).expect(slug)..];
        let nav = &nav[..nav.find("</nav>").unwrap()];
        let pills = nav.matches(r#"<a class="shortcut""#).count();
        assert!(pills > 0 && nav.matches(r#"<svg class="shortcut-icon""#).count() == pills, "{slug}: {nav}");
    }

    // The parts page lists its kinds in order, the spare parts first, under its old address.
    let parts = html(pool.clone(), "/c/pjese-dhe-aksesore").await;
    let kinds = positions(&parts, &[
        r#"<a class="shortcut" href="/c/pjese-kembimi"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/goma-dhe-disqe"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/audio-dhe-video"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/gps-dhe-navigatore"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/styling-dhe-pajisje-sportive"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/tapete-makine"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/rimorkio"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/mbushes-per-makina-elektrike"><svg class="shortcut-icon""#,
        r#"<a class="shortcut" href="/c/pjese-te-tjera"><svg class="shortcut-icon""#,
    ]);
    assert!(kinds.is_sorted(), "{kinds:?}");

    // A kind's page: its breadcrumb, the way up to its parent, then itself, and its heading under
    // it; no pills under, as it has no subcategories.
    let gps = html(pool.clone(), "/c/gps-dhe-navigatore").await;
    let head = positions(&gps, &[
        r#"<nav class="breadcrumb" aria-label="Rruga">"#,
        r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">Pjesë / aksesorë</a>"#,
        r#"<span class="breadcrumb-here" aria-current="page">GPS / navigatorë</span>"#,
        r#"<h1 class="browse-title">GPS / navigatorë</h1>"#,
    ]);
    assert!(head.is_sorted(), "{head:?}");
    assert!(!gps.contains(r#"<nav class="subcategories""#), "no subcategories, no pills under");
    // Every branch closes; the other two </details> are the order's and the view's (and the
    // typeface panel's, while TYPE_TRIAL is on).
    let others = 2 + gps.matches("<details class=\"type-menu\"").count();
    // ...and the row's filter dropdowns' (filter_drop.html).
    let others = others + gps.matches("<details class=\"filter-drop\"").count();
    assert_eq!(gps.matches("<details class=\"menu-branch\"").count(), gps.matches("</details>").count() - others, "balanced");

    // The old address still works.
    assert_eq!(get(pool.clone(), "/c/pjese-kembimi").await.status, StatusCode::OK);
    // Every dropdown of categories shows the kinds, a level further in: the search's filter too.
    let search = get(pool, "/search").await.body;
    assert!(search.contains("<option value=\"gps-dhe-navigatore\">\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}\u{a0}GPS / navigatorë</option>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_listing_form_offers_categories_at_every_depth(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    // Vehicles: cars and the like in one group, the kinds of parts in the next, under their path.
    let vehicles = form.find(r#"<optgroup label="Automjete">"#).expect("the vehicles group");
    let parts = form.find(r#"<optgroup label="Automjete › Pjesë / aksesorë">"#).expect("the parts group");
    assert!(vehicles < parts);
    let vehicles_group = &form[vehicles..parts];
    assert!(vehicles_group.contains(r#"<option value="makina""#));
    assert!(vehicles_group.contains(r#"<option value="automjete-tregtare""#));
    let parts_group = &form[parts..parts + form[parts..].find("</optgroup>").unwrap()];
    assert_eq!(parts_group.matches("<option value=").count(), 9);
    assert!(parts_group.contains(r#"<option value="gps-dhe-navigatore""#));
    // Categories that only group take no listings, so they aren't offered...
    assert!(!form.contains(r#"<option value="pjese-dhe-aksesore""#));
    assert!(!form.contains(r#"<option value="automjete""#));

    // ...nor taken.
    let started = started();
    let refused = post_form(pool, "/shto", &listing_fields(("category", "pjese-dhe-aksesore"), &started), Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh nënkategorinë"));
}

/// A part posted by this seller, with the vehicle it fits; returns its URL.
async fn post_part(pool: &PgPool, seller: &str, title: &str, fits: &[(&str, &str)]) -> String {
    let started = started();
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    for field in &mut fields {
        if field.0 == "title" {
            field.1 = title;
        }
    }
    fields.extend_from_slice(fits);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(seller)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    posted.location.expect("posted")
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn parts_say_which_vehicle_they_fit(pool: PgPool) {
    // The fields, under the parts' own names for them, each its name over it; none required.
    let fields = get(pool.clone(), "/shto/fushat?category=gps-dhe-navigatore").await.body;
    let labels = positions(&fields, &[
        r#"<label class="field-label" for="prop_vehicle_make">Marka e automjetit</label>"#,
        r#"<option value="">Zgjidh</option>"#,
        r#"<label class="field-label" for="prop_vehicle_model">Modeli i automjetit</label>"#,
        r#"<label class="field-label" for="prop_fits_from_year">Nga viti</label>"#,
        r#"<input id="prop_fits_from_year" name="prop_fits_from_year" value="" inputmode="decimal">"#,
        r#"<label class="field-label" for="prop_fits_to_year">Deri në vitin</label>"#,
    ]);
    assert!(labels.is_sorted(), "{labels:?}");
    assert!(!fields.contains(" required"));
    // So nothing says what the asterisk means.
    assert!(!fields.contains("e detyrueshme"));
    // The cars' own lists: models under their make, and "Tjetër".
    assert!(fields.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    assert!(fields.contains(r#"<option value="__other__">Tjetër…</option>"#));
    // Cars keep their own names for the same properties.
    let car_fields = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(car_fields.contains(r#"<label class="field-label" for="prop_vehicle_make">Marka <span class="required" aria-hidden="true">*</span></label>"#), "{car_fields}");
    assert!(car_fields.contains(r#"<select id="prop_vehicle_make" name="prop_vehicle_make" required>"#), "{car_fields}");
    assert!(car_fields.contains(r#"<option value="">Zgjidh</option>"#) && !car_fields.contains("Marka *"), "{car_fields}");
    assert!(car_fields.contains("e detyrueshme"));
    // A category that only groups has no fields to offer.
    assert!(!get(pool.clone(), "/shto/fushat?category=pjese-dhe-aksesore").await.body.contains("prop_"));

    let ana = new_seller(&pool, "+355691112233").await;
    let url = post_part(&pool, &ana, "Navigator origjinal për Golf", &[
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2012"),
        ("prop_fits_to_year", "2019"),
    ])
    .await;

    // The listing says it, under the same names, under its category's breadcrumb: the one above
    // it and its own, as the category's page has them.
    let page = get(pool.clone(), &url).await.body;
    positions(&page, &[
        r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">"#,
        r#"<a class="breadcrumb-here" href="/c/gps-dhe-navigatore">"#,
    ]);
    for (label, value) in [("Marka e automjetit", "Volkswagen"), ("Modeli i automjetit", "Golf"), ("Nga viti", "2012"), ("Deri në vitin", "2019")] {
        assert!(page.contains(&format!(">{label}</dt>")), "{label}");
        assert!(page.contains(&format!(">{value}</dd>")), "{value}");
    }
    // The years have a calendar, and their names before them; so do the make and model, which
    // alone would read as the part's own.
    assert!(page.contains(r#"</svg><span class="attribute-label" aria-hidden="true">Nga viti</span>2012</dd>"#));
    assert!(page.contains(r#"</svg><span class="attribute-label" aria-hidden="true">Marka e automjetit</span>Volkswagen</dd>"#));
    // Its card gives the vehicle after the city; a search for the make finds it.
    let path = url.trim_start_matches("http://localhost");
    let parts = get(pool.clone(), "/c/pjese-dhe-aksesore").await.body;
    assert!(parts.contains(&format!(r#"href="{path}">Navigator origjinal për Golf</a>"#)));
    assert!(parts.contains("<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>Volkswagen · Golf</p>"));
    assert!(get(pool.clone(), "/search?q=volkswagen+navigator").await.body.contains(path));

    // Only the make, or nothing at all: both fine.
    post_part(&pool, &ana, "Navigator për çdo Volkswagen", &[("prop_vehicle_make", "VOLKSWAGEN")]).await;
    post_part(&pool, &ana, "Navigator universal pa marke", &[]).await;

    // The years can't end before they start, nor come after next year.
    let started = started();
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.extend_from_slice(&[("prop_fits_from_year", "2019"), ("prop_fits_to_year", "2012")]);
    let refused = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Duhet të jetë 2019 ose më vonë."), "{}", refused.body);
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.push(("prop_fits_to_year", "2100"));
    let refused = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Duhet të jetë nga 1950 deri në"));
    // A model that isn't the make's is refused, as for cars.
    let mut fields = listing_fields(("category", "gps-dhe-navigatore"), &started);
    fields.extend_from_slice(&[("prop_vehicle_make", "AUDI"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF")]);
    let refused = post_form(pool, "/shto", &fields, Some(&ana)).await;
    assert!(refused.body.contains("Ky model nuk i përket markës së zgjedhur."));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn parts_filter_by_the_buyers_vehicle(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let newer = post_part(&pool, &ana, "GPS Golf 7", &[
        ("prop_vehicle_make", "VOLKSWAGEN"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2012"), ("prop_fits_to_year", "2019"),
    ])
    .await;
    let older = post_part(&pool, &ana, "GPS Golf 5", &[
        ("prop_vehicle_make", "VOLKSWAGEN"), ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_fits_from_year", "2004"), ("prop_fits_to_year", "2008"),
    ])
    .await;
    let from_only = post_part(&pool, &ana, "GPS Mercedes C", &[
        ("prop_vehicle_make", "MERCEDES_BENZ"), ("prop_fits_from_year", "2014"),
    ])
    .await;
    let any = post_part(&pool, &ana, "GPS universal", &[]).await;
    let found = |html: &str| -> Vec<bool> { [&newer, &older, &from_only, &any].iter().map(|url| html.contains(url.as_str())).collect() };

    // The parts page offers the vehicle as filters, the years as one: the buyer's vehicle's.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore").await;
    assert!(page.contains(r#"<label class="filter-label" for="filter-vehicle_make">Marka e automjetit</label>"#));
    assert!(page.contains(r#"<option value="AUDI">Audi</option>"#));
    assert!(page.contains(r#"<label class="filter-label" for="filter-fits_year">Viti i automjetit</label>"#));
    assert!(page.contains(r#"<select id="filter-fits_year" name="fits_year">"#));
    assert!(page.contains(r#"<option value="">Çdo vit</option>"#) && page.contains(r#"<option value="1970">1970</option>"#));
    assert!(!page.contains("fits_from_year") && !page.contains("fits_to_year_min"));
    assert_eq!(found(&page), [true, true, true, true]);

    // By make: the Golfs.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN").await;
    assert_eq!(found(&page), [true, true, false, false]);
    // And year: the one whose years include it.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    assert!(page.contains(r#"<option value="2015" selected>2015</option>"#));
    assert!(page.contains(r#"<a class="filter-chip" href="/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN" aria-label="Hiq filtrin Viti i automjetit: 2015">"#));
    // A kind of part narrows the page, so its pill keeps the filters; the way up, which widens it,
    // keeps only the words, the town and the map pin.
    let kept = "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015";
    assert!(page.contains(&format!(r#"<a class="shortcut" href="{kept}"><svg class="shortcut-icon""#)));
    assert!(page.contains(r#"<a class="breadcrumb-up" href="/c/automjete">Automjete</a>"#));
    let page = html(pool.clone(), "/c/gps-dhe-navigatore?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert_eq!(found(&page), [true, false, false, false]);
    // A page that doesn't take a filter leaves it out of its links too: Automjete has no make.
    let page = html(pool.clone(), "/c/automjete?vehicle_make=VOLKSWAGEN&fits_year=2015").await;
    assert!(page.contains(r#"<a class="shortcut" href="/c/pjese-dhe-aksesore">"#));
    // The years' ends count: 2012 and 2019 are in, 2011 and 2020 are not.
    for (year, fits) in [(2011, false), (2012, true), (2019, true), (2020, false)] {
        let page = html(pool.clone(), &format!("/c/pjese-dhe-aksesore?vehicle_make=VOLKSWAGEN&fits_year={year}")).await;
        assert_eq!(found(&page)[0], fits, "{year}");
    }
    // Year alone: what may fit stays, the one only from 2014 and the one that doesn't say.
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    let page = html(pool.clone(), "/c/pjese-dhe-aksesore?fits_year=2016").await;
    assert_eq!(found(&page), [true, false, true, true]);
    // The same on a kind's own page, and on search with the category.
    let page = html(pool.clone(), "/c/gps-dhe-navigatore?fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    let page = html(pool.clone(), "/search?category=pjese-dhe-aksesore&fits_year=2006").await;
    assert_eq!(found(&page), [false, true, false, true]);
    // A year that can't be one is ignored.
    for nonsense in ["abc", "1800", "20155"] {
        let page = html(pool.clone(), &format!("/c/pjese-dhe-aksesore?fits_year={nonsense}")).await;
        assert_eq!(found(&page), [true, true, true, true], "{nonsense}");
    }
    // Cars don't get it: their year is a range, as before.
    let cars = html(pool, "/c/makina").await;
    assert!(!cars.contains("fits_year"));
    assert!(cars.contains(r#"name="vehicle_year_min""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sellers_order_their_photos_and_keep_them_through_a_form_sent_back(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let fields = listing_fields(("", ""), &started);
    // The photos as the listing keeps them, the cover first: each one's width and whether it is
    // the cover.
    let photos = |pool: PgPool, no: String| async move {
        sqlx::query_as::<_, (String, i32, bool)>(
            "SELECT m.id::text, m.width_px, m.is_cover FROM listing_media m JOIN listings l ON l.id = m.listing_id
             WHERE l.public_no = $1::bigint ORDER BY m.is_cover DESC, m.sort_order",
        )
        .bind(no)
        .fetch_all(&pool)
        .await
        .unwrap()
    };

    // Posted in the order the form sends them: the first the cover.
    let posted = post_with_photos(pool.clone(), "/shto", &fields, &[jpeg(800, 600), jpeg(600, 800)], &ana).await;
    let no = posted.location.expect("posted").trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    let before = photos(pool.clone(), no.clone()).await;
    assert_eq!(before.iter().map(|photo| (photo.1, photo.2)).collect::<Vec<_>>(), vec![(800, true), (600, false)]);
    let (first, second) = (before[0].0.clone(), before[1].0.clone());

    // The edit form lists them for the script, which sends their order: a new one first, then the
    // second before the first; the new one is the cover now.
    let edit = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(edit.contains(&format!(r#"<li class="photo-item" data-photo-id="{first}">"#)), "{edit}");
    assert!(edit.contains(r#"<input type="hidden" name="photo_order" data-photos-target="order">"#));
    let order = format!("new:0,saved:{second},saved:{first}");
    let mut edited = fields.clone();
    edited.extend([("photo_order", order.as_str()), ("photos_kept", "1")]);
    let saved = post_with_photos(pool.clone(), &format!("/listing/{no}/ndrysho"), &edited, &[jpeg(1000, 500)], &ana).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    let after = photos(pool.clone(), no.clone()).await;
    assert_eq!(after.iter().map(|photo| (photo.1, photo.2)).collect::<Vec<_>>(), vec![(1000, true), (600, false), (800, false)]);
    assert_eq!((after[1].0.as_str(), after[2].0.as_str()), (second.as_str(), first.as_str()));
    // The script takes one out the same way the box does; an order naming another's photo, or
    // one taken out, is no order for it.
    let order = format!("saved:{first},saved:{},saved:{second}", before_photo_of_another(&pool).await);
    let mut edited = fields.clone();
    edited.extend([("photo_order", order.as_str()), ("remove_photo", after[0].0.as_str())]);
    post_with_photos(pool.clone(), &format!("/listing/{no}/ndrysho"), &edited, &[], &ana).await;
    let last = photos(pool.clone(), no.clone()).await;
    assert_eq!(last.iter().map(|photo| (photo.0.as_str(), photo.2)).collect::<Vec<_>>(), vec![(first.as_str(), true), (second.as_str(), false)]);

    // Sent back with an error, photos the browser holds come back as they were, and the form
    // doesn't ask for them again; without the script it does.
    let mut refused = listing_fields(("title", "Tel"), &started);
    refused.push(("photos_kept", "1"));
    let again = post_with_photos(pool.clone(), "/shto", &refused, &[jpeg(800, 600)], &ana).await;
    assert_eq!(again.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(again.body.contains(r#"data-photos-again-value="true""#) && !again.body.contains("Zgjidh fotot përsëri"), "{}", again.body);
    let plain = post_with_photos(pool.clone(), "/shto", &listing_fields(("title", "Tel"), &started), &[jpeg(800, 600)], &ana).await;
    assert!(plain.body.contains(r#"data-photos-again-value="false""#) && plain.body.contains("Zgjidh fotot përsëri: formulari i harron kur ka një gabim."));
}

/// A photo added to another seller's listing (the Mercedes, 1002), by its id.
async fn before_photo_of_another(pool: &PgPool) -> String {
    sqlx::query_scalar(
        "INSERT INTO listing_media (listing_id, media_type, status, storage_key, public_url, sort_order, is_cover)
         VALUES ('a0000000-0000-4000-8000-000000001002', 'IMAGE', 'READY', 'photos/another', '/media/photos/another.webp', 99, false)
         RETURNING id::text",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listings_get_photos(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let fields = listing_fields(("", ""), &started);
    let posted = post_with_photos(pool.clone(), "/shto", &fields, &[jpeg(3000, 2000), jpeg(800, 600)], &ana).await;
    let url = posted.location.expect("posted");
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap().to_string();

    // The listing shows both, the first as its cover, and serves them: the full one 1600 px wide,
    // as a JPEG for browsers without WebP and link previews (the filmstrip's thumbnails under
    // them, the mini copies, aside).
    let page = get(pool.clone(), &url).await.body;
    let photo_urls: Vec<&str> = page.match_indices(r#"<img src="/media/photos/"#).map(|(at, _)| {
        let start = at + r#"<img src=""#.len();
        &page[start..start + page[start..].find('"').unwrap()]
    }).filter(|url| !url.ends_with("-mini.webp?2")).collect();
    assert_eq!(photo_urls.len(), 2, "{page}");
    assert!(photo_urls.iter().all(|url| url.ends_with(".jpg")), "{photo_urls:?}");
    assert!(page.contains(r#"width="1600" height="1067""#));
    // The first loads ahead of anything else on the page.
    let first = &page[page.find(r#"<img src="/media/photos/"#).unwrap()..];
    assert!(first[..first.find('>').unwrap()].contains(r#" fetchpriority="high""#), "{first}");
    assert_eq!(page.matches("fetchpriority").count(), 1);
    // Each is kept as WebP in five sizes, which the browser picks between, here by their longer
    // side, as the page shows the photo whole: the cards' three cover a square of their shorter
    // side (900 × 600, 600 × 400 and 450 × 300 for the first). The second, 800 px wide, has a
    // small copy and a mini one of its own, and a card copy no smaller than itself, which the page
    // leaves out. A phone's page stands beside its photo from 960 px up, so the photo is sized for
    // its column, seven twelfths of the page (SPLIT_GALLERY_SIZES; a car's or a property's runs
    // across it).
    let (big, small) = (photo_urls[0].trim_end_matches(".jpg"), photo_urls[1].trim_end_matches(".jpg"));
    let sources = [
        format!(r#"<picture><source type="image/webp" srcset="{big}-mini.webp?2 450w, {big}-small.webp?2 600w, {big}-card.webp?2 900w, {big}-middle.webp 1200w, {big}.webp 1600w" sizes="(min-width: 992px) 541px, (min-width: 960px) calc((100vw - 4rem) * .5833),"#),
        format!(r#"<picture><source type="image/webp" srcset="{small}-mini.webp?2 400w, {small}-small.webp?2 533w, {small}.webp 800w" sizes="#),
    ];
    for (url, source) in photo_urls.iter().zip(&sources) {
        // Whole in its frame, over a blurred copy of itself: its tiny copy, in the page, a
        // decoration; then its copies, and the JPEG.
        let at = page.find(source.as_str()).expect(source);
        let backdrop = page[..at].rfind(r#"<img class="gallery-backdrop" src="data:image/webp;base64,"#).expect("the blurred copy");
        let between = &page[backdrop..at];
        assert!(between.trim_end().ends_with(r#" alt="">"#), "{between}");
        let img = &page[at..];
        assert!(img[img.find("<img").unwrap()..].starts_with(&format!(r#"<img src="{url}""#)));
    }
    assert_eq!(page.matches("</picture>").count(), 2);
    // Two make a strip to slide through, one at a time, as a big card's photos are
    // (slides_controller.js): the row scrolls, a dot for each photo, the first lit, the counter,
    // and the arrows, hidden until the controller comes.
    assert!(page.contains(r#"<div class="listing-media" data-controller="slides">"#));
    assert!(page.contains(r#"data-slides-target="track" data-action="scroll->slides#update" tabindex="0" role="region" aria-label="Fotot e njoftimit""#));
    assert!(page.contains(r#"<div class="gallery-dots" data-slides-target="dots" aria-hidden="true"><span class="is-current"></span><span></span></div>"#));
    // The counter is a button, "Të gjitha fotot" to screen readers, that shows the filmstrip
    // over the photos' foot and hides it again: the strip starts hidden, right after the counter
    // in the photos' own box, over which it stands (listing.css).
    assert!(page.contains(r#"<button type="button" class="gallery-count" data-action="slides#toggleThumbs" aria-controls="listing-thumbs" aria-expanded="false" title="Të gjitha fotot"><span class="visually-hidden">Të gjitha fotot: </span><span data-slides-target="count">1 / 2</span></button><div class="gallery-thumbs" id="listing-thumbs" data-slides-target="filmstrip" hidden>"#));
    // Under the photos, a filmstrip of them: each photo's mini copy in a square, the first marked
    // as the one that shows, a tap on any going to its photo; and arrows to step the strip, hidden
    // until the controller finds more thumbnails than fit.
    assert!(page.contains(r#"<div class="thumb-strip" data-slides-target="thumbs" data-action="scroll->slides#scrolled">"#));
    assert!(page.contains(&format!(r#"<button type="button" class="gallery-thumb is-current" aria-current="true" data-action="slides#pick" aria-label="Foto 1 nga 2"><img src="{big}-mini.webp?2" alt="" width="72" height="72" loading="lazy" decoding="async"></button>"#)), "{page}");
    assert!(page.contains(&format!(r#"<button type="button" class="gallery-thumb" data-action="slides#pick" aria-label="Foto 2 nga 2"><img src="{small}-mini.webp?2""#)));
    assert!(page.contains(r#"<button type="button" class="thumb-step thumb-next" data-slides-target="thumbsNext" data-action="slides#thumbsOn" aria-label="Fotot e tjera" hidden>"#));
    assert!(page.contains(r#"data-action="slides#previous" aria-label="Foto e mëparshme" hidden>"#));
    assert!(page.contains(r#"data-action="slides#next" aria-label="Foto tjetër" hidden>"#));
    assert!(page.contains(", foto 2 nga 2\""));
    let photo = get(pool.clone(), photo_urls[0]).await;
    assert_eq!(photo.status, StatusCode::OK);
    assert_eq!(photo.content_type.as_deref(), Some("image/jpeg"));
    for copy in ["", "-middle", "-card", "-small", "-mini"] {
        let webp = get(pool.clone(), &format!("{big}{copy}.webp")).await;
        assert_eq!(webp.content_type.as_deref(), Some("image/webp"), "{copy}");
        assert!(webp.body.starts_with("RIFF") && webp.body.chars().take(16).collect::<String>().contains("WEBP"));
    }
    // And the cards' three as AVIF.
    for copy in ["-card", "-small", "-mini"] {
        let avif = get(pool.clone(), &format!("{big}{copy}.avif")).await;
        assert_eq!(avif.content_type.as_deref(), Some("image/avif"), "{copy}");
        assert!(avif.body.contains("ftypavif"), "{copy}");
    }
    // A photo kept before there were small copies has its own made as it is first asked for,
    // from the card's, and kept.
    let small_key = format!("{}-small.webp", small.trim_start_matches("/media/"));
    let made = get(pool.clone(), &format!("{small}-small.webp")).await.body;
    media().delete(&small_key).await.unwrap();
    let remade = get(pool.clone(), &format!("{small}-small.webp")).await;
    assert_eq!((remade.status, remade.content_type.as_deref()), (StatusCode::OK, Some("image/webp")));
    assert_eq!(remade.headers[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert_eq!(remade.body, made);
    assert!(media().get(&small_key).await.unwrap().is_some());
    // One kept before there were AVIF copies has its WebP twin stand in for one, kept a few
    // minutes only, until the server's start has made them.
    media().delete(&format!("{}-small.avif", small.trim_start_matches("/media/"))).await.unwrap();
    let stand_in = get(pool.clone(), &format!("{small}-small.avif")).await;
    assert_eq!((stand_in.status, stand_in.content_type.as_deref()), (StatusCode::OK, Some("image/webp")));
    assert_eq!(stand_in.headers[header::CACHE_CONTROL], "public, max-age=300");
    assert_eq!(stand_in.body, made);

    // The card offers the cover's copies too, the cards' AVIF ones first, each by its shorter
    // side, which the card's square scales it by, the cards' copies under their new edition's
    // query, and shows its tiny one until it has loaded.
    let cards = get(pool.clone(), "/search?q=Galaxy").await.body;
    assert!(cards.contains(&format!(
        r#"<picture><source type="image/avif" srcset="{big}-mini.avif?2 300w, {big}-small.avif?2 400w, {big}-card.avif?2 600w, {big}-middle.webp 800w, {big}.webp 1067w" sizes="{SMALL_SIZES}"><source type="image/webp" srcset="{big}-mini.webp?2 300w, {big}-small.webp?2 400w, {big}-card.webp?2 600w, {big}-middle.webp 800w, {big}.webp 1067w" sizes="{SMALL_SIZES}">"#
    )), "{cards}");
    assert!(cards.contains(&format!(
        r#"<picture><source type="image/avif" srcset="{small}-mini.avif?2 300w, {small}-small.avif?2 400w, {small}-card.avif?2 600w" sizes="{SMALL_SIZES}"><source type="image/webp" srcset="{small}-mini.webp?2 300w, {small}-small.webp?2 400w, {small}.webp 600w" sizes="{SMALL_SIZES}">"#
    )), "{cards}");
    // The query changes nothing in what is served.
    let with_query = get(pool.clone(), &format!("{big}-small.avif?2")).await;
    assert_eq!((with_query.status, with_query.content_type.as_deref()), (StatusCode::OK, Some("image/avif")));
    assert_eq!(with_query.headers[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    assert!(cards.contains(&format!(r#"<img class="card-photo" src="{big}.jpg""#)));
    assert!(cards.contains(r#"style="background-image: url(data:image/webp;base64,"#));

    // Editing: the photos are on the form; one removed, and the other becomes the cover.
    let edit = format!("/listing/{no}/ndrysho");
    let form = get_with_cookie(pool.clone(), &edit, &ana).await.body;
    let first_id = {
        let at = form.find(r#"name="remove_photo" value=""#).expect("a photo to remove") + r#"name="remove_photo" value=""#.len();
        form[at..at + 36].to_string()
    };
    let mut fields = listing_fields(("", ""), &started);
    fields.push(("remove_photo", &first_id));
    let saved = post_with_photos(pool.clone(), &edit, &fields, &[], &ana).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    let page = get(pool.clone(), &url).await.body;
    assert_eq!(page.matches(r#"<img src="/media/photos/"#).count(), 1);
    // One is just the photo: nothing to swipe or count.
    assert!(page.contains(r#"<div class="gallery">"#));
    assert!(!page.contains(r#"data-controller="slides""#) && !page.contains("gallery-thumbs") && !page.contains("gallery-count") && !page.contains("gallery-dots") && !page.contains(", foto 1 nga"));
    // The removed one's files are gone, every copy.
    let copies = ["-middle.webp", "-card.webp", "-small.webp", "-mini.webp", "-card.avif", "-small.avif", "-mini.avif"].map(|copy| format!("{big}{copy}"));
    for file in [photo_urls[0].to_string(), format!("{big}.webp")].into_iter().chain(copies) {
        assert_eq!(get(pool.clone(), &file).await.status, StatusCode::NOT_FOUND, "{file}");
    }
    let covers: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.public_no = $1::bigint AND m.is_cover",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(covers, 1);

    // What isn't a photo is refused, and too many are too.
    let refused = post_with_photos(pool.clone(), &edit, &listing_fields(("", ""), &started), &[b"not a photo".to_vec()], &ana).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Një nga fotot nuk u lexua"));
    let many: Vec<Vec<u8>> = (0..8).map(|_| jpeg(40, 30)).collect();
    let refused = post_with_photos(pool.clone(), &edit, &listing_fields(("", ""), &started), &many, &ana).await;
    assert!(refused.body.contains("deri në 8 foto"));
    // Nor are other paths in the store served.
    assert_eq!(get(pool, "/media/../.env").await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sellers_pause_resume_and_delete_their_listings(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");
    let (pause, resume, delete) =
        (format!("/listing/{no}/caktivizo"), format!("/listing/{no}/aktivizo"), format!("/listing/{no}/fshi"));
    let searched = |pool: PgPool| async move { get(pool, "/search?q=Galaxy").await.body };

    // The seller has the buttons on their listing; others don't.
    let pause_form = format!(r#"action="{pause}""#);
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains(&pause_form));
    assert!(!get(pool.clone(), &url).await.body.contains(&pause_form));

    // Nobody else can pause or delete it.
    for action in [&pause, &delete] {
        assert_eq!(post_form(pool.clone(), action, &[], Some(&besa)).await.status, StatusCode::NOT_FOUND);
    }

    // Paused: gone from the site, still on "Njoftimet e mia", with a way back.
    let paused = post_form(pool.clone(), &pause, &[], Some(&ana)).await;
    assert_eq!(paused.location.as_deref(), Some("/njoftimet-e-mia"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), &url, &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!searched(pool.clone()).await.contains(&url));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge">Çaktivizuar</span>"#));
    assert!(mine.contains(&format!(r#"action="{resume}""#)));
    // Its page, which "Njoftimet e mia" links to, is still there for its seller: it says it is
    // paused and that no one else sees it, offers "Aktivizo", and counts no view. It can be
    // edited meanwhile.
    let own = get_with_cookie(pool.clone(), &url, &ana).await;
    assert_eq!(own.status, StatusCode::OK);
    assert!(own.body.contains(r#"<span class="badge">Çaktivizuar</span>"#), "{}", own.body);
    assert!(own.body.contains("E ke çaktivizuar këtë njoftim: askush tjetër nuk e sheh."));
    assert!(own.body.contains(&format!(r#"<form method="post" action="{resume}">"#)) && !own.body.contains(&pause_form));
    assert!(!own.body.contains("data-seen-url") && !own.body.contains("phone-reveal"));
    assert_eq!(get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.status, StatusCode::OK);
    // Pausing it again changes nothing.
    assert_eq!(post_form(pool.clone(), &pause, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);

    // Back up.
    post_form(pool.clone(), &resume, &[], Some(&ana)).await;
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);
    assert!(searched(pool.clone()).await.contains(&url));

    // Deleted: gone everywhere, for good, and the history says who did it.
    let deleted = post_form(pool.clone(), &delete, &[], Some(&ana)).await;
    assert_eq!(deleted.location.as_deref(), Some("/njoftimet-e-mia"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(&url));
    assert_eq!(post_form(pool.clone(), &resume, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    let by_seller: bool = sqlx::query_scalar(
        "SELECT h.changed_by_user_id = l.created_by_user_id
         FROM listing_status_history h JOIN listings l ON l.id = h.listing_id
         WHERE l.public_no = $1::bigint AND h.to_status = 'DELETED'",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(by_seller);
}

/// A form sent from a page of the site, as a browser sends it: with the page as its Referer.
async fn post_from_page(pool: PgPool, uri: &str, page: &str, cookie: &str) -> TestResponse {
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(header::HOST, "localhost")
        .header(header::REFERER, format!("http://localhost{page}"))
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .unwrap();
    send(pool, request).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sellers_say_a_listing_is_reserved_or_sold(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");
    let (sell, reserve, unreserve, resume) = (
        format!("/listing/{no}/shitur"),
        format!("/listing/{no}/rezervo"),
        format!("/listing/{no}/zhrezervo"),
        format!("/listing/{no}/aktivizo"),
    );
    let form = |action: &str| format!(r#"<form method="post" action="{action}">"#);
    let status = |pool: PgPool| {
        let no = no.clone();
        async move {
            sqlx::query_as::<_, (String, bool)>("SELECT status, sold_at IS NOT NULL FROM listings WHERE public_no = $1::bigint")
                .bind(no)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    // Besa keeps it and writes to Ana about it.
    send(pool.clone(), tap_heart(&no, "1", Some(besa.as_str()))).await;
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &besa).await.location.unwrap();

    // On her page, Ana has "U shit" and "Rezervo" with her own buttons; nobody else has, nor can.
    let own = get_with_cookie(pool.clone(), &url, &ana).await.body;
    assert!(own.contains(&form(&sell)) && own.contains(&form(&reserve)) && !own.contains(&form(&unreserve)), "{own}");
    assert!(own.contains("</svg>U shit</button>") && own.contains("</svg>Rezervo</button>"));
    let theirs = get_with_cookie(pool.clone(), &url, &besa).await.body;
    assert!(!theirs.contains(&form(&sell)) && !theirs.contains(&form(&reserve)));
    for action in [&sell, &reserve] {
        assert_eq!(post_form(pool.clone(), action, &[], Some(&besa)).await.status, StatusCode::NOT_FOUND);
    }

    // Reserved: still up, saying so, and still reached; back to the page it was reserved on.
    let reserved = post_from_page(pool.clone(), &reserve, &url, &ana).await;
    assert_eq!(reserved.location.as_deref(), Some(url.as_str()));
    assert_eq!(status(pool.clone()).await, ("RESERVED".to_string(), false));
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains(r#"<span class="badge warn">Rezervuar</span>"#) && page.contains("phone-reveal"));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge warn">Rezervuar</span>"#) && mine.contains(&form(&unreserve)) && mine.contains(&form(&sell)));
    assert_eq!(post_form(pool.clone(), &reserve, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    // The reservation off, up as before; from "Njoftimet e mia", back there.
    let unreserved = post_from_page(pool.clone(), &unreserve, "/njoftimet-e-mia", &ana).await;
    assert_eq!(unreserved.location.as_deref(), Some("/njoftimet-e-mia"));
    assert_eq!(status(pool.clone()).await, ("ACTIVE".to_string(), false));

    // Sold: its page stays, saying so, with no way to reach Ana about it; it leaves the lists.
    post_from_page(pool.clone(), &sell, &url, &ana).await;
    assert_eq!(status(pool.clone()).await, ("SOLD".to_string(), true));
    let page = get(pool.clone(), &url).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains(r#"<span class="badge">Shitur</span>"#), "{}", page.body);
    assert!(!page.body.contains("phone-reveal") && !page.body.contains("/mesazh\""));
    assert!(!get(pool.clone(), "/search?q=Galaxy").await.body.contains(&url));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge">Shitur</span>"#) && mine.contains(&form(&resume)) && !mine.contains(&form(&sell)));
    // Besa still finds it among those she keeps, saying so over its photo, and in her
    // conversation with Ana, after its price.
    let kept = get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body;
    assert!(kept.contains(r#"<p class="card-status is-sold">Shitur</p>"#), "{kept}");
    assert!(card_numbers(&kept).contains(&no.parse().unwrap()));
    let conversation = get_with_cookie(pool.clone(), &thread, &besa).await.body;
    assert!(conversation.contains(r#"<span class="badge conversation-status">Shitur</span>"#), "{conversation}");

    // Back on sale: up again, when it was sold forgotten.
    post_form(pool.clone(), &resume, &[], Some(&ana)).await;
    assert_eq!(status(pool.clone()).await, ("ACTIVE".to_string(), false));
    assert!(!get_with_cookie(pool.clone(), &thread, &besa).await.body.contains("conversation-status"));

    // Taken down by Ana, it leaves Besa's cards for the list of those no longer shown, which says
    // why and lets it go.
    post_form(pool.clone(), &format!("/listing/{no}/caktivizo"), &[], Some(&ana)).await;
    let kept = get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body;
    assert!(!card_numbers(&kept).contains(&no.parse().unwrap()));
    let gone = section(&kept, "saved-gone-heading");
    assert!(gone.contains("Samsung Galaxy S21, 128 GB") && gone.contains(r#"<span class="badge">Çaktivizuar nga shitësi</span>"#), "{gone}");
    assert!(gone.contains(&format!(r#"<form method="post" action="/listing/{no}/ruaj">"#)));
    let let_go = post_form(pool.clone(), &format!("/listing/{no}/ruaj"), &[("ruaj", "0")], Some(&besa)).await;
    assert_eq!(let_go.status, StatusCode::SEE_OTHER);
    assert!(!get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body.contains("saved-gone"));
    post_form(pool.clone(), &resume, &[], Some(&ana)).await;

    // A rental is let out ("U dha me qira", "Dhënë me qira"); a wanted ad is neither sold nor
    // reserved.
    sqlx::query("UPDATE listings SET listing_type = 'RENT' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert!(get_with_cookie(pool.clone(), &url, &ana).await.body.contains("</svg>U dha me qira</button>"));
    post_form(pool.clone(), &sell, &[], Some(&ana)).await;
    assert!(get(pool.clone(), &url).await.body.contains(r#"<span class="badge">Dhënë me qira</span>"#));
    post_form(pool.clone(), &resume, &[], Some(&ana)).await;
    sqlx::query("UPDATE listings SET listing_type = 'BUY' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    let wanted = get_with_cookie(pool.clone(), &url, &ana).await.body;
    assert!(!wanted.contains(&form(&sell)) && !wanted.contains(&form(&reserve)));
    for action in [&sell, &reserve] {
        assert_eq!(post_form(pool.clone(), action, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_saved_search_counts_and_tells_of_its_new_listings(pool: PgPool) {
    let besa = new_seller(&pool, "+355691112244").await;
    let ana = new_seller(&pool, "+355691112233").await;
    let search = "/c/telefona?city=tirane";
    let alerts = || marketplace::web::send_search_alerts(&pool, "https://pluribus.example");
    let unseen = |pool: PgPool, session: String| async move {
        let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
        answer["searches"].as_i64().unwrap()
    };

    // The page offers to keep it, as its filters write it; not signed in, keeping it signs in first
    // and comes back to it.
    let page = get(pool.clone(), search).await.body;
    assert!(page.contains(r#"<form class="save-search" method="post" action="/kerkimet/ruaj">"#), "{page}");
    assert!(page.contains(&format!(r#"<input type="hidden" name="adresa" value="{search}">"#)));
    let signed_out = post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", search)], None).await;
    assert_eq!(signed_out.location.as_deref(), Some("/llogaria"));
    assert_eq!(signed_out.cookie("kthehu").map(|value| value.split(';').next().unwrap()), Some(search));
    // Only a category's or a search's page can be kept.
    assert_eq!(post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/mesazhet")], Some(&besa)).await.status, StatusCode::NOT_FOUND);

    // Kept: back on the page, which says so, once however often it is kept.
    for _ in 0..2 {
        let kept = post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", search)], Some(&besa)).await;
        assert_eq!(kept.location.as_deref(), Some(search));
    }
    let id: String = sqlx::query_scalar("SELECT id::text FROM saved_searches").fetch_one(&pool).await.unwrap();
    let page = get_with_cookie(pool.clone(), search, &besa).await.body;
    assert!(page.contains(&format!(r#"<form class="save-search" method="post" action="/kerkimet/{id}/fshi">"#)), "{page}");
    assert!(page.contains(r#"</svg><span class="save-search-label">Kërkimi u ruajt</span></button>"#));
    // Its page names it by its category and filters, with nothing new yet.
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(&format!(r#"<a class="saved-search-name" href="/kerkimet/{id}">Telefona · Tiranë</a>"#)), "{list}");
    assert!(!list.contains(r#"<span class="badge state">"#));

    // A phone put up in Tiranë since: one new listing, and, once the alerts' gap has passed since
    // it was kept, an alert: a notification, the heart's dot, a word by the link on /te-ruajturat.
    post_listing(&pool, &ana).await;
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(r#"<span class="badge state">1 njoftim i ri</span>"#), "{list}");
    assert_eq!(alerts().await.unwrap(), 0);
    // Four hours pass, for the search and the listings alike.
    sqlx::query("UPDATE saved_searches SET created_at = created_at - interval '4 hours'").execute(&pool).await.unwrap();
    sqlx::query("UPDATE listings SET published_at = published_at - interval '4 hours', bumped_at = bumped_at - interval '4 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(alerts().await.unwrap(), 1);
    let (title, url): (String, String) = sqlx::query_as(
        "SELECT title, payload ->> 'url' FROM notifications WHERE notification_type = 'SAVED_SEARCH'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(title, "1 njoftim i ri: Telefona · Tiranë");
    assert_eq!(url, format!("https://pluribus.example/kerkimet/{id}"));
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 1);
    assert!(get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body.contains("Kërkimet e ruajtura · 1 me njoftime të reja"));
    // Not again within the gap.
    assert_eq!(alerts().await.unwrap(), 0);
    // Its page, once shown, says so, and the alert is seen; another alert would bring the dot back.
    let list = get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body;
    assert!(list.contains(r#"<section class="saved-searches" data-seen-url="/kerkimet/pare">"#), "{list}");
    assert_eq!(post_form(pool.clone(), "/kerkimet/pare", &[], Some(&besa)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 0);
    assert!(!get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("data-seen-url"));

    // Opened, from its page or an alert: to its page, its new listings counted from now, its alert
    // seen. Nobody else opens or lets it go.
    assert_eq!(get_with_cookie(pool.clone(), &format!("/kerkimet/{id}"), &ana).await.status, StatusCode::NOT_FOUND);
    assert_eq!(post_form(pool.clone(), &format!("/kerkimet/{id}/fshi"), &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    let opened = get_with_cookie(pool.clone(), &format!("/kerkimet/{id}"), &besa).await;
    assert_eq!(opened.location.as_deref(), Some(search));
    assert!(!get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains(r#"<span class="badge state">"#));
    assert_eq!(unseen(pool.clone(), besa.clone()).await, 0);

    // Its alerts off, none, however much comes; on again from its page.
    let off = post_form(pool.clone(), &format!("/kerkimet/{id}/lajmerime"), &[], Some(&besa)).await;
    assert_eq!(off.location.as_deref(), Some("/kerkimet-e-ruajtura"));
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Lajmëromë</button>"));
    sqlx::query("UPDATE saved_searches SET last_alerted_at = now() - interval '4 hours', last_seen_at = now() - interval '4 hours'").execute(&pool).await.unwrap();
    assert_eq!(alerts().await.unwrap(), 0);
    post_form(pool.clone(), &format!("/kerkimet/{id}/lajmerime"), &[("lajmero", "1")], Some(&besa)).await;
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Ndalo lajmërimet</button>"));

    // Let go: the page offers to keep it again.
    let gone = post_form(pool.clone(), &format!("/kerkimet/{id}/fshi"), &[], Some(&besa)).await;
    assert_eq!(gone.location.as_deref(), Some("/kerkimet-e-ruajtura"));
    assert!(get_with_cookie(pool.clone(), "/kerkimet-e-ruajtura", &besa).await.body.contains("Ende s'ke ruajtur asnjë kërkim."));
    assert!(get_with_cookie(pool.clone(), search, &besa).await.body.contains(r#"</svg><span class="save-search-label">Ruaj kërkimin</span></button>"#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listings_expire_unless_their_sellers_renew_them(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");
    let renew = format!("/listing/{no}/rifresko");
    let renew_form = format!(r#"<form method="post" action="{renew}">"#);
    let run = || marketplace::expiry::run(&pool, "https://pluribus.example");
    // Runs a statement on the listing ($1, its number).
    let set = |pool: PgPool, sql: &'static str| {
        let no = no.clone();
        async move {
            sqlx::query(sql).bind(no).execute(&pool).await.unwrap();
        }
    };
    // Its status, the days it has left, and when it was last brought up.
    let state = |pool: PgPool| {
        let no = no.clone();
        async move {
            sqlx::query_as::<_, (String, f64, Option<time::OffsetDateTime>)>(
                "SELECT status, extract(epoch FROM expires_at - now())::float8 / 86400, bumped_at
                 FROM listings WHERE public_no = $1::bigint",
            )
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    let notified = |pool: PgPool, kind: &'static str| async move {
        sqlx::query_scalar::<_, String>("SELECT title FROM notifications WHERE notification_type = $1 ORDER BY created_at")
            .bind(kind)
            .fetch_all(&pool)
            .await
            .unwrap()
    };
    send(pool.clone(), tap_heart(&no, "1", Some(besa.as_str()))).await;

    // Posted, it has 60 days; the listings from before there was an expiry have none, and stay.
    let (status, days, _) = state(pool.clone()).await;
    assert_eq!(status, "ACTIVE");
    assert!((59.99..=60.0).contains(&days), "{days}");
    assert_eq!(run().await.unwrap(), (0, 0));
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(&renew_form));

    // Within a week of it, its seller sees when, and "Rifresko", on "Njoftimet e mia" and on its page.
    set(
        pool.clone(),
        "UPDATE listings SET expires_at = (date_trunc('day', now() AT TIME ZONE 'Europe/Tirane') + interval '5 days 12 hours') AT TIME ZONE 'Europe/Tirane'
         WHERE public_no = $1::bigint",
    )
    .await;
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge warn">Skadon pas 5 ditësh</span>"#), "{mine}");
    assert!(mine.contains(&renew_form));
    let own = get_with_cookie(pool.clone(), &url, &ana).await.body;
    assert!(own.contains(&renew_form) && own.contains(r#"title="Skadon pas 5 ditësh""#), "{own}");
    assert!(!get_with_cookie(pool.clone(), &url, &besa).await.body.contains(&renew_form));
    assert_eq!(run().await.unwrap(), (0, 0));

    // Three days from it, its seller hears of it, once.
    set(pool.clone(), "UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = $1::bigint").await;
    assert_eq!(run().await.unwrap(), (1, 0));
    assert_eq!(run().await.unwrap(), (0, 0));
    assert_eq!(notified(pool.clone(), "EXPIRING").await, ["Njoftimi skadon së shpejti: Samsung Galaxy S21, 128 GB"]);

    // Its time up: off the site, the history saying why, its seller told; once.
    set(
        pool.clone(),
        "UPDATE listings SET expires_at = now() - interval '1 minute', published_at = now() - interval '60 days'
         WHERE public_no = $1::bigint",
    )
    .await;
    assert_eq!(run().await.unwrap(), (0, 1));
    assert_eq!(run().await.unwrap(), (0, 0));
    assert_eq!(state(pool.clone()).await.0, "EXPIRED");
    assert_eq!(notified(pool.clone(), "EXPIRED").await, ["Njoftimi skadoi: Samsung Galaxy S21, 128 GB"]);
    let reason: Option<String> = sqlx::query_scalar(
        "SELECT h.reason FROM listing_status_history h JOIN listings l ON l.id = h.listing_id
         WHERE l.public_no = $1::bigint AND h.to_status = 'EXPIRED'",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(reason.as_deref(), Some("Skadoi"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), &url, &besa).await.status, StatusCode::NOT_FOUND);
    assert!(!get(pool.clone(), "/search?q=Galaxy").await.body.contains(&url));
    // Besa, who keeps it, finds it among those no longer shown, saying why.
    let gone = get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body;
    assert!(section(&gone, "saved-gone-heading").contains(r#"<span class="badge danger">Skadoi</span>"#), "{gone}");
    // Its seller still has its page, saying so, with "Rifresko" and none of the buttons for a
    // listing up or taken down for a while; and it on "Njoftimet e mia", likewise.
    let own = get_with_cookie(pool.clone(), &url, &ana).await;
    assert_eq!(own.status, StatusCode::OK);
    assert!(own.body.contains(r#"<span class="badge danger">Skaduar</span>"#), "{}", own.body);
    assert!(own.body.contains("Ky njoftim skadoi: askush tjetër nuk e sheh. „Rifresko“ e kthen në faqe për 60 ditë të tjera."));
    assert!(own.body.contains(&renew_form) && !own.body.contains(&format!("/listing/{no}/aktivizo")) && !own.body.contains(&format!("/listing/{no}/caktivizo")));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge danger">Skaduar</span>"#) && mine.contains(&renew_form), "{mine}");

    // Renewed by nobody else; by Ana, from its page: up again for 60 days, back on that page, and,
    // published 60 days ago, at the top of "newest first" again.
    assert_eq!(post_form(pool.clone(), &renew, &[], Some(&besa)).await.status, StatusCode::NOT_FOUND);
    let renewed = post_from_page(pool.clone(), &renew, &url, &ana).await;
    assert_eq!(renewed.location.as_deref(), Some(url.as_str()));
    let (status, days, bumped) = state(pool.clone()).await;
    assert_eq!(status, "ACTIVE");
    assert!((59.99..=60.0).contains(&days), "{days}");
    assert!(bumped.is_some());
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);
    assert_eq!(card_numbers(&get(pool.clone(), "/").await.body).first(), Some(&no.parse().unwrap()));
    // Renewed again within the week, from "Njoftimet e mia": its 60 days from now, but not brought
    // up again.
    set(pool.clone(), "UPDATE listings SET expires_at = now() + interval '1 day' WHERE public_no = $1::bigint").await;
    let renewed = post_from_page(pool.clone(), &renew, "/njoftimet-e-mia", &ana).await;
    assert_eq!(renewed.location.as_deref(), Some("/njoftimet-e-mia"));
    let (_, days, bumped_again) = state(pool.clone()).await;
    assert!((59.99..=60.0).contains(&days), "{days}");
    assert_eq!(bumped_again, bumped);

    // Taken down for a while, it neither expires nor renews; put back up, it has 60 days again.
    post_form(pool.clone(), &format!("/listing/{no}/caktivizo"), &[], Some(&ana)).await;
    set(pool.clone(), "UPDATE listings SET expires_at = now() - interval '1 day' WHERE public_no = $1::bigint").await;
    assert_eq!(run().await.unwrap(), (0, 0));
    assert_eq!(post_form(pool.clone(), &renew, &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    post_form(pool.clone(), &format!("/listing/{no}/aktivizo"), &[], Some(&ana)).await;
    let (status, days, _) = state(pool.clone()).await;
    assert_eq!(status, "ACTIVE");
    assert!((59.99..=60.0).contains(&days), "{days}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_stores_listing_tells_its_expiry_to_whoever_still_keeps_it(pool: PgPool) {
    // Besa posts for Dritan's store, Auto Durrësi.
    let besa = new_seller(&pool, "+355691112244").await;
    let store = "44444444-4444-4444-8444-444444444444";
    sqlx::query("INSERT INTO business_members (business_id, user_id, role) VALUES ($1::uuid, $2::uuid, 'EDITOR')")
        .bind(store)
        .bind(account_of(&besa))
        .execute(&pool)
        .await
        .unwrap();
    let no = post_listing(&pool, &besa).await;
    sqlx::query("UPDATE listings SET seller_user_id = NULL, seller_business_id = $2::uuid, expires_at = now() + interval '2 days' WHERE public_no = $1::bigint")
        .bind(&no)
        .bind(store)
        .execute(&pool)
        .await
        .unwrap();
    let run = || marketplace::expiry::run(&pool, "https://pluribus.example");
    let told = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, String)>(
            "SELECT user_id::text, notification_type FROM notifications
             WHERE notification_type IN ('EXPIRING', 'EXPIRED') ORDER BY created_at, notification_type",
        )
        .fetch_all(&pool)
        .await
        .unwrap()
    };
    let expiring = |user: &str| (user.to_string(), "EXPIRING".to_string());

    // While she posts for it, Besa hears that it is about to expire.
    assert_eq!(run().await.unwrap(), (1, 0));
    assert_eq!(told(pool.clone()).await, [expiring(account_of(&besa))]);

    // Gone from the store, she hears no more of it: Dritan, its owner, who may renew it, does,
    // of the same expiry, and of its end.
    sqlx::query("DELETE FROM business_members WHERE user_id = $1::uuid").bind(account_of(&besa)).execute(&pool).await.unwrap();
    assert_eq!(run().await.unwrap(), (1, 0));
    assert_eq!(run().await.unwrap(), (0, 0));
    assert_eq!(told(pool.clone()).await, [expiring(account_of(&besa)), expiring(DRITAN_ID)]);
    let dritan = session_for(&pool, DRITAN_ID).await;
    assert!(get_with_cookie(pool.clone(), &format!("/listing/{no}-samsung-galaxy-s21-128-gb"), &dritan).await.body.contains(&format!(r#"<form method="post" action="/listing/{no}/rifresko">"#)));
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(run().await.unwrap(), (0, 1));
    assert_eq!(told(pool.clone()).await.last(), Some(&(DRITAN_ID.to_string(), "EXPIRED".to_string())));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn listings_go_into_subcategories(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    // The form groups subcategories under their category.
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    assert!(form.contains(r#"<optgroup label="Elektronikë">"#));
    assert!(form.contains(r#"<option value="telefona" data-types="SELL,BUY" data-currency="ALL" data-condition>Telefona</option>"#));
    assert!(!form.contains(r#"<option value="elektronike""#), "a category with subcategories isn't offered");

    // Posting into the category itself is refused.
    let started = started();
    let refused = post_form(pool.clone(), "/shto", &listing_fields(("category", "elektronike"), &started), Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh nënkategorinë"));

    // A phone posted in Telefona shows under Elektronikë and Telefona, and "telefon" finds it.
    let posted = post_form(pool.clone(), "/shto", &listing_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/elektronike").await.body.contains("Samsung Galaxy S21"));
    assert!(get(pool.clone(), "/c/telefona").await.body.contains("Samsung Galaxy S21"));
    assert!(!get(pool.clone(), "/c/laptope").await.body.contains("Samsung Galaxy S21"));
    assert!(get(pool.clone(), "/c/elektronike?q=telefon").await.body.contains("Samsung Galaxy S21"));

    // Moved to Laptopë, its kind follows.
    let url = posted.location.unwrap();
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let moved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("category", "laptope"), &started), Some(&ana)).await;
    assert_eq!(moved.status, StatusCode::SEE_OTHER);
    // Neither its title nor its description says "laptop"; its kind does. (Searched across every
    // category, "laptop" names Laptopë, which the page would filter by instead.)
    assert!(get(pool, "/c/elektronike?q=laptop").await.body.contains("Samsung Galaxy S21"));
}

/// A car for sale, with its properties, and one field replaced (or added).
fn car_fields<'a>(replace: (&'a str, &'a str), started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = listing_fields(("category", "makina"), started);
    for field in &mut fields {
        match field.0 {
            "title" => field.1 = "Volkswagen Golf 7 1.6 TDI, 2017",
            "price" => field.1 = "11 500",
            "currency" => field.1 = "EUR",
            _ => {}
        }
    }
    fields.extend([
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
        ("prop_vehicle_year", "2017"),
        ("prop_vehicle_mileage", "120 000"),
        ("prop_vehicle_fuel", "DIESEL"),
        ("prop_vehicle_transmission", "MANUAL"),
        ("prop_vehicle_features", "ABS"),
        ("prop_vehicle_features", "NAVIGATION"),
    ]);
    match fields.iter_mut().find(|field| field.0 == replace.0) {
        Some(field) => field.1 = replace.1,
        None if !replace.0.is_empty() => fields.push(replace),
        None => {}
    }
    fields
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_car_is_posted_with_its_make_model_and_year(pool: PgPool) {
    // The fields come with the category: the frame the form reloads when it changes.
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(frame.starts_with(r#"<turbo-frame id="listing-properties">"#));
    assert!(frame.contains(r#"<input id="prop_vehicle_year" name="prop_vehicle_year" value="" inputmode="decimal" required>"#));
    // Its main fields two to a line, each its name over it, as the canvas has them: make, model,
    // year, fuel, gearbox, body, colour and mileage; then the rest, and its options last. (Tiles
    // with icons, named inside them, in a card of their own with its name as a header, once.)
    assert!(frame.contains(r#"<fieldset class="property-fields">"#));
    // The fields' name for screen readers, out of sight: the box they are in is the category's.
    assert!(frame.contains(r#"<legend class="visually-hidden">Detajet</legend>"#) && !frame.contains("property-heading"));
    let order = positions(&frame, &[
        r#"for="prop_vehicle_make""#,
        r#"for="prop_vehicle_model""#,
        r#"for="prop_vehicle_year""#,
        r#"for="prop_vehicle_fuel""#,
        r#"for="prop_vehicle_transmission""#,
        r#"for="prop_vehicle_body_type""#,
        r#"for="prop_vehicle_color""#,
        r#"for="prop_vehicle_mileage""#,
        r#"<span class="field-label">Opsionet</span>"#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!frame.contains("tile-icon"));
    // The mileage's unit inside its field, at its end; in its name for screen readers.
    assert!(frame.contains(r#"<label class="field-label" for="prop_vehicle_mileage">Kilometrazhi<span class="visually-hidden"> (km)</span></label>"#), "{frame}");
    assert!(frame.contains(r#"<input id="prop_vehicle_mileage" name="prop_vehicle_mileage" value="" inputmode="decimal">"#), "{frame}");
    assert!(frame.contains(r#"<span class="unit" aria-hidden="true">km</span>"#), "{frame}");
    // A part's make and model are fields like the rest.
    assert!(!get(pool.clone(), "/shto/fushat?category=gps-dhe-navigatore").await.body.contains("tile"));
    assert!(frame.contains(r#"<select id="prop_vehicle_model" name="prop_vehicle_model" required data-depends-on="prop_vehicle_make">"#));
    assert!(frame.contains(r#"<optgroup label="Volkswagen" data-parent="VOLKSWAGEN">"#));
    assert!(frame.contains(r#"name="prop_vehicle_features" value="ABS""#));
    // Phones have a brand and storage, not their kind: Telefona says that.
    let phones = get(pool.clone(), "/shto/fushat?category=telefona").await.body;
    assert!(phones.contains(r#"name="prop_electronics_brand""#) && !phones.contains("prop_electronics_type"));
    // A category with subcategories has no fields of its own to offer.
    assert!(!get(pool.clone(), "/shto/fushat?category=automjete").await.body.contains("prop_"));

    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &car_fields(("", ""), &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let page = get(pool.clone(), &url).await.body;
    for detail in ["Volkswagen", "Golf", "2017", "120\u{a0}000\u{a0}km", "Naftë", "Manuale", "ABS"] {
        assert!(page.contains(detail), "{detail}");
    }
    // On its card, in the filters and in search, like the listings already there.
    let card = "<p class=\"card-summary\"><span class=\"card-town\" translate=\"no\">Tiranë · </span>2017 · 120\u{a0}000\u{a0}km · Naftë</p>";
    assert!(get(pool.clone(), "/c/makina?vehicle_fuel=DIESEL&vehicle_year_min=2017").await.body.contains(card));
    assert!(get(pool.clone(), "/c/makina?vehicle_features=NAVIGATION").await.body.contains(&url));
    assert!(!get(pool.clone(), "/c/makina?vehicle_fuel=PETROL").await.body.contains(&url));

    // The edit form starts from them, and an edit replaces them.
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"<option value="VOLKSWAGEN_GOLF" selected>Golf</option>"#));
    assert!(form.contains(r#"name="prop_vehicle_mileage" value="120000""#));
    assert!(form.contains(r#"value="NAVIGATION" checked"#));
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &car_fields(("prop_vehicle_fuel", "HYBRID"), &started), Some(&ana)).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/makina?vehicle_fuel=HYBRID").await.body.contains(&url));
    assert!(!get(pool, "/c/makina?vehicle_fuel=DIESEL").await.body.contains(&url));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_cars_title_is_made_from_what_it_is(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let mut fields = car_fields(("prop_vehicle_body_type", "SEDAN"), &started);
    fields.extend([("prop_vehicle_color", "BLACK"), ("prop_vehicle_engine", "1598")]);
    let posted = post_form(pool.clone(), "/shto", &fields, Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let no: i64 = url.trim_start_matches("/listing/").split('-').next().unwrap().parse().unwrap();

    // The page's title is made from its make, model, engine (in litres, to a tenth) and year, and
    // kept with it.
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains("<h1>Volkswagen Golf 7 1.6 TDI, 2017</h1>") && page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf 1.6, 2017</span> · "#), "{page}");
    let generated: Option<String> = sqlx::query_scalar("SELECT generated_title FROM listings WHERE public_no = $1")
        .bind(no)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(generated.as_deref(), Some("Volkswagen Golf 1.6, 2017"));
    // The seller's own heads the page, not the description; the details leave out what the made
    // title says, but for the engine's exact size; the year stands among the key facts.
    assert!(!page.contains("description-heading"), "the seller's own heads the page, not the description");
    for said in ["Marka", "Modeli"] {
        assert!(!page.contains(&format!(">{said}</dt>")), "{said}");
    }
    assert!(page.contains("<dt>Viti</dt><dd>2017</dd>") && page.matches("Viti</dt>").count() == 1);
    for shown in ["Karroceria", "Ngjyra", "Motorri"] {
        assert!(page.contains(&format!("{shown}</dt>")), "{shown}");
    }
    // Its card says it too.
    let card = format!(r#"href="{url}">Volkswagen Golf 1.6, 2017</a>"#);
    assert!(get(pool.clone(), "/c/makina").await.body.contains(&card));

    // An edit makes it again: without the engine, the make, the model and the year.
    let fields = car_fields(("prop_vehicle_year", "2018"), &started);
    let saved = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &fields, Some(&ana)).await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);
    let edited = get(pool.clone(), &url).await.body;
    assert!(edited.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf, 2018</span> · "#), "{edited}");
    // The edit form keeps the seller's own.
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"value="Volkswagen Golf 7 1.6 TDI, 2017""#));

    // A part's make and model are the vehicle it fits: its own title stays.
    let part = post_part(&pool, &ana, "Navigator origjinal për Golf", &[
        ("prop_vehicle_make", "VOLKSWAGEN"),
        ("prop_vehicle_model", "VOLKSWAGEN_GOLF"),
    ])
    .await;
    let page = get(pool, &part).await.body;
    assert!(page.contains("<h1>Navigator origjinal për Golf</h1>") && !page.contains("description-heading"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn car_properties_are_checked(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    for (field, message) in [
        (("prop_vehicle_make", ""), "Plotëso „Marka“."),
        (("prop_vehicle_year", ""), "Plotëso „Viti“."),
        (("prop_vehicle_year", "1850"), "Duhet të jetë nga 1900 deri në"),
        (("prop_vehicle_year", "3000"), "Duhet të jetë nga 1900 deri në"),
        (("prop_vehicle_mileage", "-5"), "Duhet të jetë të paktën 0."),
        (("prop_vehicle_mileage", "shumë"), "Shkruaj një numër."),
        (("prop_vehicle_model", "BMW_3_SERIES"), "Ky model nuk i përket markës së zgjedhur."),
        (("prop_vehicle_fuel", "STEAM"), "Zgjidh një nga mundësitë."),
    ] {
        let response = post_form(pool.clone(), "/shto", &car_fields(field, &started), Some(&ana)).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{field:?}");
        assert!(response.body.contains(message), "{field:?}: {message}");
        // What was typed comes back, in the car's fields.
        assert!(response.body.contains(r#"name="prop_vehicle_transmission""#), "{field:?}");
    }
}

/// A car whose make and model the seller typed, not in the lists.
fn typed_car<'a>(make: &'a str, model: &'a str, started: &'a str) -> Vec<(&'a str, &'a str)> {
    let mut fields = car_fields(("prop_vehicle_make", "__other__"), started);
    for field in &mut fields {
        match field.0 {
            "prop_vehicle_model" => field.1 = "__other__",
            "title" => field.1 = "Zastava Yugo 45, 1988",
            _ => {}
        }
    }
    fields.extend([("prop_vehicle_make_other", make), ("prop_vehicle_model_other", model)]);
    fields
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn sellers_type_a_make_or_model_the_list_lacks(pool: PgPool) {
    // The make and model lists end with "Tjetër", which opens a box to type in.
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    assert!(frame.contains(r#"<option value="__other__">Tjetër…</option>"#));
    assert!(frame.contains(r#"name="prop_vehicle_make_other""#) && frame.contains(r#"name="prop_vehicle_model_other""#));
    assert!(!frame.contains(r#"name="prop_vehicle_fuel_other""#), "only some properties take a typed option");

    let ana = new_seller(&pool, "+355691112233").await;
    let started = started();
    let posted = post_form(pool.clone(), "/shto", &typed_car("Zastava", "Yugo", &started), Some(&ana)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let url = posted.location.unwrap();
    let page = get(pool.clone(), &url).await.body;
    // They make its title, as a listed make and model would: the two as the heading, the year
    // under it.
    assert!(page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Zastava Yugo, 2017</span> · "#), "{page}");
    // Search finds it by them; the lists and filters don't offer them until someone approves them.
    assert!(get(pool.clone(), "/search?q=yugo").await.body.contains(&url));
    assert!(!get(pool.clone(), "/shto/fushat?category=makina").await.body.contains(">Zastava<"));
    assert!(!get(pool.clone(), "/c/makina").await.body.contains(">Zastava<"));
    let (active, model_under_make): (bool, bool) = sqlx::query_as(
        "SELECT make.active, model.parent_option_id = make.id
         FROM attribute_options make, attribute_options model
         WHERE make.label = 'Zastava' AND model.label = 'Yugo'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((active, model_under_make), (false, true));

    // The edit form shows them as typed.
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap();
    let form = get_with_cookie(pool.clone(), &format!("/listing/{no}/ndrysho"), &ana).await.body;
    assert!(form.contains(r#"<option value="__other__" selected>Tjetër…</option>"#));
    assert!(form.contains(r#"name="prop_vehicle_make_other" value="Zastava""#));

    // Typed again, by someone else and in other letters, it is the same make; and "bmw" is the
    // listed BMW, not a new one.
    let besa = new_seller(&pool, "+355691112244").await;
    let again = post_form(pool.clone(), "/shto", &typed_car("  ZASTAVA ", "yugo", &started), Some(&besa)).await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.body);
    let zastavas: i64 = sqlx::query_scalar("SELECT count(*) FROM attribute_options WHERE lower(label) IN ('zastava', 'yugo')")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(zastavas, 2, "one Zastava, one Yugo");
    let mut bmw = typed_car("bmw", "Seria 1", &started);
    bmw.iter_mut().find(|field| field.0 == "title").unwrap().1 = "BMW 118d, 2015";
    assert_eq!(post_form(pool.clone(), "/shto", &bmw, Some(&besa)).await.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), "/c/makina?vehicle_make=BMW").await.body.contains(">BMW Seria 1, 2017</a>"));
    // A listed model typed without its punctuation ("id buzz") is the listed one too, ID. Buzz: it
    // comes to the same code.
    let mut buzz = typed_car("volkswagen", "id buzz", &started);
    buzz.iter_mut().find(|field| field.0 == "title").unwrap().1 = "Volkswagen ID. Buzz, 2023";
    let posted = post_form(pool.clone(), "/shto", &buzz, Some(&besa)).await;
    assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
    let buzz = get(pool.clone(), &posted.location.unwrap()).await.body;
    assert!(buzz.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen ID. Buzz, 2017</span> · "#), "{buzz}");
    let typed: i64 = sqlx::query_scalar("SELECT count(*) FROM attribute_options WHERE lower(label) = 'id buzz'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(typed, 0);

    // Checked like everything else.
    for ((make, model), message) in [
        (("", "Yugo"), "Shkruaj „Marka“."),
        (("www.zastava.al", "Yugo"), "Shkruaj vetëm emrin"),
        (("!!", "Yugo"), "Shkruaj vetëm emrin"),
    ] {
        let response = post_form(pool.clone(), "/shto", &typed_car(make, model, &started), Some(&ana)).await;
        assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY, "{make}");
        assert!(response.body.contains(message), "{make}: {message}");
    }
    // A listed model doesn't go with a typed make.
    let mut golf = typed_car("Zastava", "", &started);
    golf.iter_mut().find(|field| field.0 == "prop_vehicle_model").unwrap().1 = "VOLKSWAGEN_GOLF";
    let response = post_form(pool, "/shto", &golf, Some(&ana)).await;
    assert!(response.body.contains("Ky model nuk i përket markës së zgjedhur."));
}

/// The id of the make or model named so.
async fn option_id(pool: &PgPool, label: &str) -> String {
    sqlx::query_scalar("SELECT id::text FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Whether the option named so is offered, approved.
async fn option_active(pool: &PgPool, label: &str) -> Option<bool> {
    sqlx::query_scalar("SELECT active FROM attribute_options WHERE label = $1")
        .bind(label)
        .fetch_optional(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_approve_or_merge_what_sellers_typed(pool: PgPool) {
    let started = started();
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let moderator = new_seller(&pool, MODERATOR).await;
    // Typed in: a make the list lacks, with its model; a listed make spelt short, with its model;
    // and a model of a listed make, spelt long.
    let post = |seller: String, make: &'static str, model: &'static str, title: &'static str| {
        let pool = pool.clone();
        let started = started.clone();
        async move {
            let mut fields = typed_car(make, model, &started);
            fields.iter_mut().find(|field| field.0 == "title").unwrap().1 = title;
            let posted = post_form(pool, "/shto", &fields, Some(&seller)).await;
            assert_eq!(posted.status, StatusCode::SEE_OTHER, "{}", posted.body);
            posted.location.unwrap()
        }
    };
    let yugo = post(ana.clone(), "Zastava", "Yugo", "Zastava Yugo 45, 1988").await;
    let mercedes = post(besa.clone(), "Mercedes", "C 220", "Mercedes C 220, 2012").await;
    let golf = post(ana.clone(), "volkswagen", "Golf IV", "VW Golf IV 1.9 TDI").await;
    let zastava_id = option_id(&pool, "Zastava").await;
    let yugo_id = option_id(&pool, "Yugo").await;
    let golf_iv_id = option_id(&pool, "Golf IV").await;
    let mercedes_id = option_id(&pool, "Mercedes").await;

    // Only for moderators: anyone else finds nothing, and changes nothing.
    assert_eq!(get(pool.clone(), "/moderim").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), "/moderim", &ana).await.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", "Zastava")], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    let tried = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &option_id(&pool, "Golf").await)], Some(&ana)).await;
    assert_eq!(tried.status, StatusCode::NOT_FOUND);
    assert_eq!(option_active(&pool, "Zastava").await, Some(false));
    assert_eq!(option_active(&pool, "Golf IV").await, Some(false));
    assert!(!get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains("/moderim"));

    // The moderator's own page links to it, with how many wait.
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains(r#"<a class="button secondary small" href="/moderim">Për miratim (5)</a>"#));
    // Each, with its make, how many listings have it, who typed it; the likeliest listed one
    // comes chosen for a merge; a model whose make waits waits for it.
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await;
    assert_eq!(page.status, StatusCode::OK);
    let page = page.body;
    assert!(page.contains(r#"<strong class="pending-label">Zastava</strong>"#));
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Zastava</span>"#));
    assert!(page.contains("Mirato ose bashko më parë markën, „Zastava“."));
    assert!(page.contains(r#"<a href="/search?q=Golf+IV">1 njoftim</a> · shtuar nga Përdorues"#));
    let golf_id = option_id(&pool, "Golf").await;
    assert!(page.contains(&format!(r#"<option value="{golf_id}" selected>Golf</option>"#)));
    let mercedes_benz_id = option_id(&pool, "Mercedes-Benz").await;
    assert!(page.contains(&format!(r#"<option value="{mercedes_benz_id}" selected>Mercedes-Benz</option>"#)));
    // The model of a make that waits can't be approved yet.
    let early = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Yugo")], Some(&moderator)).await;
    assert_eq!(early.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Yugo").await, Some(false));

    // Approved, the make is offered, in its alphabetical place: after Volvo.
    let approved = post_form(pool.clone(), &format!("/moderim/{zastava_id}/mirato"), &[("label", " Zastava ")], Some(&moderator)).await;
    assert_eq!((approved.status, approved.location.as_deref()), (StatusCode::SEE_OTHER, Some("/moderim")));
    let frame = get(pool.clone(), "/shto/fushat?category=makina").await.body;
    let places = positions(&frame, &[">Volkswagen<", ">Volvo<", ">Zastava<"]);
    assert!(places.is_sorted(), "{places:?}");
    // Its model, with its name put right: "Koral", which search finds it by now too.
    let approved = post_form(pool.clone(), &format!("/moderim/{yugo_id}/mirato"), &[("label", "Koral")], Some(&moderator)).await;
    assert_eq!(approved.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Koral").await, Some(true));
    assert!(get(pool.clone(), &yugo).await.body.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Zastava Koral, 2017</span> · "#));
    assert!(get(pool.clone(), "/search?q=koral").await.body.contains(&yugo));
    assert!(get(pool.clone(), "/c/makina").await.body.contains(">Zastava<"));

    // A name the list has already, or one that can't be a name, is not approved.
    for (label, message) in [("golf", "„golf“ është tashmë në listë: bashkoje me të."), ("x", "Shkruaj vetëm emrin, me 2 deri në 40 shkronja.")] {
        let refused = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/mirato"), &[("label", label)], Some(&moderator)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{label}");
        assert!(refused.body.contains(message), "{label}");
        assert_eq!(option_active(&pool, "Golf IV").await, Some(false), "{label}");
    }

    // Merged, the listing has the listed model, and the typed one is gone.
    let merged = post_form(pool.clone(), &format!("/moderim/{golf_iv_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert_eq!(option_active(&pool, "Golf IV").await, None);
    let page = get(pool.clone(), &golf).await.body;
    assert!(page.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Volkswagen Golf, 2017</span> · "#) && !page.contains(">Golf IV<"));
    // A typed make merged into the listed one takes its typed model along, under the listed make.
    let merged = post_form(pool.clone(), &format!("/moderim/{mercedes_id}/bashko"), &[("into", &mercedes_benz_id)], Some(&moderator)).await;
    assert_eq!(merged.status, StatusCode::SEE_OTHER);
    assert!(get(pool.clone(), &mercedes).await.body.contains(r#"<p class="card-meta listing-kind"><span class="listing-variant">Mercedes-Benz C 220, 2017</span> · "#));
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(r#"<span class="pending-attribute">Modeli · Mercedes-Benz</span>"#));
    assert!(page.contains(r#"<strong class="pending-label">C 220</strong>"#));
    assert!(!page.contains("më parë markën"));
    // No listed model is like it, so the merge asks which, and won't go without one.
    assert!(page.contains(r#"<option value="" selected>Bashkoje me…</option>"#));
    let c220_id = option_id(&pool, "C 220").await;
    let unchosen = post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", "")], Some(&moderator)).await;
    assert_eq!(unchosen.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(unchosen.body.contains("Zgjidh me cilin emër të listës ta bashkosh."));
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    // Not into a model of another make.
    post_form(pool.clone(), &format!("/moderim/{c220_id}/bashko"), &[("into", &golf_id)], Some(&moderator)).await;
    assert_eq!(option_active(&pool, "C 220").await, Some(false));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body;
    assert!(mine.contains("Për miratim (1)"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_makes_and_models_have_the_codes_typing_them_gives(pool: PgPool) {
    // As the posting form codes a typed option (properties.rs), a model's after its make's.
    let code = |name: &str| marketplace::format::slug(name).to_uppercase().replace('-', "_");
    let options: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT o.label, o.value_code, make.value_code
         FROM attribute_options o
         JOIN attributes a ON a.id = o.attribute_id
         LEFT JOIN attribute_options make ON make.id = o.parent_option_id
         WHERE a.code IN ('vehicle_make', 'vehicle_model') AND o.active",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(options.len() > 500, "{}", options.len());
    for (label, value_code, make) in options {
        let expected = match make {
            Some(make) => format!("{make}_{}", code(&label)),
            None => code(&label),
        };
        assert_eq!(value_code, expected, "{label}");
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_select_is_chosen_from_a_list_in_the_page(pool: PgPool) {
    // Each select comes inside the picker (picker_controller.js), which keeps it for the form and
    // offers its options in a list in the page instead of the phone's own picker; a category's
    // list folds, its subcategories under it. (A picker may have a class of its own besides, as the
    // map's towns have.)
    let plain = r#"data-controller="picker">"#;
    let folding = r#"<div class="picker" data-controller="picker" data-picker-tree-value="true">"#;
    let ana = new_seller(&pool, "+355691112233").await;
    // (The form's currency is a two-part toggle of radios, as the filters' price has, since the
    // canvas: a list no longer.)
    let pages = [
        ("/shto", 6),
        ("/shto/fushat?category=makina", 11),
        ("/search?q=golf", 4),
        ("/c/makina", 16),
        ("/harta", 2),
    ];
    // (The price's currency is a two-part toggle of radios, no list.)
    for (uri, selects) in pages {
        let page = get_with_cookie(pool.clone(), uri, &ana).await.body;
        // The typeface panel's two lists (TYPE_TRIAL, base.html) are the browser's own: not the
        // page's forms'.
        let found: Vec<usize> = page
            .match_indices("<select")
            .map(|(at, _)| at)
            .filter(|at| !page[*at..*at + page[*at..].find('>').unwrap()].contains(r#"class="type-font""#))
            .collect();
        assert_eq!(found.len(), selects, "{uri}");
        for at in found {
            let tag = &page[at..at + page[at..].find('>').unwrap()];
            let category = tag.contains(r#"name="category""#);
            let wrapper = if category { folding } else { plain };
            let start = page[..at].rfind(wrapper).unwrap_or_else(|| panic!("{uri}: {tag} has no picker"));
            assert!(page[start + wrapper.len()..at].trim().is_empty(), "{uri}: {tag} isn't its picker's");
        }
    }
}

/// A form sent to the site as one without moderators runs it, where nothing waits for review.
async fn post_form_unmoderated(pool: PgPool, uri: &str, fields: &[(&str, &str)]) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Body::from(body)).unwrap();
    send_to(marketplace::app_with(pool, media(), Moderators::new([])), request).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_posted_without_signing_in_goes_up_by_its_code_or_a_moderator(pool: PgPool) {
    // Not signed in, the form asks for a phone number or an e-mail address as well.
    let form = get(pool.clone(), "/shto").await.body;
    assert!(form.contains(r#"<input id="contact" name="contact" value="" required"#));
    assert!(form.contains("njoftimi publikohet menjëherë"));
    assert!(form.contains("llogaria krijohet vetë"));
    assert!(form.contains("Nëse ke llogari me të, të dërgojmë një kod, dhe njoftimi publikohet sapo ta shkruash."));

    // Where nobody moderates, it goes up at once, with its seller: a new account, active, named
    // as typed. Posting signs no one in: that takes a code, so typing someone's number opens
    // nothing. Its page says it is up, once, to whoever posted it, and how to change it later.
    let started = started();
    let mut fields = listing_fields(("name", "Dorina"), &started);
    fields.push(("contact", "069 777 8899"));
    let posted = post_form_unmoderated(pool.clone(), "/shto", &fields).await;
    let url = posted.location.clone().unwrap();
    assert!(url.ends_with("-samsung-galaxy-s21-128-gb"), "{url}");
    assert!(posted.cookie("session").is_none());
    assert!(get(pool.clone(), "/search?q=galaxy").await.body.contains("Samsung Galaxy S21"));
    let page = get(pool.clone(), &url).await.body;
    assert!(page.contains(r#"<span class="seller-name"><span translate="no">Dorina</span></span>"#));
    let (name, status): (String, String) =
        sqlx::query_as("SELECT display_name, status FROM users WHERE phone_e164 = '+355697778899'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), status.as_str()), ("Dorina", "ACTIVE"));
    let notice = "Njoftimi yt u publikua. Për ta ndryshuar ose fshirë më vonë";
    assert!(!page.contains(notice));
    let posted_cookie = format!("posted={}", posted.cookie("posted").unwrap());
    let shown = get_with_cookie(pool.clone(), &url, &posted_cookie).await;
    assert!(shown.body.contains(notice));
    assert_eq!(shown.cookie("posted"), Some(""), "said once");

    // Where moderators are, a new account's first listing waits for one, out of sight, and the
    // code sent with it puts it up at once: typed, it signs the account in.
    let review = |title: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_as::<_, (String, Option<String>)>("SELECT status, review_reason FROM listings WHERE title = $1")
                .bind(title)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    let waiting = ("PENDING_REVIEW".to_string(), Some("NEW_ACCOUNT".to_string()));
    let up = ("ACTIVE".to_string(), None);
    let mut fields = listing_fields(("name", "Erjona"), &started);
    fields.extend([("title", "Laptop Lenovo ThinkPad X13 me karikues"), ("contact", "069 777 8800")]);
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(posted.location.as_deref(), Some("/hyr/kodi"), "{}", posted.body);
    assert_eq!(review("Laptop Lenovo ThinkPad X13 me karikues").await, waiting);
    assert!(!get(pool.clone(), "/search?q=x13").await.body.contains("Lenovo ThinkPad"));
    let login = format!("login={}", posted.cookie("login").unwrap());
    let asked = get_with_cookie(pool.clone(), "/hyr/kodi", &login).await.body;
    assert!(asked.contains("Shkruaje dhe njoftimi yt publikohet. Pa kodin, e publikon një moderator pasi ta shohë."), "{asked}");
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = '+355697778800'")
        .execute(&pool)
        .await
        .unwrap();
    let typed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    let session = format!("session={}", typed.cookie("session").unwrap());
    assert!(typed.location.as_deref().is_some_and(|to| to.ends_with("-laptop-lenovo-thinkpad-x13-me-karikues")), "{:?}", typed.location);
    assert_eq!(review("Laptop Lenovo ThinkPad X13 me karikues").await, up);
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &session).await.body.contains("Lenovo ThinkPad"));

    // One whose code can't go yet (one went a moment ago) waits for a moderator: its poster sees
    // it waiting, nobody else does, until a moderator puts it up. With no name, the account is
    // "Përdorues".
    sqlx::query("INSERT INTO login_codes (contact, code_hash, expires_at) VALUES ('ana.re@example.com', sha256('x'), now() + interval '10 minutes')")
        .execute(&pool)
        .await
        .unwrap();
    let mut fields = listing_fields(("name", ""), &started);
    fields.extend([("title", "Biçikletë qyteti me marsha"), ("contact", "Ana.Re@Example.com")]);
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    let url = posted.location.clone().unwrap();
    assert!(url.starts_with("/listing/"), "{url}");
    assert_eq!(review("Biçikletë qyteti me marsha").await, waiting);
    let posted_cookie = format!("posted={}", posted.cookie("posted").unwrap());
    let theirs = get_with_cookie(pool.clone(), &url, &posted_cookie).await.body;
    assert!(theirs.contains("Njoftimi yt pret miratimin e një moderatori") && !theirs.contains("Njoftimi yt u publikua"), "{theirs}");
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    let moderator = new_seller(&pool, MODERATOR).await;
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(queue.contains("Llogari e re, pa e konfirmuar numrin me kod") && queue.contains("Biçikletë qyteti me marsha"), "{queue}");
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    post_form(pool.clone(), &format!("/moderim/njoftim/{no}/mirato"), &[], Some(&moderator)).await;
    assert_eq!(review("Biçikletë qyteti me marsha").await, up);
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);
    let name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE email = 'ana.re@example.com'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(name, "Përdorues");

    // A number with an account already: the listing waits, out of sight, for the code that goes
    // to it; typed, the code signs its owner in and puts the listing up under that account,
    // which keeps its name. Typing someone else's number puts nothing up under their name.
    let mut fields = listing_fields(("name", "Dikush tjetër"), &started);
    fields.extend([("title", "Telefon tjetër për shitje"), ("contact", "+355 69 000 0003")]);
    let posted = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(posted.location.as_deref(), Some("/hyr/kodi"), "{}", posted.body);
    assert!(posted.cookie("session").is_none());
    let status = |title: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE title = $1").bind(title).fetch_one(&pool).await.unwrap() }
    };
    assert_eq!(status("Telefon tjetër për shitje").await, "DRAFT");
    assert!(!get(pool.clone(), "/search?q=telefon+tjeter").await.body.contains("Telefon tjetër për shitje"));
    let login = format!("login={}", posted.cookie("login").unwrap());
    let asked = get_with_cookie(pool.clone(), "/hyr/kodi", &login).await.body;
    assert!(asked.contains("Shkruaje dhe njoftimi yt publikohet."), "{asked}");
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = '+355690000003'")
        .execute(&pool)
        .await
        .unwrap();
    let typed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    assert!(typed.cookie("session").is_some());
    assert!(typed.location.as_deref().is_some_and(|to| to.ends_with("-telefon-tjeter-per-shitje")), "{:?}", typed.location);
    assert_eq!(status("Telefon tjetër për shitje").await, "ACTIVE");
    let owner: String = sqlx::query_scalar(
        "SELECT u.display_name FROM listings l JOIN users u ON u.id = l.created_by_user_id
         WHERE l.title = 'Telefon tjetër për shitje' AND l.status = 'ACTIVE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(owner, "Dritan");

    // One never confirmed waits a day, then goes, with the number it showed.
    new_seller(&pool, "+355691112299").await;
    let mut fields = listing_fields(("name", ""), &started);
    fields.extend([("title", "Një tjetër që pret kodin"), ("contact", "069 111 2299")]);
    let waiting = post_form(pool.clone(), "/shto", &fields, None).await;
    assert_eq!(waiting.location.as_deref(), Some("/hyr/kodi"), "{}", waiting.body);
    assert!(marketplace::db::forget_drafts(&pool).await.unwrap().is_empty());
    assert_eq!(status("Një tjetër që pret kodin").await, "DRAFT", "not yet a day");
    sqlx::query("UPDATE listings SET created_at = now() - interval '25 hours' WHERE title = 'Një tjetër që pret kodin'")
        .execute(&pool)
        .await
        .unwrap();
    marketplace::db::forget_drafts(&pool).await.unwrap();
    let contacts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM listing_contacts c JOIN listings l ON l.id = c.listing_id WHERE l.title = 'Një tjetër që pret kodin'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status("Një tjetër që pret kodin").await.as_str(), contacts), ("DELETED", 0));

    // Not under a suspended account, nor without a contact that can be one.
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE phone_e164 = '+355690000003'")
        .execute(&pool)
        .await
        .unwrap();
    for (contact, message) in [("069 000 0003", "Kjo llogari është pezulluar."), ("04 222 3333", "Shkruaj një numër celulari")] {
        let mut fields = listing_fields(("title", "Edhe një telefon"), &started);
        fields.push(("contact", contact));
        let refused = post_form(pool.clone(), "/shto", &fields, None).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY, "{contact}");
        assert!(refused.body.contains(message), "{contact}");
    }
}

/// A moderator gives listing `number` the promotion `product` (/moderim).
async fn promote(pool: &PgPool, moderator: &str, number: &str, product: &str) -> TestResponse {
    post_form(pool.clone(), "/moderim/promovime", &[("njoftimi", number), ("produkti", product)], Some(moderator)).await
}

/// The listings in a page's "Të veçuara", by their numbers, in the row's order.
fn featured_numbers(html: &str) -> Vec<i64> {
    match html.find(r#"<section class="featured-row""#) {
        Some(start) => {
            let row = &html[start..];
            card_numbers(&row[..row.find("</section>").unwrap()])
        }
        None => Vec::new(),
    }
}

/// The listings in a page's results, by their numbers, past the rows over them.
fn result_numbers(html: &str) -> Vec<i64> {
    card_numbers(&html[html.find(r#"<section class="results""#).unwrap()..])
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_promote_listings_which_show_more_until_their_time(pool: PgPool) {
    let moderator = new_seller(&pool, MODERATOR).await;
    let ana = session_for(&pool, ANA_ID).await;
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let updated = |pool: PgPool| async move {
        sqlx::query_scalar::<_, time::OffsetDateTime>("SELECT updated_at FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap()
    };
    let golf_updated = updated(pool.clone()).await;

    // Only a moderator gives one; their page offers those on sale, the number from a listing's page
    // already in its form.
    assert_eq!(promote(&pool, &ana, "1001", "FEATURED_7D").await.status, StatusCode::NOT_FOUND);
    let page = get_with_cookie(pool.clone(), "/moderim?njoftimi=1001", &moderator).await.body;
    assert!(page.contains(r#"<h2 class="moderation-heading" id="promovime">Promovime</h2>"#), "{page}");
    assert!(page.contains(r#"<input id="promotion-number" name="njoftimi" value="1001""#), "{page}");
    for option in [
        r#"<option value="BUMP_ONCE">Ngritje në krye</option>"#,
        r#"<option value="FEATURED_7D">I veçuar për 7 ditë</option>"#,
        r#"<option value="HIGHLIGHT_7D">I theksuar për 7 ditë</option>"#,
        r#"<option value="CATEGORY_TOP_7D">Në krye të kategorisë për 7 ditë</option>"#,
    ] {
        assert!(page.contains(option), "{option}");
    }
    assert!(page.contains("Asnjë promovim tani."));
    // The seller's page leads to one, a moderator's to giving one.
    assert!(get_with_cookie(pool.clone(), golf, &ana).await.body.contains(r#"<p class="promotion-note"><a href="/promovo?njoftimi=1001">Promovoje</a> që ta shohin më shumë.</p>"#));
    assert!(get_with_cookie(pool.clone(), golf, &moderator).await.body.contains(r#"<a href="/moderim?njoftimi=1001#promovime">Jep një promovim</a>"#));
    assert!(!get(pool.clone(), golf).await.body.contains("promotion-note"));
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(featured_numbers(&cars).is_empty() && !cars.contains("featured-row"));

    // Featured: in the row over the results of its category and of the searches it matches, and
    // nowhere else; on /moderim with until when; its seller hears of it and sees it.
    let given = promote(&pool, &moderator, "1001", "FEATURED_7D").await;
    assert_eq!(given.location.as_deref(), Some("/moderim#promovime"));
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert_eq!(featured_numbers(&cars), vec![1001]);
    assert!(cars.contains(r#"<article class="card" id="featured-1001">"#), "{cars}");
    assert!(cars.contains(r#"<a class="row-note" href="/promovo">Të promovuara · <span class="row-note-how">Si?</span></a>"#));
    assert_eq!(result_numbers(&cars), vec![1001, 1002]);
    assert_eq!(featured_numbers(&get(pool.clone(), "/search?q=golf").await.body), vec![1001]);
    assert!(featured_numbers(&get(pool.clone(), "/search?q=mercedes").await.body).is_empty());
    assert!(featured_numbers(&get(pool.clone(), "/c/elektronike").await.body).is_empty());
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(r#"<span class="pending-attribute">I veçuar për 7 ditë</span>"#) && page.contains(&format!(r#"<a class="pending-label" href="{golf}">"#)), "{page}");
    assert!(page.contains("Nr. 1001 · deri më "));
    let mine = get_with_cookie(pool.clone(), golf, &ana).await.body;
    assert!(mine.contains(r#"<p class="promotion-note">I veçuar për 7 ditë, deri më "#), "{mine}");
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<span class="badge promoted">I veçuar për 7 ditë, deri më "#));
    assert!(get_with_cookie(pool.clone(), "/lajmerimet", &ana).await.body.contains("I veçuar për 7 ditë: Volkswagen Golf 7, 2016, naftë"));
    // Given again, its time adds up.
    promote(&pool, &moderator, "1001", "FEATURED_7D").await;
    let days: f64 = sqlx::query_scalar("SELECT extract(epoch FROM featured_until - now())::float8 / 86400 FROM listings WHERE public_no = 1001")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!((13.99..=14.0).contains(&days), "{days}");

    // Highlighted: its card stands out wherever it shows.
    promote(&pool, &moderator, "1002", "HIGHLIGHT_7D").await;
    let cars = get(pool.clone(), "/c/makina").await.body;
    assert!(cars.contains(r#"<article class="card is-highlighted" id="listing-1002">"#), "{cars}");
    assert!(cars.contains(r#"<article class="card" id="listing-1001">"#));

    // A bump takes it to the top of "newest first" at once, spent there and then.
    promote(&pool, &moderator, "1002", "BUMP_ONCE").await;
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1002, 1001]);
    // At the top of its category for a week: there now, and back once a day has gone by.
    promote(&pool, &moderator, "1001", "CATEGORY_TOP_7D").await;
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001, 1002]);
    // Giving them moved no listing's updated_at, which the sitemaps give search engines.
    assert_eq!(updated(pool.clone()).await, golf_updated);
    sqlx::query("UPDATE listings SET published_at = now() - interval '3 days', bumped_at = now() - interval '25 hours' WHERE public_no = 1001").execute(&pool).await.unwrap();
    let golf_updated = updated(pool.clone()).await;
    sqlx::query("UPDATE listings SET bumped_at = now() - interval '1 hour' WHERE public_no = 1002").execute(&pool).await.unwrap();
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1002, 1001]);
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (0, 1));
    assert_eq!(result_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001, 1002]);
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (0, 0));

    // Ended by a moderator, it gives no more; the other of its kind on it still does, until it is
    // ended too.
    let featured: Vec<String> = sqlx::query_scalar(
        "SELECT p.id::text FROM listing_promotions p JOIN promotion_products pr ON pr.id = p.promotion_product_id
         WHERE pr.code = 'FEATURED_7D' AND p.status = 'ACTIVE' ORDER BY p.ends_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(featured.len(), 2);
    let page = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(page.contains(&format!(r#"<form method="post" action="/moderim/promovime/{}/ndalo""#, featured[1])));
    let ended = post_form(pool.clone(), &format!("/moderim/promovime/{}/ndalo", featured[1]), &[], Some(&moderator)).await;
    assert_eq!(ended.location.as_deref(), Some("/moderim#promovime"));
    assert_eq!(featured_numbers(&get(pool.clone(), "/c/makina").await.body), vec![1001]);
    post_form(pool.clone(), &format!("/moderim/promovime/{}/ndalo", featured[0]), &[], Some(&moderator)).await;
    assert!(featured_numbers(&get(pool.clone(), "/c/makina").await.body).is_empty());
    let left: Option<time::OffsetDateTime> = sqlx::query_scalar("SELECT featured_until FROM listings WHERE public_no = 1001").fetch_one(&pool).await.unwrap();
    assert_eq!(left, None);

    // Its time up, it ends.
    sqlx::query(
        "UPDATE listing_promotions p SET starts_at = now() - interval '8 days', ends_at = now() - interval '1 minute'
         FROM promotion_products pr WHERE pr.id = p.promotion_product_id AND pr.code = 'HIGHLIGHT_7D'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE listings SET highlighted_until = now() - interval '1 minute' WHERE public_no = 1002").execute(&pool).await.unwrap();
    assert!(!get(pool.clone(), "/c/makina").await.body.contains("is-highlighted"));
    assert_eq!(marketplace::db::tick_promotions(&pool).await.unwrap(), (1, 0));

    // A number no listing up has, or no promotion chosen, is said beside the form.
    let refused = promote(&pool, &moderator, "999999", "FEATURED_7D").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Asnjë njoftim në faqe nuk ka numrin „999999“.") && refused.body.contains(r#"value="999999" inputmode="numeric" required aria-invalid="true""#), "{}", refused.body);
    let refused = promote(&pool, &moderator, "1001", "GJITHCKA").await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Zgjidh një nga promovimet."));

    // The moderators' record keeps each, and neither the ends nor the day's top moved updated_at.
    let actions: Vec<(String, i64)> = sqlx::query_as(
        "SELECT action_type, count(*) FROM moderation_actions WHERE action_type LIKE '%PROMOTION' GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, vec![("END_PROMOTION".to_string(), 2), ("GRANT_PROMOTION".to_string(), 5)]);
    assert_eq!(updated(pool.clone()).await, golf_updated);

    // /promovo says what each does and costs, and how to get one.
    let about = get(pool.clone(), "/promovo?njoftimi=1001").await.body;
    assert!(about.contains("<strong>I veçuar për 7 ditë</strong>") && about.contains("8,99\u{a0}€"), "{about}");
    assert!(about.contains(r#"<a href="/kontakt">Kontakt</a> me numrin e njoftimit (<strong>1001</strong>)"#), "{about}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn moderators_review_listings_with_anothers_photos_and_suspend_accounts(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let moderator = new_seller(&pool, MODERATOR).await;
    let started = started();
    let status = |title: &'static str| {
        let pool = pool.clone();
        async move { sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE title = $1").bind(title).fetch_one(&pool).await.unwrap() }
    };
    let post = |cookie: String, title: &'static str, photo: Vec<u8>| {
        let pool = pool.clone();
        let started = started.clone();
        async move {
            let fields = listing_fields(("title", title), &started);
            let posted = post_with_photos(pool, "/shto", &fields, &[photo], &cookie).await;
            posted.location.unwrap_or_else(|| panic!("{title}: {}", posted.body))
        }
    };

    // Ana's photo, then the very same file on Besa's listing: hers waits for a moderator, which
    // she sees on its page and "Njoftimet e mia"; nobody else sees it. Ana posting it again is
    // nothing new.
    let photo = jpeg(64, 48);
    let first = post(ana.clone(), "Samsung Galaxy S21, i pari", photo.clone()).await;
    let copied = post(besa.clone(), "Samsung Galaxy S21, i kopjuari", photo.clone()).await;
    post(ana.clone(), "Samsung Galaxy S21, përsëri", photo.clone()).await;
    assert_eq!(status("Samsung Galaxy S21, i pari").await, "ACTIVE");
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "PENDING_REVIEW");
    assert_eq!(status("Samsung Galaxy S21, përsëri").await, "ACTIVE");
    assert!(get_with_cookie(pool.clone(), &copied, &besa).await.body.contains("Njoftimi yt pret miratimin e një moderatori"));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body.contains("Në pritje"));
    assert_eq!(get(pool.clone(), &copied).await.status, StatusCode::NOT_FOUND);
    // Besa's other listing, with a photo of her own, goes up.
    let laptop = post(besa.clone(), "Laptop i Besës", jpeg(70, 50)).await;
    assert_eq!(status("Laptop i Besës").await, "ACTIVE");

    // The moderators see why it waits, and the listing it took the photo from; refused, Besa
    // reads why. Nobody else may.
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    let first_no = first.trim_start_matches("/listing/").split('-').next().unwrap();
    assert!(queue.contains("Foto të njëjta me njoftimin e një shitësi tjetër") && queue.contains(&format!(">nr. {first_no}</a>")), "{queue}");
    let no = copied.trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    assert_eq!(post_form(pool.clone(), &format!("/moderim/njoftim/{no}/refuzo"), &[("arsye", "foto")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    post_form(pool.clone(), &format!("/moderim/njoftim/{no}/refuzo"), &[("arsye", "foto")], Some(&moderator)).await;
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "REJECTED");
    let told = get_with_cookie(pool.clone(), &copied, &besa).await.body;
    assert!(told.contains("Moderatorët e hoqën këtë njoftim nga faqja: Fotot janë marrë nga një njoftim tjetër."), "{told}");

    // Suspended, Besa is signed out and her listing leaves the site; the moderators have her
    // with "Rikthe", which brings it back, and leaves the refused one refused.
    let besa_id = account_of(&besa).to_string();
    assert_eq!(post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/pezullo"), &[("arsye", "mashtrim")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
    post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/pezullo"), &[("arsye", "mashtrim")], Some(&moderator)).await;
    assert_eq!(status("Laptop i Besës").await, "SUSPENDED");
    // Its page says why to the moderators, the account's suspension rather than reports.
    let seen = get_with_cookie(pool.clone(), &laptop, &moderator).await.body;
    assert!(seen.contains("Ky njoftim nuk shfaqet në faqe: llogaria e shitësit është pezulluar."), "{seen}");
    assert!(!seen.contains("disa njerëz e raportuan"));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &besa).await.body.contains(r#"action="/hyr""#));
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    assert!(queue.contains("Llogari të pezulluara") && queue.contains("Mashtrim · ") && queue.contains(&format!("/moderim/perdorues/{besa_id}/rikthe")), "{queue}");
    post_form(pool.clone(), &format!("/moderim/perdorues/{besa_id}/rikthe"), &[], Some(&moderator)).await;
    assert_eq!(status("Laptop i Besës").await, "ACTIVE");
    assert_eq!(status("Samsung Galaxy S21, i kopjuari").await, "REJECTED");
    let account: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1::uuid").bind(&besa_id).fetch_one(&pool).await.unwrap();
    assert_eq!(account, "ACTIVE");
    // A moderator doesn't suspend themself.
    let own = account_of(&moderator).to_string();
    post_form(pool.clone(), &format!("/moderim/perdorues/{own}/pezullo"), &[("arsye", "spam")], Some(&moderator)).await;
    let theirs: String = sqlx::query_scalar("SELECT status FROM users WHERE id = $1::uuid").bind(&own).fetch_one(&pool).await.unwrap();
    assert_eq!(theirs, "ACTIVE");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_waiting_for_a_moderator_can_be_deleted_by_its_seller(pool: PgPool) {
    // Waiting for a moderator, its page offers its seller "Fshi", and deleting it works (it gave
    // "not found" once: the waiting status wasn't among those a delete takes). Their own page
    // offers no "Shfaq numrin": the number is theirs.
    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    sqlx::query("UPDATE listings SET status = 'PENDING_REVIEW', review_reason = 'NEW_ACCOUNT' WHERE public_no = 1005")
        .execute(&pool)
        .await
        .unwrap();
    let page = get_with_cookie(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri", &ana).await.body;
    assert!(page.contains(r#"<form method="post" action="/listing/1005/fshi""#), "{page}");
    assert!(!page.contains("contact-bar"), "{page}");
    let deleted = post_form(pool.clone(), "/listing/1005/fshi", &[], Some(&ana)).await;
    assert_eq!(deleted.status, StatusCode::SEE_OTHER, "{}", deleted.body);
    let status: String = sqlx::query_scalar("SELECT status FROM listings WHERE public_no = 1005").fetch_one(&pool).await.unwrap();
    assert_eq!(status, "DELETED");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn posting_without_signing_in_is_limited_per_network(pool: PgPool) {
    // Twenty an hour from one network (IP address), each under a new number; the next waits.
    let started = started();
    for i in 0..21 {
        let (title, phone) = (format!("Njoftim nga i njëjti rrjet {i}"), format!("069 700 {i:04}"));
        let mut fields = listing_fields(("title", &title), &started);
        fields.push(("contact", &phone));
        let response = post_form_from(pool.clone(), "/shto", &fields, "203.0.113.9").await;
        if i < 20 {
            assert_eq!(response.status, StatusCode::SEE_OTHER, "{i}: {}", response.body);
        } else {
            assert_eq!(response.status, StatusCode::UNPROCESSABLE_ENTITY);
            assert!(response.body.contains("Shumë njoftime nga ky rrjet brenda një ore."));
        }
    }
    // Another network posts as before, and so does someone signed in on the first.
    let mut fields = listing_fields(("title", "Nga një rrjet tjetër"), &started);
    fields.push(("contact", "069 701 0000"));
    assert_eq!(post_form_from(pool.clone(), "/shto", &fields, "203.0.113.10").await.status, StatusCode::SEE_OTHER);
    let ana = new_seller(&pool, "+355691112233").await;
    let fields = listing_fields(("title", "E Anës, e hyrë në llogari"), &started);
    let signed_in = send(
        pool.clone(),
        Request::post("/shto")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header("x-forwarded-for", "203.0.113.9")
            .header(header::COOKIE, ana.as_str())
            .body(Body::from(form_urlencoded::Serializer::new(String::new()).extend_pairs(&fields).finish()))
            .unwrap(),
    )
    .await;
    assert_eq!(signed_in.status, StatusCode::SEE_OTHER, "{}", signed_in.body);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_listing_shows_its_seller_and_their_profile_lists_their_listings(pool: PgPool) {
    // A private seller: Ana, with the five fixture listings she sells.
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    let ana = "/perdorues/11111111-1111-4111-8111-111111111111";
    // The category above the photo; the title under it; then the deal, the price in its box, on a
    // car's page the seller's card, her initial, her name, since when she is on the site and after
    // a dot how many listings, leading to her profile, and in the card the contact bar, the phone
    // and the message, wide pills (contact_bar.html; under the price on every other page), and
    // the share, a link in a circle, and the report, beside the message (listing.css); then the
    // description and the details; then her other listings.
    let order = positions(&page, &[
        r#"<nav class="breadcrumb listing-path""#,
        r#"<div class="gallery">"#,
        "<h1>Volkswagen Golf 7, 2016, naftë</h1>",
        r#"<p class="card-meta listing-kind">Në shitje · E përdorur · "#,
        r#"<dl class="key-facts">"#,
        r#"<div class="listing-deal""#,
        r#"<div class="price-box">"#,
        r#"<p class="listing-price">"#,
        r#"<section class="seller-section" aria-labelledby="seller-heading">"#,
        r#"<h2 id="seller-heading">Shitësi</h2>"#,
        r#"<div class="seller-card">"#,
        &format!(r#"<a class="seller" href="{ana}">"#),
        r#"<span class="avatar large" aria-hidden="true">A</span>"#,
        r#"<span class="seller-name"><span translate="no">Ana</span></span>"#,
        r#"<span class="seller-summary"><span class="seller-since">Që nga "#,
        r#"</span> <span class="seller-count">· 5 njoftime</span></span>"#,
        r#"<aside class="contact-bar" aria-label="Kontakti" data-controller="sticky">"#,
        r#"<form id="phone-reveal" method="post""#,
        "</aside>",
        r#"<div class="actions">"#,
        r#"aria-label="Shpërndaje" title="Shpërndaje">"#,
        "<h2>Përshkrimi</h2>",
        "<h2>Detajet</h2>",
    ]);
    assert!(order.is_sorted(), "{order:?}");
    // The description can fold past a dozen lines (fold_controller.js): its button waits hidden,
    // and nothing is folded in the page itself.
    assert!(page.contains(r#"<div class="description-fold" data-controller="fold">"#));
    assert!(page.contains(r#"<button type="button" class="description-more" data-fold-target="button" data-action="fold#open" hidden>"#));
    assert!(page.contains("Shfaq të gjithë përshkrimin") && !page.contains("is-folded"));
    // The bar is under the seller there, the page's one bar; on every other page it is under the
    // price, the seller after the description without it, said the same way: the initial in a
    // ring of 48 px, since when, and how many listings.
    let card = &page[page.find(r#"<section class="seller-section""#).unwrap()..];
    let card = &card[..card.find("</section>").unwrap()];
    assert!(card.contains(r#"<aside class="contact-bar""#) && page.matches(r#"<aside class="contact-bar""#).count() == 1, "{card}");
    assert!(card.contains(r#"<div class="actions">"#) && page.matches(r#"<div class="actions">"#).count() == 1, "the share and the report in the card: {card}");
    let iphone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    let card = &iphone[iphone.find(r#"<section class="seller-section""#).unwrap()..];
    let card = &card[..card.find("</section>").unwrap()];
    assert!(card.contains(r#"<span class="avatar large" aria-hidden="true">A</span>"#) && !card.contains("contact-bar"), "{card}");
    assert!(card.contains(r#"<span class="seller-summary"><span class="seller-since">Që nga "#) && card.contains(r#"<span class="seller-count">· 5 njoftime</span>"#), "{card}");
    assert!(iphone.find(r#"<aside class="contact-bar""#).unwrap() < iphone.find(r#"<section class="seller-section""#).unwrap());

    // Her profile says so too.
    let profile = get(pool.clone(), ana).await.body;
    assert!(profile.contains(r#"<h1 translate="no">Ana</h1>"#));
    assert!(profile.contains("Në Pluribus që nga ") && profile.contains(" · 5 njoftime</p>"));
    assert_eq!(card_numbers(&profile).len(), 5);
    assert!(profile.contains("Volkswagen Golf 7, 2016, naftë"));
    assert!(!profile.contains("Mercedes-Benz C 220"), "Auto Durrësi's, not Ana's");

    // A business: its listings, on its own page.
    let page = get(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.body;
    assert!(page.contains(r#"<a class="seller" href="/biznes/auto-durresi">"#));
    // A business says so on the since line of a car's card.
    assert!(page.contains(r#"<span class="seller-summary"><span class="seller-since">Biznes · që nga "#) && page.contains(r#"</span> <span class="seller-count">· 1 njoftim</span></span>"#), "{page}");
    let business = get(pool.clone(), "/biznes/auto-durresi").await.body;
    assert!(business.contains(r#"<h1 translate="no">Auto Durrësi</h1>"#) && business.contains("Biznes · "));
    assert_eq!(card_numbers(&business), vec![1002]);

    // No such seller, or one not yet confirmed.
    assert_eq!(get(pool.clone(), "/perdorues/not-a-uuid").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(pool.clone(), "/biznes/nobody").await.status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE users SET status = 'PENDING' WHERE display_name = 'Ana'").execute(&pool).await.unwrap();
    assert_eq!(get(pool, ana).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_category_pages_way_up_keeps_the_words_and_the_town(pool: PgPool) {
    // Over a main category the way up is the welcome page; its subcategories, the pills under its
    // heading, keep the town and the words, and so does the way up from one of them.
    let page = html(pool.clone(), "/c/prona?city=durres&q=shtepi").await;
    assert!(page.contains(r#"<a class="breadcrumb-up" href="/">Kreu</a>"#));
    assert!(page.contains(r#"<a class="shortcut" href="/c/apartamente?q=shtepi&city=durres">"#));
    let flats = html(pool.clone(), "/c/apartamente?city=durres&q=shtepi").await;
    assert!(flats.contains(r#"<a class="breadcrumb-up" href="/c/prona?q=shtepi&city=durres">Prona</a>"#), "{flats}");
    // A search page's heading is just a heading.
    assert!(!get(pool, "/search?q=golf").await.body.contains(r#"<nav class="breadcrumb""#));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_category_page_says_where_it_is_in_a_breadcrumb(pool: PgPool) {
    // Makina: a line of words, back first (leading a level up without scripts), then the way up to
    // Automjete, quieter, then Makina, the page's own; its heading under it; no pills under, as it
    // has no subcategories; and the trail for search engines too (schema.org's BreadcrumbList).
    let cars = html(pool.clone(), "/c/makina").await;
    let order = positions(&cars, &[
        r#"<nav class="breadcrumb" aria-label="Rruga">"#,
        r#"<a class="breadcrumb-back" href="/c/automjete" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#,
        r#"<a class="breadcrumb-up" href="/c/automjete">Automjete</a>"#,
        r#"<span class="breadcrumb-sep" aria-hidden="true">›</span>"#,
        r#"<span class="breadcrumb-here" aria-current="page">Makina</span>"#,
        r#"<h1 class="browse-title">Makina</h1>"#,
        r#"<p class="result-count" id="result-count""#,
        r#"<div class="browse-tools""#,
        r#"<details class="sort-menu cards-menu""#,
    ]);
    assert!(order.is_sorted(), "{order:?}");
    assert!(!cars.contains("category-switch") && !cars.contains("category-level") && !cars.contains("Faqja kryesore"));
    let head = &cars[cars.find(r#"<nav class="breadcrumb""#).unwrap()..cars.find(r#"<div class="browse-tools""#).unwrap()];
    assert!(!head.contains(r#"<a class="shortcut""#), "no subcategories, no pills: {head}");
    assert!(cars.contains(r#""@type":"BreadcrumbList""#));
    assert!(cars.contains(r#"{"@type":"ListItem","item":"http://127.0.0.1:3000/c/makina","name":"Makina","position":3}"#), "{cars}");
    // Three levels down, the way up is the category above alone.
    let gps = html(pool.clone(), "/c/gps-dhe-navigatore").await;
    assert!(gps.contains(r#"<a class="breadcrumb-back" href="/c/pjese-dhe-aksesore" aria-label="Kthehu""#));
    assert!(gps.contains(r#"<a class="breadcrumb-up" href="/c/pjese-dhe-aksesore">Pjesë / aksesorë</a>"#));
    // A main category's way up is the welcome page, "Kreu"; its subcategories stand under its
    // heading. A search's heading stays a heading, without either trail.
    let vehicles = html(pool.clone(), "/c/automjete").await;
    assert!(vehicles.contains(r#"<a class="breadcrumb-back" href="/" aria-label="Kthehu" title="Kthehu" data-controller="back" data-action="back#go">"#), "back to the welcome page without scripts");
    assert!(vehicles.contains(r#"<a class="breadcrumb-up" href="/">Kreu</a>"#));
    let at = positions(&vehicles, &[
        r#"<h1 class="browse-title">Automjete</h1>"#,
        r#"<nav class="subcategories" aria-label="Nënkategoritë">"#,
        r#"<a class="shortcut" href="/c/makina"><svg class="shortcut-icon""#,
    ]);
    assert!(at.is_sorted(), "{at:?}");
    let search = html(pool, "/search?q=golf").await;
    assert!(search.contains(r#"<h1 class="browse-title">Rezultatet për „golf“</h1>"#) && !search.contains(r#"<nav class="breadcrumb""#) && !search.contains("BreadcrumbList"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_profile_searches_its_sellers_listings(pool: PgPool) {
    let ana = "/perdorues/11111111-1111-4111-8111-111111111111";
    let profile = get(pool.clone(), ana).await.body;
    assert!(profile.contains(&format!(r#"<form class="profile-search" action="{ana}" method="get" role="search">"#)));

    // Only Ana's: the golf is hers, the Mercedes (also a car) is Auto Durrësi's.
    let found = get(pool.clone(), &format!("{ana}?q=golf")).await.body;
    assert_eq!(card_numbers(&found), vec![1001]);
    assert!(found.contains("1 njoftim për „golf“"));
    assert!(found.contains(r#"value="golf""#));
    // Misspelt, it still finds the golf.
    assert_eq!(card_numbers(&get(pool.clone(), &format!("{ana}?q=golff")).await.body), vec![1001]);
    let none = get(pool, &format!("{ana}?q=mercedes")).await.body;
    assert!(card_numbers(&none).is_empty() && none.contains("0 njoftime për „mercedes“"));
}

/// What a listing's page sends once it shows (application.js): it was seen, from this address and
/// browser.
fn seen(no: i64, ip: &str, agent: &str) -> Request<Body> {
    Request::post(format!("/listing/{no}/pare"))
        .header("x-forwarded-for", ip)
        .header(header::USER_AGENT, agent)
        .body(Body::empty())
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_most_seen_listings_have_a_row_on_the_welcome_page(pool: PgPool) {
    // Seen by nobody yet: no row.
    assert!(!html(pool.clone(), "/").await.contains("Më të klikuarat"));

    // A listing's page asks to count the view, but not its seller's.
    let page = get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    assert!(page.contains(r#"data-seen-url="/listing/1001/pare""#));
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let own = get_with_cookie(pool.clone(), &format!("/listing/{no}-samsung-galaxy-s21-128-gb"), &ana).await.body;
    assert!(own.contains("Ndrysho") && !own.contains("data-seen-url"));

    // 1002 seen by three visitors, one of them twice, which counts once; 1001 by two; a bot's
    // visit and a number no listing has count for nothing.
    let app = app(pool.clone());
    let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1";
    for (no, ip) in [(1002, "203.0.113.1"), (1002, "203.0.113.1"), (1002, "203.0.113.2"), (1002, "203.0.113.3"),
                     (1001, "203.0.113.1"), (1001, "203.0.113.2"), (999_999, "203.0.113.1")] {
        assert_eq!(send_to(app.clone(), seen(no, ip, safari)).await.status, StatusCode::NO_CONTENT);
    }
    send_to(app.clone(), seen(1003, "203.0.113.4", "Mozilla/5.0 (compatible; Googlebot/2.1)")).await;
    let views = |no: i64| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT sum(s.detail_views)::bigint FROM listing_daily_stats s JOIN listings l ON l.id = s.listing_id WHERE l.public_no = $1",
            )
            .bind(no)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!((views(1002).await, views(1001).await, views(1003).await), (Some(3), Some(2), None));

    // The row, over the newest, the most seen first, each saying how many times it was seen, and
    // none that nobody saw.
    let home = html(pool.clone(), "/").await;
    let row = section(&home, "most-viewed-heading");
    assert_eq!(card_numbers(row), vec![1002, 1001]);
    // A row of cards (card_row.html), its cards' ids its own; the cards of the grid's, small,
    // those of the browse pages.
    assert!(row.contains(r#"<div class="card-row"><article class="card" id="most-viewed-1002">"#), "{row}");
    // Two cards are as wide as every row's, 236px (180px on phones), and their photos are fetched
    // for it; the first one's before anything else on the page.
    assert!(row.contains(&format!(r#"sizes="{ROW_SIZES}""#)), "{row}");
    assert_eq!(home.matches(r#"fetchpriority="high""#).count(), 1);
    let first = &row[row.find(r#"<article class="card""#).unwrap()..];
    assert!(first[..first.find("</article>").unwrap()].contains(r#"fetchpriority="high""#), "{row}");
    assert!(row.contains(r#"<h2 class="card-title">"#) && row.contains(r#"<p class="card-when">"#), "{row}");
    assert!(row.contains(r#"<span class="card-views">3 shikime këtë javë</span><span class="card-views-short">3 shikime</span>"#), "{row}");
    assert!(row.contains(r#"<span class="card-views">2 shikime këtë javë</span>"#), "{row}");
    assert!(!home[home.find(r#"id="latest-heading""#).unwrap()..].contains("card-views"));

    // The listing's own page says how many times it was seen in all, under the heart; one nobody
    // saw says nothing.
    let page = get(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.body;
    assert!(page.contains(r#"<p class="listing-views" title="3 shikime">"#), "{page}");
    let unseen = get(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await.body;
    assert!(unseen.contains("listing-meta") && !unseen.contains("listing-views"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_main_categories_and_a_few_big_ones_have_their_own_most_seen(pool: PgPool) {
    // The apartment and the house in their own subcategories of Prona, the iPhone among the phones.
    for (slug, no) in [("apartamente", 1003), ("vila-dhe-shtepi", 1004), ("telefona", 1005)] {
        sqlx::query("UPDATE listings SET category_id = (SELECT id FROM categories WHERE slug = $1) WHERE public_no = $2")
            .bind(slug)
            .bind(no)
            .execute(&pool)
            .await
            .unwrap();
    }
    // The Mercedes seen by two visitors, the Golf, the apartment, the house and the iPhone by one.
    let app = app(pool.clone());
    let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) Safari/604.1";
    for (no, ip) in [(1002, "203.0.113.1"), (1002, "203.0.113.2"), (1001, "203.0.113.1"), (1003, "203.0.113.1"),
                     (1004, "203.0.113.1"), (1005, "203.0.113.1")] {
        assert_eq!(send_to(app.clone(), seen(no, ip, safari)).await.status, StatusCode::NO_CONTENT);
    }
    // Each main category's page has its own, and each big subcategory's, the most seen first, under
    // the page's buttons...
    let row = |page: &str| {
        let mut numbers = card_numbers(section(page, "most-viewed-heading"));
        numbers.sort();
        numbers
    };
    let vehicles = html(pool.clone(), "/c/automjete").await;
    assert_eq!(card_numbers(section(&vehicles, "most-viewed-heading")), [1002, 1001]);
    let (buttons, seen, results) = (vehicles.find(r#"class="browse-tools""#), vehicles.find("Më të klikuarat"), vehicles.find(r#"class="results""#));
    assert!(buttons < seen && seen < results, "under the buttons, over the results");
    // The listings under it, headed by their order; a search across the categories by the order
    // alone.
    assert!(vehicles.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të reja</h2>"#));
    let cheapest = html(pool.clone(), "/c/automjete?sort=price_asc").await;
    assert!(cheapest.contains(r#"<h2 class="results-heading" id="results-heading">Njoftimet më të lira</h2>"#));
    let search = html(pool.clone(), "/search?q=golf").await;
    assert!(search.contains(r#"<h2 class="results-heading" id="results-heading">Më të përshtatshmet</h2>"#));
    assert_eq!(row(&html(pool.clone(), "/c/prona").await), [1003, 1004]);
    assert_eq!(row(&html(pool.clone(), "/c/elektronike").await), [1005]);
    assert_eq!(row(&html(pool.clone(), "/c/makina").await), [1001, 1002]);
    assert_eq!(row(&html(pool.clone(), "/c/apartamente").await), [1003]);
    assert_eq!(row(&html(pool.clone(), "/c/vila-dhe-shtepi").await), [1004]);
    // The welcome page's has all five, the next card showing at the row's edge: its photos are
    // fetched as wide as a row's cards, narrower than a column of the grid (ROW_SIZES).
    let home = html(pool.clone(), "/").await;
    let all = section(&home, "most-viewed-heading");
    assert_eq!(all.matches(r#"<article class="card""#).count(), 5, "{all}");
    assert!(all.contains(&format!(r#"sizes="{ROW_SIZES}""#)) && !all.contains(SMALL_SIZES), "{all}");
    // A round button beside the view menu folds it away, pressed while it shows; the cookie it
    // sets (most_viewed_controller.js) keeps it folded on the next pages, where it stays in the
    // page, hidden, its photos waiting, and the results' first photo comes first instead.
    assert!(vehicles.contains(r#"class="button secondary round-button most-viewed-button" aria-controls="most-viewed" aria-pressed="true""#));
    assert!(vehicles.contains(r#"<section class="most-viewed" id="most-viewed" data-controller="drag-row" aria-labelledby="most-viewed-heading">"#));
    assert!(section(&vehicles, "most-viewed-heading").contains(r#"fetchpriority="high""#));
    let folded = get_with_cookie(pool.clone(), "/c/automjete", "most-viewed=hidden").await.body;
    assert!(folded.contains(r#"aria-controls="most-viewed" aria-pressed="false""#));
    assert!(folded.contains(r#"<section class="most-viewed" id="most-viewed" hidden data-controller="drag-row""#));
    let row_html = section(&folded, "most-viewed-heading");
    let lazy = |html: &str| html.matches(r#"loading="lazy""#).count();
    assert!(!row_html.contains(r#"fetchpriority="high""#), "{row_html}");
    assert!(lazy(row_html) > lazy(section(&vehicles, "most-viewed-heading")), "{row_html}");
    assert_eq!(folded.matches(r#"fetchpriority="high""#).count(), 1);
    assert!(section(&folded, "results-heading").contains(r#"fetchpriority="high""#));
    // ...but no other subcategory's, nor a page with a filter or words on, which it would take no
    // notice of.
    assert!(!html(pool.clone(), "/c/telefona").await.contains("Më të klikuarat"));
    assert!(!html(pool.clone(), "/c/makina?vehicle_fuel=DIESEL").await.contains("Më të klikuarat"));
    assert!(!html(pool, "/c/makina?q=golf").await.contains("Më të klikuarat"));
}

/// A request as one comes through the tunnel: to the site's name, over https.
fn from_the_site(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::HOST, "pluribus.test")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap()
}

/// The address the site's links start with (web/seo.rs): PUBLIC_URL when .env sets it, or the one
/// a request from_the_site came to.
fn site() -> String {
    std::env::var("PUBLIC_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| "https://pluribus.test".to_string())
}

/// What the page's meta tag with this property (Open Graph's) or name says.
fn meta_tag<'a>(html: &'a str, name: &str) -> Option<&'a str> {
    ["property", "name"].iter().find_map(|attribute| {
        let tag = format!(r#"<meta {attribute}="{name}" content=""#);
        let start = html.find(&tag)? + tag.len();
        Some(&html[start..start + html[start..].find('"')?])
    })
}

/// The schema.org data the page gives search engines, if any.
fn json_ld(html: &str) -> Option<Value> {
    let tag = r#"<script type="application/ld+json">"#;
    let start = html.find(tag)? + tag.len();
    Some(serde_json::from_str(&html[start..start + html[start..].find("</script>")?]).unwrap())
}

/// The addresses a sitemap lists, in its order.
fn locations(xml: &str) -> Vec<String> {
    xml.split("<loc>").skip(1).map(|rest| rest[..rest.find("</loc>").unwrap()].to_string()).collect()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_shared_listing_shows_its_title_price_and_photo(pool: PgPool) {
    let url = format!("{}/listing/1002-mercedes-benz-c-220-2018-automatike", site());
    let page = send(pool.clone(), from_the_site("/listing/1002-mercedes-benz-c-220-2018-automatike")).await.body.replace("&#38;", "&");
    // What WhatsApp, Facebook and the rest show of a link to it: the title and the price; where it
    // is, what kind of listing, in what condition, and its description; its first photo.
    assert_eq!(meta_tag(&page, "og:title"), Some("Mercedes-Benz C 220, 2018, automatike · 21\u{a0}500\u{a0}€ (i diskutueshëm)"));
    let description = "Durrës · Në shitje · E përdorur — Importuar nga Gjermania, me servis të plotë. Garanci 6 muaj.";
    assert_eq!(meta_tag(&page, "og:description"), Some(description));
    assert_eq!(meta_tag(&page, "description"), Some(description));
    let photo = "https://images.unsplash.com/photo-1624085568108-36410cfe4d24?auto=format&fit=crop&w=1200&h=900&q=75";
    assert_eq!(meta_tag(&page, "og:image"), Some(photo));
    assert_eq!(meta_tag(&page, "og:type"), Some("product"));
    assert_eq!(meta_tag(&page, "og:url"), Some(url.as_str()));
    assert_eq!(meta_tag(&page, "product:price:amount"), Some("21500"));
    assert_eq!(meta_tag(&page, "product:price:currency"), Some("EUR"));
    assert!(page.contains(&format!(r#"<link rel="canonical" href="{url}">"#)));
    assert!(!page.contains("noindex"));
    // To Google, a product and its offer.
    let product = json_ld(&page).expect("no schema.org data");
    assert_eq!(product["@type"], "Product");
    assert_eq!(product["name"], "Mercedes-Benz C 220, 2018, automatike");
    assert_eq!(product["image"], json!([photo]));
    assert_eq!(
        product["offers"],
        json!({
            "@type": "Offer",
            "price": "21500",
            "priceCurrency": "EUR",
            "availability": "https://schema.org/InStock",
            "itemCondition": "https://schema.org/UsedCondition",
            "url": url,
            "seller": {"@type": "Organization", "name": "Auto Durrësi"},
        })
    );

    // Sold, it says so, and asks search engines to leave it out.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1002").execute(&pool).await.unwrap();
    let sold = send(pool.clone(), from_the_site("/listing/1002-mercedes-benz-c-220-2018-automatike")).await.body;
    assert!(sold.contains(r#"<meta name="robots" content="noindex, follow">"#));
    assert_eq!(json_ld(&sold).unwrap()["offers"]["availability"], "https://schema.org/SoldOut");

    // A flat to rent has its price, and nothing on sale to tell Google of.
    let flat = send(pool, from_the_site("/listing/1003-apartament-2-1-me-qira-ne-tirane")).await.body;
    assert_eq!(meta_tag(&flat, "og:title"), Some("Apartament 2+1 me qira në Tiranë · 450\u{a0}€/muaj"));
    assert_eq!(meta_tag(&flat, "og:type"), Some("website"));
    assert_eq!(meta_tag(&flat, "product:price:amount"), Some("450"));
    assert!(json_ld(&flat).is_none());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_category_is_found_by_its_filters_and_a_search_not_at_all(pool: PgPool) {
    let site = site();
    // A category's page has one address, however it is ordered, and its first listing's photo.
    let cars = send(pool.clone(), from_the_site("/c/makina?sort=price_asc")).await.body.replace("&#38;", "&");
    assert!(cars.contains(&format!(r#"<link rel="canonical" href="{site}/c/makina">"#)), "{cars}");
    assert_eq!(meta_tag(&cars, "og:title"), Some("Makina"));
    assert_eq!(meta_tag(&cars, "og:description"), Some("Makina: 2 njoftime në Pluribus, me foto, çmime dhe vendndodhje."));
    let golf = "https://images.unsplash.com/photo-1591230740238-e9a71182b67c?auto=format&fit=crop&w=1200&h=900&q=75";
    assert_eq!(meta_tag(&cars, "og:image"), Some(golf));
    assert!(!cars.contains("noindex"));
    // A filter is part of the address: the diesel cars are a page of their own.
    let diesel = send(pool.clone(), from_the_site("/c/makina?vehicle_fuel=DIESEL&sort=price_asc")).await.body.replace("&#38;", "&");
    assert!(diesel.contains(&format!(r#"<link rel="canonical" href="{site}/c/makina?vehicle_fuel=DIESEL">"#)));

    // A search's results are for visitors, not for search engines' results; nor is a search
    // among a seller's listings.
    for search in ["/search?q=golf", "/c/makina?q=golf", "/biznes/auto-durresi?q=mercedes"] {
        let page = send(pool.clone(), from_the_site(search)).await.body;
        assert!(page.contains(r#"<meta name="robots" content="noindex, follow">"#), "{search}");
    }
    let business = send(pool.clone(), from_the_site("/biznes/auto-durresi")).await.body;
    assert!(!business.contains("noindex"));
    assert!(business.contains(&format!(r#"<link rel="canonical" href="{site}/biznes/auto-durresi">"#)));

    // The welcome page: what the site is, and its own picture.
    let home = send(pool.clone(), from_the_site("/")).await.body;
    assert_eq!(meta_tag(&home, "og:url"), Some(format!("{site}/").as_str()));
    assert_eq!(meta_tag(&home, "og:image"), Some(format!("{site}/static/img/pluribus.png").as_str()));
    assert!(meta_tag(&home, "og:description").unwrap().starts_with("Pluribus: njoftime në gjithë Shqipërinë."));
    // The picture is there.
    let picture = get(pool.clone(), "/static/img/pluribus.png").await;
    assert_eq!((picture.status, picture.content_type.as_deref()), (StatusCode::OK, Some("image/png")));
    // A page with nothing of its own to say says what the site is.
    let account = html(pool, "/llogaria").await;
    assert!(account.contains(r#"<meta name="description" content="Pluribus: njoftime në gjithë Shqipërinë."#));
    assert!(meta_tag(&account, "og:title").is_none());
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_visitor_chooses_english_and_the_pages_speak_it(pool: PgPool) {
    // Albanian until chosen; the switch at every page's foot names English in English.
    let home = get(pool.clone(), "/").await.body;
    assert!(home.contains(r#"<html lang="sq">"#), "{home}");
    assert!(home.contains(r#"<button type="submit" name="gjuha" value="en" lang="en">English</button>"#));
    assert!(!home.contains("i18n-en"));

    // Chosen on a page: back to it, the choice kept a year; from another site's page, home.
    let choose = |referer: &'static str, cookie: Option<String>| {
        let pool = pool.clone();
        async move {
            let mut request = Request::post("/gjuha")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::HOST, "127.0.0.1:3000")
                .header(header::REFERER, referer);
            if let Some(cookie) = cookie {
                request = request.header(header::COOKIE, cookie);
            }
            send(pool, request.body(Body::from("gjuha=en")).unwrap()).await
        }
    };
    let chosen = choose("http://127.0.0.1:3000/c/makina", None).await;
    assert_eq!(chosen.status, StatusCode::SEE_OTHER);
    assert_eq!(chosen.location.as_deref(), Some("/c/makina"));
    assert_eq!(chosen.cookie("gjuha"), Some("en"));
    let set = chosen.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(set.contains("Max-Age=31536000"), "{set}");
    assert_eq!(choose("https://another.example/", None).await.location.as_deref(), Some("/"));

    // The pages speak English: their frame, and the ways of its numbers and words; the switch
    // names Albanian in Albanian; the scripts get their texts.
    let page = get_with_cookie(pool.clone(), "/c/makina", "gjuha=en").await.body;
    assert!(page.contains(r#"<html lang="en">"#), "{page}");
    assert!(page.contains(r#"<a href="/kushtet">Terms</a>"#) && page.contains(r#"<a href="/si-funksionon">How it works</a>"#));
    assert!(page.contains(r#"<button type="submit" name="gjuha" value="sq" lang="sq">Shqip</button>"#));
    assert!(page.contains(r#"<script type="application/json" id="i18n-en">"#));

    // A signed-in user's choice is kept on their account, for what reaches them with the site
    // closed.
    let ana = session_for(&pool, ANA_ID).await;
    choose("http://127.0.0.1:3000/", Some(ana)).await;
    let kept: String = sqlx::query_scalar("SELECT preferred_language FROM users WHERE id = $1::uuid").bind(ANA_ID).fetch_one(&pool).await.unwrap();
    assert_eq!(kept, "en");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn every_page_speaks_english_to_whoever_chose_it(pool: PgPool) {
    let en = |uri: &'static str| {
        let pool = pool.clone();
        async move { get_with_cookie(pool, uri, "gjuha=en").await }
    };
    // Browsing: the welcome page, a category's page with its filters, the database's names in
    // English.
    let home = en("/").await.body;
    assert!(home.contains(r#"<h1 class="visually-hidden">What are you looking for?</h1>"#) && home.contains("</svg>Vehicles</a>"), "{home}");
    assert!(home.contains(r#"<h2 id="latest-heading">Newest listings</h2>"#), "{home}");
    let cars = en("/c/makina").await.body;
    assert!(cars.contains("<title>Cars | Pluribus</title>") && cars.contains(r#"<h1 class="browse-title">Cars</h1>"#), "{cars}");
    assert!(cars.contains(r#"aria-label="Filters: No filters""#) && cars.contains(r#"placeholder="Search in Cars""#), "{cars}");
    assert!(cars.contains(">All of Albania<") && cars.contains(r#"<a class="breadcrumb-up" href="/c/automjete">Vehicles</a>"#), "{cars}");
    let vehicles = en("/c/automjete").await.body;
    assert!(vehicles.contains(">Motorbikes<") && vehicles.contains(">Commercial vehicles<") && vehicles.contains(r#"<a class="breadcrumb-up" href="/">Home</a>"#), "{vehicles}");
    assert!(cars.contains(r#"<summary class="sort-button" aria-label="Sort: Newest" title="Sort: Newest">Newest<svg"#), "{cars}");
    assert!(cars.contains(r#"<h2 class="results-heading" id="results-heading">Newest listings</h2>"#), "{cars}");
    assert!(cars.contains(r#"<p class="result-count" id="result-count" data-show="Show 2 listings">2 listings</p>"#), "{cars}");
    assert_eq!(en("/c/makina?numri=1").await.body, r#"{"count":2,"show":"Show 2 listings","text":"2 listings"}"#);
    let vehicles = en("/c/automjete").await.body;
    assert!(vehicles.contains("2016 · 148\u{a0}000\u{a0}km · Diesel</p>"), "{vehicles}");
    let nothing = en("/search?q=xyzzyqqq").await.body;
    assert!(nothing.contains(r#"<p class="empty empty-title">No listings found.</p>"#), "{nothing}");
    // A pin's filter, its chip and its note; the order's name.
    let near = en("/c/makina?lat=41.32833&lng=19.81806&radius=5&sort=price_asc").await.body;
    assert!(near.contains(r#"aria-label="Remove filter: Within 5 km of the pin">"#) && near.contains(r#"<h2 class="results-heading" id="results-heading">Lowest-priced listings</h2>"#), "{near}");

    // A listing: its properties, names and values from the database in English, its sections and
    // its buttons.
    let golf = en("/listing/1001-volkswagen-golf-7-2016-nafte").await.body;
    assert!(golf.contains("<dt>Fuel</dt><dd>Diesel</dd>") && golf.contains("<dt>Gearbox</dt><dd>Manual</dd>"), "{golf}");
    assert!(golf.contains("<dt>Mileage</dt><dd>148\u{a0}000\u{a0}km</dd>"), "{golf}");
    assert!(golf.contains(r#"<p class="card-meta listing-kind">For sale · Used ·"#), "{golf}");
    assert!(golf.contains(r#"<span class="button-label">Show number</span>"#) && golf.contains("<h2>Location</h2>"), "{golf}");
    assert!(golf.contains(r#"<span class="listing-number">No. 1001</span>"#), "{golf}");

    // Posting, signing in, the messages, the pages about the site, and a page that isn't there.
    let form = en("/shto").await.body;
    assert!(form.contains("<title>Post a listing | Pluribus</title>") && form.contains(r#"<label class="field-label" for="title">Title</label>"#), "{form}");
    assert!(en("/llogaria").await.body.contains("<h1>Sign in or create an account</h1>"));
    assert!(en("/mesazhet").await.body.contains(r#"<p class="lead">Sign in to message sellers and read your messages.</p>"#));
    let terms = en("/kushtet").await.body;
    assert!(terms.contains("<h1>Terms of use</h1>"), "{terms}");
    assert!(terms.contains("This English version is a translation for convenience. Where it differs from the Albanian, the Albanian version applies."));
    let gone = en("/kjo-faqe-nuk-ekziston").await;
    assert_eq!(gone.status, StatusCode::NOT_FOUND);
    assert!(gone.body.contains("<h1>Page not found</h1>"), "{}", gone.body);

    // Signed in: one's own listings, editing one, and a conversation's quick first messages.
    let ana = format!("{}; gjuha=en", session_for(&pool, ANA_ID).await);
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains("<h1>My listings</h1>"));
    let edit = get_with_cookie(pool.clone(), "/listing/1001/ndrysho", &ana).await.body;
    assert!(edit.contains("<h1>Edit listing</h1>") && edit.contains(r#"<button type="submit" class="button">Save changes</button>"#), "{edit}");
    let write = get_with_cookie(pool.clone(), "/listing/1002/mesazh", &ana).await.body;
    assert!(write.contains(r#"data-conversation-text-param="What’s your best price?">What’s your best price?</button>"#), "{write}");

    // The same pages in Albanian are as they were.
    assert!(get(pool.clone(), "/c/makina").await.body.contains(r#"<h1 class="browse-title">Makina</h1>"#));
    assert!(get(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await.body.contains("<dt>Karburanti</dt><dd>Naftë</dd>"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn a_browser_that_reads_no_albanian_is_offered_english(pool: PgPool) {
    let home = |languages: &'static str, cookie: Option<&'static str>| {
        let pool = pool.clone();
        async move {
            let mut request = Request::get("/").header(header::ACCEPT_LANGUAGE, languages);
            if let Some(cookie) = cookie {
                request = request.header(header::COOKIE, cookie);
            }
            send(pool, request.body(Body::empty()).unwrap()).await.body
        }
    };
    let offer = r#"<form class="language-offer" method="post" action="/gjuha" lang="en">"#;
    // In English, until the visitor chooses either; never to a browser that reads Albanian, nor to
    // a search engine, which names no language.
    let english = home("en-GB,en;q=0.9", None).await;
    assert!(english.contains(offer) && english.contains("<p>Pluribus is in Albanian. Would you like it in English?</p>"), "{english}");
    assert!(english.contains(r#"<html lang="sq">"#));
    assert!(!home("sq-AL,sq;q=0.9,en;q=0.8", None).await.contains(offer));
    assert!(!home("en-GB,en;q=0.9", Some("gjuha=sq")).await.contains(offer));
    assert!(!home("en-GB,en;q=0.9", Some("gjuha=en")).await.contains(offer));
    assert!(!get(pool.clone(), "/").await.body.contains(offer));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_readers_language_words_what_the_site_kept_for_them(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let in_english = format!("{ana}; gjuha=en");
    // "Cilësimet" has the language too, the one in use pressed.
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &in_english).await.body;
    assert!(settings.contains("<h2>Language</h2>") && settings.contains(r#"aria-pressed="true">English</button>"#), "{settings}");
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body.contains(r#"aria-pressed="true">Shqip</button>"#));

    // The notifications keep their titles in Albanian, as the site wrote them; an English reader's
    // page words them anew, the names the site gives people and its own words among them too.
    sqlx::query(
        "INSERT INTO notifications (user_id, notification_type, title, body, payload) VALUES
         ($1::uuid, 'PRICE_DROP', 'Çmimi ra: Volkswagen Golf 7', NULL, '{}'),
         ($1::uuid, 'NEW_MESSAGE', 'Përdorues të shkroi për „Volkswagen Golf 7“', 'A është ende në shitje?', '{}'),
         ($1::uuid, 'SAVED_SEARCH', '3 njoftime të reja: Makina në Tiranë', NULL, '{}'),
         ($1::uuid, 'PROMOTED', 'I veçuar për 7 ditë: Volkswagen Golf 7', NULL, '{}'),
         ($1::uuid, 'STORE_INVITE', 'Auto Elbasani të fton në dyqan',
          'Si „Poston njoftime“: Poston dhe ndryshon njoftimet e dyqanit, dhe u përgjigjet mesazheve.', '{}')",
    )
    .bind(ANA_ID)
    .execute(&pool)
    .await
    .unwrap();
    let english = get_with_cookie(pool.clone(), "/lajmerimet", &in_english).await.body;
    for said in [
        "Price drop: Volkswagen Golf 7",
        "User wrote to you about “Volkswagen Golf 7”",
        "A është ende në shitje?",
        "3 new listings: Makina në Tiranë",
        "Featured for 7 days: Volkswagen Golf 7",
        "Auto Elbasani invites you to their store",
        "As “Posts listings”: Posts and edits the store’s listings, and answers messages.",
    ] {
        assert!(english.contains(said), "{said}\n{english}");
    }
    let albanian = get_with_cookie(pool.clone(), "/lajmerimet", &ana).await.body;
    assert!(albanian.contains("Çmimi ra: Volkswagen Golf 7") && albanian.contains("Përdorues të shkroi për „Volkswagen Golf 7“"), "{albanian}");

    // A reader in English has their e-mail in English (sender.rs's tests), and the notification it
    // keeps its Albanian.
    sqlx::query("UPDATE users SET preferred_language = 'en' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    let buyer = new_seller(&pool, "+355691112233").await;
    write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await;
    minutes_pass(&pool, 4).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    let title: String = sqlx::query_scalar("SELECT title FROM notifications WHERE notification_type = 'NEW_MESSAGE' AND payload ? 'conversation'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(title.starts_with("Përdorues të shkroi për „Volkswagen Golf"), "{title}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_language_chosen_before_signing_in_becomes_the_accounts(pool: PgPool) {
    let sent = post_form(pool.clone(), "/hyr", &[("name", "Erion"), ("contact", "069 123 4567"), ("started", &started())], Some("gjuha=en")).await;
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}; gjuha=en", sent.cookie("login").unwrap());
    sqlx::query(
        "UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8'))
         WHERE contact = '+355691234567'",
    )
    .execute(&pool)
    .await
    .unwrap();
    // The code's page, and what a wrong code says, in English.
    assert!(get_with_cookie(pool.clone(), "/hyr/kodi", &login).await.body.contains("We sent a 6-digit code"));
    let wrong = post_form(pool.clone(), "/hyr/kodi", &[("code", "000000")], Some(&login)).await;
    assert!(wrong.body.contains("That code isn’t right. You have 4 tries left."), "{}", wrong.body);
    // Signed in, the account speaks English too: its e-mails will.
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    assert_eq!(signed.status, StatusCode::SEE_OTHER);
    let kept: String = sqlx::query_scalar("SELECT preferred_language FROM users WHERE phone_e164 = '+355691234567'").fetch_one(&pool).await.unwrap();
    assert_eq!(kept, "en");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn search_engines_find_the_listings_and_the_categories_by_the_sitemaps(pool: PgPool) {
    let site = site();
    // robots.txt keeps crawlers out of a visitor's own pages, and says where the sitemaps are.
    let robots = send(pool.clone(), from_the_site("/robots.txt")).await;
    assert_eq!(robots.content_type.as_deref(), Some("text/plain; charset=utf-8"));
    for page in ["/hyr", "/njoftimet-e-mia", "/te-ruajturat", "/kerkimet", "/lajmerimet", "/shto", "/listing/*/ndrysho", "/dyqani", "/ftesa", "/ndalo"] {
        assert!(robots.body.contains(&format!("\nDisallow: {page}\n")), "{page}");
    }
    assert!(robots.body.ends_with(&format!("\nSitemap: {site}/sitemap.xml\n")));

    // The sitemap names the pages' sitemap, and as many of the listings' as their numbers need.
    let index = send(pool.clone(), from_the_site("/sitemap.xml")).await;
    assert_eq!(index.content_type.as_deref(), Some("application/xml; charset=utf-8"));
    assert_eq!(locations(&index.body), [format!("{site}/sitemaps/faqet.xml"), format!("{site}/sitemaps/njoftimet-0.xml")]);
    // The pages': the welcome page, every category's and the businesses' with listings.
    let pages = locations(&send(pool.clone(), from_the_site("/sitemaps/faqet.xml")).await.body);
    for page in ["/", "/c/automjete", "/c/makina", "/c/apartamente", "/biznes/auto-durresi"] {
        assert!(pages.contains(&format!("{site}{page}")), "{page}");
    }
    // The listings', each with the day it last changed; one sold is no longer there.
    sqlx::query("UPDATE listings SET status = 'SOLD', sold_at = now() WHERE public_no = 1001").execute(&pool).await.unwrap();
    let listings = send(pool.clone(), from_the_site("/sitemaps/njoftimet-0.xml")).await.body;
    assert_eq!(locations(&listings).len(), 11);
    assert!(!listings.contains("/listing/1001-"));
    let changed: String =
        sqlx::query_scalar("SELECT to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD') FROM listings WHERE public_no = 1002")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mercedes = format!("<url><loc>{site}/listing/1002-mercedes-benz-c-220-2018-automatike</loc><lastmod>{changed}</lastmod></url>");
    assert!(listings.contains(&mercedes), "{listings}");
    assert_eq!(get(pool, "/sitemaps/te-tjera.xml").await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_seller_sees_what_each_listing_drew(pool: PgPool) {
    // Ana's Golf, seen 3 times today and 1 200 times a fortnight ago, its number shown twice today
    // and once then; her iPhone seen once today; Besnik's flat seen 50 times today.
    sqlx::query(
        "INSERT INTO listing_daily_stats (listing_id, stat_date, detail_views, phone_reveals)
         SELECT l.id, current_date - d.days, d.views, d.reveals
         FROM listings l
         JOIN (VALUES (1001, 0, 3, 2), (1001, 14, 1200, 1), (1005, 0, 1, 0), (1003, 0, 50, 0))
              AS d (public_no, days, views, reveals) ON d.public_no = l.public_no",
    )
    .execute(&pool)
    .await
    .unwrap();
    // The Golf kept by Besnik yesterday and by Dritan a month ago, the iPhone by Besnik today.
    sqlx::query(
        "INSERT INTO saved_listings (user_id, listing_id, saved_at)
         SELECT k.user_id, l.id, now() - k.ago
         FROM listings l
         JOIN (VALUES ('22222222-2222-4222-8222-222222222222'::uuid, 1001, interval '1 day'),
                      ('33333333-3333-4333-8333-333333333333'::uuid, 1001, interval '30 days'),
                      ('22222222-2222-4222-8222-222222222222'::uuid, 1005, interval '0 days'))
              AS k (user_id, public_no, ago) ON k.public_no = l.public_no",
    )
    .execute(&pool)
    .await
    .unwrap();

    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    // All her listings together, and the last seven days' under each count; Besnik's flat isn't hers.
    let totals = &mine[mine.find(r#"<dl class="my-stats">"#).unwrap()..mine.find("</dl>").unwrap()];
    let tiles: Vec<(&str, &str)> = totals
        .split(r#"<dd class="my-stat-count">"#)
        .skip(1)
        .map(|tile| {
            let count = &tile[..tile.find("</dd>").unwrap()];
            let week = tile.split(r#"<dd class="my-stat-week">"#).nth(1).unwrap();
            (count, &week[..week.find("</dd>").unwrap()])
        })
        .collect();
    assert_eq!(tiles, [("1\u{a0}204", "+4 këtë javë"), ("3", "+2 këtë javë"), ("3", "+2 këtë javë")]);

    // Each listing's own, under its price.
    let row = |number: &str| {
        let start = mine.find(&format!(r#"href="/listing/{number}-"#)).unwrap();
        let rest = &mine[start..];
        rest[..rest.find("</li>").unwrap()].to_string()
    };
    let golf = row("1001");
    assert!(golf.contains("<span title=\"1\u{a0}203 shikime, 3 këtë javë\">"), "{golf}");
    assert!(golf.contains("Shikime: </span>1\u{a0}203</span>"));
    assert!(golf.contains(r#"<span title="3 kontakte, 2 këtë javë">"#) && golf.contains("Kontakte: </span>3</span>"));
    assert!(golf.contains(r#"<span title="2 ruajtje, 1 këtë javë">"#) && golf.contains("Ruajtje: </span>2</span>"));
    let phone = row("1005");
    assert!(phone.contains(r#"<span title="1 shikim, 1 këtë javë">"#), "{phone}");
    let sofa = row("1007");
    assert!(sofa.contains(r#"<span title="0 shikime, 0 këtë javë">"#) && sofa.contains("Shikime: </span>0</span>"));

    // A week with nothing says so.
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    let his = get_with_cookie(pool, "/njoftimet-e-mia", &besnik).await.body;
    assert!(his.contains(r#"<dd class="my-stat-week">+50 këtë javë</dd>"#));
    assert!(his.contains(r#"<dd class="my-stat-week">Asnjë këtë javë</dd>"#));
}

/// A listing's card on the page, from its opening tag to its end.
fn card_of(html: &str, number: i64) -> String {
    let start = html.find(&format!(r#"<article class="card" id="listing-{number}">"#)).unwrap_or_else(|| panic!("no card {number}"));
    let rest = &html[start..];
    rest[..rest.find("</article>").unwrap()].to_string()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_card_carries_the_start_of_its_description(pool: PgPool) {
    // The Golf's whole description, short enough, under its title, for the list to show.
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let description = positions(&golf, &[
        r#"<h2 class="card-title">"#,
        r#"<p class="card-description">Makina është në gjendje shumë të mirë, e mirëmbajtur rregullisht. Kambio manuale, 148 000 km.</p>"#,
    ]);
    assert!(description.is_sorted(), "{golf}");
    // A long one, its lines run together, is cut at a word near 400 characters, ending in "…".
    let long = format!("Shumë e mirë.\n\n{}", "fjalë ".repeat(120));
    sqlx::query("UPDATE listings SET description = $2 WHERE public_no = $1").bind(1001_i64).bind(&long).execute(&pool).await.unwrap();
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let open = r#"<p class="card-description">"#;
    let text = &golf[golf.find(open).unwrap() + open.len()..];
    let text = &text[..text.find("</p>").unwrap()];
    assert!(text.starts_with("Shumë e mirë. fjalë fjalë") && text.ends_with("fjalë…"), "{text}");
    assert!(text.chars().count() <= 401, "{}", text.chars().count());
    // An empty one, no block.
    sqlx::query("UPDATE listings SET description = '' WHERE public_no = $1").bind(1001_i64).execute(&pool).await.unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1001).contains("card-description"));
}

/// Sets a listing's price, as its seller's edit does, which listing_price_history records.
async fn set_price(pool: &PgPool, number: i64, amount: &str) {
    sqlx::query("UPDATE listings SET price_amount = $2::numeric WHERE public_no = $1")
        .bind(number)
        .bind(amount)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_listing_but_a_cars_or_a_propertys_stands_beside_its_photo(pool: PgPool) {
    // From 960 px up the page is split (listing.css, .listing-split): the photo at the left and
    // the rest, .listing-side, at its right, the seller's card in it (its photos sized for seven
    // twelfths of the page, SPLIT_GALLERY_SIZES: listings_get_photos). A car's or a property's page keeps its
    // photos across the page, and the map under the seller (wide_photos in listings.rs).
    let phone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(phone.contains(r#"<article class="listing listing-split""#), "{phone}");
    let side = &phone[phone.find(r#"<div class="listing-side">"#).unwrap()..];
    let article_end = side.find("</article>").unwrap();
    assert!(side.find(r#"<section class="seller-section" aria-labelledby="seller-heading">"#).unwrap() < article_end);
    assert!(side[..article_end].contains(r#"<h2>Përshkrimi</h2>"#) && !phone.contains("listing-map-section"));
    // The deal (the price, the bar, the circles) is in the flow there, the seller's card after
    // the description, not in it.
    let deal = &phone[phone.find(r#"<div class="listing-deal">"#).unwrap()..phone.find(r#"<section class="listing-details">"#).unwrap()];
    assert!(deal.contains(r#"<div class="price-box">"#) && !deal.contains("seller-section"), "{deal}");
    assert!(!phone.contains("key-facts") && !phone.contains("spec-grid"));
    let order = positions(&phone, &["<h2>Detajet</h2>", "<h2>Përshkrimi</h2>", r#"<section class="seller-section""#]);
    assert!(order.is_sorted(), "{order:?}");
    // A car's page is laid out as FINN's (.listing-car): the seller's card in the deal, at the
    // right from 960 px up (listing_page_shows_price_and_attributes has the rest).
    let golf = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    assert!(golf.contains(r#"<article class="listing listing-car""#), "{golf}");
    // Its map can move into that column, where it has room enough (map_aside_controller.js): the
    // controller on the columns' box, the column and the map its targets.
    assert!(golf.contains(r#"<div class="listing-side" data-controller="map-aside">"#), "{golf}");
    assert!(golf.contains(r#"<div class="listing-deal" data-map-aside-target="deal">"#));
    assert!(golf.contains(r#"<section class="listing-map-section" aria-label="Vendndodhja" data-map-aside-target="map">"#));
    assert!(!phone.contains("map-aside"), "only a car's page");
    let deal = &golf[golf.find(r#"<div class="listing-deal""#).unwrap()..golf.find(r#"<section class="listing-description">"#).unwrap()];
    assert!(deal.contains(r#"<div class="price-box">"#) && deal.contains(r#"<section class="seller-section""#), "{deal}");
    // A property's is laid out as a car's, as the canvas has it (its photos ran across the page
    // once, its details as pills): the deal at the right, its amenities under their own name, its
    // map able to move into the column.
    let flat = html(pool, "/listing/1003-apartament-2-1-me-qira-ne-tirane").await;
    assert!(flat.contains("listing-car") && !flat.contains("listing-split") && !flat.contains(r#"<dl class="attributes">"#), "{flat}");
    assert!(flat.contains(r#"<div class="listing-side" data-controller="map-aside">"#), "{flat}");
    assert!(flat.contains(r#"<ul class="options">"#), "{flat}");
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_price_lowered_lately_says_by_how_much(pool: PgPool) {
    // The Golf, from 9 500 € to 9 000 €: 5% less, after its price with what it was struck
    // through, on its card as on its page.
    set_price(&pool, 1001, "9000").await;
    let golf = card_of(&html(pool.clone(), "/c/makina").await, 1001);
    let badge = "<p class=\"card-price\"><span class=\"card-amount\">9\u{a0}000\u{a0}€</span><del class=\"card-was\"><span class=\"visually-hidden\">Çmimi ishte </span>9\u{a0}500\u{a0}€</del><span class=\"card-drop\">−5%</span></p>";
    assert!(golf.contains(badge), "{golf}");
    let page = html(pool.clone(), "/listing/1001-volkswagen-golf-7-2016-nafte").await;
    let was = "<span class=\"price-was\"><span class=\"visually-hidden\">Çmimi ishte </span><del>9\u{a0}500\u{a0}€</del> <span class=\"price-drop\">−5%</span></span>";
    assert!(page.contains(was), "{page}");

    // Dearer for a while and then back under that, the Mercedes is no cheaper than it was at its
    // lowest of the month, 21 500 €; at 20 000 €, it is, by 7%.
    set_price(&pool, 1002, "30000").await;
    set_price(&pool, 1002, "25000").await;
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1002).contains("card-drop"));
    set_price(&pool, 1002, "20000").await;
    assert!(card_of(&html(pool.clone(), "/c/makina").await, 1002).contains("20\u{a0}000\u{a0}€<span class=\"card-note\"> (i diskutueshëm)</span></span><del class=\"card-was\"><span class=\"visually-hidden\">Çmimi ishte </span>21\u{a0}500\u{a0}€</del><span class=\"card-drop\">−7%</span>"));

    // Less than 1% is no drop: the iPhone, from 55 000 to 54 700 Lekë. Nor is a price in another
    // currency: the bike, from 25 000 Lekë to 200 €.
    set_price(&pool, 1005, "54700").await;
    assert!(!card_of(&html(pool.clone(), "/c/elektronike").await, 1005).contains("card-drop"));
    sqlx::query("UPDATE listings SET price_amount = 200, currency = 'EUR' WHERE public_no = 1010").execute(&pool).await.unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/sport-dhe-hobi").await, 1010).contains("card-drop"));

    // A month on, the Golf's drop is just its price.
    sqlx::query(
        "UPDATE listing_price_history SET changed_at = changed_at - interval '31 days'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1001)",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(!card_of(&html(pool.clone(), "/c/makina").await, 1001).contains("card-drop"));
    assert!(!html(pool, "/listing/1001-volkswagen-golf-7-2016-nafte").await.contains("price-was"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn those_who_keep_a_listing_hear_when_its_price_drops(pool: PgPool) {
    // Ana's phone, at 32 000 Lekë, kept by Besnik, and by Dritan, who turned price drop alerts off.
    let ana = new_seller(&pool, "+355691112233").await;
    let no = post_listing(&pool, &ana).await;
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    let dritan = session_for(&pool, "33333333-3333-4333-8333-333333333333").await;
    for keeper in [&besnik, &dritan] {
        assert_eq!(send(pool.clone(), tap_heart(&no, "1", Some(keeper))).await.status, StatusCode::NO_CONTENT);
    }
    sqlx::query("INSERT INTO notification_preferences (user_id, price_drop_alerts) VALUES ('33333333-3333-4333-8333-333333333333', false)")
        .execute(&pool)
        .await
        .unwrap();
    // How many drops the header's heart says someone hasn't seen (heart_controller.js).
    let unseen = |session: &str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/te-ruajturat/numrat", &session).await.body).unwrap();
            answer["price_drops"].as_i64().unwrap()
        }
    };
    let started = started();
    let edit = |field: (&'static str, &'static str)| {
        let (pool, ana, started, no) = (pool.clone(), ana.clone(), started.clone(), no.clone());
        async move {
            let saved = post_form(pool, &format!("/listing/{no}/ndrysho"), &listing_fields(field, &started), Some(&ana)).await;
            assert_eq!(saved.status, StatusCode::SEE_OTHER);
        }
    };
    // Shown, the page says so (application.js) at the address it names: up to its latest drop.
    let saw = |page: &str, session: &str| {
        let start = page.find(r#"data-seen-url=""#).expect("no data-seen-url") + 15;
        let url = page[start..start + page[start..].find('"').unwrap()].to_string();
        let pool = pool.clone();
        let request = Request::post(url).header(header::COOKIE, session).body(Body::empty()).unwrap();
        async move { assert_eq!(send(pool, request).await.status, StatusCode::NO_CONTENT) }
    };

    // An edit that leaves the price is nothing to hear.
    edit(("title", "Samsung Galaxy S21, 128 GB, me kuti")).await;
    assert_eq!(unseen(&besnik).await, 0);
    // Lowered to 29 000 Lekë: Besnik hears, Dritan doesn't, nor its seller.
    edit(("price", "29 000")).await;
    assert_eq!((unseen(&besnik).await, unseen(&dritan).await, unseen(&ana).await), (1, 0, 0));
    // His page says so over the cards, from what to what, and asks to be told when it is shown.
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body;
    assert!(page.contains(r#"<div class="price-drops" data-seen-url="/te-ruajturat/pare?deri="#), "{page}");
    let drop = format!(
        "<li><a href=\"/listing/{no}-samsung-galaxy-s21-128-gb\">Samsung Galaxy S21, 128 GB</a>: nga <del>32\u{a0}000\u{a0}Lekë</del> në <strong>29\u{a0}000\u{a0}Lekë</strong></li>"
    );
    assert!(page.contains(&drop), "{page}");
    assert!(!get_with_cookie(pool.clone(), "/te-ruajturat", &dritan).await.body.contains("price-drops"));
    // Fetched, the page changes nothing; shown, the drop is seen.
    assert_eq!(unseen(&besnik).await, 1);
    saw(&page, &besnik).await;
    assert_eq!(unseen(&besnik).await, 0);
    assert!(!get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body.contains("price-drops"));

    // Lowered twice more before he looks again: one notice, from what it was when he last saw it.
    edit(("price", "27 000")).await;
    edit(("price", "25 000")).await;
    assert_eq!(unseen(&besnik).await, 1);
    let page = get_with_cookie(pool.clone(), "/te-ruajturat", &besnik).await.body;
    assert!(page.contains("nga <del>29\u{a0}000\u{a0}Lekë</del> në <strong>25\u{a0}000\u{a0}Lekë</strong></li>"), "{page}");
    // A drop after the page was made isn't seen with it, as when the browser shows its copy again.
    edit(("price", "24 000")).await;
    saw(&page, &besnik).await;
    assert_eq!(unseen(&besnik).await, 1);
    // Let go, the listing's drops are none of his business.
    assert_eq!(send(pool.clone(), tap_heart(&no, "0", Some(&besnik))).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unseen(&besnik).await, 0);
}

// Messages between buyers and sellers (web/messages.rs).

const ANA_ID: &str = "11111111-1111-4111-8111-111111111111";
const BESNIK_ID: &str = "22222222-2222-4222-8222-222222222222";
const DRITAN_ID: &str = "33333333-3333-4333-8333-333333333333";

/// The value of the first `name` attribute in the HTML.
fn attribute(html: &str, name: &str) -> String {
    let start = html.find(&format!(r#" {name}=""#)).unwrap_or_else(|| panic!("no {name} in {html}")) + name.len() + 3;
    html[start..start + html[start..].find('"').unwrap()].to_string()
}

/// The ids of the messages on a conversation's page, in order.
fn message_ids(html: &str) -> Vec<String> {
    html.match_indices(r#"data-id=""#)
        .map(|(at, found)| {
            let start = at + found.len();
            html[start..start + html[start..].find('"').unwrap()].to_string()
        })
        .collect()
}

/// How many conversations have messages the user hasn't read, as the header's button asks.
async fn unread(pool: &PgPool, cookie: &str) -> String {
    get_with_cookie(pool.clone(), "/mesazhet/numri", cookie).await.body
}

/// Sends a message from a page's form: starting a conversation (/listing/N/mesazh) or in one.
async fn write(pool: &PgPool, uri: &str, body: &str, cookie: &str) -> TestResponse {
    post_form(pool.clone(), uri, &[("body", body)], Some(cookie)).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_buyer_writes_to_a_seller_who_answers(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let listing = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Others have the listing's button to write to its seller; she doesn't.
    assert!(get_with_cookie(pool.clone(), listing, &buyer).await.body.contains(r#"href="/listing/1001/mesazh""#));
    assert!(!get_with_cookie(pool.clone(), listing, &ana).await.body.contains("/listing/1001/mesazh"));
    // Signed in, the button under the price (the seller's card has none) opens the conversation
    // in the listing's sheet, in its frame; signed out, the button leads to the page, which asks
    // to sign in first, and there is no sheet.
    let theirs = get_with_cookie(pool.clone(), listing, &buyer).await.body;
    let to_sheet = r#"<a class="button message-button" href="/listing/1001/mesazh" data-turbo-frame="conversation" data-turbo-prefetch="false" data-action="message-sheet#open">"#;
    assert_eq!(theirs.matches(to_sheet).count(), 1, "{theirs}");
    // A wide pill that says its name, beside the phone's in the contact bar.
    assert!(theirs.contains(r#"<span class="button-label">Shkruaji shitësit</span></a>"#));
    assert!(theirs.contains(r#"<div class="message-sheet" id="message-sheet" role="dialog" aria-modal="true""#));
    assert!(theirs.contains(r#"<turbo-frame id="conversation" data-message-sheet-target="frame""#));
    let anyone = get(pool.clone(), listing).await.body;
    assert!(anyone.contains(r#"href="/listing/1001/mesazh""#) && !anyone.contains("data-turbo-frame=\"conversation\"") && !anyone.contains(r#"id="message-sheet""#));
    // The page to write on: to whom, about what, and quick messages to start with; in the frame
    // of the same name, for the sheet to take.
    let page = get_with_cookie(pool.clone(), "/listing/1001/mesazh", &buyer).await;
    assert_eq!(page.status, StatusCode::OK);
    assert!(page.body.contains(r#"<turbo-frame id="conversation" data-turbo-action="advance">"#));
    // Who with first, on a line of their own; then the listing, its title the heading.
    let head = positions(&page.body, &[r#"<header class="conversation-head">"#, r#"<p class="conversation-with"><span class="avatar" aria-hidden="true">A</span><span translate="no">Ana</span></p>"#, r#"<h1 class="conversation-title">"#, r#"<span class="conversation-price">"#]);
    assert!(head.is_sorted(), "{head:?}");
    assert!(page.body.contains("Ana") && page.body.contains("Volkswagen Golf") && page.body.contains("Cili është çmimi i fundit?"));
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/mesazh", &ana).await.location.as_deref(), Some(listing));

    let sent = write(&pool, "/listing/1001/mesazh", "  A është ende në shitje?\r\n", &buyer).await;
    assert_eq!(sent.status, StatusCode::SEE_OTHER, "{}", sent.body);
    let url = sent.location.unwrap();
    assert!(url.starts_with("/mesazhet/"), "{url}");
    // Writing about the listing again goes on in the same conversation.
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/mesazh", &buyer).await.location.as_deref(), Some(url.as_str()));

    // The seller has it, unread: in the header's count, and in her inbox in bold.
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":1}"#);
    assert_eq!(unread(&pool, &buyer).await, r#"{"unread":0}"#);
    let inbox = get_with_cookie(pool.clone(), "/mesazhet", &ana).await.body;
    assert!(inbox.contains("is-unread") && inbox.contains("A është ende në shitje?") && inbox.contains("Përdorues"), "{inbox}");
    // Opening it shows it; once the page says it showed it (conversation_controller.js), it is read.
    let thread = get_with_cookie(pool.clone(), &url, &ana).await.body;
    assert!(thread.contains(r#"<p class="message-body">A është ende në shitje?</p>"#), "{thread}");
    let read = attribute(&thread, "data-conversation-read-url-value");
    assert_eq!(read, format!("{url}/lexuar"));
    let first = &message_ids(&thread)[0];
    assert_eq!(post_form(pool.clone(), &format!("{read}?deri={first}"), &[], Some(&ana)).await.status, StatusCode::NO_CONTENT);
    assert_eq!(unread(&pool, &ana).await, r#"{"unread":0}"#);

    // She answers; he sees both, his own on the right, and hers unread.
    assert_eq!(write(&pool, &url, "Po, është.", &ana).await.status, StatusCode::SEE_OTHER);
    let thread = get_with_cookie(pool.clone(), &url, &buyer).await.body;
    assert!(thread.find("A është ende në shitje?").unwrap() < thread.find("Po, është.").unwrap());
    assert!(thread.contains(r#"<li class="message mine""#));
    assert_eq!(unread(&pool, &buyer).await, r#"{"unread":1}"#);

    // What came after a message, for the page to add at its end (conversation_controller.js).
    let ids = message_ids(&thread);
    assert_eq!(ids.len(), 2);
    let newer = get_with_cookie(pool.clone(), &format!("{url}/te-rejat?pas={}", ids[0]), &buyer).await;
    assert_eq!(newer.content_type.as_deref(), Some("text/vnd.turbo-stream.html; charset=utf-8"));
    assert!(newer.body.contains(r#"<turbo-stream action="append" target="thread">"#));
    assert!(newer.body.contains("Po, është.") && !newer.body.contains("A është ende"));
    let none = get_with_cookie(pool.clone(), &format!("{url}/te-rejat?pas={}", ids[1]), &buyer).await;
    assert_eq!(none.status, StatusCode::NO_CONTENT);

    // Nobody else can read it or write in it.
    let besnik = session_for(&pool, BESNIK_ID).await;
    assert_eq!(get_with_cookie(pool.clone(), &url, &besnik).await.status, StatusCode::NOT_FOUND);
    assert_eq!(write(&pool, &url, "Hej", &besnik).await.status, StatusCode::NOT_FOUND);
    assert_eq!(get_with_cookie(pool.clone(), "/mesazhet/jo-nje-bisede", &besnik).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn writing_to_a_seller_takes_signing_in_first(pool: PgPool) {
    // The page to write on sends to sign in, and keeps itself for after (saved::after_sign_in).
    let page = get(pool.clone(), "/listing/1001/mesazh").await;
    assert_eq!(page.location.as_deref(), Some("/llogaria"));
    assert_eq!(page.cookie("kthehu"), Some("/listing/1001/mesazh"));
    let inbox = get(pool.clone(), "/mesazhet").await.body;
    assert!(inbox.contains("Hyr në llogari për t'u shkruar shitësve"));
    assert_eq!(get(pool.clone(), "/mesazhet/numri").await.body, r#"{"unread":0}"#);
    // Every page's rail leads to the conversations, the listing's page too, which a seller reads a
    // buyer's message from; on phones, which have no rail, the menu does, its button with the count
    // of those unread (the welcome page's round button in the row with its search went on
    // 2026-10-11).
    for page in ["/c/automjete", "/listing/1001-volkswagen-golf-7-2016-nafte", "/mesazhet", "/"] {
        let body = get(pool.clone(), page).await.body;
        assert!(body.contains(r#"<a class="rail-item" href="/mesazhet""#), "{page}");
        assert!(body.contains(r#"<a class="menu-row" href="/mesazhet">"#), "{page}");
    }
    assert!(!get(pool.clone(), "/").await.body.contains("inbox-shortcut"));
}

/// The notices of unread messages sent so far (notifications, NEW_MESSAGE), oldest first: to whom,
/// their title, their text and their conversation.
async fn message_notices(pool: &PgPool) -> Vec<(String, String, String, String)> {
    sqlx::query_as(
        "SELECT user_id::text, title, body, payload ->> 'conversation' FROM notifications
         WHERE notification_type = 'NEW_MESSAGE' ORDER BY created_at",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Time passes: every message, every notice about one, when each side last read and when each
/// conversation was last written in, `minutes` older.
async fn minutes_pass(pool: &PgPool, minutes: i32) {
    for (table, column) in [
        ("messages", "created_at"),
        ("notifications", "created_at"),
        ("conversation_participants", "last_read_at"),
        ("conversations", "last_message_at"),
    ] {
        sqlx::query(&format!("UPDATE {table} SET {column} = {column} - make_interval(mins => $1)"))
            .bind(minutes)
            .execute(pool)
            .await
            .unwrap();
    }
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_message_left_unread_brings_a_notice_at_most_once_an_hour(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let due = || marketplace::notices::send_due(&pool, "https://pluribus.example");
    let thread = write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await.location.unwrap();
    let conversation = thread.trim_start_matches("/mesazhet/").to_string();

    // Just written, nothing: Ana may be reading it. A few minutes on, still unread, she hears of
    // it, once: who wrote, about which listing, the message and where to answer it.
    assert_eq!(due().await.unwrap(), 0);
    minutes_pass(&pool, 4).await;
    assert_eq!(due().await.unwrap(), 1);
    let notices = message_notices(&pool).await;
    assert_eq!(notices.len(), 1);
    let (to, title, body, about) = &notices[0];
    assert_eq!((to.as_str(), body.as_str(), about.as_str()), (ANA_ID, "A është ende në shitje?", conversation.as_str()));
    assert!(title.starts_with("Përdorues të shkroi për „Volkswagen Golf"), "{title}");
    assert_eq!(due().await.unwrap(), 0);

    // More within the hour bring none; past it, the latest of those after the notice brings one.
    write(&pool, &thread, "Mund ta shoh nesër?", &buyer).await;
    minutes_pass(&pool, 10).await;
    assert_eq!(due().await.unwrap(), 0);
    minutes_pass(&pool, 60).await;
    assert_eq!(due().await.unwrap(), 1);
    assert_eq!(message_notices(&pool).await[1].2, "Mund ta shoh nesër?");

    // Read, a message brings nothing however long it waits; and Ana's answer, left unread, tells
    // the buyer.
    write(&pool, &thread, "Në orën 5?", &buyer).await;
    minutes_pass(&pool, 70).await;
    let page = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    let last = message_ids(&page).pop().unwrap();
    let read = attribute(&page, "data-conversation-read-url-value");
    post_form(pool.clone(), &format!("{read}?deri={last}"), &[], Some(&ana)).await;
    assert_eq!(due().await.unwrap(), 0);
    write(&pool, &thread, "Po, në orën 5.", &ana).await;
    minutes_pass(&pool, 4).await;
    assert_eq!(due().await.unwrap(), 1);
    let buyer_id: String = sqlx::query_scalar("SELECT buyer_user_id::text FROM conversations").fetch_one(&pool).await.unwrap();
    assert_eq!(message_notices(&pool).await[2].0, buyer_id);

    // "Cilësimet" says she hears of them, and lets her stop: then nothing, until she says yes
    // again. The form sends every box ticked, the others as they were.
    let checkbox = r#"<input type="checkbox" name="mesazhet" value="1""#;
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(settings.contains(r#"<form class="notice-settings settings-notices" method="post" action="/llogaria/njoftimet">"#));
    assert!(settings.contains(&format!("{checkbox} checked>")), "{settings}");
    let off = post_form(pool.clone(), "/llogaria/njoftimet", &[("cmimet", "1"), ("kerkimet", "1")], Some(&ana)).await;
    assert_eq!(off.location.as_deref(), Some("/llogaria/cilesimet?u=njoftimet"));
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=njoftimet", &ana).await.body;
    assert!(settings.contains(&format!("{checkbox}>")) && settings.contains("Zgjedhjet për njoftimet u ruajtën."), "{settings}");
    write(&pool, &thread, "A je aty?", &buyer).await;
    minutes_pass(&pool, 120).await;
    assert_eq!(due().await.unwrap(), 0);
    post_form(pool.clone(), "/llogaria/njoftimet", &[("mesazhet", "1"), ("cmimet", "1"), ("kerkimet", "1")], Some(&ana)).await;
    assert_eq!(due().await.unwrap(), 1);
    // Not signed in, the form leads to signing in.
    assert_eq!(post_form(pool.clone(), "/llogaria/njoftimet", &[], None).await.location.as_deref(), Some("/llogaria"));
}

/// Ana's choice of `column` in notification_preferences: on, where she has made none.
async fn ana_wants(pool: &PgPool, column: &str) -> bool {
    sqlx::query_scalar(&format!("SELECT coalesce((SELECT {column} FROM notification_preferences WHERE user_id = $1::uuid), true)"))
        .bind(ANA_ID)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn an_emails_link_stops_its_kind_without_signing_in(pool: PgPool) {
    // Ana signs in with an address as well: her notice goes by e-mail, with the link that stops
    // them, its token made as it went; the buyer, with a number alone, has none.
    let buyer = new_seller(&pool, "+355691112233").await;
    write(&pool, "/listing/1001/mesazh", "A është ende në shitje?", &buyer).await;
    minutes_pass(&pool, 4).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    let tokens: Vec<(String, String)> = sqlx::query_as("SELECT id::text, notice_token FROM users WHERE notice_token IS NOT NULL").fetch_all(&pool).await.unwrap();
    assert_eq!(tokens.len(), 1);
    let (whose, token) = &tokens[0];
    assert_eq!((whose.as_str(), token.len()), (ANA_ID, 32));

    // Opened, the link only asks: a link checker following it stops nothing.
    let link = format!("/ndalo/{token}/mesazhet");
    let asked = get(pool.clone(), &link).await;
    assert_eq!(asked.status, StatusCode::OK);
    assert!(asked.body.contains("<h1>T'i ndalojmë?</h1>"), "{}", asked.body);
    assert!(asked.body.contains("Nuk do të të vijnë më email-e për mesazhet që nuk i ke lexuar. Mesazhet i gjen si gjithmonë te „Mesazhet“."), "{}", asked.body);
    assert!(asked.body.contains(&format!(r#"<form method="post" action="{link}">"#)));
    assert!(ana_wants(&pool, "message_alerts").await);
    // A mail program's one-click POST (List-Unsubscribe-Post), with no cookie and nothing more,
    // stops them, as the box on "Cilësimet" does; "Rikthe" takes it back.
    let stopped = post_form(pool.clone(), &link, &[("List-Unsubscribe", "One-Click")], None).await;
    assert_eq!(stopped.status, StatusCode::OK);
    assert!(stopped.body.contains("<h1>U ndalën</h1>"), "{}", stopped.body);
    assert!(stopped.body.contains(&format!(r#"<form method="post" action="{link}/rikthe">"#)));
    assert!(!ana_wants(&pool, "message_alerts").await);
    assert!(post_form(pool.clone(), &format!("{link}/rikthe"), &[], None).await.body.contains("<h1>U rikthyen</h1>"));
    assert!(ana_wants(&pool, "message_alerts").await);

    // A token or a kind it doesn't know is no page.
    for wrong in [format!("/ndalo/{}/mesazhet", "0".repeat(32)), format!("/ndalo/{token}/tjeter"), format!("/ndalo/{token}/kerkimi-jo")] {
        assert_eq!(get(pool.clone(), &wrong).await.status, StatusCode::NOT_FOUND, "{wrong}");
        assert_eq!(post_form(pool.clone(), &wrong, &[], None).await.status, StatusCode::NOT_FOUND, "{wrong}");
    }

    // A search's link stops that search's alerts, and only a search of Ana's own.
    for user in [ANA_ID, BESNIK_ID] {
        let session = session_for(&pool, user).await;
        post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona?city=tirane")], Some(&session)).await;
    }
    let searches: Vec<(String, String)> = sqlx::query_as("SELECT id::text, name FROM saved_searches ORDER BY user_id").fetch_all(&pool).await.unwrap();
    let ((hers, name), (his, _)) = (&searches[0], &searches[1]);
    let alerts = |id: String| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, bool>("SELECT alerts_enabled FROM saved_searches WHERE id = $1::uuid").bind(id).fetch_one(&pool).await.unwrap()
        }
    };
    let link = format!("/ndalo/{token}/kerkimi-{hers}");
    let asked = get(pool.clone(), &link).await.body;
    assert!(asked.contains(&format!("Nuk do të të vijnë më email-e për kërkimin „{name}“. Kërkimi mbetet te „Kërkimet e ruajtura“.")), "{asked}");
    post_form(pool.clone(), &link, &[("List-Unsubscribe", "One-Click")], None).await;
    assert!(!alerts(hers.clone()).await);
    post_form(pool.clone(), &format!("{link}/rikthe"), &[], None).await;
    assert!(alerts(hers.clone()).await);
    assert_eq!(get(pool.clone(), &format!("/ndalo/{token}/kerkimi-{his}")).await.status, StatusCode::NOT_FOUND);

    // The expiry warnings' link stops every notice by e-mail, the notifications staying on the
    // site; the other choices stay as they were.
    let link = format!("/ndalo/{token}/email");
    let stopped = post_form(pool.clone(), &link, &[], None).await.body;
    assert!(stopped.contains("Nuk do të të vijnë më lajmërimet me email nga Pluribus. Lajmërimet i gjen te faqja, te „Lajmërimet“."), "{stopped}");
    assert!(!ana_wants(&pool, "email_enabled").await && ana_wants(&pool, "message_alerts").await);

    // A suspended account's link is no page.
    sqlx::query("UPDATE users SET status = 'SUSPENDED' WHERE id = $1::uuid").bind(ANA_ID).execute(&pool).await.unwrap();
    assert_eq!(get(pool.clone(), &link).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_notifications_page_lists_every_kind_and_reads_them_once_shown(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112244").await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let buyer_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    // What the account button asks for (account_controller.js): how many are unread, and how many
    // of those are about the user's own listings.
    let counts = |session: &str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move {
            let answer: Value = serde_json::from_str(&get_with_cookie(pool, "/lajmerimet/numri", &session).await.body).unwrap();
            (answer["unread"].as_i64().unwrap(), answer["own"].as_i64().unwrap())
        }
    };
    // Shown, the page says so (application.js) at the address it names.
    let saw = |page: &str, session: &str| {
        let url = attribute(page, "data-seen-url");
        let pool = pool.clone();
        let request = Request::post(url).header(header::COOKIE, session).body(Body::empty()).unwrap();
        async move { assert_eq!(send(pool, request).await.status, StatusCode::NO_CONTENT) }
    };

    // Not signed in, the page asks to sign in, and there is nothing to count.
    assert!(get(pool.clone(), "/lajmerimet").await.body.contains("Hyr në llogari për të parë lajmërimet"));
    assert_eq!(counts("").await, (0, 0));
    // Nothing yet: the page says what will come there.
    let empty = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    assert!(empty.contains("Ende s'ke asnjë lajmërim.") && !empty.contains("data-seen-url"), "{empty}");
    // Every page's account button asks for its dot.
    assert!(empty.contains(r#"<a class="icon-button account-button" href="/llogaria" aria-label="Llogaria" title="Llogaria" data-controller="account">"#));

    // The buyer keeps Ana's phone, whose price she then lowers, and keeps a search that has new
    // listings; and writes to her about it, which she leaves unread. Her phone is days from expiring.
    let no = post_listing(&pool, &ana).await;
    // Its first price an hour old, so that the drop comes after it whatever the clock does.
    sqlx::query(
        "UPDATE listing_price_history SET changed_at = changed_at - interval '1 hour'
         WHERE listing_id = (SELECT id FROM listings WHERE public_no = $1::bigint)",
    )
    .bind(&no)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(send(pool.clone(), tap_heart(&no, "1", Some(&buyer))).await.status, StatusCode::NO_CONTENT);
    let edited = post_form(pool.clone(), &format!("/listing/{no}/ndrysho"), &listing_fields(("price", "29 000"), &started()), Some(&ana)).await;
    assert_eq!(edited.status, StatusCode::SEE_OTHER);
    post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona?city=tirane")], Some(&buyer)).await;
    let search: uuid::Uuid = sqlx::query_scalar("SELECT id FROM saved_searches").fetch_one(&pool).await.unwrap();
    marketplace::db::record_search_alert(&pool, buyer_id, search, "2 njoftime të reja: Telefona · Tiranë", 2, "https://pluribus.example/kerkimet")
        .await
        .unwrap();
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &buyer).await.location.unwrap();
    minutes_pass(&pool, 4).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    sqlx::query("UPDATE listings SET expires_at = now() + interval '2 days' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(marketplace::expiry::run(&pool, "https://pluribus.example").await.unwrap(), (1, 0));

    // The buyer's: the search's alert, then the price drop, both new, each leading to what it is
    // about; nothing about listings of his own.
    assert_eq!(counts(&buyer).await, (2, 0));
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    let lines = positions(
        &page,
        &[
            &format!(r#"<a class="notification-line" href="/kerkimet/{search}">"#),
            r#"<span class="notification-title"><span class="visually-hidden">E re: </span>2 njoftime të reja: Telefona · Tiranë</span>"#,
            &format!(r#"<a class="notification-line" href="/listing/{no}-samsung-galaxy-s21-128-gb">"#),
            r#"<span class="notification-title"><span class="visually-hidden">E re: </span>Çmimi ra: Samsung Galaxy S21, 128 GB</span>"#,
        ],
    );
    assert!(lines.is_sorted(), "{page}");
    assert_eq!(page.matches(r#"<li class="notification is-unread">"#).count(), 2);
    assert!(page.contains(r#"<span class="notification-when">Sot</span>"#));
    // Fetched, the page reads nothing; shown, it reads them, and the heart loses its dot too.
    assert_eq!(counts(&buyer).await, (2, 0));
    saw(&page, &buyer).await;
    assert_eq!(counts(&buyer).await, (0, 0));
    let heart: Value = serde_json::from_str(&get_with_cookie(pool.clone(), "/te-ruajturat/numrat", &buyer).await.body).unwrap();
    assert_eq!((heart["price_drops"].as_i64(), heart["searches"].as_i64()), (Some(0), Some(0)));
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &buyer).await.body;
    assert!(!page.contains("is-unread") && !page.contains("data-seen-url") && page.contains("Çmimi ra: Samsung Galaxy S21, 128 GB"));

    // Ana's: her phone expiring, then the buyer's message, with what he wrote; one about her own
    // listing, for the account button's dot. "Njoftimet e mia" leads there, saying how many are new.
    assert_eq!(counts(&ana).await, (2, 1));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<nav class="account-nav" aria-label="Llogaria jote">"#), "{mine}");
    assert!(mine.contains(r#"<a href="/njoftimet-e-mia" aria-current="page">Njoftimet e mia</a>"#), "{mine}");
    assert!(mine.contains(r#"<a href="/lajmerimet">Lajmërimet · 2 të reja</a>"#), "{mine}");
    let page = get_with_cookie(pool.clone(), "/lajmerimet", &ana).await.body;
    let lines = positions(
        &page,
        &[
            &format!(r#"<a class="notification-line" href="/listing/{no}-samsung-galaxy-s21-128-gb">"#),
            "Njoftimi skadon së shpejti: Samsung Galaxy S21, 128 GB</span>",
            &format!(r#"<a class="notification-line" href="{thread}">"#),
            r#"<span class="notification-body">A është ende në shitje?</span>"#,
        ],
    );
    assert!(lines.is_sorted(), "{page}");
    // Her phone expires after the page was made: not read with it, as when the browser shows its
    // copy again.
    sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = $1::bigint").bind(&no).execute(&pool).await.unwrap();
    assert_eq!(marketplace::expiry::run(&pool, "https://pluribus.example").await.unwrap(), (0, 1));
    saw(&page, &ana).await;
    assert_eq!(counts(&ana).await, (1, 1));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/lajmerimet">Lajmërimet · 1 e re</a>"#));
    // Renewing the listing reads what was said of its expiry.
    post_form(pool.clone(), &format!("/listing/{no}/rifresko"), &[], Some(&ana)).await;
    assert_eq!(counts(&ana).await, (0, 0));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/lajmerimet">Lajmërimet</a>"#));

    // Another message left unread an hour on brings another notice; reading the conversation reads it.
    write(&pool, &thread, "Mund ta shoh nesër?", &buyer).await;
    minutes_pass(&pool, 70).await;
    assert_eq!(marketplace::notices::send_due(&pool, "https://pluribus.example").await.unwrap(), 1);
    assert_eq!(counts(&ana).await, (1, 0));
    let conversation = get_with_cookie(pool.clone(), &thread, &ana).await.body;
    let last = message_ids(&conversation).pop().unwrap();
    post_form(pool.clone(), &format!("{thread}/lexuar?deri={last}"), &[], Some(&ana)).await;
    assert_eq!(counts(&ana).await, (0, 0));
}

/// Adds a contact to the account signed in with `session`, as "Cilësimet" does: the form, then the
/// code sent to it (made known), typed back. Returns where that leads.
async fn add_contact(pool: &PgPool, session: &str, typed: &str, contact: &str) -> TestResponse {
    let sent = post_form(pool.clone(), "/llogaria/kontakti", &[("kontakti", typed)], Some(session)).await;
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"), "{}", sent.body);
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = $1")
        .bind(contact)
        .execute(pool)
        .await
        .unwrap();
    let page = get_with_cookie(pool.clone(), "/hyr/kodi", &format!("{session}; {login}")).await.body;
    assert!(page.contains("Shkruaje dhe shtohet te llogaria jote.") && page.contains(r#"<button type="submit" class="button">Ruaje</button>"#), "{page}");
    post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&format!("{session}; {login}"))).await
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_settings_change_the_name_the_contacts_what_one_hears_and_the_devices(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let ana_id: String = sqlx::query_scalar("SELECT id::text FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    let no = post_listing(&pool, &ana).await;
    let contact_of_listing = |pool: PgPool, no: String| async move {
        sqlx::query_as::<_, (Option<String>, Option<String>)>(
            "SELECT c.phone_e164::text, c.email::text FROM listing_contacts c JOIN listings l ON l.id = c.listing_id WHERE l.public_no = $1::bigint",
        )
        .bind(no)
        .fetch_one(&pool)
        .await
        .unwrap()
    };
    let account = |pool: PgPool| async move {
        sqlx::query_as::<_, (String, Option<String>, Option<String>, String)>(
            "SELECT display_name, phone_e164::text, email::text, auth_subject FROM users WHERE phone_e164 = '+355691119999' OR display_name = 'Ana Kola'",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    };

    // Signed out, the page asks to sign in; signed in, it is the account's, the row over its
    // heading marking it.
    assert!(get(pool.clone(), "/llogaria/cilesimet").await.body.contains(r#"action="/hyr""#));
    let page = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body;
    assert!(page.contains(r#"<a href="/llogaria/cilesimet" aria-current="page">Cilësimet</a>"#), "{page}");
    assert!(page.contains(r#"<input id="emri" name="emri" value="Përdorues""#) && page.contains("<strong>+355 69 111 2233</strong>"));
    assert!(!page.contains("Hiqe") && !page.contains(r#"name="email""#), "one contact, and no e-mail to send to");
    assert!(page.contains("Llogaria jote është e hapur vetëm në këtë pajisje."));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<a href="/llogaria/cilesimet">Cilësimet</a>"#));

    // The name, tidied; an empty one goes back with why.
    let renamed = post_form(pool.clone(), "/llogaria/emri", &[("emri", "  Ana   Kola ")], Some(&ana)).await;
    assert_eq!(renamed.location.as_deref(), Some("/llogaria/cilesimet?u=emri"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=emri", &ana).await.body.contains("Emri u ruajt."));
    assert_eq!(account(pool.clone()).await.0, "Ana Kola");
    let empty = post_form(pool.clone(), "/llogaria/emri", &[("emri", "   ")], Some(&ana)).await;
    assert_eq!(empty.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(empty.body.contains("Shkruaj emrin që do të shohin të tjerët."));

    // An e-mail address added, once the code sent to it comes back: verified, beside the number,
    // either of which can now be taken off.
    let added = add_contact(&pool, &ana, "Ana.Kola@Example.test", "ana.kola@example.test").await;
    assert_eq!(added.location.as_deref(), Some("/llogaria/cilesimet?u=kontakti"));
    let (_, phone, email, _) = account(pool.clone()).await;
    assert_eq!((phone.as_deref(), email.as_deref()), (Some("+355691112233"), Some("ana.kola@example.test")));
    let verified: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_verifications WHERE user_id = $1::uuid AND verification_type = 'EMAIL' AND verified_value = 'ana.kola@example.test')",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(verified);
    let page = get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=kontakti", &ana).await.body;
    assert!(page.contains("U ruajt te llogaria") && page.contains("<strong>ana.kola@example.test</strong>") && page.matches(">Hiqe</button>").count() == 2, "{page}");
    // Not one the account has, nor another account's.
    for (typed, why) in [("069 111 2233", "Ky është tashmë i llogarisë sate."), ("069 111 2244", "Ky është i një llogarie tjetër")] {
        let refused = post_form(pool.clone(), "/llogaria/kontakti", &[("kontakti", typed)], Some(&ana)).await;
        assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(refused.body.contains(why) && refused.body.contains(&format!(r#"value="{typed}""#)), "{}", refused.body);
    }

    // A new number in place of the old: on the account, and on its listing, which showed the old;
    // the old one free, for whoever signs in with it next.
    let changed = add_contact(&pool, &ana, "069 111 9999", "+355691119999").await;
    assert_eq!(changed.location.as_deref(), Some("/llogaria/cilesimet?u=kontakti"));
    let (_, phone, _, subject) = account(pool.clone()).await;
    assert_eq!(phone.as_deref(), Some("+355691119999"));
    assert_eq!(subject, format!("user:{ana_id}"));
    assert_eq!(contact_of_listing(pool.clone(), no.clone()).await, (Some("+355691119999".to_string()), None));
    let someone_else = sign_in_with_code(&pool, "069 111 2233", "+355691112233").await;
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &someone_else).await.body.contains("<strong>+355 69 111 2233</strong>"));

    // The number taken off: the listing shows the address instead. The address, the last one
    // left, stays.
    let removed = post_form(pool.clone(), "/llogaria/kontakti/hiq", &[("hiq", "numri")], Some(&ana)).await;
    assert_eq!(removed.location.as_deref(), Some("/llogaria/cilesimet?u=hequr"));
    assert_eq!(account(pool.clone()).await.1, None);
    assert_eq!(contact_of_listing(pool.clone(), no.clone()).await, (None, Some("ana.kola@example.test".to_string())));
    post_form(pool.clone(), "/llogaria/kontakti/hiq", &[("hiq", "email")], Some(&ana)).await;
    assert_eq!(account(pool.clone()).await.2.as_deref(), Some("ana.kola@example.test"));

    // What it hears of, as the boxes say; the e-mail box now that there is an address.
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body.contains(r#"<input type="checkbox" name="email" value="1" checked>"#));
    let saved = post_form(pool.clone(), "/llogaria/njoftimet", &[("mesazhet", "1")], Some(&ana)).await;
    assert_eq!(saved.location.as_deref(), Some("/llogaria/cilesimet?u=njoftimet"));
    let preferences: (bool, bool, bool, bool) = sqlx::query_as(
        "SELECT message_alerts, price_drop_alerts, saved_search_alerts, email_enabled FROM notification_preferences WHERE user_id = $1::uuid",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(preferences, (true, false, false, false));

    // Signed in on another device too: out of it from here, this one staying.
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at)
         VALUES (sha256(convert_to('another-device', 'UTF8')), $1::uuid, now() + interval '1 day')",
    )
    .bind(&ana_id)
    .execute(&pool)
    .await
    .unwrap();
    let other = "session=another-device".to_string();
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &ana).await.body.contains("e hapur në 2 pajisje"));
    let out = post_form(pool.clone(), "/llogaria/pajisjet", &[], Some(&ana)).await;
    assert_eq!(out.location.as_deref(), Some("/llogaria/cilesimet?u=pajisjet"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &other).await.body.contains(r#"action="/hyr""#));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=pajisjet", &ana).await.body.contains("Dole nga pajisjet e tjera."));
    let _ = besa;
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn closing_an_account_takes_its_listings_photos_and_contacts_away(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let ana_id: String = sqlx::query_scalar("SELECT id::text FROM users WHERE phone_e164 = '+355691112233'").fetch_one(&pool).await.unwrap();
    // Ana's phone with a photo, which Besa keeps and asks about; a search Ana keeps.
    let posted = post_with_photos(pool.clone(), "/shto", &listing_fields(("", ""), &started()), &[jpeg(40, 30)], &ana).await;
    let url = posted.location.expect("posted");
    let no = url.trim_start_matches("/listing/").split('-').next().unwrap().to_string();
    let photo: String = sqlx::query_scalar(
        "SELECT m.storage_key FROM listing_media m JOIN listings l ON l.id = m.listing_id WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(media().get(&photo).await.unwrap().is_some());
    send(pool.clone(), tap_heart(&no, "1", Some(&besa))).await;
    let thread = write(&pool, &format!("/listing/{no}/mesazh"), "A është ende në shitje?", &besa).await.location.unwrap();
    post_form(pool.clone(), "/kerkimet/ruaj", &[("adresa", "/c/telefona")], Some(&ana)).await;

    // Not without "FSHI" typed: back, with why, and nothing gone.
    let refused = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", "po")], Some(&ana)).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Shkruaj FSHI"));
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::OK);

    // Closed: signed out, the page says what became of it.
    let closed = post_form(pool.clone(), "/llogaria/fshi", &[("konfirmo", " fshi ")], Some(&ana)).await;
    assert_eq!(closed.status, StatusCode::OK);
    assert!(closed.body.contains("<h1>Llogaria u fshi</h1>"), "{}", closed.body);
    assert!(!closed.body.contains("dyqanet"), "{}", closed.body);
    assert_eq!(closed.cookie("session"), Some(""));
    // Its listing gone, its contact and its photo with it, the file out of the store.
    assert_eq!(get(pool.clone(), &url).await.status, StatusCode::NOT_FOUND);
    let (status, contacts, photos): (String, i64, i64) = sqlx::query_as(
        "SELECT l.status, (SELECT count(*) FROM listing_contacts c WHERE c.listing_id = l.id), (SELECT count(*) FROM listing_media m WHERE m.listing_id = l.id)
         FROM listings l WHERE l.public_no = $1::bigint",
    )
    .bind(&no)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((status.as_str(), contacts, photos), ("DELETED", 0, 0));
    assert!(media().get(&photo).await.unwrap().is_none());
    // The account anonymised, what only it had gone.
    let (name, phone, subject, user_status): (String, Option<String>, String, String) =
        sqlx::query_as("SELECT display_name, phone_e164::text, auth_subject, status FROM users WHERE id = $1::uuid")
            .bind(&ana_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((name.as_str(), phone, subject, user_status.as_str()), ("Përdorues i fshirë", None, format!("deleted:{ana_id}"), "DELETED"));
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM sessions WHERE user_id = $1::uuid) + (SELECT count(*) FROM saved_searches WHERE user_id = $1::uuid)",
    )
    .bind(&ana_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0);
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"action="/hyr""#));
    assert_eq!(get(pool.clone(), &format!("/perdorues/{ana_id}")).await.status, StatusCode::NOT_FOUND);
    // Besa keeps her message, Ana shown as a deleted user; the listing she kept says it went.
    let conversation = get_with_cookie(pool.clone(), &thread, &besa).await.body;
    assert!(conversation.contains("Përdorues i fshirë") && conversation.contains("A është ende në shitje?"), "{conversation}");
    let kept = get_with_cookie(pool.clone(), "/te-ruajturat", &besa).await.body;
    assert!(section(&kept, "saved-gone-heading").contains(r#"<span class="badge">U fshi</span>"#), "{kept}");
    // The number is free again: signing in with it opens a new account.
    let again = sign_in_with_code(&pool, "069 111 2233", "+355691112233").await;
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet", &again).await.body.contains("<strong>+355 69 111 2233</strong>"));
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn until_the_seller_answers_a_buyer_sends_three_messages_and_no_links(pool: PgPool) {
    let ana = session_for(&pool, ANA_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    // A first message with a link, empty, or too long goes back, as typed, with why.
    let refused = write(&pool, "/listing/1001/mesazh", "Shiko www.example.com", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Lidhjet mund t") && refused.body.contains(">Shiko www.example.com</textarea>"), "{}", refused.body);
    assert!(write(&pool, "/listing/1001/mesazh", " ", &buyer).await.body.contains("Shkruaj mesazhin."));
    assert!(write(&pool, "/listing/1001/mesazh", &"a".repeat(1001), &buyer).await.body.contains("deri në 1000 shkronja"));

    let url = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    for body in ["A është ende në shitje?", "Kur mund ta shoh?"] {
        assert_eq!(write(&pool, &url, body, &buyer).await.status, StatusCode::SEE_OTHER);
    }
    let fourth = write(&pool, &url, "Alo?", &buyer).await;
    assert_eq!(fourth.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(fourth.body.contains("Prit që Ana të të përgjigjet"), "{}", fourth.body);
    // Once she answers, he writes on, links and all.
    assert_eq!(write(&pool, &url, "Po, nesër pasdite.", &ana).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Mirë. Ja ku jam: https://maps.example.com/x", &buyer).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Alo?", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn one_account_writes_to_a_few_new_sellers_an_hour_and_a_day(pool: PgPool) {
    let buyer = new_seller(&pool, "+355691112233").await;
    let started = |count: i32, ago: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query(
                "INSERT INTO conversations (buyer_user_id, seller_user_id, created_at, last_message_at)
                 SELECT u.id, $1::uuid, now() - $3::interval, now() - $3::interval
                 FROM users u, generate_series(1, $2)
                 WHERE u.phone_e164 = '+355691112233'",
            )
            .bind(BESNIK_ID)
            .bind(count)
            .bind(ago)
            .execute(&pool)
            .await
            .unwrap();
        }
    };
    // Five sellers written to in the last hour: a sixth waits.
    started(5, "10 minutes").await;
    let refused = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("U ke shkruar 5 shitësve këtë orë"), "{}", refused.body);
    // Earlier in the day they count towards the day's fifteen.
    sqlx::query("UPDATE conversations SET created_at = now() - interval '3 hours'").execute(&pool).await.unwrap();
    started(10, "5 hours").await;
    let refused = write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await;
    assert!(refused.body.contains("U ke shkruar 15 shitësve sot"), "{}", refused.body);
    // A day later, he writes again.
    sqlx::query("UPDATE conversations SET created_at = now() - interval '25 hours'").execute(&pool).await.unwrap();
    assert_eq!(write(&pool, "/listing/1001/mesazh", "Përshëndetje", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_seller_blocks_a_buyer_and_lets_him_write_again(pool: PgPool) {
    let besnik = session_for(&pool, BESNIK_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let url = write(&pool, "/listing/1003/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    assert_eq!(post_form(pool.clone(), &format!("{url}/blloko"), &[], Some(&besnik)).await.status, StatusCode::SEE_OTHER);
    // He can't write to Besnik any more, here or about another of his listings.
    let refused = write(&pool, &url, "Alo?", &buyer).await;
    assert_eq!(refused.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(refused.body.contains("Besnik nuk pranon mesazhe nga ti."));
    assert!(get_with_cookie(pool.clone(), &url, &buyer).await.body.contains("Besnik nuk pranon mesazhe nga ti."));
    assert_eq!(write(&pool, "/listing/1004/mesazh", "Po kjo shtëpi?", &buyer).await.status, StatusCode::UNPROCESSABLE_ENTITY);
    // Besnik sees he blocked him, and can let him write again.
    let page = get_with_cookie(pool.clone(), &url, &besnik).await.body;
    assert!(page.contains("E ke bllokuar këtë përdorues") && page.contains(&format!(r#"action="{url}/zhblloko""#)));
    assert_eq!(post_form(pool.clone(), &format!("{url}/zhblloko"), &[], Some(&besnik)).await.status, StatusCode::SEE_OTHER);
    assert_eq!(write(&pool, &url, "Alo?", &buyer).await.status, StatusCode::SEE_OTHER);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_business_listing_reaches_whoever_posted_it(pool: PgPool) {
    let dritan = session_for(&pool, DRITAN_ID).await;
    let buyer = new_seller(&pool, "+355691112233").await;
    let url = write(&pool, "/listing/1002/mesazh", "Përshëndetje", &buyer).await.location.unwrap();
    // The buyer writes to the business; whoever posted its listing answers for it.
    assert!(get_with_cookie(pool.clone(), &url, &buyer).await.body.contains("Auto Durrësi"));
    let inbox = get_with_cookie(pool.clone(), "/mesazhet", &dritan).await.body;
    assert!(inbox.contains("Përdorues") && inbox.contains("Mercedes-Benz"), "{inbox}");
    assert_eq!(write(&pool, &url, "Po, ejani nesër.", &dritan).await.status, StatusCode::SEE_OTHER);
}

/// Signs in with a code sent to this contact, typed as `typed`, which is `contact` once read: the
/// session's cookie.
async fn sign_in_with_code(pool: &PgPool, typed: &str, contact: &str) -> String {
    let sent = post_form(pool.clone(), "/hyr", &[("contact", typed), ("started", &started())], None).await;
    assert_eq!(sent.location.as_deref(), Some("/hyr/kodi"));
    let login = format!("login={}", sent.cookie("login").unwrap());
    sqlx::query("UPDATE login_codes SET code_hash = sha256(convert_to(contact || ':123456', 'UTF8')) WHERE contact = $1")
        .bind(contact)
        .execute(pool)
        .await
        .unwrap();
    let signed = post_form(pool.clone(), "/hyr/kodi", &[("code", "123456")], Some(&login)).await;
    format!("session={}", signed.cookie("session").expect("signed in"))
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_reported_listing_comes_off_the_site_until_a_moderator_decides(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Anyone may report a listing from its page. Someone not signed in is asked to sign in first,
    // and comes back to the report after; its own seller can't.
    assert!(html(pool.clone(), golf).await.contains(r#"href="/listing/1001/raporto""#));
    let anonymous = get(pool.clone(), "/listing/1001/raporto").await;
    assert!(anonymous.body.contains("Hyr në llogari që ta raportosh."), "{}", anonymous.body);
    assert_eq!(anonymous.cookie("kthehu"), Some("/listing/1001/raporto"));
    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &ana).await.status, StatusCode::NOT_FOUND);

    let mut reporters = Vec::new();
    for phone in ["+355691110001", "+355691110002", "+355691110003", "+355691110004"] {
        reporters.push(new_seller(&pool, phone).await);
    }
    let report = |session: &str, reason: &'static str, details: &'static str| {
        let (pool, session) = (pool.clone(), session.to_string());
        async move { post_form(pool, "/listing/1001/raporto", &[("reason", reason), ("details", details)], Some(&session)).await }
    };
    // The form offers its reasons, and needs one; "Tjetër" needs what is wrong.
    let form = get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[0]).await.body;
    assert_eq!(form.matches(r#"<input type="radio" name="reason""#).count(), 7);
    // The listing at the page's top, as the canvas has it: a small card, its photo beside its
    // title and its price and town under it, all one link to it.
    let card = &form[form.find(r#"<a class="report-listing" href="/listing/1001-volkswagen-golf-7-2016-nafte">"#).expect("the listing's card")..];
    let card = &card[..card.find("</a>").unwrap()];
    assert!(card.contains(r#"<img src="https://images.unsplash.com/"#), "{card}");
    assert!(card.contains("<span class=\"report-listing-meta\">9\u{a0}500\u{a0}€ · <span translate=\"no\">Tiranë</span></span>"), "{card}");
    let none = report(&reporters[0], "", "").await;
    assert_eq!(none.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(none.body.contains("Zgjidh çfarë nuk shkon me njoftimin."));
    assert!(report(&reporters[0], "OTHER", "  ").await.body.contains("Shkruaj shkurt çfarë nuk shkon me njoftimin."));

    // Reported for the wrong details, then for a scam and as fake: it stays up, as wrong details
    // are for a moderator to see to, not reason to hide it. Thanked; a second report is the first.
    let sent = report(&reporters[0], "WRONG_DETAILS", "Është në kategorinë e gabuar.").await;
    assert_eq!(sent.location.as_deref(), Some("/listing/1001/raporto"));
    assert!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[0]).await.body.contains("Faleminderit!"));
    report(&reporters[0], "SCAM", "").await;
    report(&reporters[1], "SCAM", "Kërkon para përpara, pa e parë makinën.").await;
    report(&reporters[2], "FAKE", "").await;
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM reports").fetch_one(&pool).await.unwrap();
    assert_eq!(reports, 3);
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::OK);

    // A third for a serious reason takes it off the site: gone for others, the cards with it, and
    // the report page but for whoever reported it.
    report(&reporters[3], "SCAM", "").await;
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::NOT_FOUND);
    assert!(!card_numbers(&html(pool.clone(), "/c/makina").await).contains(&1001));
    assert!(get_with_cookie(pool.clone(), "/listing/1001/raporto", &reporters[3]).await.body.contains("Faleminderit!"));
    // Its seller still sees it, told why, and so do the moderators.
    let own = get_with_cookie(pool.clone(), golf, &ana).await.body;
    assert!(own.contains("Ky njoftim nuk shfaqet në faqe: disa njerëz e raportuan, dhe moderatorët do ta shqyrtojnë."));
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body.contains(r#"<span class="badge danger">Në shqyrtim</span>"#));
    let moderator = new_seller(&pool, MODERATOR).await;
    assert_eq!(get_with_cookie(pool.clone(), golf, &moderator).await.status, StatusCode::OK);
    // On /moderim, the hidden first: who reported it and why, and what they wrote.
    assert!(get_with_cookie(pool.clone(), "/njoftimet-e-mia", &moderator).await.body.contains("Për miratim (1)"));
    let queue = get_with_cookie(pool.clone(), "/moderim", &moderator).await.body;
    for text in ["4 veta e raportuan", r#"<span class="badge danger">Fshehur</span>"#, "<li>Mashtrim (2)</li>", "<li>Njoftim i rremë</li>",
                 "<li>Të dhëna të gabuara</li>", "Kërkon para përpara, pa e parë makinën."] {
        assert!(queue.contains(text), "{text}: {queue}");
    }

    // Kept: back on the site, its reports dismissed, both steps on record.
    let kept = post_form(pool.clone(), "/moderim/njoftim/1001/mbaje", &[], Some(&moderator)).await;
    assert_eq!(kept.location.as_deref(), Some("/moderim"));
    assert_eq!(get(pool.clone(), golf).await.status, StatusCode::OK);
    assert!(get_with_cookie(pool.clone(), "/moderim", &moderator).await.body.contains("Nuk ka njoftime të raportuara."));
    let actions: Vec<(String, bool)> = sqlx::query_as(
        "SELECT action_type, moderator_user_id IS NOT NULL FROM moderation_actions ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(actions, [("SUSPEND_LISTING".to_string(), false), ("RESTORE_LISTING".to_string(), true)]);
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM reports WHERE status = 'OPEN'").fetch_one(&pool).await.unwrap();
    assert_eq!(open, 0);

    // Another taken down for good: off the site, and its seller told so; deleting it is theirs.
    let iphone = "/listing/1005-iphone-13-128-gb-si-i-ri";
    post_form(pool.clone(), "/listing/1005/raporto", &[("reason", "PROHIBITED"), ("details", "")], Some(&reporters[0])).await;
    assert_eq!(get(pool.clone(), iphone).await.status, StatusCode::OK);
    let removed = post_form(pool.clone(), "/moderim/njoftim/1005/hiq", &[], Some(&moderator)).await;
    assert_eq!(removed.location.as_deref(), Some("/moderim"));
    assert_eq!(get(pool.clone(), iphone).await.status, StatusCode::NOT_FOUND);
    assert!(get_with_cookie(pool.clone(), iphone, &ana).await.body.contains("Moderatorët e hoqën këtë njoftim nga faqja."));
    let mine = get_with_cookie(pool.clone(), "/njoftimet-e-mia", &ana).await.body;
    assert!(mine.contains(r#"<span class="badge danger">Hequr nga moderatorët</span>"#));
    let deleted = post_form(pool.clone(), "/listing/1005/fshi", &[], Some(&ana)).await;
    assert_eq!(deleted.location.as_deref(), Some("/njoftimet-e-mia"));
    // Nobody else decides.
    assert_eq!(post_form(pool.clone(), "/moderim/njoftim/1001/hiq", &[], Some(&ana)).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn a_blocked_sellers_listings_leave_the_pages_of_whoever_blocked_them(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    let mercedes = "/listing/1002-mercedes-benz-c-220-2018-automatike";
    // Blocking takes being signed in.
    assert!(!html(pool.clone(), golf).await.contains("Blloko shitësin"));
    let besnik = session_for(&pool, "22222222-2222-4222-8222-222222222222").await;
    // The listing's page offers no block: the report's thanks do, once Besnik has reported it.
    assert!(!get_with_cookie(pool.clone(), golf, &besnik).await.body.contains("/listing/1001/blloko"));
    let report = "/listing/1001/raporto";
    assert!(!get_with_cookie(pool.clone(), report, &besnik).await.body.contains("Blloko shitësin"));
    post_form(pool.clone(), report, &[("reason", "SPAM"), ("details", "")], Some(&besnik)).await;
    let thanks = get_with_cookie(pool.clone(), report, &besnik).await.body;
    assert!(thanks.contains("Faleminderit!") && thanks.contains("Nëse s'do ta shohësh më këtë shitës, bllokoje:"), "{thanks}");
    assert!(thanks.contains(r#"<form method="post" action="/listing/1001/blloko">"#) && thanks.contains("</svg>Blloko shitësin</button>"));

    // Besnik blocks Ana, who sells the Golf, the iPhone and three more: the thanks say so then.
    let blocked = post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&besnik)).await;
    assert_eq!(blocked.location.as_deref(), Some("/listing/1001/raporto"));
    let thanks = get_with_cookie(pool.clone(), report, &besnik).await.body;
    assert!(thanks.contains("E ke bllokuar edhe shitësin") && !thanks.contains("Blloko shitësin"), "{thanks}");
    // Her listings leave his pages: the welcome page, a category's, a search, those under another
    // listing; others still see them.
    let anas = [1001, 1005, 1007, 1010, 1012];
    let his = |uri: &'static str| {
        let (pool, besnik) = (pool.clone(), besnik.clone());
        async move { card_numbers(&get_with_cookie(pool, uri, &besnik).await.body.replace("&#38;", "&")) }
    };
    let home = his("/").await;
    assert!(!home.is_empty() && home.iter().all(|number| !anas.contains(number)), "{home:?}");
    assert!(card_numbers(&html(pool.clone(), "/").await).contains(&1001));
    assert!(!his("/c/makina").await.contains(&1001));
    assert!(!his("/search?q=golf").await.contains(&1001));
    assert!(card_numbers(&html(pool.clone(), mercedes).await).contains(&1001), "the Golf is like the Mercedes");
    assert!(!his(mercedes).await.contains(&1001));
    // Her listing's own page, which he can still open, says so, and where to let her be again.
    let page = get_with_cookie(pool.clone(), golf, &besnik).await.body;
    assert!(page.contains("E ke bllokuar këtë shitës") && page.contains(r#"E zhbllokon te <a href="/llogaria/cilesimet">Cilësimet</a>."#));
    assert!(!page.contains("Zhblloko shitësin") && !page.contains("listing-safety"));

    // "Cilësimet" lists her, to let be again, and says so after.
    let settings = get_with_cookie(pool.clone(), "/llogaria/cilesimet", &besnik).await.body;
    assert!(
        settings.contains(r#"<a href="/perdorues/11111111-1111-4111-8111-111111111111"><span class="avatar" aria-hidden="true">A</span><span class="blocked-seller-name">Ana</span></a>"#),
        "{settings}"
    );
    let unblocked = post_form(pool.clone(), "/llogaria/zhblloko/11111111-1111-4111-8111-111111111111", &[], Some(&besnik)).await;
    assert_eq!(unblocked.location.as_deref(), Some("/llogaria/cilesimet?u=zhbllokuar"));
    assert!(get_with_cookie(pool.clone(), "/llogaria/cilesimet?u=zhbllokuar", &besnik).await.body.contains("U zhbllokua."));
    assert!(his("/c/makina").await.contains(&1001));

    // Nobody blocks themselves.
    let ana = session_for(&pool, "11111111-1111-4111-8111-111111111111").await;
    assert_eq!(post_form(pool.clone(), "/listing/1001/blloko", &[("blloko", "1")], Some(&ana)).await.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_number_a_seller_signed_in_with_shows_as_verified(pool: PgPool) {
    let golf = "/listing/1001-volkswagen-golf-7-2016-nafte";
    // Nobody signed in with a code yet: nothing is verified.
    assert!(!html(pool.clone(), golf).await.contains("verifikuar"));
    // Ana signs in with a code sent to her number: her listings say so beside her name, and her
    // profile under it.
    sign_in_with_code(&pool, "069 000 0001", "+355690000001").await;
    // On every listing, a car's too, a check beside her name, which says what it means as a
    // tooltip and to screen readers; on her profile, the words under her name.
    let page = html(pool.clone(), golf).await;
    assert!(page.contains(r#"<span class="seller-name"><span translate="no">Ana</span><span class="verified" title="Numër i verifikuar">"#), "{page}");
    assert!(!page.contains("</svg>Numër i verifikuar</span>"));
    let iphone = html(pool.clone(), "/listing/1005-iphone-13-128-gb-si-i-ri").await;
    assert!(iphone.contains(r#"<span class="seller-name"><span translate="no">Ana</span><span class="verified" title="Numër i verifikuar">"#), "{iphone}");
    assert!(iphone.contains(r#"<span class="visually-hidden">Numër i verifikuar</span></span></span>"#));
    assert!(html(pool.clone(), "/perdorues/11111111-1111-4111-8111-111111111111").await.contains("Numër i verifikuar</p>"));
    let (kind, value): (String, String) =
        sqlx::query_as("SELECT verification_type, verified_value FROM user_verifications WHERE status = 'VERIFIED'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), value.as_str()), ("PHONE", "+355690000001"));
    // Besnik never did: his flat says nothing of it.
    assert!(!html(pool.clone(), "/listing/1003-apartament-2-1-me-qira-ne-tirane").await.contains("verifikuar"));
    // A shop's number is verified once one of its people signs in with it: Dritan signs in with
    // his own, which isn't the one the Mercedes shows.
    sign_in_with_code(&pool, "069 000 0003", "+355690000003").await;
    assert!(!html(pool.clone(), "/listing/1002-mercedes-benz-c-220-2018-automatike").await.contains("verifikuar"));
    // A listing showing another number than the one verified says nothing either.
    sqlx::query("UPDATE listing_contacts SET phone_e164 = '+355690000011' WHERE listing_id = (SELECT id FROM listings WHERE public_no = 1001)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!html(pool.clone(), golf).await.contains("verifikuar"));
}

#[sqlx::test(migrator = "MIGRATOR")]
async fn the_scripts_can_tell_whether_someone_is_signed_in(pool: PgPool) {
    // The session cookie is out of scripts' reach; a cookie beside it says there is one, so a page
    // for someone not signed in doesn't ask for their counts. Set when a session comes without it,
    // as after signing in; cleared when it stays after the session has gone, as after signing out.
    let with = |cookie: &'static str, uri: &'static str| {
        let pool = pool.clone();
        async move { send(pool, Request::get(uri).header(header::COOKIE, cookie).body(Body::empty()).unwrap()).await }
    };
    assert_eq!(with("session=abc", "/").await.cookie("llogari"), Some("1"));
    assert_eq!(with("llogari=1", "/").await.cookie("llogari"), Some(""));
    assert_eq!(with("session=abc; llogari=1", "/").await.cookie("llogari"), None);
    assert_eq!(get(pool.clone(), "/").await.cookie("llogari"), None);
    // Not on a static file, which browsers keep.
    assert_eq!(with("session=abc", "/static/css/base.css").await.cookie("llogari"), None);
}
