//! The site in two languages: Albanian, its own, and English, for the diaspora and the foreigners
//! who buy and rent here (web/language.rs chooses, by the visitor's cookie). Whatever a page says
//! goes through `t`, keyed by its Albanian, which reads as it is in the templates and is what a
//! page says where there is no English for it; the English is the catalogue in i18n/en/, which
//! build.rs makes a sorted table of (EN). The language is the request's (LANG, which the language
//! layer sets around every request), or for what goes out on its own, an e-mail, its recipient's
//! (`in_lang`).

use std::{fmt::Display, future::Future};

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
}

/// The language now: the request's, or Albanian outside one.
pub fn lang() -> Lang {
    LANG.try_with(|lang| *lang).unwrap_or_default()
}

/// Whether the page is in English.
pub fn english() -> bool {
    lang() == Lang::En
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

    /// Every text the templates and the code give `t` (or `tf`) has its English: a page in English
    /// says nothing in Albanian but what people wrote.
    #[test]
    fn every_text_given_to_t_has_its_english() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut missing = Vec::new();
        let mut sources = Vec::new();
        for dir in ["templates", "src"] {
            let mut stack = vec![root.join(dir)];
            while let Some(path) = stack.pop() {
                if path.is_dir() {
                    stack.extend(std::fs::read_dir(&path).unwrap().filter_map(Result::ok).map(|entry| entry.path()));
                } else if path.extension().is_some_and(|ext| ext == "html" || ext == "rs") {
                    sources.push(path);
                }
            }
        }
        for path in sources {
            let text = std::fs::read_to_string(&path).unwrap();
            for key in texts_given_to_t(&text) {
                if in_english(&key).is_none() {
                    missing.push(format!("{}: „{key}“", path.strip_prefix(root).unwrap().display()));
                }
            }
        }
        assert!(missing.is_empty(), "no English in i18n/en/ for:\n{}", missing.join("\n"));
    }

    /// The literal texts in calls of `t(` or `tf(` (whatever comes before them but a letter, a
    /// digit or "_"), up to their closing quote.
    fn texts_given_to_t(source: &str) -> Vec<String> {
        let mut texts = Vec::new();
        for call in ["t(\"", "tf(\"", "t1(\"", "t2(\"", "t3(\""] {
            let mut from = 0;
            while let Some(at) = source[from..].find(call) {
                let start = from + at;
                from = start + call.len();
                let before = source[..start].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &source[from..];
                let Some(end) = rest.find('"') else { break };
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
