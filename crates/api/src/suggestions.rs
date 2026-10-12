//! What a search box suggests as its words are typed, for anyone (`GET /suggestions?q=`, since
//! C13): what the pages' rows under their search boxes are made from (pluribus-services'
//! suggestions.rs, `/sugjerime`), as data: the words finished, the categories the first of them is
//! found in most, the makes, models and brands the words name, and the towns. Never kept, as the
//! listings they come from come and go. The shapes it answers in are here, beside its handler.

use axum::{
    Json,
    extract::{RawQuery, State},
    http::{HeaderValue, header},
    response::IntoResponse,
};
use pluribus_services::{
    AppState,
    suggestions::{self as suggested, Suggestions},
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{ApiError, CategoryNameDto, openapi::Internal};

/// `GET /suggestions`: what the words typed suggest, in the pages' order: the first of `words`,
/// then it in each of `categories`, then `options`, `places`, and the rest of `words`; each a
/// search the app runs through `GET /listings`.
#[derive(Debug, Serialize, ToSchema)]
pub struct SuggestionsDto {
    /// Searches for words (`q`), three at most: the last word finished as listings use it, the
    /// most used first, after the words before it ("golf" for "gol"; one that names one of
    /// `places` left out, which its place searches), or for a whole word that seems misspelt the
    /// word it most likely misspells ("mercedes" for "mercedez"); the words as typed where the
    /// last is finished (a space after it) or isn't a word of letters ("golf 7"). None for words
    /// too short (two letters in all), or after words joined to a place.
    pub words: Vec<String>,
    /// The categories the first of `words` is found in most, two at most, each a search for it
    /// there (`q` and `category`), but one where a make, model or brand of that very name is among
    /// `options`, which finds it alone.
    pub categories: Vec<CategoryNameDto>,
    /// The makes, models and brands whose names have words starting with each word typed, the
    /// most used first, two at most, each in the category most of its listings are in.
    pub options: Vec<OptionSuggestionDto>,
    /// The towns the last word starts, a county's main town first, two at most; after words and the
    /// word that joins them to a place ("apartamente në"), the towns those words have the most
    /// listings in, three at most, and nothing else.
    pub places: Vec<PlaceSuggestionDto>,
}

/// A make, model or brand the words name (`SuggestionsDto::options`), a search by its filters in
/// the category most of its listings are in: "Volkswagen Golf" in Makina, `category=makina&
/// vehicle_make=VOLKSWAGEN&vehicle_model=VOLKSWAGEN_GOLF`.
#[derive(Debug, Serialize, ToSchema)]
pub struct OptionSuggestionDto {
    /// What it is called: a model after its make, "Volkswagen Golf"; a brand, "Samsung".
    #[schema(examples("Volkswagen Golf"))]
    pub name: String,
    pub category: CategoryNameDto,
    /// The filters that find it there, each a property's code and an option's, as `GET /listings`
    /// takes them: `vehicle_make=VOLKSWAGEN`, then `vehicle_model=VOLKSWAGEN_GOLF`.
    pub filters: Vec<FilterDto>,
}

/// A filter as a query string's pair: a property's code and one of its options' codes.
#[derive(Debug, Serialize, ToSchema)]
pub struct FilterDto {
    #[schema(examples("vehicle_make"))]
    pub code: String,
    #[schema(examples("VOLKSWAGEN"))]
    pub value: String,
}

/// A place the words lead to (`SuggestionsDto::places`): a search there (`city`, and `q` where
/// words go with it), all of its listings without them.
#[derive(Debug, Serialize, ToSchema)]
pub struct PlaceSuggestionDto {
    /// A town; or a zone of one (`zone`), which the pages don't suggest yet.
    pub kind: PlaceKind,
    /// The town's name: "Durrës".
    #[schema(examples("Durrës"))]
    pub city: String,
    /// A zone's name, for a zone, in the town (whose zones `GET /cities` has, with their slugs);
    /// none for a town.
    pub zone: Option<String>,
    /// What names the town in a filter: "durres" (`city=durres`).
    #[schema(examples("durres"))]
    pub slug: String,
    /// The words searched there: those typed before the town without the word that joins them to it
    /// ("golf" of "golf në durr"), or before that word after words joined to a place
    /// ("apartamente" of "apartamente në"); none for all of its listings ("durr").
    #[schema(examples("golf"))]
    pub q: Option<String>,
}

/// What a suggested place is: a town, or a zone of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    City,
    Zone,
}

impl From<Suggestions> for SuggestionsDto {
    fn from(found: Suggestions) -> Self {
        let there = (!found.town_words.is_empty()).then_some(found.town_words);
        Self {
            words: found.words,
            categories: found.categories.iter().map(CategoryNameDto::from).collect(),
            options: found
                .options
                .into_iter()
                .map(|option| OptionSuggestionDto {
                    category: CategoryNameDto::named(&option.category_slug, &option.category_name),
                    name: option.name,
                    filters: option.filters.into_iter().map(|(code, value)| FilterDto { code, value }).collect(),
                })
                .collect(),
            places: found
                .towns
                .into_iter()
                .map(|town| PlaceSuggestionDto { kind: PlaceKind::City, city: town.name, zone: None, slug: town.slug, q: there.clone() })
                .collect(),
        }
    }
}

/// `GET /suggestions?q=`: what the words typed suggest (`SuggestionsDto`), as the pages' rows under
/// their search boxes (`/sugjerime`), by the same (pluribus-services' `suggestions::suggest`), the
/// words read as the pages read them (`suggestions::typed`, the space after the last word kept);
/// none for words too short; never kept (`Cache-Control: no-store`).
#[utoipa::path(
    get,
    path = "/suggestions",
    tag = "browse",
    operation_id = "getSuggestions",
    summary = "What a search box suggests",
    description = "What a search box suggests as its words are typed, as the pages' suggestions under theirs: the words finished as listings use them, the categories the first of them is found in most, the makes, models and brands the words name, and the towns the last word starts; or, after words and a word that joins them to a place (\"apartamente në\"), the towns those words have the most listings in. Each a search the app runs through `GET /listings`; none for words too short.",
    params(
        ("q" = Option<String>, Query, description = "The words as typed (their first 100 characters), a space after the last saying that it is finished", example = "gol"),
    ),
    responses(
        (
            status = 200,
            description = "What the words suggest",
            body = SuggestionsDto,
            headers(("Cache-Control" = String, description = "`no-store`: never kept")),
        ),
        (status = 500, response = Internal),
    ),
)]
pub async fn suggestions(State(state): State<AppState>, RawQuery(query): RawQuery) -> Result<impl IntoResponse, ApiError> {
    let typed = suggested::typed(query.as_deref());
    let found = suggested::suggest(&state.db, &typed).await?;
    Ok(([(header::CACHE_CONTROL, HeaderValue::from_static("no-store"))], Json(SuggestionsDto::from(found))))
}
