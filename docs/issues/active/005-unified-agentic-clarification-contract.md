# 005 — Unified agentic clarification contract and job-scoped workflow

Status: active — design required before implementation
Severity: blocker
Area: chat | jobs | assistant | clarification | API | catalog | client integration | tests
Created: 2026-07-22
Resolved:

## Problem

Clarification is not currently a first-class, stable client/server workflow. The
assistant can decide that it needs more information, but it exposes only a
question string and, sometimes, a flat list of options. A client cannot know
without inference whether it should render report choices, a date-range picker,
a numeric input, free text, confirmation controls, or a multi-step conditional
form.

The runtime also discovers missing execution parameters too late. When several
capabilities are offered, it waits for the user to select one, attempts to plan
or execute it, and only then asks for its missing parameters. Required inputs
that were predictable from catalog metadata are therefore collected over
multiple server round trips instead of one client-side clarification flow.

This is an architecture and contract problem, not only a prompt-copy problem.
It crosses clarification state, job/session memory, catalog metadata, canonical
fact provenance, response presentation, HTTP/SSE contracts, frontend rendering,
and scenario coverage.

## User-visible symptoms

- Some clarification responses contain buttons; others contain only prose.
- The client must inspect `options.length` or parse `message` to choose a UI.
- Selecting a report can immediately trigger another predictable question.
- Known facts such as `this month` can be requested again after report
  selection.
- A capability-selection clarification and a missing-parameter clarification
  can have nearly the same response shape.
- “Others” risks being overloaded as report reset, field fallback, help, and
  custom value entry.
- Refresh/recovery can reproduce text, but not a typed in-progress form with
  validated field values.
- A date picker or numeric control must currently serialize its value back into
  natural-language `message` and rely on extraction to recover the same fact.

## Current design and evidence

### Internal clarification state

`crates/chat/src/assistant/context/clarification.rs` defines
`ClarificationPayload` with:

- `question`
- `options`
- `attempt`
- `source_intent`
- `allow_free_text`
- `is_missing_execution_parameters`

The payload has no discriminator for the interaction kind, no typed fields, no
conditional fields per option, no help metadata, no validation metadata, and no
stable clarification identity/revision.

### Presentation flattening

`ResponseBuilder::clarification` in
`crates/chat/src/assistant/presentation/builder.rs` maps only:

- `payload.question` to `AssistantResponse.message`
- `payload.options` to `AssistantResponse.options`

The client does not receive `attempt`, `source_intent`, `allow_free_text`, or
`is_missing_execution_parameters`. `AssistantResponseType::Clarification`
therefore identifies only the broad response category, not the interaction the
client must render.

`ResponseBuilder::missing_parameter` and
`ResponseBuilder::free_form_other_prompt` also return clarification responses
with empty options and prose/actions, creating additional implicit shapes under
the same response type.

### Late missing-parameter discovery

The semantic route first selects or offers capability candidates. Candidate
options do not carry their missing required parameters. After selection,
`execute_selected_capability` calls planning/normalization. A missing parameter
error is converted into another `ClarificationPayload`, often with the selected
capability and `others` represented as options. This makes a field-collection
problem look like another capability-selection problem.

### Narrow submission contract

`POST /chat/jobs/{job_id}/responses` currently accepts only:

```json
{
  "message": "Use July 2026",
  "option_id": "savings_deposit_top_n"
}
```

`message` is required. There is no `clarification_id`, revision, typed `answers`,
field-level validation result, or idempotency token. The repository checks that
the job is waiting and locks it, which prevents many duplicate responses, but
it cannot explicitly reject a response aimed at an older clarification
revision.

### State ownership mismatch

The HTTP continuation is job-scoped, but active pending clarification is stored
in `assistant_session_memory.pending_clarification_json`. `ContextBuilder`
loads that session value without binding it to the job being resumed. The
server does not prohibit another job from being created in the same session.
This permits one job's pending state to overwrite or be read by another job in
the same session even if the current frontend normally disables parallel
submission.

