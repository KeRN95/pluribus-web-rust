//! Display formatting with Albanian conventions: "9 500 €", "60 000 Lekë", "04.10.2026".

use rust_decimal::{Decimal, prelude::ToPrimitive};
use time::{Date, Duration, Month, OffsetDateTime, UtcOffset, macros::format_description};

/// Keeps a number and its unit on one line.
const NBSP: char = '\u{a0}';

/// A listing's price. Rents and salaries are per month.
pub fn price(amount: Option<Decimal>, currency: &str, price_type: &str, listing_type: &str) -> String {
    let (price, note) = price_parts(amount, currency, price_type, listing_type);
    price + &note
}

/// A price in two parts: the price itself, and what follows it, which the listing page sets
/// smaller: the month of a rent or a salary, and whether it can be discussed ("/muaj (i
/// diskutueshëm)"); empty when nothing does.
pub fn price_parts(amount: Option<Decimal>, currency: &str, price_type: &str, listing_type: &str) -> (String, String) {
    match (price_type, amount) {
        ("FREE", _) => ("Falas".to_string(), String::new()),
        _ if listing_type == "JOB" && (price_type == "ON_REQUEST" || amount.is_none()) => {
            ("Paga me marrëveshje".to_string(), String::new())
        }
        ("ON_REQUEST", _) | (_, None) => ("Çmimi me marrëveshje".to_string(), String::new()),
        (kind, Some(amount)) => {
            let period = match listing_type {
                "RENT" | "WANTED_RENT" | "JOB" => "/muaj",
                _ => "",
            };
            let price = match kind {
                "FROM" => format!("nga {}", money(amount, currency)),
                _ => money(amount, currency),
            };
            let note = match kind {
                "NEGOTIABLE" => format!("{period} (i diskutueshëm)"),
                _ => period.to_string(),
            };
            (price, note)
        }
    }
}

/// An amount with its currency: "9 500 €".
pub fn money(amount: Decimal, currency: &str) -> String {
    format!("{}{NBSP}{}", number(amount), currency_symbol(currency))
}

pub fn currency_symbol(code: &str) -> &str {
    match code {
        "EUR" => "€",
        "ALL" => "Lekë",
        other => other,
    }
}

/// Thousands separated by no-break spaces, decimal comma, decimals only when there are any.
pub fn number(amount: Decimal) -> String {
    let cents = (amount.round_dp(2) * Decimal::ONE_HUNDRED)
        .to_i64()
        .unwrap_or(0);
    let (whole, fraction) = (cents.abs() / 100, cents.abs() % 100);
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(NBSP);
        }
        grouped.push(digit);
    }
    let sign = if cents < 0 { "-" } else { "" };
    if fraction == 0 {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{fraction:02}")
    }
}

/// "1 njoftim", "24 njoftime"; counts that reached `limit` show as "1 000+ njoftime".
/// A count, its thousands separated by no-break spaces: "1 234". Templates pass it by reference.
pub fn count(count: impl std::borrow::Borrow<i64>) -> String {
    number(Decimal::from(*count.borrow()))
}

pub fn listing_count(count: i64, limit: i64) -> String {
    match count {
        1 => "1 njoftim".to_string(),
        n if n > limit => format!("{}+ njoftime", number(Decimal::from(limit))),
        n => format!("{} njoftime", number(Decimal::from(n))),
    }
}

/// How many a browse page found, its thousands as "k", whole ones: "1 njoftim", "850 njoftime",
/// "3k njoftime" (3 000 to 3 999), "1k+ njoftime" past the limit counted to (1 000).
pub fn short_listing_count(count: i64, limit: i64) -> String {
    let (n, more) = if count > limit { (limit, "+") } else { (count, "") };
    match n {
        1 if more.is_empty() => "1 njoftim".to_string(),
        n if n >= 1000 => format!("{}k{more} njoftime", n / 1000),
        n => format!("{n}{more} njoftime"),
    }
}

/// Words in Albanian quotes, listed: "„golf“", "„golf“ dhe „mercedes“", "„a“, „b“ dhe „c“".
pub fn quoted_list(words: &[String]) -> String {
    let quoted: Vec<String> = words.iter().map(|word| format!("„{word}“")).collect();
    match quoted.split_last() {
        None => String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} dhe {last}", rest.join(", ")),
    }
}

