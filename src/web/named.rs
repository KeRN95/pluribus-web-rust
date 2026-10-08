//! The filters a search's words name. "Apartament me qera ne tirane" names a category
//! (Apartamente), a kind of listing (for rent) and a town (Tiranë); searched as words, it found
//! the listings saying them, in any category, with none of the page's filters on. A search that
//! names any of them goes to the page with those filters on instead, each a chip to take off if
//! it was misread, and the words that name nothing stay searched there:
//! /search?category=apartamente&city=tirane&type=RENT.
//!
//! - A town is named by its name in any of its forms (Tirana, Tiranës, Durrësi, Shkodra), or by
//!   the two or three words of one (Malësi e Madhe); one shorter than four letters (Mat, Has) only
//!   after a word that leads to a place ("në Mat"). Words that name two towns name neither.
//! - A kind of listing by the words search_kind knows (migration 0024: shitje, qira, blej...),
//!   where the category takes that kind and another, as its type filter does.
//! - A category only in a search across every category, by the first word that is none of the
//!   others, as the thing sought comes first ("kufje për telefon" is headphones). The word names
//!   the categories whose names are about it, in any of its forms: a name is about its first word
//!   and those "dhe" joins to it ("Vila dhe shtëpi"), not the words that say of what ("Tapete
//!   makine" are mats). Of several, the one that takes the kind named, then the one whose name has
//!   most of the search's words, the most of them as typed, then the shortest name. Two still
//!   alike name neither ("shtëpi" is Vila dhe shtëpi and Shtëpi dhe kopsht alike, but "shtëpi me
//!   qira" only the first).

use super::{filters::url, suggestions::PLACE_WORDS};
use crate::{
    db::{Categories, Category, City},
    format,
};

/// Words that join others, folded, and name nothing: those before a word read as a filter go
/// with it ("me qira", "në Tiranë"), and words left that are only these aren't searched. As
/// search_query has them (migration 0024), and the verbs ads start with, which come before what
/// is sought ("kërkoj apartament me qira", "jepet me qira").
const FILLERS: [&str; 21] = [
    "ne", "me", "per", "dhe", "e", "i", "te", "nje", "nga", "tek", "qe", "kerkoj", "kerkohet", "kerkohen", "jap", "jep",
    "jepet", "jepen", "ofroj", "ofrohet", "ofrohen",
];

/// Endings sq_stem takes off a word (migration 0016), the longest first.
const ENDINGS: [&str; 16] = ["ave", "eve", "ash", "ve", "at", "et", "it", "ut", "in", "en", "es", "ja", "a", "e", "i", "u"];

/// How well a category fits a search, the best the greatest: whether it takes the kind named,
/// how many of the words its name has, how many of them as typed, and how short its name is.
type Fit = (bool, usize, usize, std::cmp::Reverse<usize>);

/// What a search's words name.
pub struct Named<'a> {
    pub category: Option<&'a Category>,
    pub kind: Option<&'static str>,
    pub town: Option<&'a City>,
    /// The words that name none of them, as typed.
    pub rest: Option<String>,
}

struct Word<'w> {
    typed: &'w str,
    folded: String,
    used: bool,
}

/// What `words` name, or None when they name nothing a filter could take. `category` is the
/// page's (or the one its category parameter names): its words then stay words.
pub fn read<'a>(words: &str, category: Option<&'a Category>, categories: &'a Categories, towns: &'a [City]) -> Option<Named<'a>> {
    let mut words: Vec<Word> = words
        .split_whitespace()
        .map(|typed| Word {
            typed,
            folded: format::fold(typed).trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
            used: false,
        })
        .collect();

    let town = read_town(&mut words, towns);

    // The kind the words name, if they name one only.
    let mut kinds: Vec<&'static str> = words.iter().filter_map(|word| kind(&word.folded)).collect();
    kinds.dedup();
    let named_kind = match kinds.as_slice() {
        [kind] => Some(*kind),
        _ => None,
    };

    let named_category = match category {
        Some(_) => None,
        None => read_category(&mut words, categories, named_kind),
    };
    let takes = |category: &Category, kind: &str| category.listing_types.len() > 1 && category.listing_types.iter().any(|t| t == kind);
    let kind = named_kind.filter(|kind| named_category.or(category).is_some_and(|category| takes(category, kind)));
    if let Some(kind) = kind {
        for at in 0..words.len() {
            if self::kind(&words[at].folded) == Some(kind) {
                use_with_fillers(&mut words, at);
            }
        }
    }

    if town.is_none() && named_category.is_none() && kind.is_none() {
        return None;
    }
    let left: Vec<&Word> = words.iter().filter(|word| !word.used).collect();
    let rest = match left.iter().all(|word| FILLERS.contains(&word.folded.as_str())) {
        true => None,
        false => Some(left.iter().map(|word| word.typed).collect::<Vec<_>>().join(" ")),
    };
    Some(Named { category: named_category, kind, town, rest })
}

