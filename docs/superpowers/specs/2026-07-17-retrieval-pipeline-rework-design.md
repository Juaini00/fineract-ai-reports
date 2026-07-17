# Retrieval pipeline rework — semantic-first, LLM-orchestrated

**Date:** 2026-07-17 · **Status:** design · **Owner:** akhlaquetabrez.drive@gmail.com

## Context

The semantic assistant graph classifies user queries to approved reporting capabilities in `knowledge/capabilities/*.yaml`. Three real production queries observed today, all in-scope, all rejected as `unsupported_in_domain`:

| # | Query | Router shape (correct) | Result |
|---|---|---|---|
| A | "coba berikan saya 5 client sembarang pada tahun ini" | `op=RandomSample, subj=Client` | unsupported |
| B | "give me 3 clients where have the most savings account for this year" | `op=Rank, subj=Client, output=Ranking, pii=ClientIdentity` — matches `client_top_n_by_savings_account_count` | unsupported |
| C | "berikan 3 office yg ada pada system saat ini" | `op=RandomSample, subj=Office` | unsupported |

Router output was correct in all three cases. The retrieval layer discarded the results.

## Problem statement

`crates/chat/src/assistant/retrieval.rs:22-56` executes:

```
compatible_ids(plan, catalog)            ← STRICT enum equality across 5 shape dims + domain
   ├─ empty? → return []                 ← pipeline dies here, embedding never runs
   ├─ embedding hybrid search restricted to compatible_ids
   └─ final retain filter to compatible_ids
```

`compatible_ids` requires exact equality on `operation | subject | grouping | output | pii | domain`. One dimension mismatch collapses the candidate set to empty. This inverts RAG best-practice: strict filters should score, not gate; semantic retrieval should always produce candidates.

Two orthogonal contributors:

1. **Router LLM sometimes produces `operation=random_sample`** for "give me N X" queries. No capability YAML in the catalog uses this shape. → `compatible_ids = []`.
2. **Router LLM sometimes classifies domain by noun mention** (query B: "savings account" → `domain=Savings`) even though the actual `subject=Client`. Every `client` capability filtered out by domain; every `savings` capability filtered out by subject. Zero survivors.

Both cases surface as `unsupported_in_domain` responses — indistinguishable from genuinely-out-of-scope queries.

Additional structural gaps:

- Catalog has no "browse/list" primitives (`office_list_*`, `client_list_*`), so any "give me N X" query without a ranking metric is fundamentally answerless.
- `pgvector`, `swiftide`, and `rig-core` are declared workspace deps (Cargo.toml:32, 53-55) for exactly this pipeline but the hybrid search they enable is bypassed by the gate.
- `EvidenceEvaluator` (`evidence.rs:102-140`) uses hand-picked score thresholds and requires `has_metric_entity` for multi-candidate selection — no semantic reasoning between candidates.
- `state_json` persists `input`, `client`, `classification.runtime` only. Debugging today's failures required editing code to add `tracing::info!` calls and restarting the server.

## Goals

1. Every in-scope query surfaces at least one candidate for downstream evaluation.
2. Correct capability is selected top-1 with ≥ 90% accuracy on a 20-query bilingual (ID/EN) fixture set.
3. Per-request retrieval trace is inspectable from `GET /chat/jobs/{id}` without touching logs.
4. Existing auth boundaries (`allowed_capabilities`, office scope) remain enforced — no widening of caller visibility.

## Non-goals

- Fine-tuning the router LLM. Prompt improvements only where they unblock (`random_sample` misclassification already patched in `router.rs`).
- Full `swiftide` rewrite of `KnowledgeRepository` indexing. That's a v2 concern.
- Multi-turn conversational retrieval memory.
- Response formatting quality (executor concern, out of scope).

## Target architecture

