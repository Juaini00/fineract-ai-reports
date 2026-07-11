# Audit Trail Design: Blueprint Step Mapping

Source: `docs-old/audit-trail-design.md`

## Blueprint Step Mapping

Each audit event should map to the closest blueprint step when possible:

```text
conversation_context
semantic_parser
intent_router
entity_constraint_resolver
ambiguity_detector
retrieval_planner
hybrid_retrieval
reranker
evidence_evaluator
answer_planner
answer_generator
grounded_response
```

When a blueprint step is intentionally skipped in the current implementation, emit an audit event with `status = 'skipped'` and a structured reason:

```json
{
  "reason": "strict_pipeline_not_used_in_production"
}
```

This makes blueprint gaps observable instead of implicit.
