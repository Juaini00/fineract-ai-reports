---
type: Policy
title: Query Safety Policy
description: Runtime execution is limited to reviewed SQL files. AI never generates SQL at runtime.
resource: ../../knowledge/policies/query_safety.yaml
tags: [policy, security, sql]
---

# Rules

`select_only`, `single_statement`, `parameterized_only`, `approved_sql_files_only`, `no_runtime_ai_sql`, `require_office_filter_for_fineract`, `require_date_filter_for_transaction_reports`.

Unsafe tokens rejected: `INSERT`, `UPDATE`, `DELETE`, `TRUNCATE`, `DROP`, `ALTER`, `CREATE`, `GRANT`, `REVOKE`, `COPY`, `VACUUM`, `ANALYZE`.

# Enforcement

Static validation runs during `POST /catalog/validate` and at startup via `KnowledgeSyncService`. Runtime `validate_runtime` also prepares the SQL and checks the output-column contract against `output_fields`.

See [docs/knowledge-catalog](../../docs/knowledge/catalog/07-7-validation-rules.md).
