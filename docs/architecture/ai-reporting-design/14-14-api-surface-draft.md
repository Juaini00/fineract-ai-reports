# AI Reporting Service Design: 14. API Surface Draft

Source: `docs-old/ai-reporting-design.md`

## 14. API Surface Draft

Initial backend endpoints:

```text
GET  /health
GET  /ready

POST   /auth/api-keys
GET    /auth/api-keys
GET    /auth/api-keys/{id}
DELETE /auth/api-keys/{id}
POST   /auth/api-keys/{id}/revoke

GET  /auth/me

POST /chat/sessions
GET  /chat/sessions/{session_id}
GET  /chat/sessions/{session_id}/messages

POST /chat/jobs
GET  /chat/jobs/{job_id}
GET  /chat/jobs/{job_id}/stream
POST /chat/jobs/{job_id}/responses

GET  /catalog/domains
GET  /catalog/capabilities
GET  /catalog/queries
POST /catalog/validate

GET  /jobs
GET  /jobs/{job_id}

GET  /logs/executions
GET  /usage/tokens
GET  /vector-index/status
POST /vector-index/rebuild
```
