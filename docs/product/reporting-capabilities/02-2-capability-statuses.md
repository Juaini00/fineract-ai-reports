# Reporting Capabilities: 2. Capability Statuses

Source: `docs-old/reporting-capabilities.md`

## 2. Capability Statuses

The runtime YAML enum uses the string values below (see `knowledge/capabilities/**/*.yaml`). The doc-facing term for `approved_mvp` is **enabled capability** — going forward, docs describe this as "currently implemented" or "enabled", not "MVP". The YAML enum value stays `approved_mvp` for backward compatibility with the runtime loader; renaming it is a Rust follow-up, not a doc task.

| Runtime YAML value | Doc-facing term | Meaning |
| --- | --- | --- |
| `approved_mvp` | **enabled / currently implemented** | Capability is loadable at runtime and matchable by the classifier. Legacy YAML enum name; do not rename. |
| `candidate` | **planned** (documented) | Documented as a next capability but not executable yet. Maps to `planned` in the coverage matrix. |
| `deferred` | **deferred** | Not executable until its data scope and business semantics are activated. Domain-level approval required. |
| `rejected` | **out-of-scope** | Explicitly unsupported; will never build. |

Every capability referenced in this document must either (a) exist in `knowledge/capabilities/**/*.yaml` with `status: approved_mvp` and correspond to an `implemented` cell in the coverage matrix, or (b) be listed under §11 Planned Capabilities with a target milestone and matching `planned` matrix cell.
