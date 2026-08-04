use crate::assistant::workflow::contract::{NodeInput, NodeOutputSlot, WorkflowProposal};
use crate::knowledge::{dataset::model::ShapeRole, model::KnowledgeCatalog};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataTool {
    pub name: &'static str,
    pub description: String,
}

pub const METADATA_TOOL_NAMES: [&str; 6] = [
    "search_catalog",
    "inspect_capability",
    "inspect_dataset",
    "find_entity_resolver",
    "find_compatible_next_steps",
    "propose_workflow",
];

/// Static metadata-only planning surface. Description text is regenerated from the active
/// approved catalog so it cannot drift from reviewed capability and dataset contracts.
pub fn registry(catalog: &KnowledgeCatalog) -> Vec<MetadataTool> {
    METADATA_TOOL_NAMES
        .into_iter()
        .map(|name| MetadataTool {
            name,
            description: description(name, catalog),
        })
        .collect()
}
fn description(name: &str, catalog: &KnowledgeCatalog) -> String {
    let capabilities = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .map(|capability| {
            format!(
                "{}: {}",
                capability.id,
                capability.display_name.as_deref().unwrap_or(&capability.id)
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let datasets = catalog
        .datasets
        .iter()
        .map(|dataset| dataset.id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let resolvers = catalog
        .datasets
        .iter()
        .flat_map(|dataset| {
            dataset
                .shapes
                .iter()
                .filter(move |shape| matches!(shape.role, ShapeRole::Resolver | ShapeRole::Probe))
                .map(move |shape| format!("{}.{}", dataset.id, shape.id))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{name}: capabilities=[{capabilities}]; datasets=[{datasets}]; resolvers=[{resolvers}]")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityMetadata {
    pub id: String,
    pub display_name: String,
    pub domain: String,
    pub output_mode: String,
    pub parameter_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetMetadata {
    pub id: String,
    pub shape_ids: Vec<String>,
    pub filter_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityResolverMetadata {
    pub dataset_id: String,
    pub shape_id: String,
    pub entity_kind: Option<String>,
    pub produces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSearch {
    pub capabilities: Vec<CapabilityMetadata>,
    pub datasets: Vec<DatasetMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataToolError {
    UnknownCapability,
}

/// Searches only reviewed identifiers and display metadata; source SQL and
/// dataset expressions are intentionally not part of the planning surface.
pub fn search_catalog(catalog: &KnowledgeCatalog, query: &str) -> CatalogSearch {
    let query = query.to_ascii_lowercase();
    let capabilities = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .filter(|capability| capability_matches(capability, &query))
        .map(capability_metadata)
        .collect();
    let datasets = catalog
        .datasets
        .iter()
        .filter(|dataset| dataset.id.to_ascii_lowercase().contains(&query))
        .map(dataset_metadata)
        .collect();
    CatalogSearch {
        capabilities,
        datasets,
    }
}

pub fn inspect_capability(catalog: &KnowledgeCatalog, id: &str) -> Option<CapabilityMetadata> {
    catalog
        .capabilities
        .iter()
        .find(|capability| capability.status == "approved_mvp" && capability.id == id)
        .map(capability_metadata)
}

pub fn inspect_dataset(catalog: &KnowledgeCatalog, id: &str) -> Option<DatasetMetadata> {
    catalog
        .datasets
        .iter()
        .find(|dataset| dataset.id == id)
        .map(dataset_metadata)
}

pub fn find_entity_resolver(
    catalog: &KnowledgeCatalog,
    entity_kind: &str,
) -> Vec<EntityResolverMetadata> {
    catalog
        .datasets
        .iter()
        .filter(|dataset| {
            dataset
                .entity
                .as_ref()
                .is_some_and(|entity| entity.kind == entity_kind)
        })
        .flat_map(|dataset| {
            dataset
                .shapes
                .iter()
                .filter(|shape| matches!(shape.role, ShapeRole::Resolver | ShapeRole::Probe))
                .map(move |shape| EntityResolverMetadata {
                    dataset_id: dataset.id.clone(),
                    shape_id: shape.id.clone(),
                    entity_kind: dataset.entity.as_ref().map(|entity| entity.kind.clone()),
                    produces: shape
                        .produces
                        .iter()
                        .map(|slot| slot.slot.clone())
                        .collect(),
                })
        })
        .collect()
}

/// Returns type-compatible bindings only. It does not infer or expose SQL
/// columns, filters, or expressions.
pub fn find_compatible_next_steps<'a>(
    outputs: &'a [NodeOutputSlot],
    inputs: &'a [NodeInput],
) -> Vec<(&'a NodeOutputSlot, &'a NodeInput)> {
    outputs
        .iter()
        .flat_map(|output| {
            inputs
                .iter()
                .filter(move |input| output.kind == input.kind)
                .map(move |input| (output, input))
        })
        .collect()
}

/// Creates a SQL-free proposal from selected approved capability IDs. The
/// compiler remains the sole authority that resolves the IDs into execution
/// nodes and approved catalog resources.
pub fn propose_workflow(
    catalog: &KnowledgeCatalog,
    capability_ids: Vec<String>,
) -> Result<WorkflowProposal, MetadataToolError> {
    if capability_ids
        .iter()
        .any(|id| inspect_capability(catalog, id).is_none())
    {
        return Err(MetadataToolError::UnknownCapability);
    }
    Ok(WorkflowProposal {
        capability_ids,
        nodes: Vec::new(),
        edges: Vec::new(),
    })
}

fn capability_matches(
    capability: &crate::knowledge::model::CapabilityKnowledge,
    query: &str,
) -> bool {
    query.is_empty()
        || capability.id.to_ascii_lowercase().contains(query)
        || capability
            .display_name
            .as_deref()
            .is_some_and(|name| name.to_ascii_lowercase().contains(query))
        || capability
            .description
            .as_deref()
            .is_some_and(|description| description.to_ascii_lowercase().contains(query))
}

fn capability_metadata(
    capability: &crate::knowledge::model::CapabilityKnowledge,
) -> CapabilityMetadata {
    CapabilityMetadata {
        id: capability.id.clone(),
        display_name: capability
            .display_name
            .clone()
            .unwrap_or_else(|| capability.id.clone()),
        domain: capability.domain.clone(),
        output_mode: capability.output_mode.clone(),
        parameter_names: capability
            .parameter_policies
            .iter()
            .map(|policy| policy.name.clone())
            .collect(),
    }
}

fn dataset_metadata(
    dataset: &crate::knowledge::dataset::model::DatasetKnowledge,
) -> DatasetMetadata {
    DatasetMetadata {
        id: dataset.id.clone(),
        shape_ids: dataset
            .shapes
            .iter()
            .map(|shape| shape.id.clone())
            .collect(),
        filter_ids: dataset
            .filters
            .iter()
            .map(|filter| filter.id.clone())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use crate::knowledge::catalog::loader::KnowledgeLoader;

    use super::*;

    #[test]
    fn descriptions_follow_catalog_metadata_and_registry_has_no_data_or_policy_tool() {
        let catalog = KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap();
        let mut changed = catalog.clone();
        let capability = changed
            .capabilities
            .iter_mut()
            .find(|capability| capability.status == "approved_mvp")
            .expect("approved capability");
        capability.display_name = Some("Changed test display name".into());
        let changed_tools = registry(&changed);
        assert_ne!(registry(&catalog), changed_tools);
        let tools = registry(&catalog);
        assert_eq!(tools.len(), 6);
        assert!(tools.iter().all(|tool| {
            !tool.name.contains("sql")
                && !tool.name.contains("policy")
                && !tool.description.contains("SELECT")
        }));
    }

    #[test]
    fn metadata_tools_execute_only_catalog_operations() {
        let catalog = KnowledgeLoader::new("../../knowledge", "../../queries")
            .load()
            .unwrap();
        let capability = catalog
            .capabilities
            .iter()
            .find(|capability| capability.status == "approved_mvp")
            .expect("approved capability");
        let dataset = catalog.datasets.first().expect("dataset");

        let search = search_catalog(&catalog, &capability.id);
        assert!(
            search
                .capabilities
                .iter()
                .any(|item| item.id == capability.id)
        );
        assert_eq!(
            inspect_capability(&catalog, &capability.id).unwrap().id,
            capability.id
        );
        assert_eq!(
            inspect_dataset(&catalog, &dataset.id).unwrap().id,
            dataset.id
        );
        let entity_kind = dataset
            .entity
            .as_ref()
            .map(|entity| entity.kind.as_str())
            .unwrap_or("not_an_entity");
        assert!(
            find_entity_resolver(&catalog, entity_kind)
                .iter()
                .all(|resolver| resolver.dataset_id == dataset.id
                    || resolver.entity_kind.as_deref() == Some(entity_kind))
        );
        assert_eq!(
            propose_workflow(&catalog, vec![capability.id.clone()])
                .unwrap()
                .capability_ids,
            vec![capability.id.clone()]
        );
        assert_eq!(
            propose_workflow(&catalog, vec!["not_approved".into()]),
            Err(MetadataToolError::UnknownCapability)
        );
    }

    #[test]
    fn compatible_next_steps_match_types_and_nothing_else() {
        use crate::assistant::workflow::contract::{BindingSource, Cardinality};
        use crate::knowledge::{catalog::parameter_policy::ParameterType, model::Sensitivity};

        let outputs = vec![
            NodeOutputSlot {
                name: "client_id".into(),
                kind: ParameterType::Integer,
                sensitivity: Sensitivity::PublicBusiness,
                cardinality: Cardinality::One,
            },
            NodeOutputSlot {
                name: "currency".into(),
                kind: ParameterType::Currency,
                sensitivity: Sensitivity::PublicBusiness,
                cardinality: Cardinality::One,
            },
        ];
        let inputs = vec![
            NodeInput {
                parameter: "client_id".into(),
                kind: ParameterType::Integer,
                source: BindingSource::AuthorizedScope,
            },
            NodeInput {
                parameter: "date".into(),
                kind: ParameterType::Date,
                source: BindingSource::CatalogDefault,
            },
        ];
        let matches = find_compatible_next_steps(&outputs, &inputs);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0.name, "client_id");
        assert_eq!(matches[0].1.parameter, "client_id");
    }
}
