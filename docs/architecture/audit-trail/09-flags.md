# Audit Trail Design: Flags

Source: `docs-old/audit-trail-design.md`

## Flags

`flags_json` records important analysis hints:

```json
{
  "used_lqr": true,
  "used_flat_retrieval": false,
  "used_lexical_fallback": false,
  "used_llm": true,
  "policy_blocked": false,
  "blueprint_deviation": false,
  "hardcode_risk": false,
  "pii_output_allowed": false,
  "authorized_scope_only": true
}
```

Use `hardcode_risk` for known places where current behavior depends on Fineract-owned constants or deterministic shortcuts. The audit system should not try to statically scan all SQL/code in the first version.
