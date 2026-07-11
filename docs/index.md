# AI Report Documentation

This is the primary documentation entrypoint for humans and agents.

## Start here

1. [Current status](./current/status.md) — what is implemented, partial, and pending now.
2. [Active context](./current/active-context.md) — rules and context to keep in mind before editing.
3. [Next work](./current/next-work.md) — recommended next development items.
4. [Architecture overview](./architecture/overview.md) — system boundaries and request flow.
5. [Roadmap](./roadmap/implementation-roadmap.md) — phase map and migrated phase details.
6. [Issues](./issues/README.md) — active/resolved documentation and product issues.
7. [Spec/plan workflow](./superpowers/README.md) — how new implementation work is designed and planned.

## Source-of-truth map

| Concern | Source of truth |
| --- | --- |
| Current development state | `docs/current/status.md` |
| Immediate agent context | `docs/current/active-context.md` |
| Next implementation work | `docs/current/next-work.md` |
| Stable architecture | `docs/architecture/` |
| Product/reporting rules | `docs/product/` |
| Runtime behavior | `docs/runtime/` |
| Knowledge-system explanation | `docs/knowledge/` |
| API reference | `docs/api/` |
| Manual verification flows | `docs/scenarios/` |
| Active issues | `docs/issues/active/` |
| Design specs and implementation plans | `docs/superpowers/` |
| Machine-readable runtime catalog | `knowledge/**/*.yaml` and `queries/**/*.sql` |
| Human-readable OKF concept graph | `doc-knowledge/` |

## Reading rule

Do not start by reading every file. Read `current/*` first, then follow the link for the area you are changing.
