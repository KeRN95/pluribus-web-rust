//! Database access. Fixed queries use sqlx's `query!` macros, which are checked against the real
//! schema at compile time. Listing pages use `QueryBuilder`, because their filters are optional.

use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::{PgPool, Postgres, QueryBuilder, types::Json};
use time::OffsetDateTime;
use uuid::Uuid;

/// Listings per page. Pages fetch one extra row to find out whether another page follows.
pub const PAGE_SIZE: i64 = 24;

/// Result counts stop here and show as "1 000+": counting every match of a broad search costs
/// more than the exact number is worth.
pub const COUNT_LIMIT: i64 = 1_000;

mod categories;
mod properties;
mod listing_pages;
mod saved;
mod listing;
mod sign_in;
mod posting;
mod typed_in;

pub use categories::*;
pub use properties::*;
pub use listing_pages::*;
pub use saved::*;
pub use listing::*;
pub use sign_in::*;
pub use posting::*;
pub use typed_in::*;

#[cfg(test)]
mod tests {
    use super::*;

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
