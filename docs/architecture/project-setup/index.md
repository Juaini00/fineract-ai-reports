# Project Setup

Source: `docs-old/project-setup.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines the exact project setup we will use. It keeps crate boundaries small and intentional.

The current implementation should use exactly three crates:

```text
app
core
chat
```

## Sections

- [1. Final Rule For The Initial Setup](./01-1-final-rule-for-the-initial-setup.md)
- [2. Root Cargo.toml](./02-2-root-cargo-toml.md)
- [3. app Crate](./03-3-app-crate.md)
- [4. core Crate](./04-4-core-crate.md)
- [5. chat Crate](./05-5-chat-crate.md)
- [6. Module Setup Order Inside core](./06-6-module-setup-order-inside-core.md)
- [7. Initial run() Target](./07-7-initial-run-target.md)
- [8. Validation Commands](./08-8-validation-commands.md)
- [9. What Not To Do Yet](./09-9-what-not-to-do-yet.md)
- [10. Current Implementation Position](./10-10-current-implementation-position.md)
