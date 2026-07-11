# Capability Coverage Matrix: H. Cross-cutting composite

Source: `docs-old/capability-coverage-matrix.md`

## H. Cross-cutting composite

Admin decisions supported here: real user turns bundle metrics. "Show me the biggest deposit, biggest withdrawal, biggest charge, and biggest hold this month" is one question, not four turns.

| Row | Aggregate | By bucket | Top-N | Ranking | Comparative |
| --- | --- | --- | --- | --- | --- |
| Multi-metric one request (deposit + withdrawal + charge + hold) | `planned` (v0.3 — planner returns `Vec<ExecutionPlan>`; see `docs/ai-reporting-design.md` §18.2) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) |
| Multi-domain one request (savings + group + office) | `planned` (v0.4) | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Comparative period-over-period (this period vs previous) | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) | `planned` (v0.4) | — |
| Ranking top-N offices / staff / products across a metric | — | — | `planned` (v0.3) | `planned` (v0.3) | `planned` (v0.4) |
