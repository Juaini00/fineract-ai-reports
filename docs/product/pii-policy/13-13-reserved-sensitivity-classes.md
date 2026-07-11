# Reporting PII Policy: 13. Reserved Sensitivity Classes

Source: `docs-old/reporting-pii-policy.md`

## 13. Reserved Sensitivity Classes

Section 2 defines six sensitivity classes. Only two — `public_business` and `pii` — are actively enforced by any currently implemented capability. The other four are **reserved**: documented in advance so that policy design is future-proof, and so that reviewers of new capabilities never have to invent a new class ad hoc.

Reserved classes are not dead code. Every reserved class has a concrete planned capability (see [`docs/capability-coverage-matrix.md`](../capability-coverage/index.md)) that will activate it. When those capabilities land, the class moves from "reserved" to "active" and the mask/omit rules already defined in §§3–8 apply without further policy changes.

| Class | Status | Guards which planned capability | What it protects |
| --- | --- | --- | --- |
| `sensitive_business_identifier` | reserved | `savings_activity_list`, `savings_charge_outstanding_breakdown`, `office_directory` when it starts including internal codes | Savings `account_no`, savings `external_id`, transaction `external_id`, product internal codes. Never shown even with `can_view_pii=true`; only aggregate references allowed. |
| `security_sensitive` | reserved | `audit-users-operations` domain (deferred) — password hashes, session tokens, API-key hashes | Never returned in any capability response regardless of PII flag. If a reviewer sees a column of this class in a proposed SELECT list, the capability is rejected. |
| `secret_never_expose` | reserved | Payment reference detail on `savings_activity_list`, hold reference detail on `savings_hold_balance` capabilities | `ref_no`, payment channel account numbers, encrypted payment payloads, third-party gateway ids. Hard exclude — no mask, no summary, no count-only. Present in Fineract, permanently absent from any AI response. |
| `free_text_sensitive` | reserved | `savings_activity_list` (transaction description), `client_demographics_summary` if it includes free-text address/employer notes | User-authored free-text fields that may contain PII, business intent, or slurs. Excluded by default; a future capability may summarize by fixed enum only. |

Rules that apply to reserved classes today, before any planned capability lands:

- The referential-integrity validator flags any capability YAML that declares an output field belonging to a reserved class, unless the capability also references the appropriate planned entry in `docs/capability-coverage-matrix.md`. This prevents accidental early exposure.
- The AI prompt safety rules in §11 already forbid the classifier or planner from suggesting fields it hasn't seen in an `approved_mvp` capability; reserved-class fields are therefore unreachable via prompt injection.
- `POST /catalog/validate` cross-checks the sensitivity class of every declared output field against the capability's status. `approved_mvp` capabilities may not declare reserved-class fields.

Migration path when activating a reserved class:

1. Add or flip the capability to `implemented` in the coverage matrix.
2. Update this section: move the class row into §§4–6 as appropriate, or add a new numbered subsection describing the exact fields now in scope.
3. Update the capability YAML with explicit output field declarations.
4. Update review checklist §12 questions if the class introduces a new gating flag.
