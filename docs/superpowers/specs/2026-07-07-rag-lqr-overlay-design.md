# RAG + LQR Overlay — Design Spec

> Superseded on 2026-07-13 by the semantic assistant graph migration. LQR remains an optional retrieval strategy behind config, not the source-of-truth runtime flow.

**Companion to:** `docs/superpowers/plans/2026-07-07-full-rag-blueprint-strict.md`
**Blueprint reference:** `docs/Modern_RAG_Architecture_Blueprint.md`
**Status:** Draft — approved to implement after review.

---

## 1. Problem statement

Current retrieval in `crates/chat/src/chat/service/job.rs::classify_message` embeds the user prompt once and executes two flat cosine searches against `knowledge_index`:

1. `search_capabilities(embedding, allowed_capabilities, 6)` — returns top-6 capability rows.
2. `search_context(embedding, 5)` — returns top-5 rows of any `source_type`.

The full 83-doc corpus is treated as a **single flat index**. This creates four concrete failure modes visible in production traces:

| Failure mode | Root cause | Observable symptom |
|--------------|-----------|--------------------|
| Off-domain false positive | Deferred domain doc (loan / accounting / tax) scores > `min_floor` but a savings capability scores slightly higher → executes savings SQL for a loan question. | Wrong data returned confidently. Guarded ex-post by `context_overrides_capability`, but the guard has to compare two arbitrary scores and can miss subtle cases. |
| Layer collision | A schema doc kaya kata kunci ("m_savings_account_transaction") outscores the intended capability. | Capability confidence artificially low → clarify triggered on unambiguous prompts. |
| No per-aspect routing | A prompt like "top 10 offices by client activation last quarter" carries three signals (topic, quantity, date range) that all embed into one vector. | Vector search cannot weight signals independently; irrelevant tokens (last quarter) dilute topic signal. |
| No cheap short-circuit | Every prompt pays 1 embedding call + 2 SQL queries regardless of how obviously it belongs to a deferred domain. | Wasted Voyage tokens; deferred-domain unsupported is expensive. |

The blueprint's flat "Hybrid Retrieval" (vector + BM25 + graph + metadata) mitigates the first three but does not exploit the **inherent hierarchy** of the knowledge catalog: data_area → domain → capability → query. LQR overlays that hierarchy on top of hybrid retrieval.

## 2. Definitions

**LQR (Layered Query Retrieval).** Retrieval executed in strict order across N tiers, each tier's output filtering the candidate set of the next. Every layer produces a decision: `Winner(id, score) | Ambiguous(top_k) | Reject`. Reject short-circuits.

**Layer.** A logical partition of `knowledge_index` by `source_type` (or a functional grouping of source types). Each layer answers exactly one question.

**Layer plan.** A per-layer query fragment produced by the Retrieval Planner. Each layer sees only the query fragment relevant to its question.

**Confidence budget.** Total confidence loss allowed before short-circuit to Clarify. Set from `classification.yaml`.

## 3. Layer taxonomy

Four executable layers plus one implicit governance filter:

```
Layer 0 (implicit): data_area status filter
    → drop candidates whose data_area is deferred|rejected before scoring

Layer 1: DOMAIN
    source_type = 'domain'
    Question: "What subject area is this prompt about?"
    Output: domain_winner OR Reject (off-domain short-circuit)

Layer 2: CAPABILITY
    source_type = 'capability'
    filter: domain = domain_winner AND source_id ∈ allowed_capabilities
    Question: "Given the subject, which capability answers this?"
    Output: capability_winner OR Ambiguous(clarify_options) OR Reject

Layer 3: QUERY
    source_type = 'query'
    filter: query_id = capability_winner.query_id
    Question: "Does the wired-up SQL cover the requested output_mode + params?"
    Output: query_metadata OR Reject (schema mismatch)

Layer 4: PARAM/RESOLVER (from existing plan Task 3)
    Not a knowledge_index layer; consumes resolved constraints
    Output: ExecutionPlan ready for policy + executor
```

Note: schema, metric, policy, response documents are **not** layered — they are consulted opportunistically during answer planning and rendering, not during routing. Adding them to LQR is a future upgrade only if we observe retrieval loss.

## 4. Per-layer query fragments

The LLM Retrieval Planner (Task 4 of the existing plan) is modified to emit **layer-scoped fragments** instead of one flat retrieval plan. The prompt schema becomes:

```json
{
  "layers": {
    "domain": "client activation trend",
    "capability": "monthly count of client activations",
    "query": "monthly_breakdown output for client onboarding",
    "keyword": "client activation monthly onboarding",
    "graph_hint": "client -> activation_date -> office"
  },
  "confidence": 0.87
}
```

