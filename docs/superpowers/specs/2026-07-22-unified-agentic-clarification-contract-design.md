# Unified Agentic Clarification Contract Design

## Status

Proposed source-of-truth design for
[`005 — Unified agentic clarification contract and job-scoped workflow`](../../issues/active/005-unified-agentic-clarification-contract.md).
Implementation must follow the companion plan and preserve the already-landed
clarification-continuation semantics.

## Goal

Make every clarification a versioned, machine-renderable, job-scoped workflow.
The server must plan predictable follow-up inputs before presenting options, the
client must collect those inputs locally, and one final submission must either
execute or return only a genuinely new/invalid/conflicting clarification.

## Design principles

1. **Strict structure, flexible business meaning.** The protocol has a small
   closed set of interaction and field types; labels, help, options, and values
   remain catalog-driven.
2. **Backend owns workflow.** The LLM may understand text and rank candidate
   capabilities, but Rust/catalog metadata owns required inputs, defaults,
   validation, authorization, state transitions, and execution.
3. **Job-scoped authority.** A response addressed to a job is resolved only
   against that job's active clarification.
4. **One source, many projections.** Durable job result, SSE update, and
   persisted assistant message carry the same public clarification object.
5. **Verified facts only.** Structured UI answers enter canonical state as typed
   clarification observations rather than being converted back to prose and
   re-extracted.
6. **Minimum questions.** Known facts and approved defaults are applied before
   questions are planned. Shared missing inputs are not duplicated per option.
7. **Backward-compatible migration.** Existing `message`/`option_id` clients and
   historical response JSON remain readable during rollout.
8. **YAGNI.** Version 1 implements only interaction/field primitives required by
   approved capabilities. New primitives require a contract version change or
   backward-compatible enum addition plus safe client fallback.

## Scope

Version 1 covers:

- capability/report selection;
- collection of date range, integer limit, and text search inputs;
- report-level Others/free-text recovery;
- field help and option details;
- known-value retention and approved defaults;
- field-level validation errors;
- stale/duplicate/unauthorized response protection;
- durable recovery through `GET job`;
- legacy natural-language clarification replies;
- an offline clarification scenario corpus and coverage harness.

Version 1 deliberately does not add confirmation, date-only, office-selector,
currency-selector, product-selector, or generic form-builder behavior. Those
can be introduced when an approved capability has a real use case. Authorized
office ids remain derived from the principal and are never arbitrary form input.

## Current constraints preserved

- Exactly three crates: `app`, `core`, and `chat`.
- Route → service → repository → database layering.
- PostgreSQL is durable; Redis SSE state is only a live hint.
- Same-job continuation remains `POST /chat/jobs/{job_id}/responses`.
- Bearer admin identity remains authoritative; optional API key may only narrow
  office scope according to current policy.
- SQL remains approved, catalog-owned, and parameterized.
- English-only product copy remains in force.
- Markdown remains derived output.

## Architecture overview

```text
user message
  -> semantic route + bounded capability candidates
  -> verified/effective facts
  -> ClarificationPlanner
       candidate required inputs
       - known verified facts
       - approved defaults
       = missing user inputs
  -> job-scoped ActiveClarification
  -> public ClarificationView
  -> PostgreSQL job memory/result/message + SSE projection
  -> client local wizard
  -> one structured /responses submission
  -> submission contract validation + CAS
  -> typed clarification observations
  -> effective constraints
  -> guarded execution or a new revision
```

The design introduces three distinct contracts:

1. `ParameterInputKnowledge`: reusable catalog definition of how executable
   parameters map to a user input.
2. `ClarificationPayload`: private active workflow state persisted with the job.
3. `ClarificationView`: safe public projection embedded in
   `AssistantResponse`.

Keeping private state and public presentation separate prevents source intent,
internal reasoning, authorization material, or canonical provenance from
leaking to clients.

## Public response contract

### AssistantResponse extension

Add an optional clarification object while retaining legacy top-level options:

```rust
pub struct AssistantResponse {
    pub response_type: AssistantResponseType,
    pub title: Option<String>,
    pub message: String,
    // existing sections/table/cards/options/warnings/actions/evidence...
    #[serde(default)]
    pub clarification: Option<ClarificationView>,
}
```

