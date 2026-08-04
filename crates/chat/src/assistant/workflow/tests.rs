use std::cell::Cell;

use super::contract::*;
use super::graph::WorkflowGraph;
use super::verify::{VerifyError, verify_before_execute};
use super::{
    AcquisitionFacts, AmbiguityOutcome, CompileError, compile_with_facts, resolve_ambiguity,
};
use crate::knowledge::{
    catalog::{
        loader::KnowledgeLoader,
        parameter_policy::{DefaultExpr, ParameterPolicy, ProbeRef, ResolutionStrategy},
    },
    model::{
        CapabilityDefaults, CapabilityGuards, CapabilityKind, CapabilityKnowledge,
        KnowledgeCatalog, QueryKnowledge, QueryParameter, Sensitivity,
    },
};
use app_core::auth::model::PrincipalContext;
use uuid::Uuid;

fn id(value: &str) -> NodeId {
    NodeId::new(value).unwrap()
}
fn budget() -> NodeBudget {
    NodeBudget {
        timeout_ms: 1,
        row_cap: 1,
        query_cost: 0,
    }
}
fn policy() -> NodePolicy {
    NodePolicy {
        required_capability: None,
        office_scope: OfficeScope::AuthorizedIntersection,
        max_sensitivity: Sensitivity::Pii,
        pii_required: false,
    }
}
fn complete(value: &str) -> WorkflowNode {
    WorkflowNode {
        id: id(value),
        kind: NodeKind::Complete(CompleteNode {
            terminal: TerminalState::Success,
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(),
        idempotency: Idempotency::Pure,
        retry: RetryPolicy { max_attempts: 0 },
    }
}
fn principal(capability_ids: Vec<String>) -> PrincipalContext {
    PrincipalContext {
        user_id: Uuid::nil(),
        role: "admin".into(),
        capability_ids,
        office_ids: vec![1],
        can_view_pii: true,
        legacy_api_key_id: None,
    }
}
fn catalog() -> KnowledgeCatalog {
    KnowledgeLoader::new("../../knowledge", "../../queries")
        .load()
        .unwrap()
}
fn assert_rejected(
    workflow: ExecutionWorkflow,
    principal: &PrincipalContext,
    catalog: &KnowledgeCatalog,
    expected: VerifyError,
) {
    let calls = Cell::new(0);
    let error = verify_before_execute(workflow, principal, catalog, |_| calls.set(calls.get() + 1))
        .unwrap_err();
    assert_eq!(error, expected);
    assert_eq!(
        calls.get(),
        0,
        "verification rejection must execute zero queries"
    );
}
fn workflow_budgets() -> WorkflowBudgets {
    WorkflowBudgets {
        shared_timeout_ms: 30_000,
        shared_row_cap: 1_000,
        max_query_count: 10,
        max_parallel_queries: 2,
        max_model_turns: 2,
        max_node_retries: 0,
    }
}

fn workflow(nodes: Vec<WorkflowNode>, edges: Vec<WorkflowEdge>) -> ExecutionWorkflow {
    ExecutionWorkflow {
        id: Uuid::nil(),
        contract_version: WORKFLOW_CONTRACT_VERSION,
        catalog_version: Uuid::nil(),
        nodes,
        edges,
        budgets: workflow_budgets(),
        fail_policy: FailPolicy::FailFast,
        output_contract: OutputContract {
            mode: OutputMode::Table,
            allows_partial: false,
            max_sensitivity: Sensitivity::Pii,
        },
    }
}

#[test]
fn node_id_is_validated_on_construction_and_deserialization() {
    assert!(NodeId::new("valid_node_1").is_ok());
    assert!(NodeId::new("Invalid").is_err());
    assert!(serde_json::from_str::<NodeId>("\"bad-id\"").is_err());
}
#[test]
fn workflow_round_trip_rejects_stale_fields() {
    let value = serde_json::to_value(workflow(vec![complete("complete")], vec![])).unwrap();
    assert_eq!(
        serde_json::from_value::<ExecutionWorkflow>(value.clone()).unwrap(),
        serde_json::from_value(value.clone()).unwrap()
    );
    let mut stale = value.as_object().unwrap().clone();
    stale.insert("stale".into(), serde_json::Value::Null);
    assert!(serde_json::from_value::<ExecutionWorkflow>(serde_json::Value::Object(stale)).is_err());
}
#[test]
fn workflow_proposal_rejects_sql_during_deserialization() {
    let proposal = serde_json::json!({
        "capability_ids": ["SELECT * FROM m_client"],
        "nodes": [],
        "edges": [],
    });
    assert!(serde_json::from_value::<WorkflowProposal>(proposal).is_err());
}

#[test]
fn graph_uses_petgraph_for_cycles_topology_and_fanout() {
    let start = complete("start");
    let left = complete("left");
    let right = complete("right");
    let diamond = workflow(
        vec![start, left, right],
        vec![
            WorkflowEdge {
                from: id("start"),
                to: id("left"),
                condition: EdgeCondition::Always,
            },
            WorkflowEdge {
                from: id("start"),
                to: id("right"),
                condition: EdgeCondition::Always,
            },
        ],
    );
    let graph = WorkflowGraph::new(&diamond);
    assert!(!graph.is_cyclic());
    assert_eq!(graph.topological_order().unwrap()[0], id("start"));
    assert_eq!(
        graph.runnable(&std::collections::HashSet::from([id("start")])),
        vec![id("left"), id("right")]
    );
    let cyclic = workflow(
        vec![complete("one"), complete("two")],
        vec![
            WorkflowEdge {
                from: id("one"),
                to: id("two"),
                condition: EdgeCondition::Always,
            },
            WorkflowEdge {
                from: id("two"),
                to: id("one"),
                condition: EdgeCondition::Always,
            },
        ],
    );
    assert!(WorkflowGraph::new(&cyclic).is_cyclic());
}

#[test]
fn verifier_v1_to_v10_reject_before_execution() {
    let catalog = catalog();
    let allowed = principal(
        catalog
            .capabilities
            .iter()
            .map(|cap| cap.id.clone())
            .collect(),
    );

    let cyclic = workflow(
        vec![complete("one"), complete("two")],
        vec![
            WorkflowEdge {
                from: id("one"),
                to: id("two"),
                condition: EdgeCondition::Always,
            },
            WorkflowEdge {
                from: id("two"),
                to: id("one"),
                condition: EdgeCondition::Always,
            },
        ],
    );
    assert_rejected(cyclic.clone(), &allowed, &catalog, VerifyError::Cycle);

    let mut unknown = complete("start");
    unknown.kind = NodeKind::ExecuteQuery(ExecuteQueryNode {
        capability_id: Some("missing".into()),
        dataset_id: None,
        shape_id: None,
        query_id: None,
        iterate_over: None,
    });
    unknown.budget.query_cost = 1;
    unknown.inputs.push(NodeInput {
        parameter: "office_ids".into(),
        kind: crate::knowledge::catalog::parameter_policy::ParameterType::IntegerArray,
        source: BindingSource::AuthorizedScope,
    });
    assert_rejected(
        workflow(
            vec![unknown, complete("complete")],
            vec![WorkflowEdge {
                from: id("start"),
                to: id("complete"),
                condition: EdgeCondition::Always,
            }],
        ),
        &allowed,
        &catalog,
        VerifyError::UnknownResource,
    );

    let mut incompatible = complete("start");
    incompatible.inputs.push(NodeInput {
        parameter: "not_office_scope".into(),
        kind: crate::knowledge::catalog::parameter_policy::ParameterType::String,
        source: BindingSource::AuthorizedScope,
    });
    assert_rejected(
        workflow(vec![incompatible], vec![]),
        &allowed,
        &catalog,
        VerifyError::TypeIncompatibleBinding,
    );

    let mut identifier = complete("start");
    identifier.inputs.push(NodeInput {
        parameter: "m_client.id".into(),
        kind: crate::knowledge::catalog::parameter_policy::ParameterType::String,
        source: BindingSource::ExactSensitiveInput,
    });
    assert_rejected(
        workflow(vec![identifier], vec![]),
        &allowed,
        &catalog,
        VerifyError::DataDependentSqlIdentifier,
    );

    let mut no_scope = complete("start");
    no_scope.budget.query_cost = 1;
    assert_rejected(
        workflow(vec![no_scope], vec![]),
        &allowed,
        &catalog,
        VerifyError::MissingOfficeScope,
    );

    let mut over_budget = complete("start");
    over_budget.budget.query_cost = 2;
    let mut budget_workflow = workflow(vec![over_budget], vec![]);
    budget_workflow.budgets.max_query_count = 1;
    assert_rejected(
        budget_workflow,
        &allowed,
        &catalog,
        VerifyError::BudgetExceeded,
    );

    let mut partial = workflow(vec![complete("start")], vec![]);
    partial.fail_policy = FailPolicy::ContinueLabelled;
    assert_rejected(
        partial,
        &allowed,
        &catalog,
        VerifyError::PartialResultsNotPermitted,
    );

    let mut sensitive = complete("start");
    sensitive.policy.max_sensitivity = Sensitivity::Pii;
    let mut public_output = workflow(vec![sensitive], vec![]);
    public_output.output_contract.max_sensitivity = Sensitivity::PublicBusiness;
    assert_rejected(
        public_output,
        &allowed,
        &catalog,
        VerifyError::SensitivityWidening,
    );

    assert_rejected(
        workflow(vec![complete("one"), complete("two")], vec![]),
        &allowed,
        &catalog,
        VerifyError::UnreachableOrOrphanNode,
    );

    let clarification = WorkflowNode {
        id: id("clarify"),
        kind: NodeKind::ClarificationInterrupt(ClarificationInterruptNode {
            clarification_kind: "collect_fields".into(),
            option_source: id("complete"),
            resume: id("missing"),
        }),
        inputs: vec![],
        outputs: vec![],
        policy: policy(),
        budget: budget(),
        idempotency: Idempotency::Replayable,
        retry: RetryPolicy { max_attempts: 0 },
    };
    assert_rejected(
        workflow(
            vec![clarification, complete("complete")],
            vec![WorkflowEdge {
                from: id("clarify"),
                to: id("complete"),
                condition: EdgeCondition::Always,
            }],
        ),
        &allowed,
        &catalog,
        VerifyError::DanglingResume,
    );
}

#[test]
fn verifier_v11_and_v12_reject_before_execution() {
    let mut catalog = catalog();
    catalog.queries.push(QueryKnowledge {
        id: "required_input_test".into(),
        database: "fineract".into(),
        sql_file: "queries/test.sql".into(),
        data_areas: vec![],
        tables: vec![],
        metrics: vec![],
        parameters: vec![
            QueryParameter {
                name: "office_ids".into(),
                kind: "integer_array".into(),
                required: true,
                source: None,
            },
            QueryParameter {
                name: "required_text".into(),
                kind: "string".into(),
                required: true,
                source: None,
            },
        ],
        output_fields: vec![],
        timeout_ms: None,
    });
    let query = catalog
        .queries
        .iter()
        .find(|query| query.id == "required_input_test")
        .unwrap();
    let execute = WorkflowNode {
        id: id("execute"),
        kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
            capability_id: None,
            dataset_id: None,
            shape_id: None,
            query_id: Some(query.id.clone()),
            iterate_over: None,
        }),
        inputs: vec![NodeInput {
            parameter: "office_ids".into(),
            kind: crate::knowledge::catalog::parameter_policy::ParameterType::IntegerArray,
            source: BindingSource::AuthorizedScope,
        }],
        outputs: vec![],
        policy: policy(),
        budget: NodeBudget {
            query_cost: 1,
            ..budget()
        },
        idempotency: Idempotency::ExecuteOnce,
        retry: RetryPolicy { max_attempts: 0 },
    };
    assert_rejected(
        workflow(
            vec![execute.clone(), complete("complete")],
            vec![WorkflowEdge {
                from: id("execute"),
                to: id("complete"),
                condition: EdgeCondition::Always,
            }],
        ),
        &principal(vec![]),
        &catalog,
        VerifyError::UnboundRequiredInput,
    );

    catalog.capabilities.push(CapabilityKnowledge {
        id: "forbidden_capability".into(),
        status: "approved_mvp".into(),
        domain: "test".into(),
        query_id: query.id.clone(),
        output_mode: "table".into(),
        request_shape: Default::default(),
        kind: CapabilityKind::Terminal,
        member_capability_ids: vec![],
        display_name: Some("Forbidden test".into()),
        description: None,
        data_areas: vec![],
        metrics: vec![],
        examples: vec![],
        supported_intents: vec![],
        unsupported_intents: vec![],
        continuation: false,
        required_parameters: vec![],
        optional_parameters: vec![],
        defaults: CapabilityDefaults::default(),
        guards: CapabilityGuards::default(),
        dataset_recipe: None,
        parameter_policies: vec![],
    });
    let capability = catalog.capabilities.last().unwrap();
    let mut forbidden = execute;
    forbidden.kind = NodeKind::ExecuteQuery(ExecuteQueryNode {
        capability_id: Some(capability.id.clone()),
        dataset_id: None,
        shape_id: None,
        query_id: Some(capability.query_id.clone()),
        iterate_over: None,
    });
    forbidden.policy.required_capability = Some(capability.id.clone());
    assert_rejected(
        workflow(
            vec![forbidden, complete("complete")],
            vec![WorkflowEdge {
                from: id("execute"),
                to: id("complete"),
                condition: EdgeCondition::Always,
            }],
        ),
        &principal(vec![]),
        &catalog,
        VerifyError::CapabilityNotPermitted,
    );
}

