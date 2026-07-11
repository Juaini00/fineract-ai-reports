# Capability Coverage Matrix: Status legend

Source: `docs-old/capability-coverage-matrix.md`

## Status legend

| Value | Meaning |
| --- | --- |
| `implemented` | A capability YAML with runtime status `approved_mvp` exists in `knowledge/capabilities/` and is executable end-to-end. Cell links the capability id. Doc-facing term: **enabled capability**. |
| `planned` | On the roadmap. No approved capability YAML yet. Classifier semantically matches user intent and the job ends `planned_unimplemented` (see mapping below). Target milestone in parentheses. |
| `deferred` | The whole data area or domain is deferred (loan, accounting, tax, custom-datatables, audit-users-operations). In-scope for the product commitment but not yet activated; requires domain-level approval, not just a code change. |
| `out_of_scope` | Will never be built even when asked (writes, arbitrary SQL, raw account numbers, cross-tenant reads, schema exploration). Reason must be documented. |
| `—` | Combination is not meaningful (e.g. a snapshot has no monthly breakdown). |
