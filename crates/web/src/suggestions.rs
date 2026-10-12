//! Suggestions under a search box as its words are typed (suggest_controller.js asks for them):
//! the words finished as listings use them, the categories the first of those finds most in, the
//! makes, models and brands the words name, and the towns the last word starts; or after words
//! joined to a place ("apartamente në"), the towns they have the most listings in. What they are is
//! pluribus-services' (suggestions.rs, `suggest`, this file's until C13, which the JSON API answers
//! by too); here each is a row in the page's language and a link to its search.

use askama::Template;
use axum::{
    extract::{RawQuery, State},
    response::Response,
};
use pluribus_services::suggestions::{self as suggested, Suggestions as Found};

use super::{Result, filters::url, render};
use crate::{
    AppState,
    i18n::{t, t1},
};

/// A row of suggestions: what it searches for, and after it, quieter, where; what it says in the
/// page's language, but the words searched.
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
    /// The words as typed, for each row to show bold where it meets them.
    typed: String,
}

/// A part of a row's words, bold where it meets one of the words typed, as the canvas has the rows
/// ("Volkswagen **Golf** Sportsvan"): lower case and accents aside, as search compares them
/// (format::fold: "shtepi" meets "Shtëpi"); a word of one letter meets nothing.
pub struct Part<'a> {
    pub text: &'a str,
    pub bold: bool,
}

impl Suggestions {
    fn marked<'a>(&self, label: &'a str) -> Vec<Part<'a>> {
        marked(label, &self.typed)
    }
}

fn marked<'a>(label: &'a str, typed: &str) -> Vec<Part<'a>> {
    let letters: Vec<(usize, char)> = label.char_indices().collect();
    // Each letter as search compares it; one that folds to more than one letter meets nothing.
    let folded: Vec<Option<char>> = letters
        .iter()
        .map(|(_, letter)| {
            let mut folded = crate::format::fold(&letter.to_string()).chars().collect::<Vec<_>>().into_iter();
            match (folded.next(), folded.next()) {
                (Some(one), None) => Some(one),
                _ => None,
            }
        })
        .collect();
    let mut bold = vec![false; letters.len()];
    for word in typed.split_whitespace() {
        let wanted: Vec<char> = crate::format::fold(word).chars().collect();
        if wanted.len() < 2 || wanted.len() > letters.len() {
            continue;
        }
        for start in 0..=letters.len() - wanted.len() {
            if wanted.iter().enumerate().all(|(at, letter)| folded[start + at] == Some(*letter)) {
                bold[start..start + wanted.len()].fill(true);
            }
        }
    }
    let mut parts = Vec::new();
    let mut from = 0;
    for at in 1..=letters.len() {
        if at == letters.len() || bold[at] != bold[from] {
            let end = letters.get(at).map_or(label.len(), |(index, _)| *index);
            parts.push(Part { text: &label[letters[from].0..end], bold: bold[from] });
            from = at;
        }
    }
    parts
}


/// /sugjerime?q=…: the suggestions for words being typed, as rows of links.
pub async fn suggestions(State(state): State<AppState>, RawQuery(query): RawQuery) -> Result<Response> {
    // As typed, a space at the end included, which says the last word is finished.
    let typed = suggested::typed(query.as_deref());
    let found = suggested::suggest(&state.db, &typed).await?;
    render(Suggestions { suggestions: rows(found), typed })
}

/// The rows of what the words suggest, in its order: the first search for words, then it in each
/// category it is found in most ("golf · në Makina"), the makes, models and brands the words name
/// ("Volkswagen Golf · në Makina"), the towns ("Njoftimet në Durrës", or the words there: "golf ·
/// në Durrës"), and the other searches for words.
fn rows(found: Found) -> Vec<Suggestion> {
    let mut rows = Vec::new();
    let mut words = found.words.into_iter();
    if let Some(first) = words.next() {
        rows.push(Suggestion { label: first.clone(), context: String::new(), url: words_url(&first) });
        for category in &found.categories {
            rows.push(Suggestion {
                label: first.clone(),
                context: t1("në {}", t(&category.name)),
                url: url(&format!("/c/{}", category.slug), [("q".to_string(), first.clone())]),
            });
        }
    }
    rows.extend(found.options.into_iter().map(|option| Suggestion {
        label: option.name,
        context: t1("në {}", t(&option.category_name)),
        url: url(&format!("/c/{}", option.category_slug), option.filters),
    }));
    // "apartamente në tir": apartamente, in Tirana; "në tir": all of Tirana's listings.
    let wanted = found.town_words;
    rows.extend(found.towns.iter().map(|town| match wanted.is_empty() {
        true => Suggestion {
            label: t1("Njoftimet në {}", &town.name),
            context: String::new(),
            url: url("/search", [("city".to_string(), town.slug.clone())]),
        },
        false => Suggestion {
            label: wanted.clone(),
            context: t1("në {}", &town.name),
            url: url("/search", [("q".to_string(), wanted.clone()), ("city".to_string(), town.slug.clone())]),
        },
    }));
    rows.extend(words.map(|words| Suggestion { url: words_url(&words), label: words, context: String::new() }));
    rows
}

/// A search for the words, in every category.
fn words_url(words: &str) -> String {
    url("/search", [("q".to_string(), words.to_string())])
}

#[cfg(test)]
mod tests {
    use super::marked;

    #[test]
    fn a_row_is_bold_where_it_meets_the_words_typed() {
        let shown = |label: &str, typed: &str| {
            marked(label, typed).iter().map(|part| if part.bold { format!("*{}*", part.text) } else { part.text.to_string() }).collect::<String>()
        };
        assert_eq!(shown("Volkswagen Golf Sportsvan", "golf"), "Volkswagen *Golf* Sportsvan");
        assert_eq!(shown("Volkswagen Golf", "gol"), "Volkswagen *Gol*f");
        assert_eq!(shown("golf", "golf ne durr"), "*golf*");
        assert_eq!(shown("Njoftimet në Durrës", "durr"), "Njoftimet në *Durr*ës");
        assert_eq!(shown("Shtëpi në Tiranë", "shtepi"), "*Shtëpi* në Tiranë");
        assert_eq!(shown("apartament", "a"), "apartament");
        assert_eq!(shown("", "golf"), "");
    }
}
