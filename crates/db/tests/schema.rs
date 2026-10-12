//! Runs tests/schema_checks.sql (constraint and behaviour checks) against a fresh database,
//! migrated by the migrations this crate embeds (MIGRATOR).

use sqlx::PgPool;

#[sqlx::test(migrator = "pluribus_db::MIGRATOR")]
async fn schema_checks_pass(pool: PgPool) -> sqlx::Result<()> {
    // The script is written for psql: drop its psql commands (\echo, \pset), keep the SQL.
    let script = include_str!("schema_checks.sql")
        .lines()
        .filter(|line| !line.starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n");
    sqlx::raw_sql(&script).execute(&pool).await?;
    Ok(())
}
