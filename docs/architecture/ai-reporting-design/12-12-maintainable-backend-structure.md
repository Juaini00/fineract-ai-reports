# AI Reporting Service Design: 12. Maintainable Backend Structure

Source: `docs-old/ai-reporting-design.md`

## 12. Maintainable Backend Structure

The current implementation intentionally uses a small two-crate workspace. Do not add more crates yet.

Current structure:

```text
ai_report/
  Cargo.toml
  .env
  docs/
    ai-reporting-design.md

  knowledge/
    domains/
      savings.yaml
      loan.yaml
    capabilities/
      savings/
        deposit_total.yaml
        deposit_top_n.yaml
        deposit_monthly_breakdown.yaml
        deposit_monthly_top_n.yaml
    queries/
      savings/
        deposit_total.yaml
        deposit_top_n.yaml
        deposit_monthly_breakdown.yaml
        deposit_monthly_top_n.yaml

  queries/
    savings/
      deposit_total.sql
      deposit_top_n.sql
      deposit_monthly_breakdown.sql
      deposit_monthly_top_n.sql

  migrations/
    *.sql

  crates/
    app/
      Cargo.toml
      src/
        main.rs

    core/
      Cargo.toml
      src/
        lib.rs
        config.rs
        db.rs
        telemetry.rs
        api/
          mod.rs
          error.rs
          response.rs
          extractors/
            mod.rs
            validated_json.rs
          routes/
            mod.rs
            auth.rs
            health.rs
        auth/
          mod.rs
          api_key.rs
          model.rs
          repository.rs
          service.rs
```

### 12.1 Why This Structure

The separation is based on responsibility boundaries:

```text
app
  -> binary entrypoint and composition root

core
  -> shared foundation: config, DB pools, API primitives, auth, validation, response envelope

chat
  -> chat-driven reporting feature: sessions, messages, jobs, checkpoints, future pipeline
```

Benefits:

1. The initial project remains easy to understand.
2. `app` stays thin but owns composition wiring.
3. `core` stays focused on shared foundation.
4. `chat` owns the main product flow without forcing separate `knowledge` or `reporting` crates.
5. Additional crates are added only when there is a concrete non-chat surface or stable boundary.

### 12.2 Crates NOT to add

Do not create these crates yet:

```text
api
infra
knowledge
reporting
runtime
ai_report_*
```

Use short crate names only. The allowed workspace crates for now are `app`, `core`, and `chat`.

For now, add modules inside `crates/core/src/`.
