# Full RAG Blueprint Strict Design

## Goal

Make `docs/Modern_RAG_Architecture_Blueprint.md` the authoritative runtime flow for chat reporting.

The system must orchestrate every stage. LLMs reason inside bounded contracts. Vector search retrieves knowledge evidence only. Fineract transactional data remains accessible only through approved catalog SQL after policy checks.

## Non-Goals

- Do not let the LLM generate SQL.
- Do not index Fineract transactional rows in pgvector.
- Do not add new crates. Keep work inside `app`, `core`, and `chat`.
- Do not create speculative interfaces with multiple fake implementations.
- Do not preserve silent lexical/vector fallbacks in strict mode.

## Strict Runtime Flow

```text
User message
  -> Conversation Context
  -> LLM Semantic Parser
  -> Deterministic Intent Router
  -> Entity & Constraint Resolver
  -> Ambiguity Detector
  -> Retrieval Planner
  -> Vector Retrieval
  -> Keyword Retrieval
  -> Graph Retrieval
  -> Metadata Filter
  -> Hybrid Retrieval Merge
  -> Reranker
  -> Evidence Evaluator
  -> Answer Planner
  -> Policy Guard
  -> Approved SQL Executor
  -> LLM Answer Generator
  -> Grounded Response
```

Strict mode means every stage runs in this order. Existing deterministic shortcuts, such as classifying savings activity before semantic parsing, are removed from the main path.

## Configuration Rules

- `LLM_API_KEY` is required for chat reporting.
- `VOYAGEAI_API_KEY` is required for vector retrieval.
- Runtime retrieval requires the latest catalog version to be `embedded` and to match the loaded catalog content hash.
- If required config or embedded index is missing, the job fails with a sanitized operational error and logs the internal reason.
- Local tests can use fake clients only in test modules; production runtime must not silently downgrade.

## Conversation Context

Input to semantic parsing includes:

- Current user message.
- Session history needed to resolve follow-up replies.
- Previous job classification only when answering a clarification or follow-up.
- `ClientContext` capability scope, office scope, and PII permission.

Context must not include secrets, raw API keys, or unrestricted Fineract rows.

## LLM Semantic Parser

The first reasoning stage is an LLM structured-output call.

Output schema:

```json
{
  "intent": "report | clarification_answer | unsupported | tool_action",
  "domain": "savings | client | organization | unknown",
  "entities": [
    { "type": "capability_hint | product | currency | office | date_period", "value": "..." }
  ],
  "constraints": {
    "from_date": "YYYY-MM-DD or null",
    "to_date": "YYYY-MM-DD or null",
    "quantity": { "mode": "all | limit | top_n | default", "value": 10 },
    "currency_code": "string or null",
    "product_ids": null
  },
  "requires_retrieval": true,
  "confidence": 0.0
}
```

The parser extracts meaning only. It does not select SQL and does not choose executable capability by itself.

`all` is represented as `constraints.quantity.mode = "all"`. Range numbers such as `3 months` must not become row limits.

## Intent Router

Rust maps parsed intent to a workflow:

- `report` -> reporting retrieval pipeline.
- `clarification_answer` -> resolve against pending job context, then reporting retrieval pipeline.
- `tool_action` -> unsupported for now unless explicitly implemented later.
- `unsupported` or unknown -> fail with `unsupported_request` or ask clarification if recoverable.

Write-like requests remain blocked before retrieval execution.

## Entity And Constraint Resolver

The resolver converts parser output into normalized constraints:

- Date periods become `from_date` and `to_date`.
- `all` omits SQL limit for list capabilities.
- `limit` and `top_n` keep explicit numeric limits.
- Office scope is derived only from `ClientContext` and policy, not from LLM authority.
- Currency and product filters must match catalog/Fineract-owned values when available.
- Missing required date constraints trigger clarification before retrieval execution.

The resolver also records unresolved entities for ambiguity checks.

## Ambiguity Detector

Ambiguity is checked before retrieval execution.

Clarify when:

- Parser confidence is too low.
- Date period is missing for non-summary reports.
- Multiple intent shapes conflict, such as `all` and `top 10` in the same request.
- Domain/capability hints point to several incompatible outputs with no clear winner.

Do not execute when ambiguity remains.

## Retrieval Planner

The retrieval planner builds retrieval inputs from parsed intent and resolved constraints, not from raw prompt alone.

Planner output:

```json
{
  "vector_query": "normalized semantic phrase",
  "keyword_query": "important terms and synonyms",
  "graph_query": "domain -> capability -> query/data_area/schema",
  "metadata_filter": {
    "domain": "savings",
    "allowed_capabilities": ["..."],
    "output_mode": "list"
  }
}
```

The planner may use LLM reasoning, but Rust validates the shape and allowed fields.

## Retrieval Backends

### Vector Retrieval

Searches `knowledge_index` over the latest matching `embedded` catalog version.

Allowed source types:

- capability
- query
- domain
- data_area
- schema
- metric
- policy
- response

Vector results never execute directly.

### Keyword Retrieval

Use a deterministic BM25-style scorer over loaded catalog retrieval documents.