Invariant:

```text
response_type == clarification  <=>  clarification.is_some()
```

All production clarification builders must satisfy this invariant. Tests and
contract validation reject implicit clarification responses that contain only
prose/actions.

Legacy `AssistantResponse.options` remains a deprecated projection of report
options during migration. It must never be used as workflow authority. For
`collect_fields` and `free_text`, it is empty.

### ClarificationView

```rust
pub struct ClarificationView {
    pub version: u16,                  // 1
    pub id: Uuid,
    pub revision: u32,
    pub kind: ClarificationKind,
    pub question: String,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
    #[serde(default)]
    pub options: Vec<ClarificationChoice>,
    #[serde(default)]
    pub allow_free_text: bool,
}

pub enum ClarificationKind {
    SelectOption,
    CollectFields,
    FreeText,
}
```

Validation rules:

| Kind | Required shape |
| --- | --- |
| `select_option` | at least one non-Others option; top-level fields may contain inputs shared by every option |
| `collect_fields` | at least one field; options empty |
| `free_text` | fields and report options empty; `allow_free_text=true` |

The public object contains no `source_intent`, attempt ceiling, retrieval scores,
authorized office ids, raw LLM reason, or SQL information.

### Clarification fields

```rust
pub struct ClarificationField {
    pub key: String,
    #[serde(rename = "type")]
    pub field_type: ClarificationFieldType,
    pub label: String,
    pub required: bool,
    #[serde(default)]
    pub value: Option<serde_json::Value>,
    #[serde(default)]
    pub default_value: Option<serde_json::Value>,
    #[serde(default)]
    pub help_text: Option<String>,
    #[serde(default)]
    pub validation: ClarificationValidation,
    #[serde(default)]
    pub errors: Vec<ClarificationFieldError>,
}

pub enum ClarificationFieldType {
    DateRange,
    Integer,
    Text,
}

#[derive(Default)]
pub struct ClarificationValidation {
    pub min_integer: Option<i64>,
    pub max_integer: Option<i64>,
    pub max_length: Option<u32>,
    pub max_range_days: Option<u32>,
}

pub struct ClarificationFieldError {
    pub code: String,
    pub message: String,
}
```

JSON values by field type:

```json
{"type":"date_range","value":{"from":"2026-07-01","to":"2026-07-31"}}
{"type":"integer","value":10}
{"type":"text","value":"Tony"}
```

Dates use strict ISO `YYYY-MM-DD`. A partial date range may prefill one side with
the other set to `null`. Fields fully satisfied by verified facts are omitted
from the clarification rather than shown as redundant read-only steps.

`default_value` appears only for a catalog-approved user-visible default. An
automatically applied approved default is omitted from fields and recorded in
canonical provenance. Version 1 does not expose a generic “Skip” control unless
the field is optional and has `default_value`.

### Clarification choices and help

```rust
pub struct ClarificationChoice {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub help: Option<ClarificationHelp>,
    #[serde(default)]
    pub fields: Vec<ClarificationField>,
}

pub struct ClarificationHelp {
    pub details: String,
    pub example: Option<String>,
    #[serde(default)]
    pub requirements: Vec<String>,
}
```

`fields` contains only inputs specific to that choice. Inputs with equivalent
key/type/validation across every non-Others choice are lifted to
`ClarificationView.fields` as shared fields.

Others has the stable id `others`, no fields, and means only “none of these
reports; let me describe another request.” Help/detail is metadata rendered
locally and is never submitted as an option.

### Example: options with conditional inputs

