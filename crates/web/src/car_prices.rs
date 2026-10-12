//! A car's price against what cars of its model and year ask (db/car_prices.rs): the bubble under
//! a car's price on its page (car_price.html), "Çmim i mirë" on the cards of clear bargains, and
//! the listing form's hint under the price as one is typed (/shto/cmimi, price_hint_controller.js).
//! One verdict for all three (verdict): compared with the median of the cars of its model two
//! years either side, the price being in euros on both sides.

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Deserialize;
use serde_json::json;

use super::Result;
use crate::{
    AppState, db, format,
    i18n::{t, t1, t3},
};

/// How a car's price compares with what cars of its model and year ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// 15 % or more under the median: green.
    Good,
    /// Within 15 % of it: yellow.
    Near,
    /// 15 % or more over it: red.
    Above,
}

impl Verdict {
    pub fn words(self) -> &'static str {
        match self {
            Self::Good => t("Çmim i mirë"),
            Self::Near => t("Rreth mesatares"),
            Self::Above => t("Mbi mesataren"),
        }
    }

    /// The bubble's colour, as a property's square metre has them (listing.css).
    pub fn tone(self) -> &'static str {
        match self {
            Self::Good => "price-below",
            Self::Near => "price-near",
            Self::Above => "price-above",
        }
    }
}

/// The verdict on a price against the median, both in euros: none under half of it, where a price
/// says only that it is likely wrong (lekë typed as euros), or a lure.
pub fn verdict(price_eur: Decimal, median_eur: Decimal) -> Option<Verdict> {
    if median_eur <= Decimal::ZERO {
        return None;
    }
    let ratio = price_eur / median_eur;
    match ratio {
        ratio if ratio < Decimal::new(50, 2) => None,
        ratio if ratio <= Decimal::new(85, 2) => Some(Verdict::Good),
        ratio if ratio < Decimal::new(115, 2) => Some(Verdict::Near),
        _ => Some(Verdict::Above),
    }
}

/// A car's page's bubble under the price, and its card.
pub struct CarPrice {
    pub verdict: Verdict,
    /// The median, and the middle half's ends ("9 500–11 400 €"), in the listing's currency,
    /// rounded (rounded).
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
        let verdict = verdict(car.price_eur, car.prices.median_eur)?;
        let shown = Shown::new(&car.prices, currency, eur_rate)?;
        Some(Self { verdict, median: shown.median, range: shown.range, listings: car.prices.listings, cars: shown.cars })
    }

    pub fn words(&self) -> &'static str {
        self.verdict.words()
    }

    pub fn tone(&self) -> &'static str {
        self.verdict.tone()
    }
}

/// What cars of a model and year ask, as the pages say it, in a currency.
struct Shown {
    median: String,
    range: String,
    cars: String,
}

impl Shown {
    fn new(prices: &db::ModelPrices, currency: &str, eur_rate: Option<Decimal>) -> Option<Self> {
        let rate = eur_rate.filter(|rate| *rate > Decimal::ZERO)?;
        let amount = |eur: Decimal| rounded(eur / rate, currency);
        let symbol = format::currency_symbol(currency);
        let model = match &prices.make {
            Some(make) if !prices.model.starts_with(make.as_str()) => format!("{make} {}", prices.model),
            _ => prices.model.clone(),
        };
        Some(Self {
            median: format::money(amount(prices.median_eur), currency),
            range: format!("{}–{}\u{a0}{symbol}", format::number(amount(prices.low_eur)), format::number(amount(prices.high_eur))),
            cars: format!("{model} {}–{}", prices.year - 2, prices.year + 2),
        })
    }
}

/// An amount to show as about what cars ask: to the hundred euros, to the ten thousand lekë.
fn rounded(amount: Decimal, currency: &str) -> Decimal {
    let step = Decimal::from(if currency == "ALL" { 10_000 } else { 100 });
    (amount / step).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero) * step
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
    let Some(shown) = Shown::new(&prices, currency, Some(rate)) else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let text = t3("{} në shitje: shumica kërkojnë {}, mesatarja {}.", shown.cars, shown.range, shown.median);
    // The price typed, its thousands' spaces or dots aside ("9 500", "9.500"): a comma before the
    // last two digits at most is its cents'.
    let digits: String = query.price.chars().filter(|c| c.is_ascii_digit() || *c == ',').collect();
    let price = digits.replace(',', ".").parse::<Decimal>().ok().filter(|price| *price > Decimal::ZERO);
    let said = price.map(|price| match verdict(price * rate, prices.median_eur) {
        Some(verdict) => t1("Te faqja e makinës, çmimi yt del „{}“.", verdict.words()),
        None => t("Çmimi yt është shumë nën mesataren: a është i saktë, dhe në monedhën e duhur?").to_string(),
    });
    Ok(Json(json!({ "text": text, "verdict": said })).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_price_is_good_well_under_the_median_and_says_nothing_far_under_it() {
        let median = Decimal::from(10_000);
        let verdict_at = |price: i64| verdict(Decimal::from(price), median);
        assert_eq!(verdict_at(4_999), None);
        assert_eq!(verdict_at(5_000), Some(Verdict::Good));
        assert_eq!(verdict_at(8_500), Some(Verdict::Good));
        assert_eq!(verdict_at(8_501), Some(Verdict::Near));
        assert_eq!(verdict_at(11_499), Some(Verdict::Near));
        assert_eq!(verdict_at(11_500), Some(Verdict::Above));
        assert_eq!(rounded(Decimal::from(10_250), "EUR"), Decimal::from(10_300));
        assert_eq!(rounded(Decimal::from(1_234_567), "ALL"), Decimal::from(1_230_000));
    }
}
