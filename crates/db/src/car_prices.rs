//! What cars of a model ask (car_model_prices, migration 0041): for each model and year among the
//! cars up for sale at a price, the cars of that model two years either side, how many, and the
//! quartiles of their prices in euros. A car's page compares its price with the median
//! (web/car_prices.rs), the cards mark a clear bargain (CARD_COLUMNS, model_median), and the
//! listing form says what such cars ask as a price is typed. Rebuilt with the other views, at
//! startup and every 15 minutes (main.rs).

use super::*;

/// The cars' category, and the properties a car's model and year are (migration 0002), whose ids
/// are fixed.
const CARS: &str = "10000000-0000-0000-0000-000000000101";
const VEHICLE_MODEL: &str = "20000000-0000-0000-0000-000000000002";
const VEHICLE_YEAR: &str = "20000000-0000-0000-0000-000000000003";

/// What cars of a model and year ask, in euros.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ModelPrices {
    /// The make's name ("Volkswagen"), and the model's ("Golf").
    pub make: Option<String>,
    pub model: String,
    pub year: i32,
    /// How many cars they come from: of that model, two years either side.
    pub listings: i64,
    pub low_eur: Decimal,
    pub median_eur: Decimal,
    pub high_eur: Decimal,
}

/// A car's own price in euros, with what cars like it ask.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CarPrice {
    #[sqlx(flatten)]
    pub prices: ModelPrices,
    pub price_eur: Decimal,
}

/// Rebuilds what cars of each model ask, so that it follows the listings. Pages go on reading the
/// old figures meanwhile.
pub async fn refresh_car_prices(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY car_model_prices").execute(db).await?;
    Ok(())
}

const PRICES: &str = "make.label AS make, o.label AS model, p.year, p.listings, p.low_eur, p.median_eur, p.high_eur";

/// What cars like this listing ask, its model's and year's, with its own price: None for one that
/// isn't a car up for sale at a price (a starting price, one for parts), or whose model and year
/// have too few to say.
pub async fn car_price(db: &PgPool, listing_id: Uuid) -> sqlx::Result<Option<CarPrice>> {
    sqlx::query_as(&format!(
        "SELECT {PRICES}, l.price_normalized AS price_eur
         FROM listings l
         JOIN listing_attribute_values m ON m.listing_id = l.id AND m.attribute_id = '{VEHICLE_MODEL}'
         JOIN listing_attribute_values y ON y.listing_id = l.id AND y.attribute_id = '{VEHICLE_YEAR}'
         JOIN car_model_prices p ON p.model_id = m.value_option_id AND p.year = y.value_number::int
         JOIN attribute_options o ON o.id = m.value_option_id
         LEFT JOIN attribute_options make ON make.id = o.parent_option_id
         WHERE l.id = $1 AND l.category_id = '{CARS}' AND l.listing_type = 'SELL' AND l.price_type IN ('FIXED', 'NEGOTIABLE')
           AND l.price_normalized > 0 AND l.condition IS DISTINCT FROM 'FOR_PARTS'"
    ))
    .bind(listing_id)
    .fetch_optional(db)
    .await
}

/// What cars of this model (by its code in the form, VOLKSWAGEN_GOLF) and year ask: the listing
/// form's hint under the price.
pub async fn model_prices(db: &PgPool, model: &str, year: i32) -> sqlx::Result<Option<ModelPrices>> {
    sqlx::query_as(&format!(
        "SELECT {PRICES}
         FROM attribute_options o
         JOIN car_model_prices p ON p.model_id = o.id AND p.year = $2
         LEFT JOIN attribute_options make ON make.id = o.parent_option_id
         WHERE o.attribute_id = '{VEHICLE_MODEL}' AND o.value_code = $1"
    ))
    .bind(model)
    .bind(year)
    .fetch_optional(db)
    .await
}

/// How many euros one of `currency` is worth (exchange_rates): None for a currency it has none for.
pub async fn eur_rate(db: &PgPool, currency: &str) -> sqlx::Result<Option<Decimal>> {
    sqlx::query_scalar("SELECT eur_rate FROM exchange_rates WHERE currency = $1").bind(currency).fetch_optional(db).await
}
