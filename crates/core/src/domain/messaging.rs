//! The rules about sending a message, apart from the edges that send one (the pages,
//! web/messages.rs, and the JSON API, api/messages.rs) and the database that keeps it
//! (db/messages.rs): who may write at all (`Writing`), how long a message may be, and how much one
//! account may send, which keeps spam down: in a minute and in a day; to new sellers in an hour
//! and in a day; and until the other side answers, three messages in a row at most and no links,
//! which is what spam and scams mostly send. `refuse` says why a message can't go as a code, with
//! the figures its words need (`Refusal`), which the API gives as it is (`Refusal::code`) and both
//! edges say in the language now (`Refusal::reason`). And what both edges say of a message alike:
//! what it is as it goes (`tidy`), its start as the inbox shows it (`preview`), and what a
//! conversation says of its listing once it is no longer simply up (`listing_label`). Nothing here
//! reads the database or knows a page.

use super::{ListingType, Status};
use crate::i18n::{t, t1};

/// A message's length, in characters.
pub const MAX_CHARS: usize = 1000;
/// How much of a message the inbox shows, in characters, on one line (`preview`).
pub const PREVIEW_CHARS: usize = 90;
/// Messages one account sends: in a minute, which only a script outpaces, and in a day.
pub const PER_MINUTE: i64 = 8;
pub const PER_DAY: i64 = 200;
/// New sellers one account writes to (conversations it starts): in an hour, and in a day.
pub const NEW_PER_HOUR: i64 = 5;
pub const NEW_PER_DAY: i64 = 15;
/// Until the other side answers, how many messages in a row; and those carry no links.
pub const UNANSWERED: i64 = 3;

/// What the conversation's page lets its visitor do: write, or why they can't.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writing {
    Open,
    /// They blocked the other side, who can't write to them either; unblocking opens it again.
    Blocked,
    /// The other side blocked them.
    BlockedBy,
    /// A listing that takes no messages: down, sold, or with messages turned off.
    Closed,
}

impl Writing {
    /// For a listing that takes messages or doesn't (`open`: a conversation once started always
    /// does), with the blocks between the visitor and the other side, either way, coming first.
    pub fn of(open: bool, blocked: bool, blocked_by: bool) -> Self {
        if blocked {
            Writing::Blocked
        } else if blocked_by {
            Writing::BlockedBy
        } else if open {
            Writing::Open
        } else {
            Writing::Closed
        }
    }

    /// Why a message can't go, for a form sent anyway: nothing while the page is open to write.
    pub fn refusal(self) -> Option<Refusal> {
        match self {
            Writing::Open => None,
            Writing::Blocked => Some(Refusal::Blocked),
            Writing::BlockedBy => Some(Refusal::BlockedBy),
            Writing::Closed => Some(Refusal::Closed),
        }
    }
}

/// What an account sent lately (db::sent_lately), for the limits: messages in the last minute and
/// day, and conversations it started (sellers it wrote to) in the last hour and day.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SentLately {
    pub last_minute: i64,
    pub last_day: i64,
    pub started_last_hour: i64,
    pub started_last_day: i64,
}

/// A message about to go, as the limits see it.
#[derive(Debug, Clone, Copy)]
pub struct Outgoing<'a> {
    /// What it says, as it is sent: without space around it (web/messages.rs's `tidy`).
    pub body: &'a str,
    /// How many links it holds, as a listing's text counts them (web/post.rs's `link_count`).
    pub links: usize,
    /// While the other side hasn't answered yet, how many messages in a row its sender's side sent
    /// (none, for the first); None once the other side has answered.
    pub unanswered: Option<i64>,
    /// Whether it starts a conversation, to a seller its sender hasn't written to about the listing.
    pub new_seller: bool,
}