```json
{
  "response_type": "clarification",
  "title": "Additional information required",
  "message": "Choose a report and complete its required information.",
  "options": [
    {"id":"savings_deposit_total","label":"Savings Deposit Total","description":"..."},
    {"id":"savings_deposit_top_n","label":"Top Savings Deposits","description":"..."},
    {"id":"others","label":"Another report","description":"Describe a different report."}
  ],
  "clarification": {
    "version": 1,
    "id": "507f1f77-bcf8-4f9c-8ba0-95f190fc1122",
    "revision": 1,
    "kind": "select_option",
    "question": "Which report would you like?",
    "fields": [
      {
        "key": "date_range",
        "type": "date_range",
        "label": "Report period",
        "required": true,
        "value": null,
        "default_value": null,
        "help_text": "Select inclusive start and end dates.",
        "validation": {"max_range_days": 366},
        "errors": []
      }
    ],
    "options": [
      {
        "id": "savings_deposit_total",
        "label": "Savings Deposit Total",
        "description": "Total deposit amount and count for a period.",
        "help": {
          "details": "Combines all matching deposit transactions.",
          "example": "Total deposits this month.",
          "requirements": ["Report period"]
        },
        "fields": []
      },
      {
        "id": "savings_deposit_top_n",
        "label": "Top Savings Deposits",
        "description": "Largest deposit transactions for a period.",
        "help": {
          "details": "Orders matching deposits from largest to smallest.",
          "example": "Top 10 deposits this month.",
          "requirements": ["Report period", "Number of records"]
        },
        "fields": [
          {
            "key": "limit",
            "type": "integer",
            "label": "Number of records",
            "required": true,
            "value": null,
            "default_value": null,
            "help_text": "Maximum number of transactions to return.",
            "validation": {"min_integer": 1, "max_integer": 100},
            "errors": []
          }
        ]
      },
      {
        "id": "others",
        "label": "Another report",
        "description": "Describe a different report in your own words.",
        "help": null,
        "fields": []
      }
    ],
    "allow_free_text": false
  }
}
```

If the original request already resolves the date range, the shared
`date_range` field is absent. If `savings_deposit_top_n` has an approved
`default_limit`, `limit` is also absent and the default is recorded as an
`ApprovedDefault` fact.

## Client rendering algorithm

The client uses only `clarification.kind`, fields, and options:

```ts
function buildSteps(view: ClarificationView, selectedOptionId?: string) {
  if (view.kind === "free_text") return [{ kind: "free_text" }];
  if (view.kind === "collect_fields") return view.fields.map(fieldStep);

  const selected = view.options.find(option => option.id === selectedOptionId);
  return [
    { kind: "select_option", options: view.options },
    ...view.fields.map(fieldStep),
    ...(selected?.fields ?? []).map(fieldStep),
  ];
}
```

Rules:

- Before option selection, render option cards and local “View details.”
- After selection, calculate steps from shared plus selected fields.
- Show progress only when `steps.length > 1`.
- Back/continue/help do not call the server.
- If selected option has no fields and no shared fields, submit on explicit
  confirmation/click according to client UX.
- Disable final submit while required values are absent or locally invalid.
- Server validation remains authoritative.
- On page refresh, rebuild from `GET /chat/jobs/{id}`.
- Historical messages render the same structure read-only; controls are active
  only when durable job status is waiting and id/revision match.
- Unknown kind/type displays safe generic copy and triggers durable recovery;
  it never guesses a component or submits hidden defaults.

## Private active clarification state

Evolve the internal `ClarificationPayload` into the persisted private workflow
state:

```rust
pub struct ClarificationPayload {
    pub version: u16,
    pub id: Uuid,
    pub revision: u32,
    pub kind: ClarificationKind,
    pub question: String,
    pub fields: Vec<ClarificationField>,
    pub options: Vec<ClarificationOption>,
    pub attempt: u32,
    pub source_intent: Option<SourceIntentSnapshot>,
    pub allow_free_text: bool,
    pub is_missing_execution_parameters: bool,
}
```

Internal `ClarificationOption` additionally retains capability identity and the
safe presentation metadata needed to create `ClarificationChoice`. It must not
retain authorization as a substitute for rechecking the current principal.

Lifecycle:

- A new ambiguity creates a new UUID, revision `1`, attempt `1`.
- Field validation errors retain the UUID and increment revision.
- An unresolved semantic reply increments attempt and revision.
- A changed candidate set/new request creates a new UUID.
- Accepted submission, execution completion, explicit supersession, bounded
  recovery, or cancellation clears the active payload.
- Historical revisions remain in checkpoints/messages/audit, not in the active
  column.

### Job-scoped persistence

Add `pending_clarification_json JSONB NULL` to `assistant_job_memory` and
`pending_clarification: Option<ClarificationPayload>` to `JobMemory`.
`JobMemoryRepository::{create,get,save}` reads/writes it under the existing
optimistic revision check.

