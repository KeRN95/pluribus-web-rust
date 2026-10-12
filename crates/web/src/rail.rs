//! The rail at the screen's left from 720px up (base.html): the open menu's places in icons alone,
//! posting, the account's pages, every main category, the dark theme and the language. It says
//! which of them the page is, from the address of the request being answered, which a middleware
//! keeps for it while the page is made, and on a category's page its main category, which the
//! page says (`set_category`).

use std::cell::RefCell;

use axum::{extract::Request, middleware::Next, response::Response};

tokio::task_local! {
    /// The path of the request being answered.
    static PATH: String;
    /// The main category the page being made is in, its address ("/c/automjete"), or nothing.
    static CATEGORY: RefCell<String>;
}

/// Keeps the request's path for the rail while its page is made (`here`), and room for the page to
/// say its main category (`set_category`).
pub async fn keep_path(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    PATH.scope(path, CATEGORY.scope(RefCell::new(String::new()), next.run(request))).await
}

/// Which of the account's places on the rail the page is: "mine" (the visitor's listings), "saved"
/// (the listings kept), "searches" (the searches kept), "alerts" (the notifications), "messages"
/// (the conversations, a conversation too) or "settings"; "" anywhere else, and outside a request.
pub fn here() -> &'static str {
    PATH.try_with(|path| place(path)).unwrap_or("")
}

/// The main category the page is in, its address ("/c/automjete"), for the rail to mark: a
/// category's page says it (browse.rs); "" on every other page, and outside a request.
pub fn category() -> String {
    CATEGORY.try_with(|category| category.borrow().clone()).unwrap_or_default()
}

/// Says which main category, by its slug, the page being made is in (`category`).
pub fn set_category(slug: &str) {
    let _ = CATEGORY.try_with(|category| *category.borrow_mut() = format!("/c/{slug}"));
}

fn place(path: &str) -> &'static str {
    match path {
        "/njoftimet-e-mia" => "mine",
        "/te-ruajturat" => "saved",
        "/kerkimet-e-ruajtura" => "searches",
        "/lajmerimet" => "alerts",
        "/mesazhet" => "messages",
        "/llogaria/cilesimet" => "settings",
        _ if path.starts_with("/mesazhet/") => "messages",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::place;

    #[test]
    fn the_rail_knows_its_places_by_their_addresses() {
        assert_eq!(place("/njoftimet-e-mia"), "mine");
        assert_eq!(place("/te-ruajturat"), "saved");
        assert_eq!(place("/kerkimet-e-ruajtura"), "searches");
        assert_eq!(place("/lajmerimet"), "alerts");
        assert_eq!(place("/mesazhet"), "messages");
        assert_eq!(place("/mesazhet/12"), "messages");
        assert_eq!(place("/llogaria/cilesimet"), "settings");
        for elsewhere in ["/", "/search", "/c/makina", "/harta", "/listing/5-golf", "/llogaria"] {
            assert_eq!(place(elsewhere), "", "{elsewhere}");
        }
    }
}
