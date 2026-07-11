# AI Reporting Service Design: 18. Planned Architecture Changes

Source: `docs-old/ai-reporting-design.md`

## 18. Planned Architecture Changes

The following architectural changes are planned but not yet implemented. They are documented here so the coverage matrix's `planned` entries have a concrete design target, and so that reviewers can push back on drift toward ad-hoc solutions.

### 18.1 Bucket-parametric capabilities

**Problem.** The current design forces one capability per bucket size. To answer weekly, daily, and custom N-day questions over savings deposits alone would require four near-duplicate capabilities: `savings_deposit_monthly_breakdown` (exists), `savings_deposit_weekly_breakdown`, `savings_deposit_daily_breakdown`, `savings_deposit_custom_bucket_breakdown`. The same expansion applies to withdrawals, activity, charges, and every future metric. This is quadratic capability growth for linear user need growth.

**Change.** Introduce a single bucket-parametric capability per metric family:

- `savings_deposit_breakdown` with parameter `bucket ∈ {day, week, month, N_days}` and, when `bucket=N_days`, a `bucket_days: integer` parameter validated to `>= 2` and `<= configured_max_bucket_days`.
- `savings_withdrawal_breakdown` — same shape.
- `savings_activity_breakdown` — new metric family covering deposits + withdrawals + interest postings + fees, with the same bucket parameter.

Each capability maps to one approved SQL file with `date_trunc('$bucket', transaction_date)` (for month/week/day) or a computed `floor((transaction_date - $from_date) / $bucket_days)` (for N_days). Parameter substitution happens through the same bound-parameter path as today — the bucket choice is validated by the policy guard before SQL selection.

**Migration path.**

1. Author `savings_deposit_breakdown` with `status: approved_mvp`, shipping alongside the existing four monthly capabilities.
2. Mark `savings_deposit_monthly_breakdown` and `savings_deposit_monthly_top_n` `status: superseded_by: savings_deposit_breakdown`. The classifier continues to accept both; results are identical when `bucket=month`.
3. Run both in parallel for one release. Log which capability the classifier picks per request.
4. After one release with no divergence, remove the `_monthly_` capabilities and their SQL files. Coverage matrix rows flip from four `implemented` cells to two `implemented` cells covering all four bucket columns.

**Non-goals of this change.** No new output_mode is introduced — the response payload keeps `bucket_start`, `bucket_end`, and metric columns, just with a variable bucket width. The formatter labels the axis "Month" / "Week" / "Day" / "N-day window" based on the `bucket` parameter.

### 18.2 Composite planner and executor

**Problem.** Real user turns often ask for multiple metrics in one breath: *"Show me the biggest deposit, biggest withdrawal, biggest charge, and biggest hold this month."* Today's planner returns `Option<ExecutionPlan>` — one capability per turn. Answering this question forces either four sequential turns or a formatter-side fake concatenation of unrelated single-capability responses.

**Change.** The planner returns an `ExecutionPlanBatch` — internally a `Vec<ExecutionPlan>` with a shared budget:

```text
struct ExecutionPlanBatch {
    plans: Vec<ExecutionPlan>,
    shared_timeout_ms: u32,
    shared_row_cap: u32,
    output_mode: OutputMode::Composite,
}
```

Contracts:

