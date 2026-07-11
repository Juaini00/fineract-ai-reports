# Implementation Steps: Phase 20: LQR Retrieval Overlay

Source: `docs-old/implementation-steps.md`

## Phase 20: LQR Retrieval Overlay

Goal: reduce off-domain false positives and add per-layer retrieval trace while keeping flat retrieval as fallback.

Current status:

```text
PARTIALLY DONE

Slice LQR-1:
Layered Query Retrieval is available behind LQR_ENABLED=false.
Layer 1 retrieves domain rows and short-circuits deferred/rejected domains before capability search.
Layer 2 retrieves capabilities scoped to the winning domain and API key allowed_capabilities.
Classification state records state_json.classification.layers for domain/capability audit.
Flat vector retrieval remains the default and fallback path until scenario 16 plus scenarios 05/06/07 pass with LQR_ENABLED=true.
```

Each new capability requires:

1. Capability YAML.
2. Query YAML.
3. Approved SQL file.
4. Query validation.
5. Test cases.
6. Permission scope definition.