fn policy_item(name: &str, resolution: Vec<ResolutionStrategy>) -> ParameterPolicy {
    ParameterPolicy {
        name: name.into(),
        kind: crate::knowledge::catalog::parameter_policy::ParameterType::Date,
        required: true,
        default: None,
        fill_when_missing: false,
        user_may_override: true,
        hard_cap: None,
        user_required: false,
        resolution,
        probe: None,
    }
}

fn source_from_compilation(
    parameter: ParameterPolicy,
    facts: AcquisitionFacts,
) -> Result<BindingSource, CompileError> {
    let mut catalog = catalog();
    let capability = catalog
        .capabilities
        .iter_mut()
        .find(|capability| capability.status == "approved_mvp")
        .unwrap();
    let capability_id = capability.id.clone();
    let parameter_name = parameter.name.clone();
    capability.parameter_policies = vec![parameter.clone()];
    let workflow = compile_with_facts(
        WorkflowProposal {
            capability_ids: vec![capability_id],
            nodes: vec![],
            edges: vec![],
        },
        &catalog,
        Uuid::nil(),
        workflow_budgets(),
        &facts,
    )?;
    let execute = workflow
        .nodes
        .iter()
        .find(|node| matches!(node.kind, NodeKind::ExecuteQuery(_)))
        .unwrap();
    Ok(execute
        .inputs
        .iter()
        .find(|input| input.parameter == parameter_name)
        .unwrap()
        .source
        .clone())
}

