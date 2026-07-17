# Epic: Retrieval pipeline rework — semantic-first, LLM-orchestrated

**Status:** open · **Priority:** P0 · **Created:** 2026-07-17

## Problem

Three real requests from production logs today, all classified as `unsupported_in_domain` despite being valid, in-scope reporting questions the catalog can answer:

| Query | Router output (correct) | Actual result |
|---|---|---|
| "coba berikan saya 5 client sembarang pada tahun ini" | `op=RandomSample, subj=Client` | `unsupported` |
| "give me 3 clients where have the most savings account for this year" | `op=Rank, subj=Client, output=Ranking, pii=ClientIdentity` ✅ (matches `client_top_n_by_savings_account_count`) | `unsupported` |
| "berikan 3 office yg ada pada system saat ini" | `op=RandomSample, subj=Office` | `unsupported` |

The router did its job. The retrieval pipeline threw the results away.

## Root cause (single)

`crates/chat/src/assistant/retrieval.rs:22-56` executes:

```
compatible_ids(plan, catalog)           ← STRICT enum equality across 5 shape dims + domain
   │
   ├─ empty? → return []                ← whole pipeline dies here
   │
   ├─ embedding hybrid search restricted to compatible_ids
   └─ final retain filter to compatible_ids
```

The entire embedding + pgvector + hybrid-search stack is **hard-gated** behind exact-match enum filtering on the request shape. One mismatch in any of `operation | subject | grouping | output | pii | domain` and semantic recall drops to zero.

This inverts RAG best-practice. Semantic retrieval should come first, then shape/domain compatibility should score/boost — not gate.

## Failure taxonomy (from logs)

1. **Shape not in catalog at all** — LLM produces `operation=random_sample` for "give me N X" queries; no capability YAML has this shape. → `compatible_ids = []`.
2. **Domain mismatch despite subject match** — Router classifies "top clients by savings account" as `domain=Savings` (noun-driven), but `client_top_n_by_savings_account_count` has `domain=client`. Subject was right; domain filter still killed it.
3. **Catalog gaps for browse/list use cases** — No `office_list_basic`, `client_list_all`, or similar low-effort "browse" capabilities. Users don't always want top-N-by-metric.
4. **Semantic search bypassed** — pgvector, `search_hybrid_by_source_type`, and swiftide are wired but effectively unused because of the gate.
5. **No LLM re-ranker** — Enum matching is brittle; an LLM reading top-K descriptions could pick correctly even when shape dimensions are slightly off.

## Stack we have but don't use

Declared in `Cargo.toml` for this exact migration (line 51-55) but touched minimally in retrieval:

- `rig-core 0.40` — agent + tool framework, ideal for structured capability selection.
- `swiftide 0.32` — RAG pipeline with indexing + query stages + evaluator.
- `schemars 1.2` — derive JSON schemas from Rust types, tighter LLM structured contracts.
- `pgvector 0.4` — already indexed but under-queried due to gate.

## Target architecture

```
message
  │
  ├─→ SemanticRouter (LLM structured intent)
  │
  ├─→ RetrievalEngine
  │     ├─ embed(message) → hybrid search top-K (K=10-20)
  │     ├─ score = cosine * 0.6 + shape_match * 0.3 + subject_match * 0.1
  │     └─ return ranked candidates (never empty when catalog has capabilities)
  │
  ├─→ LlmReranker (rig-core agent, structured output via schemars)
  │     ├─ input: query + top-K candidates
  │     └─ output: { decision: Select | Clarify | Unsupported, capability_id, confidence, reason }
  │
  └─→ execute or clarify
```

## Sub-issues

Order below is the recommended sequence. Low-risk unlocks first, big rewrite after observability is in place.

| # | Title | Priority | Effort |
|---|---|---|---|
| [04](./04-drop-domain-filter.md) | Drop redundant domain strict filter | P1 | XS |
| [01](./01-semantic-first-retrieval.md) | Invert retrieval: semantic first, shape as score | P0 | S |
| [06](./06-retrieval-trace-observability.md) | Persist retrieval trace to `state_json` | P1 | S |
| [03](./03-catalog-browse-primitives.md) | Add browse/list capabilities to catalog | P1 | S |
| [02](./02-llm-reranker.md) | Replace `EvidenceEvaluator` with LLM re-ranker | P0 | M |
| [05](./05-schemars-contracts.md) | Structured LLM contracts via `schemars` | P2 | S |
| [07](./07-swiftide-eval-harness.md) | Regression eval harness with fixture set | P2 | M |
| [08](./08-clarification-reply-routing-failed.md) | Clarification-reply routes to `Routing failed` for valid option | P1 | M |

## Success criteria for the epic

- All three queries in the "Problem" table above route to a valid capability (or `Clarify` with sensible options), not `unsupported`.
- Regression: 20-query bilingual fixture set in issue 07 achieves ≥ 90% correct top-1 capability selection.
- `state_json.retrieval_trace` is inspectable per request without needing log streaming.
- Zero net LoC increase or decrease target (issue 02 replaces 150 LoC of matcher with ~250 LoC of reranker; issue 04 removes ~10 LoC; issue 01 modifies ~100 LoC).

## Out of scope

- Full swiftide indexing pipeline rewrite (would displace `KnowledgeRepository`; save for a v2).
- Fine-tuning the router LLM. Prompt improvements only where they unblock (already done for `random_sample` misclassification in router.rs).
- Multi-turn conversational retrieval memory (separate concern).
