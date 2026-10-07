//! How many times each listing's page is seen, for the welcome page's "Më të klikuarat"
//! (db::most_viewed). The page says it was shown (application.js, POST /listing/{no}/pare), and
//! the views gather here, in memory, and go into the day's statistics (listing_daily_stats) every
//! FLUSH_EVERY in one statement, rather than one UPDATE per view, which would make a popular
//! listing's row a lock hot spot (README, "Analytics are aggregated"). A visitor (their address and
//! browser) counts once per listing per AGAIN_AFTER, however often they reload or come back to it.
//! Views not yet written when the server stops are lost: a minute's at most. The most seen listings
//! are kept here too, so that no page waits for them: read as the server starts, for the welcome
//! page and each category page that shows its own, then read again after each writing, for the
//! pages that asked for them lately; they can't change any faster.

use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sqlx::PgPool;
use uuid::Uuid;

use crate::db::{self, Category, ListingCard};

/// How often the views gathered are written.
pub const FLUSH_EVERY: Duration = Duration::from_secs(60);

/// The subcategories whose pages show their own most seen, besides every main category's.
pub const MOST_VIEWED_IN: [&str; 3] = ["makina", "apartamente", "vila-dhe-shtepi"];

/// Whether a category's page shows its own most seen (web/browse.rs): every main category's does,
/// and those of MOST_VIEWED_IN.
pub fn has_own_most_viewed(category: &Category) -> bool {
    category.parent_id.is_none() || MOST_VIEWED_IN.contains(&category.slug.as_str())
}

/// For how long after a page last asked for a row of the most seen it is read again every
/// FLUSH_EVERY (read_again); one asked for again later, read longer ago than STALE, is read before
/// it is shown.
const KEEP_WARM: Duration = Duration::from_secs(10 * 60);
const STALE: Duration = Duration::from_secs(5 * 60);

/// How long the same visitor seeing the same listing again counts for nothing.
pub const AGAIN_AFTER: Duration = Duration::from_secs(30 * 60);

#[derive(Clone)]
pub struct Views(Arc<Inner>);

struct Inner {
    /// Zero: each view is written at once (the tests).
    every: Duration,
    pending: Mutex<Pending>,
    /// The most seen listings, in rows.
    most_viewed: Mutex<MostViewed>,
}

/// The most seen listings: in every category (None), and in each category that shows its own (by
/// its id).
type MostViewed = HashMap<Option<Uuid>, Row>;

/// A row of the most seen: the categories it looks in (None: every one), its listings, when they
/// were read, and when a page last asked for them.
struct Row {
    categories: Option<Vec<Uuid>>,
    cards: Vec<ListingCard>,
    read: Instant,
    asked: Instant,
}

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
    /// those under it (`category`: its id, and all of theirs). The task reads them in the
    /// background (warm, read_again), so that a page doesn't wait for them: only a row no page
    /// asked for in a while is read as it is asked for, as all of them are in the tests.
    pub async fn most_viewed(&self, db: &PgPool, category: Option<(Uuid, &[Uuid])>) -> sqlx::Result<Vec<ListingCard>> {
        let key = category.map(|(id, _)| id);
        let ids = category.map(|(_, ids)| ids);
        if self.0.every.is_zero() {
            return db::most_viewed(db, ids, db::MOST_VIEWED_DAYS, db::MOST_VIEWED).await;
        }
        let asked = Instant::now();
        if let Some(row) = self.0.most_viewed.lock().expect("the views' lock").get_mut(&key) {
            row.asked = asked;
            if row.read.elapsed() < STALE {
                return Ok(row.cards.clone());
            }
        }
        let cards = db::most_viewed(db, ids, db::MOST_VIEWED_DAYS, db::MOST_VIEWED).await?;
        let row = Row { categories: ids.map(<[Uuid]>::to_vec), cards: cards.clone(), read: Instant::now(), asked };
        self.0.most_viewed.lock().expect("the views' lock").insert(key, row);
        Ok(cards)
    }

    /// Reads the rows of every page that shows one, as the server starts: the welcome page's, and
    /// each category's that has its own (has_own_most_viewed).
    async fn warm(&self, db: &PgPool) {
        let categories = match db::categories(db).await {
            Ok(categories) => categories,
            Err(error) => {
                tracing::warn!(%error, "could not read the categories whose most seen to read");
                return;
            }
        };
        let rows: Vec<(Uuid, Vec<Uuid>)> = categories
            .top_level()
            .chain(MOST_VIEWED_IN.iter().filter_map(|slug| categories.by_slug(slug)))
            .map(|category| (category.id, categories.subtree_ids(category.id)))
            .collect();
        let every_category = std::iter::once(None);
        for category in every_category.chain(rows.iter().map(|(id, ids)| Some((*id, ids.as_slice())))) {
            if let Err(error) = self.most_viewed(db, category).await {
                tracing::warn!(%error, "could not read the most seen listings");
            }
        }
    }

    /// Reads again the rows a page asked for within KEEP_WARM.
    async fn read_again(&self, db: &PgPool) {
        let due: Vec<(Option<Uuid>, Option<Vec<Uuid>>)> = self.0.most_viewed.lock().expect("the views' lock")
            .iter()
            .filter(|(_, row)| row.asked.elapsed() < KEEP_WARM)
            .map(|(key, row)| (*key, row.categories.clone()))
            .collect();
        for (key, categories) in due {
            match db::most_viewed(db, categories.as_deref(), db::MOST_VIEWED_DAYS, db::MOST_VIEWED).await {
                Ok(cards) => {
                    if let Some(row) = self.0.most_viewed.lock().expect("the views' lock").get_mut(&key) {
                        row.cards = cards;
                        row.read = Instant::now();
                    }
                }
                Err(error) => tracing::warn!(%error, "could not read the most seen listings again"),
            }
        }
    }

    /// Starts the task that reads every row of the most seen (warm), then every FLUSH_EVERY (or the
    /// `every` given) writes the views gathered, and reads again the rows pages asked for lately.
    pub fn flush_now_and_then(&self, db: PgPool) {
        let views = self.clone();
        tokio::spawn(async move {
            views.warm(&db).await;
            let mut tick = tokio::time::interval(views.0.every);
            tick.tick().await;
            loop {
                tick.tick().await;
                if let Err(error) = views.flush(&db).await {
                    tracing::warn!(%error, "could not write the listings' views");
                }
                views.read_again(&db).await;
            }
        });
    }
}
