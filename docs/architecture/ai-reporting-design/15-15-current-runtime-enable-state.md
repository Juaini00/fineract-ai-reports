# AI Reporting Service Design: 15. Current Runtime Enable-State

Source: `docs-old/ai-reporting-design.md`

## 15. Current Runtime Enable-State

The scope commitment is the full coverage matrix. The runtime today enables a subset — see `docs/capability-coverage-matrix.md` for the implemented-vs-planned-vs-deferred grid.

Currently enabled domain:

```text
savings (client and organization foundation tables are joined for scope, not driven directly yet)
```

Currently enabled capabilities (nine, all with `status: approved_mvp` in `knowledge/capabilities/**/*.yaml`):

```text
savings_balance_summary
savings_deposit_total
savings_deposit_top_n
savings_deposit_monthly_breakdown
savings_deposit_monthly_top_n
savings_withdrawal_total
savings_withdrawal_top_n
savings_withdrawal_monthly_breakdown
savings_withdrawal_monthly_top_n
```

Current runtime flow:

```text
Client sends request with API key
  -> API key middleware creates ClientContext
  -> user asks a deposit question
  -> local classifier matches savings/deposit/top_n or total
  -> guard validates params
  -> guard validates API key scopes
  -> Rust executes approved query
  -> template formats response
  -> audit log is stored
```

LLM planner fallback is wired via DeepSeek OpenAI-compatible endpoint (see `crates/chat/src/chat/planner`). Adding a new capability follows the phased guide in `docs/knowledge-catalog.md` §14.
