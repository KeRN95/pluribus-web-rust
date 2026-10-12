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
//!   ("Tapete makine" are mats); a category whose name became one word on 2026-10-11 is about its
//!   old name's words as well as its new one's (FORMER_NAMES: "goma" is Rrota, once "Goma /
//!   disqe"). Of several, the one that takes the kind named, then the one whose name has most of
//!   the search's words, the most of them as typed, then the shortest name. Two still alike name
//!   neither ("shtëpi" is Vila and Shtëpi alike, once "Vila / shtëpi" and "Shtëpi / kopsht", but
//!   "shtëpi me qira" only the first).
//! - On an English page (i18n.rs), English's words as well: a category by its English name, about
//!   the last word of each part ("Flats"; a renamed one's old name, "Villas / houses"), in the singular
//!   or the plural; the kinds (rent, sale, buy); its joining words ("in", "for") skipped. "Flat for
//!   rent in Tirana" is the flats to rent in Tiranë; the words left stay searched, in the titles'
//!   Albanian.
//!
//! The towns and the zones are read by pluribus-services' places.rs (this file's until C9, which
//! moved them there for a new listing's zone, the posting form's and the JSON API's), with the words
//! themselves (`Word`, `words`).

use pluribus_services::places::{Word, read_town, read_zone, words};

use super::{filters::url, post::zoned};
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

/// The words English joins others with, folded, which name nothing on an English page, as
/// FILLERS do ("flats for rent in Tirana").
const ENGLISH_FILLERS: [&str; 12] = ["a", "an", "the", "in", "for", "to", "with", "of", "at", "on", "near", "and"];

/// Whether a folded word joins others and names nothing: an Albanian one, and on an English page
/// an English one too.
fn filler(word: &str) -> bool {
    FILLERS.contains(&word) || (crate::i18n::english() && ENGLISH_FILLERS.contains(&word))
}

/// The names some categories had until 2026-10-11, when each became one word (migration 0042), in
/// Albanian and in English, by their slugs: a search names a category by what its old name was
/// about as well, as searches named it then ("goma" is Rrota, as "Goma / disqe" was; "shtëpi me
/// qira" the houses, Vila, as "Vila / shtëpi" was; on an English page "tyres" Rrota too), and the
/// old name's words count as its name's. A category added since has none.
const FORMER_NAMES: [(&str, &str, &str); 32] = [
    ("shtepi-dhe-kopsht", "Shtëpi / kopsht", "Home / garden"),
    ("veshje-dhe-aksesore", "Veshje / aksesorë", "Clothes / accessories"),
    ("sport-dhe-hobi", "Sport / hobi", "Sport / hobbies"),
    ("kafshe-shtepiake", "Kafshë shtëpiake", "Pets"),
    ("makineri-dhe-biznes", "Makineri / biznes", "Machinery / business"),
    ("automjete-tregtare", "Automjete tregtare", "Commercial vehicles"),
    ("pjese-dhe-aksesore", "Pjesë / aksesorë", "Parts / accessories"),
    ("pjese-kembimi", "Pjesë këmbimi", "Spare parts"),
    ("goma-dhe-disqe", "Goma / disqe", "Tyres / rims"),
    ("audio-dhe-video", "Audio / video", "Audio / video"),
    ("gps-dhe-navigatore", "GPS / navigatorë", "GPS / sat navs"),
    ("styling-dhe-pajisje-sportive", "Styling / pajisje sportive", "Styling / tuning"),
    ("tapete-makine", "Tapete makine", "Car mats"),
    ("mbushes-per-makina-elektrike", "Mbushës për makina elektrike", "EV chargers"),
    ("vila-dhe-shtepi", "Vila / shtëpi", "Villas / houses"),
    ("konsola-lojerash", "Konsola lojërash", "Games consoles"),
    ("aparate-fotografike", "Aparate fotografike", "Cameras"),
    ("audio-dhe-kufje", "Audio / kufje", "Audio / headphones"),
    ("pune-turizem", "Turizëm / gastronomi", "Tourism / hospitality"),
    ("pune-call-center", "Call center", "Call centre"),
    ("transport-dhe-transferime", "Transport / transferime", "Transport / transfers"),
    ("foto-dhe-video", "Foto / video", "Photo / video"),
    ("ndertim-dhe-lyerje", "Ndërtim / lyerje", "Building / painting"),
    ("xhaketa-dhe-pallto", "Xhaketa / pallto", "Jackets / coats"),
    ("instrumente-muzikore", "Instrumente muzikore", "Musical instruments"),
    ("kamping-dhe-peshkim", "Kamping / peshkim", "Camping / fishing"),
    ("sporte-dimerore", "Sporte dimërore", "Winter sports"),
    ("aksesore-per-kafshe", "Aksesorë për kafshë", "Pet accessories"),
    ("makineri-ndertimi", "Makineri ndërtimi", "Construction machinery"),
    ("makineri-bujqesore", "Makineri bujqësore", "Farm machinery"),
    ("energji-dhe-gjeneratore", "Energji / gjeneratorë", "Energy / generators"),
    ("makineri-industriale", "Makineri industriale", "Industrial machinery"),
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
    /// The town, the zone's when they name a zone.
    pub town: Option<&'a City>,
    pub zone: Option<&'a Zone>,
    /// The words that name none of them, as typed.
    pub rest: Option<String>,
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
    let rest = match left.iter().all(|word| filler(&word.folded)) {
        true => None,
        false => Some(left.iter().map(|word| word.typed).collect::<Vec<_>>().join(" ")),
    };
    Some(Named { category: named_category, kind, town, zone, rest })
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

/// The kind of listing a folded word names, as search_kind has them (migration 0024).
fn kind(word: &str) -> Option<&'static str> {
    match word {
        "shitje" | "shitja" | "shitjes" | "shitet" | "shiten" | "shes" => Some("SELL"),
        "qira" | "qiraja" | "qirase" | "qera" | "qeraja" | "qerase" => Some("RENT"),
        "blej" | "blejme" | "blihet" => Some("BUY"),
        // On an English page, English's words for them too.
        "sale" | "sell" | "selling" if crate::i18n::english() => Some("SELL"),
        "rent" | "rental" | "rentals" | "let" | "lease" if crate::i18n::english() => Some("RENT"),
        "buy" | "buying" | "wanted" if crate::i18n::english() => Some("BUY"),
        _ => None,
    }
}

