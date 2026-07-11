# Reporting Capabilities: 9. Unsupported Requests

Source: `docs-old/reporting-capabilities.md`

## 9. Unsupported Requests

The service must reject or clarify requests that ask for:

- Arbitrary SQL or database exploration.
- Full Fineract schema search.
- Report fields not declared by the selected capability.
- Raw account numbers, external ids, payment references, tokens, passwords, command JSON, or command results.
- Loan / accounting / tax / audit / custom-datatable results before those deferred domains are activated.
- Office scopes outside the API key's `allowed_office_ids`.
