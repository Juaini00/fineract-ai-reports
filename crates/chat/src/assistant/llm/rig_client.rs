use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use app_core::config::{EmbeddingConfig, LlmConfig, llm_pricing};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, TokenUsage};

pub struct RigLlmClient {
    http: reqwest::Client,
    llm: LlmConfig,
    embedding: Option<EmbeddingConfig>,
}

impl RigLlmClient {
    pub fn new(llm: &LlmConfig, embedding: Option<&EmbeddingConfig>) -> Result<Self> {
        let _ = std::mem::size_of::<rig_core::providers::openai::Client>();
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_millis(llm.timeout_ms))
                .build()
                .context("build LLM HTTP client")?,
            llm: llm.clone(),
            embedding: embedding.cloned(),
        })
    }

    pub fn is_enabled(&self) -> bool {
        !self.llm.api_key.trim().is_empty()
    }

    fn chat_url(&self) -> String {
        if !self.llm.chat_completions_url.trim().is_empty() {
            return self.llm.chat_completions_url.clone();
        }
        format!(
            "{}/chat/completions",
            self.llm.base_url.trim_end_matches('/')
        )
    }
}

#[async_trait]
impl LlmClient for RigLlmClient {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<LlmResponse<Value>> {
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for semantic routing");
        }
        let started = Instant::now();
        let body = |response_format: Value, system: &str| {
            json!({
                "model": self.llm.model,
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": user}
                ],
                "temperature": self.llm.temperature,
                "max_tokens": self.llm.max_output_tokens,
                "response_format": response_format
            })
        };
        // RigLlmClient owns the project LLM boundary; this OpenAI-compatible transport preserves custom provider URLs.
        let schema_format = json!({"type":"json_schema","json_schema":{"name":"assistant_structured_response","schema":schema,"strict":true}});
        let mut response = self
            .http
            .post(self.chat_url())
            .bearer_auth(&self.llm.api_key)
            .json(&body(schema_format, system))
            .send()
            .await
            .context("send structured LLM request")?;
        if response.status().as_u16() == 400 {
            response = self
                .http
                .post(self.chat_url())
                .bearer_auth(&self.llm.api_key)
                .json(&body(
                    json!({"type":"json_object"}),
                    &json_object_system(system, &schema),
                ))
                .send()
                .await
                .context("send structured LLM json_object fallback")?;
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            bail!("structured LLM request failed with status {status}: {body}");
        }
        let wire: ChatResponse = response
            .json()
            .await
            .context("parse structured LLM response")?;
        let content = wire
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .context("LLM response missing message content")?;
        let value = parse_structured_content(content)?;
        let usage = match wire.usage {
            Some(ChatUsage {
                prompt_tokens: Some(input_tokens),
                completion_tokens: Some(output_tokens),
            }) => TokenUsage::provider_reported(input_tokens, output_tokens),
            _ => TokenUsage::default(),
        };
        let cost_usd = usage.total_tokens().and_then(|_| {
            llm_pricing(&self.llm.provider, &self.llm.model).map(|price| {
                (usage.input_tokens.unwrap_or_default() as f64 * price.input_usd_per_1m
                    + usage.output_tokens.unwrap_or_default() as f64 * price.output_usd_per_1m)
                    / 1_000_000.0
            })
        });
        Ok(LlmResponse {
            value,
            usage,
            cost_usd,
            provider: self.llm.provider.clone(),
            model: self.llm.model.clone(),
            latency_ms: started.elapsed().as_millis() as i32,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse> {
        let Some(config) = &self.embedding else {
            bail!("embedding config is required")
        };
        if config.api_key.trim().is_empty() {
            bail!("EMBEDDING_API_KEY is required")
        }
        let started = Instant::now();
        let response = self
            .http
            .post(format!(
                "{}/embeddings",
                config.base_url.trim_end_matches('/')
            ))
            .bearer_auth(&config.api_key)
            .json(&json!({"model": config.model, "input": text}))
            .send()
            .await
            .context("send embedding request")?;
        if !response.status().is_success() {
            bail!("embedding request failed with status {}", response.status());
        }
        let wire: EmbeddingWire = response.json().await.context("parse embedding response")?;
        let vector = wire
            .data
            .first()
            .map(|item| item.embedding.clone())
            .unwrap_or_default();
        Ok(EmbeddingResponse {
            vector,
            usage: wire
                .usage
                .and_then(|usage| usage.prompt_tokens)
                .map(|input_tokens| TokenUsage::provider_reported(input_tokens, 0))
                .unwrap_or_default(),
            cost_usd: None,
            provider: config.provider.clone(),
            model: config.model.clone(),
            latency_ms: started.elapsed().as_millis() as i32,
        })
    }

    fn llm_metadata(&self) -> (String, String) {
        (self.llm.provider.clone(), self.llm.model.clone())
    }

    fn embedding_metadata(&self) -> (String, String) {
        self.embedding
            .as_ref()
            .map(|config| (config.provider.clone(), config.model.clone()))
            .unwrap_or_else(|| self.llm_metadata())
    }
}

/// System prompt for the `json_object` fallback, used when a provider rejects
/// `json_schema` (DeepSeek among them). Two things must be restated here that
/// `json_schema` would have enforced on the wire:
///
/// 1. The literal word "JSON" — DeepSeek 400s on `json_object` without it.
/// 2. The schema itself. `json_object` only constrains the reply to *some*
///    JSON object, so without the schema the model omits fields at random.
///    Omitted fields land on their serde defaults, which silently turns a real
///    request into whichever variant happens to be `#[default]`.
fn json_object_system(system: &str, schema: &Value) -> String {
    let schema = serde_json::to_string(schema).unwrap_or_default();
    format!(
        "{system}\n\nRespond with a single JSON object and nothing else. \
It must conform to this JSON Schema, including every required field:\n{schema}"
    )
}

fn parse_structured_content(content: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(content)
        .map_err(|error| anyhow!("malformed structured LLM JSON: {error}"))?;
    match &value {
        Value::Object(object) if object.len() == 1 => Ok(object.values().next().cloned().unwrap()),
        Value::Object(_) | Value::Array(_) => Ok(value),
        _ => bail!("malformed structured LLM JSON: expected object or array"),
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<i32>,
    completion_tokens: Option<i32>,
}

#[derive(Deserialize)]
struct EmbeddingWire {
    data: Vec<EmbeddingData>,
    usage: Option<EmbeddingUsage>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingUsage {
    prompt_tokens: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_object_system_carries_the_word_json_and_the_schema() {
        let schema = json!({"required": ["intent"]});
        let prompt = json_object_system("Route the user request.", &schema);
        // DeepSeek 400s on json_object without a literal "json" in the prompt.
        assert!(prompt.to_lowercase().contains("json"));
        // Without the schema the model omits fields and serde defaults them.
        assert!(prompt.contains("\"required\":[\"intent\"]"));
        assert!(prompt.starts_with("Route the user request."));
    }
}
