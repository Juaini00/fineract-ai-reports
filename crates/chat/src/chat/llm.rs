use std::time::Duration;

use anyhow::{Context, Result, bail};
use app_core::config::LlmConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::chat::classifier::ClarificationOption;
use crate::chat::pipeline::answer::{GeneratedAnswer, parse_generated_answer};
use crate::chat::pipeline::model::ParsedIntent;
use crate::chat::pipeline::parser::parse_semantic_response;
use crate::chat::pipeline::retrieval::{LayeredRetrievalPlan, parse_layered_retrieval_response};

const LAYERED_RETRIEVAL_SYSTEM: &str = r#"You are a retrieval planner for a chat-driven banking reporting system.
Given a user report request and conversation context, produce JSON that splits the request into four short retrieval fragments matching the catalog's layered structure: domain, capability, query, keyword. Each fragment must be under 12 words. Never include dates, limits, or currency codes; those are resolved separately.

Return JSON only:
{"layers":{"domain":string,"capability":string,"query":string,"keyword":string,"graph_hint":string?},"confidence":number}
"#;

#[derive(Clone)]
pub struct LlmPlannerClient {
    http: reqwest::Client,
    provider: String,
    api_key: String,
    url: String,
    model: String,
    max_output_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LlmPlannerDecision {
    Capability(String),
    Clarify(String),
    Unsupported,
}

impl LlmPlannerClient {
    pub fn new(config: &LlmConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .context("build LLM HTTP client")?;

        Ok(Self {
            http,
            provider: config.provider.clone(),
            api_key: config.api_key.clone(),
            url: config.chat_completions_url.clone(),
            model: config.model.clone(),
            max_output_tokens: config.max_output_tokens,
            temperature: config.temperature,
        })
    }

    pub fn is_enabled(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    pub async fn choose_capability(
        &self,
        message: &str,
        options: &[ClarificationOption],
    ) -> Result<LlmPlannerDecision> {
        if options.is_empty() {
            bail!("LLM planner fallback requires options");
        }
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for planner fallback");
        }

        let allowed = options
            .iter()
            .map(|option| json!({ "label": option.label, "capability": option.capability }))
            .collect::<Vec<_>>();
        let system = "You choose one approved reporting capability. Return only JSON. Never write SQL. If the user intent does not exactly match an option, return unsupported or clarify.";
        let user = json!({
            "user_message": message,
            "options": allowed,
            "response_schema": {
                "decision": "capability | clarify | unsupported",
                "capability": "one provided capability when decision=capability",
                "question": "short English question when decision=clarify"
            }
        })
        .to_string();

        let content = self.chat_json(system, user, "planner fallback").await?;
        let decision: PlannerResponse = match serde_json::from_str(&content) {
            Ok(decision) => decision,
            Err(_) => {
                serde_json::from_value(extract_json_object(&content)?).with_context(|| {
                    format!("parse {} planner fallback JSON content", self.provider)
                })?
            }
        };

        match decision.decision.as_str() {
            "capability" => {
                let Some(capability) = decision.capability else {
                    bail!("LLM chose capability without capability id");
                };
                if options.iter().any(|option| option.capability == capability) {
                    Ok(LlmPlannerDecision::Capability(capability))
                } else {
                    bail!("LLM returned capability outside approved options")
                }
            }
            "clarify" => Ok(LlmPlannerDecision::Clarify(
                decision
                    .question
                    .unwrap_or_else(|| "Please clarify which report you want.".to_string()),
            )),
            "unsupported" => Ok(LlmPlannerDecision::Unsupported),
            other => bail!("LLM returned unsupported planner decision {other}"),
        }
    }

    pub async fn parse_intent(
        &self,
        message: &str,
        context: &serde_json::Value,
    ) -> Result<ParsedIntent> {
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for semantic parser");
        }

