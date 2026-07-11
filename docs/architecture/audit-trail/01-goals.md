# Audit Trail Design: Goals

Source: `docs-old/audit-trail-design.md`

## Goals

1. Track every important chat job stage without making the main pipeline wait on audit writes.
2. Persist enough structured data for management, debugging, and blueprint-compliance analysis.
3. Show which layer handled a request: auth, conversation context, classification, retrieval, policy, SQL execution, formatting, and answer generation.
4. Record non-standard paths such as lexical fallback, skipped strict semantic parsing, policy blocks, unsupported requests, and known hardcode risks.
5. Avoid storing secrets, raw API keys, raw embeddings, full SQL result rows, or hidden prompts.