```
message
  │
  ├─→ SemanticRouter (LLM structured intent)
  │
  ├─→ RetrievalEngine
  │     ├─ embed(message) → hybrid search top-K over ALL approved capabilities
  │     │                    within caller's allowed_capabilities (auth kept)
  │     ├─ score = cosine × 0.6 + shape_match × 0.3 + keyword_hits × 0.1
  │     └─ return ranked candidates (never empty when catalog non-empty)
  │
  ├─→ LlmReranker (rig-core structured agent)
  │     ├─ input: message + top-K candidate summaries
  │     ├─ output: { decision: Select | Clarify | Unsupported,
  │     │           capability_id?, confidence, alternatives[], reason }
  │     └─ schema derived via schemars::JsonSchema
  │
  ├─→ execute or clarify
  │
  └─→ persist retrieval_trace to chat_jobs.state_json
```

Shape becomes a scoring signal. Domain check removed (subject subsumes it). Embedding always runs. Reranker replaces threshold arithmetic with LLM reasoning over candidate metadata.

## Work decomposition

Detailed per-issue specs in `docs/issues/retrieval-pipeline-rework/`:

| # | File | Priority | Effort | Depends on |
|---|---|---|---|---|
| 04 | `04-drop-domain-filter.md` | P1 | XS | — |
| 01 | `01-semantic-first-retrieval.md` | P0 | S | 04 (recommended) |
| 06 | `06-retrieval-trace-observability.md` | P1 | S | — |
| 03 | `03-catalog-browse-primitives.md` | P1 | S | 01 (to be reachable) |
| 02 | `02-llm-reranker.md` | P0 | M | 01, 06 |
| 05 | `05-schemars-contracts.md` | P2 | S | 02 |
| 07 | `07-swiftide-eval-harness.md` | P2 | M | 03, 06 |

**Recommended sequence:** 04 → 01 → 06 → 03 → 02 → 05 → 07. Low-risk unlocks first, big rewrite (02) after trace (06) makes debugging easy, eval harness (07) last to lock in the accuracy floor.

## Data / schema changes

- No SQL schema changes.
- `JobMemory` gains a `retrieval_trace: serde_json::Value` field (issue 06).
- `chat_jobs.state_json.retrieval_trace` — new JSON subtree, ≤ 8KB p95.
- Three new capability YAMLs + queries (issue 03). Vector index rebuild required.

## Testing strategy

- **Unit** — retrieval scoring (01), reranker verdict mapping (02), fixture parsing (07).
- **Integration** — replay the three failing queries in the "Context" table; assert non-`unsupported` outcome after issues 01+02+03 land.
- **Regression eval** — 20-fixture bilingual set, ≥ 90% top-1 accuracy floor enforced in CI (07).
- **Existing suites** — `assistant_retrieval_evidence.rs`, `savings_answer_quality.rs`, `organization_answer_quality.rs` must continue to pass; thresholds adjusted only where scoring semantics change.

## Rollout

- Feature-flag gate not needed — retrieval changes are internal, no API contract shift.
- Deploy in the recommended sequence above; each issue is independently mergeable.
- After 02 lands, monitor `state_json.retrieval_trace.decision.kind` distribution in staging for 24h before promoting.

## Risks

| Risk | Mitigation |
|---|---|
| LLM reranker adds latency | Budget +500ms p95; measure per issue 02. Cacheable per (query, catalog version). |
| Semantic-first widens false-positives (irrelevant capabilities score high on cosine) | Reranker (02) is the safety net; shape_score in ranking keeps intent-shape aligned candidates on top. |
| Fixture set overfits to today's queries | 20 fixtures cover 3 decision types × 3 domains × 2 languages; expand quarterly. |
| Vector index rebuild after adding capabilities disrupts running requests | Rebuild via existing `POST /vector-index/rebuild` in maintenance window. |

## Open questions

None blocking. Follow-ups if 02 lands well:

- Should router's `AssistantIntent` also flow through schemars-derived schema (issue 05 already covers)?
- Do we need a cross-turn retrieval memory for follow-up questions? Separate spec.

## Success criteria

- Three failing queries route correctly (or `Clarify` with sensible options) — no `unsupported` at retrieval stage.
- Fixture eval ≥ 90% top-1 accuracy.
- Retrieval trace visible in job payload.
- Net LoC neutral (±100).
