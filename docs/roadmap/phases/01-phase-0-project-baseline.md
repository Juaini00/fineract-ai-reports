# Implementation Steps: Phase 0: Project Baseline

Source: `docs-old/implementation-steps.md`

## Phase 0: Project Baseline

Goal: ensure the project compiles and the local environment is ready.

Tasks:

1. Confirm Rust project builds with installed dependencies.
2. Confirm `.env` contains required application, database, LLM provider, auth, and guard settings.
3. Confirm PostgreSQL database `ai_reports` exists.
4. Confirm `pgvector` extension is enabled in `ai_reports`.
5. Confirm Fineract database connection values are present in `.env`.
6. Confirm Redis runs through Docker Compose, not Homebrew/local service.

Validation:

```bash
cargo check
cargo test
```

Database validation:

```bash
PGPASSWORD=password psql -h 127.0.0.1 -p 5432 -U root -d ai_reports -c "SELECT extname, extversion FROM pg_extension WHERE extname = 'vector';"
```

Expected result:

```text
vector extension is installed and active
```

Current status:

```text
DONE
```
