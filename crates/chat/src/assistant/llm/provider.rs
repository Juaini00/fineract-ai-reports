use std::{
    future::Future,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow, bail};
use app_core::config::{EmbeddingConfig, LlmConfig, llm_pricing};
use async_trait::async_trait;
use rig_core::{
    client::{CompletionClient, EmbeddingsClient},
    completion::Prompt,
    embeddings::EmbeddingModel,
    providers::openai,
};
use schemars::Schema;
use serde_json::{Value, json};

use super::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, TokenUsage};

/// First backoff step; doubles per attempt.
const RETRY_BASE_DELAY_MS: u64 = 500;
/// Ceiling for a single wait.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(8);
/// Ceiling for all waiting inside one call, so a struggling provider cannot
/// stretch a background job into minutes of sleeping.
const RETRY_BUDGET: Duration = Duration::from_secs(45);
/// Spread simultaneous retries so jobs that failed together do not return in
/// lockstep and re-overload the provider.
const RETRY_JITTER_RATIO: f64 = 0.25;

/// The project LLM provider adapter.
///
/// Rig owns the OpenAI-compatible client, structured-output request, and agent
/// turn loop. This adapter owns application configuration, bounded retries,
/// pricing, and the sanitized `LlmClient` boundary used by the rest of chat.
pub struct LlmProvider {
    llm: LlmConfig,
    embedding: Option<EmbeddingConfig>,
}

struct ProviderStructuredResponse {
    content: String,
    input_tokens: u64,
    output_tokens: u64,
}

impl LlmProvider {
    pub fn new(llm: &LlmConfig, embedding: Option<&EmbeddingConfig>) -> Result<Self> {
        Ok(Self {
            llm: llm.clone(),
            embedding: embedding.cloned(),
        })
    }

    pub fn is_enabled(&self) -> bool {
        !self.llm.api_key.trim().is_empty()
    }

    fn chat_base_url(&self) -> String {
        let url = if self.llm.chat_completions_url.trim().is_empty() {
            self.llm.base_url.trim()
        } else {
            self.llm.chat_completions_url.trim()
        };
        url.strip_suffix("/chat/completions")
            .unwrap_or(url)
            .trim_end_matches('/')
            .to_string()
    }

    fn openai_client(api_key: &str, base_url: &str) -> Result<openai::CompletionsClient> {
        openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .context("build Rig OpenAI-compatible client")
    }

    async fn send_with_retry<F, Fut, T>(
        &self,
        max_retries: u32,
        label: &'static str,
        send: F,
    ) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        let deadline = Instant::now() + RETRY_BUDGET;
        let mut attempt = 0u32;
        loop {
            match send().await {
                Ok(response) => return Ok(response),
                Err(error) if is_transient_error(&error) && attempt < max_retries => {
                    let delay = retry_delay(attempt, None, rand::random::<f64>());
                    if Instant::now() + delay > deadline {
                        return Err(anyhow!("{label}: {error}"));
                    }
                    tracing::warn!(
                        label,
                        attempt = attempt + 1,
                        max_retries,
                        delay_ms = delay.as_millis() as u64,
                        reason = %error,
                        "retrying transient LLM failure"
                    );
                    tokio::time::sleep(delay).await;
                    attempt += 1;
                }
                Err(error) => return Err(anyhow!("{label}: {error}")),
            }
        }
    }

    async fn send_structured(
        &self,
        system: &str,
        user: &str,
        schema: Option<Schema>,
    ) -> Result<ProviderStructuredResponse> {
        let client = Self::openai_client(&self.llm.api_key, &self.chat_base_url())?;
        let mut agent = client
            .agent(self.llm.model.clone())
            .preamble(system)
            .temperature(f64::from(self.llm.temperature))
            .max_tokens(self.llm.max_output_tokens as u64);
        if let Some(schema) = schema {
            agent = agent.output_schema_raw(schema);
        }
        let response = tokio::time::timeout(
            Duration::from_millis(self.llm.timeout_ms),
            agent.build().prompt(user).max_turns(1).extended_details(),
        )
        .await
        .map_err(|_| anyhow!("LLM request timed out"))??;
        Ok(ProviderStructuredResponse {
            content: response.output,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
        })
    }

    async fn send_json_object_fallback(
        &self,
        system: &str,
        user: &str,
        schema: &Value,
    ) -> Result<ProviderStructuredResponse> {
        let fallback_system = json_object_system(system, schema);
        let client = Self::openai_client(&self.llm.api_key, &self.chat_base_url())?;
        let response = tokio::time::timeout(
            Duration::from_millis(self.llm.timeout_ms),
            client
                .agent(self.llm.model.clone())
                .preamble(&fallback_system)
                .temperature(f64::from(self.llm.temperature))
                .max_tokens(self.llm.max_output_tokens as u64)
                .additional_params(json!({"response_format": {"type": "json_object"}}))
                .build()
                .prompt(user)
                .max_turns(1)
                .extended_details(),
        )
        .await
        .map_err(|_| anyhow!("LLM request timed out"))??;
        Ok(ProviderStructuredResponse {
            content: response.output,
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
        })
    }
}

