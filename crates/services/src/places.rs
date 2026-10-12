//! The places words name: a town, by its name in any of its forms (Tirana, Tiranës, Durrësi,
//! Shkodra), or by the two or three words of one (Malësi e Madhe), one shorter than four letters
//! (Mat, Has) only after a word that leads to a place ("në Mat"), words that name two towns naming
//! neither; and a zone of a town (migration 0019), named as a town is, its longer names first, a
//! name that zones of several towns share (Qendër) being each of theirs. What a new listing's title
//! or description names, for its form's "Zona" (`zone_in`: the pages' form and the JSON API's,
//! posting.rs's `zone_named`), and what a search's words name (pluribus-web's named.rs, which reads
//! the towns and the zones by these, and the categories and the kinds of listing itself). Moved out
//! of pluribus-web's named.rs in C9, for the API to guess a listing's zone as the pages do.

use pluribus_core::{
    domain::{City, Zone},
    format,
};

/// The words that join what is searched to a place, folded ("në" and "ne" alike): "apartamente në
/// Tiranë", "makina te Durrësi", "shtëpi afër Vlorës". A town's row of suggestions leaves the last
/// of them out of its search, since its filter says where (pluribus-web's suggestions.rs); so does
/// a search that names the town (named.rs), and a place read here takes the one before it along.
pub const PLACE_WORDS: [&str; 8] = ["ne", "te", "tek", "nga", "per", "prane", "afer", "rreth"];

/// A zone, with the town it is in.
pub type TownZone<'a> = (&'a City, &'a Zone);

/// One of a text's words: as typed, folded without the marks around it ("Tiranë." is tirane), and
/// whether a place (or, in a search, another filter) read it already.
pub struct Word<'w> {
    pub typed: &'w str,
    pub folded: String,
    pub used: bool,
}

/// The zone `text` names, a new listing's title or description (posting::zone_named), with its
/// town: in `town`, the one the seller chose, or, with none chosen, in a town the text names too
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
pub fn words(text: &str) -> Vec<Word<'_>> {
    text.split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | '/'))
        .filter(|typed| !typed.is_empty())
        .map(|typed| Word {
            typed,
            folded: format::fold(typed).trim_matches(|c: char| !c.is_alphanumeric()).to_string(),
            used: false,
        })
        .collect()
}

/// The zone the words name, as each town with a zone so called has it (Qendër is in three), its
/// words marked used, with the word that leads to it ("në"); None for none, or for two.
pub fn read_zone<'a>(words: &mut [Word], towns: &'a [City]) -> Option<Vec<TownZone<'a>>> {
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
pub fn read_town<'a>(words: &mut [Word], towns: &'a [City]) -> Option<&'a City> {
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

#[cfg(test)]
mod tests {
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
            zones,
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
}