Related state is duplicated across:

- `assistant_session_memory.pending_clarification_json`
- `assistant_job_memory.structured_response_json`
- `chat_jobs.result_json`
- assistant message metadata
- job checkpoints/events
- Redis SSE latest-event payload

There is no single job-scoped authoritative active-clarification object.

### Canonical facts are available but bypassed

Canonical state already defines `ConstraintPatch`, `TypedFactValue`,
`FactSourceKind::Clarification`, `FactObservation`, and
`observations_from_patch`. The production response path does not accept a typed
patch from the client. It stores text and runs deterministic extraction again,
losing the opportunity to preserve direct field-level provenance from trusted
UI controls.

`FactSourceKind::ApprovedDefault` also exists, but runtime required-parameter
defaulting currently happens directly in tool normalization. For example,
`limit`/`top_n` receives a global default of `10` without a production
`ApprovedDefault` observation.

### Catalog metadata gap

The current approved catalog has 29 capabilities but only five required
parameter combinations:

| Capability count | Required parameter combination |
| ---: | --- |
| 9 | `from_date`, `to_date`, `limit` |
| 9 | `limit` |
| 6 | `from_date`, `to_date` |
| 4 | none |
| 1 | `search` |

This means the first version needs only a small set of UI primitives, not a
separate response format per capability.

Capability YAML already contains useful keys such as `defaults`, `guards`, and
sometimes `clarification`, but `CapabilityKnowledge` does not deserialize those
keys. They are silently ignored. The active validator checks broad parameter
presence and query parameter types, but does not enforce the documented
`required_parameters_match_query` rule or validate clarification presentation
metadata, defaults, min/max values, help text, or conditional input mappings.

### Current contract tests are insufficient

Schemars exposes `ClarificationPayload` and `AssistantResponse`, and tests cover
representative serialization. Existing integration tests primarily assert that
clarification has non-empty options or non-empty text, does not loop, preserves
the same job, and accepts valid option ids. They do not prove that every
clarification is machine-renderable, that predictable follow-ups are bundled,
or that structured answers become verified execution facts.

## Root causes

1. Clarification is modeled as prose plus optional capability choices rather
   than a versioned interaction protocol.
2. Required-input planning happens after capability selection instead of while
   candidate options are prepared.
3. Active clarification ownership is session-scoped while continuation and
   canonical execution state are job-scoped.
4. Client answers are flattened into text before entering the canonical fact
   model.
5. Catalog parameter metadata is execution-oriented and partially ignored; it
   is not yet a typed clarification/input contract.
6. Defaults and validation rules do not have one authoritative, provenance-aware
   policy.
7. HTTP, SSE, persisted message, and durable job recovery surfaces do not share
   a tested clarification sub-contract.

## Goals

- Give every clarification one stable, versioned, machine-readable envelope.
- Let the client render without parsing business prose or guessing from empty
  arrays.
- Plan predictable follow-up fields before presenting capability options.
- Allow the client to collect option selection and all required values locally,
  then submit them once.
- Ask a second server-side clarification only for invalid, conflicting,
  unsupported, or genuinely new ambiguity.
- Make active clarification state authoritative and job-scoped.
- Convert structured answers directly into validated canonical facts with
  provenance.
- Keep semantic option resolution and free-text recovery available.
- Keep LLM reasoning bounded; capability requirements, field types, validation,
  and authorization remain backend/catalog controlled.
- Build broad offline scenario coverage without placing the entire dataset in
  the LLM context.

## Expected behavioral invariants

1. The client switches on a discriminator, never on `message` contents.
2. Existing facts are prefilled or omitted and are not requested again.
3. Required inputs for offered options are known before the first clarification
   is emitted.
4. Shared missing fields are represented once; option-specific fields remain
   conditional on that option.
5. “Others” is available for “none of these reports”, not as a universal field
   fallback or help action.
6. Help/detail is presentation metadata and a local UI action. It is not a
   clarification answer and does not consume an attempt.
