# LLM Extraction Gateway — Design

**Status:** proposed (for review).
**Date:** 2026-07-24.
**Owner branch:** to be created after approval.

## 1. Goal

Establish the LLM as the single **extraction gateway** for every user request,
producing a schema-constrained structured payload that a deterministic
downstream resolver consumes to fill defaults, apply business-date semantics,
and decide when clarification is actually needed.

The gateway must eliminate three classes of current defect:

1. Capabilities that logically require no time range (lookup / snapshot)
   nevertheless trigger `from_date` / `to_date` clarification.
2. Capabilities that logically want an unbounded result set are truncated to a
   default `limit`, or worse, ask the user for a row cap.
3. "Today" resolves to `Utc::now()` instead of the tenant's **business date**,
   which is the only correct notion of "today" for banking data.

## 2. Non-goals

- Not implementing a knowledge-approval workflow.
- Not replacing the semantic router / capability catalog; the gateway feeds it.
- Not changing `/management/*` observability endpoints.
- Not adding a general-purpose expression sandbox — the default-expression
  language is an allowlisted DSL, not user-evaluable code.
- Not implementing a full working-days / calendar capability in this spec — it
  is called out as a downstream capability that becomes trivially expressible
  once the gateway is in place.

## 3. Architecture

Three sequential layers with tight, testable contracts:

```
User message
     │
     ▼
┌──────────────────────────┐
│ Layer 1: LLM Gateway     │  ← schema-constrained JSON output
│  (extract + suggest)     │
└──────────┬───────────────┘
           ▼
┌──────────────────────────┐
│ Layer 2: Deterministic   │  ← YAML policy, BusinessDateProvider,
│  Resolver                │    entity canonicalization
└──────────┬───────────────┘
           ▼
┌──────────────────────────┐
│ Layer 3: Clarification   │  ← ask only if truly missing
│  Decider                 │
└──────────┬───────────────┘
           ▼
     Execute or Ask
```

Key discipline: **LLM reports, deterministic decides.** The LLM never invents
absolute values (dates, IDs, limits); it only surfaces what the user said,
suggests candidate capabilities with confidence, and quotes the phrase span
that motivates each hint.

## 4. Layer 1 — LLM Gateway output contract

### 4.1 Output schema

The LLM is called with a JSON-schema-constrained tool. Output is a single
object of type `LlmGatewayExtraction`:

```jsonc
{
  "intent_kind": "report_request" | "data_lookup" | "clarification_reply"
                | "follow_up" | "greeting" | "help"
                | "out_of_domain" | "unsupported_in_domain" | "unsafe_request",

  "domain": "savings" | "client" | "loan" | "organization" | "accounting"
          | "audit" | "tax" | "group_center" | "unknown",

  "language": "id" | "en" | "mixed" | "unknown",

  "entities": [
    { "type": "person_name" | "office" | "currency"
             | "product" | "metric" | "account_number" | "capability_hint",
      "value": "<verbatim substring from user text>",
      "phrase_span": [start, end] }
  ],

  "temporal_hint": {
    "phrase": "<verbatim substring>",
    "phrase_span": [start, end],
    "inferred": "today" | "yesterday" | "this_week" | "last_week"
              | "this_month" | "last_month" | "this_year" | "last_year"
              | "recent" | "as_of_now" | "range" | "none",
    "range_hint": {                     // present only when inferred == "range"
      "from_phrase": "<substring>",
      "to_phrase":   "<substring>"
    },
    "confidence": 0.0–1.0
  } | null,

  "quantity_hint": {
    "phrase": "<verbatim substring>",
    "phrase_span": [start, end],
    "inferred": "all" | "top_n" | "limit" | "default",
    "value": <integer or null>,          // required when inferred in {top_n, limit}
    "confidence": 0.0–1.0
  } | null,

  "candidates": [
    { "capability_id": "<stable id from the summary catalogue>",
      "confidence": 0.0–1.0,
      "why": "<one-sentence justification quoting user phrasing>" }
  ]
}
```

### 4.2 What the LLM sees

- The user message (current turn).
- A **summary catalogue** of visible capabilities: `id`, `display_name`,
  `description`, one-line `use_when`, `unsupported_intents`. No SQL, no
  parameter internals, no PII policy.
- The recent turn history summary (already assembled by the context builder).
- The stable list of enum values above.

The LLM never sees: raw SQL, catalog YAML paths, per-parameter defaults, the
BusinessDateProvider result, other users' data.