/// Why a message can't go: a code, not words, with the figures its words say; the page says it in
/// its language (web/messages.rs), naming the other side where it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Its sender blocked the other side (`Writing::Blocked`).
    Blocked,
    /// The other side blocked its sender (`Writing::BlockedBy`).
    BlockedBy,
    /// The listing takes no messages now (`Writing::Closed`).
    Closed,
    /// Nothing in it.
    Empty,
    /// Longer than its `most` characters (MAX_CHARS).
    TooLong { most: usize },
    /// Its sender's side sent `most` in a row already (UNANSWERED), and the other side hasn't
    /// answered yet.
    TooManyInARow { most: i64 },
    /// A link, before the other side answered.
    LinksBeforeAnswer,
    /// Its sender sent `most` messages in the last minute already (PER_MINUTE).
    TooFast { most: i64 },
    /// Its sender sent `most` messages in the last day already (PER_DAY).
    TooManyToday { most: i64 },
    /// It starts a conversation, and its sender wrote to `most` new sellers in the last hour
    /// already (NEW_PER_HOUR).
    NewSellersThisHour { most: i64 },
    /// It starts a conversation, and its sender wrote to `most` new sellers in the last day
    /// already (NEW_PER_DAY).
    NewSellersToday { most: i64 },
}

impl Refusal {
    /// Its code, the same in every language, as the JSON API gives it (snake_case, as its error
    /// codes are): "blocked", "blocked_by", "closed", "empty", "too_long", "too_many_in_a_row",
    /// "links_before_answer", "too_fast", "too_many_today", "new_sellers_this_hour",
    /// "new_sellers_today".
    pub fn code(self) -> &'static str {
        match self {
            Refusal::Blocked => "blocked",
            Refusal::BlockedBy => "blocked_by",
            Refusal::Closed => "closed",
            Refusal::Empty => "empty",
            Refusal::TooLong { .. } => "too_long",
            Refusal::TooManyInARow { .. } => "too_many_in_a_row",
            Refusal::LinksBeforeAnswer => "links_before_answer",
            Refusal::TooFast { .. } => "too_fast",
            Refusal::TooManyToday { .. } => "too_many_today",
            Refusal::NewSellersThisHour { .. } => "new_sellers_this_hour",
            Refusal::NewSellersToday { .. } => "new_sellers_today",
        }
    }

    /// What it says, in the language now, naming the other side (`other`, their name as given)
    /// where it does: the words the pages have always said it in (web/messages.rs's `reason` until
    /// C11), which the JSON API says too.
    pub fn reason(self, other: &str) -> String {
        match self {
            Refusal::Blocked => t("E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.").to_string(),
            Refusal::BlockedBy => t1("{} nuk pranon mesazhe nga ti.", other),
            Refusal::Closed => t("Ky njoftim nuk pranon mesazhe tani.").to_string(),
            Refusal::Empty => t("Shkruaj mesazhin.").to_string(),
            Refusal::TooLong { most } => t1("Mesazhi mund të ketë deri në {} shkronja.", most),
            Refusal::TooManyInARow { .. } => t1("Prit që {} të të përgjigjet para se të shkruash më shumë.", other),
            Refusal::LinksBeforeAnswer => t1("Lidhjet mund t'i dërgosh pasi {} të të përgjigjet.", other),
            Refusal::TooFast { .. } => t("Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.").to_string(),
            Refusal::TooManyToday { .. } => t("Ke dërguar shumë mesazhe sot. Provo përsëri nesër.").to_string(),
            Refusal::NewSellersThisHour { most } => t1("U ke shkruar {} shitësve këtë orë. Prit pak para se t'i shkruash një tjetri.", most),
            Refusal::NewSellersToday { most } => t1("U ke shkruar {} shitësve sot. Mund t'u shkruash të tjerëve nesër.", most),
        }
    }
}

/// Why this message can't go, if it can't: who may write first (`writing`), then what it says, then
/// how much its sender sent lately (`sent`).
pub fn refuse(writing: Writing, message: &Outgoing<'_>, sent: &SentLately) -> Option<Refusal> {
    if let Some(refusal) = writing.refusal() {
        return Some(refusal);
    }
    let chars = message.body.chars().count();
    if chars == 0 {
        return Some(Refusal::Empty);
    }
    if chars > MAX_CHARS {
        return Some(Refusal::TooLong { most: MAX_CHARS });
    }
    if let Some(in_a_row) = message.unanswered {
        if in_a_row >= UNANSWERED {
            return Some(Refusal::TooManyInARow { most: UNANSWERED });
        }
        if message.links > 0 {
            return Some(Refusal::LinksBeforeAnswer);
        }
    }
    if sent.last_minute >= PER_MINUTE {
        Some(Refusal::TooFast { most: PER_MINUTE })
    } else if sent.last_day >= PER_DAY {
        Some(Refusal::TooManyToday { most: PER_DAY })
    } else if message.new_seller && sent.started_last_hour >= NEW_PER_HOUR {
        Some(Refusal::NewSellersThisHour { most: NEW_PER_HOUR })
    } else if message.new_seller && sent.started_last_day >= NEW_PER_DAY {
        Some(Refusal::NewSellersToday { most: NEW_PER_DAY })
    } else {
        None
    }
}