7. Back/continue between local steps does not call the server.
8. Only final submit calls `/responses`.
9. Structured answers are validated against exactly the active clarification
   and authorized capability.
10. A stale, unknown, unauthorized, duplicate, or malformed submission never
    executes SQL.
11. Clarification remains on the same job.
12. `GET job` is the durable recovery source; SSE is only a live hint.
13. Markdown remains a fallback rendering, not the source of UI behavior.
14. The LLM may rank options or interpret free text, but it cannot invent an
    unsupported field type, validation rule, default, capability, or SQL.

## Proposed clarification response contract

The design spec should finalize an additive/versioned contract along these
lines:

```json
{
  "response_type": "clarification",
  "title": "Additional information required",
  "message": "Choose a report and complete the required information.",
  "clarification": {
    "version": 1,
    "id": "clarification-uuid",
    "revision": 1,
    "kind": "select_option",
    "question": "Which report would you like?",
    "fields": [],
    "options": [
      {
        "id": "savings_deposit_total",
        "label": "Total deposits",
        "description": "Total deposit amount and count for a period.",
        "help": {
          "details": "Combines all matching deposit transactions.",
          "example": "Total deposits this month."
        },
        "fields": [
          {
            "key": "date_range",
            "type": "date_range",
            "label": "Report period",
            "required": true,
            "value": null,
            "help_text": "Select inclusive start and end dates."
          }
        ]
      },
      {
        "id": "others",
        "label": "Another report",
        "description": "Describe a different report in your own words.",
        "fields": []
      }
    ],
    "allow_free_text": false
  },
  "options": []
}
```

The exact compatibility placement of legacy top-level `options` must be decided
in the design spec. During migration, it may remain populated as a deprecated
projection while `clarification` is authoritative.

### Clarification kind enum

Initial closed set:

- `select_option` — choose one capability/report; options may have conditional
  fields.
- `collect_fields` — capability is known; collect one or more missing fields.
- `free_text` — describe a different request.
- `confirmation` — explicit confirm/cancel only where product or policy requires
  it.

Do not add `single_tab` or `multi_tab`. The client derives local steps from the
selected structure. A single `date_range` is one field/step even though it has
start and end values.

### Field type enum

Initial set should cover current approved capabilities:

- `date`
- `date_range`
- `integer`
- `text`
- `single_select`
- `multi_select`
- `office_selector` only if office selection is a real user-editable input;
  authorization-derived office scope must not be exposed as an arbitrary value.

Every field needs stable `key`, `type`, `label`, `required`, optional current
`value`, optional approved default, help metadata, and type-appropriate
validation metadata. Unknown future enum values must produce a safe client
fallback rather than a crash.

### Client rendering rules

- `select_option`: render option cards. “View details” expands local help.
- `collect_fields`: render fields directly with no report options.
- `free_text`: render a text area.
- `confirmation`: render confirm/cancel controls.
- Show progress only when the resolved local step count is greater than one.
- Selecting an option reveals its fields locally; it does not submit.
- Fields with valid known values are prefilled or omitted according to the
  finalized contract.
- Help, back, continue, skip-approved-default, and detail expansion are local UI
  actions, not overloaded option ids.
- Historical clarification messages are read-only unless the corresponding job
  is still waiting on the same clarification id/revision.

## Proposed structured submission contract

The design spec should extend `/responses` to support typed answers while
retaining a compatibility path for existing `message`/`option_id` clients:

```json
{
  "clarification_id": "clarification-uuid",
  "revision": 1,
  "option_id": "savings_deposit_top_n",
  "answers": {
    "date_range": {
      "from": "2026-07-01",
      "to": "2026-07-31"
    },
    "limit": 10
  },
  "message": null
}
```

Rules:

- `clarification_id` and `revision` bind the answer to active state.
- `option_id` is required for `select_option`, except a clearly defined legacy
  semantic-text compatibility route.