During migration, `assistant_session_memory.pending_clarification_json` may be
updated as a compatibility/context projection, but `ContextBuilder` and response
resolution must take the active job payload supplied by `JobService`. Session
state is never authoritative for `/jobs/{job_id}/responses`.

`run_graph_skeleton` loads job memory before building the context and injects
that job's pending payload into the `ContextWindow`. This isolates simultaneous
jobs in one session without imposing a new “one active job per session” rule.

## Catalog input contract

### New reusable parameter registry

Add `knowledge/parameters/*.yaml` loaded into
`KnowledgeCatalog.parameter_inputs` as typed `ParameterInputKnowledge`:

```yaml
id: date_range
parameters: [from_date, to_date]
type: date_range
label: Report period
help_text: Select inclusive start and end dates.
required: true
validation: {}

---
id: limit
parameters: [limit]
type: integer
label: Number of records
help_text: Maximum number of records to return.
required: true
validation:
  min_integer: 1

---
id: search
parameters: [search]
type: text
label: Client name
help_text: Enter all or part of the client name.
required: true
validation:
  max_length: 200
```

Use separate YAML files if the loader requires one document per file. The
registry is reusable presentation/input metadata; it does not change query SQL
or authorization.

### Capability defaults and guards

Extend `CapabilityKnowledge` with the clarification-relevant typed portions of
existing YAML:

```rust
#[serde(default)]
pub defaults: CapabilityDefaults, // default_limit
#[serde(default)]
pub guards: CapabilityGuards,     // max_limit, max_date_range_days
```

Other existing defaults/guards remain available to their current consumers or
are explicitly represented as needed; do not silently discard fields that are
now validation authority.

Rules:

- Required user parameters are required query parameters excluding
  `source: authorized_scope`.
- `capability.required_parameters` must exactly match those user parameters.
- Every required user parameter must be covered exactly once by a parameter
  input contract.
- A multi-parameter input such as `date_range` covers `from_date` and `to_date`.
- Capability `default_limit` is valid only when `limit` is accepted and within
  `max_limit`.
- `max_date_range_days` overrides the registry's optional bound.
- Optional query filters are not proactively asked in version 1.
- Known optional facts supplied by the user continue into execution when
  verified.
- Authorization-derived parameters never become fields.

### Default policy

Remove the unconditional global required-limit fallback from execution. A
missing required field is handled in this order:

1. verified effective fact;
2. capability-approved catalog default, recorded as
   `FactSourceKind::ApprovedDefault`;
3. clarification field.

This aligns clarification with verified payload issue `003` and prevents a Rust
hardcode from silently becoming business policy.

## Clarification planning

Introduce a pure `ClarificationPlanner` that accepts:

```text
candidate capability ids
catalog input metadata
verified/effective constraints
source intent
existing clarification identity (for revision/error updates only)
```

For each authorized/executable candidate:

1. Load its query and required user parameters.
2. Group parameters through input registry contracts.
3. Mark each input satisfied, defaulted, partially known, or missing.
4. Apply capability bounds/defaults.
5. Build safe option label/description/help from capability metadata.
6. Exclude candidates that are missing catalog/input contracts or are no longer
   authorized; catalog validation should make this exceptional.

Planning result:

- One known capability + missing inputs → `collect_fields`.
- Multiple candidates → `select_option`, even if some have no fields.
- No missing inputs after a single confident selection → execute directly.
- Others/free-text recovery → `free_text`.

### Shared field minimization

A field is shared only when every non-Others candidate contains an equivalent
field contract after capability overrides. Equivalence includes key, type,
required flag, default behavior, and validation bounds. The planner lifts the
field to top-level and removes it from each choice. It does not union
incompatible bounds into a misleading shared field.

### Partial values

A date range with only one verified endpoint remains one `date_range` field
with a partial `value`. The server asks once for the compound input; it does not
produce separate untyped “from” and “to” prose unless a future UI contract
explicitly introduces date-only fields.

## Structured submission contract

### DTO

Evolve `RespondToChatJobRequest` additively:

```rust
pub struct RespondToChatJobRequest {
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub option_id: Option<String>,
    #[serde(default)]
    pub clarification_id: Option<Uuid>,
    #[serde(default)]
    pub clarification_revision: Option<u32>,
    #[serde(default)]
    pub answers: BTreeMap<String, serde_json::Value>,
}
```