#[test]
fn acquisition_order_uses_each_source_before_unsupported() {
    use crate::knowledge::catalog::parameter_policy::ParameterType;

    let office = source_from_compilation(
        ParameterPolicy {
            name: "office_ids".into(),
            kind: ParameterType::IntegerArray,
            required: true,
            default: None,
            fill_when_missing: false,
            user_may_override: false,
            hard_cap: None,
            user_required: false,
            resolution: vec![ResolutionStrategy::AuthorizedScope],
            probe: None,
        },
        AcquisitionFacts::default(),
    )
    .unwrap();
    assert_eq!(office, BindingSource::AuthorizedScope);

    let mut defaulted = policy_item(
        "from_date",
        vec![
            ResolutionStrategy::CatalogDefault,
            ResolutionStrategy::DeterministicExtraction,
        ],
    );
    defaulted.default = Some(DefaultExpr::BusinessToday);
    assert_eq!(
        source_from_compilation(
            defaulted,
            AcquisitionFacts {
                deterministic: vec![crate::assistant::ConstraintField::FromDate],
                ..Default::default()
            },
        )
        .unwrap(),
        BindingSource::CatalogDefault
    );
    assert_eq!(
        source_from_compilation(
            policy_item(
                "from_date",
                vec![ResolutionStrategy::DeterministicExtraction]
            ),
            AcquisitionFacts {
                deterministic: vec![crate::assistant::ConstraintField::FromDate],
                ..Default::default()
            },
        )
        .unwrap(),
        BindingSource::DeterministicExtraction {
            field: crate::assistant::ConstraintField::FromDate
        }
    );
    assert_eq!(
        source_from_compilation(
            policy_item("from_date", vec![ResolutionStrategy::VerifiedUserText]),
            AcquisitionFacts {
                verified_user: vec![crate::assistant::ConstraintField::FromDate],
                ..Default::default()
            },
        )
        .unwrap(),
        BindingSource::VerifiedUserText {
            field: crate::assistant::ConstraintField::FromDate
        }
    );
    let prior = id("prior_choice");
    assert_eq!(
        source_from_compilation(
            policy_item("from_date", vec![ResolutionStrategy::SafePriorSelection]),
            AcquisitionFacts {
                safe_prior: std::collections::BTreeMap::from([("from_date".into(), prior.clone())]),
                ..Default::default()
            },
        )
        .unwrap(),
        BindingSource::SafePriorSelection {
            clarification: prior
        }
    );
    let prior_step = id("earlier_step");
    assert_eq!(
        source_from_compilation(
            policy_item("from_date", vec![ResolutionStrategy::PriorStep]),
            AcquisitionFacts {
                prior_steps: std::collections::BTreeMap::from([(
                    "from_date".into(),
                    (prior_step.clone(), "date".into()),
                )]),
                ..Default::default()
            },
        )
        .unwrap(),
        BindingSource::PriorStep {
            node: prior_step,
            slot: "date".into()
        }
    );
    let mut probe = policy_item("client_id", vec![ResolutionStrategy::AuthorizedDataProbe]);
    probe.kind = ParameterType::Integer;
    probe.probe = Some(ProbeRef {
        dataset_id: "client.identity".into(),
        shape_id: "identity_candidates".into(),
        output_slot: "client_id".into(),
    });
    assert!(matches!(
        source_from_compilation(probe, AcquisitionFacts::default()).unwrap(),
        BindingSource::AuthorizedDataProbe { .. }
    ));
    let mut clarify = policy_item("from_date", vec![ResolutionStrategy::Clarify]);
    clarify.user_required = true;
    assert!(matches!(
        source_from_compilation(clarify, AcquisitionFacts::default()).unwrap(),
        BindingSource::SafePriorSelection { .. }
    ));
    assert_eq!(
        source_from_compilation(
            policy_item(
                "from_date",
                vec![ResolutionStrategy::DeterministicExtraction]
            ),
            AcquisitionFacts::default(),
        ),
        Err(CompileError::Unsupported)
    );
}

