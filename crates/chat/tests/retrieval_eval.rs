//! Fixture eval harness (issue 07) — measures top-1 accuracy of the
//! router -> retrieval -> rerank pipeline against 20 bilingual fixtures.
//!
//! Design: rather than spinning up the full DB-backed chat-job graph
//! (`spawn_app` in `common/mod.rs`), this composes the same building blocks
//! the graph itself uses (`SemanticRouter`, `RetrievalEngine::retrieve`,
//! `LlmReranker`) exactly as `retrieval_scoring.rs` / `assistant_retrieval_evidence.rs`
//! already do. No Postgres/Fineract/Redis connection is required in either
//! mode -- `RetrievalEngine::retrieve` is called with `llm: None,
//! knowledge: None` so it always uses the deterministic catalog fallback.
//!
//! Two LLM backends, selected by `EVAL_USE_REAL_LLM=1`:
//! - stub (default): `KeywordStubLlm`, a small `LlmClient` impl that answers
//!   both the router call and the reranker call purely from keyword overlap
//!   between the message and catalog metadata (title/description/examples).
//!   It never looks at a fixture's expected answer, so its accuracy is a
//!   real (if crude) signal about retrieval quality, not a hardcoded echo.
//!   Given the stub's ceiling is "keyword overlap", the 90%/85% accuracy
//!   floors are NOT asserted in this mode -- only that all 20 fixtures load,
//!   the full call path runs without panicking, and per-bucket accuracy is
//!   computed and printed.
//! - real (`EVAL_USE_REAL_LLM=1`): `RigLlmClient` built from
//!   `LLM_API_KEY`/`DEEPSEEK_API_KEY`. This is the actual production
//!   reranker, and the accuracy floors ARE asserted here. If no API key is
//!   present the test prints a message and skips (infra unavailable is not
//!   a CI failure).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use app_core::config::LlmConfig;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use chat::assistant::{
    ContextWindow, SemanticRouter,
    evidence::RetrievalPlan,
    llm::{
        EmbeddingResponse, LlmClient, LlmPurpose, LlmResponse, SharedLlmClient, TokenUsage,
        rig_client::RigLlmClient,
    },
    reranker::{LlmReranker, RerankerVerdict},
    retrieval::RetrievalEngine,
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::model::KnowledgeCatalog;

const OVERALL_FLOOR: f32 = 0.90;
const BUCKET_FLOOR: f32 = 0.85;

#[derive(Debug, Deserialize)]
struct Fixture {
    id: String,
    message: String,
    language: String,
    #[allow(dead_code)]
    domain: String,
    expected_decision: String,
    #[serde(default)]
    expected_capability: Option<String>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/retrieval_eval")
}

fn load_fixtures() -> Vec<Fixture> {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {dir:?}: {e}"))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    entries.sort();
    entries
        .into_iter()
        .map(|path| {
            let text =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
            serde_yaml::from_str::<Fixture>(&text)
                .unwrap_or_else(|e| panic!("parse fixture {path:?}: {e}"))
        })
        .collect()
}

fn empty_context() -> ContextWindow {
    ContextWindow {
        summary: None,
        active_domain: None,
        selected_entities: json!({}),
        recent_messages: Vec::new(),
        relevant_jobs: Vec::new(),
        pending_clarification: None,
        source_intent: None,
        source_snippets: Vec::new(),
        client_scope: json!({}),
        warnings: Vec::new(),
    }
}

/// Deterministic non-LLM stand-in used for both the router call and the
/// reranker call. Routes purely on keyword overlap -- see module docs.
struct KeywordStubLlm;

#[async_trait]
impl LlmClient for KeywordStubLlm {
    async fn structured_value(
        &self,
        purpose: LlmPurpose,
        _system: &str,
        user: &str,
        _schema: Value,
    ) -> Result<LlmResponse<Value>> {
        let value = match purpose {
            LlmPurpose::RouteIntent => stub_route(user),
            LlmPurpose::EvidenceRetrieval => stub_rerank(user),
            other => anyhow::bail!("KeywordStubLlm does not support purpose {other}"),
        };
        Ok(LlmResponse {
            value,
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "stub".into(),
            model: "keyword-stub".into(),
            latency_ms: 0,
        })
    }

