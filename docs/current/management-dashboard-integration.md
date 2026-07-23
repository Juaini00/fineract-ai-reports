# Management dashboard backend integration

Backend contract required by the administration dashboard client.

This document extends [`management-client-integration.md`](./management-client-integration.md). Existing `/management/status`, `/management/knowledge`, `/management/audit`, and `/management/llm-usage` endpoints remain unchanged.

“Management” is an API authorization and governance boundary, not a dedicated UI page. The data in this document is consumed by the main Dashboard. Knowledge Base, Settings, Profile, and other resource-oriented pages may also consume management-authorized APIs.

## Goal

Provide one consistent server-side dashboard snapshot without requiring the client to:

- invent or hardcode business metrics;
- download and aggregate complete audit feeds;
- calculate operational incident thresholds;
- join data from multiple paginated endpoints;
- infer missing usage as zero;
- expose internal database structures.

## Endpoint

### `GET /management/dashboard`

Returns the operational dashboard snapshot for an admin-selected period.

Authentication follows all rules in `management-client-integration.md`:

```http
Authorization: Bearer <admin-session-jwt>
```

`X-API-Key` is ignored.

### Query parameters

| Name | Type | Required | Rules |
| --- | --- | --- | --- |
| `from` | RFC 3339 timestamp | yes | Inclusive range start |
| `to` | RFC 3339 timestamp | yes | Exclusive range end |

Validation:

- `from < to`;
- maximum span is 90 days;
- invalid or excessive ranges return `400 invalid_time_range`;
- unknown query parameters may be ignored according to the existing API convention;
- all timestamps and day buckets are UTC.

Examples:

```http
GET /management/dashboard?from=2026-07-16T00:00:00Z&to=2026-07-23T00:00:00Z
```

```bash
curl -sH "Authorization: Bearer $TOKEN" \
  "$BASE/management/dashboard?from=$FROM&to=$TO"
```

## Response

The standard response envelope is mandatory.

```json
{
  "success": true,
  "data": {
    "range": {
      "from": "2026-07-16T00:00:00Z",
      "to": "2026-07-23T00:00:00Z"
    },
    "generated_at": "2026-07-23T00:00:01Z",
    "status": {
      "provider": {
        "name": "configured-provider",
        "model": "configured-model"
      },
      "catalog": {
        "content_hash": "sha256:catalog",
        "validation_status": "valid"
      },
      "index": {
        "status": "ready",
        "version_id": "uuid-or-null"
      },
      "audit": {
        "decision_audit_status": "healthy",
        "telemetry": {
          "dropped_events": 0,
          "last_persisted_at": "2026-07-23T00:00:00Z"
        }
      },
      "features": {
        "reference_knowledge": false,
        "cost_warnings": true
      }
    },
    "jobs": {
      "created": 148,
      "completed": 141,
      "failed": 3,
      "blocked": 2,
      "awaiting_clarification": 2,
      "active": 4
    },
    "activity_by_day": [
      {
        "date": "2026-07-16",
        "created": 12,
        "completed": 11,
        "failed": 1,
        "blocked": 0
      }
    ],
    "llm_usage": {
      "calls": 120,
      "input_tokens": 3400,
      "output_tokens": 900,
      "total_tokens": 4300,
      "unknown_usage_calls": 0,
      "errors": 1,
      "p95_latency_ms": 812,
      "estimated_cost": {
        "amount": "0.0421",
        "currency": "USD",
        "price_version": "2026-07-01"
      },
      "warnings": []
    },
    "knowledge": {
      "total": 29,
      "available": 25,
      "deferred": 3,
      "unavailable": 1,
      "domains": 7,
      "catalog_version": "sha256:catalog",
      "index_version": "uuid-or-null"
    },
    "recent_audit_events": [],
    "attention_items": []
  },
  "error": null
}
```

Numbers in this document are wire-format examples only. The server must always calculate response values from persisted/configured runtime state.

## Response schema

```ts
interface ManagementDashboardResponse {
  range: {
    from: string;
    to: string;
  };
  generated_at: string;
  status: ManagementStatusResponse;
  jobs: DashboardJobSummary;
  activity_by_day: DashboardDailyActivity[];
  llm_usage: DashboardLlmUsage;
  knowledge: DashboardKnowledgeSummary;
  recent_audit_events: AuditEvent[];
  attention_items: AttentionItem[];
}

interface DashboardJobSummary {
  created: number;
  completed: number;
  failed: number;
  blocked: number;
  awaiting_clarification: number;
  active: number;
}

interface DashboardDailyActivity {
  date: string;
  created: number;
  completed: number;
  failed: number;
  blocked: number;
}

interface DashboardLlmUsage {
  calls: number;
  input_tokens: number | null;
  output_tokens: number | null;
  total_tokens: number | null;
  unknown_usage_calls: number;
  errors: number;
  p95_latency_ms: number | null;
  estimated_cost?: {
    amount: string;
    currency: string;
    price_version: string;
  };
  warnings: WarningCode[];
}

interface DashboardKnowledgeSummary {
  total: number;
  available: number;
  deferred: number;
  unavailable: number;
  domains: number;
  catalog_version: string;
  index_version: string | null;
}

interface AttentionItem {
  id: string;
  kind: string;
  severity: "info" | "warning" | "critical" | string;
  message: string;
  occurred_at: string;
  resource?: {
    type: string;
    id: string;
  };
}
```

