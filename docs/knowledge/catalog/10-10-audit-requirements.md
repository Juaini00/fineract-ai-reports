# Knowledge Catalog: 10. Audit Requirements

Source: `docs-old/knowledge-catalog.md`

## 10. Audit Requirements

Every report job should record enough knowledge metadata to explain the decision.

Audit fields:

- Request id or job id.
- API key id.
- Catalog version or checksum.
- Selected data area ids.
- Selected domain id.
- Selected capability id.
- Selected query id.
- Selected metric ids.
- Confidence score.
- Required parameters and sanitized values.
- Office scope applied.
- PII mode applied.
- Decision outcome: execute, clarify, unsupported, forbidden, or failed.
- Query latency and row count.

Do not audit raw API keys, raw secrets, unsafe prompt details, or large raw result payloads.
