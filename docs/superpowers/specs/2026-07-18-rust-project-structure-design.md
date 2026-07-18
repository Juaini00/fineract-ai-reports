# Rust Project Module Structure Design

**Status:** Approved high-level design  
**Date:** 2026-07-18

## Purpose

Restructure the Rust source into a hybrid feature-first, layer-second layout. The
workspace remains exactly three crates: `app`, `core`, and `chat`. This design
changes module ownership and file boundaries, not product behavior.

The cited sources support the principles used here—explicit module boundaries,
limited visibility, navigable workspaces, and documented architecture. They do
not prescribe one canonical directory tree for this project.

## Current Problems

- Several files combine orchestration, domain decisions, state transitions,
  integration details, and presentation.
- Large files make ownership unclear and increase the blast radius of changes.
- Related assistant behavior is spread across technical layers rather than
  grouped around the assistant workflow.
- Public module surfaces can grow accidentally when extraction relies on broad
  `pub` visibility.
- Tests embedded beside already-large production code obscure the runtime flow.

Priority decomposition examples are the current approximately 2,679-line
runtime, 1,118-line job service, 876-line extraction, 770-line tool, and
729-line canonical-state files. These are priority examples, not acceptance
thresholds or permission for other files to grow to those sizes.

## Design Principles

1. Keep the existing crate boundaries and dependency direction.
2. Organize `chat` by cohesive feature, then use layers inside each feature.
3. Give each module one clear reason to change and one named owner concern.
4. Keep parent modules as thin façades: declarations, narrow re-exports, and
   small coordination only.
5. Default to private; widen visibility only for a demonstrated caller.
6. Preserve `route → service → repository → database`; only repositories use
   `sqlx` or execute SQL.
7. Prefer mechanical moves and compatibility re-exports during migration over
   simultaneous behavioral rewrites.

## Target Tree

The tree is directional. A leaf should exist only when it owns real behavior;
empty scaffolding is not required.

```text
crates/
├── app/src/
│   ├── main.rs
│   └── bootstrap.rs              # optional; composition only
├── core/src/
│   ├── lib.rs
│   ├── api/                      # envelope, errors, validation, extractors
│   ├── auth/                     # login/session/API-key primitives
│   ├── config/                   # typed application configuration
│   ├── database/                 # pools and shared DB infrastructure
│   └── telemetry/                # tracing and observability setup
└── chat/src/
    ├── lib.rs
    ├── api/                      # HTTP DTOs, handlers, routes, SSE boundary
    ├── conversation/             # sessions, messages, clarification lifecycle
    ├── job/                      # durable jobs, checkpoints, events, worker flow
    ├── assistant/
    │   ├── understanding/        # intent and deterministic extraction
    │   ├── context/              # canonical request and conversation context
    │   ├── retrieval/            # catalog/evidence retrieval and selection
    │   ├── state/                # assistant state and transitions
    │   ├── execution/            # approved-plan and tool orchestration
    │   ├── presentation/         # structured response and rendering
    │   └── llm/                  # model/router integration boundaries
    ├── knowledge/                # catalog loading, validation, and indexing
    ├── policy/                   # authorization and policy evaluation
    └── audit/                    # assistant decisions and trace records
```

`knowledge/` YAML and `queries/` SQL remain at repository root. Schema changes
remain exclusively in `migrations/`.

## Module Ownership

### `app`

Owns process startup and dependency composition. `main.rs` should remain a thin
entrypoint. If startup wiring becomes hard to follow, `bootstrap.rs` may build
state, routes, and workers; it must not acquire domain policy or persistence.

### `core`

Owns reusable platform foundations shared across crates. Its groups are
technical because their cohesion is cross-feature: API primitives, auth,
configuration, database infrastructure, and telemetry. It must not absorb chat
domain behavior merely to make code reusable.

### `chat`

Owns chat-driven reporting. `api`, `conversation`, and `job` provide external
and durable lifecycle boundaries. `assistant` owns the end-to-end reasoning
pipeline. `knowledge`, `policy`, and `audit` remain explicit supporting
capabilities with narrow interfaces.

Within a feature, use layer names such as `service` and `repository` only when
both responsibilities exist. Repositories own database access; services own
domain orchestration. Handlers translate HTTP requests and responses only.

## Visibility Rules

- Begin with private items and private child modules.
- Use `pub(super)` for a parent façade and `pub(crate)` for genuine intra-crate
  collaboration. Use unrestricted `pub` only for an intentional crate API.
- Re-export the smallest stable surface from a parent `mod.rs` or named module;
  do not mirror every child item publicly.
- Do not use visibility to bypass ownership. Move behavior or add a narrow
  operation instead of exposing internal state.
