//! The work that runs on its own, started once by the binary as the server starts (`start_all`,
//! from src/main.rs, since D2; until then `pluribus_web::state` started it, and main.rs rebuilt
//! the figures itself): the figures the pages read rebuilt and the old sign-in codes forgotten
//! (`refresh`, every REFRESH_EVERY), the notices of unread messages (notices.rs), the saved
//! searches' alerts (searches.rs), the listings' expiry with the drafts never put up (expiry.rs),
//! the promotions in time (promotions.rs), the euro rate (rates.rs), and, once as a server starts,
//! the copies the photos kept before them lack (media.rs).
//!
//! Each job runs in one process at a time, however many servers share the database: it has a
//! PostgreSQL advisory lock of its own (`Job::key`, the keys below; README, "pluribus-services"),
//! which a process takes before it runs the job (`run_locked`) and keeps, on a connection of its
//! own, for as long as it runs it. Another process finds the lock taken, says so once and runs
//! nothing, asking again every half minute at most (`looks_every`), so that it takes the job over
//! within that once the first stops: cleanly (`Jobs::stop`, which lets go of every lock), or not
//! (the database lets go of a session's locks as its connection ends). Two servers thus never send
//! a notice or an alert twice, nor expire, re-price or rebuild at the same time; and a binary built
//! on the library and run beside the server runs none of them unless it starts them. Until D2b a
//! process asked again only at the job's own interval, which left a job nobody's for up to a
//! quarter of an hour (six hours for the euro rate) after the server that held it went. What is a
//! process's own runs in each, unlocked: the views gathered in its memory, written every minute
//! (views.rs), whose counts add up whichever server writes them; the recommendations it keeps
//! (recommended.rs) have no task. README, "Before production", says what else a server keeps in
//! its memory alone.

use std::{future::Future, pin::pin, time::Duration};

use sqlx::{Connection, PgConnection, PgPool};
use tokio::{
    sync::watch,
    task::{JoinHandle, JoinSet},
    time::MissedTickBehavior,
};

use crate::{AppState, expiry, notices, promotions, rates, searches};

/// How often the figures the pages read are rebuilt, and the sign-in codes over a day old
/// forgotten (`refresh`): every quarter of an hour, so that they follow the listings posted
/// meanwhile.
pub const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);

/// The high half of every job's key, "PLUB" in ASCII, so that the site's locks are told apart from
/// any other program's on the database (sqlx takes one of its own while it migrates); the low half
/// is the job's number. `pg_locks` shows a key as its `classid`, 1347179842, and its `objid`, the
/// number.
const SITE: i64 = 0x504C_5542 << 32;

/// A job that runs in one process at a time: its lock's key, its name in the log, and how often it
/// runs, the first time as soon as its process takes the lock (Duration::ZERO for one that runs
/// once, as a server starts: `run_once_locked`).
#[derive(Clone, Copy, Debug)]
pub struct Job {
    pub key: i64,
    pub name: &'static str,
    pub every: Duration,
}

/// The figures the pages read rebuilt, and the old sign-in codes forgotten (`refresh`).
pub const REFRESH: Job = Job { key: SITE | 1, name: "refresh", every: REFRESH_EVERY };
/// The notices of unread messages (notices.rs).
pub const NOTICES: Job = Job { key: SITE | 2, name: "notices", every: notices::CHECK_EVERY };
/// The saved searches' alerts (searches.rs).
pub const SEARCH_ALERTS: Job = Job { key: SITE | 3, name: "search alerts", every: searches::ALERT_CHECK_EVERY };
/// The listings whose time is up expired, their sellers warned before, and the drafts never put up
/// forgotten (expiry.rs).
pub const EXPIRY: Job = Job { key: SITE | 4, name: "expiry", every: expiry::CHECK_EVERY };
/// The promotions in time (promotions.rs).
pub const PROMOTIONS: Job = Job { key: SITE | 5, name: "promotions", every: promotions::CHECK_EVERY };
/// The euro rate read and taken, and the prices in lekë worked out again (rates.rs).
pub const RATES: Job = Job { key: SITE | 6, name: "rates", every: rates::RATE_CHECK_EVERY };
/// The copies the photos kept before them lack (media.rs, `Media::fill_in_copies`): made once, as
/// a server starts, by the one that takes this lock; a server that finds another making them
/// leaves them to it.
pub const PHOTO_COPIES: Job = Job { key: SITE | 7, name: "photo copies", every: Duration::ZERO };