/// A message as it is sent and kept, whichever edge it came from: its line breaks the same
/// everywhere, without space around it (`Outgoing::body`).
pub fn tidy(body: &str) -> String {
    body.replace("\r\n", "\n").trim().to_string()
}

/// The start of a message as the inbox shows it, on one line: its white space a single space
/// each, and past PREVIEW_CHARS characters, the first PREVIEW_CHARS - 1 and an ellipsis.
pub fn preview(body: &str) -> String {
    let text = body.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.chars().count() > PREVIEW_CHARS {
        true => format!("{}…", text.chars().take(PREVIEW_CHARS - 1).collect::<String>()),
        false => text,
    }
}

/// What a conversation says of its listing once it is no longer simply up, from the status and
/// the kind its row spells (Status::conversation_label: "Rezervuar", "Shitur" or "Dhënë me qira",
/// "Jo më në faqe"), in the language now; nothing while it is up. A status the schema doesn't
/// allow counts as off the site, and a kind it doesn't as one on sale.
pub fn listing_label(status: Option<&str>, kind: Option<&str>) -> Option<&'static str> {
    let kind = kind.and_then(ListingType::parse).unwrap_or(ListingType::Sell);
    Status::parse(status?).unwrap_or(Status::Deleted).conversation_label(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nothing sent lately.
    const QUIET: SentLately = SentLately { last_minute: 0, last_day: 0, started_last_hour: 0, started_last_day: 0 };

    /// A first message, which starts a conversation with a new seller: no answer yet, none in a row.
    fn first(body: &str) -> Outgoing<'_> {
        Outgoing { body, links: 0, unanswered: Some(0), new_seller: true }
    }

    /// A message into a conversation the other side has answered in.
    fn reply(body: &str) -> Outgoing<'_> {
        Outgoing { body, links: 0, unanswered: None, new_seller: false }
    }

    #[test]
    fn a_message_within_every_limit_goes() {
        assert_eq!(refuse(Writing::Open, &first("Përshëndetje! A është ende në dispozicion?"), &QUIET), None);
        // As long as it may be, counted in characters, not bytes.
        assert_eq!(refuse(Writing::Open, &first(&"ë".repeat(MAX_CHARS)), &QUIET), None);
        // Once the other side answered, links go too, however many came in a row.
        let answered = Outgoing { links: 2, ..reply("Ja ku jam: https://maps.example.com/x www.example.com") };
        assert_eq!(refuse(Writing::Open, &answered, &QUIET), None);
        // One short of every limit.
        let busy = SentLately {
            last_minute: PER_MINUTE - 1,
            last_day: PER_DAY - 1,
            started_last_hour: NEW_PER_HOUR - 1,
            started_last_day: NEW_PER_DAY - 1,
        };
        assert_eq!(refuse(Writing::Open, &first("Kur mund ta shoh?"), &busy), None);
        let second = Outgoing { unanswered: Some(UNANSWERED - 1), ..reply("Alo?") };
        assert_eq!(refuse(Writing::Open, &second, &busy), None);
        // Sellers written to lately don't limit a message into a conversation started already.
        let started = SentLately { started_last_hour: NEW_PER_HOUR, started_last_day: NEW_PER_DAY, ..QUIET };
        assert_eq!(refuse(Writing::Open, &reply("Po, nesër pasdite."), &started), None);
    }

    #[test]
    fn a_block_either_way_or_a_closed_listing_comes_before_the_rest() {
        assert_eq!(Writing::of(true, false, false), Writing::Open);
        assert_eq!(Writing::of(false, false, false), Writing::Closed);
        assert_eq!(Writing::of(true, false, true), Writing::BlockedBy);
        // The visitor's own block first, whatever else holds.
        assert_eq!(Writing::of(false, true, true), Writing::Blocked);
        assert_eq!(Writing::of(false, false, true), Writing::BlockedBy);
        // Refused for it before anything the message says or its sender sent counts.
        let flooded = SentLately { last_minute: 99, last_day: 999, started_last_hour: 99, started_last_day: 99 };
        let empty = first("");
        assert_eq!(refuse(Writing::Blocked, &empty, &flooded), Some(Refusal::Blocked));
        assert_eq!(refuse(Writing::BlockedBy, &empty, &flooded), Some(Refusal::BlockedBy));
        assert_eq!(refuse(Writing::Closed, &empty, &flooded), Some(Refusal::Closed));
        assert_eq!(Writing::Open.refusal(), None);
    }

    #[test]
    fn an_empty_or_too_long_message_is_refused() {
        assert_eq!(refuse(Writing::Open, &first(""), &QUIET), Some(Refusal::Empty));
        assert_eq!(refuse(Writing::Open, &first(&"a".repeat(MAX_CHARS + 1)), &QUIET), Some(Refusal::TooLong { most: 1000 }));
        // What it says counts before how much its sender sent.
        let flooded = SentLately { last_minute: PER_MINUTE, ..QUIET };
        assert_eq!(refuse(Writing::Open, &reply(""), &flooded), Some(Refusal::Empty));
    }

    #[test]
    fn until_the_other_side_answers_three_in_a_row_and_no_links() {
        let fourth = Outgoing { unanswered: Some(UNANSWERED), ..reply("Alo?") };
        assert_eq!(refuse(Writing::Open, &fourth, &QUIET), Some(Refusal::TooManyInARow { most: 3 }));
        let link = Outgoing { links: 1, ..first("Shiko www.example.com") };
        assert_eq!(refuse(Writing::Open, &link, &QUIET), Some(Refusal::LinksBeforeAnswer));
        let second_link = Outgoing { links: 1, unanswered: Some(1), ..reply("https://example.com") };
        assert_eq!(refuse(Writing::Open, &second_link, &QUIET), Some(Refusal::LinksBeforeAnswer));
        // Too many in a row, whatever it holds.
        let fourth_link = Outgoing { links: 1, ..fourth };
        assert_eq!(refuse(Writing::Open, &fourth_link, &QUIET), Some(Refusal::TooManyInARow { most: 3 }));
    }

    #[test]
    fn too_many_in_a_minute_or_a_day_are_refused() {
        let minute = SentLately { last_minute: PER_MINUTE, last_day: PER_MINUTE, ..QUIET };
        assert_eq!(refuse(Writing::Open, &reply("Alo?"), &minute), Some(Refusal::TooFast { most: 8 }));
        let day = SentLately { last_day: PER_DAY, ..QUIET };
        assert_eq!(refuse(Writing::Open, &reply("Alo?"), &day), Some(Refusal::TooManyToday { most: 200 }));
        // The minute's limit first.
        let both = SentLately { last_minute: PER_MINUTE, last_day: PER_DAY, ..QUIET };
        assert_eq!(refuse(Writing::Open, &first("Alo?"), &both), Some(Refusal::TooFast { most: 8 }));
    }

    #[test]
    fn a_few_new_sellers_an_hour_and_a_day() {
        let hour = SentLately { started_last_hour: NEW_PER_HOUR, started_last_day: NEW_PER_HOUR, ..QUIET };
        assert_eq!(refuse(Writing::Open, &first("Përshëndetje"), &hour), Some(Refusal::NewSellersThisHour { most: 5 }));
        let day = SentLately { started_last_day: NEW_PER_DAY, ..QUIET };
        assert_eq!(refuse(Writing::Open, &first("Përshëndetje"), &day), Some(Refusal::NewSellersToday { most: 15 }));
        // The day's messages before the sellers.
        let flooded = SentLately { last_day: PER_DAY, ..hour };
        assert_eq!(refuse(Writing::Open, &first("Përshëndetje"), &flooded), Some(Refusal::TooManyToday { most: 200 }));
    }

    #[test]
    fn each_refusal_says_why_in_the_pages_words_under_a_code_of_its_own() {
        let said = |refusal: Refusal| refusal.reason("Ana");
        assert_eq!(said(Refusal::Blocked), "E ke bllokuar këtë përdorues. Zhbllokoje për t'i shkruar.");
        assert_eq!(said(Refusal::BlockedBy), "Ana nuk pranon mesazhe nga ti.");
        assert_eq!(said(Refusal::Closed), "Ky njoftim nuk pranon mesazhe tani.");
        assert_eq!(said(Refusal::Empty), "Shkruaj mesazhin.");
        assert_eq!(said(Refusal::TooLong { most: MAX_CHARS }), "Mesazhi mund të ketë deri në 1000 shkronja.");
        assert_eq!(said(Refusal::TooManyInARow { most: 3 }), "Prit që Ana të të përgjigjet para se të shkruash më shumë.");
        assert_eq!(said(Refusal::LinksBeforeAnswer), "Lidhjet mund t'i dërgosh pasi Ana të të përgjigjet.");
        assert_eq!(said(Refusal::TooFast { most: 8 }), "Po dërgon shumë shpejt. Prit një minutë dhe provo përsëri.");
        assert_eq!(said(Refusal::TooManyToday { most: 200 }), "Ke dërguar shumë mesazhe sot. Provo përsëri nesër.");
        assert_eq!(
            said(Refusal::NewSellersThisHour { most: 5 }),
            "U ke shkruar 5 shitësve këtë orë. Prit pak para se t'i shkruash një tjetri."
        );
        assert_eq!(said(Refusal::NewSellersToday { most: 15 }), "U ke shkruar 15 shitësve sot. Mund t'u shkruash të tjerëve nesër.");
        // In English, as an English page says them.
        let english = crate::i18n::LANG.sync_scope(crate::i18n::Lang::En, || said(Refusal::BlockedBy));
        assert_eq!(english, "Ana doesn’t accept messages from you.");
        // Each code its own.
        let every = [
            Refusal::Blocked,
            Refusal::BlockedBy,
            Refusal::Closed,
            Refusal::Empty,
            Refusal::TooLong { most: MAX_CHARS },
            Refusal::TooManyInARow { most: UNANSWERED },
            Refusal::LinksBeforeAnswer,
            Refusal::TooFast { most: PER_MINUTE },
            Refusal::TooManyToday { most: PER_DAY },
            Refusal::NewSellersThisHour { most: NEW_PER_HOUR },
            Refusal::NewSellersToday { most: NEW_PER_DAY },
        ];
        let mut codes: Vec<&str> = every.iter().map(|refusal| refusal.code()).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), every.len(), "{codes:?}");
        assert_eq!(Refusal::TooManyInARow { most: 3 }.code(), "too_many_in_a_row");
    }

    #[test]
    fn messages_go_without_space_around_them_and_the_inbox_shows_their_start_on_one_line() {
        assert_eq!(tidy("  Përshëndetje!\r\nA është ende?  \n"), "Përshëndetje!\nA është ende?");
        assert_eq!(preview("Përshëndetje!\nA është\tende?"), "Përshëndetje! A është ende?");
        let long = "a".repeat(PREVIEW_CHARS + 1);
        assert_eq!(preview(&long), format!("{}…", "a".repeat(PREVIEW_CHARS - 1)));
        assert_eq!(preview(&"ë".repeat(PREVIEW_CHARS)), "ë".repeat(PREVIEW_CHARS), "as long as it may be, in characters");
    }

    #[test]
    fn a_conversation_says_what_became_of_its_listing_once_it_is_no_longer_up() {
        assert_eq!(listing_label(Some("ACTIVE"), Some("SELL")), None);
        assert_eq!(listing_label(Some("RESERVED"), Some("SELL")), Some("Rezervuar"));
        assert_eq!(listing_label(Some("SOLD"), Some("SELL")), Some("Shitur"));
        assert_eq!(listing_label(Some("SOLD"), Some("RENT")), Some("Dhënë me qira"));
        assert_eq!(listing_label(Some("PAUSED"), None), Some("Jo më në faqe"));
        // What the schema doesn't allow: off the site, and on sale.
        assert_eq!(listing_label(Some("GONE"), Some("SWAP")), Some("Jo më në faqe"));
        assert_eq!(listing_label(Some("SOLD"), Some("SWAP")), Some("Shitur"));
        assert_eq!(listing_label(None, Some("SELL")), None, "no listing, nothing to say of it");
    }
}
