//! What a search box suggests as its words are typed: the words finished as listings use them, the
//! categories the first of those is found in most, the makes, models and brands the words name,
//! and the towns the last word starts; or, after words and the word that joins them to a place
//! ("apartamente në"), the towns those words have the most listings in. The pages' rows under
//! their search boxes (pluribus-web's suggestions.rs, `/sugjerime`, which words each row and links
//! it to its search) and the JSON API's (pluribus-api's suggestions.rs, `GET /suggestions`) are
//! made from it, pluribus-web's suggestions.rs's own until C13.

use pluribus_core::format;
use pluribus_db::{self as db, Category, City, ListingFilter, NamedOption};
use sqlx::PgPool;

use crate::{filters::SEARCH_CHARS, places::PLACE_WORDS};

/// The most finished searches offered, the categories the first finds most in, the makes, models
/// and brands, and the towns.
const COMPLETIONS: i64 = 3;
const CATEGORIES: i64 = 2;
const OPTIONS: i64 = 2;
const TOWNS: usize = 2;
/// The towns offered after words joined to a place ("apartamente në": one of PLACE_WORDS, which a
/// town's row leaves the last of out of its search, since its filter says where).
const PLACE_TOWNS: i64 = 3;

/// What words being typed suggest (`suggest`), in the order the pages' rows go: the first of
/// `words`, then it searched in each of `categories`, then `options`, `towns`, and the rest of
/// `words`.
#[derive(Debug, Default)]
pub struct Suggestions {
    /// Searches for words: the last word finished as listings use it, the most used first, after
    /// the words before it (COMPLETIONS at most; one that names one of `towns` left out, whose row
    /// searches it), or for a whole word that seems misspelt the word it most likely misspells;
    /// the words as typed where the last is finished (a space after it) or isn't a word of letters
    /// ("golf 7"). None for words too short, or after words joined to a place.
    pub words: Vec<String>,
    /// The categories the first of `words` is found in most (CATEGORIES at most), but one where a
    /// make, model or brand of that very name is among `options`, whose row finds it alone.
    pub categories: Vec<Category>,
    /// The makes, models and brands whose names have words starting with each word typed, the
    /// most used first (OPTIONS at most), each in the category most of its listings are in, with
    /// the filters that find it there.
    pub options: Vec<NamedOption>,
    /// The towns the last word starts, a county's main town first (TOWNS at most); after words
    /// joined to a place, the towns those words have the most listings in (PLACE_TOWNS at most).
    pub towns: Vec<City>,
    /// What each of `towns` searches there: the words before it without the word that joins them
    /// to it ("apartamente" of "apartamente në tir"), or after words joined to a place, those
    /// words; empty for all of the town's listings.
    pub town_words: String,
}

/// The words of a query string's `q` as typed, for its suggestions: its first SEARCH_CHARS
/// characters, the spaces before them left out but not those after, which say that the last word
/// is finished.
pub fn typed(query: Option<&str>) -> String {
    form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(name, _)| name == "q")
        .map(|(_, words)| words.trim_start().chars().take(SEARCH_CHARS).collect())
        .unwrap_or_default()
}