/// How long the connection holding a lock has to answer before each round, past which it counts as
/// gone, and the lock with it.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

/// How long `Jobs::stop` waits for the jobs to let go of their locks: a database that doesn't
/// answer doesn't hold a server's stop up, and lets go of them itself as their connections end.
const STOP_WITHIN: Duration = Duration::from_secs(10);

/// How often, at most, a process that finds a job's lock taken asks for it again (`looks_every`):
/// every half minute, so that once the server holding it goes (a deploy, a server or a one-off
/// binary run beside it for a while), the job is another's within half a minute rather than at
/// the end of its own interval, a quarter of an hour, six hours for the euro rate (until D2b).
pub const LOOK_EVERY: Duration = Duration::from_secs(30);

/// How often a process waiting for the lock of a job that runs `every` asks for it again:
/// LOOK_EVERY, or the job's own interval where that is shorter.
pub fn looks_every(every: Duration) -> Duration {
    every.min(LOOK_EVERY)
}

/// Starts every job, once per process, from the state the server serves (`pluribus_web::state`):
/// the senders the notices and alerts go by, the address their links start with (the settings',
/// `Config::site_origin`), the photo store; the locked ones each under its lock (`run_locked`, the
/// keys above), at once and then at their intervals, as they always started, and this process's
/// own, the views gathered (views.rs), unlocked. `main` calls it once the state is made, and
/// `Jobs::stop` once the server has stopped; dropped, the jobs stop too.
pub fn start_all(state: &AppState) -> Jobs {
    let mut jobs = Jobs::new();
    let db = &state.db;
    let origin = state.config.site_origin();
    jobs.spawn(db, REFRESH, each(db.clone(), |db| async move { refresh(&db).await }));
    jobs.own.spawn(state.views.flush_now_and_then(db.clone()));
    jobs.spawn_once(db, PHOTO_COPIES, {
        let media = state.media.clone();
        async move { media.fill_in_copies().await }
    });
    jobs.spawn(
        db,
        NOTICES,
        each((db.clone(), state.senders.clone(), origin.clone()), |(db, senders, origin)| async move {
            notices::tick(&db, &senders, &origin).await
        }),
    );
    jobs.spawn(
        db,
        SEARCH_ALERTS,
        each((db.clone(), state.senders.clone(), origin.clone()), |(db, senders, origin)| async move {
            searches::tick_alerts(&db, &senders, &origin).await
        }),
    );
    jobs.spawn(
        db,
        EXPIRY,
        each((db.clone(), state.media.clone(), state.senders.clone(), origin), |(db, media, senders, origin)| async move {
            expiry::tick(&db, &media, &senders, &origin).await
        }),
    );
    jobs.spawn(db, PROMOTIONS, each(db.clone(), |db| async move { promotions::tick(&db).await }));
    jobs.spawn(db, RATES, each(db.clone(), |db| async move { rates::check(&db).await }));
    jobs
}

/// A job's round, made afresh each time from a copy of `with` (the pool and what else the job
/// needs, each cheap to copy), since each round runs as a task of its own (`run_locked`).
fn each<T: Clone, R>(with: T, mut round: impl FnMut(T) -> R) -> impl FnMut() -> R {
    move || round(with.clone())
}

