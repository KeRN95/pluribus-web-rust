//! The welcome page's row of the newest listings, which follows new ones while the page is open:
//! the page asks for them at /te-rejat (`<turbo-stream-source>` in home.html), and the server
//! sends each listing posted since as a server-sent event, a Turbo Stream that puts its card in
//! front of the row (newest_stream.html, newest_controller.js), no more than two a second.

use std::{collections::VecDeque, convert::Infallible, sync::LazyLock, time::Duration};

use askama::Template;
use axum::{
    extract::{RawQuery, State},
    http::HeaderMap,
    response::{
        IntoResponse, Response,
        sse::{Event, Sse},
    },
};
use futures_util::stream;
use sqlx::PgPool;
use time::OffsetDateTime;
use tokio::{sync::watch, time::Instant};
use uuid::Uuid;

use super::filters::Params;
use crate::{
    AppState,
    db::{self, Cursor, ListingCard, Sort},
};

/// How many listings the row starts with, the newest, and how many of the newest a stream looks
/// at; as new ones come, the page lets the row grow to thirty (newest_controller.js, KEEP).
pub const STRIP: usize = 10;

/// How often a stream looks for listings posted since it last looked.
const LOOK_EVERY: Duration = Duration::from_secs(1);

/// The least time between two listings a stream sends: no more than two a second, however many
/// are posted at once. The row keeps only the newest, and so does the line of those waiting.
const SEND_EVERY: Duration = Duration::from_millis(500);

/// How long one response lasts. Then it ends, and EventSource connects again RETRY later, going on
/// from the last listing it got (Last-Event-ID). Where nothing stands in between, a listing comes
/// the moment it is posted all the same; a proxy that holds every response back until it ends, as
/// Cloudflare's quick tunnels do, then passes it on a few seconds late rather than never.
const WINDOW: Duration = Duration::from_secs(4);
const RETRY: Duration = Duration::from_secs(1);

/// Set as the server starts shutting down, which ends every stream: it would otherwise wait for
/// each page that has one open to close.
static CLOSING: LazyLock<watch::Sender<bool>> = LazyLock::new(|| watch::Sender::new(false));

/// Ends the streams open now, and any opened from now on (main.rs, as the server shuts down).
pub fn close_streams() {
    CLOSING.send_replace(true);
}

/// Where a listing stands in the newest-first order: when it was posted or last bumped, then its
/// id.
type Key = (OffsetDateTime, Uuid);

impl ListingCard {
    /// Where the listing stands in the newest-first order, as text (db::Cursor): where a stream
    /// starts after (`since`), and each event's id, which EventSource sends back when it connects
    /// again.
    pub fn newest_cursor(&self) -> String {
        Cursor::after(Sort::Newest, self).encode()
    }
}

/// A new listing's card, for the front of the row.
#[derive(Template)]
#[template(path = "newest_stream.html")]
struct NewestStream<'a> {
    card: &'a ListingCard,
}

/// GET /te-rejat?since=<cursor>: the listings posted after the one the page's row starts with,
/// then each listing as it is posted, as server-sent events, for WINDOW. Then EventSource connects
/// again and says the last listing it got (Last-Event-ID), which the stream goes on from, as when
/// the connection drops. Without either, it starts with the newest listings.
pub async fn stream(State(state): State<AppState>, headers: HeaderMap, RawQuery(query): RawQuery) -> Response {
    let params = Params::parse(query.as_deref());
    let since = headers
        .get("last-event-id")
        .and_then(|value| value.to_str().ok())
        .or_else(|| params.first("since"))
        .and_then(|value| match Cursor::decode(Sort::Newest, value)? {
            Cursor::Newest { sort_at, id } => Some((sort_at, id)),
            _ => None,
        });
    let follower = Follower {
        db: state.db,
        since,
        known: None,
        waiting: VecDeque::new(),
        next_look: Instant::now(),
        next_send: Instant::now(),
        until: Instant::now() + WINDOW,
        told_retry: false,
        closing: CLOSING.subscribe(),
    };
    let events = stream::unfold(follower, |follower| async move {
        let (event, follower) = follower.next().await?;
        Some((Ok::<_, Infallible>(event), follower))
    });
    Sse::new(events).into_response()
}

/// One page's stream: what it last saw, what waits to be sent, and when it may look and send.
struct Follower {
    db: PgPool,
    /// Where the page's row starts: what the first look finds after it is new.
    since: Option<Key>,
    /// The newest listings when the stream last looked, the newest first; None before it has.
    known: Option<Vec<Key>>,
    /// The new listings yet to be sent, the oldest first: at most STRIP.
    waiting: VecDeque<ListingCard>,
    next_look: Instant,
    next_send: Instant,
    /// When this response ends (WINDOW).
    until: Instant,
    /// Whether it has told EventSource to connect again RETRY after it ends.
    told_retry: bool,
    closing: watch::Receiver<bool>,
}