- `answers` may contain only offered field keys.
- Required fields must be present unless an approved default is explicitly
  selected/applied.
- `message` is required only for `free_text` or legacy natural-language
  responses; it should not be mandatory for typed field submission.
- Unknown fields, invalid types, unavailable options, unauthorized values, and
  stale revisions fail closed.
- Syntactically malformed requests return the standard `400` validation
  envelope.
- Well-shaped but invalid business values keep the job waiting and return
  machine-readable field errors through one consistently documented response
  path. The design spec must settle exact HTTP status/response data semantics.
- Accepted typed answers become canonical `FactObservation` records with
  `FactSourceKind::Clarification`; do not stringify and re-extract them.
- Natural-language answers continue through deterministic/semantic extraction
  and carry their actual provenance.

## Job and memory model changes

The active clarification must become job-scoped authoritative state containing
at least:

- clarification id and revision;
- kind and attempt count;
- source intent/capability candidates;
- known effective facts relevant to the form;
- fields/options offered;
- field defaults and validation contract snapshot;
- creation/update timestamps;
- status such as active, answered, superseded, cancelled, or exhausted.

`assistant_session_memory` may retain a non-authoritative reference for context,
but it must not be the source used to answer a job-scoped response. Parallel
jobs in one session must not overwrite or consume each other's clarification.
The design spec must choose whether this requires a dedicated job-memory JSON
column/table or a typed extension to existing job memory; do not rely only on
rendered `structured_response` as workflow authority.

Persisted older JSON must remain deserializable through explicit serde defaults,
version handling, or migration. Avoid a schema shape that makes existing job or
message history unreadable.

## Endpoint impact

### `POST /chat/jobs`

- Build candidate clarification plans before execution.
- Merge already verified user facts.
- Resolve approved defaults with provenance.
- Attach shared and option-conditional missing fields.
- Persist active job-scoped clarification before returning waiting status.
- Do not emit a predictable second question after option selection.

### `GET /chat/jobs/{job_id}`

- Return the full versioned clarification under durable `result_json` while the
  job waits.
- Expose enough state for page-refresh recovery.
- Never require audit or SSE replay to reconstruct the active form.

### `GET /chat/jobs/{job_id}/stream`

- Reuse the exact same structured clarification contract as `GET job`.
- Keep existing SSE event names (`status` and `update`) unless a separate
  versioned API decision explicitly changes them.
- Continue treating SSE as deduplicated, non-durable notification; clients must
  reconcile with `GET job`.

### `POST /chat/jobs/{job_id}/responses`

- Accept clarification id/revision, option, and typed answers.
- Validate active state, authorization, field contract, and values before
  requeueing/execution.
- Persist raw submission plus normalized typed facts and provenance.
- Make duplicate/stale behavior explicit instead of collapsing all cases into a
  generic missing/not-waiting response.
- The design spec must decide whether success returns the inserted message plus
  updated job snapshot, or retains the current inserted-message response and a
  required `GET job`. Prefer a response that avoids an unnecessary recovery
  round trip without breaking existing clients silently.

### `GET /chat/sessions/{session_id}/messages`

- Persist the complete versioned structured response in assistant message
  metadata.
- Render historical clarification safely as read-only.
- Do not make opaque diagnostic/canonical state part of the public message
  contract.

### `GET /chat/jobs/{job_id}/audit`

Record at minimum:

- clarification id/revision and reason;
- candidate options and fields offered;
- known values omitted/prefilled;
- default decisions and provenance;
- raw and normalized user answers;
- validation failures/conflicts;
- resolution source and final outcome.

Do not expose hidden prompts, SQL, secrets, or unsafe internal errors.

### `POST /catalog/validate`

Validate clarification-related catalog contracts:

- capability required parameters match required query parameters excluding
  authorization-derived inputs;
- field mapping exists for every user-supplied required parameter;
- field type and value type agree with query/canonical types;
- defaults are typed, bounded, and semantically approved;
- min/max/date-range constraints are valid;
- option/help metadata is safe and present where required;
- no user control can expand authorized office or capability scope.