/// One rebuilding of the figures the pages read, over all the listings, so that they follow those
/// posted meanwhile: the words search fixes typos and finds words' other forms from
/// (db::refresh_search_words: a couple of seconds for 100 000 listings, and searches go on
/// meanwhile), what a square metre costs in each zone, which a property's page compares its price
/// with (db::refresh_zone_prices), what cars of each model ask (db::refresh_car_prices) and how
/// quickly each seller answers (db::refresh_response_times); and the sign-in codes asked for over
/// a day ago forgotten, with the contacts and addresses (IPs) they were asked from, as the privacy
/// page promises (db::forget_old_codes). A failure in the log, and the rest done all the same.
/// main.rs's own loop until D2.
pub async fn refresh(db: &PgPool) {
    if let Err(error) = pluribus_db::refresh_search_words(db).await {
        tracing::warn!(%error, "could not refresh the words search corrects typos from");
    }
    if let Err(error) = pluribus_db::refresh_zone_prices(db).await {
        tracing::warn!(%error, "could not refresh what a square metre costs in each zone");
    }
    if let Err(error) = pluribus_db::refresh_car_prices(db).await {
        tracing::warn!(%error, "could not refresh what cars of each model ask");
    }
    if let Err(error) = pluribus_db::refresh_response_times(db).await {
        tracing::warn!(%error, "could not refresh how quickly the sellers answer");
    }
    if let Err(error) = pluribus_db::forget_old_codes(db).await {
        tracing::warn!(%error, "could not forget the old sign-in codes");
    }
}

/// The jobs `start_all` started, each a task of its own: `stop` ends them, each letting go of its
/// lock. Dropped, they stop as well, as soon as their tasks next run.
#[must_use = "the jobs stop as soon as this is dropped"]
pub struct Jobs {
    /// Says `true` to every job once they are to stop; dropped, it tells them so too.
    stop: watch::Sender<bool>,
    running: Vec<JoinHandle<()>>,
    /// What is the process's own, which no lock holds (the views' writing): ended outright by
    /// `stop`, or as the jobs are dropped.
    own: JoinSet<()>,
}

impl Jobs {
    fn new() -> Self {
        Self { stop: watch::Sender::new(false), running: Vec::new(), own: JoinSet::new() }
    }

    /// What a job waits on for its end: `stop`, or the jobs dropped.
    fn stopped(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut stop = self.stop.subscribe();
        async move {
            // An error is the jobs dropped, which ends them as well.
            let _ = stop.wait_for(|stopped| *stopped).await;
        }
    }

    /// Runs `round` as `job` says, under its lock (`run_locked`), in a task of its own.
    fn spawn<F, R>(&mut self, db: &PgPool, job: Job, round: F)
    where
        F: FnMut() -> R + Send + 'static,
        R: Future<Output = ()> + Send + 'static,
    {
        let (db, stopped) = (db.clone(), self.stopped());
        self.running.push(tokio::spawn(async move { run_locked(&db, job, stopped, round).await }));
    }

    /// Runs `work` once, if this process takes `job`'s lock (`run_once_locked`), in a task of its
    /// own.
    fn spawn_once(&mut self, db: &PgPool, job: Job, work: impl Future<Output = ()> + Send + 'static) {
        let (db, stopped) = (db.clone(), self.stopped());
        self.running.push(tokio::spawn(async move { run_once_locked(&db, job, stopped, work).await }));
    }

    /// Ends every job: a round under way is dropped, as a server's stop always dropped it, and each
    /// lock this process holds is let go of (pg_advisory_unlock), so that another server takes its
    /// job over at its next look; and the process's own, the views' writing, whose views not yet
    /// written are lost, as they always were at a stop. Waits for them up to STOP_WITHIN.
    pub async fn stop(self) {
        let Self { stop, running, mut own } = self;
        stop.send_replace(true);
        let ended = async {
            own.shutdown().await;
            for job in running {
                // A job whose task panicked has said so (the panic's message), and holds nothing.
                let _ = job.await;
            }
        };
        if tokio::time::timeout(STOP_WITHIN, ended).await.is_err() {
            tracing::warn!("the jobs did not let go of their locks in time: the database does as their connections end");
        }
    }
}

