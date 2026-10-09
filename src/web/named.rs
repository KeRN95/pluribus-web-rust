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
//! - A zone of a town (migration 0019) only where the search is about property, whose listings
//!   can say theirs: the page's category, or the one the words name, is Prona or under it
//!   ("apartament 2+1 në Plazh" is the flats in Plazh, in Durrës). Elsewhere its name stays a word
//!   ("peshqir plazhi" is a beach towel). It is named as a town is, and read before the towns, as
//!   some are named after one (Rruga e Durrësit is in Tirana); it puts its town on too. A name
//!   that zones of several towns share (Qendër) is the one in the town the words name, or else in
//!   the page's; a zone in another town than the one named, and two zones, name none.
//! - A kind of listing by the words search_kind knows (migration 0024: shitje, qira, blej...),
//!   where the category takes that kind and another, as its type filter does.
//! - A category only in a search across every category, by the first word that is none of the
//!   others, as the thing sought comes first ("kufje për telefon" is headphones). The word names
//!   the categories whose names are about it, in any of its forms: a name is about its first word
//!   and those a "/" (or "dhe") joins to it ("Vila / shtëpi"), not the words that say of what
//!   ("Tapete makine" are mats). Of several, the one that takes the kind named, then the one whose
//!   name has most of the search's words, the most of them as typed, then the shortest name. Two
//!   still alike name neither ("shtëpi" is Vila / shtëpi and Shtëpi / kopsht alike, but "shtëpi
//!   me qira" only the first).

use super::{filters::url, post::zoned, suggestions::PLACE_WORDS};
use crate::{
    db::{Categories, Category, City, Zone},
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

/// A zone, with the town it is in.
type TownZone<'a> = (&'a City, &'a Zone);

/// What a search's words name.
pub struct Named<'a> {
    pub category: Option<&'a Category>,
    pub kind: Option<&'static str>,
    /// The town, the zone's when they name a zone.
    pub town: Option<&'a City>,
    pub zone: Option<&'a Zone>,
    /// The words that name none of them, as typed.
    pub rest: Option<String>,
}

struct Word<'w> {
    typed: &'w str,
    folded: String,
    used: bool,
}

/// What `words` name, or None when they name nothing a filter could take. `category` is the
/// page's (or the one its category parameter names): its words then stay words. `town` is the
/// page's, where a zone's name that several towns' zones share is.
pub fn read<'a>(
    words: &str,
    category: Option<&'a Category>,
    town: Option<&'a City>,
    categories: &'a Categories,
    towns: &'a [City],
) -> Option<Named<'a>> {
    // Read with a zone, then kept where the search turns out to be about property; read again
    // without one otherwise, its name a word.
    let in_property = |named: &Named| named.category.or(category).is_some_and(|category| zoned(categories, category));
    read_words(words, category, Some(town), categories, towns)
        .filter(|named| named.zone.is_some() && in_property(named))
        .or_else(|| read_words(words, category, None, categories, towns))
}

/// What `words` name (see read): with `zone_town` a zone, with the page's town in it, and nothing
/// without one.
fn read_words<'a>(
    words: &str,
    category: Option<&'a Category>,
    zone_town: Option<Option<&'a City>>,
    categories: &'a Categories,
    towns: &'a [City],
) -> Option<Named<'a>> {
    let mut words = self::words(words);

    let zones = match zone_town {
        Some(_) => Some(read_zone(&mut words, towns)?),
        None => None,
    };
    let mut town = read_town(&mut words, towns);
    let mut zone = None;
    if let Some(zones) = zones {
        // Of the zones so called, the one in the town named, or else in the page's.
        let (zone_town, named) = match (town, zones.as_slice()) {
            (Some(town), _) => zones.iter().find(|(city, _)| city.id == town.id).copied()?,
            (None, [one]) => *one,
            (None, _) => {
                let page = zone_town.flatten()?;
                zones.iter().find(|(city, _)| city.id == page.id).copied()?
            }
        };
        town = Some(zone_town);
        zone = Some(named);
    }

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
    Some(Named { category: named_category, kind, town, zone, rest })
}

