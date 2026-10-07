use std::{net::SocketAddr, time::Duration};

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

/// How often the words search corrects typos from are rebuilt (db::refresh_search_words). It takes
/// a couple of seconds for 100 000 listings, and searches go on meanwhile.
const SEARCH_WORDS_REFRESH: Duration = Duration::from_secs(15 * 60);

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

    // Search fixes typos and finds words' other forms from the words listings use, a list rebuilt
    // now and then every SEARCH_WORDS_REFRESH, so that it follows the listings posted meanwhile.
    tokio::spawn({
        let db = db.clone();
        async move {
            let mut refresh = tokio::time::interval(SEARCH_WORDS_REFRESH);
            loop {
                refresh.tick().await;
                if let Err(error) = marketplace::db::refresh_search_words(&db).await {
                    tracing::warn!(%error, "could not refresh the words search corrects typos from");
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
        })
        .await?;
    Ok(())
}