/// A distance from the map pin: "850 m", "2,4 km", "18 km".
pub fn distance(meters: f64) -> String {
    if meters < 950.0 {
        format!("{}{NBSP}m", ((meters / 10.0).round() * 10.0) as i64)
    } else if meters < 9_950.0 {
        format!("{:.1}{NBSP}km", meters / 1000.0).replace('.', ",")
    } else {
        format!("{}{NBSP}km", (meters / 1000.0).round() as i64)
    }
}

/// How a listing type reads in filters.
pub fn listing_type(code: &str) -> &str {
    match code {
        "SELL" => "Në shitje",
        "BUY" => "Kërkoj të blej",
        "RENT" => "Me qira",
        "WANTED_RENT" => "Kërkoj me qira",
        "SERVICE" => "Shërbim",
        "JOB" => "Ofertë pune",
        other => other,
    }
}

/// What a sold listing (SOLD) says it is, by its kind: a flat let out is "Dhënë me qira", anything
/// else "Shitur".
pub fn sold(listing_type: &str) -> &'static str {
    match listing_type {
        "RENT" => "Dhënë me qira",
        _ => "Shitur",
    }
}

/// The seller's button that says a listing is sold, by its kind: "U shit", or "U dha me qira".
pub fn sell_action(listing_type: &str) -> &'static str {
    match listing_type {
        "RENT" => "U dha me qira",
        _ => "U shit",
    }
}

pub fn condition(code: &str) -> &str {
    match code {
        "NEW" => "E re",
        "LIKE_NEW" => "Si e re",
        "USED" => "E përdorur",
        "FOR_PARTS" => "Për pjesë",
        other => other,
    }
}

/// A date by Albania's clock: "04.10.2026".
pub fn date(value: OffsetDateTime) -> String {
    tirana(value)
        .format(format_description!("[day].[month].[year]"))
        .unwrap_or_default()
}

/// When a listing expires, as its seller's buttons say it within a week, by Albania's clock:
/// "Skadon sot", "Skadon nesër", "Skadon pas 3 ditësh"; None from a week on, or once past.
pub fn expires_soon(expires: OffsetDateTime, now: OffsetDateTime) -> Option<String> {
    if expires <= now {
        return None;
    }
    match (tirana(expires).date() - tirana(now).date()).whole_days() {
        0 => Some("Skadon sot".to_string()),
        1 => Some("Skadon nesër".to_string()),
        days @ 2..=6 => Some(format!("Skadon pas {days} ditësh")),
        _ => None,
    }
}

/// When something happened, as a line says it, by Albania's clock: "Sot", "Dje", "para 3 ditësh"
/// within the week, and the date from a week on.
pub fn when(value: OffsetDateTime, now: OffsetDateTime) -> String {
    match (tirana(now).date() - tirana(value).date()).whole_days() {
        ..=0 => "Sot".to_string(),
        1 => "Dje".to_string(),
        days @ 2..=6 => format!("para {days} ditësh"),
        _ => date(value),
    }
}

/// A moment by Albania's clock: Central European Time, an hour ahead of UTC, and two from the
/// last Sunday of March at 01:00 UTC to the last Sunday of October at 01:00 UTC, the summer
/// time Europe keeps.
fn tirana(value: OffsetDateTime) -> OffsetDateTime {
    let utc = value.to_offset(UtcOffset::UTC);
    let last_sunday_at_one = |month: Month| {
        let last = Date::from_calendar_date(utc.year(), month, 31).expect("a month of 31 days");
        let sunday = last - Duration::days(i64::from(last.weekday().number_days_from_sunday()));
        sunday.with_hms(1, 0, 0).expect("a time of day").assume_utc()
    };
    let summer = (last_sunday_at_one(Month::March)..last_sunday_at_one(Month::October)).contains(&utc);
    value.to_offset(UtcOffset::from_hms(if summer { 2 } else { 1 }, 0, 0).expect("an offset"))
}

/// Albanian mobile numbers as +355 69 123 4567; anything else is shown unchanged.
pub fn phone(e164: &str) -> String {
    match e164.strip_prefix("+355") {
        Some(rest) if rest.len() == 9 && rest.starts_with('6') => {
            format!("+355 {} {} {}", &rest[..2], &rest[2..5], &rest[5..])
        }
        _ => e164.to_string(),
    }
}