/// Runs `round` every `job.every` for as long as this process holds the job's lock, the first time
/// as soon as it takes it, until `shutdown` comes, when it lets go of the lock. Where another
/// process holds it, this one runs nothing: it says so once, at info, and asks again every half
/// minute at most (`looks_every`; at the job's own interval until D2b), as it does after a failure
/// to ask, which is in the log. The lock is held on a connection of its own, out of the pool
/// (`take`), which must answer before each round: one that doesn't has ended, and the lock with
/// it, which another process may have taken since, so this one asks for it again at once. Each
/// round runs as a task of its own, so that one that panics is logged and the next comes at its
/// time (until D2 a panic ended its job until the server's restart); one under way when
/// `shutdown` comes is dropped, as a server's stop always dropped it, before the lock is let go
/// of. A round that fails says so itself, in its own module's words (notices.rs, expiry.rs...), as
/// the jobs always did.
pub async fn run_locked<F, R>(db: &PgPool, job: Job, shutdown: impl Future<Output = ()>, mut round: F)
where
    F: FnMut() -> R,
    R: Future<Output = ()> + Send + 'static,
{
    let mut shutdown = pin!(shutdown);
    // Whether another process held the lock at the last look: said once, until this one takes it.
    let mut waiting = false;
    loop {
        let taken = tokio::select! {
            () = &mut shutdown => return,
            taken = take(db, job.key) => taken,
        };
        match taken {
            Ok(Some(connection)) => {
                if waiting {
                    tracing::info!(job = job.name, "this process runs the job now");
                } else {
                    tracing::debug!(job = job.name, "this process runs the job");
                }
                waiting = false;
                let mut lock = Lock { connection, job };
                let mut every = tokio::time::interval(job.every);
                every.set_missed_tick_behavior(MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        () = &mut shutdown => {
                            lock.release().await;
                            return;
                        }
                        _ = every.tick() => {}
                    }
                    if let Err(error) = lock.answers().await {
                        tracing::warn!(%error, job = job.name, "the connection that held this job's lock is gone: asking for the lock again");
                        break;
                    }
                    let mut running = JoinSet::new();
                    running.spawn(round());
                    tokio::select! {
                        () = &mut shutdown => {
                            running.shutdown().await;
                            lock.release().await;
                            return;
                        }
                        ended = running.join_next() => {
                            if let Some(Err(error)) = ended
                                && error.is_panic()
                            {
                                tracing::error!(job = job.name, "a round of this job panicked: the next comes at its time");
                            }
                        }
                    }
                }
                // The lock went with its connection: asked for again at once, as any process may.
                continue;
            }
            Ok(None) => {
                if !waiting {
                    tracing::info!(job = job.name, asks_every = ?looks_every(job.every), "another process runs this job: this one asks for it again meanwhile");
                }
                waiting = true;
            }
            Err(error) => tracing::warn!(%error, job = job.name, "could not ask for this job's lock"),
        }
        tokio::select! {
            () = &mut shutdown => return,
            () = tokio::time::sleep(looks_every(job.every)) => {}
        }
    }
}

/// Runs `work` once, if this process takes `job`'s lock: a job for one process at a time that runs
/// once, as a server starts (PHOTO_COPIES). Where another process holds the lock, that one is
/// doing it, and this one leaves it to it (said at info). The lock is let go of once the work is
/// done, or once `shutdown` comes, which drops the work.
pub async fn run_once_locked(db: &PgPool, job: Job, shutdown: impl Future<Output = ()>, work: impl Future<Output = ()>) {
    let mut shutdown = pin!(shutdown);
    let taken = tokio::select! {
        () = &mut shutdown => return,
        taken = take(db, job.key) => taken,
    };
    match taken {
        Ok(Some(connection)) => {
            let lock = Lock { connection, job };
            tokio::select! {
                () = &mut shutdown => {}
                () = work => {}
            }
            lock.release().await;
        }
        Ok(None) => tracing::info!(job = job.name, "another process is doing this job: this one leaves it to it"),
        Err(error) => tracing::warn!(%error, job = job.name, "could not ask for this job's lock: the job waits for the next start"),
    }
}

