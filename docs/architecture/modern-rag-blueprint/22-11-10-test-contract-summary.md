# Modern RAG Architecture Blueprint: 11.10 Test contract summary

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.10 Test contract summary

- Unit tests own: bucket mapping YAML round-trip, decimal rendering,
  header sentence generation.
- Integration tests own: end-to-end payload shape, coverage
  truncation flag, per-currency grouping, empty result behaviour.
- Neither layer greps the message string for domain vocabulary owned
  by Fineract.
