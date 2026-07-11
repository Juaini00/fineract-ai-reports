# Implementation Steps: Phase 12: Local Classifier MVP

Source: `docs-old/implementation-steps.md`

## Phase 12: Local Classifier MVP

Goal: classify simple savings deposit questions without AI first.

Supported examples:

```text
Who made the largest deposit today?
Show the largest deposits today.
What is the total deposit this month?
Total deposits from January to September 2026.
```

Classifier output:

```json
{
  "domain": "savings",
  "capability": "savings_deposit_total",
  "output_mode": "total",
  "params": {
    "from_date": "2026-01-01",
    "to_date": "2026-09-30"
  },
  "confidence": 0.86
}
```

If confidence is low:

```text
return unsupported or clarification
```

Current status:

```text
PARTIALLY DONE

Implemented:
crates/chat/src/chat/classifier.rs
Savings-specific local capability rules were removed; runtime capability selection now comes from vector/catalog retrieval plus approved clarification options.
Classifier still owns generic parameter extraction for date ranges and top-N limits after a catalog capability is selected.
Stores the classification result in chat_jobs.state_json.classification when a job is created.

Still pending:
typed parameter extraction from query metadata beyond date range and top-N limit
confidence calibration for broader domains as more approved capabilities are added
```
