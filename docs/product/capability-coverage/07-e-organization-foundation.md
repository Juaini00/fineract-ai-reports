# Capability Coverage Matrix: E. Organization foundation

Source: `docs-old/capability-coverage-matrix.md`

## E. Organization foundation

Admin decisions supported here: branch inventory, hierarchy for consolidation reads, staff attribution, cross-branch performance, and productivity by office / staff. Foundation lookups are prerequisites for meaningful office-level filtering, and organization-level rankings are the top-of-funnel questions in management review meetings.

| Row | Snapshot | Aggregate by bucket | Top-N | Individual list | Ranking (offices/staff) | Comparative |
| --- | --- | --- | --- | --- | --- | --- |
| Office directory | `implemented` (`organization_office_summary`) | — | — | `planned` (v0.2 — `<planned: office_list>`; flat list scoped to caller) | — | — |
| Office hierarchy tree | `planned` (v0.2 — `<planned: office_hierarchy>`) | — | — | `planned` (v0.2) | — | — |
| Staff directory (aggregate; PII-gated for row-level) | `planned` (v0.3 — `<planned: staff_directory>`) | — | — | `planned` (v0.4 — row-level PII gate) | — | — |
| Staff assignment history summary | `planned` (v0.3) | `planned` (v0.4) | — | `planned` (v0.4) | — | — |
| Staff count per office | `planned` (v0.3) | — | — | — | `planned` (v0.4) | — |
| Office activity summary (portfolio + transactions rolled up) | `planned` (v0.3 — `<planned: office_performance_summary>`) | `planned` (v0.3) | — | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by portfolio balance | — | — | `planned` (v0.3) | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by deposit volume | — | — | `planned` (v0.3) | — | `planned` (v0.3) | `planned` (v0.4) |
| Office ranking by client onboarding | — | — | `planned` (v0.4) | — | `planned` (v0.4) | `planned` (v0.4) |
| Staff ranking by loan officer portfolio (loan-domain gated) | — | — | `deferred` | — | `deferred` | `deferred` |