`ManagementStatusResponse`, `AuditEvent`, and `WarningCode` use the existing shapes and stable values from `management-client-integration.md`.

Unknown future values for `kind`, `severity`, event types, warning codes, or statuses must remain forward-compatible strings.

## Metric semantics

### Requested range

The requested range is always interpreted as:

```text
[from, to)
```

The start is inclusive and the end is exclusive. This prevents duplicate counting across adjacent dashboard windows.

The response must echo the normalized range in `data.range`.

### Job summary

#### `created`

Number of chat jobs whose `created_at` falls inside `[from, to)`.

#### `completed`

Number of jobs whose successful terminal transition occurred inside `[from, to)`.

This is a transition count, not merely the number of rows whose current status is `completed`.

#### `failed`

Number of jobs whose failed terminal transition occurred inside `[from, to)`.

#### `blocked`

Number of jobs that produced a blocked execution outcome inside `[from, to)`.

A job must not be counted more than once as blocked when retrying or materializing duplicate audit events. Use the job aggregate identity and stable outcome event.

#### `awaiting_clarification`

Current snapshot count at `generated_at` for jobs in `waiting_for_user_input`.

This value is not constrained to jobs created inside the requested range.

#### `active`

Current snapshot count at `generated_at` for jobs in non-terminal execution states, currently:

- `queued`;
- `running`.

Do not include `waiting_for_user_input`, because that state is represented separately by `awaiting_clarification`.

### Daily activity

`activity_by_day` is ordered ascending by date.

The server must include every UTC calendar day intersecting the requested range, including days where all values are zero. The client must never invent missing day buckets.

Each field uses the same event/transition semantics as the job summary.

For a partial first or last UTC day, count only events inside the exact `[from, to)` range.

### LLM usage

Aggregate all LLM traces whose `created_at` falls inside `[from, to)`.

Rules:

- `calls` includes successful and failed calls;
- `errors` counts calls with a failed/non-OK trace status;
- token totals are `null` when one or more included calls has genuinely unknown provider usage;
- zero tokens are valid and are not equivalent to unknown usage;
- `unknown_usage_calls` counts traces whose `usage_status == unavailable`;
- `p95_latency_ms` is `null` when no latency sample exists;
- `estimated_cost` is omitted when cost data is incomplete, currency differs, or multiple price versions collide;
- warnings are advisory and reuse the stable `WarningCode` values from the existing management API.

The implementation should share aggregation logic with `/management/llm-usage` instead of maintaining a second calculation with different semantics.

### Knowledge summary

Knowledge counts are based on the same validated catalog snapshot identified by `catalog_version`.

- `total`: total visible catalog capabilities;
- `available`: capabilities mapped to `available`;
- `deferred`: capabilities mapped to `deferred`;
- `unavailable`: capabilities mapped to `unavailable`;
- `domains`: distinct visible `domain_id` count;
- `catalog_version`: same safe catalog content hash returned by `/management/knowledge`;
- `index_version`: latest matching ready index version, otherwise `null`.

The invariant must hold:

```text
knowledge.total == knowledge.available + knowledge.deferred + knowledge.unavailable
```

Reference knowledge is not included until a real reference source exists.

### Recent audit events

Return at most 10 events ordered by:

```text
occurred_at DESC, id DESC
```

Use the same safe projection as `/management/audit`:

- never expose raw SQL;
- never expose prompts or provider response bodies;
- never expose stack traces or secret configuration;
- preserve nullable `job_id`, `session_id`, and `sanitized_error`;
- unknown event types are returned as their raw string rather than failing the snapshot.

Only events inside `[from, to)` are included.

### Attention items

Attention items are derived by the server from operational state. The frontend does not define incident thresholds.

Initial supported kinds:

| Kind | Suggested severity | Condition |
| --- | --- | --- |
| `audit_delayed` | warning | Audit outbox has a pending backlog beyond the configured delay threshold |
| `audit_unavailable` | critical | Audit repository or dispatcher health cannot be determined |
| `index_unavailable` | warning | No ready knowledge index exists |
| `catalog_invalid` | critical | Loaded catalog validation status is not valid |
| `telemetry_dropped` | warning | Dropped telemetry increased during the requested range |
| `llm_error_rate_high` | warning/critical | LLM error rate exceeds configured threshold |
| `usage_missing` | info/warning | One or more LLM calls has unknown token usage |
| `job_failure_rate_high` | warning/critical | Job failure rate exceeds configured threshold |

Rules:

- maximum 10 items;
- order by severity (`critical`, `warning`, `info`) then `occurred_at DESC`;
- `id` must be stable enough for client rendering/deduplication;
- `message` must be sanitized and display-safe;
- thresholds belong to backend configuration, not the client;
- return an empty array when no condition requires attention;
- do not manufacture positive “all healthy” attention items.

Suggested stable ID format:

```text
<kind>:<resource-id-or-global>:<UTC-date-or-state-version>
```

## Snapshot consistency

The endpoint combines persisted event counts, current state, catalog state, and telemetry.

Requirements:

1. Assign `generated_at` once at the beginning of request processing.
2. Use `generated_at` as the upper boundary for current-state reads when relevant.
3. Use a PostgreSQL read-only transaction with a consistent snapshot for database-derived sections.
4. Capture the in-memory catalog reference once so knowledge counts and catalog hash cannot come from different catalog reloads.
5. The status, knowledge index version, and knowledge summary must refer to compatible catalog/index identities.
6. A failure in a mandatory aggregate section returns a sanitized `500 internal`; do not silently replace failed reads with zero.
7. Operationally unavailable components may be represented by their documented status value when that unavailability is valid domain state rather than a query failure.

## Performance requirements

- Target response time: p95 below 500 ms for a 90-day range under normal production load.
- The endpoint must use database aggregation and bounded result sets; it must not load all jobs, traces, or audit events into application memory.
- Recent audit events are capped at 10.
- Attention items are capped at 10.
- Daily activity is capped naturally by the 90-day validation limit.
- Queries should use existing timeline indexes where possible.
- Add indexes only when supported by query plans from realistic data volumes.
- The response may use a short private cache, recommended maximum 15 seconds, keyed by normalized `from` and `to`. Authorization must still be evaluated on every request.

## Error contract

Reuse the management error envelope.

| Code | HTTP | Meaning |
| --- | --- | --- |
| `unauthorized` | 401 | Bearer token missing or invalid |
| `forbidden` | 403 | Authenticated user is not an admin |
| `invalid_time_range` | 400 | Invalid order or span greater than 90 days |
| `validation_failed` | 400 | Other query validation failure |
| `internal` | 500 | Unexpected aggregate/query failure |

Errors must not expose SQL, schema details, provider payloads, stack traces, prompts, or secrets.

Example:

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

## Recommended backend structure

Keep the endpoint thin and compose existing domain services:

```text
api/routes/management.rs
  GET /management/dashboard

api/handlers/management.rs
  dashboard(query, admin, state)

management/dashboard.rs
  ManagementDashboardService
  - job_summary
  - daily_activity
  - llm_usage_summary
  - knowledge_summary
  - recent_audit_events
  - attention_items
```

Reuse these existing boundaries where possible:

- management status service/repository;
- management audit repository safe projection;
- LLM usage repository aggregation semantics;
- catalog knowledge repository;
- knowledge index repository.

Do not implement the endpoint by making internal HTTP calls to the service's own existing endpoints.

## Settings follow-up

Settings is a separate resource-oriented feature and must not be embedded into the dashboard response.

Future endpoints:

```http
GET   /management/settings
PATCH /management/settings
```

Only allowlisted non-secret settings may be returned or changed. Every mutation requires:

- admin bearer authorization;
- schema and range validation;
- optimistic concurrency using a version or ETag;
- immutable management audit event;
- sanitized response and errors.

Provider secrets, JWT secrets, database URLs, API keys, raw system prompts, and other credentials must never be returned.

## Profile boundary

Profile is user-scoped rather than system-management scoped:

```http
GET /auth/me
```

If editable profile fields are later supported:

```http
PATCH /auth/me
```

Role and permissions must not be editable through the profile endpoint.

## Acceptance criteria

The backend work is complete when:

1. An admin can request 1, 7, 30, and 90-day snapshots.
2. Non-admin bearer users receive `403 forbidden`.
3. Missing/invalid bearer tokens receive `401 unauthorized`.
4. `X-API-Key` does not affect authorization or visibility.
5. Invalid and greater-than-90-day ranges receive `400 invalid_time_range`.
6. Job summary values follow transition semantics and do not double-count retries.
7. Daily activity contains every UTC day in the selected range, including zero days.
8. Unknown LLM usage produces nullable token totals and a nonzero `unknown_usage_calls` count.
9. Mixed currencies or price versions omit `estimated_cost` and return the appropriate warning.
10. Knowledge counts satisfy the documented invariant and use one catalog snapshot.
11. Recent audit events are safe, newest-first, and capped at 10.
12. Attention thresholds are evaluated by the server and results are capped at 10.
13. Query failures do not become fake zero metrics.
14. No response or error leaks SQL, prompts, provider payloads, stack traces, or secrets.
15. The existing management endpoints remain backward compatible.
16. A contract fixture is added at `crates/chat/tests/fixtures/management/dashboard.json`.
17. Integration coverage validates the success envelope, authorization, time-range validation, zero-day buckets, nullable usage semantics, and sanitized failures.
