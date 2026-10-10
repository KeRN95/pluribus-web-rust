//! The links in e-mail notices that stop them without signing in: /ndalo/{token}/{what}, the token
//! the account's own (db::notice_token, migration 0039), `what` the e-mails stopped: "mesazhet",
//! the notices of unread messages (notices.rs), as the box „Mesazhet që nuk i ke lexuar“ on
//! "Cilësimet" stops them; "kerkimi-{id}", one saved search's alerts (searches.rs), as its switch
//! on "Kërkimet e ruajtura" does; "email", which the expiry warnings' link stops (expiry.rs), every
//! notice by e-mail, as the box „Edhe me email“ does, the notifications staying on "Lajmërimet".
//! Opening a link asks first, so that a link checker following it stops nothing; its form, or a
//! mail program's one-click POST to the same address (the List-Unsubscribe-Post header, RFC 8058,
//! sender.rs), stops them; "Rikthe" takes it back.

use askama::Template;
use axum::{
    extract::{Path, State},
    response::Response,
};
use uuid::Uuid;

use super::{Result, not_found, render};
use crate::{AppState, db};

/// What a link stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum What {
    Messages,
    Search(Uuid),
    Email,
}

impl What {
    fn parse(what: &str) -> Option<Self> {
        match what {
            "mesazhet" => Some(Self::Messages),
            "email" => Some(Self::Email),
            what => what.strip_prefix("kerkimi-")?.parse().ok().map(Self::Search),
        }
    }

    /// What goes on as before, once they stop.
    fn stays(self) -> &'static str {
        match self {
            Self::Messages => "Mesazhet i gjen si gjithmonë te „Mesazhet“.",
            Self::Search(_) => "Kërkimi mbetet te „Kërkimet e ruajtura“.",
            Self::Email => "Lajmërimet i gjen te faqja, te „Lajmërimet“.",
        }
    }
}

/// Where a link's page stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Opened: whether to stop them.
    Ask,
    Stopped,
    /// "Rikthe": they come again.
    Back,
}

#[derive(Template)]
#[template(path = "unsubscribe.html")]
struct UnsubscribePage {
    step: Step,
    /// The e-mails it is about, as the page says them: "email-e për mesazhet që nuk i ke lexuar".
    about: String,
    /// What goes on as before (What::stays).
    stays: &'static str,
    /// The link's own address, which its forms post to.
    action: String,
}

/// The account behind the token and what the link stops, with how the page names it; None for a
/// token or a search that isn't (or is no longer) theirs.
async fn link(state: &AppState, token: &str, what: &str) -> Result<Option<(Uuid, What, String)>> {
    let (Some(user), Some(what)) = (db::notice_token_user(&state.db, token).await?, What::parse(what)) else {
        return Ok(None);
    };
    let about = match what {
        What::Messages => "email-e për mesazhet që nuk i ke lexuar".to_string(),
        What::Email => "lajmërimet me email nga Pluribus".to_string(),
        What::Search(id) => match db::saved_searches(&state.db, user).await?.into_iter().find(|search| search.id == id) {
            Some(search) => format!("email-e për kërkimin „{}“", search.name),
            None => return Ok(None),
        },
    };
    Ok(Some((user, what, about)))
}

/// The e-mails a link stops on or off again.
async fn set(state: &AppState, user: Uuid, what: What, on: bool) -> Result<()> {
    match what {
        What::Search(id) => {
            db::set_search_alerts(&state.db, user, id, on).await?;
        }
        What::Messages | What::Email => {
            let mut preferences = db::notice_preferences(&state.db, user).await?;
            match what {
                What::Messages => preferences.message_alerts = on,
                _ => preferences.email_enabled = on,
            }
            db::set_notice_preferences(&state.db, user, preferences).await?;
        }
    }
    Ok(())
}

/// The link's page at `step`, once what it stops is set as that step says.
async fn page(state: AppState, token: String, what: String, step: Step) -> Result<Response> {
    let Some((user, parsed, about)) = link(&state, &token, &what).await? else {
        return not_found().await;
    };
    match step {
        Step::Ask => {}
        Step::Stopped => set(&state, user, parsed, false).await?,
        Step::Back => set(&state, user, parsed, true).await?,
    }
    render(UnsubscribePage { step, about, stays: parsed.stays(), action: format!("/ndalo/{token}/{what}") })
}

/// GET /ndalo/{token}/{what}: whether to stop them.
pub async fn ask(State(state): State<AppState>, Path((token, what)): Path<(String, String)>) -> Result<Response> {
    page(state, token, what, Step::Ask).await
}

/// POST /ndalo/{token}/{what}: stops them, from the page's form or a mail program's one-click POST
/// (whose body, List-Unsubscribe=One-Click, says nothing more).
pub async fn stop(State(state): State<AppState>, Path((token, what)): Path<(String, String)>) -> Result<Response> {
    page(state, token, what, Step::Stopped).await
}

/// POST /ndalo/{token}/{what}/rikthe: they come again.
pub async fn back(State(state): State<AppState>, Path((token, what)): Path<(String, String)>) -> Result<Response> {
    page(state, token, what, Step::Back).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_names_what_it_stops() {
        assert_eq!(What::parse("mesazhet"), Some(What::Messages));
        assert_eq!(What::parse("email"), Some(What::Email));
        let id = Uuid::nil();
        assert_eq!(What::parse(&format!("kerkimi-{id}")), Some(What::Search(id)));
        assert_eq!(What::parse("kerkimi-jo"), None);
        assert_eq!(What::parse("tjeter"), None);
    }
}
