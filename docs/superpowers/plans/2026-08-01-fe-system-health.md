# Frontend System Health Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give an admin one page that answers "is the system healthy, and is the retrieval index current?" — and lets them rebuild the index without a terminal.

**Why this first:** a stale vector index is exactly what made a newly approved capability invisible to retrieval earlier in this project. There was no way to see or fix that from the UI.

**Tech Stack:** React 19 + Vite + TypeScript, TanStack Query, shadcn/base-ui components, Tailwind 4, vitest. Repo: `/Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard`.

## Global Constraints

- Frontend repo only. Do NOT edit the backend repo (`/Users/tabrezakhlaque/project/personal/rust/projects/ai_report`); read it only to confirm contracts.
- Do not add a dependency. Everything needed is already installed.
- `npm run test` and `npm run lint` must both pass before every commit.
- Follow the existing module structure: `src/module/dashboard/` already contains `layout/DashboardLayout`, `pages/`, and `knowledge-base/page.tsx`. Match that structure and the existing code style (terse, early returns, typed services).
- This is a banking admin UI: never render a server-supplied string as raw HTML, and never surface raw error text. Reuse the existing sanitising error pattern from `src/module/chat/service/index.ts` (`safeText`, `serviceError`).
- All three endpoints require an admin Bearer token. Reuse the existing auth header helper pattern rather than reading `localStorage` in a component.

## Backend contract (verified against source, not assumed)

```
GET /management/status                     → ManagementStatusResponse
GET /vector-index/status                   → index snapshot
POST /vector-index/rebuild                 → triggers a re-embed
```

`GET /management/status` returns:
```jsonc
{
  "provider": { "name": "deepseek", "model": "deepseek-chat" },
  "catalog":  { "content_hash": "...", "validation_status": "valid" },
  "index":    { "status": "unavailable", "version_id": null },   // ← SEE WARNING
  "audit":    { "decision_audit_status": "healthy",
                "telemetry": { "dropped_events": 0, "last_persisted_at": null } },
  "features": { "reference_knowledge": false, "cost_warnings": true }
}
```

**WARNING — do not use `status.index`.** It is hardcoded to
`{"status":"unavailable","version_id":null}` in
`crates/chat/src/api/handlers/management.rs`, with the comment "This slice does
not read an index repository." Rendering it would tell the admin the index is
unavailable when it is fine. Index data comes ONLY from `/vector-index/status`:

```jsonc
{
  "catalog_version_id": "c6760aa3-...",
  "version": "local",
  "content_hash": "e32c249c...",
  "status": "embedded",
  "document_count": 117,
  "embedding_model": "voyage-3-large",
  "embedding_dimensions": 1024,
  "synced_at": "2026-07-31T09:27:17Z"
}
```

It may return `null` data when no catalog version has ever been indexed — treat
that as "never indexed", not as an error.

All responses use the envelope `{ success, data, error }`; unwrap with the same
helper the chat service uses.

---

### Task 1: Service layer and types

**Files:**
- Create: `src/module/dashboard/system-health/service.ts`
- Create: `src/module/dashboard/system-health/types.ts`
- Test: `src/module/dashboard/system-health/service.test.ts`

**Interfaces:**
- Produces: `systemHealthService.GetStatus()`, `.GetIndexStatus()`, `.RebuildIndex()`, plus `ManagementStatus` and `VectorIndexStatus` types.

- [ ] **Step 1: Write failing tests**

Cover: each call unwraps the `{success,data,error}` envelope; a `success:false`
response raises a sanitised error and never leaks raw server text; a `null` index
payload resolves to `null` rather than throwing.

- [ ] **Step 2: Run and watch fail**

```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report_dashboard
npm run test -- system-health
```

- [ ] **Step 3: Implement**

Mirror `src/module/chat/service/index.ts`: an auth-header helper, `unwrap`,
and a `serviceError` that maps 401/403/404/500 to safe messages. Do not
re-implement sanitisation from scratch — follow that file's shape.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/module/dashboard/system-health/
git commit -m "feat(dashboard): add system health service"
```

---

### Task 2: Health page with index rebuild

**Files:**
- Create: `src/module/dashboard/system-health/page.tsx`
- Create: `src/module/dashboard/system-health/page.test.tsx`
- Modify: `src/app/router/index.tsx` (add the route)
- Modify: the dashboard navigation component (find it under `src/module/dashboard/layout/`)

- [ ] **Step 1: Write failing tests**

Cover:
- renders provider name/model, catalog validation status, and audit status from `/management/status`;
- renders index `status`, `document_count`, `embedding_model`, `synced_at` from `/vector-index/status`;
- renders a "never indexed" state when the index payload is `null`;
- clicking Rebuild calls `RebuildIndex` and refetches the index status afterwards;
- Rebuild is disabled while in flight, so it cannot be double-fired.

- [ ] **Step 2: Run and watch fail**

- [ ] **Step 3: Implement**

Use TanStack Query for both reads and a mutation for the rebuild, invalidating
the index query on success. Rebuild re-embeds the whole catalog and costs real
money with the embedding provider, so it must ask for confirmation before firing
and must show a clear in-flight state.

Follow `src/module/dashboard/knowledge-base/page.tsx` for page structure and
`DashboardLayout` usage.

- [ ] **Step 4: Tests, lint, commit**

```bash
npm run test && npm run lint
git add src/
git commit -m "feat(dashboard): add system health page with index rebuild"
```

---

## Definition of Done

- [ ] `npm run test` and `npm run lint` pass.
- [ ] The page shows real index state from `/vector-index/status`, never the hardcoded `status.index`.
- [ ] Rebuild asks for confirmation, disables while in flight, and refreshes index state on success.
- [ ] A never-indexed catalog renders as "never indexed", not as an error.
- [ ] Verified against the running backend on http://127.0.0.1:3007 with a real admin login.

## Out of Scope

LLM cost/usage, audit timelines, and knowledge/capability browsing — each is its
own follow-up plan.
