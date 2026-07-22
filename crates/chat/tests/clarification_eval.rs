//! Offline behavior corpus for the pure catalog-backed clarification planner.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use chat::assistant::{
    ClarificationFacts, ClarificationKind, ClarificationPlanResult, ClarificationPlanner,
    ConstraintField, TypedFactValue,
};
use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::model::KnowledgeCatalog;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

const REQUIRED_BUCKETS: &[&str] = &[
    "direct_execution",
    "collect_one",
    "collect_multiple",
    "options_no_fields",
    "options_shared_fields",
    "options_conditional_fields",
    "known_values",
    "partial_values",
    "approved_default",
    "others",
    "invalid_value",
    "stale_revision",
    "parallel_job_isolation",
];

const REQUIRED_INPUTS: &[&str] = &["date_range", "limit", "search"];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    id: String,
    bucket: String,
    #[serde(default)]
    scenario_kind: ScenarioKind,
    #[serde(default = "planner_assertion_default")]
    planner_assertion: bool,
    candidate_capabilities: Vec<String>,
    #[serde(default)]
    known_facts: BTreeMap<ConstraintField, TypedFactValue>,
    #[serde(default)]
    coverage_inputs: Vec<InputCoverage>,
    expected: Option<ExpectedPlan>,
}

fn planner_assertion_default() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ScenarioKind {
    #[default]
    Planner,
    RuntimeState,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct InputCoverage {
    key: String,
    state: InputState,
}

#[derive(Debug, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
enum InputState {
    Positive,
    Missing,
    Invalid,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectedPlan {
    direct_execution: bool,
    #[serde(default)]
    capability_id: Option<String>,
    #[serde(default)]
    kind: Option<ClarificationKind>,
    #[serde(default)]
    option_ids: Vec<String>,
    #[serde(default)]
    shared_field_keys: Vec<String>,
    #[serde(default)]
    shared_field_values: BTreeMap<String, Value>,
    #[serde(default)]
    conditional_field_keys: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    must_not_ask: Vec<String>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/clarification")
}

fn load_fixtures() -> Vec<Fixture> {
    let dir = fixtures_dir();
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("read fixture directory {dir:?}: {error}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("read fixture entry: {error}"))
                .path()
        })
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yaml")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read fixture {path:?}: {error}"));
            serde_yaml::from_str(&text)
                .unwrap_or_else(|error| panic!("parse fixture {path:?}: {error}"))
        })
        .collect()
}

fn load_catalog() -> KnowledgeCatalog {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root");
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("load production knowledge catalog")
}

fn requested_field_keys(result: &ClarificationPlanResult) -> BTreeSet<String> {
    match result {
        ClarificationPlanResult::Complete { .. } => BTreeSet::new(),
        ClarificationPlanResult::Clarify { payload, .. } => payload
            .fields
            .iter()
            .chain(
                payload
                    .options
                    .iter()
                    .flat_map(|option| option.fields.iter()),
            )
            .map(|field| field.key.clone())
            .collect(),
    }
}

