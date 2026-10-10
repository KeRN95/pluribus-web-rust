//! Database access. Fixed queries use sqlx's `query!` macros, which are checked against the real
//! schema at compile time. Listing pages use `QueryBuilder`, because their filters are optional.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, Postgres, QueryBuilder, postgres::PgConnectOptions, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

/// The site's connections, each planning every query for the values it runs with
/// (plan_cache_mode = force_custom_plan). Once a connection has run a prepared query five times,
/// Postgres may plan it once for any values (a generic plan), and a filter's values match a
/// handful of listings or tens of thousands: a plan made for none of them took the cars filtered
/// to BMW from 16 ms to 90 from their sixth time on a connection. Planning every time costs a
/// fraction of a millisecond a query.
pub fn plan_for_values(options: PgConnectOptions) -> PgConnectOptions {
    options.options([("plan_cache_mode", "force_custom_plan")])
}

/// Listings per page. Pages fetch one extra row to find out whether another page follows.
pub const PAGE_SIZE: i64 = 24;

/// Result counts stop here and show as "1 000+": counting every match of a broad search costs
/// more than the exact number is worth.
pub const COUNT_LIMIT: i64 = 1_000;

/// A browse page asks again, once it shows, for a count that stopped at COUNT_LIMIT, this time to
/// here (browse.rs, numri=1): so that the count still moves as filters narrow a big category,
/// without the page waiting for it. Counting a town's cars to here takes some 70 ms, where to
/// COUNT_LIMIT it takes 10.
pub const PRECISE_COUNT_LIMIT: i64 = 10_000;

/// SQL for the price a listing read as `l` dropped from, as a scalar subquery: the lowest it was
/// over the 30 days before its price last changed, when that change, in the last 30 days, made it
/// at least 1% lower in the same currency; NULL otherwise, or when the listing is no longer up or
/// isn't on sale, for rent or a service. From listing_price_history, which a trigger fills as
/// prices change (migration 0001). The lowest of the 30 days rather than the last, so a price
/// raised for a while and lowered back is no drop. Cards (CARD_COLUMNS), a listing's page and the
/// notifications to those who keep it (price_drops.rs) all read it from here.
macro_rules! price_was {
    () => {
        "(SELECT min(h.price_amount)
          FROM (SELECT p.changed_at
                FROM listing_price_history p
                WHERE p.listing_id = l.id
                ORDER BY p.changed_at DESC
                LIMIT 1) cur
          JOIN listing_price_history h
            ON h.listing_id = l.id AND h.changed_at < cur.changed_at
           AND h.changed_at >= coalesce((SELECT max(b.changed_at)
                                         FROM listing_price_history b
                                         WHERE b.listing_id = l.id AND b.changed_at <= cur.changed_at - interval '30 days'),
                                        '-infinity')
          WHERE cur.changed_at > now() - interval '30 days'
            AND l.status IN ('ACTIVE', 'RESERVED') AND l.listing_type IN ('SELL', 'RENT', 'SERVICE')
            AND l.price_type IN ('FIXED', 'NEGOTIABLE', 'FROM')
            AND h.currency = l.currency AND h.price_type IN ('FIXED', 'NEGOTIABLE', 'FROM') AND h.price_amount IS NOT NULL
          HAVING l.price_amount <= min(h.price_amount) * 0.99)"
    };
}

mod categories;
mod properties;
mod listing_pages;
mod saved;
mod listing;
mod messages;
mod sign_in;
mod sitemap;
mod posting;
mod price_drops;
mod safety;
mod typed_in;
mod notices;
mod searches;
mod expiry;
mod notifications;
mod rates;

pub use categories::*;
pub use properties::*;
pub use listing_pages::*;
pub use saved::*;
pub use listing::*;
pub use messages::*;
pub use sign_in::*;
pub use sitemap::*;
pub use posting::*;
pub use price_drops::*;
pub use safety::*;
pub use typed_in::*;
pub use notices::*;
pub use searches::*;
pub use expiry::*;
pub use notifications::*;
pub use rates::*;

#[cfg(test)]
mod tests {
    use sqlx::ConnectOptions;

    use super::*;

    /// A connection opened as the site opens its own plans every query for its values.
    #[sqlx::test]
    async fn the_sites_connections_plan_queries_for_their_values(pool: PgPool) -> sqlx::Result<()> {
        let mut connection = plan_for_values((*pool.connect_options()).clone()).connect().await?;
        let mode: String = sqlx::query_scalar("SHOW plan_cache_mode").fetch_one(&mut connection).await?;
        assert_eq!(mode, "force_custom_plan");
        Ok(())
    }

    #[test]
    fn cursors_survive_a_round_trip() {
        let id = Uuid::from_u128(0x0199_a1b2_c3d4_7e5f_8a6b_7c8d_9e0f_1a2b);
        let newest = Cursor::Newest {
            sort_at: OffsetDateTime::from_unix_timestamp_nanos(1_790_000_000_123_456_000).unwrap(),
            id,
        };
        assert_eq!(Cursor::decode(Sort::Newest, &newest.encode()), Some(newest));

        let priced = Cursor::Price { price: Some(Decimal::new(1_250_050, 2)), id };
        assert_eq!(priced.encode(), format!("12500.50_{id}"));
        assert_eq!(Cursor::decode(Sort::PriceLow, &priced.encode()), Some(priced));

        let unpriced = Cursor::Price { price: None, id };
        assert_eq!(Cursor::decode(Sort::PriceHigh, &unpriced.encode()), Some(unpriced));

        assert_eq!(Cursor::decode(Sort::Newest, "garbage"), None);
        assert_eq!(Cursor::decode(Sort::PriceLow, &format!("abc_{id}")), None);

        // Distances keep every digit, so the next page starts exactly after the last listing.
        let near = Cursor::Distance { meters: 2_345.678_901_234_567, id };
        assert_eq!(Cursor::decode(Sort::Nearest, &near.encode()), Some(near));
        assert_eq!(Cursor::decode(Sort::Nearest, &format!("inf_{id}")), None);
    }
}
