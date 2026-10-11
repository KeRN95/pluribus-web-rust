//! The rail at the screen's left from 720px up (base.html): posting, then the front page, the
//! search, the listings kept and the conversations. It says which of them the page is, from the
//! address of the request being answered, which a middleware keeps for it while the page is made.

use axum::{extract::Request, middleware::Next, response::Response};

tokio::task_local! {
    /// The path of the request being answered.
    static PATH: String;
}

/// Keeps the request's path for the rail while its page is made (`here`).
pub async fn keep_path(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_string();
    PATH.scope(path, next.run(request)).await
}

/// Which of the rail's places the page is: "home" (the front page), "search" (a search or a
/// category's listings, which its search in the top bar searches, or the map), "saved" (the
/// listings kept) or "messages" (the conversations, a conversation too); "" anywhere else, and
/// outside a request.
pub fn here() -> &'static str {
    PATH.try_with(|path| place(path)).unwrap_or("")
}

fn place(path: &str) -> &'static str {
    match path {
        "/" => "home",
        "/search" | "/harta" => "search",
        "/te-ruajturat" => "saved",
        "/mesazhet" => "messages",
        _ if path.starts_with("/c/") => "search",
        _ if path.starts_with("/mesazhet/") => "messages",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::place;

    #[test]
    fn the_rail_knows_its_places_by_their_addresses() {
        assert_eq!(place("/"), "home");
        assert_eq!(place("/search"), "search");
        assert_eq!(place("/c/makina"), "search");
        assert_eq!(place("/harta"), "search");
        assert_eq!(place("/te-ruajturat"), "saved");
        assert_eq!(place("/mesazhet"), "messages");
        assert_eq!(place("/mesazhet/12"), "messages");
        assert_eq!(place("/mesazhet/numri"), "messages");
        assert_eq!(place("/listing/5-golf"), "");
        assert_eq!(place("/llogaria"), "");
    }
}