/// The zone `text` names, a new listing's title or description (post::guess_zone), with its town:
/// in `town`, the one the seller chose, or, with none chosen, in a town the text names too
/// ("Blloku, Tiranë"). A zone named with no town to say whose ("pranë plazhit" could be any
/// town's), or in another town than the one chosen, is none, as are two zones.
pub fn zone_in<'a>(text: &str, town: Option<&'a City>, towns: &'a [City]) -> Option<TownZone<'a>> {
    let mut words = words(text);
    let zones = read_zone(&mut words, towns)?;
    let town = match town {
        Some(town) => town,
        None => read_town(&mut words, towns)?,
    };
    zones.into_iter().find(|(city, _)| city.id == town.id)
}

/// Text as words, each folded and without the marks around it ("Tiranë." is tirane): split at
/// spaces, and at the commas and slashes that run places together ("Blloku,Tiranë").
fn words(text: &str) -> Vec<Word<'_>> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '/'))
        .filter(|typed| !typed.is_empty())
        .map(|typed| Word {
            typed,
            folded: format::fold(typed).trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
            used: false,
        })
        .collect()
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
    if let Some(zone) = named.zone {
        pairs.push(("zone".to_string(), zone.slug.clone()));
    }
    if let Some(kind) = named.kind {
        pairs.push(("type".to_string(), kind.to_string()));
    }
    let replaced = |name: &str| match name {
        "q" | "after" | "numri" => true,
        "category" => named.category.is_some(),
        "city" | "zone" | "lat" | "lng" | "radius" => named.town.is_some(),
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

/// The zone the words name, as each town with a zone so called has it (Qendër is in three), its
/// words marked used, with the word that leads to it ("në"); None for none, or for two.
fn read_zone<'a>(words: &mut [Word], towns: &'a [City]) -> Option<Vec<TownZone<'a>>> {
    let mut found: Option<(Vec<TownZone>, usize, usize)> = None;
    let mut at = 0;
    while at < words.len() {
        // The longest name first: Plazhi i Vjetër, in Vlorë, before Plazh, in Durrës.
        let named = (1..=3).rev().filter(|len| at + len <= words.len()).find_map(|len| {
            let zones: Vec<TownZone> = towns
                .iter()
                .flat_map(|town| town.zones.iter().map(move |zone| (town, zone)))
                .filter(|(_, zone)| names_zone(&words[at..at + len], zone))
                .collect();
            (!zones.is_empty()).then_some((zones, len))
        });
        let Some((zones, len)) = named else {
            at += 1;
            continue;
        };
        match found.as_ref().map(|(other, ..)| other[0].1.slug == zones[0].1.slug) {
            Some(false) => return None,
            Some(true) => {}
            None => found = Some((zones, at, len)),
        }
        at += len;
    }
    let (zones, at, len) = found?;
    use_place(words, at, len);
    Some(zones)
}