/// A month and year, for "on the site since": "tetor 2026".
pub fn month_year(value: OffsetDateTime) -> String {
    const MONTHS: [&str; 12] = [
        "janar", "shkurt", "mars", "prill", "maj", "qershor", "korrik", "gusht", "shtator", "tetor", "nëntor", "dhjetor",
    ];
    format!("{} {}", MONTHS[usize::from(u8::from(value.month())) - 1], value.year())
}

/// The first letter of a name, for its round badge: "Auto Elbasani" gives "A".
pub fn initial(name: &str) -> String {
    name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_else(|| "?".to_string())
}

/// A phone number or an e-mail address, as someone signs in with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Contact {
    /// In E.164: +355691234567.
    Phone(String),
    /// In lower case.
    Email(String),
}

impl Contact {
    /// Reads a contact the way people type it: "069 123 4567", "0691234567", "+355 69 123 4567",
    /// "00355691234567" or "Ana@Example.com". Albanian numbers without a country code must be
    /// mobile numbers (06x), since the code goes out by SMS.
    pub fn parse(input: &str) -> Option<Contact> {
        let input = input.trim();
        if input.contains('@') {
            let email = input.to_lowercase();
            let (user, domain) = email.split_once('@')?;
            let valid = !user.is_empty()
                && !domain.contains('@')
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
                && email.len() <= 254
                && !email.chars().any(char::is_whitespace);
            return valid.then_some(Contact::Email(email));
        }
        let digits: String = input.chars().filter(|c| !matches!(c, ' ' | '-' | '.' | '(' | ')')).collect();
        let e164 = if let Some(rest) = digits.strip_prefix('+') {
            format!("+{rest}")
        } else if let Some(rest) = digits.strip_prefix("00") {
            format!("+{rest}")
        } else {
            format!("+355{}", digits.strip_prefix('0')?)
        };
        let number = &e164[1..];
        let valid = number.chars().all(|c| c.is_ascii_digit())
            && (7..=15).contains(&number.len())
            && !number.starts_with('0')
            && match number.strip_prefix("355") {
                Some(national) => national.len() == 9 && national.starts_with('6'),
                None => true,
            };
        valid.then_some(Contact::Phone(e164))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Contact::Phone(value) | Contact::Email(value) => value,
        }
    }

    pub fn phone(&self) -> Option<&str> {
        match self {
            Contact::Phone(phone) => Some(phone),
            Contact::Email(_) => None,
        }
    }

    pub fn email(&self) -> Option<&str> {
        match self {
            Contact::Email(email) => Some(email),
            Contact::Phone(_) => None,
        }
    }

    /// Enough to recognise it on the screen without showing it to someone looking over a
    /// shoulder: "+355 69 *** **67", "a***@example.com".
    pub fn masked(&self) -> String {
        match self {
            // The country and network codes and the last two digits stay.
            Contact::Phone(number) => {
                let shown = phone(number);
                let total = shown.chars().filter(char::is_ascii_digit).count();
                let keep_start = if number.starts_with("+355") { 5 } else { 4 };
                let mut seen = 0;
                shown
                    .chars()
                    .map(|c| {
                        if !c.is_ascii_digit() {
                            return c;
                        }
                        seen += 1;
                        if seen > keep_start && seen + 2 <= total { '*' } else { c }
                    })
                    .collect()
            }
            Contact::Email(email) => {
                let (user, domain) = email.split_once('@').unwrap_or((email, ""));
                let first: String = user.chars().take(1).collect();
                format!("{first}***@{domain}")
            }
        }
    }
}

/// The URL slug for a title: "Shtëpi 2+1 në Durrës!" becomes "shtepi-2-1-ne-durres".
pub fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in fold(title).chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug: String = slug.trim_end_matches('-').chars().take(80).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() { "njoftim".to_string() } else { slug }
}

