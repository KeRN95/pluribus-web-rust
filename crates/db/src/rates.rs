//! The euro rate prices are worked out at (exchange_rates, price_in_eur): taking the Bank of
//! Albania's when it has moved far enough (rates.rs), and working the euro prices of the listings
//! priced in lekë out again at it (price_normalized), which every price filter and order goes by.

use time::Date;

use super::*;

/// How far the official rate must move from the one prices are worked out at before it is taken:
/// a share of it, half a percent. Working the prices out again rewrites every listing priced in
/// lekë (half the listings, minutes of the database's time), so a rate that wobbles a few
/// hundredths a day isn't worth it; a price off by under half a percent sorts and filters as it
/// would anyway.
pub const RATE_STEP: Decimal = Decimal::from_parts(5, 0, 0, false, 3);

/// How many listings a re-pricing goes through a time, each batch its own short transaction.
pub const REPRICE_BATCH: i64 = 2_000;

/// The rate prices are worked out at: euros a lek (exchange_rates.eur_rate), lekë a euro, the
/// official rate's day, and whether the listings' euro prices still have to be worked out again at
/// it.
#[derive(Debug, sqlx::FromRow)]
pub struct EuroRate {
    pub eur_rate: Decimal,
    pub lek_per_eur: Decimal,
    pub rate_date: Option<Date>,
    pub reprice_due: bool,
}

pub async fn euro_rate(db: &PgPool) -> sqlx::Result<EuroRate> {
    sqlx::query_as(
        "SELECT eur_rate, round(1 / eur_rate, 2) AS lek_per_eur, rate_date, normalized_at IS NULL AS reprice_due
         FROM exchange_rates WHERE currency = 'ALL'",
    )
    .fetch_one(db)
    .await
}

/// Takes the official rate (`lek_per_eur` lekë a euro, of `day`) when it is RATE_STEP or more from
/// the one prices are worked out at: the listings' euro prices are then due to be worked out again
/// (reprice_batch). Returns whether it was taken.
pub async fn take_euro_rate(db: &PgPool, lek_per_eur: Decimal, day: Date) -> sqlx::Result<bool> {
    let taken = sqlx::query(
        "UPDATE exchange_rates
         SET eur_rate = round(1 / $1, 8), rate_date = $2, updated_at = now(), normalized_at = NULL
         WHERE currency = 'ALL' AND abs(eur_rate * $1 - 1) >= $3",
    )
    .bind(lek_per_eur)
    .bind(day)
    .bind(RATE_STEP)
    .execute(db)
    .await?
    .rows_affected();
    Ok(taken == 1)
}

/// Works out again the euro prices of the next REPRICE_BATCH listings priced in lekë after `after`
/// (by id), rewriting those whose price_normalized isn't what the rate makes it, and leaving their
/// updated_at as it was (migration 0032). Only those with an amount: a free one without one keeps
/// its 0, which puts it first among the cheapest. Returns the last id it went through, None once there
/// were no more, and how many it rewrote.
pub async fn reprice_batch(db: &PgPool, after: Option<Uuid>) -> sqlx::Result<(Option<Uuid>, u64)> {
    let mut tx = db.begin().await?;
    sqlx::query("SELECT set_config('app.keep_updated_at', 'on', true)").execute(&mut *tx).await?;
    let (last, rewritten): (Option<Uuid>, i64) = sqlx::query_as(
        "WITH batch AS (
             SELECT id FROM listings
             WHERE currency = 'ALL' AND price_amount IS NOT NULL AND ($1::uuid IS NULL OR id > $1)
             ORDER BY id
             LIMIT $2
         ),
         rewritten AS (
             UPDATE listings l SET price_normalized = price_in_eur(l.price_amount, 'ALL')
             FROM batch b
             WHERE l.id = b.id AND l.price_normalized IS DISTINCT FROM price_in_eur(l.price_amount, 'ALL')
             RETURNING 1
         )
         SELECT (SELECT id FROM batch ORDER BY id DESC LIMIT 1), (SELECT count(*) FROM rewritten)",
    )
    .bind(after)
    .bind(REPRICE_BATCH)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((last, rewritten as u64))
}

/// A re-pricing went through every listing priced in lekë at `eur_rate`: it is done, unless the
/// rate changed meanwhile, whose own pass is then still due.
pub async fn repriced(db: &PgPool, eur_rate: Decimal) -> sqlx::Result<()> {
    sqlx::query("UPDATE exchange_rates SET normalized_at = now() WHERE currency = 'ALL' AND normalized_at IS NULL AND eur_rate = $1")
        .bind(eur_rate)
        .execute(db)
        .await?;
    Ok(())
}