/// What `typed`, the words as typed (`typed`), suggests (`Suggestions`).
pub async fn suggest(db: &PgPool, typed: &str) -> sqlx::Result<Suggestions> {
    let words = typed.trim_end();
    // Every word folded, for the names of makes, models and brands: "Mercedes-Benz" has two.
    let starts: Vec<String> = format::fold(words)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|start| !start.is_empty())
        .map(str::to_string)
        .collect();
    if starts.iter().map(String::len).sum::<usize>() < 2 {
        return Ok(Suggestions::default());
    }
    let (before, last) = match words.rsplit_once(char::is_whitespace) {
        Some((before, last)) => (before.trim_end(), last),
        None => ("", words),
    };
    // Words, then the word that joins them to a place ("apartamente në", typed "ne" as often): the
    // towns those words have the most listings in, rather than words starting "ne". Words nothing
    // is found for go on as any others.
    if !before.is_empty() && PLACE_WORDS.contains(&format::fold(last).as_str()) {
        let towns = towns_for(db, before).await?;
        if !towns.is_empty() {
            return Ok(Suggestions { towns, town_words: before.to_string(), ..Suggestions::default() });
        }
    }
    // The last word is being typed, unless a space follows it or it isn't a word of letters
    // ("golf 7", "2+1"): then the words are taken as they are.
    let start = format::fold(last);
    let typing = !typed.ends_with(char::is_whitespace) && start.len() >= 2 && start.chars().all(|c| c.is_ascii_lowercase());
    let with_before = |word: &str| match before.is_empty() {
        true => word.to_string(),
        false => format!("{before} {word}"),
    };

    let (mut completions, options, cities, categories) = tokio::try_join!(
        async {
            if !typing {
                return Ok(vec![words.to_string()]);
            }
            let finished = db::words_starting_with(db, &start, COMPLETIONS).await?;
            if !finished.is_empty() || start.len() < 4 {
                return Ok(finished.iter().map(|word| with_before(word)).collect());
            }
            // A whole word that seems misspelt: the word it most likely misspells.
            Ok(db::spelling(db, &start).await?.map(|word| with_before(&word)).into_iter().collect())
        },
        db::named_options(db, &starts, OPTIONS),
        db::cities(db),
        db::categories(db),
    )?;

    // The towns the last word starts, a county's main town first: "kor" is Korçë before Kolonjë.
    let mut towns: Vec<&City> = match typing {
        true => cities.iter().filter(|city| city.slug.split('-').any(|part| part.starts_with(&start))).collect(),
        false => Vec::new(),
    };
    towns.sort_by_key(|city| (format::slug(&city.county) != city.slug, city.name.clone()));
    towns.truncate(TOWNS);
    // A town's name finished as a word searches what the town's row does.
    completions.retain(|completion: &String| {
        let word = completion.rsplit(' ').next().unwrap_or(completion);
        !towns.iter().any(|town| town.slug.split('-').any(|part| part == word))
    });

    let mut found = Vec::new();
    if let Some(first) = completions.first() {
        let search = db::text_search(db, Some(first)).await?;
        let found_in = db::listing_categories(db, &ListingFilter { search, ..Default::default() }, CATEGORIES).await?;
        for category in found_in.into_iter().filter_map(|(id, _)| categories.by_id(id)) {
            // A brand or make of that name has a row of its own there, which finds it alone:
            // "Samsung në Telefona" rather than "samsung" searched in it too.
            let named = |option: &NamedOption| option.category_slug == category.slug && format::fold(&option.name) == format::fold(first);
            if !options.iter().any(named) {
                found.push(category.clone());
            }
        }
    }
    Ok(Suggestions {
        words: completions,
        categories: found,
        options,
        towns: towns.into_iter().cloned().collect(),
        // "apartamente në tir": apartamente, in Tirana; "në tir": all of Tirana's listings.
        town_words: without_place_word(before).to_string(),
    })
}

/// The towns where `words` have the most listings, PLACE_TOWNS at most, the most first.
async fn towns_for(db: &PgPool, words: &str) -> sqlx::Result<Vec<City>> {
    let search = db::text_search(db, Some(words)).await?;
    let filter = ListingFilter { search, ..Default::default() };
    let (found_in, cities) = tokio::try_join!(db::listing_towns(db, &filter, PLACE_TOWNS), db::cities(db))?;
    Ok(found_in.into_iter().filter_map(|(id, _)| cities.iter().find(|city| city.id == id).cloned()).collect())
}

/// The words before a town, without the one that joins them to it (PLACE_WORDS): "apartamente në"
/// and "apartamente ne" are "apartamente", and "në" alone is nothing.
fn without_place_word(before: &str) -> &str {
    let (rest, last) = before.rsplit_once(char::is_whitespace).unwrap_or(("", before));
    match PLACE_WORDS.contains(&format::fold(last).as_str()) {
        true => rest.trim_end(),
        false => before,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_typed_keep_the_space_after_them_and_lose_the_one_before() {
        assert_eq!(typed(Some("q=%20golf%20")), "golf ");
        assert_eq!(typed(Some("lang=en&q=apartamente+n%C3%AB")), "apartamente në");
        assert_eq!(typed(Some(&format!("q={}", "a".repeat(150)))).len(), SEARCH_CHARS);
        assert_eq!(typed(None), "");
        assert_eq!(without_place_word("apartamente në"), "apartamente");
        assert_eq!(without_place_word("apartamente ne"), "apartamente");
        assert_eq!(without_place_word("në"), "");
        assert_eq!(without_place_word("golf 7"), "golf 7");
    }
}