The planner is prompted with the **known layer taxonomy** and asked to distil the user prompt into 4 short fragments — one per executable layer + one for keyword pass. This is what makes LQR precise: irrelevant tokens (dates, limits, currency) go into the resolver's structured constraints, not into the topic fragments.

## 5. Layer execution

Each layer runs a hybrid retrieval (vector + BM25 + metadata) restricted to its source_type + filter set, and applies its own decision policy.

### Layer 1 — Domain

```
Inputs: layer_plan.domain, embed(layer_plan.domain)
Filter: source_type = 'domain'
Retrieve: top-3 by hybrid score
Decision:
  if top.confidence < DOMAIN_MIN_FLOOR (default 0.55):
      → Reject(reason='no_domain_match')
  if top.domain.status == 'deferred' or 'rejected':
      → Reject(reason='off_domain', domain=top.id)
  if (top.confidence - second.confidence) < DOMAIN_MIN_GAP (default 0.10):
      → Ambiguous(top_3_domains)  # rare — currently 7 domains
  else:
      → Winner(domain_id=top.id, confidence=top.confidence)
```

Reject at Layer 1 short-circuits — no Layer 2/3 vector call, no LLM answer generation. Off-domain becomes O(1 embed + 1 SQL) instead of O(1 embed + 2 SQL + 1 LLM).

### Layer 2 — Capability

```
Inputs: layer_plan.capability, embed(layer_plan.capability), domain_winner
Filter: source_type='capability'
      AND metadata.domain = domain_winner
      AND source_id ∈ allowed_capabilities
Retrieve: top-6 by hybrid score
Decision (reuses existing gap-based logic):
  if top.confidence < CAP_MIN_FLOOR (default 0.40):
      → Reject(reason='no_capability_match')
  if (top - second) < CAP_MIN_GAP (default 0.05):
      → Ambiguous(top_3_capabilities)
  else:
      → Winner(capability_id=top.id, confidence=top.confidence)
```

Ambiguity at Layer 2 triggers the clarification flow with capability options — **exactly the current behavior** but filtered by domain first, so options never span domains.

### Layer 3 — Query

```
Inputs: capability_winner
Filter: source_type='query' AND source_id = capability_winner.query_id
Retrieve: fetch metadata row directly (no scoring — it's identity lookup)
Decision:
  if query.output_mode != capability.output_mode:
      → Reject(reason='schema_mismatch', internal error)
  if resolver requires params query does not accept:
      → Reject(reason='unsupported_params')
  else:
      → Winner(query_metadata)
```

Layer 3 is not fuzzy — it's a wiring check. Its role in LQR is to catch catalog drift (capability YAML edited without query YAML).

## 6. Score aggregation

Because each layer executes independently, we track a **per-layer confidence trace**:

```
final_confidence = min(domain_conf, capability_conf)
```

Using `min` (rather than mean or product) means a strong capability match with a weak domain score cannot mask an off-domain false positive. This is intentional — LQR trades some average-case confidence for tail-case correctness.

This trace is persisted to `chat_jobs.state_json.classification.layers`:

```json
"classification": {
  "outcome": "matched",
  "capability": "client_activation_monthly_breakdown",
  "confidence": 0.71,
  "layers": [
    { "layer": "domain",     "winner": "client",  "confidence": 0.81, "candidates": [...] },
    { "layer": "capability", "winner": "client_activation_monthly_breakdown", "confidence": 0.71, "candidates": [...] },
    { "layer": "query",      "winner": "client.activation_monthly_breakdown", "confidence": 1.00 }
  ]
}
```

This trace is **the** debugging surface for retrieval drift. When a prompt goes wrong, you look at which layer said what.

## 7. Interaction with existing hybrid retrieval + reranker

LQR is orthogonal to Task 4's hybrid retrieval implementation. Each layer's retrieval call *is* a hybrid call (vector + BM25 + metadata + reranker), just scoped by `source_type` filter. Concretely:

- Task 4 defines `search_hybrid(pool, source_type_filter, keyword, embedding, metadata_filter, limit)`. LQR calls this three times with different filters, not three separate implementations.
- Reranker (cross-encoder) runs per layer if enabled, since the candidate set differs.
- BM25 keyword pass uses `layer_plan.keyword` at every layer (same lexical fragment across layers is fine — it's cheap).

## 8. Interaction with Retrieval Planner (existing plan Task 4)

The existing plan Task 4 Step 2 (`build_retrieval_plan`) produces a single `RetrievalPlanStrict`. **LQR requires the planner to emit a `LayeredRetrievalPlan`** with per-layer fragments (§4). The struct change is additive — old callers can be dropped or shimmed.

`LlmClient::plan_retrieval` prompt is updated to include the layer taxonomy as a hard schema requirement. Prompt template excerpted in Task LQR-B.

## 9. Adaptive retry (interaction with Evidence Evaluator, existing plan Task 5)

When Evidence Evaluator returns `enough=false`:

1. First retry: rerun **Layer 2 only** with a relaxed `layer_plan.capability` fragment (LLM generates a broader variant). Domain winner from first pass is trusted.
2. Second retry: rerun Layer 1 as well (broader `layer_plan.domain`). This catches semantic parser mistakes.
3. Third retry: give up → `Unsupported(reason='evidence_insufficient_after_retry')`.

Rerunning only what's needed = cheaper than flat retrieval retry.

## 10. Configuration

Add to `knowledge/policies/classification.yaml`:

```yaml
# Existing:
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Others — let me describe it in my own words"

# New for LQR:
lqr:
  domain_min_floor: 0.55
  domain_min_gap: 0.10
  capability_min_floor: 0.40    # reuses old min_floor
  capability_min_gap: 0.05      # reuses old min_gap
  retry_budget: 2
  score_aggregation: min        # min | mean | product
```

Loading updates `ClassificationPolicy` in `crates/chat/src/knowledge/model.rs`.

## 11. Failure modes & explicit non-goals

**In scope:**
- Off-domain short-circuit at Layer 1 (main win).
- Domain-scoped capability disambiguation (removes cross-domain confusion in clarify options).
- Per-layer audit trace.
- Adaptive retry with rerun-what-changed.

**Explicit non-goals for this overlay:**
- Cross-layer reranking (a global cross-encoder over all 83 docs). Blocked by cost; revisit if precision drops.
- Graph search over YAML links. Deferred to `pipeline/retrieval.rs::graph_evidence` in the existing plan — LQR does not require it.
- Layered embedding — every source_type still uses the same Voyage `voyage-2` model with `input_type=query` at retrieval time. Per-layer specialised embeddings would double sync cost.

## 12. Test strategy

Unit (in `crates/chat/src/chat/pipeline/lqr.rs`):
- Layer 1 domain reject when top domain is deferred.
- Layer 1 domain reject when top confidence < floor.
- Layer 2 capability filtered by `allowed_capabilities` correctly excludes unallowed matches.
- Layer 2 ambiguity when gap < min_gap emits clarification options ordered by confidence.
- Layer 3 query mismatch produces schema_mismatch reject (catalog validator gap catcher).
- `final_confidence` uses `min` across layers.
- Adaptive retry rerun-what-changed budget respected.

Integration (in `crates/chat/tests/`):
- Prompt "loan disbursement this month" → Layer 1 reject at reason=`off_domain`, no Layer 2 vector call (assert via mock repository call count).
- Prompt "top 5 offices by client activation this month" → Layer 1 winner=`client`, Layer 2 winner=`client_activation_top_n_offices`, Layer 3 pass, executes.
- Prompt "banana republic" → Layer 1 reject reason=`no_domain_match`.
- Prompt in Bahasa "aktivasi klien bulan ini" → Layer 1 winner=`client` (synonym match via existing domain concept synonyms in retrieval_text).

## 13. Rollout / feature flag

Add env `LQR_ENABLED=true|false` (default false during Phase 20 rollout).

- `classify_message` in `job.rs`: if `LQR_ENABLED=false`, existing flat flow runs unchanged.
- If `LQR_ENABLED=true`, LQR orchestrator replaces `search_capabilities` + `search_context` + `context_overrides_capability` block.
- Both paths share the downstream `classify_retrieved_capability` → `ExecutionPlan` → policy → executor stages.

Once integration tests + a real-DB smoke pass, flip default and eventually delete the flag.

## 14. Success criteria

Before deleting `LQR_ENABLED=false` path:

- All existing integration tests (`chat_full_flow`, `chat_no_loop`, `user_journeys_real_db`) pass with LQR on.
- New `crates/chat/tests/lqr_layered_retrieval.rs` scenarios (13 test cases per §12) pass.
- `POST /vector-index/rebuild` on prod-shaped catalog + issuing 20 canned prompts shows:
  - 0 off-domain false positives.
  - No regression in match confidence for in-domain queries (mean drop < 0.05).
  - Layer 1 short-circuit trims ≥ 40% cost on off-domain prompts (measure via `state_json.classification.layers` — Layer 2 present iff Layer 1 winner).