impl Follower {
    /// The next listing's event, as soon as there is one and SEND_EVERY has passed since the last
    /// one (first, how soon to connect again); None once WINDOW has passed, or the server is
    /// shutting down.
    async fn next(mut self) -> Option<(Event, Self)> {
        if !self.told_retry {
            self.told_retry = true;
            return Some((Event::default().retry(RETRY), self));
        }
        loop {
            let now = Instant::now();
            if now >= self.until {
                return None;
            }
            if now >= self.next_send {
                if let Some(card) = self.waiting.pop_front() {
                    self.next_send = now + SEND_EVERY;
                    match (NewestStream { card: &card }).render() {
                        // Each line a "data:" line of its own. The templates' lines may end in
                        // CR LF, and a CR would end a line of the event early.
                        Ok(html) => {
                            let event = Event::default().id(card.newest_cursor()).data(html.replace('\r', ""));
                            return Some((event, self));
                        }
                        Err(error) => tracing::warn!(%error, "could not render a new listing's card"),
                    }
                    continue;
                }
            }
            if now >= self.next_look {
                if let Err(error) = self.look().await {
                    tracing::warn!(%error, "could not look for new listings");
                }
                self.next_look = Instant::now() + LOOK_EVERY;
                continue;
            }
            let wake = if self.waiting.is_empty() { self.next_look } else { self.next_look.min(self.next_send) };
            tokio::select! {
                () = tokio::time::sleep_until(wake.min(self.until)) => {}
                _ = self.closing.wait_for(|closing| *closing) => return None,
            }
        }
    }

    /// Looks at the newest listings, and puts those posted or bumped since the last look in line
    /// to be sent; at the first look, those after where the page's row starts.
    async fn look(&mut self) -> sqlx::Result<()> {
        let newest = db::newest_listing_keys(&self.db, STRIP as i64).await?;
        let new = match &self.known {
            Some(known) => fresh(known, &newest),
            None => newest.iter().filter(|key| self.since.is_none_or(|since| **key > since)).map(|key| key.1).collect(),
        };
        self.known = Some(newest);
        if !new.is_empty() {
            for card in db::cards_by_id(&self.db, &new).await? {
                wait(&mut self.waiting, card, |card| card.id);
            }
        }
        Ok(())
    }
}

/// Of the newest listings now, those that weren't among them when last looked: posted or bumped
/// since. Not one that has only moved up among them because one of them has gone (sold, paused,
/// deleted): that one is older than all of them, when there were STRIP of them.
fn fresh(known: &[Key], newest: &[Key]) -> Vec<Uuid> {
    let oldest = known.iter().min().filter(|_| known.len() >= STRIP);
    newest
        .iter()
        .filter(|key| !known.contains(key) && oldest.is_none_or(|oldest| *key > oldest))
        .map(|key| key.1)
        .collect()
}

/// Puts `item` last in line, after taking out one for the same listing that waits already
/// (bumped again), and lets the first in line go past STRIP: the row only keeps the newest.
fn wait<T>(line: &mut VecDeque<T>, item: T, listing: impl Fn(&T) -> Uuid) {
    let id = listing(&item);
    line.retain(|waiting| listing(waiting) != id);
    line.push_back(item);
    if line.len() > STRIP {
        line.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A listing posted `second`s after a moment, with id `n`.
    fn key(second: i64, n: u128) -> Key {
        (OffsetDateTime::from_unix_timestamp(1_800_000_000 + second).unwrap(), Uuid::from_u128(n))
    }

    #[test]
    fn new_listings_are_those_posted_or_bumped_since_the_last_look() {
        // Ten listings, the newest first.
        let known: Vec<Key> = (1..=10).rev().map(|n| key(n, n as u128)).collect();
        // Two posted, which push the two oldest out.
        let posted: Vec<Key> = [key(12, 12), key(11, 11)].into_iter().chain(known[..8].iter().copied()).collect();
        assert_eq!(fresh(&known, &posted), [Uuid::from_u128(12), Uuid::from_u128(11)]);
        // One bumped, which comes first again.
        let bumped: Vec<Key> =
            [key(11, 3)].into_iter().chain(known.iter().copied().filter(|key| key.1 != Uuid::from_u128(3))).collect();
        assert_eq!(fresh(&known, &bumped), [Uuid::from_u128(3)]);
        // One gone, so that an older one moves up among the ten: nothing new.
        let gone: Vec<Key> =
            known.iter().copied().filter(|key| key.1 != Uuid::from_u128(5)).chain([key(0, 100)]).collect();
        assert!(fresh(&known, &gone).is_empty());
        // While there were fewer than ten, any that wasn't there is new.
        assert_eq!(fresh(&known[..3], &[key(11, 11), known[0], known[1], known[2]]), [Uuid::from_u128(11)]);
        assert!(fresh(&known, &known).is_empty());
    }

    #[test]
    fn the_line_keeps_the_newest_and_each_listing_once() {
        let mut line = VecDeque::new();
        for n in 0..12 {
            wait(&mut line, Uuid::from_u128(n), |id| *id);
        }
        assert_eq!(line.len(), STRIP);
        assert_eq!(line.front(), Some(&Uuid::from_u128(2)));
        // Bumped again while it waits: last in line, and there once.
        wait(&mut line, Uuid::from_u128(5), |id| *id);
        assert_eq!(line.len(), STRIP);
        assert_eq!(line.back(), Some(&Uuid::from_u128(5)));
        assert_eq!(line.iter().filter(|id| **id == Uuid::from_u128(5)).count(), 1);
    }
}
