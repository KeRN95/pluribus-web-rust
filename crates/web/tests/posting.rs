//! Posting a listing and looking after it (web/post.rs, ai_fill.rs, photos.rs): the form, its
//! checks and the AI that fills it in; posting signed in or not, and the limits; where it is, its
//! zone and its point on the map; its photos, their order and their copies; editing it; pausing,
//! reserving, selling, deleting, renewing and expiring; and "Njoftimet e mia", with what each
//! listing drew.

mod common;

use std::sync::{Arc, Mutex};

use axum::{Router, body::Body, http::{Request, StatusCode, header}};
use common::*;
use pluribus_services::{Moderators, ai::Ai};
use serde_json::{Value, json};
use sqlx::PgPool;

/// The app with an AI to fill in the listing form, which the server at `url` stands in for
/// (ai_stand_in).
fn app_with_ai(pool: PgPool, url: &str) -> Router {
    let ai = Ai::new("test-key", url, "claude-haiku-4-5-20251001");
    pluribus_web::app_with_ai(pool, media(), Moderators::new([MODERATOR]), Some(ai))
}

/// A form sent from this network: the address a proxy in front of the app says it came from.
async fn post_form_from(pool: PgPool, uri: &str, fields: &[(&str, &str)], ip: &str) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let request = Request::post(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("x-forwarded-for", ip);
    send(pool, request.body(Body::from(body)).unwrap()).await
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
    // Editing it, taking it down and deleting it are round buttons, their icons alone, their
    // words said to screen readers and under the pointer, on one line; the rest under them, in
    // words (the user's ask of 2026-10-11).
    let tools = &mine[mine.find(r#"<div class="my-listing-tools">"#).expect("the row's tools")..];
    let tools = &tools[..tools.find("</div>").unwrap()];
    assert!(tools.contains(&format!(r#"href="/listing/{no}/ndrysho" aria-label="Ndrysho" title="Ndrysho"><svg"#)), "{tools}");
    assert!(tools.contains(r#"aria-label="Çaktivizo""#) && tools.contains(r#"class="button secondary small danger" aria-label="Fshi""#), "{tools}");
    assert!(!tools.contains("U shit") && mine.contains(r#"<div class="my-listing-more">"#), "{tools}");
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
async fn the_listing_form_offers_categories_at_every_depth(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let form = get_with_cookie(pool.clone(), "/shto", &ana).await.body;
    // Vehicles: cars and the like in one group, the kinds of parts in the next, under their path.
    let vehicles = form.find(r#"<optgroup label="Automjete">"#).expect("the vehicles group");
    let parts = form.find(r#"<optgroup label="Automjete › Pjesë">"#).expect("the parts group");
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
    // Behind them their glow, the first's tiny copy, which the controller turns to the one that
    // shows as they slide (a_listings_photos_glow_as_youtubes_ambient_light).
    let opening = r#"<div class="listing-glow" aria-hidden="true" data-slides-target="glow"><img src=""#;
    let glow = page.find(opening).expect("the glow") + opening.len();
    let tiny = &page[glow..glow + page[glow..].find('"').unwrap()];
    assert!(tiny.starts_with("data:image/webp;base64,"), "{tiny}");
    assert_eq!(page.find(&format!(r#"<img class="gallery-backdrop" src="{tiny}""#)), page.find(r#"<img class="gallery-backdrop""#), "the first's");
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
async fn listings_expire_unless_their_sellers_renew_them(pool: PgPool) {
    let ana = new_seller(&pool, "+355691112233").await;
    let besa = new_seller(&pool, "+355691112244").await;
    let no = post_listing(&pool, &ana).await;
    let url = format!("/listing/{no}-samsung-galaxy-s21-128-gb");
    let renew = format!("/listing/{no}/rifresko");
    let renew_form = format!(r#"<form method="post" action="{renew}">"#);
    let run = || pluribus_services::expiry::run(&pool, "https://pluribus.example");
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

/// A form sent to the site as one without moderators runs it, where nothing waits for review.
async fn post_form_unmoderated(pool: PgPool, uri: &str, fields: &[(&str, &str)]) -> TestResponse {
    let body = form_urlencoded::Serializer::new(String::new()).extend_pairs(fields).finish();
    let request = Request::post(uri).header(header::CONTENT_TYPE, "application/x-www-form-urlencoded").body(Body::from(body)).unwrap();
    send_to(pluribus_web::app_with(pool, media(), Moderators::new([])), request).await
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
    assert!(pluribus_db::forget_drafts(&pool).await.unwrap().is_empty());
    assert_eq!(status("Një tjetër që pret kodin").await, "DRAFT", "not yet a day");
    sqlx::query("UPDATE listings SET created_at = now() - interval '25 hours' WHERE title = 'Një tjetër që pret kodin'")
        .execute(&pool)
        .await
        .unwrap();
    pluribus_db::forget_drafts(&pool).await.unwrap();
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