#[test]
fn iteration_expands_bounded_siblings_and_rejects_grouped_or_non_array_paths() {
    let mut catalog = catalog();
    let query_id = catalog.queries.first().unwrap().id.clone();
    let proposal = || WorkflowProposal {
        capability_ids: vec![],
        nodes: vec![
            WorkflowNode {
                id: id("per_client"),
                kind: NodeKind::ExecuteQuery(ExecuteQueryNode {
                    capability_id: None,
                    dataset_id: Some("client.identity".into()),
                    shape_id: Some("identity_candidates".into()),
                    query_id: Some(query_id.clone()),
                    iterate_over: Some(IterateOver {
                        source: id("resolve_clients"),
                        slot: "client_id".into(),
                        max: 2,
                    }),
                }),
                inputs: vec![],
                outputs: vec![],
                policy: policy(),
                budget: NodeBudget {
                    query_cost: 1,
                    ..budget()
                },
                idempotency: Idempotency::ExecuteOnce,
                retry: RetryPolicy { max_attempts: 0 },
            },
            complete("complete"),
        ],
        edges: vec![WorkflowEdge {
            from: id("per_client"),
            to: id("complete"),
            condition: EdgeCondition::Always,
        }],
    };
    {
        let dataset = catalog
            .datasets
            .iter_mut()
            .find(|dataset| dataset.id == "client.identity")
            .unwrap();
        dataset.shapes[0].grouped_by = None;
    }
    let workflow = compile_with_facts(
        proposal(),
        &catalog,
        Uuid::nil(),
        workflow_budgets(),
        &AcquisitionFacts::default(),
    )
    .unwrap();
    assert!(
        workflow
            .nodes
            .iter()
            .any(|node| node.id == id("per_client_iter_0"))
    );
    assert!(
        workflow
            .nodes
            .iter()
            .any(|node| node.id == id("per_client_iter_1"))
    );

    {
        let dataset = catalog
            .datasets
            .iter_mut()
            .find(|dataset| dataset.id == "client.identity")
            .unwrap();
        dataset.shapes[0].grouped_by = Some("client_id".into());
    }
    assert!(matches!(
        compile_with_facts(
            proposal(),
            &catalog,
            Uuid::nil(),
            workflow_budgets(),
            &AcquisitionFacts::default()
        ),
        Err(CompileError::GroupedQueryPreferred(_))
    ));
    {
        let dataset = catalog
            .datasets
            .iter_mut()
            .find(|dataset| dataset.id == "client.identity")
            .unwrap();
        dataset.shapes[0].grouped_by = None;
        dataset.filters.retain(|filter| filter.id != "client_id");
    }
    assert_eq!(
        compile_with_facts(
            proposal(),
            &catalog,
            Uuid::nil(),
            workflow_budgets(),
            &AcquisitionFacts::default()
        ),
        Err(CompileError::InvalidProposal)
    );
}

