---
type: Response
title: Unsupported / Error Templates
description: Sanitized templates for out-of-scope or unsafe requests.
resource: ../../knowledge/responses/unsupported.yaml
tags: [responses, errors]
---

# Templates

| Case | Message |
|---|---|
| `unsupported_domain` | "This request is not supported by the current reporting scope." |
| `unsupported_capability` | "This report type is not available yet." |
| `forbidden_scope` | "This API key is not allowed to run the requested report." |
| `unsafe_request` | "This request cannot be processed because it violates reporting safety rules." |
| `deferred_domain` | "This data area is documented but not enabled for MVP reporting yet." |
| `pii_not_allowed` | "This report cannot return the requested identity or reference fields." |

# Rules

Sanitized only — never leak SQL, prompts, stack traces, or secrets. Prefer suggesting a supported alternative when one exists.
