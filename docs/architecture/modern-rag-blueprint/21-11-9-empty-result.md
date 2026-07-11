# Modern RAG Architecture Blueprint: 11.9 Empty result

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.9 Empty result

When `rows.len() == 0` after policy filtering:

``` json
{
  "answer_plan": { ..., "coverage": { "returned_rows": 0, ... } },
  "structured": { "by_currency": {}, "rows": [], "weekly_aggregation": [], "period_aggregation": { "buckets": [] } },
  "message": "No savings activity from 2026-05-05 to 2026-07-05 in your authorised offices."
}
```

No fake sections. No "Deposits (0 rows)" cluttering the output.
