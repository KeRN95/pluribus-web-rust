//! The site in two languages: Albanian, its own, and English, for the diaspora and the foreigners
//! who buy and rent here (web/language.rs chooses, by the visitor's cookie). Whatever a page says
//! goes through `t`, keyed by its Albanian, which reads as it is in the templates and is what a
//! page says where there is no English for it; the English is the catalogue in i18n/en/, which
//! build.rs makes a sorted table of (EN). The language is the request's (LANG, which the language
//! layer sets around every request), or for what goes out on its own, an e-mail, its recipient's
//! (`in_lang`).

use std::{borrow::Cow, fmt::Display, future::Future};

use time::{Date, Month, macros::format_description};

include!(concat!(env!("OUT_DIR"), "/i18n_en.rs"));

/// A language the site speaks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    Sq,
    En,
}

impl Lang {
    /// Its code, as `<html lang>`, the cookie and users.preferred_language have it.
    pub fn code(self) -> &'static str {
        match self {
            Self::Sq => "sq",
            Self::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "sq" => Some(Self::Sq),
            "en" => Some(Self::En),
            _ => None,
        }
    }
}

tokio::task_local! {
    /// The language of the request being answered.
    pub static LANG: Lang;
    /// Whether its page offers English: to a visitor who hasn't chosen a language and whose
    /// browser reads no Albanian (web/language.rs).
    pub static OFFER: bool;
}

/// The language now: the request's, or Albanian outside one.
pub fn lang() -> Lang {
    LANG.try_with(|lang| *lang).unwrap_or_default()
}

/// Whether the page is in English.
pub fn english() -> bool {
    lang() == Lang::En
}

/// Whether the page offers English at its top (OFFER, base.html).
pub fn offer_english() -> bool {
    OFFER.try_with(|offer| *offer).unwrap_or(false)
}

/// What `text`, in Albanian, says in the language now: its English from the catalogue, or itself
/// where there is none. A template's text, `{{ crate::t("Shto një njoftim") }}`, a handler's, or
/// one from the database (a category's name, a property's choice), which the catalogue has too.
pub fn t<S: AsRef<str> + ?Sized>(text: &S) -> &str {
    let text = text.as_ref();
    match lang() {
        Lang::Sq => text,
        Lang::En => in_english(text).unwrap_or(text),
    }
}

/// `t`, then each `{name}` in it given its value: `tf("{n} njoftime", &[("n", &3)])`.
pub fn tf(text: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut said = t(text).to_string();
    for (name, value) in values {
        said = said.replace(&format!("{{{name}}}"), &value.to_string());
    }
    said
}

/// `t`, then its "{}" given `a`: `t1("para {} ditësh", 3)`. Templates call these, which hand
/// their values over by reference.
pub fn t1<S: AsRef<str> + ?Sized>(text: &S, a: impl Display) -> String {
    filled(t(text), &[&a])
}

/// `t`, then its two "{}" given `a` and `b`, in their order, or as "{1}" and "{2}" say where
/// the English puts them otherwise.
pub fn t2<S: AsRef<str> + ?Sized>(text: &S, a: impl Display, b: impl Display) -> String {
    filled(t(text), &[&a, &b])
}

pub fn t3<S: AsRef<str> + ?Sized>(text: &S, a: impl Display, b: impl Display, c: impl Display) -> String {
    filled(t(text), &[&a, &b, &c])
}

/// A text's places for values filled: "{1}", "{2}"... by their numbers, else each "{}" in turn.
fn filled(text: &str, values: &[&dyn Display]) -> String {
    if text.contains("{1}") {
        let mut said = text.to_string();
        for (at, value) in values.iter().enumerate() {
            said = said.replace(&format!("{{{}}}", at + 1), &value.to_string());
        }
        return said;
    }
    let mut said = String::with_capacity(text.len() + 16);
    let mut values = values.iter();
    let mut rest = text;
    while let Some(at) = rest.find("{}") {
        said.push_str(&rest[..at]);
        match values.next() {
            Some(value) => said.push_str(&value.to_string()),
            None => said.push_str("{}"),
        }
        rest = &rest[at + 2..];
    }
    said.push_str(rest);
    said
}