#[test]
fn ambiguity_gate_selects_then_probes_then_clarifies_only_when_exhausted() {
    let mut catalog = catalog();
    let template = catalog
        .capabilities
        .iter()
        .find(|capability| capability.status == "approved_mvp")
        .unwrap()
        .clone();
    let mut date = template.clone();
    date.id = "date_option".into();
    date.display_name = Some("Date option".into());
    date.parameter_policies = vec![policy_item("from_date", vec![])];
    let mut client = template;
    client.id = "client_option".into();
    client.display_name = Some("Client option".into());
    client.parameter_policies = vec![policy_item("client_id", vec![])];
    catalog.capabilities.extend([date, client]);

    assert!(matches!(
        resolve_ambiguity(&["date_option".into()], &AcquisitionFacts::default(), &catalog),
        AmbiguityOutcome::Select { capability_id, confidence_overridden: true } if capability_id == "date_option"
    ));
    assert!(matches!(
        resolve_ambiguity(
            &["date_option".into(), "client_option".into()],
            &AcquisitionFacts { deterministic: vec![crate::assistant::ConstraintField::FromDate], ..Default::default() },
            &catalog,
        ),
        AmbiguityOutcome::Select { capability_id, .. } if capability_id == "date_option"
    ));
    assert!(matches!(
        resolve_ambiguity(
            &["date_option".into(), "client_option".into()],
            &AcquisitionFacts::default(),
            &catalog,
        ),
        AmbiguityOutcome::Probe { dataset_id, shape_id } if dataset_id == "client.identity" && shape_id == "identity_candidates"
    ));
    catalog.datasets.clear();
    assert!(matches!(
        resolve_ambiguity(
            &["date_option".into(), "client_option".into()],
            &AcquisitionFacts::default(),
            &catalog,
        ),
        AmbiguityOutcome::Clarify { options } if options == vec![
            ("client_option".into(), "Client option".into()),
            ("date_option".into(), "Date option".into()),
        ]
    ));
}