Two modes are valid:

**Structured v1**

- `clarification_id` and `clarification_revision` required;
- `option_id` required for `select_option`;
- offered answers supplied as typed JSON;
- `message` required only for Others/free-text.

**Legacy**

- no clarification identity and empty answers;
- current non-empty `message` behavior preserved;
- optional `option_id` resolved through existing continuation semantics.

Mixed partial structured mode fails `400`; it must not silently fall back to
legacy interpretation.

### Submission validation

`JobService::respond` loads the job's active payload and validates before
requeueing:

1. resource ownership/archive/waiting checks;
2. structured vs legacy mode;
3. id/revision equality;
4. option membership and current authorization;
5. answer keys exactly match offered shared/selected fields;
6. required answer presence/default policy;
7. strict value types and bounds;
8. cross-field date ordering/range length;
9. conflict with explicit selected capability/domain/metric;
10. normalized patch creation.

Validated output contains:

```rust
pub struct ValidatedClarificationSubmission {
    pub clarification_id: Option<Uuid>,
    pub clarification_revision: Option<u32>,
    pub selected_option_id: Option<String>,
    pub source_message: String,
    pub display_message: String,
    pub answers: BTreeMap<String, Value>,
    pub constraint_patch: ConstraintPatch,
}
```

`display_message` is safe transcript text synthesized from option/field labels
when structured mode has no user prose. Raw structured answers remain in message
metadata. Do not invent semantic facts outside validated values.

### Error semantics

- Invalid JSON/DTO type: `400 invalid_request_body`.
- Invalid request combination or field value: `400 clarification_validation_error`
  with `details.fields = [{field, code, message}]`; job stays waiting and no user
  message is inserted.
- Accessible waiting job but stale id/revision: `409 clarification_stale`; job
  stays waiting.
- Duplicate response after the job left waiting: `409 clarification_not_active`
  for structured clients.
- Missing/foreign/archived job/session: sanitized `404`.
- Unauthorized option/value: `400 clarification_validation_error` or sanitized
  `404` where revealing availability would cross an authorization boundary; it
  never executes.
- Operational persistence/runtime failure: sanitized `500` with durable-state
  reconciliation guidance.

Add safe `ApiError` constructors for coded bad-request/conflict errors with
structured details. Internal error strings remain unrendered by clients.

### Compare-and-set persistence

Extend `JobRepository::respond` to receive the validated submission and expected
clarification id/revision. In the existing transaction:

- lock the waiting job;
- verify `assistant_job_memory.pending_clarification_json` still matches the
  expected id/revision for structured mode;
- insert the clarification message and typed metadata;
- set job queued;
- insert checkpoint/status event.

If the compare fails, return a typed stale/not-active outcome. This closes the
race between service validation and persistence. Legacy mode uses the active
job payload but retains existing behavior during migration.

The accepted message metadata contains:

```json
{
  "type": "clarification_response",
  "clarification_id": "...",
  "clarification_revision": 1,
  "selected_option_id": "savings_deposit_top_n",
  "answers": {"date_range":{"from":"2026-07-01","to":"2026-07-31"},"limit":10},
  "constraint_patch": {"from_date": "typed-value", "to_date": "typed-value", "limit_value": "typed-value"},
  "source_message": null
}
```

Exact typed-value serialization follows existing `ConstraintPatch` serde.

## Canonical fact integration

Add `constraint_patch` to `RuntimeUserInput`. On a non-initial turn:

- structured mode calls `observations_from_patch` with
  `FactSourceKind::Clarification` semantics;
- legacy mode continues deterministic extraction and semantic resolution;
- if structured mode also contains meaningful free text, deterministic facts
  may be extracted only for fields not supplied in the patch and must retain
  distinct provenance;
- conflicting replay or invalid patch fails before execution;
- accepted clarification observations outrank original request/default facts
  through existing merge precedence.

Approved defaults are converted to observations before deriving effective
constraints. The planner snapshot and normalized parameters therefore point to
winning observation ids for both user answers and defaults.

Do not store UI values as trusted facts merely because they came from a client.
Trust comes from server validation against the active field contract and
principal before observation creation.

## Runtime continuation