/// Text as search compares it, in lower case and without accents: "Durrës" becomes "durres", as
/// the search index keeps it.
pub fn fold(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'ë' | 'é' | 'è' | 'ê' => 'e',
            'ç' | 'ć' | 'č' => 'c',
            'á' | 'à' | 'â' | 'ä' => 'a',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'š' => 's',
            'ž' => 'z',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn reads_contacts_the_way_people_type_them() {
        let phone = |n: &str| Some(Contact::Phone(n.to_string()));
        assert_eq!(Contact::parse("069 123 4567"), phone("+355691234567"));
        assert_eq!(Contact::parse("+355 69 123 4567"), phone("+355691234567"));
        assert_eq!(Contact::parse("00355-69-123-4567"), phone("+355691234567"));
        assert_eq!(Contact::parse("+39 333 123 4567"), phone("+393331234567"));
        assert_eq!(Contact::parse(" Ana@Example.com "), Some(Contact::Email("ana@example.com".to_string())));
        // A landline can't get an SMS; nor can half a number, or a broken address.
        assert_eq!(Contact::parse("04 222 3333"), None);
        assert_eq!(Contact::parse("069 123"), None);
        assert_eq!(Contact::parse("ana@example"), None);
        assert_eq!(Contact::parse("ana@@example.com"), None);
        assert_eq!(Contact::parse("hello"), None);
    }

    #[test]
    fn masks_contacts() {
        assert_eq!(Contact::Phone("+355691234567".to_string()).masked(), "+355 69 *** **67");
        assert_eq!(Contact::Email("ana@example.com".to_string()).masked(), "a***@example.com");
    }

    #[test]
    fn lists_words_in_quotes() {
        let words = |list: &[&str]| list.iter().map(|word| word.to_string()).collect::<Vec<_>>();
        assert_eq!(quoted_list(&[]), "");
        assert_eq!(quoted_list(&words(&["golf"])), "„golf“");
        assert_eq!(quoted_list(&words(&["golf", "mercedes"])), "„golf“ dhe „mercedes“");
        assert_eq!(quoted_list(&words(&["golf", "mercedes", "volkswagen"])), "„golf“, „mercedes“ dhe „volkswagen“");
    }

    #[test]
    fn folds_text_as_search_compares_it() {
        assert_eq!(fold("Shtëpi në Durrës, Korçë"), "shtepi ne durres, korce");
    }

    #[test]
    fn makes_slugs() {
        assert_eq!(slug("Shtëpi 2+1 në Durrës!"), "shtepi-2-1-ne-durres");
        assert_eq!(slug("  Çanta lëkure  "), "canta-lekure");
        assert_eq!(slug("!!!"), "njoftim");
    }

    fn amount(value: &str) -> Option<Decimal> {
        Some(Decimal::from_str(value).unwrap())
    }

    #[test]
    fn formats_prices() {
        assert_eq!(price(amount("9500.00"), "EUR", "FIXED", "SELL"), "9\u{a0}500\u{a0}€");
        assert_eq!(
            price(amount("60000"), "ALL", "NEGOTIABLE", "SELL"),
            "60\u{a0}000\u{a0}Lekë (i diskutueshëm)"
        );
        assert_eq!(
            price(amount("1234567.5"), "EUR", "FROM", "SELL"),
            "nga 1\u{a0}234\u{a0}567,50\u{a0}€"
        );
        assert_eq!(price(None, "EUR", "ON_REQUEST", "SELL"), "Çmimi me marrëveshje");
        assert_eq!(price(amount("0"), "ALL", "FREE", "SELL"), "Falas");
    }

    #[test]
    fn rents_and_salaries_are_per_month() {
        assert_eq!(price(amount("450"), "EUR", "FIXED", "RENT"), "450\u{a0}€/muaj");
        assert_eq!(
            price(amount("60000"), "ALL", "NEGOTIABLE", "JOB"),
            "60\u{a0}000\u{a0}Lekë/muaj (i diskutueshëm)"
        );
        assert_eq!(price(None, "ALL", "ON_REQUEST", "JOB"), "Paga me marrëveshje");
    }

    #[test]
    fn counts_listings() {
        assert_eq!(listing_count(0, 1000), "0 njoftime");
        assert_eq!(listing_count(1, 1000), "1 njoftim");
        assert_eq!(listing_count(1000, 1000), "1\u{a0}000 njoftime");
        assert_eq!(listing_count(1001, 1000), "1\u{a0}000+ njoftime");
    }

    #[test]
    fn short_listing_counts_say_thousands_as_k() {
        assert_eq!(short_listing_count(0, 1000), "0 njoftime");
        assert_eq!(short_listing_count(1, 1000), "1 njoftim");
        assert_eq!(short_listing_count(999, 1000), "999 njoftime");
        assert_eq!(short_listing_count(1000, 1000), "1k njoftime");
        assert_eq!(short_listing_count(1001, 1000), "1k+ njoftime");
        assert_eq!(short_listing_count(3412, 10_000), "3k njoftime");
        assert_eq!(short_listing_count(10_001, 10_000), "10k+ njoftime");
    }

    #[test]
    fn formats_distances() {
        assert_eq!(distance(0.0), "0\u{a0}m");
        assert_eq!(distance(847.0), "850\u{a0}m");
        assert_eq!(distance(960.0), "1,0\u{a0}km");
        assert_eq!(distance(2_430.0), "2,4\u{a0}km");
        assert_eq!(distance(18_400.0), "18\u{a0}km");
    }

    #[test]
    fn says_when_by_albanias_clock() {
        use time::macros::datetime;
        // A Saturday evening in October, by Albania's summer time (UTC+2): the 10th in Tirana.
        let now = datetime!(2026-10-10 21:30 UTC);
        assert_eq!(when(datetime!(2026-10-10 06:00 UTC), now), "Sot");
        // 22:30 UTC on the 9th is half past midnight on the 10th in Tirana: today, not yesterday.
        assert_eq!(when(datetime!(2026-10-09 22:30 UTC), now), "Sot");
        assert_eq!(when(datetime!(2026-10-09 21:00 UTC), now), "Dje");
        assert_eq!(when(datetime!(2026-10-07 12:00 UTC), now), "para 3 ditësh");
        assert_eq!(when(datetime!(2026-10-04 12:00 UTC), now), "para 6 ditësh");
        assert_eq!(when(datetime!(2026-10-03 12:00 UTC), now), "03.10.2026");
        // A moment just ahead of the clock still counts as today.
        assert_eq!(when(datetime!(2026-10-10 22:00 UTC), now), "Sot");
        // In winter (UTC+1): 23:30 UTC on the 4th of December is the 5th in Tirana.
        assert_eq!(date(datetime!(2026-12-04 23:30 UTC)), "05.12.2026");
        assert_eq!(date(datetime!(2026-12-04 22:30 UTC)), "04.12.2026");
        // Summer time starts on the last Sunday of March at 01:00 UTC (the 29th in 2026).
        assert_eq!(date(datetime!(2026-03-28 22:30 UTC)), "28.03.2026");
        assert_eq!(date(datetime!(2026-03-29 22:30 UTC)), "30.03.2026");
    }

    #[test]
    fn says_when_a_listing_expires_within_the_week() {
        use time::macros::datetime;
        // Saturday the 10th, half past nine in the evening in Tirana (UTC+2).
        let now = datetime!(2026-10-10 19:30 UTC);
        assert_eq!(expires_soon(datetime!(2026-10-10 21:00 UTC), now).as_deref(), Some("Skadon sot"));
        // 22:30 UTC is half past midnight on the 11th in Tirana: tomorrow.
        assert_eq!(expires_soon(datetime!(2026-10-10 22:30 UTC), now).as_deref(), Some("Skadon nesër"));
        assert_eq!(expires_soon(datetime!(2026-10-13 08:00 UTC), now).as_deref(), Some("Skadon pas 3 ditësh"));
        assert_eq!(expires_soon(datetime!(2026-10-16 08:00 UTC), now).as_deref(), Some("Skadon pas 6 ditësh"));
        // A week or more off, nothing yet; once past, nothing either (it has expired).
        assert_eq!(expires_soon(datetime!(2026-10-17 08:00 UTC), now), None);
        assert_eq!(expires_soon(datetime!(2026-10-10 19:00 UTC), now), None);
    }

    #[test]
    fn formats_albanian_mobile_numbers() {
        assert_eq!(phone("+355691234567"), "+355 69 123 4567");
        assert_eq!(phone("+390612345678"), "+390612345678");
    }
}
