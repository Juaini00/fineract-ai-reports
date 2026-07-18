#[path = "../src/assistant/understanding/intent.rs"]
mod intent;

use intent::{AssistantIntent, AssistantIntentKind, AssistantLanguage, ContextReference};
use reqwest::StatusCode;
use rig_core::tool::Tool;
use schemars::schema_for;
use serde::{Deserialize, Serialize};
use std::time::Duration;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Deserialize)]
struct LookupArgs {
    capability: String,
}

#[derive(Debug, Serialize)]
struct LookupResult {
    capability: String,
    approved: bool,
}

#[derive(Clone, Copy)]
struct FakeCapabilityLookup;

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

impl Tool for FakeCapabilityLookup {
    const NAME: &'static str = "fake_capability_lookup";

    type Args = LookupArgs;
    type Error = std::convert::Infallible;
    type Output = LookupResult;

    fn description(&self) -> String {
        "Phase 0 fake tool that confirms a capability can round-trip through Rig's Tool trait."
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": { "capability": { "type": "string" } },
            "required": ["capability"]
        })
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(LookupResult {
            capability: args.capability,
            approved: true,
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = required_env("LLM_PROVIDER")?;
    let api_key = required_env("LLM_API_KEY")?;
    let url = required_env("LLM_CHAT_COMPLETIONS_URL")?;
    let model = required_env("LLM_MODEL")?;
    let timeout_ms = std::env::var("LLM_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(DEFAULT_TIMEOUT_MS);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()?;

    let response = match send_chat_completion(
        &client,
        &url,
        &api_key,
        &model,
        serde_json::json!({
            "type": "json_schema",
            "json_schema": {
                "name": "AssistantIntent",
                "schema": schema_for!(AssistantIntent),
                "strict": false
            }
        }),
    )
    .await
    {
        Ok(response) => response,
        Err(err)
            if err
                .downcast_ref::<reqwest::Error>()
                .and_then(reqwest::Error::status)
                == Some(StatusCode::BAD_REQUEST) =>
        {
            send_chat_completion(
                &client,
                &url,
                &api_key,
                &model,
                serde_json::json!({ "type": "json_object" }),
            )
            .await?
        }
        Err(err) => return Err(err),
    };

    let content = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("{provider} returned no chat choices"))?
        .message
        .content;
    let intent: AssistantIntent = serde_json::from_str(content.trim())?;

    if !matches!(intent.intent, AssistantIntentKind::ReportRequest)
        || !matches!(intent.language, AssistantLanguage::En)
        || !matches!(intent.context_reference, ContextReference::None)
        || intent.confidence <= 0.0
    {
        anyhow::bail!("structured AssistantIntent failed Phase 0 validation: {intent:?}");
    }

    let tool_result = FakeCapabilityLookup
        .call(LookupArgs {
            capability: "savings.balance.summary".to_string(),
        })
        .await?;

    println!(
        "{}",
        serde_json::json!({ "intent": intent, "fake_tool_result": tool_result })
    );
    Ok(())
}

async fn send_chat_completion(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
    response_format: serde_json::Value,
) -> anyhow::Result<ChatCompletionResponse> {
    Ok(client
        .post(url)
        .bearer_auth(api_key)
        .json(&serde_json::json!({
            "model": model,
            "messages": [
                {
                    "role": "system",
                    "content": "Return only a JSON object matching AssistantIntent. Use exact snake_case enum values: intent=report_request, domain=savings, language=en, context_reference=none. Use entities=[] and constraints={}. No markdown."
                },
                {
                    "role": "user",
                    "content": "Extract an AssistantIntent for: show total savings balances for this month. Return intent=report_request, domain=savings, language=en, context_reference=none, confidence greater than 0, entities=[], constraints={}, and reason string."
                }
            ],
            "response_format": response_format
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<ChatCompletionResponse>()
        .await?)
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("missing {name}"))
}
