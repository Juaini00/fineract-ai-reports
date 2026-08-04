use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use app_core::config::{EmbeddingConfig, LlmConfig, llm_pricing};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, TokenUsage};

/// First backoff step; doubles per attempt.
const RETRY_BASE_DELAY_MS: u64 = 500;
/// Ceiling for a single wait, including a provider-supplied `Retry-After`.
const RETRY_MAX_DELAY: Duration = Duration::from_secs(8);
/// Ceiling for all waiting inside one call, so a struggling provider cannot
/// stretch a background job into minutes of sleeping.
const RETRY_BUDGET: Duration = Duration::from_secs(45);
/// Spread simultaneous retries so jobs that failed together do not return in
/// lockstep and re-overload the provider.
const RETRY_JITTER_RATIO: f64 = 0.25;

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

    /// Sends `build()` and retries it while the failure looks transient.
    ///
    /// Returns the last response even when it is an error status: the callers
    /// own the "what does a bad status mean" decision (the 400 branch below
    /// still needs to see its 400), this only decides *whether to try again*.
    async fn send_with_retry<F>(
        &self,
        max_retries: u32,
        label: &'static str,
        build: F,
    ) -> Result<reqwest::Response>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let deadline = Instant::now() + RETRY_BUDGET;
        let mut attempt = 0u32;
        loop {
            let outcome = build().send().await;
            let retryable = match &outcome {
                Ok(response) => {
                    is_transient_status(response.status().as_u16()).then(|| retry_after(response))
                }
                Err(error) => (error.is_timeout() || error.is_connect()).then_some(None),
            };
            let Some(retry_after) = retryable else {
                return outcome.context(label);
            };
            let delay = retry_delay(attempt, retry_after, rand::random::<f64>());
            if attempt >= max_retries || Instant::now() + delay > deadline {
                return outcome.context(label);
            }
            let reason = match &outcome {
                Ok(response) => response.status().to_string(),
                Err(error) => error.to_string(),
            };
            tracing::warn!(
                label,
                attempt = attempt + 1,
                max_retries,
                delay_ms = delay.as_millis() as u64,
                reason,
                "retrying transient LLM failure"
            );
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
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
            .send_with_retry(self.llm.max_retries, "send structured LLM request", || {
                self.http
                    .post(self.chat_url())
                    .bearer_auth(&self.llm.api_key)
                    .json(&body(schema_format.clone(), system))
            })
            .await?;
        if response.status().as_u16() == 400 {
            let fallback_system = json_object_system(system, &schema);
            response = self
                .send_with_retry(
                    self.llm.max_retries,
                    "send structured LLM json_object fallback",
                    || {
                        self.http
                            .post(self.chat_url())
                            .bearer_auth(&self.llm.api_key)
                            .json(&body(json!({"type":"json_object"}), &fallback_system))
                    },
                )
                .await?;
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
            .send_with_retry(config.max_retries, "send embedding request", || {
                self.http
                    .post(format!(
                        "{}/embeddings",
                        config.base_url.trim_end_matches('/')
                    ))
                    .bearer_auth(&config.api_key)
                    .json(&json!({"model": config.model, "input": text}))
            })
            .await?;
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

/// Statuses where the same request may well succeed on a second attempt: the
/// provider is overloaded (429/503) or a gateway hiccuped (500/502/504).
///
/// Everything else is ours to fix, and retrying only delays and hides the real
/// error: 401/403 are a bad or unauthorised key, 404 a wrong URL, 422 a bad
/// payload. 400 is deliberately excluded — `structured_value` answers it with
/// the `json_object` schema fallback, and retrying an identical rejected body
/// would just burn the budget before that fallback runs.
fn is_transient_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

/// ponytail: `Retry-After` in seconds only. The HTTP-date form is legal but no
/// LLM provider sends it; that case falls back to our own backoff.
fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Wait before retry `attempt` (0-based): a provider-supplied `Retry-After` if
/// there is one, else exponential backoff from `RETRY_BASE_DELAY_MS`. Both are
/// capped at `RETRY_MAX_DELAY` and stretched by up to `RETRY_JITTER_RATIO`.
///
/// Pure on purpose — `jitter` is passed in so the schedule can be asserted
/// without sleeping.
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// One-shot HTTP server that replies with `script[i]` to request `i` (the
    /// last entry repeats) and counts how many requests it received. Lets the
    /// retry policy be observed end to end without a provider.
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

    fn client(url: &str, max_retries: u32) -> RigLlmClient {
        RigLlmClient::new(
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

    async fn route(client: &RigLlmClient) -> Result<LlmResponse<Value>> {
        client
            .structured_value(LlmPurpose::Test, "system", "user", json!({"type":"object"}))
            .await
    }

    /// `Retry-After: 0` keeps this test instant while proving the header is
    /// what drives the wait — the exponential fallback is asserted purely
    /// below, so nothing here has to sleep through it.
    fn busy(seconds: &str) -> String {
        response(
            "503 Service Unavailable",
            &format!("Retry-After: {seconds}\r\n"),
            r#"{"error":{"message":"Service is too busy."}}"#,
        )
    }

    #[tokio::test]
    async fn a_transient_status_is_retried_until_the_configured_limit() {
        let (url, requests) = spawn_provider(vec![busy("0")]).await;

        let error = route(&client(&url, 2)).await.unwrap_err();

        // 1 initial attempt + LLM_MAX_RETRIES retries, then the original error.
        assert_eq!(requests.load(Ordering::SeqCst), 3);
        assert!(error.to_string().contains("503"), "{error}");
    }

    #[tokio::test]
    async fn the_retry_count_comes_from_config_not_a_constant() {
        let (url, requests) = spawn_provider(vec![busy("0")]).await;

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
            r#"{"choices":[{"message":{"content":"{\"intent\":\"report\",\"confidence\":1}"}}]}"#,
        );
        let (url, requests) = spawn_provider(vec![rejected, accepted]).await;

        let response = route(&client(&url, 3)).await.unwrap();

        // Exactly two: the json_schema attempt and the json_object fallback.
        assert_eq!(requests.load(Ordering::SeqCst), 2);
        assert_eq!(response.value["intent"], "report");
    }

    #[test]
    fn only_overload_and_gateway_statuses_are_transient() {
        for status in [429, 500, 502, 503, 504] {
            assert!(is_transient_status(status), "{status} should be retried");
        }
        // 400 has the schema fallback; the rest are our bug, not the provider's.
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
        // Jitter only ever stretches, never past 1 + RETRY_JITTER_RATIO.
        assert_eq!(retry_delay(0, None, 1.0), Duration::from_millis(625));
    }

    #[test]
    fn retry_after_wins_over_backoff_but_not_over_the_ceiling() {
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(3)), 0.0),
            Duration::from_secs(3)
        );
        // A provider asking for a minute must not park a job for a minute.
        assert_eq!(
            retry_delay(0, Some(Duration::from_secs(60)), 0.0),
            RETRY_MAX_DELAY
        );
    }

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
