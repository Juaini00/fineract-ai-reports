# Audit Trail Design: Example Timeline

Source: `docs-old/audit-trail-design.md`

## Example Timeline

```text
request_received                 completed  conversation_context
auth_context_loaded              completed  conversation_context
classification_started           started    intent_router
lqr_planner_started              started    retrieval_planner
lqr_planner_completed            completed  retrieval_planner
classification_completed         completed  intent_router
execution_plan_built             completed  answer_planner
policy_evaluated                 completed  evidence_evaluator
sql_selected                     completed  hybrid_retrieval
sql_executed                     completed  hybrid_retrieval
response_formatted               completed  grounded_response
llm_answer_generation_completed  completed  answer_generator
job_completed                    completed  grounded_response
```
