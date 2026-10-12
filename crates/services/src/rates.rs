//! The euro rate. Listings are priced in lekë and in euros, and every price is kept in euros too
//! (price_normalized, price_in_eur), which the price filters and orders go by, at the rate in
//! exchange_rates: a round 100 lekë a euro until this took the real one. Every RATE_CHECK_EVERY the
//! server (one at a time however many share the database: jobs.rs, which runs `check`) reads the
//! Bank of Albania's official rate off its page (it publishes one each working day, around noon,
//! with no feed to read it from), takes it when it has moved db::RATE_STEP or
//! more from the one prices are worked out at (db::take_euro_rate), and then works out again the
//! euro prices of the listings priced in lekë, db::REPRICE_BATCH a time with a pause between, so
//! that the pages' own queries go between them; a pass the server stopped in the middle of goes on
//! at its next start.

use std::time::{Duration, Instant};

use pluribus_db as db;
use rust_decimal::Decimal;
use sqlx::PgPool;
use time::{Date, Month};

/// Where the Bank of Albania publishes its official rates, the euro's among them.
pub const RATES_PAGE: &str = "https://www.bankofalbania.org/Markets/Official_exchange_rate/";
/// How often the server reads it.
pub const RATE_CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The pause between two batches of a re-pricing.
const REPRICE_PAUSE: Duration = Duration::from_millis(200);

/// The official euro rate on the bank's page: its day (after "Last update:", "09.10.2026") and
/// lekë a euro, the cell after the EUR row's code in the table after it ("91.65"). None when the
/// page isn't as it was, or the rate isn't one lekë a euro could be (50 to 200).
pub fn parse_euro_rate(page: &str) -> Option<(Date, Decimal)> {
    let rest = &page[page.find("Last update:")?..];
    let start = rest.find("<b>")? + "<b>".len();
    let end = start + rest[start..].find("</b>")?;
    let mut parts = rest[start..end].trim().split('.');
    let day: u8 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let year: i32 = parts.next()?.parse().ok()?;
    let date = Date::from_calendar_date(year, Month::try_from(month).ok()?, day).ok()?;
    // Tags come in either case (<TD>, <td>); lowering ASCII keeps every byte where it was.
    let lower = rest.to_ascii_lowercase();
    let code = lower.find(">eur</td>")? + ">eur</td>".len();
    let cell = code + lower[code..].find("<td")?;
    let value = cell + lower[cell..].find('>')? + 1;
    let value_end = value + lower[value..].find('<')?;
    let rate: Decimal = rest[value..value_end].trim().parse().ok()?;
    (Decimal::from(50) < rate && rate < Decimal::from(200)).then_some((date, rate))
}

/// The bank's page, as text.
async fn fetch_page() -> Result<String, reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("Pluribus/", env!("CARGO_PKG_VERSION")))
        .build()?;
    client.get(RATES_PAGE).send().await?.error_for_status()?.text().await
}

/// Works out again the euro prices of the listings priced in lekë, where that is due (a rate taken
/// since the last pass, or a pass broken off): returns how many it rewrote.
pub async fn reprice(db: &PgPool) -> sqlx::Result<u64> {
    let rate = db::euro_rate(db).await?;
    if !rate.reprice_due {
        return Ok(0);
    }
    let started = Instant::now();
    let (mut after, mut rewritten) = (None, 0);
    loop {
        let (last, count) = db::reprice_batch(db, after).await?;
        rewritten += count;
        match last {
            Some(id) => after = Some(id),
            None => break,
        }
        tokio::time::sleep(REPRICE_PAUSE).await;
    }
    db::repriced(db, rate.eur_rate).await?;
    tracing::info!(lek_per_eur = %rate.lek_per_eur, rewritten, seconds = started.elapsed().as_secs(), "listings priced in lekë worked out in euros again");
    Ok(rewritten)
}

/// Reads the official rate and takes it if it moved far enough, then re-prices if that is due;
/// each failure in the log. What the jobs run every RATE_CHECK_EVERY, as the server starts and
/// from then on, in the process that holds the rate's lock (jobs.rs, `start_all`; `start` here,
/// from `pluribus_web::state`, until D2), so that two servers never re-price at once.
pub async fn check(db: &PgPool) {
    match fetch_page().await {
        Ok(page) => match parse_euro_rate(&page) {
            Some((day, lek_per_eur)) => match db::take_euro_rate(db, lek_per_eur, day).await {
                Ok(true) => tracing::info!(%lek_per_eur, %day, "the Bank of Albania's euro rate taken"),
                Ok(false) => tracing::debug!(%lek_per_eur, %day, "the euro rate is near enough the one prices are worked out at"),
                Err(error) => tracing::warn!(%error, "could not take the euro rate"),
            },
            None => tracing::warn!(page = RATES_PAGE, "the euro rate isn't where it was on the Bank of Albania's page"),
        },
        Err(error) => tracing::warn!(%error, page = RATES_PAGE, "could not read the Bank of Albania's rates"),
    }
    if let Err(error) = reprice(db).await {
        tracing::warn!(%error, "could not work the euro prices out again");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bank's page as it was on 10 October 2026, its euro table cut down to two rows.
    const PAGE: &str = r#"<script>var x = { "alert": "Please, enter Date in DD.MM.YYYY format!" }</script>
<div class="mb-2"><span class="font-weight-normal">Last update:</span>&nbsp;<span class="text-primary font-size-table"><b>09.10.2026</b>&nbsp;&nbsp;<em><b>12:08:19</b></em></span></div>
<TABLE class="table table-sm" cellSpacing="0" cellPadding="3" width="100%" border="1">
<thead class="thead-light"><TR><th colspan="3">Main Currency</th><th class="text-right" colspan="2">Albanian Lek per  Foreign Currency Unit</th ></TR></thead>
<TR>
    <TD nowrap>US Dollar</TD>
    <TD nowrap>USD</TD>
    <td align="right" nowrap>81.70</td>
    <td align="right" nowrap>-0.24</td>
</TR>
<TR>
    <TD nowrap>Euro</TD>
    <TD nowrap>EUR</TD>
    <td align="right" nowrap>91.65</td>
    <td align="right" nowrap>-0.03</td>
</TR>
</TABLE>
<select><option value='1,2'>EUR</option></select>"#;

    #[test]
    fn reads_the_euro_rate_off_the_banks_page() {
        let (day, rate) = parse_euro_rate(PAGE).unwrap();
        assert_eq!(day, Date::from_calendar_date(2026, Month::October, 9).unwrap());
        assert_eq!(rate, "91.65".parse::<Decimal>().unwrap());
        // A page changed past reading, or a rate no euro could cost, gives none.
        assert_eq!(parse_euro_rate(&PAGE.replace("Last update:", "Përditësuar:")), None);
        assert_eq!(parse_euro_rate(&PAGE.replace(">EUR</TD>", ">EURO</TD>")), None);
        assert_eq!(parse_euro_rate(&PAGE.replace("91.65", "9165")), None);
        assert_eq!(parse_euro_rate(&PAGE.replace("09.10.2026", "31.02.2026")), None);
    }
}
