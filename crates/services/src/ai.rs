//! Claude, Anthropic's AI, through its Messages API. Here it fills in the listing form from what a
//! seller wrote and the photos they added (web/ai_fill.rs). ANTHROPIC_API_KEY in .env turns it on;
//! without it the form has no AI. The model and the API's address are settings too, with their
//! defaults (config.rs: DEFAULT_ANTHROPIC_MODEL, Haiku, and DEFAULT_ANTHROPIC_URL).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use crate::config::Config;

/// An answer takes a few seconds; past this the seller fills the form in themselves.
const TIMEOUT: Duration = Duration::from_secs(30);

/// The API, with the key and the model to ask. Cheap to clone.
#[derive(Clone)]
pub struct Ai(Arc<Settings>);

struct Settings {
    http: reqwest::Client,
    key: String,
    url: String,
    model: String,
}

/// A tool the AI answers by calling, which is how its answer comes back as data: the tool's name,
/// what it is for, and the JSON schema of what it takes.
pub struct Tool<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub input_schema: Value,
}

impl Ai {
    /// The API at `url` ("https://api.anthropic.com", or the tests' stand-in), with this key and
    /// model.
    pub fn new(key: &str, url: &str, model: &str) -> Self {
        let http = reqwest::Client::builder().timeout(TIMEOUT).build().expect("an HTTP client");
        Self(Arc::new(Settings {
            http,
            key: key.to_string(),
            url: url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }))
    }

    /// From the settings (config.rs): ANTHROPIC_API_KEY, ANTHROPIC_BASE_URL and ANTHROPIC_MODEL.
    /// None without a key.
    pub fn from_config(config: &Config) -> Option<Self> {
        let key = config.anthropic_api_key.as_deref()?;
        Some(Self::new(key, &config.anthropic_base_url, &config.anthropic_model))
    }

    /// Asks the AI to call `tool` once about `text` and the JPEG `photos`, following `instructions`,
    /// and returns what it called the tool with. The instructions stay in Anthropic's prompt cache
    /// for a few minutes, so long lists in them cost a tenth on the calls that follow.
    pub async fn call_tool(&self, instructions: &str, tool: &Tool<'_>, photos: &[Vec<u8>], text: &str) -> anyhow::Result<Value> {
        let mut content: Vec<Value> = photos
            .iter()
            .map(|jpeg| json!({"type": "image", "source": {"type": "base64", "media_type": "image/jpeg", "data": STANDARD.encode(jpeg)}}))
            .collect();
        content.push(json!({"type": "text", "text": text}));
        let request = json!({
            "model": self.0.model,
            "max_tokens": 1024,
            "temperature": 0,
            "system": [{"type": "text", "text": instructions, "cache_control": {"type": "ephemeral"}}],
            "tools": [{"name": tool.name, "description": tool.description, "input_schema": tool.input_schema}],
            "tool_choice": {"type": "tool", "name": tool.name},
            "messages": [{"role": "user", "content": content}],
        });
        let started = Instant::now();
        let response = self
            .0
            .http
            .post(format!("{}/v1/messages", self.0.url))
            .header("x-api-key", &self.0.key)
            .header("anthropic-version", "2023-06-01")
            .json(&request)
            .send()
            .await
            .context("calling the AI")?;
        let status = response.status();
        let answer: Value = response.json().await.context("reading the AI's answer")?;
        if !status.is_success() {
            bail!("the AI answered {status}: {}", answer["error"]["message"].as_str().unwrap_or("no message"));
        }
        // What it cost, in tokens: the instructions read from the cache cost a tenth.
        let usage = &answer["usage"];
        tracing::info!(
            model = %self.0.model,
            ms = started.elapsed().as_millis() as u64,
            input_tokens = usage["input_tokens"].as_u64().unwrap_or(0),
            cached_tokens = usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
            output_tokens = usage["output_tokens"].as_u64().unwrap_or(0),
            "the AI answered"
        );
        answer["content"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|block| block["type"] == "tool_use" && block["name"] == tool.name)
            .map(|block| block["input"].clone())
            .context("the AI's answer doesn't call the tool")
    }
}
