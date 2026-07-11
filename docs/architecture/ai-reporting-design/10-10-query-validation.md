# AI Reporting Service Design: 10. Query Validation

Source: `docs-old/ai-reporting-design.md`

## 10. Query Validation

Queries must be validated before they are available at runtime.

Startup/CI validation:

1. Load YAML catalog.
2. Load SQL file.
3. Ensure referenced SQL file exists.
4. Ensure the SQL is SELECT-only.
5. Ensure the SQL is not multi-statement.
6. Ensure placeholders match defined parameters.
7. Run `EXPLAIN` using sample parameters.
8. Verify output columns where possible.
9. Disable invalid queries.

The AI must not create new SQL automatically in production runtime.
