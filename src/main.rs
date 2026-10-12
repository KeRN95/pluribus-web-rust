use std::net::SocketAddr;

use anyhow::Context;
use pluribus_services::{config::Config, jobs};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            // `pluribus` is every crate of the site's (pluribus_web, pluribus_db...), a log's
            // target being the module a line comes from, matched by the start of its name.
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("marketplace=info,pluribus=info,tower_http=info")),
        )
        .init();

    // Every setting, read once, now: the environment, with .env's added above (the log's RUST_LOG
    // aside, which the log has read already). Nothing else reads the environment; the state keeps
    // them (pluribus_web::state).
    let config = Config::from_env()?;
    let options: PgConnectOptions = config.database_url.parse().context("DATABASE_URL is not a PostgreSQL address")?;
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect_with(pluribus_db::plan_for_values(options))
        .await
        .context("could not connect to PostgreSQL")?;

    pluribus_db::MIGRATOR.run(&db).await.context("could not run migrations")?;

    let addr = config.bind_addr;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");

    // The pages and, beside them under /api/v1, the JSON API, over one state, which keeps the
    // settings (pluribus_web::state). Nested after the pages' layers are on, the API has none of
    // them, only its own (pluribus_api::router).
    let state = pluribus_web::state(db, config);
    // The work that runs on its own, started once, here, over the state: the figures the pages
    // read rebuilt and the old sign-in codes forgotten (this file's own loop until D2), the
    // notices, the alerts, the expiry, the promotions, the euro rate and the photos' missing
    // copies, each under a lock of its own in the database, so that a second server, or a binary
    // run beside this one, runs none of them twice; and the views this process gathers, its own
    // (pluribus-services' jobs.rs).
    let jobs = jobs::start_all(&state);
    let app = pluribus_web::router(state.clone()).nest(pluribus_api::PREFIX, pluribus_api::router(state));
    // The connection's address is the visitor's IP for the sign-in limits (web/account.rs).
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;
    // Stopped cleanly, once the requests under way are answered: the jobs let go of their locks,
    // so that another server takes them over at its next look rather than once the database sees
    // this one's connections gone.
    jobs.stop().await;
    Ok(())
}
