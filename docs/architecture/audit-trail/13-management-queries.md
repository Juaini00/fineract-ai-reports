# Audit Trail Design: Management Queries

Source: `docs-old/audit-trail-design.md`

## Management Queries

The audit table should support questions like:

```text
Which jobs used lexical fallback today?
Which jobs skipped semantic parsing?
Which jobs were blocked by policy?
Which capabilities fail most often?
Which stages are slowest?
Which requests took a non-standard path compared with the blueprint?
Which API keys trigger the most unsupported requests?
```