/// This process's try for a job's lock (pg_try_advisory_lock, which never waits): the connection
/// that took it, out of the pool, or None where another process holds it. The try goes through a
/// pooled connection, back in the pool where the lock is another's (it holds nothing then); one
/// that took it leaves the pool (`PoolConnection::detach`), which may open another in its place,
/// so that the pages keep their ten; and one whose answer never came is closed rather than reused,
/// since it may hold the lock.
async fn take(db: &PgPool, key: i64) -> sqlx::Result<Option<PgConnection>> {
    let mut connection = db.acquire().await?;
    match sqlx::query_scalar::<_, bool>("SELECT pg_try_advisory_lock($1)").bind(key).fetch_one(&mut *connection).await {
        Ok(true) => Ok(Some(connection.detach())),
        Ok(false) => Ok(None),
        Err(error) => {
            connection.close_on_drop();
            Err(error)
        }
    }
}

/// A job's lock, held: the connection that holds it, and the job's.
struct Lock {
    connection: PgConnection,
    job: Job,
}

impl Lock {
    /// Whether its connection answers, within ANSWER_WITHIN (a round trip with nothing in it,
    /// `Connection::ping`): while it does, its session is there, and the lock with it.
    async fn answers(&mut self) -> Result<(), String> {
        match tokio::time::timeout(ANSWER_WITHIN, self.connection.ping()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err(format!("no answer within {} s", ANSWER_WITHIN.as_secs())),
        }
    }