- Keep DTOs at the API boundary and persistence records at the repository
  boundary; translate explicitly at ownership edges.

These rules follow Rust's visibility model while reducing accidental coupling.

## Enforceable File-Decomposition Guidance

There is no hard lines-of-code purity rule. A decomposition review is mandatory
when either condition is true:

1. a production file approaches roughly 400 lines; or
2. a file contains multiple independently changing responsibilities, at any
   size.

The review must identify the file's responsibilities and either split them or
record why one cohesive unit is clearer. Parent modules and façades must stay
thin; moving a god file behind a small `mod.rs` does not satisfy the review.
Tests may move into a sibling `tests.rs` or focused test modules when inline
tests obscure production logic. Generated code and data tables may justify a
larger file but do not justify mixed responsibilities.

## Trace-Flow Example

For a bearer-authenticated request asking for a report:

1. `chat::api` validates the request and calls the conversation/job service.
2. `chat::conversation` records the user message through its repository.
3. `chat::job` creates or resumes the durable job through its repository.
4. `chat::assistant::understanding` derives structured intent and parameters.
5. `assistant::context` builds canonical context; `assistant::retrieval` selects
   approved catalog evidence from `chat::knowledge`.
6. `chat::policy` evaluates authorization before execution.
7. `assistant::execution` runs only an approved plan; repository code executes
   approved SQL with office scope bound inside SQL.
8. `assistant::state` records meaningful transitions and `chat::job` persists
   checkpoints/events.
9. `assistant::presentation` produces the structured English response.
10. `chat::api` returns the standard `{ success, data, error }` envelope or SSE
    event without exposing internal prompts, SQL errors, or stack details.

Clarification remains on the same job through
`POST /chat/jobs/{job_id}/responses`. PostgreSQL remains durable truth; Redis
remains live SSE coordination only.

## Migration Principles and Phases

Migration is incremental and behavior-preserving:

1. **Map ownership:** inventory responsibilities and callers of each priority
   file; agree on destination modules and narrow interfaces.
2. **Create boundaries:** add only destination modules needed by the next move;
   use temporary compatibility re-exports where they keep each step buildable.
3. **Move cohesive units:** extract tests first when useful, then move leaf
   types/functions before orchestration. Do not mix moves with logic changes.
4. **Thin orchestration:** split runtime and job coordination along the target
   assistant/conversation/job boundaries while preserving checkpoints and flow.
5. **Tighten visibility:** remove compatibility exports and reduce `pub` after
   callers have moved.
6. **Verify and document:** run focused tests per move, workspace checks at phase
   boundaries, and update architecture/current-state docs when reality changes.

Each phase should be independently reviewable and revertible. File moves should
not alter HTTP contracts, SQL, authorization, state semantics, or user-visible
responses.

## Acceptance Criteria

- The workspace still contains exactly `app`, `core`, and `chat` crates.
- `app` remains the composition root; no domain logic moves into it.
- `core` and `chat` ownership matches this design without speculative modules.
- Every database call remains in a repository; handlers and services contain no
  `sqlx` calls.
- Existing authentication, authorization, office-scope, PII, durability,
  clarification, response-envelope, and English-only invariants remain intact.
- Priority god files have documented responsibility maps and are decomposed into
  cohesive modules, or a review records a concrete cohesion justification.
- Parent façades are thin and public exports are intentional and minimal.
- Focused tests and workspace checks pass after each migration phase.
- No source layout change requires a schema, knowledge YAML, or approved SQL
  behavior change.

## Non-Goals

- Adding, renaming, or merging crates.
- Rewriting assistant behavior, prompts, SQL, policies, or API contracts.
- Introducing a framework, dependency-injection abstraction, or generic plugin
  system.
- Enforcing a universal file-length limit or splitting cohesive code mechanically.
- Moving root `knowledge/`, `queries/`, or `migrations/` into Rust crates.
- Combining the structural migration with performance or product work.

## References

- [The Rust Programming Language: Packages and Crates](https://doc.rust-lang.org/book/ch07-01-packages-and-crates.html)
- [The Rust Programming Language: Defining Modules to Control Scope and Privacy](https://doc.rust-lang.org/book/ch07-02-defining-modules-to-control-scope-and-privacy.html)
- [The Rust Reference: Visibility and Privacy](https://doc.rust-lang.org/reference/visibility-and-privacy.html)
- [The Cargo Book: Workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [matklad: Large Rust Workspaces](https://matklad.github.io/2021/08/22/large-rust-workspaces.html)
- [matklad: ARCHITECTURE.md](https://matklad.github.io/2021/02/06/ARCHITECTURE.md.html)
- [rust-analyzer repository](https://github.com/rust-lang/rust-analyzer), an
  example of a large Rust workspace with explicit crate and module boundaries
