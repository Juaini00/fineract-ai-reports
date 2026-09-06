# Issue 007 — Bundle 8: Catalog Retrieval Remediation — Design

**Bundle:** B8 (W-A3 + W-D2) of `docs/superpowers/plans/2026-07-27-issue-007-program-roadmap.md`.

## Fresh audit

Bundle 7's real-catalog suite is green only because its 28 observed retrieval failures are held in an explicit ledger. The fallback scorer adds the same request-shape boost to each candidate because the test derives the request shape from the intended capability. Its remaining signal is catalogue text matching, so sparse or overlapping descriptions/examples produce ties and wrong winners. The fixed policy remains `min_floor: 0.40` and `min_gap: 0.05`.

The E1 English pending-charges phrase is affected because the target lacks the analyst vocabulary already represented in its approved output (`unpaid`, `due`, `overdue`, `paid`, and `balance`). The other ledger rows are the same metadata problem: missing Indonesian vocabulary, singular/plural variants, or ambiguous lifecycle/office wording.

The audit also corrects one stale inventory claim: `savings_pending_charges_clients` already selects and declares `amount_levied_total`, so G1 is covered. G2 is a real capability gap: the existing approved SQL intentionally returns all unpaid outstanding charges and cannot truthfully answer an *overdue-only* request by client-side filtering. It needs a narrow, separately approved query backed by the existing savings-charge data area and metric.

## Goal

Make every covered savings, client, and organization inventory phrase rank its intended approved capability first and clear the unchanged floor/gap; keep missing loan phrases explicitly Unsupported for Issue 008. Promote the two savings-charge inventory rows to covered: one by recording the shipped `amount_levied_total` field accurately, one by adding an approved strictly-overdue charge list.

## Design

- Enrich target capability `description` and `examples` with truthful analyst vocabulary and concise Indonesian equivalents. Normalize fallback lexical coverage by request-term count so broad shared terms cannot reach the 0.99 cap before catalog-specific vocabulary differentiates candidates. Do not alter request shapes, floors, gaps, or test phrases.
- Refine the two office/client-count capability descriptions so their real output distinction is discoverable: `client_summary_by_office` is lifecycle-status distribution, while `organization_office_client_summary` is the office population view including offices without clients. This removes the root metadata collision without changing either approved query.
- Add `savings_strictly_overdue_charges_clients` with a matching query and SQL statement. It reuses the existing unpaid-charge metric and exact output contract, binds `office_ids` in SQL, and adds the necessary `charge_due_date < as_of_date` predicate. It does not relabel future-due or undated outstanding charges as overdue.
- Convert the Bundle 7 known-gap test into a direct rank/floor/gap assertion and move G1/G2 from partial to covered. The historical 28-row ledger remains in the inventory, marked resolved with the remediation category instead of being silently erased.

## Constraints

Approved SQL only; one parameterized `SELECT`; `office_ids = ANY($1::bigint[])` inside SQL; PII labels and English-only product copy preserved; no new crate, dependency, migration, scorer rule, floor/gap change, or phrase-specific production conditional. The new query selects only the approved existing charge output fields; client display name remains `pii` at the query contract.

## Risks and success criteria

The only new capability is tied to inventory G2 and uses current in-scope tables. Validation must prove the real catalog loads; all 62 covered inventory phrases pass rank/floor/gap with `0.40`/`0.05`; the ten missing loan phrases remain Unsupported with no catalog alternatives; normalized lexical coverage does not saturate broad candidates; and the strict-overdue capability is mapped to an approved query with the required due-date predicate. The suite supplies the intended request shape, so it proves catalog fallback retrieval rather than prompt-to-SQL end-to-end handling. No loan row is promoted; Issue 008 remains owner of the five missing loan inventory rows.