No new clarification endpoint is required unless the design proves the existing
job continuation surface cannot be versioned safely.

## Internal process changes

1. **Route/retrieve:** identify one capability or a bounded candidate set.
2. **Clarification planner:** compare each candidate's input contract with
   effective verified facts and approved defaults.
3. **Question minimizer:** compute shared versus conditional missing fields and
   omit facts already known.
4. **Presentation builder:** produce the versioned discriminated contract.
5. **Job persistence:** atomically store active clarification and waiting job
   state.
6. **Submission validator:** bind id/revision, validate option/answers and
   authorization, and reject stale or unexpected data.
7. **Canonical merge:** append structured clarification observations and derive
   effective constraints with provenance.
8. **Execution planner:** execute only from verified constraints/defaults.
9. **Presentation/recovery:** persist and emit the same structured response
   through job result, messages, and SSE.
10. **Audit:** record the full safe decision trail.

## Catalog/input-contract design requirements

Do not put UI behavior directly into every capability as duplicated prose.
Define a small typed parameter registry or equivalent catalog-owned mapping for
reusable parameters such as:

```text
from_date + to_date -> date_range
limit               -> integer
search              -> text
currency_code       -> single_select/text according to approved semantics
product_ids         -> multi_select when supported
```

Capability/query metadata should reference these contracts and override only
real capability-specific bounds/defaults/help. The design must avoid three
independent sources of truth between capability YAML, query YAML, and Rust
hardcodes.

The distinction between these concepts must be explicit:

- technically required query parameter;
- authorization-derived parameter;
- user-required semantic input;
- optional filter;
- approved default;
- value already verified from conversation state.

A query parameter being `required: true` does not automatically mean the user
must be asked. For example, authorization-derived office ids and approved
limits may be filled without user interaction, but their source must be
explicit and auditable.

## Clarification dataset and evaluation corpus

A large dataset is valuable for offline coverage, not as a single runtime LLM
prompt. The runtime should receive only the current intent, bounded candidates,
known facts, and relevant contracts. Dataset growth must be coverage-driven,
deduplicated, and bucketed rather than an unbounded collection of near-identical
phrases.

Each fixture should capture structured expectations, for example:

```yaml
id: ambiguous_deposit_with_known_period
message: "Show a deposit report for this month"
known_facts:
  from_date: "resolved-start"
  to_date: "resolved-end"
expected:
  kind: select_option
  option_ids:
    - savings_deposit_total
    - savings_deposit_top_n
  must_not_ask:
    - date_range
  conditional_fields:
    savings_deposit_top_n:
      - limit
```

Required coverage buckets:

### Decision topology

- unambiguous capability with no missing inputs executes directly;
- one known capability with one missing field;
- one known capability with multiple missing fields;
- multiple options with no follow-up fields;
- multiple options with shared missing fields;
- multiple options with different conditional fields;
- free-text/Others recovery;
- confirmation only for a real supported use case.

### Known facts and defaults

- all fields supplied in the original request;
- partial date range;
- resolved relative period retained after option selection;
- explicit limit retained;
- approved default applied without another question;
- user value overrides approved default;
- optional filters omitted;
- shared fields deduplicated across options.

### Resolution behavior

- exact option id;
- exact label;
- semantic paraphrase;
- meaningful Others message;
- boilerplate Others requiring free text;
- new request superseding previous candidates;
- invalid/stale option;
- cancel, if product supports cancellation;
- bounded unresolved attempts without loops.

### Structured validation

- valid date range;
- reversed date range;
- date range over capability maximum;
- missing start or end date;
- zero, negative, non-integer, and over-maximum limit;
- unknown answer key;
- wrong value type;
- unauthorized office/capability value;
- conflicting metric/domain;
- duplicate and stale revision submission.

### Lifecycle and delivery

