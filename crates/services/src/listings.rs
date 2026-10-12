//! What a listing's page works out from the database's rows, which the pages (pluribus-web's
//! listings.rs and car_prices.rs) and the JSON API (pluribus-api's listing.rs) say alike, moved out
//! of the pages in step C5: whether someone looks after a listing as its seller (`owns`), its
//! properties in the language now (`shown`), what cars of a car's model and year ask, in the
//! listing's currency and in words (`CarPrices`), and whom a view of its page counts as
//! (`visitor`). The rules and the maths beside them, which need no row, are pluribus-core's
//! (domain::listing, domain::Status).

use pluribus_core::{
    format,
    i18n::{t, t3, t_value},
};
use pluribus_db::{self as db, AttributeValue, ListingDetail, ModelPrices};
use rust_decimal::{Decimal, RoundingStrategy};
use sqlx::PgPool;
use uuid::Uuid;

/// Whether `user` looks after the listing as its seller, its page offering them its buttons: a
/// private seller's own, or, for a store's, one of the store's people who post for it
/// (db::posts_for, as manages_listing has it in the database).
pub async fn owns(db: &PgPool, user: Uuid, listing: &ListingDetail) -> sqlx::Result<bool> {
    match listing.seller_business_id {
        None => Ok(user == listing.created_by_user_id),
        Some(store) => db::posts_for(db, store, user).await,
    }
}

/// A listing's property as its page shows it, in the language now, a multi-select's each value: a
/// choice's label, a text someone typed and the name of a box ticked as the database words them
/// (i18n::t_value, which says them in English), a number with its unit (format::attribute_number:
/// "148 000 km"), a box left unticked "Jo", a date (format::day). None for a property with no
/// value. The pages' (AttributeValueExt::shown) and the API's (`shown`).
pub fn shown(attribute: &AttributeValue) -> Vec<String> {
    if !attribute.labels.is_empty() {
        return attribute.labels.iter().map(|label| t_value(label).into_owned()).collect();
    }
    let shown = match (&attribute.value_text, attribute.value_number, attribute.value_bool, attribute.value_date) {
        (Some(text), ..) => t_value(text).into_owned(),
        (None, Some(number), ..) => format::attribute_number(number, attribute.unit.as_deref()),
        (None, None, Some(true), _) => t_value(&attribute.label).into_owned(),
        (None, None, Some(false), _) => t("Jo").to_string(),
        (None, None, None, Some(date)) => format::day(date),
        (None, None, None, None) => return Vec::new(),
    };
    vec![shown]
}

/// What cars of a model and year ask (db's car_model_prices, in euros), as the pages say it in a
/// listing's currency: the median and the middle half's ends, rounded to the hundred euros or the
/// ten thousand lekë, and which cars they are. A car's page shows them in the card its price's
/// bubble opens (pluribus-web's car_prices.rs, car_price.html), the listing form's hint and the API
/// say them in a sentence (`text`).
pub struct CarPrices {
    /// The median, rounded: 10 300.
    pub median_amount: Decimal,
    /// The same with its currency, as the pages write it: "10 300 €".
    pub median: String,
    /// The middle half's ends: "9 500–11 400 €".
    pub range: String,
    /// Which cars they are, their make, their model and the years either side: "Volkswagen Golf
    /// 2014–2018".
    pub cars: String,
}

impl CarPrices {
    /// In `currency`, at `eur_rate` (euros a unit of it): None without a rate to say them by.
    pub fn new(prices: &ModelPrices, currency: &str, eur_rate: Option<Decimal>) -> Option<Self> {
        let rate = eur_rate.filter(|rate| *rate > Decimal::ZERO)?;
        let amount = |eur: Decimal| rounded(eur / rate, currency);
        let symbol = format::currency_symbol(currency);
        let model = match &prices.make {
            Some(make) if !prices.model.starts_with(make.as_str()) => format!("{make} {}", prices.model),
            _ => prices.model.clone(),
        };
        let median_amount = amount(prices.median_eur);
        Some(Self {
            median_amount,
            median: format::money(median_amount, currency),
            range: format!("{}–{}\u{a0}{symbol}", format::number(amount(prices.low_eur)), format::number(amount(prices.high_eur))),
            cars: format!("{model} {}–{}", prices.year - 2, prices.year + 2),
        })
    }

    /// What such cars ask, in a sentence, in the language now: "Volkswagen Golf 2014–2018 në shitje:
    /// shumica kërkojnë 9 500–11 400 €, mesatarja 10 300 €."
    pub fn text(&self) -> String {
        t3("{} në shitje: shumica kërkojnë {}, mesatarja {}.", &self.cars, &self.range, &self.median)
    }
}

/// An amount to show as about what cars ask: to the hundred euros, to the ten thousand lekë.
fn rounded(amount: Decimal, currency: &str) -> Decimal {
    let step = Decimal::from(if currency == "ALL" { 10_000 } else { 100 });
    (amount / step).round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero) * step
}