    async fn embed(&self, _purpose: LlmPurpose, _text: &str) -> Result<EmbeddingResponse> {
        // Never called: RetrievalEngine::retrieve() runs with knowledge=None.
        Ok(EmbeddingResponse {
            vector: vec![0.0],
            usage: TokenUsage::default(),
            cost_usd: None,
            provider: "stub".into(),
            model: "keyword-stub".into(),
            latency_ms: 0,
        })
    }
}

const ID_MARKERS: &[&str] = &[
    "yang",
    "berikan",
    "saya",
    "pada",
    "yg",
    "dengan",
    "ada",
    "sembarang",
    "tahun",
    "coba",
    "kantor",
    "mana",
    "bulan",
    "terbesar",
    "ringkasan",
    "kami",
    "penarikan",
    "setoran",
    "berapa",
    "resep",
    "paling",
    "enak",
    "saat",
];

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .map(str::to_string)
        .collect()
}

/// Purely keyword-driven router stand-in. Uses only the message text and a
/// small set of generic operation/domain keywords -- no knowledge of any
/// fixture's expected answer.
fn stub_route(user: &str) -> Value {
    let parsed: Value = serde_json::from_str(user).unwrap_or_default();
    let message = parsed["message"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();

    let domain = if message.contains("office") || message.contains("kantor") {
        "organization"
    } else if message.contains("saving")
        || message.contains("deposit")
        || message.contains("withdraw")
        || message.contains("tabungan")
        || message.contains("penarikan")
        || message.contains("setoran")
    {
        "savings"
    } else if message.contains("client") {
        "client"
    } else {
        "unknown"
    };

    let operation = if ["top", "most", "highest", "largest", "terbesar", "tertinggi"]
        .iter()
        .any(|kw| message.contains(kw))
    {
        "rank"
    } else if ["random", "sembarang", "sample", "acak"]
        .iter()
        .any(|kw| message.contains(kw))
    {
        "random_sample"
    } else if ["recent", "recently", "yg ada", "system saat ini"]
        .iter()
        .any(|kw| message.contains(kw))
    {
        "list"
    } else if ["total", "berapa"].iter().any(|kw| message.contains(kw)) {
        "total"
    } else {
        "summary"
    };

    let output = match operation {
        "rank" => "ranking",
        "random_sample" | "list" => "list",
        "total" => "scalar",
        _ => "summary",
    };

    let subject = match domain {
        "organization" => "office",
        "savings" => "savings_transaction",
        "client" => "client",
        _ => "unknown",
    };

    let language = if ID_MARKERS.iter().any(|kw| message.contains(kw)) {
        "id"
    } else {
        "en"
    };

    json!({
        "intent": "report_request",
        "domain": domain,
        "request_shape": {
            "operation": operation,
            "subject": subject,
            "grouping": "none",
            "output": output,
            "pii": if domain == "client" { "client_identity" } else { "none" },
        },
        "language": language,
        "entities": [],
        "constraints": {},
        "context_reference": "none",
        "confidence": 0.7,
        "reason": "keyword stub",
    })
}

/// Purely keyword-driven reranker stand-in: scores each candidate by token
/// overlap between the query and that candidate's own title/description/
/// examples/output_mode (the same fields the real LLM prompt receives).
fn stub_rerank(user: &str) -> Value {
    let parsed: Value = serde_json::from_str(user).unwrap_or_default();
    let query_terms = tokenize(parsed["query"].as_str().unwrap_or_default());
    let candidates = parsed["candidates"].as_array().cloned().unwrap_or_default();

    let mut scored: Vec<(String, f32)> = candidates
        .iter()
        .map(|candidate| {
            let id = candidate["id"].as_str().unwrap_or_default().to_string();
            let examples = candidate["examples"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let haystack = format!(
                "{} {} {} {}",
                candidate["title"].as_str().unwrap_or_default(),
                candidate["description"].as_str().unwrap_or_default(),
                examples,
                candidate["output_mode"].as_str().unwrap_or_default(),
            )
            .to_lowercase();
            let hits = query_terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count() as f32;
            (id, hits)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    let Some((top_id, top_score)) = scored.first().cloned() else {
        return json!({"decision": "unsupported", "capability_id": null, "confidence": 0.0, "alternatives": [], "reason": "no candidates"});
    };
    if top_score == 0.0 {
        return json!({"decision": "unsupported", "capability_id": null, "confidence": 0.0, "alternatives": [], "reason": "no keyword overlap"});
    }
    let runner_up = scored.get(1).map(|(_, score)| *score).unwrap_or(0.0);
    if runner_up > 0.0 && runner_up >= top_score * 0.8 {
        let alternatives: Vec<String> = scored.iter().take(4).map(|(id, _)| id.clone()).collect();
        return json!({"decision": "clarify", "capability_id": null, "confidence": 0.0, "alternatives": alternatives, "reason": "ambiguous keyword overlap"});
    }
    json!({
        "decision": "select",
        "capability_id": top_id,
        "confidence": (0.6 + top_score * 0.08).min(0.95),
        "alternatives": [],
        "reason": "keyword match",
    })
}

fn real_llm_config(api_key: String) -> LlmConfig {
    let get = |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.to_string());
    LlmConfig {
        provider: get("LLM_PROVIDER", "deepseek"),
        api_key,
        chat_completions_url: get(
            "LLM_CHAT_COMPLETIONS_URL",
            "https://api.deepseek.com/chat/completions",
        ),
        base_url: get("LLM_BASE_URL", ""),
        model: get("LLM_MODEL", "deepseek-chat"),
        timeout_ms: get("LLM_TIMEOUT_MS", "30000").parse().unwrap_or(30_000),
        max_retries: 1,
        max_output_tokens: 4000,
        temperature: 0.1,
    }
}

async fn run_pipeline(
    llm: &SharedLlmClient,
    catalog: &Arc<KnowledgeCatalog>,
    fixture: &Fixture,
) -> (RerankerVerdict, Option<String>) {
    let router = SemanticRouter::new(llm.clone());
    let intent = router
        .route(&fixture.message, &empty_context())
        .await
        .unwrap_or_else(|e| panic!("router errored on fixture {}: {e}", fixture.id));
    let plan = RetrievalPlan::new(&fixture.message, &intent, true, vec![]);
    let evidence = RetrievalEngine::retrieve(&plan, None, None, Some(catalog))
        .await
        .unwrap_or_else(|e| panic!("retrieve errored on fixture {}: {e}", fixture.id));
    let decision = LlmReranker::new(Some(llm))
        .rerank(&fixture.message, &evidence)
        .await;
    (decision.decision, decision.capability_id)
}

#[derive(Default, Clone, Copy)]
struct BucketStats {
    total: usize,
    correct: usize,
}

impl BucketStats {
    fn accuracy(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            self.correct as f32 / self.total as f32
        }
    }
}

#[test]
fn fixtures_cover_required_buckets() {
    let fixtures = load_fixtures();
    assert_eq!(fixtures.len(), 20, "expected exactly 20 fixtures");

    let clarify = fixtures
        .iter()
        .filter(|f| f.expected_decision == "clarify")
        .count();
    let unsupported = fixtures
        .iter()
        .filter(|f| f.expected_decision == "unsupported")
        .count();
    assert!(
        clarify >= 4,
        "expected at least 4 clarify fixtures, got {clarify}"
    );
    assert!(
        unsupported >= 3,
        "expected at least 3 unsupported fixtures, got {unsupported}"
    );

    let en = fixtures.iter().filter(|f| f.language == "en").count();
    let id = fixtures.iter().filter(|f| f.language == "id").count();
    assert!(
        en >= 8 && id >= 8,
        "language balance too skewed: en={en} id={id}"
    );

    for domain in ["client", "organization", "savings"] {
        let count = fixtures.iter().filter(|f| f.domain == domain).count();
        assert!(count >= 2, "domain {domain} has too few fixtures: {count}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn retrieval_eval_meets_accuracy_floor() {
    let use_real = std::env::var("EVAL_USE_REAL_LLM").ok().as_deref() == Some("1");

    let llm: SharedLlmClient = if use_real {
        let api_key = std::env::var("LLM_API_KEY")
            .or_else(|_| std::env::var("DEEPSEEK_API_KEY"))
            .unwrap_or_default();
        if api_key.trim().is_empty() {
            eprintln!(
                "retrieval_eval: EVAL_USE_REAL_LLM=1 but LLM_API_KEY/DEEPSEEK_API_KEY is unset; skipping"
            );
            return;
        }
        Arc::new(RigLlmClient::new(&real_llm_config(api_key), None).expect("build real LLM client"))
    } else {
        Arc::new(KeywordStubLlm)
    };

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    let catalog = Arc::new(
        KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
            .load()
            .expect("load knowledge catalog"),
    );

    let fixtures = load_fixtures();
    assert_eq!(fixtures.len(), 20, "expected exactly 20 fixtures");

    let mut overall = BucketStats::default();
    let mut by_language: BTreeMap<String, BucketStats> = BTreeMap::new();
    let mut by_decision: BTreeMap<String, BucketStats> = BTreeMap::new();

    for fixture in &fixtures {
        let (decision, capability_id) = run_pipeline(&llm, &catalog, fixture).await;
        let decision_str = match decision {
            RerankerVerdict::Select => "select",
            RerankerVerdict::Clarify => "clarify",
            RerankerVerdict::Unsupported => "unsupported",
        };
        let correct = decision_str == fixture.expected_decision
            && (fixture.expected_decision != "select"
                || capability_id.as_deref() == fixture.expected_capability.as_deref());

        overall.total += 1;
        by_language
            .entry(fixture.language.clone())
            .or_default()
            .total += 1;
        by_decision
            .entry(fixture.expected_decision.clone())
            .or_default()
            .total += 1;
        if correct {
            overall.correct += 1;
            by_language.get_mut(&fixture.language).unwrap().correct += 1;
            by_decision
                .get_mut(&fixture.expected_decision)
                .unwrap()
                .correct += 1;
        } else {
            eprintln!(
                "MISS [{}] mode={} expected=({}, {:?}) actual=({decision_str}, {capability_id:?})",
                fixture.id,
                if use_real { "real" } else { "stub" },
                fixture.expected_decision,
                fixture.expected_capability,
            );
        }
    }

    eprintln!(
        "=== retrieval_eval ({}) ===",
        if use_real { "real-llm" } else { "stub" }
    );
    eprintln!(
        "overall: {}/{} = {:.2}",
        overall.correct,
        overall.total,
        overall.accuracy()
    );
    for (language, stats) in &by_language {
        eprintln!(
            "language={language}: {}/{} = {:.2}",
            stats.correct,
            stats.total,
            stats.accuracy()
        );
    }
    for (decision, stats) in &by_decision {
        eprintln!(
            "decision={decision}: {}/{} = {:.2}",
            stats.correct,
            stats.total,
            stats.accuracy()
        );
    }

    if use_real {
        assert!(
            overall.accuracy() >= OVERALL_FLOOR,
            "overall accuracy {:.2} below {OVERALL_FLOOR} floor",
            overall.accuracy()
        );
        for (language, stats) in &by_language {
            assert!(
                stats.accuracy() >= BUCKET_FLOOR,
                "language={language} accuracy {:.2} below {BUCKET_FLOOR} floor",
                stats.accuracy()
            );
        }
        for (decision, stats) in &by_decision {
            assert!(
                stats.accuracy() >= BUCKET_FLOOR,
                "decision={decision} accuracy {:.2} below {BUCKET_FLOOR} floor",
                stats.accuracy()
            );
        }
    }
    // ponytail: stub mode intentionally does not assert the accuracy floor
    // -- see module docs. It still exercises the full router -> retrieval ->
    // rerank call path for all 20 fixtures without panicking, which is the
    // meaningful guarantee available without a real LLM.
}
