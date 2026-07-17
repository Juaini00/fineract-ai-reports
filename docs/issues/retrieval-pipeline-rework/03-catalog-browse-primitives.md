# 03 — Add browse/list primitives to catalog

**Parent:** [Epic](./README.md) · **Priority:** P1 · **Effort:** S (~4 YAML + 3 SQL files)

## Problem

Two of three failing queries from the epic README ask for a simple browse:

- "berikan 3 office yg ada pada system saat ini"
- "coba berikan saya 5 client sembarang pada tahun ini"

The catalog has no capability for "give me N of X" without a ranking metric. Every client/office capability is either a top-N-by-metric, a summary, or a hierarchy. A user who doesn't know the available metrics has no path to a basic list.

## Proposed change

Add three low-cost capabilities:

| id | shape | Notes |
|---|---|---|
| `office_list_basic` | `op=list, subj=office, output=list, pii=none` | Lists offices in authorized scope. Bounded by `limit` (default 50, max 200). |
| `client_list_recent` | `op=list, subj=client, grouping=none, output=list, pii=client_identity` | Recently activated clients in scope, most recent first. Bounded by `limit`. |
| `client_random_sample` | `op=random_sample, subj=client, output=list, pii=client_identity` | Random sample using `TABLESAMPLE SYSTEM_ROWS(n)` or `ORDER BY random()` if extension unavailable. Bounded (max 50). |

Each capability needs:

- `knowledge/capabilities/<domain>/<name>.yaml` — declaration.
- `knowledge/queries/<domain>/<name>.yaml` — SQL contract with parameter/output declarations.
- `queries/<domain>/<name>.sql` — approved SQL, uses `office_ids` bound parameter for scope.

## Files

- `knowledge/capabilities/organization/office_list_basic.yaml`
- `knowledge/queries/organization/office_list_basic.yaml`
- `queries/organization/office_list_basic.sql`
- `knowledge/capabilities/client/client_list_recent.yaml`
- `knowledge/queries/client/client_list_recent.yaml`
- `queries/client/client_list_recent.sql`
- `knowledge/capabilities/client/client_random_sample.yaml`
- `knowledge/queries/client/client_random_sample.yaml`
- `queries/client/client_random_sample.sql`
- Vector index rebuild after adding capabilities: hit `POST /vector-index/rebuild`.

## Acceptance criteria

- All three new capabilities pass `POST /catalog/validate`.
- All three respect office scope — a caller without office_id X sees no rows from that office.
- `client_random_sample` respects `can_view_pii` — masks PII fields when caller lacks it.
- Failing queries in epic README route to these new capabilities post-embedding-index-rebuild.

## Test plan

- `chat/tests/` integration test per capability: create caller with limited office scope, assert result rows all belong to allowed offices.
- Fixture query "berikan 3 office" → asserts top-1 is `office_list_basic`.

## Out of scope

- Multi-domain browse (e.g. "browse everything"). Users can specify a domain.
- Filters beyond `limit`. Follow-up if needed.

## Dependencies

- None. Landable independently, but pairs well with issue 01 so the new capabilities are actually reachable.