    /// Lets go of the lock (pg_advisory_unlock), then closes its connection. Closing alone would
    /// let go of it too, as the session ends, but a connection kept open, as a pooled one is for
    /// the next to use, keeps its session's locks: the job's is let go of by its key.
    async fn release(mut self) {
        let unlocked = sqlx::query_scalar::<_, bool>("SELECT pg_advisory_unlock($1)")
            .bind(self.job.key)
            .fetch_one(&mut self.connection)
            .await;
        match unlocked {
            Ok(true) => tracing::debug!(job = self.job.name, "this process let go of the job"),
            Ok(false) => tracing::warn!(job = self.job.name, "this job's lock wasn't this process's to let go of"),
            Err(error) => tracing::warn!(%error, job = self.job.name, "could not let go of this job's lock: the database does as its connection ends"),
        }
        if let Err(error) = self.connection.close().await {
            tracing::debug!(%error, job = self.job.name, "the connection that held this job's lock did not close cleanly");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fmt::Debug,
        sync::{
            Arc, Mutex,
            atomic::{AtomicUsize, Ordering::SeqCst},
        },
    };

    use tracing::{
        Event, Level, Metadata, Subscriber,
        field::Field,
        span::{Attributes, Id, Record},
    };

    use super::*;
    use crate::{
        Moderators,
        config::Config,
        media::Media,
        recommended::Recommended,
        sender::Senders,
        views::{self, Views},
    };

    /// The tests' job, every 20 ms, under a key no job of the site's has.
    const TEST: Job = Job { key: SITE | 0xFF, name: "test", every: Duration::from_millis(20) };

    /// A round that counts itself.
    async fn count(rounds: Arc<AtomicUsize>) {
        rounds.fetch_add(1, SeqCst);
    }

    /// Whether `done` comes true within ten seconds, asked every 5 ms.
    async fn until(mut done: impl AsyncFnMut() -> bool) -> bool {
        let start = tokio::time::Instant::now();
        while !done().await {
            if start.elapsed() > Duration::from_secs(10) {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        true
    }

    /// The backend (its pid) that holds the tests' job's lock in the test's database, if one does.
    async fn holder(db: &PgPool) -> Option<i32> {
        sqlx::query_scalar(
            "SELECT pid FROM pg_locks
             WHERE locktype = 'advisory' AND granted AND objsubid = 1
               AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
               AND ((classid::bigint << 32) | objid::bigint) = $1",
        )
        .bind(TEST.key)
        .fetch_optional(db)
        .await
        .expect("pg_locks")
    }

    /// Two processes with the same job (two `Jobs`, each with its own connections): the second runs
    /// nothing while the first holds the lock, however many intervals pass, and takes the job over
    /// once the first stops, which lets go of it; the first runs no more.
    #[sqlx::test(migrations = false)]
    async fn one_process_at_a_time_runs_a_job(db: PgPool) {
        let (first, second) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let mut first_process = Jobs::new();
        first_process.spawn(&db, TEST, each(first.clone(), count));
        assert!(until(async || first.load(SeqCst) >= 2).await, "the first runs the job");
        let holding = holder(&db).await.expect("the first holds the lock");

        let mut second_process = Jobs::new();
        second_process.spawn(&db, TEST, each(second.clone(), count));
        let before = first.load(SeqCst);
        tokio::time::sleep(TEST.every * 10).await;
        assert_eq!(second.load(SeqCst), 0, "the second runs nothing while the first holds the lock");
        assert!(first.load(SeqCst) > before, "the first goes on");
        assert_eq!(holder(&db).await, Some(holding));

        first_process.stop().await;
        let stopped = first.load(SeqCst);
        assert!(until(async || second.load(SeqCst) >= 1).await, "the second takes the job over");
        assert_ne!(holder(&db).await, Some(holding));
        assert_eq!(first.load(SeqCst), stopped, "the first runs no more");
        second_process.stop().await;
        assert_eq!(holder(&db).await, None, "a job stopped holds nothing");
    }

    /// A process that ends without stopping its jobs, killed, or its task dropped as here, lets go
    /// of their locks all the same: its connection ends, and the database lets go of the session's
    /// locks with it.
    #[sqlx::test(migrations = false)]
    async fn a_process_that_ends_lets_go_of_its_jobs(db: PgPool) {
        let (first, second) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
        let first_process = tokio::spawn({
            let (db, round) = (db.clone(), each(first.clone(), count));
            async move { run_locked(&db, TEST, std::future::pending(), round).await }
        });
        assert!(until(async || first.load(SeqCst) >= 1).await, "the first runs the job");
        let holding = holder(&db).await.expect("the first holds the lock");
        let mut second_process = Jobs::new();
        second_process.spawn(&db, TEST, each(second.clone(), count));
        tokio::time::sleep(TEST.every * 5).await;
        assert_eq!(second.load(SeqCst), 0, "the second runs nothing while the first holds the lock");

        first_process.abort();
        assert!(until(async || second.load(SeqCst) >= 1).await, "the second takes the job over");
        assert_ne!(holder(&db).await, Some(holding));
        second_process.stop().await;
    }

    /// What the tests' own subscriber keeps of the log: each line at `level` or above, from every
    /// module or from those whose names start with `target`, its fields as `name=value`, the message
    /// among them.
    #[derive(Clone)]
    struct Logged {
        level: Level,
        target: Option<&'static str>,
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl Logged {
        fn new(level: Level, target: Option<&'static str>) -> Self {
            Self { level, target, lines: Arc::default() }
        }

        fn lines(&self) -> Vec<String> {
            self.lines.lock().unwrap().clone()
        }
    }

    impl Subscriber for Logged {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            *metadata.level() <= self.level && self.target.is_none_or(|target| metadata.target().starts_with(target))
        }

        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }

        fn record(&self, _: &Id, _: &Record<'_>) {}

        fn record_follows_from(&self, _: &Id, _: &Id) {}

        fn event(&self, event: &Event<'_>) {
            let mut line = String::new();
            event.record(&mut |field: &Field, value: &dyn Debug| line.push_str(&format!("{}={value:?} ", field.name())));
            self.lines.lock().unwrap().push(line);
        }

        fn enter(&self, _: &Id) {}

        fn exit(&self, _: &Id) {}
    }

    /// A round that fails, by an error its job logs as the site's jobs log theirs, or by a panic,
    /// which `run_locked` logs, ends neither the job nor its hold on the lock: the next round comes
    /// at its time.
    #[sqlx::test(migrations = false)]
    async fn a_round_that_fails_is_logged_and_the_next_comes(db: PgPool) {
        let logged = Logged::new(Level::WARN, None);
        let _logging = tracing::subscriber::set_default(logged.clone());
        let rounds = Arc::new(AtomicUsize::new(0));
        let mut process = Jobs::new();
        process.spawn(
            &db,
            TEST,
            each((db.clone(), rounds.clone()), |(db, rounds)| async move {
                match rounds.fetch_add(1, SeqCst) {
                    0 => {
                        if let Err(error) = sqlx::query("SELECT 1 / 0").execute(&db).await {
                            tracing::warn!(%error, "could not run the tests' round");
                        }
                    }
                    1 => panic!("the tests' second round panics"),
                    _ => {}
                }
            }),
        );
        assert!(until(async || rounds.load(SeqCst) >= 4).await, "rounds come after the two that failed");
        assert!(holder(&db).await.is_some(), "the job keeps its lock");
        process.stop().await;
        let lines = logged.lines();
        assert!(lines.iter().any(|line| line.contains("could not run the tests' round") && line.contains("division by zero")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("a round of this job panicked") && line.contains("job=\"test\"")), "{lines:?}");
    }

    /// A lock whose connection ends (the database restarted, or the backend ended by hand, as here)
    /// goes with it: before the next round the job finds the connection gone, asks for the lock
    /// again on another, and goes on.
    #[sqlx::test(migrations = false)]
    async fn a_job_whose_connection_ends_takes_its_lock_again(db: PgPool) {
        let rounds = Arc::new(AtomicUsize::new(0));
        let mut process = Jobs::new();
        process.spawn(&db, TEST, each(rounds.clone(), count));
        assert!(until(async || rounds.load(SeqCst) >= 1).await, "the job runs");
        let holding = holder(&db).await.expect("the job holds its lock");
        let ended: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)").bind(holding).fetch_one(&db).await.unwrap();
        assert!(ended);
        assert!(until(async || holder(&db).await.is_some_and(|pid| pid != holding)).await, "the lock taken again");
        let after = rounds.load(SeqCst);
        assert!(until(async || rounds.load(SeqCst) > after).await, "the rounds go on");
        process.stop().await;
        assert_eq!(holder(&db).await, None);
    }

    /// A job that runs once, as a server starts, is left to the process doing it, and done where no
    /// other process is: the photos' missing copies, made by one server.
    #[sqlx::test(migrations = false)]
    async fn a_job_run_once_is_left_to_the_process_doing_it(db: PgPool) {
        const ONCE: Job = Job { key: TEST.key, name: "test once", every: Duration::ZERO };
        let runs = Arc::new(AtomicUsize::new(0));
        // Another process doing it: its lock, taken by hand.
        let mut other = db.acquire().await.unwrap().detach();
        let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(ONCE.key).fetch_one(&mut other).await.unwrap();
        assert!(taken);
        run_once_locked(&db, ONCE, std::future::pending(), count(runs.clone())).await;
        assert_eq!(runs.load(SeqCst), 0, "left to the other process");

        let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)").bind(ONCE.key).fetch_one(&mut other).await.unwrap();
        assert!(unlocked);
        other.close().await.unwrap();
        run_once_locked(&db, ONCE, std::future::pending(), count(runs.clone())).await;
        assert_eq!(runs.load(SeqCst), 1, "done once no other process is doing it");
        assert_eq!(holder(&db).await, None, "and its lock let go of");
    }

    /// A process waiting for a job's lock asks for it again every half minute, or at the job's own
    /// interval where that is shorter: a job of a quarter of an hour's, or the euro rate's six
    /// hours, is another's within half a minute of its holder going.
    #[test]
    fn a_process_waiting_for_a_lock_asks_again_within_half_a_minute() {
        for job in [REFRESH, NOTICES, SEARCH_ALERTS, EXPIRY, PROMOTIONS, RATES] {
            assert_eq!(looks_every(job.every), Duration::from_secs(30), "{}", job.name);
        }
        assert_eq!(looks_every(TEST.every), TEST.every);
    }

    /// The site's locks granted in the test's database: each job's number (its key's low half) and
    /// the backend (its pid) that holds it, by number.
    async fn site_locks(db: &PgPool) -> Vec<(i64, i32)> {
        sqlx::query_as(
            "SELECT objid::bigint, pid FROM pg_locks
             WHERE locktype = 'advisory' AND granted AND objsubid = 1 AND classid::bigint = $1
               AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
             ORDER BY objid",
        )
        .bind(SITE >> 32)
        .fetch_all(db)
        .await
        .expect("pg_locks")
    }

    /// The jobs as the server starts them (`start_all`, over a state made as pluribus_web::state
    /// makes one, on the fixtures): within a few seconds each periodic job holds its lock, on a
    /// connection of its own, but the euro rate's, which another process holds here (so that no test
    /// reads the bank's page), and which its job leaves to it; the photos' missing copies made, and
    /// their lock let go of; every lock lasting through the jobs' first round (the expiry's takes a
    /// listing whose time is up off the site); and `Jobs::stop` letting go of all of them. D2's own
    /// tests ran `run_locked` alone; a look at the deployed site, which held none of these locks
    /// (D2b: its deploy had gone back to the binary before D2), had nothing to hold it to.
    #[sqlx::test(migrator = "pluribus_db::MIGRATOR", fixtures("../../web/tests/fixtures/listings.sql"))]
    async fn the_jobs_hold_their_locks_as_the_server_starts_them(db: PgPool) {
        // Ana's Golf, one of the fixtures', its time up.
        const GOLF: i64 = 1001;
        let logged = Logged::new(Level::DEBUG, Some("pluribus_services::jobs"));
        let _logging = tracing::subscriber::set_default(logged.clone());
        sqlx::query("UPDATE listings SET expires_at = now() - interval '1 minute' WHERE public_no = $1")
            .bind(GOLF)
            .execute(&db)
            .await
            .unwrap();
        // Another process running the euro rate: its lock, taken by hand.
        let mut other = db.acquire().await.unwrap().detach();
        let taken: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)").bind(RATES.key).fetch_one(&mut other).await.unwrap();
        assert!(taken);
        let other_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut other).await.unwrap();