/// The town the words name, its words marked used, with the word that leads to it ("në"). Words
/// a zone took are not a town's (Rruga e Durrësit).
fn read_town<'a>(words: &mut [Word], towns: &'a [City]) -> Option<&'a City> {
    let mut found: Option<(&City, usize, usize)> = None;
    let mut at = 0;
    while at < words.len() {
        // The longest name first: Malësi e Madhe before a town called Madhe, were there one.
        let named = (1..=3).rev().filter(|len| at + len <= words.len()).find_map(|len| {
            if words[at..at + len].iter().any(|word| word.used) {
                return None;
            }
            let joined = joined(&words[at..at + len]);
            let after_place = at > 0 && PLACE_WORDS.contains(&words[at - 1].folded.as_str());
            towns
                .iter()
                .find(|town| match len {
                    1 => names_place(&joined, &town.slug, &town.name) && (town.slug.len() >= 4 || after_place),
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
    use_place(words, at, len);
    Some(town)
}

/// Whether `words` are a zone's name: one word in any of its forms, as a town's (names_place),
/// with its hyphen if it has one ("ish-kenete"); or its two or three words, the first in any of its
/// forms ("Komunën e Parisit", "Rrugën e Durrësit"), the others as they are.
fn names_zone(words: &[Word], zone: &Zone) -> bool {
    let slugs: Vec<&str> = zone.slug.split('-').collect();
    let names: Vec<&str> = zone.name.split([' ', '-']).collect();
    match words {
        [word] => names_place(&word.folded, &zone.slug, &zone.name),
        [first, rest @ ..] => {
            slugs.len() == words.len()
                && names_place(&first.folded, slugs[0], names.first().copied().unwrap_or_default())
                && rest.iter().zip(&slugs[1..]).all(|(word, slug)| word.folded == *slug)
        }
        [] => false,
    }
}

/// Words folded and joined as a slug joins them: "komuna-e-parisit".
fn joined(words: &[Word]) -> String {
    words.iter().map(|word| word.folded.as_str()).collect::<Vec<_>>().join("-")
}

/// Marks a place's `len` words from `at` used, and the word that leads to it ("në").
fn use_place(words: &mut [Word], at: usize, len: usize) {
    for word in &mut words[at..at + len] {
        word.used = true;
    }
    if at > 0 && PLACE_WORDS.contains(&words[at - 1].folded.as_str()) {
        words[at - 1].used = true;
    }
}

/// Whether a folded word is a place's one-word name (`slug`, `name`) in one of its forms: as it is
/// (tirane), with the article or a case ending (tirana, tiranes, tiranen; durresi, durresit,
/// durresin; shijaku, but gramshi and plazhi, whose h is part of a letter), a name that has its
/// article already with another ending or none (blloku: bllokut, bllokun, "në Bllok"; komuna:
/// komunes, komunen), and with the ë before a last r, l or z gone, as the language drops it
/// (shkodra, shkodres; kamza).
fn names_place(word: &str, slug: &str, name: &str) -> bool {
    if word == slug {
        return true;
    }
    if slug.contains('-') {
        return false;
    }
    let ends = |stem: &str, endings: &[&str]| endings.iter().any(|ending| word.strip_prefix(stem) == Some(*ending));
    if let Some(stem) = slug.strip_suffix('e') {
        return ends(stem, &["a", "es", "en"]);
    }
    let lowered = name.to_lowercase();
    if lowered.ends_with("ër") || lowered.ends_with("ël") || lowered.ends_with("ëz") {
        let (head, last) = slug.split_at(slug.len() - 2);
        return ends(&format!("{head}{}", &last[1..]), &["a", "es", "en"]);
    }
    if let Some(stem) = slug.strip_suffix(['u', 'i']) {
        return word == stem || ends(slug, &["t", "n"]);
    }
    if let Some(stem) = slug.strip_suffix('a') {
        return ends(stem, &["es", "en"]);
    }
    let digraph = ["dh", "sh", "th", "xh", "zh"].iter().any(|letter| slug.ends_with(letter));
    match slug.ends_with(['k', 'g', 'h']) && !digraph {
        true => ends(slug, &["u", "ut", "un"]),
        false => ends(slug, &["i", "it", "in"]),
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
    candidates.sort_by_key(|(_, fit)| std::cmp::Reverse(*fit));
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

/// The words a category's name is about, folded: its first and those "dhe" or a "/" joins to it
/// ("Vila / shtëpi": vila, shtepi; migration 0028), not those that say of what ("Tapete makine":
/// tapete; "Aksesorë për kafshë": aksesore).
fn about(name: &str) -> Vec<String> {
    let words: Vec<String> = format::fold(&name.replace('/', " dhe "))
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect();
    (0..words.len()).filter(|at| *at == 0 || words[at - 1] == "dhe").map(|at| words[at].clone()).collect()
}

/// The words of a category's name that say what it is, folded: "Vila / shtëpi" is vila, shtepi.
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

    fn names(word: &str, name: &str) -> bool {
        names_place(word, &format::slug(name), name)
    }

    #[test]
    fn a_place_is_named_in_its_forms() {
        for (name, forms) in [
            ("Tiranë", &["tirane", "tirana", "tiranes", "tiranen"][..]),
            ("Vlorë", &["vlore", "vlora", "vlores"]),
            ("Durrës", &["durres", "durresi", "durresit", "durresin"]),
            ("Shijak", &["shijak", "shijaku", "shijakut"]),
            ("Shkodër", &["shkoder", "shkodra", "shkodres"]),
            ("Kamëz", &["kamez", "kamza", "kamzes"]),
            ("Fushë-Arrëz", &["fushe-arrez"]),
            ("Gramsh", &["gramsh", "gramshi", "gramshit"]),
            // Zones: one whose h is part of a letter, two with their article already, one with an ë.
            ("Plazh", &["plazh", "plazhi", "plazhit", "plazhin"]),
            ("Blloku", &["blloku", "bllokut", "bllokun", "bllok"]),
            ("Komuna", &["komuna", "komunes", "komunen"]),
            ("Qendër", &["qender", "qendra", "qendres", "qendren"]),
        ] {
            for form in forms {
                assert!(names(form, name), "{name}: {form}");
            }
        }
        for (name, word) in [
            ("Tiranë", "tiranez"),
            ("Durrës", "durresa"),
            ("Berat", "bera"),
            ("Shkodër", "shkodri"),
            ("Gramsh", "gramshu"),
            ("Blloku", "bllokui"),
        ] {
            assert!(!names(word, name), "{name}: {word}");
        }
    }

    /// A town with these zones, its id `n`.
    fn town(n: u128, name: &str, zones: &[&str]) -> City {
        let zones = zones
            .iter()
            .enumerate()
            .map(|(at, zone)| Zone { id: Uuid::from_u128(n * 100 + at as u128 + 1), name: zone.to_string(), slug: format::slug(zone) })
            .collect();
        City {
            id: Uuid::from_u128(n),
            name: name.to_string(),
            slug: format::slug(name),
            county: String::new(),
            latitude: 0.0,
            longitude: 0.0,
            zones: Json(zones),
        }
    }

    #[test]
    fn a_listing_names_its_zone_with_its_town() {
        let towns = [
            town(1, "Tiranë", &["Blloku", "Komuna e Parisit", "Qendër", "Rruga e Durrësit"]),
            town(2, "Durrës", &["Plazh", "Qendër"]),
        ];
        let zone = |text: &str, chosen: Option<usize>| {
            zone_in(text, chosen.map(|at| &towns[at]), &towns).map(|(town, zone)| format!("{}/{}", town.slug, zone.slug))
        };
        // In the town chosen, in any of its forms, of one word or a few.
        assert_eq!(zone("Apartament 2+1 në Bllok", Some(0)).as_deref(), Some("tirane/blloku"));
        assert_eq!(zone("Apartament në Komunën e Parisit", Some(0)).as_deref(), Some("tirane/komuna-e-parisit"));
        assert_eq!(zone("Apartament në qendër", Some(1)).as_deref(), Some("durres/qender"));
        // With no town chosen, a zone with the town the words name too: one named after a town
        // isn't that town.
        assert_eq!(zone("Apartament, Blloku,Tiranë", None).as_deref(), Some("tirane/blloku"));
        assert_eq!(zone("Apartament në Rrugën e Durrësit, Tiranë", None).as_deref(), Some("tirane/rruga-e-durresit"));
        // A zone with no town to say whose, or in another town than the one chosen, or two zones,
        // are none.
        assert_eq!(zone("Apartament pranë plazhit", None), None);
        assert_eq!(zone("Apartament pranë plazhit", Some(0)), None);
        assert_eq!(zone("Apartament në Bllok, 5 minuta nga qendra", Some(0)), None);
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

    #[test]
    fn a_name_is_about_the_words_a_slash_or_dhe_joins() {
        assert_eq!(about("Vila / shtëpi"), ["vila", "shtepi"]);
        assert_eq!(about("Vila dhe shtëpi"), ["vila", "shtepi"]);
        assert_eq!(about("Shtëpi / kopsht"), ["shtepi", "kopsht"]);
        // Not the words that say of what.
        assert_eq!(about("Aksesorë për kafshë"), ["aksesore"]);
        assert_eq!(about("Tapete makine"), ["tapete"]);
    }
}