        let system = "You are the semantic parser for a reporting RAG pipeline. Return only JSON. Extract intent, domain, entities, date constraints, and quantity. Do not choose SQL. Do not invent capability ids.";
        let user = json!({
            "user_message": message,
            "context": context,
            "response_schema": {
                "intent": "report | clarification_answer | unsupported | tool_action",
                "domain": "savings | client | organization | unknown",
                "entities": [{ "type": "capability_hint | product | currency | office | date_period", "value": "string" }],
                "constraints": {
                    "from_date": "YYYY-MM-DD or null",
                    "to_date": "YYYY-MM-DD or null",
                    "quantity": { "mode": "all | limit | top_n | default", "value": "integer when needed" },
                    "currency_code": "string or null",
                    "product_ids": "array of integers or null"
                },
                "requires_retrieval": true,
                "confidence": "number between 0 and 1"
            }
        })
        .to_string();

        let content = self.chat_json(system, user, "semantic parser").await?;
        parse_semantic_response(&content)
    }

    pub async fn generate_answer(
        &self,
        user_message: &str,
        structured: &serde_json::Value,
    ) -> Result<GeneratedAnswer> {
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for answer generation");
        }
        let system = "You generate a SHORT grounded reporting summary from an aggregated evidence packet. \
The evidence never includes raw rows — reason from the pre-computed aggregates \
(structured.by_currency, structured.weekly_aggregation, structured.row_count, answer_plan.coverage). \
STRICT length limits: `message` MUST be under 600 characters total, one paragraph, no lists, no per-row detail. \
The deterministic formatter already renders the full table — your job is a 2–4 sentence overview only. \
Return ONLY JSON: {\"message\": string, \"citations\": [string]}. \
Citations must reference paths that appear in evidence \
(e.g. `answer_plan.coverage`, `structured.by_currency`, `structured.weekly_aggregation`, `structured.row_count`). \
Do not add facts not present in evidence. Do not restate individual rows.";
        let user = json!({
            "user_message": user_message,
            "evidence": structured,
            "response_schema": {
                "message": "markdown string",
                "citations": [
                    "answer_plan.coverage",
                    "structured.by_currency",
                    "structured.weekly_aggregation",
                    "structured.row_count"
                ]
            }
        })
        .to_string();
        let content = self.chat_json(system, user, "answer generation").await?;
        parse_generated_answer(&content)
    }

    pub async fn plan_layered_retrieval(
        &self,
        message: &str,
        conversation_context: &Value,
    ) -> Result<LayeredRetrievalPlan> {
        if !self.is_enabled() {
            bail!("LLM_API_KEY is required for layered retrieval planner");
        }
        let user = json!({
            "conversation_context": conversation_context,
            "user_message": message,
        })
        .to_string();
        let content = self
            .chat_json(LAYERED_RETRIEVAL_SYSTEM, user, "layered retrieval planner")
            .await?;
        parse_layered_retrieval_response(&content)
    }

    async fn chat_json(&self, system: &str, user: String, operation: &str) -> Result<String> {
        let response = self
            .http
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": user }
                ],
                "temperature": self.temperature,
                "max_tokens": self.max_output_tokens,
                "response_format": { "type": "json_object" }
            }))
            .send()
            .await
            .with_context(|| format!("request {} {operation}", self.provider))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!(
                "{} {operation} failed with status {status}: {body}",
                self.provider
            );
        }

        let payload: ChatCompletionResponse = response
            .json()
            .await
            .with_context(|| format!("parse {} {operation} response", self.provider))?;
        payload
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message.content)
            .with_context(|| format!("{} {operation} returned no choice", self.provider))
    }
}

fn extract_json_object(content: &str) -> Result<Value> {
    let start = content
        .find('{')
        .context("LLM content has no JSON object")?;
    let end = content
        .rfind('}')
        .context("LLM content has no JSON object end")?;
    serde_json::from_str(&content[start..=end]).context("parse embedded JSON object")
}

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

#[derive(Debug, Serialize, Deserialize)]
struct PlannerResponse {
    decision: String,
    #[serde(default)]
    capability: Option<String>,
    #[serde(default)]
    question: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_object_from_wrapped_content() {
        assert_eq!(
            extract_json_object("```json\n{\"decision\":\"unsupported\"}\n```").unwrap()["decision"],
            "unsupported"
        );
    }
}
