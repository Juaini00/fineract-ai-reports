# 04 — Vector Index Admin

**Phase covered:** Phase 18 admin surface.
**Precondition:** `API_KEY` from `02`. `VOYAGEAI_API_KEY` in `.env` if you want embeddings populated; without it, `status` returns `indexed` (no embedding).

## Rebuild

```bash
curl -X POST {{BASE_URL}}/vector-index/rebuild \
  -H "Authorization: Bearer {{API_KEY}}"
```

### Expected (HTTP 200)
```json
{
  "success": true,
  "data": {
    "catalog_version_id": "<uuid>",
    "content_hash": "<sha256>",
    "document_count": <n>,
    "embedding_model": "voyage-3-large"   // or null when Voyage is not configured
  },
  "error": null
}
```

## Status

```bash
curl {{BASE_URL}}/vector-index/status \
  -H "Authorization: Bearer {{API_KEY}}"
```

### Expected (HTTP 200) — populated
```json
{
  "success": true,
  "data": {
    "catalog_version_id": "<uuid>",
    "version": "local",
    "content_hash": "<sha256>",
    "status": "embedded",        // "indexed" when no embeddings
    "document_count": <n>,
    "embedding_model": "voyage-3-large",
    "embedding_dimensions": 1024,
    "synced_at": "<rfc3339>",
    "created_at": "<rfc3339>"
  },
  "error": null
}
```

### Expected — empty (no rebuild yet, no startup sync)
```json
{ "success": true, "data": { "status": "empty" }, "error": null }
```

## Side effects
- DB `knowledge_catalog_versions`: upserts row keyed by `content_hash` (UNIQUE). Status progresses `loaded → validated → indexed → embedded`.
- DB `knowledge_index`: rows replaced for that `catalog_version_id` (deterministic SHA-256 dedup per document).
- Embeddings call: Voyage API hit only when `VOYAGEAI_API_KEY` is set.

## Failure modes

| Trigger | Expected |
| --- | --- |
| Voyage API down | HTTP 500 `error.message` includes Voyage error; no catalog version row written |
| YAML referential error | HTTP 500 from validator (same messages as `03-catalog-validate.md`) |
| Wrong / missing API key | HTTP 401 |
