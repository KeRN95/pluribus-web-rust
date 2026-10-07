//! How many times each listing's page is seen, for the welcome page's "Më të klikuarat"
//! (db::most_viewed). The page says it was shown (application.js, POST /listing/{no}/pare), and
//! the views gather here, in memory, and go into the day's statistics (listing_daily_stats) every
//! FLUSH_EVERY in one statement, rather than one UPDATE per view, which would make a popular
//! listing's row a lock hot spot (README, "Analytics are aggregated"). A visitor (their address and
//! browser) counts once per listing per AGAIN_AFTER, however often they reload or come back to it.
//! Views not yet written when the server stops are lost: a minute's at most. The most seen listings
//! are kept here too, for as long: they can't change any faster.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sqlx::PgPool;
use uuid::Uuid;

use crate::db::{self, ListingCard};

/// How often the views gathered are written.
pub const FLUSH_EVERY: Duration = Duration::from_secs(60);

/// How long the same visitor seeing the same listing again counts for nothing.
pub const AGAIN_AFTER: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct Views(Arc<Inner>);

struct Inner {
    /// Zero: each view is written at once (the tests).
    every: Duration,
    pending: Mutex<Pending>,
    /// The most seen listings, and when they were read.
    most_viewed: Mutex<MostViewed>,
}

/// The most seen listings, and when they were read: in every category (None), and in each category
/// that shows its own (by its id).
type MostViewed = HashMap<Option<Uuid>, (Instant, Vec<ListingCard>)>;

#[derive(Default)]
struct Pending {
    /// Views not yet written, by the listing's number.
    counts: HashMap<i64, i64>,
    /// When each visitor last counted for each listing, by a hash of who they are.
    seen: HashMap<(u64, i64), Instant>,
}

impl Views {
    /// Views gathered and written every `every`, by the task flush_now_and_then starts.
    pub fn buffered(every: Duration) -> Self {
        Self(Arc::new(Inner { every, pending: Mutex::default(), most_viewed: Mutex::default() }))
    }

    /// Each view written as it comes, for the tests.
    pub fn immediate() -> Self {
        Self::buffered(Duration::ZERO)
    }

    /// Counts a view of listing `public_no` by `visitor` (whatever tells visitors apart: their
    /// address and browser), unless they counted for it within AGAIN_AFTER.
    pub async fn record(&self, db: &PgPool, public_no: i64, visitor: &str) -> sqlx::Result<()> {
        let mut hasher = DefaultHasher::new();
        visitor.hash(&mut hasher);
        let key = (hasher.finish(), public_no);
        let now = Instant::now();
        {
            let mut pending = self.0.pending.lock().expect("the views' lock");
            match pending.seen.get(&key) {
                Some(at) if now.duration_since(*at) < AGAIN_AFTER => return Ok(()),
                _ => {}
            }
            pending.seen.insert(key, now);
            *pending.counts.entry(public_no).or_default() += 1;
        }
        if self.0.every.is_zero() {
            self.flush(db).await?;
        }
        Ok(())
    }

    /// Writes the views gathered so far, and forgets visitors seen longer ago than AGAIN_AFTER.
    /// If writing fails, the views wait for the next time.
    pub async fn flush(&self, db: &PgPool) -> sqlx::Result<()> {
        let counts = {
            let mut pending = self.0.pending.lock().expect("the views' lock");
            let now = Instant::now();
            pending.seen.retain(|_, at| now.duration_since(*at) < AGAIN_AFTER);
            std::mem::take(&mut pending.counts)
        };
        if counts.is_empty() {
            return Ok(());
        }
        let (numbers, views): (Vec<i64>, Vec<i64>) = counts.iter().map(|(no, n)| (*no, *n)).unzip();
        if let Err(error) = db::add_views(db, &numbers, &views).await {
            let mut pending = self.0.pending.lock().expect("the views' lock");
            for (no, n) in counts {
                *pending.counts.entry(no).or_default() += n;
            }
            return Err(error);
        }
        Ok(())
    }

    /// The listings seen the most this week (db::most_viewed), in every category, or in one and
    /// those under it (`category`: its id, and all of theirs); read again once they are older than
    /// the views written since.
    pub async fn most_viewed(&self, db: &PgPool, category: Option<(Uuid, &[Uuid])>) -> sqlx::Result<Vec<ListingCard>> {
        let every = self.0.every;
        let key = category.map(|(id, _)| id);
        if let Some((at, cards)) = self.0.most_viewed.lock().expect("the views' lock").get(&key) {
            if at.elapsed() < every {
                return Ok(cards.clone());
            }
        }
        let ids = category.map(|(_, ids)| ids);
        let cards = db::most_viewed(db, ids, db::MOST_VIEWED_DAYS, db::MOST_VIEWED).await?;
        if !every.is_zero() {
            self.0.most_viewed.lock().expect("the views' lock").insert(key, (Instant::now(), cards.clone()));
        }
        Ok(cards)
    }

    /// Starts the task that writes the views every FLUSH_EVERY (or the `every` given).
    pub fn flush_now_and_then(&self, db: PgPool) {
        let views = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(views.0.every);
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(error) = views.flush(&db).await {
                    tracing::warn!(%error, "could not write the listings' views");
                }
            }
        });
    }
}
