use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::assistant::llm_trace_repo::{LlmTrace, LlmTraceRepository};

use super::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse};

#[derive(Debug, Clone)]
pub struct LlmTraceContext {
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub api_key_id: Uuid,
    pub graph_state: Option<String>,
}

pub struct TracedLlmClient {
    inner: Arc<dyn LlmClient>,
    repo: LlmTraceRepository,
    context: Option<LlmTraceContext>,
}

impl TracedLlmClient {
    pub fn new(
        inner: Arc<dyn LlmClient>,
        repo: LlmTraceRepository,
        context: Option<LlmTraceContext>,
    ) -> Self {
        Self {
            inner,
            repo,
            context,
        }
    }

    async fn record(&self, trace: LlmTrace) {
        if let Err(error) = self.repo.record(&trace).await {
            tracing::warn!(%error, "failed to record LLM trace");
        }
    }
}

#[async_trait]
impl LlmClient for TracedLlmClient {
    async fn structured_value(
        &self,
        purpose: LlmPurpose,
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<LlmResponse<Value>> {
        let result = self
            .inner
            .structured_value(purpose, system, user, schema)
            .await;
        if let Some(context) = &self.context {
            match &result {
                Ok(response) => {
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: purpose.to_string(),
                        provider: response.provider.clone(),
                        model: response.model.clone(),
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        cost_usd: response.cost_usd,
                        latency_ms: response.latency_ms,
                        status: "ok".into(),
                        error_kind: None,
                    })
                    .await;
                }
                Err(error) => {
                    let (provider, model) = self.inner.llm_metadata();
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: purpose.to_string(),
                        provider,
                        model,
                        input_tokens: 0,
                        output_tokens: 0,
                        cost_usd: None,
                        latency_ms: 0,
                        status: classify_status(error),
                        error_kind: Some(error.to_string()),
                    })
                    .await;
                }
            }
        }
        result
    }

    async fn embed(&self, purpose: LlmPurpose, text: &str) -> Result<EmbeddingResponse> {
        let result = self.inner.embed(purpose, text).await;
        if let Some(context) = &self.context {
            match &result {
                Ok(response) => {
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: purpose.to_string(),
                        provider: response.provider.clone(),
                        model: response.model.clone(),
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        cost_usd: response.cost_usd,
                        latency_ms: response.latency_ms,
                        status: "ok".into(),
                        error_kind: None,
                    })
                    .await;
                }
                Err(error) => {
                    let (provider, model) = self.inner.embedding_metadata();
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: purpose.to_string(),
                        provider,
                        model,
                        input_tokens: 0,
                        output_tokens: 0,
                        cost_usd: None,
                        latency_ms: 0,
                        status: classify_status(error),
                        error_kind: Some(error.to_string()),
                    })
                    .await;
                }
            }
        }
        result
    }

    fn llm_metadata(&self) -> (String, String) {
        self.inner.llm_metadata()
    }

    fn embedding_metadata(&self) -> (String, String) {
        self.inner.embedding_metadata()
    }

    async fn record_malformed(&self, purpose: LlmPurpose, error: &str) {
        let Some(context) = &self.context else {
            return;
        };
        let (provider, model) = self.inner.llm_metadata();
        self.record(LlmTrace {
            job_id: context.job_id,
            session_id: context.session_id,
            api_key_id: context.api_key_id,
            graph_state: context.graph_state.clone(),
            purpose: purpose.to_string(),
            provider,
            model,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: None,
            latency_ms: 0,
            status: "malformed".into(),
            error_kind: Some(error.into()),
        })
        .await;
    }
}

fn classify_status(error: &anyhow::Error) -> String {
    let message = error.to_string().to_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        "timeout".into()
    } else if message.contains("malformed")
        || message.contains("schema mismatch")
        || message.contains("parse structured")
    {
        "malformed".into()
    } else {
        "error".into()
    }
}
