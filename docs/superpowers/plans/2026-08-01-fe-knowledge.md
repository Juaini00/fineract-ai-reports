# Frontend Knowledge Catalog Browser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an admin browse what the assistant is actually allowed to answer — every approved capability, its parameters, its output columns, and its stated limitations.

**Tech Stack:** React 19 + Vite + TypeScript, TanStack Query, Tailwind 4, vitest. Repo: `/Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard`.

## Global Constraints

- Frontend repo only. Do NOT edit the backend repo; read it only to confirm contracts.
- Do not add a dependency.
- `npm run test` and `npm run lint` must pass before every commit.
- Follow the structure established in `src/module/dashboard/audit/`, `llm-usage/` and `system-health/` (service + types + page + tests).
- Banking admin UI: never render a server-supplied string as raw HTML.

## `/catalog/capabilities` IS EXCLUDED — and this is a security decision, not an oversight

`GET /catalog/capabilities` does **not** accept a session JWT. Its handler calls
`app_core::api::handlers::auth::authorize_bootstrap_admin`, which compares the
Bearer token against the static `AUTH_BOOTSTRAP_ADMIN_TOKEN` from configuration
(`crates/core/src/api/handlers/auth.rs:159-168`). A normal admin login receives
**403 "invalid bootstrap admin token"**.

That token is the credential which provisions API keys. Shipping it to a browser
SPA so this page could call the endpoint would be a real security regression.

**Do not attempt to call `/catalog/capabilities` from the frontend.** Everything
this page needs is available from `/management/knowledge`, which is properly
gated by `AuthenticatedManagementAdmin` (session JWT). If the endpoint is ever
wanted in the UI, the fix belongs in the backend's auth model, not here.

## Backend contract (read from source — verify the list item shape live)

```
GET /management/knowledge          (AuthenticatedManagementAdmin — session JWT)
GET /management/knowledge/{id}     (same)
```

List response:
```jsonc
{
  "items": [ /* verify the exact item shape against the live response */ ],
  "next_cursor": "..." | null,
  "catalog_version": "...",
  "index_version": "...",
  "reference_knowledge_status": "..."
}
```

Detail response (`KnowledgeDetailResponse`):
```jsonc
{
  "id": "catalog:savings_deposit_total",
  "kind": "catalog",
  "title": "...",
  "status": "...",
  "execution_mode": "...",
  "domain_id": "savings",
  "data_area_ids": ["..."],
  "parameters": [ /* ParameterResponse */ ],
  "output_fields": [ /* OutputFieldResponse */ ],
  "limitations": ["..."]
}
```

### TRAP 1 — the detail id requires a `catalog:` prefix

`KnowledgeService::detail` begins with `public_id.strip_prefix("catalog:")?`
(`crates/chat/src/management/knowledge.rs:86`). The `?` on an `Option` means a
bare capability id returns `None`, which the handler turns into **404 "Knowledge
item was not found."**

So `GET /management/knowledge/savings_deposit_total` → 404, while
`GET /management/knowledge/catalog:savings_deposit_total` → 200.

The list already returns ids in prefixed form. Pass the id through unchanged;
never strip it, and never reconstruct it from a capability name. URL-encode it
when placing it in a path — it contains a colon.

### TRAP 2 — `kind=reference` means "feature disabled", not "no results"

`KnowledgeService::list` short-circuits on `Some(KnowledgeKind::Reference)` and
returns a *disabled* response (`knowledge.rs:50-52`). This matches
`features.reference_knowledge: false` reported by `/management/status`.

An empty list there means the feature is switched off, not that a search found
nothing. Rendering "No results" would send an admin hunting for data that cannot
exist. Surface `reference_knowledge_status` and say the feature is unavailable.

### TRAP 3 — the cursor is an item id, not an offset

`list` resolves the cursor by `items.iter().position(|item| item.id == cursor)`
(`knowledge.rs:62-65`). Pass `next_cursor` back verbatim. An invalid cursor
returns HTTP 400 with error code `invalid_cursor` — handle it by resetting
pagination rather than showing a generic failure.

---

### Task 1: Service layer and types

**Files:**
- Create: `src/module/dashboard/knowledge/service.ts`, `types.ts`
- Test: `src/module/dashboard/knowledge/service.test.ts`

**Interfaces:**
- Produces: `knowledgeService.ListItems({ kind?, cursor?, ... })` and `.GetDetail(id)`, plus `KnowledgeListResponse`, `KnowledgeDetail`, `ParameterResponse`, `OutputFieldResponse` types.
- `GetDetail` takes the id **exactly as the list returned it** (already `catalog:`-prefixed) and URL-encodes it for the path.

- [ ] **Step 1: Write failing tests**

Cover: the envelope is unwrapped; `GetDetail` URL-encodes the colon and does not
strip the prefix; a 404 from detail surfaces as a not-found error rather than a
generic failure; a 400 `invalid_cursor` is distinguishable so the page can reset;
a disabled `reference` response is not mistaken for an empty result.

- [ ] **Step 2: Run and watch fail**

```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard
npm run test -- knowledge
```

Note there is an existing unrelated `knowledge-base` module — make sure your
filter does not confuse the two, and do not modify it.

- [ ] **Step 3: Implement**

Mirror `src/module/dashboard/audit/service.ts`.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/module/dashboard/knowledge/
git commit -m "feat(dashboard): add knowledge catalog service"
```

---

### Task 2: Catalog browser page with detail view

**Files:**
- Create: `src/module/dashboard/knowledge/page.tsx`, `page.test.tsx`
- Modify: `src/app/router/index.tsx`
- Modify: the dashboard navigation component under `src/module/dashboard/layout/`

- [ ] **Step 1: Write failing tests**

Cover:
- the list renders items with their domain and status;
- selecting an item fetches detail using the **unmodified prefixed id**;
- detail renders parameters, output fields and limitations;
- an item with no `limitations` renders nothing for that section rather than an empty heading;
- `reference_knowledge_status` disabled renders "feature unavailable", **not** "no results";
- "Load more" passes `next_cursor` verbatim; `invalid_cursor` resets to the first page;
- an empty `items` array renders an empty state, not a broken list.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement**

TanStack Query for both list and detail, detail keyed on the selected id.
`catalog_version` and `index_version` in the list response tell an admin which
catalog they are looking at — show them; they are the same versions the System
Health page reports, and a mismatch is meaningful.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/
git commit -m "feat(dashboard): add knowledge catalog browser"
```

---

## Definition of Done

- [ ] `npm run test` and `npm run lint` pass.
- [ ] Detail fetch uses the prefixed id and works against the live backend (a bare id would 404).
- [ ] Reference knowledge renders as "feature unavailable", never as "no results".
- [ ] Cursor pagination passes `next_cursor` verbatim; `invalid_cursor` resets.
- [ ] `/catalog/capabilities` is NOT called from the frontend.
- [ ] Verified against http://127.0.0.1:3007 with a real admin login, with the actual list and detail responses recorded.

## Out of Scope

`/catalog/capabilities` (see the security note above) and `GET /chat/jobs/{job_id}/audit`.