### 4.3 What the LLM must never do

- Emit an absolute date (`"2026-07-24"`) unless the user typed it verbatim.
- Emit a `capability_id` not in the catalogue.
- Emit values for `entities.value` that do not appear in the user text.
- Guess office IDs, account numbers, or amounts.

The Layer-2 resolver rejects any output that violates these rules and treats
it as a Layer-1 failure (see §9).

### 4.4 Model call semantics

- Temperature 0 for extraction (deterministic-as-possible from LLM).
- JSON-schema-forced decoding (already in scope via the existing `schemars`
  contracts and `TracedLlmClient`).
- One retry on schema-validation failure with the same prompt; a second
  failure classifies as `layer1_extraction_failed` (see §9).

## 5. Layer 2 — Deterministic Resolver

### 5.1 Capability YAML changes (per-parameter policy)

Today, `required_parameters` / `optional_parameters` / `clarification.missing_parameters`
are three lists at the capability root. This is replaced with a **per-parameter
policy block** colocated under `parameters:`:

```yaml
parameters:
  from_date:
    type: date
    required: false
    default: business_today                # expression, see §5.2
    fill_when_missing: true                # auto-fill silently
    user_may_override: true

  to_date:
    type: date
    required: false
    default: business_today
    fill_when_missing: true
    user_may_override: true

  limit:
    type: integer
    required: false
    default: unbounded                     # → Quantity::All downstream
    hard_cap: 10000
    user_may_override: true

  office_ids:
    type: integer_array
    required: false
    default: authorized_scope              # → intersected with caller scope
    user_may_override: false               # scope narrowing only, never widen
```

Rules the loader enforces at startup (fail-fast in `KnowledgeValidator`):

- Every parameter that appears in the query's `required_parameters` must have
  either `required: true` **or** a `default` expression.
- `hard_cap` is only valid on `integer` / `integer_array` typed parameters.
- `default` expression must parse against the whitelist in §5.2.
- `office_ids.user_may_override` MUST be `false`.
- No two parameters may reference the same output SQL slot.

Legacy `clarification.missing_parameters` is deprecated (see §11 migration).

### 5.2 Default-expression DSL (allowlist only, not sandboxed)

Recognized tokens, parsed into typed nodes at YAML load time:

| Expression | Meaning |
|---|---|
| `business_today` | `BusinessDateProvider.today(tenant)` |
| `wall_today` | `Utc::now().date_naive()` |
| `business_today - 1d` … `- 366d` | date arithmetic |
| `business_today - 1m`, `- 1y` | calendar month/year subtraction |
| `start_of_month(business_today)` | first day of that month |
| `end_of_month(business_today)` | last day of that month |
| `unbounded` | `Quantity::All` |
| `authorized_scope` | caller's `allowed_office_ids` |
| literal ISO date `"YYYY-MM-DD"` | as-is |
| literal integer | as-is |

Anything else fails validation. No arbitrary expressions, no user variables,
no arithmetic on integers, no function composition beyond the table.

### 5.3 `BusinessDateProvider`

New service in `crates/chat/src/assistant/temporal/business_date.rs`:

```rust
#[async_trait]
pub trait BusinessDateProvider: Send + Sync {
    async fn today(&self, tenant: &TenantId) -> Result<BusinessDate, BusinessDateError>;
}

pub struct BusinessDate {
    pub date: NaiveDate,
    pub source: BusinessDateSource,   // Fineract | WallClockFallback
    pub resolved_at: DateTime<Utc>,
}
```

