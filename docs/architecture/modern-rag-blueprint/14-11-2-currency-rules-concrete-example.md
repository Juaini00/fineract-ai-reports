# Modern RAG Architecture Blueprint: 11.2 Currency rules (concrete example)

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.2 Currency rules (concrete example)

Given the response mixes `USD` and `AED`:

**WRONG — current output.** Header total sums USD and AED as if they
were the same unit:

    ### Charges paid (3 row(s), total: USD 7.210000)
    1. 2026-07-05 — USD 7.030000 (Saving Product - USD, office: Head Office)
    2. 2026-07-05 — AED 0.090000 (Current Account With OD - AED, office: Head Office)
    3. 2026-07-05 — USD 0.090000 (Current Account USD, office: Head Office)

**RIGHT — required output.** Sub-sections per currency, no cross-currency
math anywhere:

    ### Charges paid

    #### USD (2 transactions, total 7.12)
    1. 2026-07-05 — 7.03 (Saving Product - USD, office: Head Office)
    2. 2026-07-05 — 0.09 (Current Account USD, office: Head Office)

    #### AED (1 transaction, total 0.09)
    1. 2026-07-05 — 0.09 (Current Account With OD - AED, office: Head Office)

Formatter never invents a currency, never normalises to a "primary"
currency, never picks one for the header. If a row has no
`currency_code` (Fineract shouldn't allow this but we assume nothing),
render as `unknown` — do not drop.

The same rule applies to product name, office name, and every other
Fineract-owned label: pass through as-is, no substitution, no
inference.
