# Capability Coverage Matrix: Column variants

Source: `docs-old/capability-coverage-matrix.md`

## Column variants

Every row is expressed against the following query shapes. A cell describes the state of *this row × this shape*.

| Column | Meaning |
| --- | --- |
| Snapshot | "Right now" answer with no date bounds. |
| Aggregate total | Single number over a bounded date range. |
| Aggregate by day | One row per day in the range. |
| Aggregate by week | One row per ISO week. |
| Aggregate by month | One row per calendar month. |
| Aggregate by N-day bucket | Custom fixed-width bucket, parameter `bucket_days`. |
| Aggregate by quarter | One row per calendar quarter (roadmap; alias of `bucket=quarter`). |
| Top-N transactions | Ranked list across the range. |
| Top-N per month / week / day | Ranked list per bucket. |
| Individual row list | Paginated detail rows. |
| Composite | Multiple metrics in one response (planner returns `Vec<ExecutionPlan>`). |
| Comparative | Period-over-period delta. |
| Ranking (top-N entities) | Top-N offices / products / staff over a metric. |

Cell values use the status legend above. When a cell is `implemented`, the parenthesised id maps to a capability YAML in `knowledge/capabilities/`. When `planned`, the parenthesised id is the working name and may not yet exist — reference it via `<planned: id>` until the YAML lands.

---