/// Whom a view of a listing's page counts as (views.rs, which counts each visitor once in a
/// while): their address (`ip`) and their browser (`agent`), a line apart; none for a client that
/// says nothing of itself, or says it is a bot, a spider, a crawler or Yahoo's Slurp, whose views
/// don't count. The pages' (`POST /listing/{number}/pare`) and the API's
/// (`POST /api/v1/listings/{no}/seen`).
pub fn visitor(ip: Option<&str>, agent: &str) -> Option<String> {
    let lower = agent.to_ascii_lowercase();
    if agent.is_empty() || ["bot", "spider", "crawl", "slurp"].iter().any(|word| lower.contains(word)) {
        return None;
    }
    Some(format!("{}\n{agent}", ip.unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use pluribus_core::i18n::{LANG, Lang};
    use time::macros::date;

    use super::*;

    fn attribute(label: &str) -> AttributeValue {
        AttributeValue {
            code: "code".to_string(),
            label: label.to_string(),
            value_number: None,
            unit: None,
            value_bool: None,
            value_date: None,
            value_text: None,
            labels: Vec::new(),
            labels_en: Vec::new(),
            multi: false,
            key: false,
        }
    }

    #[test]
    fn a_property_is_shown_in_the_language_now() {
        let mileage = AttributeValue { value_number: Some(Decimal::from(148_000)), unit: Some("km".to_string()), ..attribute("Kilometrazhi") };
        let features = AttributeValue { labels: vec!["Kondicioner".to_string(), "ABS".to_string()], multi: true, ..attribute("Opsionet") };
        let warranty = AttributeValue { value_bool: Some(true), ..attribute("Me garanci") };
        let unticked = AttributeValue { value_bool: Some(false), ..attribute("Me garanci") };
        let since = AttributeValue { value_date: Some(date!(2026 - 10 - 04)), ..attribute("Data") };
        let typed = AttributeValue { value_text: Some("Golf 7, 1.6 TDI".to_string()), ..attribute("Versioni") };
        assert_eq!(shown(&mileage), ["148\u{a0}000\u{a0}km"]);
        assert_eq!(shown(&features), ["Kondicioner", "ABS"]);
        assert_eq!(shown(&warranty), ["Me garanci"]);
        assert_eq!(shown(&unticked), ["Jo"]);
        assert_eq!(shown(&since), ["04.10.2026"]);
        assert_eq!(shown(&typed), ["Golf 7, 1.6 TDI"]);
        assert_eq!(shown(&attribute("Bosh")), Vec::<String>::new());
        LANG.sync_scope(Lang::En, || {
            assert_eq!(shown(&features), ["Air conditioning", "ABS"]);
            assert_eq!(shown(&unticked), ["No"]);
            assert_eq!(shown(&since), ["4 Oct 2026"]);
            assert_eq!(shown(&typed), ["Golf 7, 1.6 TDI"], "as typed");
        });
    }

    #[test]
    fn what_cars_ask_is_said_rounded_in_the_listings_currency() {
        let golfs = ModelPrices {
            make: Some("Volkswagen".to_string()),
            model: "Golf".to_string(),
            year: 2016,
            listings: 10,
            low_eur: Decimal::from(9_500),
            median_eur: Decimal::from(10_250),
            high_eur: Decimal::from(11_375),
        };
        let euros = CarPrices::new(&golfs, "EUR", Some(Decimal::ONE)).unwrap();
        assert_eq!(euros.median_amount, Decimal::from(10_300));
        assert_eq!((euros.median.as_str(), euros.range.as_str()), ("10\u{a0}300\u{a0}€", "9\u{a0}500–11\u{a0}400\u{a0}€"));
        assert_eq!(euros.text(), "Volkswagen Golf 2014–2018 në shitje: shumica kërkojnë 9\u{a0}500–11\u{a0}400\u{a0}€, mesatarja 10\u{a0}300\u{a0}€.");
        // In lekë, to the ten thousand (a lek a hundredth of a euro: 1 025 000 lekë).
        let lek = CarPrices::new(&golfs, "ALL", Some(Decimal::new(1, 2))).unwrap();
        assert_eq!(lek.median_amount, Decimal::from(1_030_000));
        assert!(CarPrices::new(&golfs, "EUR", None).is_none(), "no rate to say them by");
        assert_eq!(rounded(Decimal::from(1_234_567), "ALL"), Decimal::from(1_230_000));
        // A model named with its make says it once.
        let named = ModelPrices { model: "Volkswagen Polo".to_string(), ..golfs };
        assert_eq!(CarPrices::new(&named, "EUR", Some(Decimal::ONE)).unwrap().cars, "Volkswagen Polo 2014–2018");
    }

    #[test]
    fn a_view_counts_as_its_visitors_address_and_browser_but_a_bots() {
        assert_eq!(visitor(Some("10.0.0.1"), "Mozilla/5.0").as_deref(), Some("10.0.0.1\nMozilla/5.0"));
        assert_eq!(visitor(None, "Pluribus/1.0 (Android)").as_deref(), Some("\nPluribus/1.0 (Android)"));
        for agent in ["", "Googlebot/2.1", "Baiduspider", "SomeCrawler", "Yahoo! Slurp"] {
            assert_eq!(visitor(Some("10.0.0.1"), agent), None, "{agent}");
        }
    }
}
