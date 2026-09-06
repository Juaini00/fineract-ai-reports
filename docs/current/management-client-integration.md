# Management client integration

Everything a client needs to consume the `/management/*` admin API: URLs, auth,
request parameters, response shapes, and stable enum values.

## Base and transport

- Base URL: `http://<host>:3007` (local dev on `:3007`).
- Content type: `application/json`.
- Every response uses the standard envelope:

```json
{ "success": true,  "data": { ... }, "error": null }
{ "success": false, "data": null,     "error": { "code": "...", "message": "...", "details": null } }
```

## Authentication

Every `/management/*` route requires a **Bearer session JWT** for a user whose
`role == "admin"`.

```
Authorization: Bearer <session-jwt>
```

- Missing bearer → `401 unauthorized`.
- Bearer for a non-admin user → `403 forbidden`.
- `X-API-Key` is **ignored** for management routes — it never changes visibility
  or authorization, and a bad key never turns a valid bearer request into 401.

## Common validation rules

- Time ranges (`from`, `to`) are RFC 3339 timestamps. `from < to`, span ≤ **90 days**,
  else `400 invalid_time_range`.
- `limit` is `1..=100` (defaults to 50 when omitted).
- `cursor` is opaque — pass back exactly what the previous page returned in
  `next_cursor`; do not decode. Invalid cursor → `400 invalid_cursor`.
- Sanitized errors: responses never leak raw SQL, prompts, provider text,
  stack traces, or secret configuration.

## Endpoints

### `GET /management/status`

Operational snapshot: provider identity, catalog/index/audit health, feature flags.

Query params: none.

Response `data`:

```json
{
  "provider": { "name": "configured-provider", "model": "configured-model" },
  "catalog":  { "content_hash": "sha256:catalog", "validation_status": "valid" },
  "index":    { "status": "ready", "version_id": "uuid|null" },
  "audit":    {
    "decision_audit_status": "healthy",
    "telemetry": { "dropped_events": 0, "last_persisted_at": null }
  },
  "features": { "reference_knowledge": false, "cost_warnings": true }
}
```

`decision_audit_status` ∈ `healthy | delayed | unavailable`.
`index.status` ∈ `ready | unavailable`.

---

### `GET /management/knowledge`

Safe catalog inventory. IDs are shaped `catalog:<capability_id>`.

Query params:

| Name        | Type                                   | Notes                                        |
| ----------- | -------------------------------------- | -------------------------------------------- |
| `kind`      | `catalog \| reference`                 | Optional. `reference` returns empty + status |
| `status`    | `available \| deferred \| unavailable` | Optional filter                              |
| `domain_id` | string, ≤ 128                          | Optional filter                              |
| `cursor`    | opaque                                 | From previous page                           |
| `limit`     | 1..=100                                | Default 50                                   |

Response `data`:

```json
{
  "items": [
    {
      "id": "catalog:savings_deposit_total",
      "kind": "catalog",
      "title": "Total deposits",
      "status": "available",
      "execution_mode": "approved_catalog_query",
      "domain_id": "savings"
    }
  ],
  "next_cursor": null,
  "catalog_version": "sha256:catalog",
  "index_version": "uuid|null",
  "reference_knowledge_status": "disabled"
}
```

`reference_knowledge_status` is only present when `kind=reference` was requested
and returns `"disabled"` — no reference source exists yet.

---

### `GET /management/knowledge/{id}`

Detailed metadata for one catalog item. `{id}` is the `catalog:<capability_id>`
form returned by the list endpoint.

Response `data`:

```json
{
  "id": "catalog:savings_deposit_total",
  "kind": "catalog",
  "title": "Total deposits",
  "status": "available",
  "execution_mode": "approved_catalog_query",
  "domain_id": "savings",
  "data_area_ids": ["savings_transactions"],
  "parameters":    [{ "name": "office_id", "type": "integer", "required": true }],
  "output_fields": [{ "name": "total_amount", "sensitivity": "public" }],
  "limitations": []
}
```