Concrete implementation reads from the Fineract read-replica in a single SQL
call that returns the configured business date for the tenant (specific table
identified during Phase-0 exploration; not this spec's job to guess).

Semantics:

- **Request-scoped cache.** Called at most once per chat job run; result
  passed through the graph runtime context.
- **Fallback to wall clock is explicit.** If the SQL returns null / times out
  / errors, the provider returns `source: WallClockFallback` and the resolver
  **must** enqueue a `business_date.fallback_used` audit event (Phase 2.3
  outbox pattern).
- **Never used for audit timestamps.** `occurred_at`, LLM trace `created_at`,
  outbox `created_at` all stay on `Utc::now()`. Business date is a domain
  concept, not a system clock.

### 5.4 Resolution steps (in order)

For each parameter in the selected capability:

1. **User-typed value?** — accept if it type-checks and passes `hard_cap`.
2. **LLM-hinted value?** — if `temporal_hint.confidence >= 0.7`, resolve
   against `business_today` using the fixed mapping table below and accept.
   Below the threshold, discard the hint.

   | `temporal_hint.inferred` | `from_date` | `to_date` |
   | --- | --- | --- |
   | `today` / `as_of_now` | `business_today` | `business_today` |
   | `yesterday` | `business_today - 1d` | `business_today - 1d` |
   | `this_week` | Monday of week | `business_today` |
   | `last_week` | Monday of prev week | Sunday of prev week |
   | `this_month` | `start_of_month(business_today)` | `business_today` |
   | `last_month` | `start_of_month(business_today - 1m)` | `end_of_month(business_today - 1m)` |
   | `this_year` | Jan 1 of business year | `business_today` |
   | `last_year` | Jan 1 of prev year | Dec 31 of prev year |
   | `recent` | `business_today - 1d` | `business_today` |
   | `range` | resolve `range_hint.from_phrase` and `range_hint.to_phrase` through the same table; if either does not map, discard the hint | (same) |
   | `none` | do not fill from hint; proceed to step 3 | (same) |
3. **YAML default expression?** — evaluate against
   `(BusinessDateProvider, caller_scope, tenant)` and accept.
4. **Otherwise** — the parameter is unfilled; it becomes an input to Layer 3.

Every filled parameter records its `PayloadSource` (`user_text`, `llm_claim`,
`catalog_default`, `business_date`, `authorized_scope`) so the audit trail can
distinguish user intent from system inference.

### 5.5 Candidate capability selection

Deterministic post-processing of Layer-1 `candidates`:

1. Drop candidates whose `capability_id` is not in the visible catalogue.
2. Drop candidates whose `unsupported_intents` matches the extracted
   `intent_kind` or `request_shape`.
3. Apply the existing `ClassificationPolicy` (score floor + gap) to decide
   Match / Clarify / Unsupported — reuses `decide_from_scores` unchanged.

## 6. Layer 3 — Clarification Decider

Ask if and only if all three hold:

- Layer-2 resolver still has one or more parameters marked
  `required: true` with no fill from user text, LLM hint, or default.
- OR the classifier outcome is `Clarify` (multiple viable capabilities within
  score gap).
- OR the extracted `intent_kind` is `unsafe_request` or `unsupported_in_domain`
  (route to sanitized rejection, not clarification).

The decider produces one of three outcomes:

- `Execute { capability_id, resolved_parameters }`
- `Clarify { question, options, missing_fields }` — same wire format as today
- `Reject { code, sanitized_message }`

Never ask a parameter question when a `default` exists. Never ask
`from_date`/`to_date` for a capability whose `parameters` block does not
declare them.

## 7. Worked examples

Applied to the user's cases from the discussion:

| User request | LLM extracts | Resolver fills | Decision |
|---|---|---|---|
| "client mana saja yang belum bayar hutang" | `intent=data_lookup`, `temporal.inferred=as_of_now`, `quantity.inferred=all`, candidate `loan_arrears_clients` | `as_of=business_today`, `limit=unbounded` | Execute — no ask |
| "tampilkan client yang hari ini membayar" | `temporal.inferred=today`, `quantity.inferred=default`, candidate `loan_repayments_today` | `paid_date=business_today`, `limit=default_from_yaml` | Execute — no ask |
| "siapa saja yang di-assign weekly fee di savings" | no temporal, no quantity, candidate `savings_fee_assignments` (metadata capability, zero date params) | none needed | Execute — no ask |
| "loan interest yang baru diposting" | `temporal.inferred=recent`, candidate `loan_interest_recent` | `posted_after=business_today-1d, posted_before=business_today` | Execute — no ask |
| "hari kerja hari apa saja" | `intent=data_lookup`, candidate `office_working_days` (metadata capability) | none | Execute — no ask |
| "top 10 offices bulan lalu" | `temporal.inferred=last_month`, `quantity.inferred=top_n value=10`, candidate `organization_office_activity_ranking` | `from=start_of_month(prev), to=end_of_month(prev), limit=10` | Execute — no ask |
| "deposits" (bare word) | no temporal, no quantity, multiple candidates within gap | nothing to auto-fill | Clarify — capability picker |

## 8. Data flow and job memory

- `LlmGatewayExtraction` persists in `job_memory.state_json.llm_extraction`.
- `ResolvedRequest` persists in `job_memory.state_json.resolved_request` with
  per-parameter `PayloadSource`.
- On clarification reply, the job continues (existing pattern — no new job).
  The reply is fed back to Layer 1 with the prior extraction as context so
  the LLM can reconcile the user's answer.

## 9. Failure semantics and audit

| Failure | Handling | Audit event |
|---|---|---|
| LLM schema-invalid after 1 retry | Return sanitized 500 to user, do not fall back to raw LLM text | `llm_extraction.failed` (existing `chat.job_failed`) |
| LLM emits `capability_id` not in catalogue | Drop that candidate silently | `llm_gateway.candidate_dropped` (telemetry only) |
| LLM emits `entities.value` not in user text | Drop that entity | `llm_gateway.entity_dropped` |
| BusinessDateProvider fails | Fallback to wall clock, resolver marks affected parameters `source: wall_clock_fallback` | `business_date.fallback_used` |
| YAML default expression evaluates to invalid value (past `hard_cap`, negative date, etc.) | Fail loudly at startup (KnowledgeValidator), never at runtime | n/a |
| Required parameter unfilled after resolver | Layer-3 Clarify | `chat.clarification_requested` (existing) |

Every failure produces a sanitized user-visible message. Raw LLM text, raw
SQL errors, and stack traces are never surfaced.

## 10. Testing strategy

- **Unit tests** for the default-expression parser (whitelist / rejection).
- **Unit tests** for the resolver: user-typed vs LLM-hint vs YAML default
  precedence, including per-parameter `PayloadSource` labeling.
- **Fixture-driven tests** for Layer-1 output schema conformance using canned
  LLM responses.
- **End-to-end scenario tests** covering every row in §7, asserting the
  observed decision (Execute vs Clarify) and the audit events emitted.
- **YAML validator tests** for the new parameter policy block on every
  existing capability under `knowledge/capabilities/`.
- **BusinessDateProvider tests** with a fake provider that toggles between
  Fineract-source and fallback modes, asserting audit-event emission on
  fallback.
- **Regression guard** for the current clarification tests — no capability
  that previously asked for `from_date` / `to_date` may keep doing so unless
  its `parameters` block explicitly marks those parameters `required: true`
  with no default.

## 11. Migration plan

Backwards compatibility is not required (this feature ships in one branch).
Steps:

1. Add the new `parameters:` policy block schema to `KnowledgeCatalog` model
   and `KnowledgeValidator`. Old `required_parameters` / `optional_parameters`
   / `clarification.missing_parameters` are read once during migration and
   erased.
2. Auto-migrate existing YAML files: any parameter in the old required list
   becomes `required: true`; any date parameter with no evidence of a
   historical range in the query gets `default: business_today`; any `limit`
   with no evidence of true row-cap intent gets `default: unbounded`.
3. Manual review pass on each capability to correct the auto-migration.
4. Wire `BusinessDateProvider` into the graph runtime context.
5. Replace the existing extraction call path in
   `crates/chat/src/assistant/understanding/extraction/` with the new
   Layer-1 gateway. Deterministic temporal/quantity extractors become
   validators of the LLM output rather than primary extractors.
6. Update the classifier call site to consume `LlmGatewayExtraction.candidates`.
7. Update the clarification decider to obey `parameters[].required` and skip
   filled defaults.

## 12. Open questions (to resolve during planning, not now)

- Exact Fineract table / column for tenant business date (Phase-0 lookup).
- Whether we cache `BusinessDate` across concurrent jobs for the same tenant
  or per-request only. Per-request is simpler; per-tenant with short TTL is
  faster but couples correctness to cache invalidation.
- Whether to expose a `default: today_or_wall` (business date if available,
  wall-clock silently) as a first-class expression or force explicit fallback
  observability. Bias toward the explicit form.
- Whether `entity.type=office` should trigger office-name lookup in Layer 2
  or remain a hint until execution binds it. Bias toward lookup at Layer 2
  because the resolver already has the pool.

## 13. Acceptance

Design is accepted when the user confirms:

- The three-layer split and the LLM's "report, don't decide" discipline.
- The per-parameter policy block replacing the three legacy lists.
- The default-expression whitelist as scoped (no more expressions added
  without a follow-up spec).
- `BusinessDateProvider` as a first-class service with explicit fallback
  observability.
- The seven §7 examples as the working correctness bar.
