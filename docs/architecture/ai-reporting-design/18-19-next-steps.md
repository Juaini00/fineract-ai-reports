# AI Reporting Service Design: 19. Next Steps

Source: `docs-old/ai-reporting-design.md`

## 19. Next Steps

1. Extend the local classifier only where needed for currently enabled phrases and date ranges.
2. Add response formatting and assistant chat message output for executed reports.
3. Add remaining SQL validation if execution needs it: EXPLAIN/output/schema checks.
4. Move synchronous create-job execution into a background worker.
5. Add Redis-backed SSE only after background execution exists.
