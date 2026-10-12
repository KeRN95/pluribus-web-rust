//! The site as a whole (web/lib.rs, pages.rs, seo.rs, language.rs): its name, logo and icon; the
//! pages about it, and an address with nothing there; its static files, its scripts, minified, and
//! its stylesheet; the headers that keep it safe, and forms posted from other sites; what search
//! engines and link previews get; and English, for whoever chooses it.

mod common;

use axum::{body::Body, http::{Request, StatusCode, header}};
use common::*;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

#[sqlx::test(migrator = "MIGRATOR", fixtures("listings"))]
async fn the_site_is_called_pluribus(pool: PgPool) {
    // The logo is the name in lowercase, the word alone, in Clash Display drawn as shapes
    // (logo.svg): the letters in the text's colour, the i's dot and the inside of the b apart (the
    // dot black as the letters and the b's inside in the accent on the light theme, the dot in the
    // accent and the b open on the dark: the user's ask of 2026-10-11), and the word for screen
    // readers. The site's one typeface, Geist, comes with the
    // page, the only font that does: the headings' Inter and the logo's Playfair Display went with
    // their files. The pages' titles and the footer say the name.
    let home = html(pool.clone(), "/").await;
    let logo = &home[home.find(r#"<a class="logo" id="logo" href="/" data-turbo-permanent translate="no"><svg class="logo-mark" "#).expect("the logo")..];
    let logo = &logo[..logo.find("</a>").unwrap()];
    // The b's counter comes first, under the letters, grown by its line (24 units all round) so
    // that no hairline shows at its edge: the user's ask of 2026-10-11.
    assert!(logo.contains(r#" viewBox="0 0 4126 1140" aria-hidden="true"><path class="logo-counter" d="M2465 643Q2465 707 2501.5 735Q2538 763 2611 763Q2685 763 2718.5 735Q2752 707 2752 640Q2752 572 2719 544Q2686 516 2612 516Q2537 516 2501 543.5Q2465 571 2465 635L2465 643Z" stroke-width="24" stroke-linejoin="round"/><path fill="currentColor" d="M190 1060"#), "{logo}");
    assert!(logo.contains(r#"<rect class="logo-dot" x="2086" y="220" width="150" height="118"/></svg>"#) && !logo.contains("logo-dot-edge"), "{logo}");
    assert!(logo.ends_with(r#"</svg><span class="visually-hidden">pluribus</span>"#), "{logo}");
    // The dot and the b's inside take their colours from the theme (--logo-dot, --logo-counter),
    // the dot with no edge now that it is black on white.
    let css = get(pool.clone(), &format!("/static/css/site.css?v={}", asset_version())).await.body;
    assert!(css.contains(".logo-dot{fill:var(--logo-dot);") && !css.contains("logo-edge"));
    assert!(css.contains(".logo-counter{fill:var(--logo-counter);stroke:var(--logo-counter);"));
    assert!(css.contains("--logo-dot:currentColor;") && css.contains("--logo-counter:var(--highlight);"));
    assert_eq!(css.matches("--logo-dot:var(--highlight);--logo-counter:transparent;").count(), 2, "both dark themes");
    assert!(css.contains("--dot-edge:inset 0 0 0 1.5px rgb(27 27 27 / 45%);"));
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
    assert_eq!(pluribus_db::forget_old_codes(&pool).await.unwrap(), 1);
    let left: Vec<String> = sqlx::query_scalar("SELECT contact FROM login_codes").fetch_all(&pool).await.unwrap();
    assert_eq!(left, ["+355690000102"]);
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
    let version = asset_version();
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
    let version = asset_version();
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
    let version = asset_version();
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
    for sheet in pluribus_web::STYLESHEETS {
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
    let modules = pluribus_web::js_modules();
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
        // The categories' line, which a mouse drags from 720px up (the user's ask of 2026-10-11).
        "controllers/drag_row_controller.js",
    ] {
        assert!(home.iter().any(|module| module == used), "{used} in {home:?}");
    }
    for unused in ["controllers/map_controller.js", "controllers/leaflet.js", "controllers/filters_controller.js"] {
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

/// A request as one comes through the tunnel: to the site's name, over https.
fn from_the_site(uri: &str) -> Request<Body> {
    Request::get(uri)
        .header(header::HOST, "pluribus.test")
        .header("x-forwarded-proto", "https")
        .body(Body::empty())
        .unwrap()
}

/// The address the site's links start with (web/seo.rs): PUBLIC_URL when the settings give one,
/// or the one a request from_the_site came to, the tests' settings giving none
/// (`settings_reach_the_pages_they_change` gives one).
fn site() -> String {
    Config::for_tests().public_url.unwrap_or_else(|| "https://pluribus.test".to_string())
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
    assert!(home.contains(r#"<h1 class="visually-hidden">What are you looking for?</h1>"#) && home.contains(r#"<span class="shortcut-name">Vehicles</span>"#), "{home}");
    assert!(home.contains(r#"<h2 id="latest-heading">Newest listings</h2>"#), "{home}");
    let cars = en("/c/makina").await.body;
    assert!(cars.contains("<title>Cars | Pluribus</title>") && cars.contains(r#"<h1 class="browse-title">Cars</h1>"#), "{cars}");
    assert!(cars.contains(r#"aria-label="Filters: No filters""#) && cars.contains(r#"placeholder="Search in Cars""#), "{cars}");
    assert!(cars.contains(">All of Albania<") && cars.contains(r#"<a class="breadcrumb-back title-back" href="/c/automjete" aria-label="Back" title="Back""#), "{cars}");
    let vehicles = en("/c/automjete").await.body;
    assert!(vehicles.contains(">Motorbikes<") && vehicles.contains(">Commercial<") && vehicles.contains(r#"<a class="breadcrumb-back title-back" href="/" aria-label="Back""#), "{vehicles}");
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
    assert!(golf.contains(r#"<p class="card-meta listing-kind">Used ·"#) && golf.contains(r#"<span class="price-kind">For sale</span>"#), "{golf}");
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
    assert_eq!(pluribus_services::notices::send_due(&pool, &Senders::none(), "https://pluribus.example").await.unwrap(), 1);
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
async fn settings_reach_the_pages_they_change(pool: PgPool) {
    // The settings are read once, as the server starts, into the state (pluribus-services'
    // config.rs); the pages take them from the state they are answered over, the templates and the
    // site's address from the request's own (web::request_config). These turn on the typefaces'
    // trial, give the site's address and who runs it, and the address to write to.
    let config = Config {
        type_trial: true,
        public_url: Some("https://pluribus.al".into()),
        contact_email: Some("info@pluribus.al".into()),
        operator: Some("Pluribus sh.p.k.".into()),
        ..Config::for_tests()
    };
    let config = std::sync::Arc::new(config);
    let set = || pluribus_web::router(AppState { config: config.clone(), ..state(pool.clone()) });
    let home = send_to(set(), from_the_site("/")).await;
    // The top bar's typeface panel, and the policy letting Google Fonts and unpkg's icons in.
    assert!(home.body.contains(r#"<details class="type-menu" data-controller="dropdown type-trial""#));
    let policy = home.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(policy.contains("style-src 'self' 'unsafe-inline' https://fonts.googleapis.com;"), "{policy}");
    assert!(policy.contains("font-src 'self' https://fonts.gstatic.com;") && policy.contains("connect-src 'self' https://unpkg.com;"), "{policy}");
    // The site's address, whatever the request came to.
    let contact = send_to(set(), from_the_site("/kontakt")).await.body;
    assert!(contact.contains(r#"<link rel="canonical" href="https://pluribus.al/kontakt">"#), "{contact}");
    assert!(contact.contains(r#"Na shkruaj te <a href="mailto:info@pluribus.al">info@pluribus.al</a>."#), "{contact}");
    let privacy = send_to(set(), from_the_site("/privatesia")).await.body;
    assert!(privacy.contains("Pluribus e drejton Pluribus sh.p.k., që vendos si mbahen të dhënat e tua"), "{privacy}");
    let promote = send_to(set(), from_the_site("/promovo")).await.body;
    assert!(promote.contains(r#"<a href="mailto:info@pluribus.al?subject="#), "{promote}");
    let robots = send_to(set(), from_the_site("/robots.txt")).await.body;
    assert!(robots.contains("Sitemap: https://pluribus.al/sitemap.xml"), "{robots}");

    // The tests' own settings have none of it: the policy as the site's always is, the address the
    // request came to, and no address to write to.
    let home = send(pool.clone(), from_the_site("/")).await;
    assert!(!home.body.contains("type-menu"));
    let policy = home.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
    assert!(!policy.contains("googleapis") && !policy.contains("unpkg"), "{policy}");
    assert!(policy.contains("style-src 'self' 'unsafe-inline'; img-src"), "{policy}");
    let contact = send(pool.clone(), from_the_site("/kontakt")).await.body;
    assert!(contact.contains(r#"<link rel="canonical" href="https://pluribus.test/kontakt">"#), "{contact}");
    assert!(!contact.contains("mailto:"), "{contact}");
    assert!(!send(pool, from_the_site("/privatesia")).await.body.contains("Pluribus e drejton"));
}
