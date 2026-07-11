# Project Setup: 10. Current Implementation Position

Source: `docs-old/project-setup.md`

## 10. Current Implementation Position

The initial setup described in this document is complete:

```text
1. Root Cargo.toml is workspace-only.
2. crates/app is the binary entrypoint.
3. crates/core owns shared foundation.
4. crates/chat exists and owns chat-driven reporting feature code.
5. health/readiness/auth foundations are implemented.
6. chat now has separate api, chat, knowledge, and policy modules.
```

Continue with `docs/implementation-steps.md` for the active roadmap.
