# Modern RAG Architecture Blueprint: 11.8 Machine-readability contract

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.8 Machine-readability contract

Consequence of Design Principle 5.

- Integration tests MUST assert against `structured.by_currency.<CODE>.<bucket>.count`
  and `structured.coverage.truncated`, never grep the markdown message.
- Renaming a section header ("Deposits" → "Setoran") must not break
  any integration test.
- Message markdown IS a human artefact and may reword freely between
  versions; the JSON structured payload is a versioned contract.
