use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::audit::llm_trace_repository::{
    LlmTrace, LlmTraceErrorCode, LlmTraceRepository, LlmTraceUsageStatus,
};

use super::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse};

#[derive(Debug, Clone)]
pub struct LlmTraceContext {
    pub job_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub user_id: Uuid,
    pub legacy_api_key_id: Option<Uuid>,
    pub graph_state: Option<String>,
    pub correlation_id: Option<Uuid>,
    pub context_contract_version: Option<i16>,
    pub catalog_version_id: Option<Uuid>,
    pub index_version_id: Option<Uuid>,
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
                        user_id: context.user_id,
                        legacy_api_key_id: context.legacy_api_key_id,
                        graph_state: context.graph_state.clone(),
                        correlation_id: context.correlation_id,
                        context_contract_version: context.context_contract_version,
                        catalog_version_id: context.catalog_version_id,
                        index_version_id: context.index_version_id,
                        purpose: purpose.to_string(),
                        provider: response.provider.clone(),
                        model: response.model.clone(),
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        usage_status: usage_status(&response.usage),
                        cost_usd: response.cost_usd,
                        price_version: response.cost_usd.map(|_| "static_config_v1".into()),
                        cost_currency: response.cost_usd.map(|_| "USD".into()),
                        latency_ms: response.latency_ms,
                        status: "ok".into(),
                        error_kind: None,
                        error_code: None,
                    })
                    .await;
                }
                Err(error) => {
                    let (provider, model) = self.inner.llm_metadata();
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        user_id: context.user_id,
                        legacy_api_key_id: context.legacy_api_key_id,
                        graph_state: context.graph_state.clone(),
                        correlation_id: context.correlation_id,
                        context_contract_version: context.context_contract_version,
                        catalog_version_id: context.catalog_version_id,
                        index_version_id: context.index_version_id,
                        purpose: purpose.to_string(),
                        provider,
                        model,
                        input_tokens: None,
                        output_tokens: None,
                        usage_status: LlmTraceUsageStatus::Unavailable,
                        cost_usd: None,
                        price_version: None,
                        cost_currency: None,
                        latency_ms: 0,
                        status: classify_status(error),
                        error_kind: Some(error.to_string()),
                        error_code: normalize_error_code(error),
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
                        user_id: context.user_id,
                        legacy_api_key_id: context.legacy_api_key_id,
                        graph_state: context.graph_state.clone(),
                        correlation_id: context.correlation_id,
                        context_contract_version: context.context_contract_version,
                        catalog_version_id: context.catalog_version_id,
                        index_version_id: context.index_version_id,
                        purpose: purpose.to_string(),
                        provider: response.provider.clone(),
                        model: response.model.clone(),
                        input_tokens: response.usage.input_tokens,
                        output_tokens: response.usage.output_tokens,
                        usage_status: usage_status(&response.usage),
                        cost_usd: response.cost_usd,
                        price_version: response.cost_usd.map(|_| "static_config_v1".into()),
                        cost_currency: response.cost_usd.map(|_| "USD".into()),
                        latency_ms: response.latency_ms,
                        status: "ok".into(),
                        error_kind: None,
                        error_code: None,
                    })
                    .await;
                }
                Err(error) => {
                    let (provider, model) = self.inner.embedding_metadata();
                    self.record(LlmTrace {
                        job_id: context.job_id,
                        session_id: context.session_id,
                        user_id: context.user_id,
                        legacy_api_key_id: context.legacy_api_key_id,
                        graph_state: context.graph_state.clone(),
                        correlation_id: context.correlation_id,
                        context_contract_version: context.context_contract_version,
                        catalog_version_id: context.catalog_version_id,
                        index_version_id: context.index_version_id,
                        purpose: purpose.to_string(),
                        provider,
                        model,
                        input_tokens: None,
                        output_tokens: None,
                        usage_status: LlmTraceUsageStatus::Unavailable,
                        cost_usd: None,
                        price_version: None,
                        cost_currency: None,
                        latency_ms: 0,
                        status: classify_status(error),
                        error_kind: Some(error.to_string()),
                        error_code: normalize_error_code(error),
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
            user_id: context.user_id,
            legacy_api_key_id: context.legacy_api_key_id,
            graph_state: context.graph_state.clone(),
            correlation_id: context.correlation_id,
            context_contract_version: context.context_contract_version,
            catalog_version_id: context.catalog_version_id,
            index_version_id: context.index_version_id,
            purpose: purpose.to_string(),
            provider,
            model,
            input_tokens: None,
            output_tokens: None,
            usage_status: LlmTraceUsageStatus::Unavailable,
            cost_usd: None,
            price_version: None,
            cost_currency: None,
            latency_ms: 0,
            status: "malformed".into(),
            error_kind: Some(error.into()),
            error_code: Some(LlmTraceErrorCode::ProviderMalformed),
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

fn usage_status(usage: &super::TokenUsage) -> LlmTraceUsageStatus {
    if usage.is_provider_reported() {
        LlmTraceUsageStatus::ProviderReported
    } else {
        LlmTraceUsageStatus::Unavailable
    }
}

fn normalize_error_code(error: &anyhow::Error) -> Option<LlmTraceErrorCode> {
    match classify_status(error).as_str() {
        "timeout" => Some(LlmTraceErrorCode::ProviderTimeout),
        "malformed" => Some(LlmTraceErrorCode::ProviderMalformed),
        _ => None,
    }
}