This replaces the current fallback-only lexical path. It is always part of hybrid retrieval.

### Graph Retrieval

Use catalog relationships as the graph:

- domain -> capability
- capability -> query
- query -> data_area
- query -> schema/table
- capability -> metric/policy/response

Graph search can start as a bounded one-hop/two-hop traversal. It must return evidence nodes, not execution authority.

### Metadata Filter

Apply filters after each retrieval pass and during merge:

- allowed capability ids
- domain
- status
- output mode
- data areas
- source type

Unauthorized capabilities must not reach reranking.

## Hybrid Merge And Reranker

Merge vector, keyword, graph, and metadata-filtered evidence into one candidate set.

Initial score:

```text
0.45 vector + 0.35 keyword + 0.15 metadata + 0.05 graph/freshness
```

If no cross-encoder is available, the deterministic weighted score is the reranker. This is acceptable as the first strict implementation because the reranker stage still exists and is testable.

## Evidence Evaluator

Before planning an answer, evidence must prove:

- At least one approved executable capability is supported.
- The selected capability maps to one approved query.
- Required data area, policy, and response evidence are present.
- Evidence does not contradict parser intent or resolved constraints.
- Catalog index hash matches the loaded catalog hash.

If evidence is weak:

- Clarify when user input can resolve it.
- Fail with `unsupported_request` when no approved evidence exists.
- Fail with operational error when index/catalog state is invalid.

Do not silently retry with a different flow in strict mode.

## Answer Planner

The answer planner produces the response sections and grounded output contract from evidence and query contract.

For `savings_activity_list`, the plan includes:

- coverage
- by-currency grouping
- rows
- weekly aggregation
- period aggregation
- rendered markdown message

`known_total_rows` remains `null` unless a cheap approved count query exists.

## SQL Execution And Policy

Policy guard remains mandatory before SQL execution.

SQL execution rules:

- Use only catalog-approved SQL files.
- Bind only resolved, validated params.
- Office scope is bound from policy decision.
- PII output requires `can_view_pii`.
- For list quantity `all`, bind `NULL` limit only when the query metadata marks `limit` optional.

## LLM Answer Generator

The final LLM call generates human prose only.

Input:

- User request.
- Parsed intent.
- Resolved constraints.
- Selected capability and answer plan.
- SQL result rows already filtered by policy.
- Structured response draft generated by Rust.

The LLM must return JSON containing:

```json
{
  "message": "grounded markdown",
  "citations": ["result.rows[0]", "answer_plan.coverage"]
}
```

Rust keeps the structured payload authoritative. If the LLM output conflicts with structured data or cites missing evidence, the job fails rather than returning invented prose.

## Grounded Response

The API response keeps the existing envelope:

```json
{ "success": true, "data": ..., "error": null }
```

Assistant message metadata contains:

- parser output
- routing decision
- resolver output
- retrieval plan
- retrieval evidence
- reranker scores
- evidence evaluation
- answer plan
- structured response
- generated message

## Migration From Current Implementation

Current shortcuts to remove or demote:

- `classify_savings_activity_list()` must not run before semantic parser.
- Lexical retrieval must not be fallback-only; it becomes one hybrid retrieval input.
- LLM planner fallback over clarification options must be replaced by parser/planner/generator calls.
- `ExecutionPlan.retrieval_plan` must become actual retrieval input, not just audit output.
- Deterministic formatter can build structured response, but LLM answer generator owns final prose in strict mode.

## Failure Modes

- Missing `LLM_API_KEY`: fail job with `pipeline_config_error`.
- Missing `VOYAGEAI_API_KEY`: fail job with `pipeline_config_error`.
- No embedded vector index: fail job with `vector_index_required`.
- Catalog hash mismatch: fail job with `vector_index_stale`.
- Weak evidence: clarify or unsupported, depending on recoverability.
- LLM malformed JSON: fail sanitized, log raw parser error internally.
- LLM unsupported capability or SQL attempt: fail sanitized and log policy violation.

## Testing Strategy

Unit tests:

- Semantic parser schema validation using fixture LLM responses.
- Quantity parsing: `all`, `limit 20`, `top 7`, `3 months`.
- Intent routing state machine.
- Constraint resolver date/limit/office behavior.
- Keyword scorer and graph traversal.
- Hybrid merge/reranker scoring.
- Evidence evaluator required-source and stale-index checks.
- LLM answer generator grounding validation.

Integration tests:

- Missing LLM config fails strictly.
- Missing embedded index fails strictly.
- `show me the list of all saving activity for this month` executes activity list with no default limit.
- Ambiguous prompt returns clarification.
- Off-domain prompt does not execute savings capability.
- Generated response message is grounded in structured SQL result.

Manual verification:

- `POST /vector-index/rebuild` returns `embedded` with current catalog hash.
- `GET /vector-index/status` shows current embedded catalog.
- Postman chat request records every blueprint stage in `state_json`.

## Rollout Notes

This is a core-flow change. Implement behind a strict pipeline path and switch chat reporting to it only when the full path passes focused tests. Do not keep old fallback behavior active in strict mode.
