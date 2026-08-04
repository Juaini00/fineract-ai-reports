use std::fs::read_dir;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::knowledge::catalog::parameter_policy::{
    DefaultExpr, ParameterPolicy, ParameterType, ProbeRef, ResolutionStrategy,
};
use crate::knowledge::dataset::model::DatasetKnowledge;
use crate::knowledge::model::{
    CapabilityKnowledge, DataAreasKnowledge, DomainKnowledge, GenericKnowledge, KnowledgeCatalog,
    ParameterBindingKnowledge, ParameterInputKnowledge, QueryKnowledge,
};

pub struct KnowledgeLoader {
    root_path: PathBuf,
    query_path: PathBuf,
}

impl KnowledgeLoader {
    pub fn new(root_path: impl Into<PathBuf>, query_path: impl Into<PathBuf>) -> Self {
        Self {
            root_path: root_path.into(),
            query_path: query_path.into(),
        }
    }

    pub fn load(&self) -> Result<KnowledgeCatalog> {
        let data_areas = self.load_yaml_dir::<DataAreasKnowledge>("data-scope/areas")?;
        let domains = self.load_yaml_dir::<DomainKnowledge>("domains")?;
        let schemas = self.load_yaml_dir::<GenericKnowledge>("schema")?;
        let metrics = self.load_yaml_dir::<GenericKnowledge>("metrics")?;
        let capabilities = self.load_capabilities()?;
        let queries = self.load_yaml_dir::<QueryKnowledge>("queries")?;
        let policies = self.load_yaml_dir::<GenericKnowledge>("policies")?;
        let responses = self.load_yaml_dir::<GenericKnowledge>("responses")?;
        let parameter_inputs = self.load_yaml_dir::<ParameterInputKnowledge>("parameters")?;
        let parameter_bindings = self
            .load_yaml_dir::<ParameterBindingKnowledge>("parameter-bindings")?
            .into_iter()
            .flat_map(|file| file.bindings)
            .collect();
        let datasets = self.load_yaml_dir::<DatasetKnowledge>("datasets")?;

        let classification = self.load_classification_policy()?;

        Ok(KnowledgeCatalog {
            root_path: self.root_path.clone(),
            query_path: self.query_path.clone(),
            data_areas,
            domains,
            schemas,
            metrics,
            capabilities,
            queries,
            policies,
            responses,
            parameter_inputs,
            parameter_bindings,
            classification,
            datasets,
        })
    }

    fn load_classification_policy(&self) -> Result<crate::knowledge::model::ClassificationPolicy> {
        use crate::knowledge::model::ClassificationPolicy;
        let path = self.root_path.join("policies").join("classification.yaml");
        if !path.exists() {
            return Ok(ClassificationPolicy::default());
        }
        let contents =
            std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let policy: ClassificationPolicy = serde_yaml::from_str(&contents)
            .with_context(|| format!("parse classification policy at {}", path.display()))?;
        Ok(policy)
    }

    fn load_capabilities(&self) -> Result<Vec<CapabilityKnowledge>> {
        let dir = self.root_path.join("capabilities");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for path in collect_yaml_files(&dir)? {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let cap = parse_capability(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            out.push(cap);
        }
        Ok(out)
    }

    fn load_yaml_dir<T>(&self, relative_dir: &str) -> Result<Vec<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let dir = self.root_path.join(relative_dir);
        let mut items = Vec::new();

        if !dir.exists() {
            return Ok(items);
        }

        for path in collect_yaml_files(&dir)? {
            let content = std::fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let item = serde_yaml::from_str::<T>(&content)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            items.push(item);
        }

        Ok(items)
    }
}

fn collect_yaml_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    collect_yaml_files_recursive(dir, &mut files)?;

    files.sort();
    Ok(files)
}

