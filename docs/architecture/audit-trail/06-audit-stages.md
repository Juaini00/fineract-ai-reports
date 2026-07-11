# Audit Trail Design: Audit Stages

Source: `docs-old/audit-trail-design.md`

## Audit Stages

Recommended initial stages:

```text
request_received
auth_context_loaded
conversation_context_built
semantic_parser
classification_started
classification_completed
lqr_planner_started
lqr_planner_completed
flat_retrieval_fallback
lexical_retrieval_fallback
clarification_required
execution_plan_built
policy_evaluated
sql_selected
sql_executed
response_formatted
llm_answer_generation_started
llm_answer_generation_completed
job_completed
job_failed
```

Not every job emits every stage. Missing stages are useful: they reveal which path the request actually took.
