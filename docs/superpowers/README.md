# Specs And Plans

Significant implementation work should follow this path:

```text
idea/request
  -> docs/superpowers/specs/YYYY-MM-DD-topic-design.md
  -> docs/superpowers/plans/YYYY-MM-DD-topic.md
  -> implementation
  -> update docs/current/status.md
  -> update or close docs/issues/*
```

## Specs

Specs describe what should be built and why. They should include:

- problem
- goal
- non-goals
- design
- affected docs/code areas
- risks
- success criteria

## Plans

Plans describe how to implement the approved spec. They should include:

- files to create/modify
- ordered tasks
- validation commands
- review checkpoints

Existing migrated specs and plans remain in [`specs/`](./specs/) and [`plans/`](./plans/).
