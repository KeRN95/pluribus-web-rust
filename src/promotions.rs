//! Promotions in time (db/promotions.rs): every CHECK_EVERY, those whose time is up end, and a
//! listing at the top of its category for a while (CATEGORY_TOP) goes back to the top of
//! "newest first" once it has been a day since it last went (db::tick_promotions).

use std::time::Duration;

use sqlx::PgPool;

/// How often the server looks.
pub const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);

/// Looks every CHECK_EVERY, from now on, in the background.
pub fn start(db: PgPool) {
    tokio::spawn(async move {
        let mut every = tokio::time::interval(CHECK_EVERY);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            every.tick().await;
            match crate::db::tick_promotions(&db).await {
                Ok((ended, topped)) if ended + topped > 0 => tracing::info!(ended, topped, "promotions ended, listings brought to the top"),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "could not look after the promotions"),
            }
        }
    });
}