- Every plan in the batch must independently pass the same policy guards (capability scope, office scope, PII, parameter validity). No plan is executed unless the whole batch validates.
- The executor runs the batch concurrently under the shared timeout budget with a bounded task fan-out; the first plan to fail cancels the rest and the whole batch fails.
- The `Composite` output_mode formatter concatenates one labeled section per plan (label = capability's `display_name` from its YAML) with the plan's normal per-capability output_mode rendered inside.
- Total returned row count across all plans is capped by `shared_row_cap` to prevent a wide fan-out from bypassing per-capability limits.
- Redis SSE and Postgres checkpoints operate at the batch level: one job id, one `chat_jobs` row, one `chat_job:{id}:live_state` key. Per-plan progress is recorded in `chat_jobs.state_json.batch_progress`.

**Classifier interaction.** The classifier detects composite intent (multiple metric references in one utterance) and emits a `CompositeMatched` outcome carrying the list of capability ids. Any single plan mapping to `planned` or `deferred` demotes the whole batch to `PlannedUnimplemented` or `Unsupported` respectively — we never partially answer a composite request.

**Migration path.** Introduce `ExecutionPlanBatch` alongside `ExecutionPlan`; for backward compatibility the single-plan path wraps into a batch of one. The composite output_mode ships behind a feature flag until one release of soak testing shows no per-plan cancellation flakiness.

### 18.3 `PlannedUnimplemented` fourth outcome

**Problem.** Today the decision policy has three terminal outcomes: `Matched`, `ClarificationRequired`, `Unsupported`. When a user asks something reasonable and roadmapped — say, a weekly breakdown — the system has no way to say "we know what you mean and it's coming, but not today." It either falls back to `Unsupported` (misleading — it sounds permanent) or hallucinates a nearby match.

**Change.** Add a fourth outcome to the decision policy in §8:

```text
enum ClassificationOutcome {
    Matched(ExecutionPlanBatch),
    ClarificationRequired(ClarificationPrompt),
    PlannedUnimplemented(PlannedFeatureRef),
    Unsupported(UnsupportedReason),
}
```

`PlannedFeatureRef` carries the coverage matrix row/column pair the classifier matched, plus the target milestone from the matrix. The response template is sanitized and fixed:

> This report is planned but not yet available in this release. Expected in {target_milestone}.

Assignment rule:

- The classifier retrieves against the knowledge index as today. If retrieval semantically hits a capability example whose YAML declares `status: planned` (or references a `planned` matrix cell), the outcome is `PlannedUnimplemented`.
- No SQL runs. No planner LLM fallback runs. No candidate SQL is materialized. The classifier's decision is final for this outcome.
- Terminal job status is `planned_unimplemented` — distinct from `completed`, `awaiting_clarification`, and `unsupported`. Redis live_state uses the same value.
- `chat_job_events` records one `planned_unimplemented` event with the matched matrix cell and target milestone for audit and product-analytics counting.

**Why distinct from `Unsupported`.** `Unsupported` covers hard rejects and deferred domains — the answer is "no, not now, not in a foreseeable milestone." `PlannedUnimplemented` says "yes, we plan to answer this, we know where it belongs, come back next release." The two need different templates, different analytics, and different product responses.

### 18.4 Ranking and comparative planners

**Problem.** "Which office has the highest deposit total this month?" and "How did deposits move this month vs last month?" are common admin questions. Neither is a single-metric aggregate; both require multiple internal executions with a specific rendering.

**Ranking planner change.** For a top-N-entities-over-metric question, the planner emits an `ExecutionPlanBatch` where each plan is the same capability parameterised over one entity value (or a single grouped-by capability if one exists). The batch executor concurrently runs the plans (bounded fan-out) and the composite formatter renders as a leaderboard. Ranking requires no new capabilities per metric family — only the target entity dimension (`office_id`, `product_id`, `staff_id`) as a group-by parameter on the underlying capability.

**Comparative planner change.** For period-over-period, the planner emits a batch of two same-shape plans (this period, previous period) plus a delta directive. The formatter renders both totals and their difference. Comparative shares the composite output_mode contract but adds a delta section.

Both live inside the `Vec<ExecutionPlan>` design in §18.2; they do not introduce a new fan-out primitive.

### 18.5 Classifier training signal

**Problem.** The coverage matrix grows faster than executable capabilities. If the classifier's retrieval index only contains executable-capability documents, distinguishing similar prompts becomes impossible — "weekly deposit breakdown" (planned) and "monthly deposit breakdown" (implemented) look nearly identical in embedding space.

**Change.** The catalog retrieval-text builder must, for every capability (implemented or planned) and every domain concept, emit a rich retrieval document containing:

- `display_name` and `description`.
- All `examples` (bilingual EN/ID).
- All domain concept `synonyms` transitively usable by this capability.
- Cross-links to related planned capabilities in the same matrix row.
- The matrix row-column pair the capability maps to.

For `planned` capabilities that have no YAML yet, a lightweight retrieval-stub document is added at index time from the `<planned: id>` entries in `docs/reporting-capabilities.md` §11 — enough surface for the classifier to distinguish the prompt from a nearby `implemented` capability and route it to `PlannedUnimplemented`.

### 18.6 Interaction summary

The three changes above compose. A composite request touching three metrics where one is `planned` demotes the whole batch to `PlannedUnimplemented` (§18.2 rule). Adding a new bucket size is a parameter change, not a new capability, so it does not shift any coverage matrix row from `planned` to `implemented` — only new metric families do (§18.1 non-goal). `PlannedUnimplemented` is the outcome that keeps the coverage matrix honest without letting the classifier drift into speculative SQL (§18.3).
