# Reporting Capabilities

Source: `docs-old/reporting-capabilities.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the reporting capabilities that the AI Reporting Service is allowed to execute against the Fineract read-only database.

Capabilities are the runtime contract between user intent, authorization, approved SQL, and response formatting. The service must not execute arbitrary AI-generated SQL.

> **Scope commitment.** The service commits to supporting all reasonable admin reporting questions over the in-scope reporting-data areas (savings, client, organization, group/center foundations) — currently as a mix of `implemented` and `planned` capabilities. Deferred domains (loan, accounting, tax, audit, custom-datatables) are inside the product commitment but not yet activated. For the full implemented-vs-planned-vs-deferred-vs-out-of-scope picture, see [`docs/capability-coverage-matrix.md`](../capability-coverage/index.md). This document details the capability contract; the matrix is the scoreboard.
>
> Capability shortage today is a known-gap, not an intended design. Every reasonable admin decision-support question is expected to fit somewhere in the coverage matrix — as `implemented`, `planned`, `deferred`, or (with an explicit reason) `out_of_scope`.

## Sections

- [1. Capability Rules](./01-1-capability-rules.md)
- [2. Capability Statuses](./02-2-capability-statuses.md)
- [3. Common Parameters](./03-3-common-parameters.md)
- [4. Common Savings Joins](./04-4-common-savings-joins.md)
- [5. Currently Implemented Capabilities](./05-5-currently-implemented-capabilities.md)
- [6. Additional Currently Implemented Savings Capabilities](./06-6-additional-currently-implemented-savings-capabilities.md)
- [7. Candidate Savings Capabilities](./07-7-candidate-savings-capabilities.md)
- [8. Deferred Capabilities](./08-8-deferred-capabilities.md)
- [9. Unsupported Requests](./09-9-unsupported-requests.md)
- [10. Implementation Notes](./10-10-implementation-notes.md)
- [11. Planned Capabilities](./11-11-planned-capabilities.md)
- [12. Non-Goals](./12-12-non-goals.md)
