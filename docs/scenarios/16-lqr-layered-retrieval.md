# 16 — LQR Layered Retrieval

**Phase covered:** Phase 20 LQR overlay.
**Precondition:** `LQR_ENABLED=true`; `POST /vector-index/rebuild` has produced an embedded catalog containing domain and capability rows.

## A. Off-domain short-circuit

Run `POST /chat/jobs` with prompts for deferred domains.

| Prompt | Expected classification |
| --- | --- |
| "Total loan disbursement this month" | `outcome=unsupported`, `source=lqr`, `layers[0].layer=domain`, `layers.len=1` |
| "Show journal entries today" | `outcome=unsupported`, `source=lqr`, `layers[0].layer=domain`, `layers.len=1` |
| "Tax collected this quarter" | `outcome=unsupported`, `source=lqr`, `layers[0].layer=domain`, `layers.len=1` |

The unsupported source should normalize off-domain rejects to `off_domain_match`.

## B. In-domain match

Run approved savings prompts with an API key whose `allowed_capabilities` includes the target capability.

| Prompt | Expected classification |
| --- | --- |
| "Total savings deposit this month" | `outcome=matched`, `source=lqr`, domain + capability + query layers present |
| "Largest deposits today" | `outcome=matched`, `source=lqr`, domain + capability + query layers present |

## C. Domain-scoped ambiguity

Prompt: "show me deposits".

Expected: `outcome=clarification_required`, `source=lqr`, options limited to the winning domain.

## D. Bahasa synonym probe

Prompt: "aktivasi klien bulan ini".

Expected: Layer 1 chooses `client`; Layer 2 chooses the approved client activation capability if the API key allows it, otherwise returns unsupported/clarification without crossing into savings options.

## Verification notes

- `state_json.classification.layers` is the audit surface.
- Layer 2 must be absent for deferred-domain prompts.
- Keep `LQR_ENABLED=false` as default until this scenario and scenarios `05`, `06`, and `07` pass with LQR enabled.
