# Modern RAG Architecture Blueprint: 11.3 Coverage transparency

Source: `docs-old/Modern_RAG_Architecture_Blueprint.md`

## 11.3 Coverage transparency

Silent truncation is a bug. If any of the following are true, the
`coverage` block AND the message header MUST say so explicitly:

- `LIMIT` was applied (`returned_rows == limit_applied`).
- Requested office scope was narrowed by policy (`policy_decision`
  changed `office_ids`).
- A currency filter was applied.
- A product filter was applied.

Required header sentence when truncated:

> "Showing 10 most recent transactions. Result limited by `limit=10`;
> narrow the date range or raise the limit to see more."

`known_total_rows` is `null` unless the capability defines a cheap
COUNT query. When present, it becomes:

> "Showing 10 of 1,601 transactions in this range."