fn assert_fixture(catalog: &KnowledgeCatalog, fixture: &Fixture) {
    if !fixture.planner_assertion {
        assert_eq!(
            fixture.scenario_kind,
            ScenarioKind::RuntimeState,
            "{}: non-planner fixture must declare scenario_kind: runtime_state",
            fixture.id
        );
        assert!(
            fixture.expected.is_none(),
            "{}: coverage-only fixtures cannot have expected plans",
            fixture.id
        );
        return;
    }
    assert_eq!(
        fixture.scenario_kind,
        ScenarioKind::Planner,
        "{}: planner fixture must use scenario_kind: planner",
        fixture.id
    );
    let expected = fixture
        .expected
        .as_ref()
        .unwrap_or_else(|| panic!("{}: planner fixture needs expected plan", fixture.id));
    let actual = ClarificationPlanner::new(catalog).plan(
        &fixture.candidate_capabilities,
        &ClarificationFacts {
            values: fixture.known_facts.clone(),
        },
        Uuid::nil(),
    );
    let actual_debug = format!("{actual:#?}");

    match (&actual, expected.direct_execution) {
        (ClarificationPlanResult::Complete { capability_id, .. }, true) => assert_eq!(
            Some(capability_id),
            expected.capability_id.as_ref(),
            "{}: actual plan:\n{actual_debug}",
            fixture.id
        ),
        (ClarificationPlanResult::Clarify { payload, .. }, false) => {
            assert_eq!(
                Some(&payload.kind),
                expected.kind.as_ref(),
                "{}: actual plan:\n{actual_debug}",
                fixture.id
            );
            let option_ids: Vec<_> = payload
                .options
                .iter()
                .map(|option| option.id.clone())
                .collect();
            assert_eq!(
                option_ids, expected.option_ids,
                "{}: actual plan:\n{actual_debug}",
                fixture.id
            );
            let shared_keys: Vec<_> = payload
                .fields
                .iter()
                .map(|field| field.key.clone())
                .collect();
            assert_eq!(
                shared_keys, expected.shared_field_keys,
                "{}: actual plan:\n{actual_debug}",
                fixture.id
            );
            let shared_values: BTreeMap<_, _> = payload
                .fields
                .iter()
                .filter_map(|field| field.value.clone().map(|value| (field.key.clone(), value)))
                .collect();
            if !expected.shared_field_values.is_empty() {
                assert_eq!(
                    shared_values, expected.shared_field_values,
                    "{}: actual plan:\n{actual_debug}",
                    fixture.id
                );
            }
            let conditional_keys: BTreeMap<_, _> = payload
                .options
                .iter()
                .map(|option| {
                    (
                        option.id.clone(),
                        option
                            .fields
                            .iter()
                            .map(|field| field.key.clone())
                            .collect(),
                    )
                })
                .collect();
            assert_eq!(
                conditional_keys, expected.conditional_field_keys,
                "{}: actual plan:\n{actual_debug}",
                fixture.id
            );
        }
        _ => panic!(
            "{}: direct_execution={} but actual plan was:\n{actual_debug}",
            fixture.id, expected.direct_execution
        ),
    }

    let requested = requested_field_keys(&actual);
    for key in &expected.must_not_ask {
        assert!(
            !requested.contains(key),
            "{}: asked for {key}; actual plan:\n{actual_debug}",
            fixture.id
        );
    }
}

#[test]
fn fixtures_cover_required_buckets_and_user_inputs() {
    let fixtures = load_fixtures();
    let buckets: BTreeSet<_> = fixtures
        .iter()
        .map(|fixture| fixture.bucket.as_str())
        .collect();
    for bucket in REQUIRED_BUCKETS {
        assert!(
            buckets.contains(bucket),
            "missing clarification coverage bucket {bucket}"
        );
    }

    for input in REQUIRED_INPUTS {
        let states: BTreeSet<_> = fixtures
            .iter()
            .flat_map(|fixture| fixture.coverage_inputs.iter())
            .filter(|coverage| coverage.key == *input)
            .map(|coverage| &coverage.state)
            .collect();
        assert!(
            states.contains(&InputState::Positive),
            "{input} needs positive coverage"
        );
        assert!(
            states.contains(&InputState::Missing) || states.contains(&InputState::Invalid),
            "{input} needs missing or invalid coverage"
        );
    }
}

#[test]
fn clarification_corpus_matches_production_catalog() {
    let catalog = load_catalog();
    let fixtures = load_fixtures();
    for fixture in &fixtures {
        assert_fixture(&catalog, fixture);
    }
}

#[test]
fn production_required_user_inputs_are_covered() {
    let catalog = load_catalog();
    let required_inputs: BTreeSet<_> = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .filter_map(|capability| {
            catalog
                .queries
                .iter()
                .find(|query| query.id == capability.query_id)
        })
        .flat_map(|query| {
            query.parameters.iter().filter(|parameter| {
                parameter.required && parameter.source.as_deref() != Some("authorized_scope")
            })
        })
        .filter_map(|parameter| {
            catalog
                .parameter_inputs
                .iter()
                .find(|input| input.parameters.iter().any(|name| name == &parameter.name))
        })
        .map(|input| input.id.as_str())
        .collect();
    assert_eq!(
        required_inputs,
        REQUIRED_INPUTS.iter().copied().collect(),
        "update corpus coverage when approved user inputs change"
    );
}
