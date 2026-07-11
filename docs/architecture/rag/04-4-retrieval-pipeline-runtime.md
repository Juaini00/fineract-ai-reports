# RAG Architecture: 4. Retrieval Pipeline (Runtime)

Source: `docs-old/rag-architecture.md`

## 4. Retrieval Pipeline (Runtime)

### 4.1 Position In The Request Flow

Authentication and scope checks happen **before** any retrieval, so unauthorized callers never spend embedding/LLM tokens. See `docs/ai-reporting-design.md` §3.

```text
request
  -> API key auth (core)
  -> ClientContext attached
  -> chat job created (chat::chat::service)
  -> deterministic write-intent guard
  -> if API key has allowed capabilities: embed query
  -> vector search latest catalog version capability/query rows scoped to allowed_capabilities
  -> catalog lexical fallback when embedding/vector search is unavailable
  -> choose one high-confidence capability or ask clarification from close candidates
  -> persist classification.source and classification.candidates in chat_jobs.state_json
  -> build structured retrieval_plan, evidence_evaluation, and answer_plan
  -> [retrieval planning ends here]
  -> policy guard (chat::policy::authorization)
  -> approved SQL execution
  -> response format
  -> audit
```

### 4.2 What Vector Search Returns

A ranked list of knowledge rows from `knowledge_index` that can resolve to approved capabilities. The runtime currently cares about:

- `source_type` and `source_id` (for example `capability:savings_deposit_total` or `query:savings.deposit_total`) — used to resolve the owning approved capability.
- `metadata_json` — used as derived catalog metadata. Capability rows are filtered by capability id, and query rows are mapped back through capability `query_id` before planning.
- Similarity score — feeds the planner's confidence calculation.
- Only the latest `knowledge_catalog_versions` row with status `embedded` or `indexed` is searched. Duplicate capability ids are collapsed before decision-making.

The planner does **not** receive raw SQL text; it receives a selected approved capability id and resolves the query through the typed catalog.

### 4.3 Confidence And Decision

The planner combines vector similarity, lexical/example overlap, and parameter completeness into a single confidence per candidate. The decision policy from `docs/ai-reporting-design.md` §8 then routes to:

- **execute** — confidence >= `0.55`, one clear capability, complete params, and policy guard passes.
- **clarify** — confidence from `0.40` to `0.55`, close candidates within `0.05`, ambiguous capability, or missing required params (`from_date`, `to_date`, etc.).
- **unsupported** — confidence < `0.40`, no approved capability matches, or the request asks for excluded data / write / arbitrary SQL. The job is marked `failed` with `unsupported_request`; it must not remain queued.

A vector-retrieved capability is still a candidate. The policy guard in `crates/chat/src/policy/authorization.rs` is the final gate before execution.

When clarification options are returned, the user can answer with the option text, the capability id, or a 1-based option number such as `1` or `2`.

### 4.4 Where The LLM Provider Fits

The LLM provider is **not** part of the retrieval store. It is a planner fallback when the local classifier's confidence is low, and later a formatter for natural-language responses over the structured SQL result. The current/default provider is DeepSeek, but the client uses an OpenAI-compatible chat-completions contract. It receives:

- The user message.
- The top-k retrieved capability/domain descriptors (descriptions + example phrases + parameter schemas) — never raw SQL, never raw Fineract rows beyond the approved query output contract.
- `ClientContext` capability scope so it cannot recommend a capability the caller is not allowed to run.

The LLM can return only one of: a `capability_id` choice with extracted parameters, a clarification question, or an `unsupported` verdict. It cannot author new SQL.

Current implementation is narrower: the LLM planner fallback is called only after Rust has already produced a clarification with approved options. It receives the user message and those options, and any returned capability must be one of the provided option capability ids.