`404 not_found` when the id does not exist or is not visible.

---

### `GET /management/audit`

Newest-first immutable decision-audit feed.

Query params:

| Name         | Type                                                           | Required |
| ------------ | -------------------------------------------------------------- | -------- |
| `from`, `to` | RFC 3339                                                       | **yes**  |
| `event_type` | see stable values below                                        | no       |
| `outcome`    | `success \| blocked \| clarification \| unsupported \| failed` | no       |
| `job_id`     | UUID                                                           | no       |
| `session_id` | UUID                                                           | no       |
| `cursor`     | opaque                                                         | no       |
| `limit`      | 1..=100 (default 50)                                           | no       |

Response `data`:

```json
{
  "items": [
    {
      "id": "uuid",
      "job_id": "uuid|null",
      "session_id": "uuid|null",
      "aggregate_type": "chat_job",
      "event_type": "chat_job_completed",
      "outcome": "success",
      "summary":  { "kind": "job_completed" },
      "sanitized_error": null,
      "occurred_at": "2026-07-23T00:00:00Z"
    }
  ],
  "next_cursor": "opaque-cursor-or-null"
}
```

Paginate by re-issuing the same query with `cursor=<next_cursor>`.

---

### `GET /management/audit/jobs/{job_id}`

Same feed filtered to one job. Accepts the same optional filters as
`/management/audit` except `job_id`, which is bound to the path.

---

### `GET /management/llm-usage`

Aggregate LLM telemetry.

Query params:

| Name         | Type                                | Required |
| ------------ | ----------------------------------- | -------- |
| `from`, `to` | RFC 3339 (≤ 90 days)                | **yes**  |
| `group_by`   | `day \| model \| purpose \| status` | **yes**  |

Response `data`:

```json
{
  "range": { "from": "...", "to": "..." },
  "groups": [
    {
      "key": "2026-07-22T00:00:00Z",
      "calls": 12,
      "input_tokens": 3400,
      "output_tokens": 900,
      "total_tokens": 4300,
      "unknown_usage_calls": 0,
      "errors": 0,
      "p95_latency_ms": 812,
      "estimated_cost": {
        "amount": "0.0421",
        "currency": "USD",
        "price_version": "2026-07-01"
      }
    }
  ],
  "warnings": ["usage_missing", "price_version_mismatch"]
}
```

Semantic rules:

- Token fields are `null` when the provider did not report usage.
- `unknown_usage_calls` counts calls whose usage is truly unknown — it is
  **not** the same as zero tokens.
- `estimated_cost` is omitted for a group when price data is unavailable or
  multiple price versions collide.
- `warnings` are advisory (never block jobs).

## Stable enum values

| Field            | Values                                                              |
| ---------------- | ------------------------------------------------------------------- |
| `execution_mode` | `approved_catalog_query`, `catalog_metadata_only`, `reference_only` |
| `KnowledgeStatus`| `available`, `deferred`, `unavailable`                              |
| `outcome`        | `success`, `blocked`, `clarification`, `unsupported`, `failed`      |
| `aggregate_type` | `chat_job`, `chat_session`, `management`                            |
| `AuditEventType` | `chat_job_created`, `knowledge_retrieval_completed`, `context_assembled`, `policy_evaluated`, `execution_authorized`, `execution_blocked`, `execution_completed`, `chat_clarification_requested`, `chat_clarification_received`, `chat_job_completed`, `chat_job_failed`, `chat_session_archived`, `chat_session_deleted` |
| `WarningCode`    | `usage_missing`, `cost_estimate_unavailable`, `price_version_mismatch`, `telemetry_dropped`, `unusual_usage_detected` |
| `HealthStatus`   | `healthy`, `delayed`, `unavailable`                                 |

Treat unknown enum values from a newer server as fall-through: render the raw
string, do not fail the request.

## Error contract

```json
{
  "success": false,
  "data": null,
  "error": {
    "code": "invalid_time_range",
    "message": "Request validation failed.",
    "details": null
  }
}
```

Common `error.code` values:

