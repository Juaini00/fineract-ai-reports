# 03 — Catalog Validate

**Phase covered:** Phase 10 (YAML referential integrity) + Phase 11 (runtime SQL prepare + output-column contract).
**Precondition:** `API_KEY` from `02`. Fineract DB reachable.

## Request

```bash
curl -X POST {{BASE_URL}}/catalog/validate \
  -H "Authorization: Bearer {{API_KEY}}"
```

## Expected (HTTP 200)
```json
{
  "success": true,
  "data": {
    "valid": true,
    "data_areas": <n>,
    "domains": <n>,
    "capabilities": <n>,
    "queries": <n>
  },
  "error": null
}
```

## What ran
1. YAML load — `crates/chat/src/knowledge/catalog/loader.rs`.
2. Cross-reference validation — `KnowledgeValidator::validate` (capability → domain → query → SQL file).
3. SQL static safety — SELECT-only, single statement, no unsafe tokens, placeholder + cast match.
4. SQL runtime prepare — `validator::validate_runtime` runs `pool.prepare(sql)` against `FINERACT_DATABASE_URL` for every `database: fineract` query and compares the returned columns to the declared `output_fields` (names, in declared order).

## Failure modes

| Trigger | Expected (HTTP 500) |
| --- | --- |
| Capability references unknown domain in YAML | `error.message="capability X references unknown domain Y"` |
| SQL file missing | `error.message="query X references non-existing sql file ..."` |
| SQL parse error (typo, missing table) | `error.message="query X failed prepare against fineract: ..."` |
| Output column drift (rename column in SQL but not YAML) | `error.message="query X output columns [...] do not match declared output_fields [...]"` |
| Unsafe SQL token (e.g. `UPDATE`) | `error.message="query X SQL contains unsafe command UPDATE"` |
| Placeholder/parameter count mismatch | `error.message="query X SQL is missing placeholder $N"` |

## Notes
- This endpoint is **on-demand** — startup does not run runtime prepare. Run after editing any SQL or YAML.
- Authentication is required; the bootstrap admin token is not accepted here.
- Column **type** matching is intentionally not enforced; the executor's `try_get` catches type drift at request time (ponytail: upgrade when output_fields gain typed PG OIDs).