/// A property's value as the database words it for a page, in the language now: a choice's label
/// ("Naftë"), "Jo", or a number with its unit, apart by no-break spaces (format_attribute_number:
/// "148 000 km", "6,1 inç"), which in English takes a decimal point and its unit's English, and a
/// date ("04.10.2026") in English "4 Oct 2026". Whatever else it is, a text someone typed, stays.
pub fn t_value<S: AsRef<str> + ?Sized>(text: &S) -> Cow<'_, str> {
    let text = text.as_ref();
    if !english() {
        return Cow::Borrowed(text);
    }
    if let Some(english) = in_english(text) {
        return Cow::Borrowed(english);
    }
    let numeric = |part: &str| part.chars().any(|c| c.is_ascii_digit()) && part.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | '\u{a0}'));
    if let Some((number, unit)) = text.rsplit_once('\u{a0}')
        && numeric(number)
        && !unit.is_empty()
        && !numeric(unit)
    {
        // The units are kept in the plural ("muaj", "vite"); one is singular in English.
        let unit = in_english(unit).unwrap_or(unit);
        let unit = match number {
            "1" => unit.strip_suffix('s').filter(|one| one.len() > 1).unwrap_or(unit),
            _ => unit,
        };
        return Cow::Owned(format!("{}\u{a0}{unit}", number.replace(',', ".")));
    }
    if numeric(text) {
        return Cow::Owned(text.replace(',', "."));
    }
    let date = || {
        let mut parts = text.split('.').map(|part| part.parse::<u16>().ok());
        let (Some(Some(day)), Some(Some(month)), Some(Some(year)), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
            return None;
        };
        let month = Month::try_from(u8::try_from(month).ok()?).ok()?;
        let date = Date::from_calendar_date(i32::from(year), month, u8::try_from(day).ok()?).ok()?;
        date.format(format_description!("[day padding:none] [month repr:short] [year]")).ok()
    };
    if text.len() == 10
        && let Some(date) = date()
    {
        return Cow::Owned(date);
    }
    Cow::Borrowed(text)
}

/// A text the site keeps as it wrote it, in Albanian (a notification's title, "Çmimi ra: Golf 7"),
/// in the language now: the first of `templates` it fits ("Çmimi ra: {}"), said with the values
/// taken out of it; or the text as it is.
pub fn reworded<'a>(text: &'a str, templates: &[&str]) -> Cow<'a, str> {
    reworded_by(text, templates, |value| value)
}

/// `reworded`, each value said by `say`: the site's own words in it in the language now too
/// (`t`: "Si „Pronar“: …"), or a name it gave someone (`name`).
pub fn reworded_by<'a>(text: &'a str, templates: &[&str], say: impl Fn(&'a str) -> &'a str) -> Cow<'a, str> {
    if !english() {
        return Cow::Borrowed(text);
    }
    for template in templates {
        if let (Some(english), Some(values)) = (in_english(template), values_in(text, template)) {
            let values: Vec<&str> = values.into_iter().map(&say).collect();
            let values: Vec<&dyn Display> = values.iter().map(|value| value as &dyn Display).collect();
            return Cow::Owned(filled(english, &values));
        }
    }
    Cow::Borrowed(text)
}

/// A person's name as a page shows it: theirs, as they wrote it; or the one the site gave them,
/// in the language now: "Përdorues" to someone who gave none (db/sign_in.rs), "Përdorues i
/// fshirë" once their account is gone (db::DELETED_NAME).
pub fn name<S: AsRef<str> + ?Sized>(name: &S) -> &str {
    let name = name.as_ref();
    match name {
        "Përdorues" | "Përdorues i fshirë" => t(name),
        _ => name,
    }
}

/// The values that fill a template's "{}" places to make `text`, if it fits: each up to the next
/// piece of the template's own text, the last to the end.
fn values_in<'a>(text: &'a str, template: &str) -> Option<Vec<&'a str>> {
    let pieces: Vec<&str> = template.split("{}").collect();
    let (first, rest) = pieces.split_first()?;
    let mut text = text.strip_prefix(first)?;
    let mut values = Vec::new();
    for (at, piece) in rest.iter().enumerate() {
        let end = match at + 1 == rest.len() {
            true => text.strip_suffix(piece)?.len(),
            false => text.find(piece)?,
        };
        values.push(&text[..end]);
        text = &text[end + piece.len()..];
    }
    text.is_empty().then_some(values)
}

