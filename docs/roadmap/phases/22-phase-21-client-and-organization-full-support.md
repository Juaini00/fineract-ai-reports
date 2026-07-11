# Implementation Steps: Phase 21: Client and Organization Full Support

Source: `docs-old/implementation-steps.md`

## Phase 21: Client and Organization Full Support

Goal: bring client and organization domains to feature parity with savings so operational reporting is not limited to the savings domain.

Current status:

```text
DONE (branch feat/client-organization-full-support, 2026-07-09)

Client domain (7 approved_mvp capabilities total):
  * client_lifecycle_summary              (aggregate lifecycle summary)
  * client_activation_monthly_breakdown   (monthly activation counts)
  * client_activation_top_n_offices       (top offices by activation count)
  * client_top_n_by_savings_balance      (PII-gated, top-N by current balance)
  * client_top_n_by_savings_account_count(PII-gated, top-N by number of active accounts)
  * client_top_n_by_deposit_volume       (PII-gated, top-N by deposit total in date range)
  * client_summary_by_office             (lifecycle breakdown per office, top-N)

Organization domain (8 approved_mvp capabilities total):
  * organization_office_summary           (aggregate office/staff summary)
  * organization_hierarchy_summary        (aggregate hierarchy depth summary)
  * organization_office_opening_monthly_breakdown (monthly office opening counts)
  * organization_office_client_summary   (per-office client lifecycle counts)
  * organization_office_savings_summary  (per-office savings totals ranked by balance)
  * organization_office_activity_ranking (top offices by transaction volume in date range)
  * organization_office_hierarchy_tree   (walkable tree with parent_id and depth)
  * organization_office_dormant          (offices with zero savings activity in date range)

Catalog metrics after this phase:
  * capabilities: 16 -> 25
  * queries:      16 -> 25
  * retrieval documents: 83 -> 101

PII treatment:
  * client_id and client_display_name output fields carry sensitivity=pii.
  * Planner blocks PII-output capabilities when the API key has can_view_pii=false,
    because GET /chat/jobs/{job_id} returns raw result_json.
  * Formatter omits PII fields when policy.can_view_pii=false as defense-in-depth.
  * All organization capabilities are PII-free (aggregates only).

Office scope:
  * Every SQL file constrains rows to authorized office_ids via the array_bigint
    parameter sourced from authorized_scope. No post-fetch Rust filtering.

Follow-ups deferred from this phase:
  * POST /vector-index/rebuild must run before end-to-end chat tests can retrieve
    the new capabilities.
  * Per-job clarification memory must use `chat_jobs.state_json` plus
    `chat_jobs.state_revision`; see docs/job-memory.md. Implemented typed
    `PendingIntent` in crates/chat/src/chat/pending_intent.rs and wired
    clarification responses to resolve active pending intent before fallback
    reclassification. Candidate capabilities are filtered against prompt shape,
    irrelevant clarification responses keep the pending intent active instead
    of executing, and resolved/null pending intents are ignored by runtime
    readers.
  * Long-form docs (capability-coverage-matrix.md, knowledge-catalog.md,
    reporting-capabilities.md, scenarios/03-catalog-validate.md) still reference
    the 16/16 catalog snapshot and must be resynced.
  * Bespoke narrative renderers per new capability are optional; generic
    render::rows / render::summary already serve them via the LLM answer path.
```
