# Implementation Steps: Phase 11: Query Validation

Source: `docs-old/implementation-steps.md`

## Phase 11: Query Validation

Goal: ensure SQL files are safe before runtime execution.

Validation checks:

1. SQL file exists.
2. SQL is SELECT-only.
3. SQL is not multi-statement.
4. SQL does not contain unsafe commands.
5. Placeholder count matches query metadata.
6. `EXPLAIN` succeeds with sample params.
7. Output columns match output contract when possible.

Unsafe commands include:

```text
INSERT
UPDATE
DELETE
TRUNCATE
DROP
ALTER
CREATE
GRANT
REVOKE
COPY
VACUUM
ANALYZE
```

Current status:

```text
PARTIALLY DONE

Implemented static checks:
SQL file exists
SQL starts with SELECT
SQL is single-statement
SQL does not contain blocked unsafe command tokens
SQL placeholders match declared parameter count/order
basic SQL casts match declared parameter types
office/date/limit clauses are present when required by metadata

Runtime checks added via crates/chat/src/knowledge/catalog/validator.rs::validate_runtime:
SQL is prepared against the Fineract pool (covers parse / EXPLAIN gate without executing rows)
Returned column names are compared to the declared output_fields contract
Wired into POST /catalog/validate; route fails fast on parse or contract mismatch

Still pending:
column type matching against output_fields (currently name-only; runtime executor try_get catches type drift)
table/column cross-check against loaded schema knowledge (depends on Phase 10 schema typing)
```
