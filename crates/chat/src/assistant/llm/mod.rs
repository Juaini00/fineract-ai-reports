use std::sync::Arc;

use std::{collections::VecDeque, sync::Mutex};

use anyhow::{Result, bail};
use app_core::config::llm_pricing;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub mod planner_client;
pub mod rig_client;
pub mod router;
pub mod traced_client;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LlmPurpose {
    RouteIntent,
    RouteEmbedding,
    ClarificationEmbedding,
    ClarificationResolve,
    EvidenceRetrieval,
    ResponseBuild,
    Test,
}

impl std::fmt::Display for LlmPurpose {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::RouteIntent => "route_intent",
            Self::RouteEmbedding => "route_embedding",
            Self::ClarificationEmbedding => "clarification_embedding",
            Self::ClarificationResolve => "clarification_resolve",
            Self::EvidenceRetrieval => "evidence_retrieval",
            Self::ResponseBuild => "response_build",
            Self::Test => "test",
        };
        f.write_str(value)
    }
}

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
        purpose: LlmPurpose,
        system: &str,
        user: &str,
        schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>>;

    async fn embed(&self, purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse>;

    fn llm_metadata(&self) -> (String, String) {
        ("unknown".into(), "unknown".into())
    }

    fn embedding_metadata(&self) -> (String, String) {
        self.llm_metadata()
    }

    async fn record_malformed(&self, _purpose: LlmPurpose, _error: &str) {}
}

/// Runs a structured LLM call and decodes the response as `T`.
///
/// `schema` lets the caller supply a pre-built `schemars::Schema` (e.g. one
/// trimmed or shared across call sites); when `None` it is derived from `T`
/// via `schemars::schema_for!`. The schema is always sent to the provider's
/// structured-output mode via `LlmClient::structured_value`.
pub async fn structured<T>(
    client: &dyn LlmClient,
    purpose: LlmPurpose,
    system: &str,
    user: &str,
    schema: Option<schemars::Schema>,
) -> Result<LlmResponse<T>>
where
    T: JsonSchema + DeserializeOwned + Serialize,
{
    let schema = schema.unwrap_or_else(|| schemars::schema_for!(T));
    let response = client
        .structured_value(purpose, system, user, serde_json::to_value(schema)?)
        .await?;
    let value = match decode_structured::<T>(&response.value) {
        Ok(value) => value,
        Err(error) => {
            client.record_malformed(purpose, &error.to_string()).await;
            return Err(error);
        }
    };
    Ok(LlmResponse {
        value,
        usage: response.usage,
        cost_usd: response.cost_usd,
        provider: response.provider,
        model: response.model,
        latency_ms: response.latency_ms,
    })
}

/// Decodes a structured LLM response into `T`, preferring the real
/// field-level decode error (via `serde_path_to_error`) over a generic
/// message. Falls back to unwrapping a single-key wrapper object, which some
/// providers use when they can't return a bare object/array.
fn decode_structured<T: DeserializeOwned>(value: &serde_json::Value) -> Result<T> {
    let primary_error = match serde_path_to_error::deserialize(value.clone()) {
        Ok(value) => return Ok(value),
        Err(error) => error,
    };
    let unwrapped = value.as_object().and_then(|object| {
        (object.len() == 1)
            .then(|| object.values().next())
            .flatten()
            .cloned()
    });
    match unwrapped.and_then(|inner| serde_path_to_error::deserialize(inner).ok()) {
        Some(value) => Ok(value),
        None => bail!("structured LLM response schema mismatch: {primary_error}"),
    }
}

pub type SharedLlmClient = Arc<dyn LlmClient>;

pub struct FakeLlmClient {
    structured: Mutex<VecDeque<Result<serde_json::Value, String>>>,
    embeddings: Mutex<VecDeque<Result<Vec<f32>, String>>>,
    provider: String,
    model: String,
}

impl Default for FakeLlmClient {
    fn default() -> Self {
        Self::new("fake", "fake")
    }
}

impl FakeLlmClient {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            structured: Mutex::new(VecDeque::new()),
            embeddings: Mutex::new(VecDeque::new()),
            provider: provider.into(),
            model: model.into(),
        }
    }

    pub fn push_structured(&self, value: serde_json::Value) {
        self.structured.lock().unwrap().push_back(Ok(value));
    }

    pub fn push_structured_error(&self, error: impl Into<String>) {
        self.structured.lock().unwrap().push_back(Err(error.into()));
    }

    pub fn push_embedding(&self, vector: Vec<f32>) {
        self.embeddings.lock().unwrap().push_back(Ok(vector));
    }

    pub fn push_embedding_error(&self, error: impl Into<String>) {
        self.embeddings.lock().unwrap().push_back(Err(error.into()));
    }
}

#[async_trait]
impl LlmClient for FakeLlmClient {
    async fn structured_value(
        &self,
        _purpose: LlmPurpose,
        _system: &str,
        _user: &str,
        _schema: serde_json::Value,
    ) -> Result<LlmResponse<serde_json::Value>> {
        let next = self
            .structured
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("fake structured queue empty".into()));
        let value = match next {
            Ok(value) => value,
            Err(error) => bail!(error),
        };
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
        };
        let cost_usd = llm_pricing(&self.provider, &self.model).map(|price| {
            (usage.input_tokens as f64 * price.input_usd_per_1m
                + usage.output_tokens as f64 * price.output_usd_per_1m)
                / 1_000_000.0
        });
        Ok(LlmResponse {
            value,
            usage,
            cost_usd,
            provider: self.provider.clone(),
            model: self.model.clone(),
            latency_ms: 1,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, _text: &str) -> Result<EmbeddingResponse> {
        let next = self
            .embeddings
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("fake embedding queue empty".into()));
        let vector = match next {
            Ok(vector) => vector,
            Err(error) => bail!(error),
        };
        Ok(EmbeddingResponse {
            vector,
            usage: TokenUsage {
                input_tokens: 7,
                output_tokens: 0,
            },
            cost_usd: None,
            provider: self.provider.clone(),
            model: self.model.clone(),
            latency_ms: 1,
        })
    }

    fn llm_metadata(&self) -> (String, String) {
        (self.provider.clone(), self.model.clone())
    }
}