/// The scripts' texts in English (EN_JS) as JSON, which an English page carries for i18n.js: its
/// "<" escaped, so that nothing in it closes the script it is in.
pub fn scripts_json() -> String {
    let texts: serde_json::Map<String, serde_json::Value> =
        EN_JS.iter().map(|(key, english)| (key.to_string(), serde_json::Value::from(*english))).collect();
    serde_json::Value::Object(texts).to_string().replace('<', "\\u003c")
}

/// The catalogue's English for an Albanian text.
pub fn in_english(text: &str) -> Option<&'static str> {
    EN.binary_search_by(|(key, _)| (*key).cmp(text)).ok().map(|at| EN[at].1)
}

/// `work` in `lang`: what goes out on its own, in its recipient's language.
pub async fn in_lang<F: Future>(lang: Lang, work: F) -> F::Output {
    LANG.scope(lang, work).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_says_a_text_in_the_language_asked_for() {
        assert!(EN.windows(2).all(|pair| pair[0].0 < pair[1].0), "sorted, for the binary search");
        // Outside a request, Albanian.
        assert_eq!(t("Kushtet"), "Kushtet");
        LANG.sync_scope(Lang::En, || {
            assert_eq!(t("Kushtet"), "Terms");
            // What the catalogue lacks stays as it is.
            assert_eq!(t("Fjalë pa përkthim"), "Fjalë pa përkthim");
            assert_eq!(tf("{n} njoftime", &[("n", &3)]), "3 listings");
            assert_eq!(t1("{} njoftime", 24), "24 listings");
            assert_eq!(filled("{2} of {1}", &[&"a", &"b"]), "b of a");
        });
        LANG.sync_scope(Lang::Sq, || assert_eq!(tf("{n} njoftime", &[("n", &3)]), "3 njoftime"));
    }

    #[test]
    fn a_property_value_says_its_number_unit_and_date_the_english_way() {
        let nbsp = '\u{a0}';
        assert_eq!(t_value(&format!("148{nbsp}000{nbsp}km")), format!("148{nbsp}000{nbsp}km"), "Albanian outside a request");
        LANG.sync_scope(Lang::En, || {
            assert_eq!(t_value(&format!("148{nbsp}000{nbsp}km")), format!("148{nbsp}000{nbsp}km"));
            assert_eq!(t_value(&format!("6,1{nbsp}inç")), format!("6.1{nbsp}in"));
            assert_eq!(t_value(&format!("2{nbsp}muaj")), format!("2{nbsp}months"));
            assert_eq!(t_value(&format!("1{nbsp}muaj")), format!("1{nbsp}month"));
            assert_eq!(t_value(&format!("1{nbsp}copë")), format!("1{nbsp}pc"));
            assert_eq!(t_value(&format!("1{nbsp}m²")), format!("1{nbsp}m²"));
            assert_eq!(t_value("2,5"), "2.5");
            assert_eq!(t_value("2016"), "2016");
            assert_eq!(t_value("04.10.2026"), "4 Oct 2026");
            assert_eq!(t_value("Golf 7, 1.6 TDI"), "Golf 7, 1.6 TDI");
            assert_eq!(t_value("Kushtet"), "Terms");
        });
    }

    #[test]
    fn a_kept_albanian_text_is_reworded_for_an_english_reader() {
        let titles = ["Çmimi ra: {}", "{} të shkroi për „{}“"];
        assert_eq!(reworded("Çmimi ra: Golf 7", &titles), "Çmimi ra: Golf 7", "Albanian outside a request");
        LANG.sync_scope(Lang::En, || {
            assert_eq!(reworded("Çmimi ra: Golf 7", &titles), "Price drop: Golf 7");
            assert_eq!(reworded("Ana të shkroi për „Golf „7““", &titles), "Ana wrote to you about “Golf „7“”");
            // What fits none stays as it is.
            assert_eq!(reworded("Diçka tjetër", &titles), "Diçka tjetër");
            assert_eq!(values_in("Ana të shkroi për „Golf", "{} të shkroi për „{}“"), None);
            // The site's names for people, and its own words among the values, as it says them.
            assert_eq!(reworded_by("Përdorues të shkroi për „Golf“", &titles, |value| name(value)), "User wrote to you about “Golf”");
            assert_eq!(name("Përdorues i fshirë"), "Deleted user");
            assert_eq!(name("Kushtet"), "Kushtet", "a name someone gave themselves stays");
        });
    }

    /// Every text the templates and the code give `t` (or `tf`) has its English: a page in English
    /// says nothing in Albanian but what people wrote.
    /// And every text the scripts give theirs (i18n.js) is among the scripts' own (i18n/en/js*.txt),
    /// the only ones an English page carries for them.
    #[test]
    fn every_text_given_to_t_has_its_english() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut missing = Vec::new();
        let mut sources = Vec::new();
        for dir in ["templates", "src", "static/js"] {
            let mut stack = vec![root.join(dir)];
            while let Some(path) = stack.pop() {
                if path.is_dir() {
                    stack.extend(std::fs::read_dir(&path).unwrap().filter_map(Result::ok).map(|entry| entry.path()));
                } else if path.extension().is_some_and(|ext| ext == "html" || ext == "rs" || ext == "js") {
                    sources.push(path);
                }
            }
        }
        for path in sources {
            let text = std::fs::read_to_string(&path).unwrap();
            let script = path.extension().is_some_and(|ext| ext == "js");
            let calls: &[&str] = match script {
                true => &["t(\"", "tf(\"", "t('", "tf('"],
                false => &["t(\"", "tf(\"", "t1(\"", "t2(\"", "t3(\""],
            };
            for key in texts_given_to_t(&text, calls) {
                let known = match script {
                    true => EN_JS.binary_search_by(|(text, _)| (*text).cmp(&key)).is_ok(),
                    false => in_english(&key).is_some(),
                };
                if !known {
                    missing.push(format!("{}: „{key}“", path.strip_prefix(root).unwrap().display()));
                }
            }
        }
        assert!(missing.is_empty(), "no English in i18n/en/ (for a script, in a js*.txt file) for:\n{}", missing.join("\n"));
    }

    /// A template's text that goes out as it is (crate::t of a text alone, then "|safe": one with an
    /// apostrophe, which escaping would turn into "&#39;") says nothing HTML would read as markup,
    /// in either language.
    #[test]
    fn a_text_that_goes_out_unescaped_has_no_markup() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let mut wrong = Vec::new();
        for entry in std::fs::read_dir(&root).unwrap().filter_map(Result::ok) {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            for (at, _) in text.match_indices("\")|safe") {
                let Some(start) = text[..at].rfind("t(\"") else { continue };
                let key = &text[start + 3..at];
                let english = in_english(key).unwrap_or(key);
                if [key, english].iter().any(|said| said.contains(['<', '>', '&', '"'])) {
                    wrong.push(format!("{}: „{key}“", entry.file_name().to_string_lossy()));
                }
            }
        }
        assert!(wrong.is_empty(), "texts that go out unescaped with markup in them:\n{}", wrong.join("\n"));
    }

    /// The literal texts given to `calls` (a function's name, its parenthesis and the quote that
    /// opens the text; whatever comes before them but a letter, a digit, "_" or "."), up to the
    /// quote that closes them.
    fn texts_given_to_t(source: &str, calls: &[&str]) -> Vec<String> {
        let mut texts = Vec::new();
        for call in calls {
            let quote = call.chars().next_back().expect("a call ends with its quote");
            let mut from = 0;
            while let Some(at) = source[from..].find(call) {
                let start = from + at;
                from = start + call.len();
                let before = source[..start].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
                let rest = &source[from..];
                let Some(end) = rest.find(quote) else { break };
                let key = &rest[..end];
                // This file's own examples of what is missing.
                if key != "Fjalë pa përkthim" {
                    texts.push(key.to_string());
                }
            }
        }
        texts
    }
}