/// Where the page at `path`, asked for with `query`, shows what its words name: the words that
/// name nothing searched, then the filters they name, then its other filters as they were.
pub fn url_for(path: &str, query: Option<&str>, named: &Named) -> String {
    let mut pairs: Vec<(String, String)> = Vec::new();
    if let Some(rest) = &named.rest {
        pairs.push(("q".to_string(), rest.clone()));
    }
    if let Some(category) = named.category {
        pairs.push(("category".to_string(), category.slug.clone()));
    }
    if let Some(town) = named.town {
        pairs.push(("city".to_string(), town.slug.clone()));
    }
    if let Some(kind) = named.kind {
        pairs.push(("type".to_string(), kind.to_string()));
    }
    let replaced = |name: &str| match name {
        "q" | "after" | "numri" => true,
        "category" => named.category.is_some(),
        "city" | "lat" | "lng" | "radius" => named.town.is_some(),
        "type" => named.kind.is_some(),
        _ => false,
    };
    pairs.extend(
        form_urlencoded::parse(query.unwrap_or_default().as_bytes())
            .map(|(name, value)| (name.into_owned(), value.trim().to_string()))
            .filter(|(name, value)| !value.is_empty() && !replaced(name)),
    );
    url(path, pairs)
}

/// The town the words name, its words marked used, with the word that leads to it ("në").
fn read_town<'a>(words: &mut [Word], towns: &'a [City]) -> Option<&'a City> {
    let mut found: Option<(&City, usize, usize)> = None;
    let mut at = 0;
    while at < words.len() {
        // The longest name first: Malësi e Madhe before a town called Madhe, were there one.
        let named = (1..=3).rev().filter(|len| at + len <= words.len()).find_map(|len| {
            let joined = words[at..at + len].iter().map(|word| word.folded.as_str()).collect::<Vec<_>>().join("-");
            let after_place = at > 0 && PLACE_WORDS.contains(&words[at - 1].folded.as_str());
            towns
                .iter()
                .find(|town| match len {
                    1 => names_town(&joined, town) && (town.slug.len() >= 4 || after_place),
                    _ => joined == town.slug,
                })
                .map(|town| (town, len))
        });
        match (named, found) {
            (Some((town, _)), Some((other, ..))) if town.id != other.id => return None,
            (Some((town, len)), None) => found = Some((town, at, len)),
            _ => {}
        }
        at += named.map_or(1, |(_, len)| len);
    }
    let (town, at, len) = found?;
    for word in &mut words[at..at + len] {
        word.used = true;
    }
    if at > 0 && PLACE_WORDS.contains(&words[at - 1].folded.as_str()) {
        words[at - 1].used = true;
    }
    Some(town)
}

/// Whether a folded word is the town's name in one of its forms: as it is (tirane), with the
/// article or a case ending (tirana, tiranes, tiranen; durresi, durresit, durresin; shijaku), and
/// with the ë before a last r, l or z gone, as the language drops it (shkodra, shkodres; kamza).
fn names_town(word: &str, town: &City) -> bool {
    let name = town.slug.as_str();
    if word == name {
        return true;
    }
    if name.contains('-') {
        return false;
    }
    let ends = |stem: &str, endings: &[&str]| endings.iter().any(|ending| word.strip_prefix(stem) == Some(*ending));
    if let Some(stem) = name.strip_suffix('e') {
        return ends(stem, &["a", "es", "en"]);
    }
    let lowered = town.name.to_lowercase();
    if lowered.ends_with("ër") || lowered.ends_with("ël") || lowered.ends_with("ëz") {
        let (head, last) = name.split_at(name.len() - 2);
        return ends(&format!("{head}{}", &last[1..]), &["a", "es", "en"]);
    }
    match name.ends_with(['k', 'g', 'h']) {
        true => ends(name, &["u", "ut", "un"]),
        false => ends(name, &["i", "it", "in"]),
    }
}

/// The kind of listing a folded word names, as search_kind has them (migration 0024).
fn kind(word: &str) -> Option<&'static str> {
    match word {
        "shitje" | "shitja" | "shitjes" | "shitet" | "shiten" | "shes" => Some("SELL"),
        "qira" | "qiraja" | "qirase" | "qera" | "qeraja" | "qerase" => Some("RENT"),
        "blej" | "blejme" | "blihet" => Some("BUY"),
        _ => None,
    }
}