        let config = Arc::new(Config::for_tests());
        let state = AppState {
            db: db.clone(),
            media: Media::from_config(&config),
            config,
            senders: Arc::new(Senders::none()),
            moderators: Moderators::default(),
            ai: None,
            views: Views::buffered(views::FLUSH_EVERY),
            recommended: Recommended::kept(),
        };
        let jobs = start_all(&state);
        let numbers = |locks: &[(i64, i32)]| locks.iter().map(|(job, _)| *job).collect::<Vec<_>>();
        assert!(until(async || numbers(&site_locks(&db).await) == [1, 2, 3, 4, 5, 6]).await, "{:?}", site_locks(&db).await);
        let held = site_locks(&db).await;
        let mut holders: Vec<i32> = held.iter().filter(|(job, _)| *job != 6).map(|(_, pid)| *pid).collect();
        holders.sort_unstable();
        holders.dedup();
        assert_eq!(holders.len(), 5, "each job on a connection of its own: {held:?}");
        assert!(!holders.contains(&other_pid), "{held:?}");
        assert_eq!(held.last(), Some(&(6, other_pid)), "the euro rate's lock stays the other process's");

        // Each job's first round ran under its lock, and the locks are still held after it.
        let status = async || {
            sqlx::query_scalar::<_, String>("SELECT status FROM listings WHERE public_no = $1").bind(GOLF).fetch_one(&db).await.unwrap()
        };
        assert!(until(async || status().await == "EXPIRED").await, "the expiry's first round");
        assert_eq!(site_locks(&db).await, held, "the locks last through the first round");
        let lines = logged.lines();
        for name in ["refresh", "notices", "search alerts", "expiry", "promotions"] {
            let took = format!("job={name:?}");
            assert!(lines.iter().any(|line| line.contains("this process runs the job") && line.contains(&took)), "{name}: {lines:?}");
        }
        assert!(lines.iter().any(|line| line.contains("another process runs this job") && line.contains("job=\"rates\"")), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains("let go of the job") && line.contains("job=\"photo copies\"")), "{lines:?}");

        jobs.stop().await;
        assert_eq!(site_locks(&db).await, [(6, other_pid)], "the jobs stopped hold nothing");
        let unlocked: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)").bind(RATES.key).fetch_one(&mut other).await.unwrap();
        assert!(unlocked);
        other.close().await.unwrap();
    }
}
