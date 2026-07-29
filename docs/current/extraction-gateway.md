# LLM Extraction Gateway — Integration Guide

**Status**: layers built and end-to-end pipeline callable
(`assistant::understanding::pipeline::run`). Runtime wiring
(replacing the current router step with this pipeline) is a follow-up.

**Spec**: `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md`.
**Plan**: `docs/superpowers/plans/2026-07-24-llm-extraction-gateway.md`.

## What it is

Three-layer pipeline that turns a raw user message into a routing decision:

1. **Layer 1 — Gateway** (`understanding::gateway`). One LLM call constrained
   to the `LlmGatewayExtraction` JSON schema. Extracts `intent_kind`,
   `domain`, `language`, `entities`, optional `temporal_hint`, optional
   `quantity_hint`, and ranked `candidates` (capability ids with `confidence`
   and `why`). Sanitized on return: entities that don't appear verbatim in
   the user message are dropped; candidates outside the caller's visible
   catalogue are dropped.

2. **Layer 2 — Resolver** (`understanding::resolver`). For the selected
   capability, resolves each declared parameter in this order:
     1. LLM `temporal_hint` / `quantity_hint` (only if `confidence >= 0.7`).
     2. YAML `default` expression, evaluated against
        `EvaluationContext { business_today, wall_today, authorized_office_ids }`.
     3. If both fail and the parameter is `required`, it lands in
        `unfilled_required`.
   Each filled parameter records its `PayloadSource` (`LlmClaim` or
   `CatalogDefault`).

3. **Layer 3 — Decider** (`understanding::decider`). Combines the extraction,
   the resolved parameters, and the existing gap-based classifier
   (`decide_from_scores`) into `Execute` / `Clarify` / `Reject`.

## How defaults are chosen

Defaults are declared per parameter under `parameters:` in a capability YAML,
using the fixed DSL from spec §5.2:

```yaml
parameters:
  from_date:
    type: date
    required: false
    default: start_of_month(business_today)
    fill_when_missing: true

  to_date:
    type: date
    required: false
    default: business_today

  limit:
    type: integer
    required: false
    default: "10"
    hard_cap: 100
```

The resolver evaluates the default expression via
`DefaultExpr::evaluate(&EvaluationContext)`; the whitelist is:
`business_today`, `wall_today`, `business_today - Nd/Nm/Ny`,
`start_of_month(business_today)`, `end_of_month(business_today)`,
`unbounded`, `authorized_scope`, literal date `YYYY-MM-DD`, literal integer.
Anything else fails the loader.

## "today" vs business date

- **"today" always means `EvaluationContext.business_today`** — the tenant's
  Fineract business date, resolved once per job by
  `AuditingBusinessDateProvider` (`assistant::temporal::business_date`).
- **Wall clock is only used for audit-time fields** (`Utc::now()` for
  `occurred_at`, `resolved_at`).
- Fallback: if the Fineract lookup fails or returns null, the provider
  degrades to wall-clock and emits a `business_date.fallback_used` audit
  event so operators can see when the tenant date drifted.

## Observing `business_date.fallback_used`

Filter the management audit stream:

```
GET /management/audit?event_type=business_date.fallback_used&from=...&to=...
```

Every occurrence carries the resolved date (`resolved_date`) so you can
reconcile against Fineract's own business-date table. If this fires
routinely, the Fineract read-replica is unhealthy — investigate before
trusting any date-scoped report from that window.

Related audit events introduced by Bundle 11:
- `execution.result_truncated` — result was clamped by `hard_cap` or the
  global row backstop; the shown row count is authoritative, but there
  were more rows in the underlying query.
- `execution.timed_out` — SQL execution hit its per-query
  `statement_timeout`; the response is an error, and the audit trail
  captures the capability/query id.

## Migration notes for capability authors

**No new YAML surface**. The `parameters:` block already exists in every
approved-MVP capability (Bundle 4). The gateway pipeline consumes exactly
that block via the existing `ParameterPolicy` loader — no new fields,
no new files.

Contract the pipeline enforces (Layer 3 refuses to ship otherwise):
- Never ask for `from_date`/`to_date` when a `default` is declared —
  Bundle 10's catalog-wide validator rejects that shape at load time.
- Never ask a parameter that is not listed in the capability's
  `parameters:` block.
- A defaultless required parameter (today only `client_name_lookup.search`)
  always surfaces as a `Clarify { missing_fields: [...] }` decision.

To add a new capability that participates in gateway routing:
1. Author its YAML under `knowledge/capabilities/<domain>/…yaml` with a
   `parameters:` block whose defaults cover every user-facing input the
   query needs.
2. Add its `.sql` under `queries/…` — reviewed as usual.
3. Add a scenario row to
   `crates/chat/src/assistant/understanding/pipeline.rs` if the capability
   exercises a novel temporal or quantity phrasing.

## What is deferred

- **Runtime injection** — `pipeline::run` is not yet called by
  `AssistantGraphRuntime`. The current router step continues to serve
  production traffic. Wiring is spec §7 Task 7.1 steps 3–4.
- **Deterministic-extractor demotion** — spec §7 Task 7.2 will move
  `understanding::extraction` from primary path to verification helper
  (used to double-check that LLM entities appear verbatim, phrase spans
  align, etc.).
- **Loan scenarios** — spec §7 rows for `loan_arrears_clients`,
  `loan_repayments_today`, `loan_interest_recent` wait for issue 008 to
  ship the loan capability catalogue.

## Runbook

- **Symptoms of an LLM regression**: `SchemaInvalidAfterRetry` errors
  spike in the gateway warn log
  (`target=assistant::gateway, event=schema-invalid`). Check that the
  provider hasn't shipped a response-format change; the schema is
  generated from `schemars::schema_for!(LlmGatewayExtraction)` and
  matches spec §4.1 exactly.
- **Symptoms of an over-defaulted capability**: a capability that
  should ask returns `Execute` with an obviously-wrong parameter.
  Check the YAML `parameters:` block; a stray `default:` on a required
  input silences the clarification.
- **Symptoms of a sanitizer drop**: `target=assistant::gateway,
  event=sanitize dropped …` counts rise. The LLM is emitting entities
  or capability ids the resolver won't accept — usually a prompt drift
  or a catalog update the LLM hasn't seen.
