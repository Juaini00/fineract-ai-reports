# Knowledge Catalog: 8. Runtime Decision Rules

Source: `docs-old/knowledge-catalog.md`

## 8. Runtime Decision Rules

The catalog supports three runtime outcomes.

### 8.1 Execute

Execute only when:

- Domain confidence is high enough.
- Capability confidence is high enough.
- Required parameters are complete.
- API key allows the capability.
- Capability data areas are approved.
- Office scope is valid.
- PII policy allows the output contract.
- Query safety validation has passed.

### 8.2 Clarify

Ask clarification when:

- Domain is likely but capability is ambiguous.
- Required date range is missing.
- User asks for `top` or `largest` without a limit and no default applies.
- Office/product/currency filter is ambiguous.
- The request combines multiple capabilities and MVP only supports atomic execution.

### 8.3 Unsupported

Reject safely when:

- The request maps to no approved capability.
- The request asks to modify data.
- The request asks for arbitrary SQL.
- The request asks for excluded PII or secrets.
- The request asks for a deferred data domain.
- The request requires tables outside approved data scope.
