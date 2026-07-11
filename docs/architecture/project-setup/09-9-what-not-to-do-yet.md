# Project Setup: 9. What Not To Do Yet

Source: `docs-old/project-setup.md`

## 9. What Not To Do Yet

Do not create these crates yet:

```text
api
infra
runtime
knowledge
reporting
ai_report_core
ai_report_api
ai_report_chat
ai_report_runtime
```

Do not create all modules upfront.

Do not split `knowledge` or `reporting` into crates before there is a concrete need.

Do not add reporting execution before health/readiness/auth and chat job foundations are working.

Do not add dynamic SQL generation.
