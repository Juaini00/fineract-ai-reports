# Modern RAG Architecture Blueprint: 11.1 Envelope

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.1 Envelope

Every reporting response returns:

``` json
{
  "answer_plan": {
    "capability": "savings_activity_list",
    "sections": ["overview", "deposits", "withdrawals", "charges_paid", "interest_and_dividends", "holds", "other", "weekly_aggregation", "period_aggregation"],
    "coverage": {
      "requested_range": { "from": "2026-05-05", "to": "2026-07-05" },
      "returned_rows": 10,
      "limit_applied": 10,
      "truncated": true,
      "known_total_rows": null,
      "currencies_returned": ["USD", "AED"],
      "offices_returned": [1]
    }
  },
  "structured": {
    "by_currency": {
      "USD": {
        "deposits":               { "count": 0, "total": "0.00" },
        "withdrawals":            { "count": 0, "total": "0.00" },
        "charges_paid":           { "count": 2, "total": "7.12" },
        "interest_and_dividends": { "count": 0, "total": "0.00" },
        "holds":                  { "count": 0, "total": "0.00" },
        "other":                  { "count": 6, "total": "0.85" }
      },
      "AED": {
        "charges_paid": { "count": 1, "total": "0.09" },
        "other":        { "count": 1, "total": "0.12" }
      }
    },
    "rows": [
      { "transaction_id": 123, "transaction_date": "2026-07-05", "transaction_type_raw": "withdrawal_fee", "semantic_bucket": "charges_paid", "amount": "7.03", "currency_code": "USD", "product_id": 4, "product_name": "Saving Product - USD", "office_id": 1, "office_name": "Head Office" }
    ],
    "weekly_aggregation": [
      { "week_start": "2026-06-29", "week_end": "2026-07-05",
        "by_currency": { "USD": { "charges_paid": "7.12", "other": "0.85" },
                         "AED": { "charges_paid": "0.09", "other": "0.12" } } }
    ],
    "period_aggregation": {
      "bucket_size_days": 2,
      "reason": "range 61 days → 2-day buckets",
      "buckets": [
        { "start": "2026-05-05", "end": "2026-05-06",
          "by_currency": { "USD": { "no_activity": true } } }
      ]
    }
  },
  "message": "…rendered markdown…"
}
```

Consumers pick what they need. Frontend can render `message` directly.
Integration tests assert against `structured.by_currency.USD.deposits.count`,
never grep the markdown.