Resolution order:

1. Load active clarification from job memory.
2. If structured submission matches it, bypass semantic option matching and use
   the validated option/patch.
3. If legacy reply, preserve current exact/embedding/LLM resolver and Others
   semantics.
4. Merge current-turn typed/deterministic facts with source facts.
5. Re-run clarification planning for the selected capability.
6. If all requirements are satisfied, execute.
7. If invalid/conflicting facts remain, emit same-id next revision with field
   errors or a new conflict clarification as appropriate.
8. Clear active state only on accepted supersession, execution completion,
   terminal recovery, or explicit new active payload replacement.

Predictable required parameters must be present in the first candidate response,
so step 5 should normally execute. It remains a defensive gate for catalog
changes, legacy clients, and genuinely new constraints.

## Endpoint behavior

### POST `/chat/jobs`

No request schema change. When routing is ambiguous, the response status remains
`waiting_for_user_input`, but job result contains `clarification`. Candidate
input planning happens before persistence.

### GET `/chat/jobs/{id}`

Remains the durable recovery endpoint. While waiting,
`result_json.structured_response.clarification` is authoritative public state.
Internal pending state is not exposed directly.

### GET `/chat/jobs/{id}/stream`

Keep SSE event names and envelope. The emitted `structured_response` is the same
serialized response used in job result. No SSE-only clarification schema is
introduced.

### POST `/chat/jobs/{id}/responses`

Accepts structured and legacy modes. Version 1 preserves the existing successful
`201` data shape (inserted `ChatMessage`) to avoid silently breaking clients.
After success, clients reconcile through `GET job` as currently documented.
This extra state fetch is not an extra user clarification round and can be
optimized by a future versioned endpoint if measured latency justifies it.

### GET `/chat/sessions/{id}/messages`

Assistant metadata retains the public `AssistantResponse`; historical
clarifications remain renderable. User clarification metadata retains validated
structured answers for audit but clients treat metadata outside the documented
public response contract as opaque.

### GET `/chat/jobs/{id}/audit`

Add safe clarification decision/checkpoint summaries: id/revision, kind,
offered option/field ids, accepted keys, validation outcome, resolution source,
and provenance identifiers. Do not include hidden prompts or raw SQL.

### POST `/catalog/validate`

Fails when parameter input mapping, required parameter alignment, default, or
bound metadata is inconsistent. Invalid catalogs must fail before chat runtime.

## Dataset design

### Location and format

Add fixtures under:

```text
crates/chat/tests/fixtures/clarification/
```

One YAML file per scenario keeps diffs and failures focused. A fixture contains:

```rust
struct ClarificationFixture {
    id: String,
    message: String,
    candidate_capabilities: Vec<String>,
    known_facts: BTreeMap<String, Value>,
    expected_kind: Option<ClarificationKind>,
    expected_option_ids: Vec<String>,
    expected_shared_fields: Vec<String>,
    expected_conditional_fields: BTreeMap<String, Vec<String>>,
    must_not_ask: Vec<String>,
    expected_direct_execution: bool,
}
```

A separate response fixture section may describe a submission and expected
validation/outcome. Do not couple planner fixtures to live retrieval score order;
pass candidate ids directly to the pure planner. Retrieval evaluation remains
in its existing corpus.

### Coverage, not volume

Require named buckets rather than an exact file count:

- direct execution;
- known capability, one/multiple missing fields;
- multiple options with none/shared/conditional fields;
- all/partial known values;
- approved/default override behavior;
- Others/free text;
- invalid type/range/limit;
- stale/duplicate/unauthorized submission;
- same-session parallel-job isolation;
- recovery/SSE/message projection;
- conflict and bounded retry.

The corpus is loaded only by tests/evaluation. Runtime LLM prompts receive only
bounded active state and relevant catalog entries.

## Compatibility and migration

1. Add nullable job-memory column; existing rows read `None`.
2. Add serde defaults for new public/private fields so historical JSON remains
   readable.
3. Keep session pending as a temporary projection and legacy resolver input only
   until all job continuations read job memory.
4. Add public `clarification` while retaining top-level legacy options.
5. Accept both structured and legacy response DTO modes.
6. Update client to prefer public `clarification` and fall back to legacy
   message/options during deployment.
