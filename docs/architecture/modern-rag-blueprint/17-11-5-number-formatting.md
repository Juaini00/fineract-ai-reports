# Modern RAG Architecture Blueprint: 11.5 Number formatting

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.5 Number formatting

- Decimal amounts render with the fraction digits Fineract associates
  with the currency (via `m_organisation_currency.decimal_places` —
  fetched, not hardcoded). If unavailable, default to 2.
- Trim trailing zeros only after the currency's minimum fraction digits.
  So USD `7.030000` → `7.03`; JPY `100.00` → `100` if JPY.decimal_places
  is 0.
- Thousands separator: comma. `1,234.56`.
- Never render `-0.00`; collapse to `0`.
- Percentages: 1 decimal + `%`.
