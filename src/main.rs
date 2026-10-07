use std::{net::SocketAddr, time::Duration};

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

/// How often the words search corrects typos from (db::refresh_search_words) and what a square
/// metre costs in each zone (db::refresh_zone_prices) are rebuilt. The words take a couple of
/// seconds for 100 000 listings, and searches go on meanwhile.
const REFRESH_EVERY: Duration = Duration::from_secs(15 * 60);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("marketplace=info,tower_http=info")),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("could not connect to PostgreSQL")?;

    sqlx::migrate!().run(&db).await.context("could not run migrations")?;

    // Search fixes typos and finds words' other forms from the words listings use, and a
    // property's page compares its price per square metre with its zone's: figures over all the
    // listings, rebuilt now and then every REFRESH_EVERY, so that they follow the listings posted
    // meanwhile.
    tokio::spawn({
        let db = db.clone();
        async move {
            let mut refresh = tokio::time::interval(REFRESH_EVERY);
            loop {
                refresh.tick().await;
                if let Err(error) = marketplace::db::refresh_search_words(&db).await {
                    tracing::warn!(%error, "could not refresh the words search corrects typos from");
                }
                if let Err(error) = marketplace::db::refresh_zone_prices(&db).await {
                    tracing::warn!(%error, "could not refresh what a square metre costs in each zone");
                }
            }
        }
    });

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()
        .context("BIND_ADDR is not a valid address")?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on http://{addr}");

    // The connection's address is the visitor's IP for the sign-in limits (web/account.rs).
    axum::serve(listener, marketplace::app(db).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
            // The welcome page's row of the newest listings keeps a stream open for as long as
            // the page is open (web/newest.rs): end them, or the server would wait for every one.
            marketplace::web::close_streams();
        })
        .await?;
    Ok(())
}
