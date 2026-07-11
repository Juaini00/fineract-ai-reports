# Project Setup: 1. Final Rule For The Initial Setup

Source: `docs-old/project-setup.md`

## 1. Final Rule For The Initial Setup

Use this structure:

```text
ai_report/
  Cargo.toml
  .env
  docs/

  crates/
    app/
      Cargo.toml
      src/
        main.rs

    core/
      Cargo.toml
      src/
        lib.rs

    chat/
      Cargo.toml
      src/
        lib.rs
```

Meaning:

```text
app  = binary entrypoint and composition root
core = shared application foundation
chat = main chat-driven reporting feature
```

Do not add more crates yet.

Do not use crate names like `ai_report_core`, `ai_report_app`, `ai_report_chat`, `chat_service`, `knowledge`, or `reporting`.

The crate names must be:

```text
app
core
chat
```

Knowledge remains a folder-based catalog under `knowledge/`.

SQL remains under `queries/`.

Reporting remains part of the chat-driven feature until there is a concrete non-chat report API or scheduling surface.