/// The category the first word that is no filler, kind or town names (see the module's note), its
/// words in the search marked used.
fn read_category<'a>(words: &mut [Word], categories: &'a Categories, kind: Option<&str>) -> Option<&'a Category> {
    let sought: Vec<usize> = (0..words.len())
        .filter(|at| !words[*at].used && !FILLERS.contains(&words[*at].folded.as_str()) && self::kind(&words[*at].folded).is_none())
        .collect();
    let first = &words[*sought.first()?].folded;
    let mut candidates: Vec<(&Category, Fit)> = categories
        .iter()
        .filter_map(|category| {
            if !about(&category.name).iter().any(|word| same_word(first, word)) {
                return None;
            }
            let name = name_words(&category.name);
            let matched = sought.iter().filter(|at| name.iter().any(|word| same_word(&words[**at].folded, word))).count();
            let typed = sought.iter().filter(|at| name.contains(&words[**at].folded)).count();
            let takes = kind.is_some_and(|kind| category.listing_types.iter().any(|t| t == kind));
            Some((category, (takes, matched, typed, std::cmp::Reverse(name.len()))))
        })
        .collect();
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let (category, key) = candidates.first()?;
    if candidates.get(1).is_some_and(|(_, other)| other == key) {
        return None;
    }
    let name = name_words(&category.name);
    for at in sought {
        if name.iter().any(|word| same_word(&words[at].folded, word)) {
            words[at].used = true;
        }
    }
    Some(category)
}

/// The words a category's name is about, folded: its first and those "dhe" joins to it ("Vila dhe
/// shtëpi": vila, shtepi), not those that say of what ("Tapete makine": tapete; "Aksesorë për
/// kafshë": aksesore).
fn about(name: &str) -> Vec<String> {
    let words: Vec<String> = format::fold(name)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    (0..words.len()).filter(|at| *at == 0 || words[at - 1] == "dhe").map(|at| words[at].clone()).collect()
}

/// The words of a category's name that say what it is, folded: "Vila dhe shtëpi" is vila, shtepi.
fn name_words(name: &str) -> Vec<String> {
    format::fold(name)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !FILLERS.contains(word))
        .map(str::to_string)
        .collect()
}

/// Whether two folded words are one word in two forms: the same once sq_stem's ending is off
/// (migration 0016), keeping three letters where it keeps four, so that vila and vile meet, and
/// qen and qeni.
fn same_word(a: &str, b: &str) -> bool {
    a == b || stem(a) == stem(b)
}

fn stem(word: &str) -> &str {
    ENDINGS
        .iter()
        .find_map(|ending| word.strip_suffix(ending))
        .filter(|stem| stem.len() >= 3)
        .unwrap_or(word)
}

/// Marks the word at `at` used, and the fillers just before it, which go with it ("me qira").
fn use_with_fillers(words: &mut [Word], at: usize) {
    words[at].used = true;
    let mut before = at;
    while before > 0 && FILLERS.contains(&words[before - 1].folded.as_str()) {
        before -= 1;
        words[before].used = true;
    }
}

#[cfg(test)]
mod tests {
    use sqlx::types::Json;
    use uuid::Uuid;

    use super::*;

    fn town(name: &str) -> City {
        City {
            id: Uuid::nil(),
            name: name.to_string(),
            slug: format::slug(name),
            county: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            zones: Json(Vec::new()),
        }
    }

    #[test]
    fn a_town_is_named_in_its_forms() {
        for (name, forms) in [
            ("Tiranë", &["tirane", "tirana", "tiranes", "tiranen"][..]),
            ("Vlorë", &["vlore", "vlora", "vlores"]),
            ("Durrës", &["durres", "durresi", "durresit", "durresin"]),
            ("Shijak", &["shijak", "shijaku", "shijakut"]),
            ("Shkodër", &["shkoder", "shkodra", "shkodres"]),
            ("Kamëz", &["kamez", "kamza", "kamzes"]),
            ("Fushë-Arrëz", &["fushe-arrez"]),
        ] {
            for form in forms {
                assert!(names_town(form, &town(name)), "{name}: {form}");
            }
        }
        for (name, word) in [("Tiranë", "tiranez"), ("Durrës", "durresa"), ("Berat", "bera"), ("Shkodër", "shkodri")] {
            assert!(!names_town(word, &town(name)), "{name}: {word}");
        }
    }

    #[test]
    fn words_meet_in_their_forms() {
        for (a, b) in [("apartament", "apartamente"), ("makine", "makina"), ("vile", "vila"), ("qeni", "qen"), ("telefoni", "telefona")] {
            assert!(same_word(a, b), "{a} and {b}");
        }
        // Different words that start alike stay apart.
        for (a, b) in [("makina", "makineri"), ("audio", "audi"), ("ora", "or")] {
            assert!(!same_word(a, b), "{a} and {b}");
        }
    }
}
