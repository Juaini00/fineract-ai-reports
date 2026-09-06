# Frontend LLM Cost & Usage Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show an admin what the LLM actually costs and where the latency goes, over a chosen date range.

**Why this next:** instrumenting the pipeline revealed that routing (~2500ms) and reranking (~2000ms) account for roughly 90% of request latency, while the SQL itself takes ~6ms. That was invisible until now, and it is only actionable if it can be tracked over time rather than observed once.

**Tech Stack:** React 19 + Vite + TypeScript, TanStack Query, Tailwind 4, vitest. Repo: `/Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard`.

## Global Constraints

- Frontend repo only. Do NOT edit the backend repo; read it only to confirm contracts.
- Do not add a dependency. If a chart is built, it must use what is already installed or hand-written SVG.
- `npm run test` and `npm run lint` must pass before every commit.
- Follow the structure just established in `src/module/dashboard/system-health/` (service + types + page + tests) and the service shape in `src/module/chat/service/index.ts` (auth headers, envelope unwrap, sanitised errors).
- Banking admin UI: never render a server-supplied string as raw HTML, never surface raw error text.

## Backend contract (read from source, not assumed)

```
GET /management/llm-usage?from=<iso8601>&to=<iso8601>&group_by=day|model|purpose|status
```

`group_by` is **required** — the backend DTO is `pub group_by: LlmGroupBy`, not an
`Option`. The UI must always send one. Valid values: `day`, `model`, `purpose`, `status`.

```jsonc
{
  "range": { "from": "2026-07-01T00:00:00Z", "to": "2026-08-01T00:00:00Z" },
  "groups": [
    {
      "key": "router",
      "calls": 1284,
      "input_tokens": 412900,        // nullable
      "output_tokens": 38210,        // nullable
      "total_tokens": 451110,        // nullable
      "unknown_usage_calls": 3,
      "errors": 7,
      "p95_latency_ms": 2491,        // nullable
      "estimated_cost": {            // ABSENT when price versions are mixed
        "amount": "12.40", "currency": "USD", "price_version": "v1"
      }
    }
  ],
  "warnings": ["usage_missing" | "cost_estimate_unavailable" | "price_version_mismatch"]
}
```

## THE TRAP — null and absent are NOT zero

This is a cost page. Rendering an unknown value as `0` understates spend and
misleads the person deciding whether the system is affordable. It is a worse
failure than showing nothing.

- `input_tokens`, `output_tokens`, `total_tokens`, `p95_latency_ms` are **nullable**.
  `null` means *not reported*, not *none*. Render `—` (or "unknown"), never `0`.
- `estimated_cost` is **absent** when a group spans more than one price version
  (backend: `estimated_cost` is only set when `row.price_versions <= 1`). An absent
  cost means *cannot be computed*, not *free*. Never sum absent costs into a total
  as zero.
- `unknown_usage_calls` is how many calls in that group reported no usage at all.
  If it is greater than zero, any token or cost figure for that group is a
  **lower bound**. Say so in the UI.
- `warnings` explains why figures may be incomplete. Surface them; do not swallow them.
- `estimated_cost.amount` is a decimal serialised by the backend. Do not do
  floating-point arithmetic on it for display; treat it as a string, or sum with
  care and label the result as an estimate.

A totals row is only honest if it states what it excluded. If any group in the
range lacks a cost, the total must be presented as "at least X" with a note,
not as a clean number.

---

### Task 1: Service layer and types

**Files:**
- Create: `src/module/dashboard/llm-usage/service.ts`, `types.ts`
- Test: `src/module/dashboard/llm-usage/service.test.ts`

**Interfaces:**
- Produces: `llmUsageService.GetUsage({ from, to, groupBy })`, plus `LlmUsageResponse`, `UsageGroup`, `UsageCost`, `LlmGroupBy`, `UsageWarning` types.
- `groupBy` is required in the call signature, mirroring the backend.

- [ ] **Step 1: Write failing tests**

Cover: query string is built with all three params; the envelope is unwrapped;
`success:false` raises a sanitised error with no raw server text; nullable token
fields survive as `null` (NOT coerced to 0) through the type boundary; a group
with no `estimated_cost` stays `undefined` rather than becoming a zero cost.

- [ ] **Step 2: Run and watch fail**

```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard
npm run test -- llm-usage
```

- [ ] **Step 3: Implement**

Mirror `src/module/dashboard/system-health/service.ts`.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/module/dashboard/llm-usage/
git commit -m "feat(dashboard): add llm usage service"
```

---

### Task 2: Usage page

**Files:**
- Create: `src/module/dashboard/llm-usage/page.tsx`, `page.test.tsx`
- Modify: `src/app/router/index.tsx`
- Modify: the dashboard navigation component under `src/module/dashboard/layout/`

**BEFORE WRITING ANY CHART, STAT TILE, OR KPI ROW:** load the `dataviz` skill and
follow it. It governs chart form, colour, and accessibility. If you decide a
table alone is clearer than a chart, that is a legitimate choice — say so in your
report rather than adding a chart for decoration.

Note `group_by=day` is a time series; `model`, `purpose` and `status` are
categorical. They do not want the same visual treatment.

- [ ] **Step 1: Write failing tests**

Cover:
- a date range control and a `group_by` selector, both feeding the query;
- a group with `total_tokens: null` renders `—`, **not** `0`;
- a group with no `estimated_cost` renders "unavailable", **not** `$0.00`;
- a group with `unknown_usage_calls > 0` is visibly marked as a lower bound;
- returned `warnings` are rendered;
- an empty `groups` array renders an empty state, not a broken table.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement**

TanStack Query keyed on `{from, to, groupBy}`. Default the range to the last 7
days and `group_by` to `purpose` — that is the grouping that exposes the
routing/reranking latency split described in the goal.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/
git commit -m "feat(dashboard): add llm cost and usage page"
```

---

## Definition of Done

- [ ] `npm run test` and `npm run lint` pass.
- [ ] Null tokens and absent costs render as unknown/unavailable — never as zero.
- [ ] Groups with `unknown_usage_calls > 0` are marked as lower bounds.
- [ ] Backend `warnings` are surfaced.
- [ ] All four `group_by` values work against the running backend.
- [ ] Verified against http://127.0.0.1:3007 with a real admin login, with the actual response recorded.

## Out of Scope

Audit timelines and knowledge/capability browsing — separate plans.
