# Knowledge Catalog: 7. Validation Rules

Source: `docs-old/knowledge-catalog.md`

## 7. Validation Rules

Catalog validation should fail fast when any critical rule is violated.

Critical validation failures:

- Duplicate ids.
- Data scope file missing for an area listed in `docs/reporting-data-scope.md`.
- Machine-readable data scope disagrees with human-readable reporting data scope.
- Unknown data area reference.
- Unknown domain reference.
- Unknown query reference.
- Missing SQL file.
- Query metadata points to an unsafe SQL file.
- Capability marked `approved_mvp` without query metadata.
- Fineract query without office authorization behavior.
- Capability, query, schema, metric, or response references a deferred or out-of-scope data area.
- Capability, query, schema, or metric references a table not listed in approved data scope.
- Query references a column classified as excluded or `secret_never_expose`.
- Output field missing sensitivity classification.
- PII field returned without explicit capability approval.
- Query output contract includes a field marked `secret_never_expose`.

Warnings:

- Candidate capability has no query yet.
- Domain has no approved capability.
- Synonym appears in multiple domains with ambiguous meaning.
- Example phrase maps to multiple capabilities with similar score.
- Schema table is documented but unused by approved capabilities.
- Data area is included but has no schema knowledge yet.
- Data area is included but has no capability yet.
