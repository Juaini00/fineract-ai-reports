use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::assistant::llm_trace_repo::{LlmTrace, LlmTraceRepository};

use super::{EmbeddingResponse, LlmClient, LlmResponse};

#[derive(Debug, Clone)]
pub struct LlmTraceContext {
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub api_key_id: Uuid,
    pub graph_state: Option<String>,
    pub purpose: String,
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
        system: &str,
        user: &str,
        schema: Value,
    ) -> Result<LlmResponse<Value>> {
        let result = self.inner.structured_value(system, user, schema).await;
        if let Some(context) = &self.context {
            match &result {
                Ok(response) => {
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: context.purpose.clone(),
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
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: context.purpose.clone(),
                        provider: "unknown".into(),
                        model: "unknown".into(),
                        input_tokens: 0,
                        output_tokens: 0,
                        cost_usd: None,
                        latency_ms: 0,
                        status: "error".into(),
                        error_kind: Some(error.to_string()),
                    })
                    .await;
                }
            }
        }
        result
    }

    async fn embed(&self, text: &str) -> Result<EmbeddingResponse> {
        let result = self.inner.embed(text).await;
        if let Some(context) = &self.context {
            match &result {
                Ok(response) => {
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: format!("{}:embedding", context.purpose),
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
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        api_key_id: context.api_key_id,
                        graph_state: context.graph_state.clone(),
                        purpose: format!("{}:embedding", context.purpose),
                        provider: "unknown".into(),
                        model: "unknown".into(),
                        input_tokens: 0,
                        output_tokens: 0,
                        cost_usd: None,
                        latency_ms: 0,
                        status: "error".into(),
                        error_kind: Some(error.to_string()),
                    })
                    .await;
                }
            }
        }
        result
    }
}