fn collect_yaml_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_yaml_files_recursive(&path, files)?;
            continue;
        }

        if path.extension().and_then(|extension| extension.to_str()) == Some("yaml") {
            files.push(path);
        }
    }

    Ok(())
}

/// Parse a capability YAML, lifting the optional `parameters:` block into
/// `CapabilityKnowledge::parameter_policies` and leaving legacy fields
/// (`required_parameters`, `optional_parameters`) untouched during the
/// migration window.
pub fn parse_capability(yaml: &str) -> Result<CapabilityKnowledge> {
    let mut value: serde_yaml::Value = serde_yaml::from_str(yaml).context("invalid YAML")?;
    let policies = match value.as_mapping_mut().and_then(|m| m.remove("parameters")) {
        Some(raw) => parse_parameters_block(&raw)?,
        None => Vec::new(),
    };
    let mut cap: CapabilityKnowledge =
        serde_yaml::from_value(value).context("capability schema mismatch")?;
    cap.parameter_policies = policies;
    Ok(cap)
}

fn parse_parameters_block(value: &serde_yaml::Value) -> Result<Vec<ParameterPolicy>> {
    let mapping = value
        .as_mapping()
        .context("`parameters` must be a mapping of name -> policy")?;
    let mut out = Vec::with_capacity(mapping.len());
    for (name_val, policy_val) in mapping {
        let name = name_val
            .as_str()
            .context("parameter name must be a string")?
            .to_string();
        let policy_map = policy_val
            .as_mapping()
            .with_context(|| format!("policy for `{name}` must be a mapping"))?;
        let kind = read_type(policy_map, &name)?;
        let required = policy_map
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let default = match policy_map.get("default") {
            Some(v) => Some(read_default_expr(v, &name)?),
            None => None,
        };
        let fill_when_missing = policy_map
            .get("fill_when_missing")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let user_may_override = policy_map
            .get("user_may_override")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        let hard_cap = policy_map.get("hard_cap").and_then(|v| v.as_i64());
        let user_required = policy_map
            .get("user_required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let resolution = match policy_map.get("resolution") {
            Some(value) => serde_yaml::from_value::<Vec<ResolutionStrategy>>(value.clone())
                .with_context(|| format!("`resolution` for `{name}` must be a strategy list"))?,
            None => Vec::new(),
        };
        let probe = match policy_map.get("probe") {
            Some(value) => Some(
                serde_yaml::from_value::<ProbeRef>(value.clone())
                    .with_context(|| format!("`probe` for `{name}` is invalid"))?,
            ),
            None => None,
        };
        out.push(ParameterPolicy {
            name,
            kind,
            required,
            default,
            fill_when_missing,
            user_may_override,
            hard_cap,
            user_required,
            resolution,
            probe,
        });
    }
    Ok(out)
}

fn read_type(map: &serde_yaml::Mapping, name: &str) -> Result<ParameterType> {
    let raw = map
        .get("type")
        .and_then(|v| v.as_str())
        .with_context(|| format!("policy for `{name}` is missing `type`"))?;
    match raw {
        "date" => Ok(ParameterType::Date),
        "integer" => Ok(ParameterType::Integer),
        "integer_array" => Ok(ParameterType::IntegerArray),
        "string" => Ok(ParameterType::String),
        "currency" => Ok(ParameterType::Currency),
        other => anyhow::bail!("policy for `{name}` has unknown type `{other}`"),
    }
}