- page refresh restores active form from `GET job`;
- duplicate SSE events render idempotently;
- Redis unavailable still permits durable recovery;
- two jobs in one session keep isolated clarification state;
- archived session rejects continuation;
- completed/failed/expired job rejects continuation;
- historical clarification controls are disabled;
- concurrent response attempts cannot execute twice.

### Safety and audit

- no raw SQL/prompt/internal error in response;
- no option outside authorized capability set;
- office selectors cannot exceed principal scope;
- every accepted field has provenance;
- every default has `ApprovedDefault` provenance;
- conflicting/untrusted fields do not execute.

The first product-language corpus remains English under the current language
policy. Multilingual fixtures should be added only with the separate extraction,
classification, and template support required by project policy; do not create
a misleading bilingual UI contract ahead of that support.

Do not hardcode an exact fixture count. Tests should require coverage of named
buckets and current approved parameter/capability contracts so catalog growth
forces deliberate fixture updates.

## Compatibility and rollout

Recommended rollout sequence:

1. Add typed catalog/input metadata and validation without changing responses.
2. Add job-scoped pending clarification and identity/revision behind the current
   flow.
3. Add versioned clarification response as an additive field while retaining
   the legacy top-level projection.
4. Add typed `/responses` answers while retaining legacy natural-language
   submissions.
5. Update client to render the discriminated contract and submit once.
6. Add canonical structured-answer observations and approved-default
   provenance.
7. Switch clarification planning to bundle predictable fields.
8. Remove deprecated implicit client inference only after compatibility tests
   and client rollout.
9. Remove session pending clarification authority after job-scoped cutover and
   migration/recovery verification.

Unknown clarification kinds/field types must fail safely on old clients. New
clients should show a generic safe fallback and refresh/reconcile rather than
crash or silently submit incomplete data.

## Testing requirements

### Unit/contract tests

- Serde/Schemars coverage for every clarification kind and field type.
- Contract validation rejects impossible combinations, such as
  `select_option` with no options.
- Client fixture examples prove deterministic rendering decisions.
- Parameter-registry mapping and capability/query alignment tests.
- Shared/conditional field minimization tests.
- Structured answer to `ConstraintPatch`/`FactObservation` tests.
- Approved-default provenance and precedence tests.
- Old persisted response/pending JSON remains readable.

### Runtime tests

- Selection plus predictable fields completes in one `/responses` submission.
- Known date/limit facts are not asked again.
- Invalid answers remain waiting and expose field-level errors.
- Stale/duplicate/unauthorized submissions never execute.
- Parallel jobs in one session do not share pending state.
- Free-text legacy and semantic-option replies remain supported.
- Clarification attempts remain bounded.

### API/integration tests

- `POST jobs`, `GET job`, SSE, persisted messages, and audit use compatible
  clarification structures.
- Reload recovery works without Redis.
- Response submission is atomic with job status and canonical observations.
- Archived/missing/foreign resources retain sanitized access behavior.
- Client compatibility path is documented and tested.

### Dataset/evaluation tests

- Named behavior buckets are present.
- All approved user-supplied required parameter mappings have at least one
  positive and one missing/invalid scenario.
- Retrieval ambiguity fixtures verify relevant candidate inclusion without
  requiring unstable exact ordering among tied alternatives unless ordering is
  a declared contract.
- Evaluation reports results by decision kind, parameter shape, and failure
  category, not only aggregate accuracy.

## Acceptance gates

- Every waiting clarification has a versioned `kind`, stable id/revision, and a
  contract the client can render without reading prose.
- `fields` and `options` have defined presence/empty-array semantics.
- One known missing `date_range` renders as one field without report options or
  an unnecessary stepper.
- Option selection with two predictable required inputs can be completed in one
  final submission and does not trigger the same predictable questions again.
- Existing verified facts are retained across capability selection.
- Help/detail does not submit, mutate state, or consume attempts.
- “Others” is limited to report/request escape behavior; field fallbacks use
  explicit field-specific controls.
