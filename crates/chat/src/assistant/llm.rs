use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};

pub mod rig_client;
pub mod traced_client;

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: i32,
    pub output_tokens: i32,
}

impl TokenUsage {
    pub fn total_tokens(&self) -> i32 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone)]
pub struct LlmResponse<T> {
    pub value: T,
    pub usage: TokenUsage,
    pub cost_usd: Option<f64>,
    pub provider: String,
    pub model: String,
    pub latency_ms: i32,
}

#[derive(Debug, Clone)]
pub struct EmbeddingResponse {
    pub vector: Vec<f32>,
    pub usage: TokenUsage,
    pub cost_usd: Option<f64>,
    pub provider: String,
    pub model: String,
    pub latency_ms: i32,
}

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn structured_value(
        &self,
        system: &str,
        user: &str,
        schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>>;

    async fn embed(&self, text: &str) -> Result<EmbeddingResponse>;
}

pub async fn structured<T>(
    client: &dyn LlmClient,
    system: &str,
    user: &str,
) -> Result<LlmResponse<T>>
where
    T: JsonSchema + DeserializeOwned + Serialize,
{
    let schema = schemars::schema_for!(T);
    let response = client
        .structured_value(system, user, serde_json::to_value(schema)?)
        .await?;
    let value = serde_json::from_value(response.value.clone()).or_else(|_| {
        response
            .value
            .as_object()
            .and_then(|object| {
                (object.len() == 1)
                    .then(|| object.values().next())
                    .flatten()
                    .cloned()
            })
            .map(serde_json::from_value)
            .transpose()?
            .ok_or_else(|| {
                <serde_json::Error as serde::de::Error>::custom(
                    "structured LLM response schema mismatch",
                )
            })
    })?;
    Ok(LlmResponse {
        value,
        usage: response.usage,
        cost_usd: response.cost_usd,
        provider: response.provider,
        model: response.model,
        latency_ms: response.latency_ms,
    })
}

pub type SharedLlmClient = Arc<dyn LlmClient>;
