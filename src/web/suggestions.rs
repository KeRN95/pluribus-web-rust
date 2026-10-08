//! Suggestions under a search box as its words are typed (suggest_controller.js asks for them):
//! the words finished as listings use them, the categories the first of those finds most in, the
//! makes, models and brands the words name, and the towns the last word starts. Each is a link to
//! its search.

use askama::Template;
use axum::{
    extract::{RawQuery, State},
    response::Response,
};
use sqlx::PgPool;

use super::{
    Result,
    filters::{SEARCH_CHARS, url},
    render,
};
use crate::{
    AppState,
    db::{self, City, ListingFilter},
    format,
};

/// The most finished searches offered, the categories the first finds most in, the makes, models
/// and brands, and the towns.
const COMPLETIONS: i64 = 3;
const CATEGORIES: i64 = 2;
const OPTIONS: i64 = 2;
const TOWNS: usize = 2;
/// The towns offered after words joined to a place ("apartamente në").
const PLACE_TOWNS: i64 = 3;

/// The words that join what is searched to a place, folded ("në" and "ne" alike): "apartamente në
/// Tiranë", "makina te Durrësi", "shtëpi afër Vlorës". A town's row leaves the last of them out of
/// its search, since its filter says where; so does a search that names the town (named.rs).
pub const PLACE_WORDS: [&str; 8] = ["ne", "te", "tek", "nga", "per", "prane", "afer", "rreth"];

/// A row of suggestions: what it searches for, and after it, quieter, where.
pub struct Suggestion {
    pub label: String,
    /// "në Makina", or empty.
    pub context: String,
    pub url: String,
}

#[derive(Template)]
#[template(path = "suggestions.html")]
struct Suggestions {
    suggestions: Vec<Suggestion>,
}

/// /sugjerime?q=…: the suggestions for words being typed, as rows of links.
pub async fn suggestions(State(state): State<AppState>, RawQuery(query): RawQuery) -> Result<Response> {
    // As typed, a space at the end included, which says the last word is finished.
    let typed: String = form_urlencoded::parse(query.unwrap_or_default().as_bytes())
        .find(|(name, _)| name == "q")
        .map(|(_, words)| words.trim_start().chars().take(SEARCH_CHARS).collect())
        .unwrap_or_default();
    render(Suggestions { suggestions: suggest(&state.db, &typed).await? })
}

async fn suggest(db: &PgPool, typed: &str) -> Result<Vec<Suggestion>> {
    let words = typed.trim_end();
    // Every word folded, for the names of makes, models and brands: "Mercedes-Benz" has two.
    let starts: Vec<String> = format::fold(words)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|start| !start.is_empty())
        .map(str::to_string)
        .collect();
    if starts.iter().map(String::len).sum::<usize>() < 2 {
        return Ok(Vec::new());
    }
    let (before, last) = match words.rsplit_once(char::is_whitespace) {
        Some((before, last)) => (before.trim_end(), last),
        None => ("", words),
    };
    // Words, then the word that joins them to a place ("apartamente në", typed "ne" as often): the
    // towns those words have the most listings in, rather than words starting "ne". Words nothing
    // is found for go on as any others.
    if !before.is_empty() && PLACE_WORDS.contains(&format::fold(last).as_str()) {
        let places = places_for(db, before).await?;
        if !places.is_empty() {
            return Ok(places);
        }
    }
    // The last word is being typed, unless a space follows it or it isn't a word of letters
    // ("golf 7", "2+1"): then the words are taken as they are.
    let start = format::fold(last);
    let typing = !typed.ends_with(char::is_whitespace)
        && start.len() >= 2
        && start.chars().all(|c| c.is_ascii_lowercase());
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
    completions.retain(|completion| {
        let word = completion.rsplit(' ').next().unwrap_or(completion);
        !towns.iter().any(|town| town.slug.split('-').any(|part| part == word))
    });

    let mut suggestions = Vec::new();
    let mut completions = completions.into_iter();
    if let Some(first) = completions.next() {
        let search = db::text_search(db, Some(&first)).await?;
        let found_in = db::listing_categories(db, &ListingFilter { search, ..Default::default() }, CATEGORIES).await?;
        suggestions.push(Suggestion { label: first.clone(), context: String::new(), url: words_url(&first) });
        for category in found_in.into_iter().filter_map(|(id, _)| categories.by_id(id)) {
            // A brand or make of that name has a row of its own there, which finds it alone:
            // "Samsung në Telefona" rather than "samsung" searched in it too.
            let named = |option: &db::NamedOption| {
                option.category_slug == category.slug && format::fold(&option.name) == format::fold(&first)
            };
            if options.iter().any(named) {
                continue;
            }
            suggestions.push(Suggestion {
                label: first.clone(),
                context: format!("në {}", category.name),
                url: url(&format!("/c/{}", category.slug), [("q".to_string(), first.clone())]),
            });
        }
    }
    suggestions.extend(options.into_iter().map(|option| Suggestion {
        label: option.name,
        context: format!("në {}", option.category_name),
        url: url(&format!("/c/{}", option.category_slug), option.filters),
    }));
    // "apartamente në tir": apartamente, in Tirana; "në tir": all of Tirana's listings.
    let wanted = without_place_word(before);
    suggestions.extend(towns.into_iter().map(|town| match wanted.is_empty() {
        true => Suggestion {
            label: format!("Njoftimet në {}", town.name),
            context: String::new(),
            url: url("/search", [("city".to_string(), town.slug.clone())]),
        },
        false => Suggestion {
            label: wanted.to_string(),
            context: format!("në {}", town.name),
            url: url("/search", [("q".to_string(), wanted.to_string()), ("city".to_string(), town.slug.clone())]),
        },
    }));
    suggestions.extend(completions.map(|completion| Suggestion {
        url: words_url(&completion),
        label: completion,
        context: String::new(),
    }));
    Ok(suggestions)
}

/// The towns where `words` have the most listings, each a row that searches them there:
/// "apartamente · në Tiranë".
async fn places_for(db: &PgPool, words: &str) -> Result<Vec<Suggestion>> {
    let search = db::text_search(db, Some(words)).await?;
    let filter = ListingFilter { search, ..Default::default() };
    let (found_in, cities) = tokio::try_join!(db::listing_towns(db, &filter, PLACE_TOWNS), db::cities(db))?;
    Ok(found_in
        .into_iter()
        .filter_map(|(id, _)| cities.iter().find(|city| city.id == id))
        .map(|town| Suggestion {
            label: words.to_string(),
            context: format!("në {}", town.name),
            url: url("/search", [("q".to_string(), words.to_string()), ("city".to_string(), town.slug.clone())]),
        })
        .collect())
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

/// A search for the words, in every category.
fn words_url(words: &str) -> String {
    url("/search", [("q".to_string(), words.to_string())])
}
