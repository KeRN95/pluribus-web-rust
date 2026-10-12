//! The rules about sending a message, apart from the pages that send one (web/messages.rs) and the
//! database that keeps it (db/messages.rs): who may write at all (`Writing`), how long a message
//! may be, and how much one account may send, which keeps spam down: in a minute and in a day; to
//! new sellers in an hour and in a day; and until the other side answers, three messages in a row
//! at most and no links, which is what spam and scams mostly send. `refuse` says why a message
//! can't go as a code, with the figures its words need (`Refusal`), which the page says in its
//! language: nothing here reads the database or knows a page.

/// A message's length, in characters.
pub const MAX_CHARS: usize = 1000;
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
}