| Code                 | HTTP | When                                               |
| -------------------- | ---- | -------------------------------------------------- |
| `unauthorized`       | 401  | Missing or invalid bearer                          |
| `forbidden`          | 403  | Bearer user role is not admin                      |
| `not_found`          | 404  | Unknown resource id                                |
| `invalid_cursor`     | 400  | Cursor was tampered with or is stale               |
| `invalid_time_range` | 400  | `from >= to` or span > 90 days                     |
| `validation_failed`  | 400  | Query params failed schema/range constraints       |
| `internal`           | 500  | Unexpected server error (details are always empty) |

## curl quickstart

```bash
BASE=http://127.0.0.1:3007
TOKEN=<admin-session-jwt>

# Operational status
curl -sH "Authorization: Bearer $TOKEN" "$BASE/management/status"

# Knowledge inventory, filter by domain
curl -sH "Authorization: Bearer $TOKEN" \
  "$BASE/management/knowledge?domain_id=savings&limit=20"

# Audit feed for a 7-day window
FROM=2026-07-16T00:00:00Z
TO=2026-07-23T00:00:00Z
curl -sH "Authorization: Bearer $TOKEN" \
  "$BASE/management/audit?from=$FROM&to=$TO&outcome=failed"

# LLM cost per day
curl -sH "Authorization: Bearer $TOKEN" \
  "$BASE/management/llm-usage?from=$FROM&to=$TO&group_by=day"
```

## TypeScript reference

```ts
type Envelope<T> =
  | { success: true;  data: T;    error: null }
  | { success: false; data: null; error: { code: string; message: string; details: unknown | null } };

type ExecutionMode   = "approved_catalog_query" | "catalog_metadata_only" | "reference_only";
type KnowledgeStatus = "available" | "deferred" | "unavailable";
type AuditOutcome    = "success" | "blocked" | "clarification" | "unsupported" | "failed";
type WarningCode     = "usage_missing" | "cost_estimate_unavailable" | "price_version_mismatch" | "telemetry_dropped" | "unusual_usage_detected";

interface KnowledgeItem {
  id: string;
  kind: "catalog" | "reference";
  title: string;
  status: KnowledgeStatus;
  execution_mode: ExecutionMode;
  domain_id: string;
}

interface KnowledgeListResponse {
  items: KnowledgeItem[];
  next_cursor: string | null;
  catalog_version: string;
  index_version: string | null;
  reference_knowledge_status?: "disabled";
}

interface AuditEvent {
  id: string;
  job_id: string | null;
  session_id: string | null;
  aggregate_type: "chat_job" | "chat_session" | "management";
  event_type: string;                     // treat unknowns as fall-through
  outcome: AuditOutcome;
  summary: Record<string, unknown>;
  sanitized_error: { code: string } | null;
  occurred_at: string;                    // RFC 3339
}

interface UsageGroup {
  key: string;
  calls: number;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  unknown_usage_calls: number;
  errors: number;
  p95_latency_ms: number | null;
  estimated_cost?: { amount: string; currency: string; price_version: string };
}

interface LlmUsageResponse {
  range: { from: string; to: string };
  groups: UsageGroup[];
  warnings: WarningCode[];
}
```

## Retention

Session deletion is archive-only. Management decision audit uses nullable live
relations plus immutable aggregate snapshots, so archiving or controlled
relation removal does not erase its identity. Automatic audit and telemetry
purging is not implemented.

## Legacy compatibility

- `GET /chat/jobs/{job_id}/audit` continues to return its existing shape;
  the new management feed is additive.
- Clients migrating to `/management/audit/jobs/{job_id}` gain filters and
  cursors and stop parsing internal fields.

## Reference fixtures

Wire-format examples used by contract tests
(`crates/chat/tests/fixtures/management/*.json`):

- `status.json` — `/management/status`
- `knowledge-list.json`, `knowledge-detail.json`, `reference-disabled.json`
- `audit-list.json`, `audit-job.json`
- `llm-usage.json`
- `error.json` — sanitized error envelope