fn read_default_expr(value: &serde_yaml::Value, name: &str) -> Result<DefaultExpr> {
    let expr = value
        .as_str()
        .with_context(|| format!("`default` for `{name}` must be a string expression"))?;
    DefaultExpr::parse(expr).map_err(|e| anyhow::anyhow!("`default` for `{name}`: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP_WITH_POLICY: &str = r#"
id: loan_arrears_clients
status: approved_mvp
domain: loan
query_id: loan.arrears_clients
output_mode: table
request_shape:
  operation: list
  subject: client
  grouping: none
  output: list
  pii: client_identity
parameters:
  as_of:
    type: date
    required: false
    default: business_today
    fill_when_missing: true
  limit:
    type: integer
    required: false
    default: unbounded
    hard_cap: 10000
  office_ids:
    type: integer_array
    required: false
    default: authorized_scope
    user_may_override: false
"#;

    #[test]
    fn parses_new_parameters_block_into_policies() {
        let cap = parse_capability(CAP_WITH_POLICY).unwrap();
        assert_eq!(cap.parameter_policies.len(), 3);
        let by_name: std::collections::BTreeMap<_, _> = cap
            .parameter_policies
            .iter()
            .map(|p| (p.name.as_str(), p))
            .collect();
        assert_eq!(by_name["as_of"].default, Some(DefaultExpr::BusinessToday));
        assert!(by_name["as_of"].fill_when_missing);
        assert_eq!(by_name["limit"].default, Some(DefaultExpr::Unbounded));
        assert_eq!(by_name["limit"].hard_cap, Some(10000));
        assert!(!by_name["office_ids"].user_may_override);
    }

    #[test]
    fn parses_parameter_acquisition_metadata() {
        let cap = parse_capability(
            r#"
id: probe_capability
status: approved_mvp
domain: savings
query_id: probe.query
output_mode: table
request_shape:
  operation: list
  subject: savings_account
  grouping: none
  output: list
  pii: none
parameters:
  client_id:
    type: integer
    user_required: true
    resolution: [deterministic_extraction, authorized_data_probe, clarify]
    probe:
      dataset_id: client.identity
      shape_id: identity_candidates
      output_slot: client_id
"#,
        )
        .unwrap();
        let policy = &cap.parameter_policies[0];
        assert!(policy.user_required);
        assert_eq!(
            policy.resolution,
            vec![
                ResolutionStrategy::DeterministicExtraction,
                ResolutionStrategy::AuthorizedDataProbe,
                ResolutionStrategy::Clarify,
            ]
        );
        assert_eq!(
            policy.probe.as_ref().unwrap().shape_id,
            "identity_candidates"
        );
    }

    #[test]
    fn legacy_capability_without_parameters_block_still_loads() {
        let cap = parse_capability(
            r#"
id: legacy
status: approved_mvp
domain: savings
query_id: legacy.query
output_mode: table
request_shape:
  operation: list
  subject: savings_account
  grouping: none
  output: list
  pii: none
required_parameters: [from_date, to_date]
"#,
        )
        .unwrap();
        assert!(cap.parameter_policies.is_empty());
        assert_eq!(cap.required_parameters, vec!["from_date", "to_date"]);
    }

    #[test]
    fn load_query_reads_timeout_ms() {
        let yaml = r#"
id: savings.demo
database: fineract
sql_file: queries/savings/demo.sql
timeout_ms: 8000
parameters:
  - name: limit
    type: integer
    required: false
"#;
        let query: crate::knowledge::model::QueryKnowledge =
            serde_yaml::from_str(yaml).expect("query yaml parses");
        assert_eq!(query.timeout_ms, Some(8000));
    }

    #[test]
    fn load_query_timeout_ms_absent_is_none() {
        let yaml = r#"
id: savings.demo
database: fineract
sql_file: queries/savings/demo.sql
"#;
        let query: crate::knowledge::model::QueryKnowledge =
            serde_yaml::from_str(yaml).expect("query yaml parses");
        assert_eq!(query.timeout_ms, None);
    }

    #[test]
    fn unknown_default_expression_is_rejected() {
        let bad = r#"
id: bad
status: approved_mvp
domain: savings
query_id: bad.query
output_mode: table
request_shape:
  operation: list
  subject: savings_account
  grouping: none
  output: list
  pii: none
parameters:
  from_date:
    type: date
    default: today() + 1d
"#;
        assert!(parse_capability(bad).is_err());
    }
}
