# Capability Coverage Matrix

Source: `docs-old/capability-coverage-matrix.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


Single source of truth for **what user questions this service commits to answering, what runs today, what is on the roadmap, and what will never be built**. All other docs link here rather than restate scope.

Scope is not "what's implemented today". Scope is **the full agreed reporting-data surface** — every reasonable admin decision-support question over the in-scope Fineract data areas (savings, client, organization, and group/center foundations). Some capabilities are enabled at runtime today; the rest are `planned` and land on a milestone; deferred domains (loan, accounting, tax, audit, custom-datatables) are in-scope for the product vision but not yet activated. If a real admin question doesn't fit anywhere in this matrix, the matrix is wrong — file a row.

Update this file first when adding, deferring, or removing a capability. The runtime YAML in `knowledge/capabilities/**/*.yaml` is authoritative for what actually executes; this matrix is authoritative for what we *say* about scope.

## Sections

- [Status legend](./01-status-legend.md)
- [Column variants](./02-column-variants.md)
- [A. Savings — balance & lifecycle](./03-a-savings-balance-lifecycle.md)
- [B. Savings — transactions](./04-b-savings-transactions.md)
- [C. Savings — interest & fees](./05-c-savings-interest-fees.md)
- [D. Client foundation](./06-d-client-foundation.md)
- [E. Organization foundation](./07-e-organization-foundation.md)
- [F. Group / center foundation (conditional — enabled per deployment)](./08-f-group-center-foundation-conditional-enabled-per-deployment.md)
- [G. Deferred domains](./09-g-deferred-domains.md)
- [H. Cross-cutting composite](./10-h-cross-cutting-composite.md)
- [Rows always out of scope regardless of column](./11-rows-always-out-of-scope-regardless-of-column.md)
- [Milestone map](./12-milestone-map.md)
- [How outcomes map at runtime](./13-how-outcomes-map-at-runtime.md)
- [Adding a row or column](./14-adding-a-row-or-column.md)
- [Contract test policy](./15-contract-test-policy.md)
- [Cross-references](./16-cross-references.md)
