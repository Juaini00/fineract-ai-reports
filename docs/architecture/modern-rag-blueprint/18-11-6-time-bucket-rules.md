# Modern RAG Architecture Blueprint: 11.6 Time bucket rules

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.6 Time bucket rules

- Aggregation ranges follow the **requested** date range, not the
  returned rows. Empty buckets render as "no activity". Users need to
  see the shape of quiet periods, not just the loud ones.
- Adaptive bucket size for period aggregation:

  Range span            Bucket size
  --------------------- -----------
  ≤ 14 days             daily
  15–60 days            2-day
  61–180 days           weekly
  181–730 days          monthly
  > 730 days            quarterly

  The chosen size + the reason ("range 61 days → 2-day buckets") goes
  into `structured.period_aggregation.reason`.
- Weekly aggregation is *in addition to* period aggregation when the
  range spans multiple ISO weeks. Weekly always starts Monday.
