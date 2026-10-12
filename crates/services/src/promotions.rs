//! Promotions in time (db/promotions.rs): every CHECK_EVERY, those whose time is up end, and a
//! listing at the top of its category for a while (CATEGORY_TOP) goes back to the top of
//! "newest first" once it has been a day since it last went (db::tick_promotions); one server at a
//! time however many share the database (jobs.rs, which runs `tick`).

use std::time::Duration;

use sqlx::PgPool;

/// How often the server looks.
pub const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);

/// One look, what it did or a failure in the log: what the jobs run every CHECK_EVERY, as the
/// server starts and from then on, in the process that holds the promotions' lock (jobs.rs,
/// `start_all`; `start` here, from `pluribus_web::state`, until D2).
pub async fn tick(db: &PgPool) {
    match pluribus_db::tick_promotions(db).await {
        Ok((ended, topped)) if ended + topped > 0 => tracing::info!(ended, topped, "promotions ended, listings brought to the top"),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "could not look after the promotions"),
    }
}