7. After client adoption and recovery tests, stop writing session pending
   authority and mark top-level clarification options deprecated.
8. Removal of compatibility fields requires a separate versioned breaking
   change, not this implementation.

## Security and privacy

- Option ids must be in both active payload and current authorized capability
  projection.
- Input contracts cannot turn authorization-derived office ids into user input.
- Structured values are untrusted until server validation completes.
- Date/limit/search bounds apply before persistence and execution.
- Search help/examples must not reveal real PII.
- Public responses contain no source intent, principal projection, observation
  ids, SQL, prompts, or internal errors.
- Audit stores only safe summaries and existing protected diagnostic records.
- Invalid/stale submissions never reach SQL planning.

## Error handling and recovery

- Field validation is deterministic and returns stable codes.
- A field-error revision retains prior valid values and adds errors only to the
  affected fields.
- Operational failures do not silently clear active clarification.
- Page refresh always recovers from `GET job`.
- Duplicate SSE updates are idempotent because id/revision and serialized
  payload are stable.
- If Redis is disabled/unavailable, job result and messages remain sufficient.
- Bounded unresolved legacy replies retain the existing free-text recovery
  ceiling.

## Testing strategy

### Contract tests

- Schemars includes `ClarificationView`, kinds, fields, validation, and choices.
- Round trips cover each version-1 kind/type.
- Contract validator rejects impossible kind/array combinations.
- Old `AssistantResponse` JSON without `clarification` still deserializes.

### Catalog tests

- Registry loads all required input mappings.
- Capability/query required parameters align excluding authorized scope.
- Defaults and guards validate.
- Every approved required user parameter is covered exactly once.

### Planner tests

- Known facts/defaults remove questions.
- Partial date range prefill works.
- Shared fields lift only when equivalent.
- Conditional option fields remain attached.
- One capability uses `collect_fields`; complete one executes directly.

### Submission tests

- Structured DTO combinations and field validation.
- ID/revision CAS and duplicate races.
- Option/field authorization.
- Patch mapping and safe display message.
- Legacy behavior remains green.

### Runtime/canonical tests

- Structured answers create clarification observations.
- Approved defaults have winning provenance.
- Source facts survive selection.
- Selected capability executes after one final submission.
- Invalid/conflicting values stay waiting and never execute.

### API/integration tests

- POST job, GET job, SSE update, message metadata, and audit agree.
- Refresh without Redis restores the form.
- Parallel jobs in one session are isolated.
- Archived/foreign/missing job behavior remains sanitized.

## Acceptance scenarios

1. `Show a deposit report` offers total/top options with the common date range
   and top-only limit in one response.
2. Selecting top deposits, filling date range and limit, and submitting once
   executes the same job without asking those fields again.
3. `Show a deposit report this month` omits date range from every option.
4. `Show the 10 largest deposits this month` executes directly when retrieval is
   confident.
5. A known report missing only date range returns `collect_fields` with one
   `date_range` and no report options.
6. A reversed date range returns a stable field error, preserves the job's
   active state, and executes no SQL.
7. A stale revision returns `409` and does not consume the current clarification.
8. Two waiting jobs in one session retain different ids/options/fields and
   cannot consume each other's response.
9. Others with meaningful prose reroutes that prose on the same job; help/detail
   does not call the server.
10. Legacy `message` plus optional `option_id` retains message-authoritative
    continuation, context preservation, conflict re-clarification, Others
    routing, and bounded recovery behavior.
11. Every executed structured field/default has canonical provenance.
12. Existing historical response JSON remains readable.

## Non-goals

- LLM-generated forms, defaults, validators, SQL, or authorization.
- A generic workflow/form engine.
- Frontend implementation inside this Rust repository.
- New reporting capabilities.
- Multilingual product support.
- SSE ordered replay redesign.
- Removing legacy response/submission compatibility in the same release.

## Rollout gates

Before enabling structured clarification by default:

- catalog validation passes for all approved capabilities;
- contract/planner/submission fixture suites pass;
- job-scoped state migration and old-row reads pass;
- legacy clarification integration tests pass;
- one-submit E2E scenarios pass;
- auth/office/PII regression suites pass;
- current client integration docs contain exact TypeScript contracts and
  recovery algorithm.
