use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use app_core::config::{EmbeddingConfig, LlmConfig, llm_pricing};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{EmbeddingResponse, LlmClient, LlmResponse, TokenUsage};

pub struct RigLlmClient {
    http: reqwest::Client,
    llm: LlmConfig,
    embedding: Option<EmbeddingConfig>,
}

impl RigLlmClient {
    pub fn new(llm: &LlmConfig, embedding: Option<&EmbeddingConfig>) -> Result<Self> {
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
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<LlmResponse<Value>> {
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for semantic routing");
        }
        let started = Instant::now();
        let body = |response_format: Value| {
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
        // rig-core stays in the dependency graph for future native Tool/agent wiring; this transport is deliberately OpenAI-compatible so custom URLs work provider-agnostically.
        let schema_format = json!({"type":"json_schema","json_schema":{"name":"assistant_structured_response","schema":schema,"strict":true}});
        let mut response = self
            .http
            .post(self.chat_url())
            .bearer_auth(&self.llm.api_key)
            .json(&body(schema_format))
            .send()
            .await
            .context("send structured LLM request")?;
        if response.status().as_u16() == 400 {
            response = self
                .http
                .post(self.chat_url())
                .bearer_auth(&self.llm.api_key)
                .json(&body(json!({"type":"json_object"})))
                .send()
                .await
                .context("send structured LLM json_object fallback")?;
        }
        if !response.status().is_success() {
            bail!(
                "structured LLM request failed with status {}",
                response.status()
            );
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
        let value = serde_json::from_str(content).context("parse structured LLM JSON")?;
        let usage = TokenUsage {
            input_tokens: wire
                .usage
                .as_ref()
                .and_then(|u| u.prompt_tokens)
                .unwrap_or(0),
            output_tokens: wire
                .usage
                .as_ref()
                .and_then(|u| u.completion_tokens)
                .unwrap_or(0),
        };
        let cost_usd = llm_pricing(&self.llm.provider, &self.llm.model).map(|price| {
            (usage.input_tokens as f64 * price.input_usd_per_1m
                + usage.output_tokens as f64 * price.output_usd_per_1m)
                / 1_000_000.0
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

    async fn embed(&self, text: &str) -> Result<EmbeddingResponse> {
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
            usage: TokenUsage {
                input_tokens: wire.usage.and_then(|u| u.prompt_tokens).unwrap_or(0),
                output_tokens: 0,
            },
            cost_usd: None,
            provider: config.provider.clone(),
            model: config.model.clone(),
            latency_ms: started.elapsed().as_millis() as i32,
        })
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