#[async_trait]
impl LlmClient for LlmProvider {
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
        let rig_schema: Schema =
            serde_json::from_value(schema.clone()).context("parse LLM JSON schema")?;
        let response = match self
            .send_with_retry(self.llm.max_retries, "send structured LLM request", || {
                self.send_structured(system, user, Some(rig_schema.clone()))
            })
            .await
        {
            Ok(response) => response,
            Err(error) if status_code(&error) == Some(400) => {
                self.send_with_retry(
                    self.llm.max_retries,
                    "send structured LLM json_object fallback",
                    || self.send_json_object_fallback(system, user, &schema),
                )
                .await?
            }
            Err(error) => return Err(error),
        };
        let value = parse_structured_content(&response.content)?;
        let usage = token_usage(response.input_tokens, response.output_tokens);
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
        let client = Self::openai_client(&config.api_key, config.base_url.trim_end_matches('/'))?;
        let response = tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client
                .embedding_model(config.model.clone())
                .embed_text_with_usage(text),
        )
        .await
        .map_err(|_| anyhow!("embedding request timed out"))??;
        let vector = response
            .embeddings
            .into_iter()
            .next()
            .map(|embedding| {
                embedding
                    .vec
                    .into_iter()
                    .map(|value| value as f32)
                    .collect()
            })
            .unwrap_or_default();
        Ok(EmbeddingResponse {
            vector,
            usage: token_usage(response.usage.input_tokens, response.usage.output_tokens),
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

fn token_usage(input_tokens: u64, output_tokens: u64) -> TokenUsage {
    match (i32::try_from(input_tokens), i32::try_from(output_tokens)) {
        (Ok(input_tokens), Ok(output_tokens)) if input_tokens > 0 || output_tokens > 0 => {
            TokenUsage::provider_reported(input_tokens, output_tokens)
        }
        _ => TokenUsage::default(),
    }
}

/// System prompt for the `json_object` fallback, used when a provider rejects
/// `json_schema` (DeepSeek among them). The literal word "JSON" and the schema
/// are both required by providers that only support the object response mode.
fn json_object_system(system: &str, schema: &Value) -> String {
    let schema = serde_json::to_string(schema).unwrap_or_default();
    format!(
        "{system}\n\nRespond with a single JSON object and nothing else. \
It must conform to this JSON Schema, including every required field:\n{schema}"
    )
}

fn status_code(error: &anyhow::Error) -> Option<u16> {
    for cause in error.chain() {
        let message = cause.to_string();
        if let Some(status) = message
            .split(|character: char| !character.is_ascii_digit())
            .find_map(|token| (token.len() == 3).then(|| token.parse().ok()).flatten())
        {
            return Some(status);
        }
    }
    None
}

fn is_transient_error(error: &anyhow::Error) -> bool {
    status_code(error).is_some_and(is_transient_status)
        || error.to_string().to_ascii_lowercase().contains("timeout")
        || error
            .to_string()
            .to_ascii_lowercase()
            .contains("connection")
}

/// Statuses where the same request may well succeed on a second attempt: the
/// provider is overloaded (429/503) or a gateway hiccuped (500/502/504).
fn is_transient_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

fn retry_delay(attempt: u32, retry_after: Option<Duration>, jitter: f64) -> Duration {
    let base =
        retry_after.unwrap_or_else(|| Duration::from_millis(RETRY_BASE_DELAY_MS << attempt.min(5)));
    base.min(RETRY_MAX_DELAY)
        .mul_f64(1.0 + RETRY_JITTER_RATIO * jitter.clamp(0.0, 1.0))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    async fn spawn_provider(script: Vec<String>) -> (String, Arc<AtomicUsize>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = requests.clone();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let index = counter.fetch_add(1, Ordering::SeqCst);
                let mut buffer = [0u8; 8192];
                let _ = stream.read(&mut buffer).await;
                let reply = &script[index.min(script.len() - 1)];
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        (url, requests)
    }

    fn response(status: &str, headers: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{body}",
            body.len()
        )
    }

    fn client(url: &str, max_retries: u32) -> LlmProvider {
        LlmProvider::new(
            &LlmConfig {
                provider: "test".into(),
                api_key: "test-key".into(),
                chat_completions_url: url.into(),
                base_url: String::new(),
                model: "test-model".into(),
                timeout_ms: 2_000,
                max_retries,
                max_output_tokens: 16,
                temperature: 0.0,
            },
            None,
        )
        .unwrap()
    }

    async fn route(client: &LlmProvider) -> Result<LlmResponse<Value>> {
        client
            .structured_value(LlmPurpose::Test, "system", "user", json!({"type":"object"}))
            .await
    }

    fn busy() -> String {
        response(
            "503 Service Unavailable",
            "",
            r#"{"error":{"message":"Service is too busy."}}"#,
        )
    }

    #[tokio::test]
    async fn a_transient_status_is_retried_until_the_configured_limit() {
        let (url, requests) = spawn_provider(vec![busy()]).await;

        let error = route(&client(&url, 2)).await.unwrap_err();

        assert_eq!(requests.load(Ordering::SeqCst), 3);
        assert!(error.to_string().contains("503"), "{error}");
    }

    #[tokio::test]
    async fn the_retry_count_comes_from_config_not_a_constant() {
        let (url, requests) = spawn_provider(vec![busy()]).await;

        route(&client(&url, 0)).await.unwrap_err();
        assert_eq!(requests.load(Ordering::SeqCst), 1, "0 retries = 1 attempt");

        route(&client(&url, 4)).await.unwrap_err();
        assert_eq!(requests.load(Ordering::SeqCst), 6, "4 retries = 5 attempts");
    }

    #[tokio::test]
    async fn a_non_transient_status_is_not_retried() {
        let denied = response("401 Unauthorized", "", r#"{"error":"bad key"}"#);
        let (url, requests) = spawn_provider(vec![denied]).await;

        let error = route(&client(&url, 3)).await.unwrap_err();

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(error.to_string().contains("401"), "{error}");
    }

    #[tokio::test]
    async fn a_400_still_falls_back_to_json_object_instead_of_retrying() {
        let rejected = response("400 Bad Request", "", r#"{"error":"no json_schema"}"#);
        let accepted = response(
            "200 OK",
            "",
            r#"{"id":"chatcmpl-test","object":"chat.completion","created":1,"model":"test-model","choices":[{"index":0,"message":{"role":"assistant","content":"{\"intent\":\"report\",\"confidence\":1}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
        );
        let (url, requests) = spawn_provider(vec![rejected, accepted]).await;

        let response = route(&client(&url, 3)).await.unwrap();

        assert_eq!(requests.load(Ordering::SeqCst), 2);
        assert_eq!(response.value["intent"], "report");
    }

    #[test]
    fn only_overload_and_gateway_statuses_are_transient() {
        for status in [429, 500, 502, 503, 504] {
            assert!(is_transient_status(status), "{status} should be retried");
        }
        for status in [200, 400, 401, 403, 404, 409, 422] {
            assert!(!is_transient_status(status), "{status} must not be retried");
        }
    }

    #[test]
    fn backoff_doubles_per_attempt_and_stays_under_the_ceiling() {
        assert_eq!(retry_delay(0, None, 0.0), Duration::from_millis(500));
        assert_eq!(retry_delay(1, None, 0.0), Duration::from_millis(1_000));
        assert_eq!(retry_delay(2, None, 0.0), Duration::from_millis(2_000));
        assert_eq!(retry_delay(9, None, 0.0), RETRY_MAX_DELAY);
        assert_eq!(retry_delay(0, None, 1.0), Duration::from_millis(625));
    }

    #[test]
    fn retry_after_wins_over_backoff_but_not_over_the_ceiling() {
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(3)), 0.0),
            Duration::from_secs(3)
        );
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(60)), 0.0),
            RETRY_MAX_DELAY
        );
    }

    #[test]
    fn json_object_system_carries_the_word_json_and_the_schema() {
        let schema = json!({"required": ["intent"]});
        let prompt = json_object_system("Route the user request.", &schema);
        assert!(prompt.to_lowercase().contains("json"));
        assert!(prompt.contains("\"required\":[\"intent\"]"));
        assert!(prompt.starts_with("Route the user request."));
    }
}
