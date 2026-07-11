# Reporting PII Policy

Source: `docs-old/reporting-pii-policy.md`

This is the split, readable version of the original document. Content was migrated section-by-section so no old context is dropped.

## Original introduction


This document defines how the AI Reporting Service handles PII, sensitive business identifiers, and secrets in Fineract reporting responses.

The policy applies to every protected reporting endpoint, every chat/report response, and every reporting capability — currently implemented or planned. PII gating is orthogonal to capability status: a `planned` capability that will return PII must declare its PII contract before implementation begins.

## Sections

- [1. Core Rules](./01-1-core-rules.md)
- [2. Sensitivity Classes](./02-2-sensitivity-classes.md)
- [3. Always Excluded Fields](./03-3-always-excluded-fields.md)
- [4. PII Fields](./04-4-pii-fields.md)
- [5. Sensitive Business Identifiers](./05-5-sensitive-business-identifiers.md)
- [6. Security Sensitive Fields](./06-6-security-sensitive-fields.md)
- [7. Masking Rules](./07-7-masking-rules.md)
- [8. Behavior By API Key](./08-8-behavior-by-api-key.md)
- [9. Per-Capability Application (currently implemented)](./09-9-per-capability-application-currently-implemented.md)
- [10. Enforcement Points](./10-10-enforcement-points.md)
- [11. AI Prompt Safety](./11-11-ai-prompt-safety.md)
- [12. Review Checklist For New Capabilities](./12-12-review-checklist-for-new-capabilities.md)
- [13. Reserved Sensitivity Classes](./13-13-reserved-sensitivity-classes.md)
