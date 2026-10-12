//! A car's price against what cars of its model and year ask (db/car_prices.rs): the bubble under
//! a car's price on its page (car_price.html), "Çmim i mirë" on the cards of clear bargains, and
//! the listing form's hint under the price as one is typed (/shto/cmimi, price_hint_controller.js).
//! One verdict for all three (domain::listing::car_verdict, which the API gives too): compared
//! with the median of the cars of its model two years either side, the price being in euros on
//! both sides; and what such cars ask, as the pages and the API say it (pluribus-services'
//! listings::CarPrices). Both were this file's until C5.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use pluribus_services::listings::CarPrices;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;

use super::Result;
use crate::{
    AppState, db,
    domain::listing::{CarVerdict, car_verdict},
    i18n::{t, t1},
};

/// A car's page's bubble under the price, and its card.
pub struct CarPrice {
    pub verdict: CarVerdict,
    /// The median, and the middle half's ends ("9 500–11 400 €"), in the listing's currency,
    /// rounded (CarPrices).
    pub median: String,
    pub range: String,
    /// How many cars they come from, and which: "Volkswagen Golf 2014–2018".
    pub listings: i64,
    pub cars: String,
}

impl CarPrice {
    /// From what cars like it ask, in the listing's currency (`eur_rate`, euros a unit of it); None
    /// when there is no verdict, or no rate to show it by.
    pub fn new(car: db::CarPrice, currency: &str, eur_rate: Option<Decimal>) -> Option<Self> {
        let verdict = car_verdict(car.price_eur, car.prices.median_eur)?;
        let shown = CarPrices::new(&car.prices, currency, eur_rate)?;
        Some(Self { verdict, median: shown.median, range: shown.range, listings: car.prices.listings, cars: shown.cars })
    }

    pub fn words(&self) -> &'static str {
        self.verdict.words()
    }

    pub fn tone(&self) -> &'static str {
        self.verdict.tone()
    }
}

#[derive(Deserialize)]
pub struct HintQuery {
    #[serde(default)]
    model: String,
    year: Option<i32>,
    #[serde(default)]
    currency: String,
    /// The price typed so far, as typed ("9 500").
    #[serde(default)]
    price: String,
}

/// GET /shto/cmimi?model=VOLKSWAGEN_GOLF&year=2016&currency=EUR&price=9500: what cars of that
/// model and year ask, in words, and how the price typed compares, as the car's page will say it
/// ({"text", "verdict"}); 204 with too few cars to say.
pub async fn hint(State(state): State<AppState>, Query(query): Query<HintQuery>) -> Result<Response> {
    let currency = if query.currency == "ALL" { "ALL" } else { "EUR" };
    let (Some(year), false) = (query.year, query.model.is_empty()) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let (prices, rate) = tokio::try_join!(db::model_prices(&state.db, &query.model, year), db::eur_rate(&state.db, currency))?;
    let (Some(prices), Some(rate)) = (prices, rate.filter(|rate| *rate > Decimal::ZERO)) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let Some(shown) = CarPrices::new(&prices, currency, Some(rate)) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    // The price typed, its thousands' spaces or dots aside ("9 500", "9.500"): a comma before the
    // last two digits at most is its cents'.
    let digits: String = query.price.chars().filter(|c| c.is_ascii_digit() || *c == ',').collect();
    let price = digits.replace(',', ".").parse::<Decimal>().ok().filter(|price| *price > Decimal::ZERO);
    let said = price.map(|price| match car_verdict(price * rate, prices.median_eur) {
        Some(verdict) => t1("Te faqja e makinës, çmimi yt del „{}“.", verdict.words()),
        None => t("Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?").to_string(),
    });
    Ok(Json(json!({ "text": shown.text(), "verdict": said })).into_response())
}