/// The category the first word that is no filler, kind or town names (see the module's note), its
/// words in the search marked used.
fn read_category<'a>(words: &mut [Word], categories: &'a Categories, kind: Option<&str>) -> Option<&'a Category> {
    let sought: Vec<usize> = (0..words.len())
        .filter(|at| !words[*at].used && !filler(&words[*at].folded) && self::kind(&words[*at].folded).is_none())
        .collect();
    let first = &words[*sought.first()?].folded;
    // On an English page, a category's English name says what it is about too.
    let english = |category: &Category| crate::i18n::english().then(|| crate::i18n::in_english(&category.name)).flatten();
    let alike = |a: &str, b: &str| same_word(a, b) || (crate::i18n::english() && same_english(a, b));
    // And a category renamed on 2026-10-11 by its old name too, in either language (FORMER_NAMES).
    let former = |category: &Category| FORMER_NAMES.iter().find(|(slug, ..)| *slug == category.slug).map(|(_, old, english)| (*old, *english));
    let name_of = |category: &Category| {
        let mut name = name_words(&category.name);
        name.extend(english(category).map(english_words).unwrap_or_default());
        if let Some((old, old_english)) = former(category) {
            name.extend(name_words(old));
            if crate::i18n::english() {
                name.extend(english_words(old_english));
            }
        }
        name
    };
    let mut candidates: Vec<(&Category, Fit)> = categories
        .iter()
        .filter_map(|category| {
            let about_it = about(&category.name).iter().any(|word| same_word(first, word))
                || english(category).is_some_and(|name| english_about(name).iter().any(|word| same_english(first, word)))
                || former(category).is_some_and(|(old, old_english)| {
                    about(old).iter().any(|word| same_word(first, word))
                        || (crate::i18n::english() && english_about(old_english).iter().any(|word| same_english(first, word)))
                });
            if !about_it {
                return None;
            }
            let name = name_of(category);
            let matched = sought.iter().filter(|at| name.iter().any(|word| alike(&words[**at].folded, word))).count();
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
    let name = name_of(category);
    for at in sought {
        if name.iter().any(|word| alike(&words[at].folded, word)) {
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

/// The words an English category name is about, folded: the last of each part a "/" or "and"
/// joins, English putting what a thing is last ("Villas / houses": villas, houses; "Commercial
/// vehicles": vehicles; "Pet accessories": accessories).
fn english_about(name: &str) -> Vec<String> {
    format::fold(&name.replace(" and ", " / "))
        .split('/')
        .filter_map(|part| part.split(|c: char| !c.is_alphanumeric()).rfind(|word| !word.is_empty()).map(str::to_string))
        .collect()
}

/// The words of an English category name, folded, less English's joining words.
fn english_words(name: &str) -> Vec<String> {
    format::fold(name)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty() && !ENGLISH_FILLERS.contains(word))
        .map(str::to_string)
        .collect()
}

/// Whether two folded English words are one, in the singular or the plural ("flat", "flats";
/// "lorry", "lorries"; "watch", "watches"), or one of the few people type for another
/// ("apartment" for a flat, "mobile" for a phone).
fn same_english(a: &str, b: &str) -> bool {
    fn singular(word: &str) -> String {
        let word = match word {
            "apartment" | "apartments" => "flat",
            "mobile" | "mobiles" | "cellphone" | "cellphones" => "phone",
            word => word,
        };
        if let Some(stem) = word.strip_suffix("ies").filter(|stem| stem.len() >= 2) {
            return format!("{stem}y");
        }
        if let Some(stem) = word.strip_suffix("es").filter(|stem| ["sh", "ch", "x", "ss"].iter().any(|end| stem.ends_with(end))) {
            return stem.to_string();
        }
        match word.strip_suffix('s').filter(|stem| stem.len() >= 2 && !stem.ends_with('s')) {
            Some(stem) => stem.to_string(),
            None => word.to_string(),
        }
    }
    a == b || singular(a) == singular(b)
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
    while before > 0 && filler(&words[before - 1].folded) {
        before -= 1;
        words[before].used = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_words_meet_in_the_singular_and_the_plural() {
        for (a, b) in [("flat", "flats"), ("apartment", "flats"), ("phone", "phones"), ("mobile", "phones"), ("lorry", "lorries"), ("watch", "watches"), ("tv", "tvs"), ("car", "cars")] {
            assert!(same_english(a, b), "{a} {b}");
        }
        for (a, b) in [("phone", "headphones"), ("glass", "glas"), ("car", "cart")] {
            assert!(!same_english(a, b), "{a} {b}");
        }
        assert_eq!(english_about("Villas / houses"), ["villas", "houses"]);
        assert_eq!(english_about("Commercial vehicles"), ["vehicles"]);
        assert_eq!(english_about("Audio and headphones"), ["audio", "headphones"]);
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