- Active pending clarification is authoritative and job-scoped.
- Two jobs in one session cannot consume each other's clarification.
- Typed answers become verified canonical facts with clarification provenance.
- Approved defaults have explicit provenance and do not silently contradict
  user input.
- Capability/query/input metadata is validated before runtime.
- `GET job`, SSE, and persisted assistant metadata carry the same structured
  clarification contract.
- Stale, malformed, conflicting, unauthorized, or duplicate answers cannot
  execute SQL.
- Legacy `message`/`option_id` clients have a documented migration path.
- Dataset coverage is broad and bucketed without loading the corpus into every
  LLM call.
- Relevant unit, contract, runtime, and API scenario tests pass.

## Non-goals

- Do not let the LLM generate SQL, field schemas, validation bounds, defaults,
  or authorization rules.
- Do not index transactional Fineract rows into the clarification dataset.
- Do not build a generic survey/form platform unrelated to approved reporting
  inputs.
- Do not add arbitrary frontend component names to business capability ids.
- Do not expose canonical/audit internals directly to the client response.
- Do not weaken bearer ownership, office scope, PII, or approved capability
  guards.
- Do not make Indonesian a supported product language as part of this issue
  without the separate multilingual extraction/template work required by
  policy.
- Do not require one server round trip per local wizard tab.
- Do not solve unrelated SSE replay/log limitations beyond keeping durable job
  recovery authoritative.

## Relationship to other issues

- `003 — Robust verified payload extraction` owns the broader requirement that
  every executed field be verified and provenance-backed. This issue owns the
  clarification-specific response/submission protocol, input metadata, and
  typed clarification observations needed to satisfy that boundary. Avoid
  implementing two competing fact models.
- `001 — Clarification response matching must be semantic` is resolved and
  remains the basis for semantic option/free-text resolution. This issue must
  preserve that behavior rather than returning to literal-only matching.
- Retrieval pipeline issue `08` addressed valid option continuation/routing.
  This issue builds a stable client and state contract above that corrected
  continuation path.
- The existing clarification-continuation correctness spec in the working tree
  covers dual `option_id`/`message` semantics and bounded recovery. It is a
  prerequisite/compatibility input, not a replacement for this broader issue.

## Required follow-up design document

After issue review, create a dedicated design spec under
`docs/superpowers/specs/` covering:

- exact Rust and TypeScript discriminated contracts;
- authoritative job-state storage and migration;
- catalog parameter/input schema;
- clarification planning/minimization algorithm;
- structured submission validation and HTTP semantics;
- canonical observation/default integration;
- compatibility and rollout;
- complete scenario fixture schema and initial matrix.

Implementation must not begin from the illustrative JSON in this issue alone.

## Links

- `docs/issues/active/003-verified-payload-extraction.md`
- `docs/issues/resolved/001-clarification-response-matching-must-be-semantic.md`
- `docs/issues/retrieval-pipeline-rework/08-clarification-reply-routing-failed.md`
- `docs/superpowers/specs/2026-07-19-clarification-continuation-correctness-design.md`
- `docs/current/chat-client-integration.md`
- `docs/architecture/chat-data-model/10-9-clarification-flow-state.md`
- `crates/chat/src/assistant/context/clarification.rs`
- `crates/chat/src/assistant/execution/runtime/clarification.rs`
- `crates/chat/src/assistant/execution/runtime/execution.rs`
- `crates/chat/src/assistant/context/canonical_state/`
- `crates/chat/src/assistant/presentation/builder.rs`
- `crates/chat/src/assistant/presentation/response.rs`
- `crates/chat/src/api/dto/job.rs`
- `crates/chat/src/api/handlers/job.rs`
- `crates/chat/src/job/service/run.rs`
- `crates/chat/src/job/repository/mod.rs`
- `crates/chat/src/knowledge/model.rs`
- `crates/chat/src/knowledge/catalog/validator.rs`
- `knowledge/capabilities/`
- `knowledge/queries/`
- `knowledge/responses/clarification.yaml`
