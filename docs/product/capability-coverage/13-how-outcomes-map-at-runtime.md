# Capability Coverage Matrix: How outcomes map at runtime

Source: `docs-old/capability-coverage-matrix.md`

## How outcomes map at runtime

The classifier and planner emit one of four terminal outcomes. The coverage-matrix status drives the mapping.

| Matrix status | Classifier outcome | Job terminal status | User-facing template |
| --- | --- | --- | --- |
| `implemented` | `Matched` (single) or `CompositeMatched` (batch) | `completed` | Report renders normally per output_mode. |
| `planned` | `PlannedUnimplemented` | `planned_unimplemented` | Sanitised: "This report is planned but not yet available in this release. Expected in {target_milestone}." No SQL runs. |
| `deferred` | `Unsupported` with reason `deferred_domain` | `unsupported` | Sanitised: "That data area is not yet enabled." |
| `out_of_scope` | `Unsupported` with reason `hard_reject` | `unsupported` | Sanitised: "That request is not supported." |
| — (nonsense combination) | `ClarificationRequired` | `awaiting_clarification` | Structured clarification prompt. |

`PlannedUnimplemented` is the fourth outcome; the runtime today has `Matched | ClarificationRequired | Unsupported`. See `docs/ai-reporting-design.md` §18.3 for the design.
