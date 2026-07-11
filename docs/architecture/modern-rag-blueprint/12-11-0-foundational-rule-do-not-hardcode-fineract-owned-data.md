# Modern RAG Architecture Blueprint: 11.0 Foundational rule — do not hardcode Fineract-owned data

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.0 Foundational rule — do not hardcode Fineract-owned data

Anything whose canonical list lives inside Apache Fineract flows through
query results at runtime. This includes at minimum:

  Category                            Fineract source
  ----------------------------------- --------------------------------------
  Currency code + fraction digits     `m_organisation_currency`, per-txn `currency_code`
  Transaction type semantics          `transaction_type_enum` on each txn table
  Savings/loan product identifiers    `m_savings_product`, `m_loan_product`
  Office identifiers and names        `m_office`
  Client / group / staff identifiers  `m_client`, `m_group`, `m_staff`
  Charge codes                        `m_charge`
  Payment types                       `m_payment_type`
  Client / loan / savings statuses    respective `status_enum` columns

**Rules that follow from this:**

- Formatter code must not contain a Rust `match` on currency codes,
  product ids, or office names. If a value is missing, render as-is
  (with a `null`-safe fallback) — never guess.
- Any mapping table we *do* need (e.g. `transaction_type_enum → semantic
  bucket`) lives under `knowledge/domain/` YAML and is loaded at
  startup. Adding a new enum value = YAML change, not code change +
  redeploy.
- Currency arithmetic uses the currency-code string that comes back on
  each row. Rows in different currencies are grouped separately. **Never
  sum across currencies.**
