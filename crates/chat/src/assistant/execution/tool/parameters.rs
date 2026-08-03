use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Supplies a value for a required row-limit parameter the user did not specify.
pub(super) const DEFAULT_REPORT_LIMIT: i64 = 10;

fn default_required_parameter(parameter: &QueryParameter) -> Option<Value> {
    (parameter.required && matches!(parameter.name.as_str(), "limit" | "top_n"))
        .then(|| json!(DEFAULT_REPORT_LIMIT))
}

use crate::{
    assistant::{
        AssistantEntityType, AssistantIntent, ConstraintField, DeterministicExtraction,
        EffectiveConstraints, LimitMode, ListPatch, Quantity, TypedFactValue,
    },
    knowledge::{
        catalog::parameter_policy::{EvaluationContext, ParameterPolicy, ResolvedValue},
        model::{CapabilityKnowledge, KnowledgeCatalog, QueryKnowledge, QueryParameter},
    },
};

pub fn approved_default_patch(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
) -> Result<crate::assistant::ConstraintPatch> {
    let capability = executable_capability(catalog, capability_id)?;
    let Some(limit) = capability.defaults.default_limit else {
        return Ok(Default::default());
    };
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    let mode = if query
        .parameters
        .iter()
        .any(|parameter| parameter.name == "top_n")
    {
        LimitMode::TopN
    } else if query
        .parameters
        .iter()
        .any(|parameter| parameter.name == "limit")
    {
        LimitMode::Limit
    } else {
        return Ok(Default::default());
    };
    Ok([
        (ConstraintField::LimitMode, TypedFactValue::LimitMode(mode)),
        (ConstraintField::LimitValue, TypedFactValue::Integer(limit)),
    ]
    .into_iter()
    .collect())
}

pub(super) fn normalize_effective_parameters(
    catalog: &KnowledgeCatalog,
    capability_id: &str,
    effective: &EffectiveConstraints,
) -> Result<Value> {
    let capability = executable_capability(catalog, capability_id)?;
    let query = catalog
        .queries
        .iter()
        .find(|item| item.id == capability.query_id)
        .ok_or_else(|| anyhow::anyhow!("selected capability has no approved query"))?;
    if let Some(TypedFactValue::Metric(metric)) = effective.values.get(&ConstraintField::Metric) {
        let trusted = normalize_metric(metric);
        if !capability
            .metrics
            .iter()
            .any(|item| normalize_metric(item) == trusted)
        {
            bail!("selected capability does not match requested metric {metric}");
        }
    }
    validate_effective_date_range(effective)?;
    let mut params = serde_json::Map::new();
    for parameter in &query.parameters {
        if matches!(
            parameter.source.as_deref(),
            Some("authorized_scope" | "transient_sensitive_input")
        ) {
            continue;
        }
        let value = effective_parameter(effective, &parameter.name)
            .or_else(|| default_required_parameter(parameter));
        if let Some(value) = value {
            params.insert(parameter.name.clone(), value);
        } else if parameter.required {
            bail!("missing parameter {}", parameter.name);
        }
    }
    clamp_hard_caps(&mut params, &capability.parameter_policies);
    Ok(Value::Object(params))
}

pub(super) fn clamp_hard_caps(
    params: &mut serde_json::Map<String, Value>,
    policies: &[ParameterPolicy],
) {
    for policy in policies {
        let Some(cap) = policy.hard_cap else {
            continue;
        };
        let Some(requested) = params.get(&policy.name).and_then(Value::as_i64) else {
            continue;
        };
        if requested > cap {
            tracing::warn!(
                target: "assistant::hard_cap_clamp",
                parameter = %policy.name,
                requested,
                applied = cap,
                "row-limit clamped to catalog hard_cap"
            );
            params.insert(policy.name.clone(), json!(cap));
        }
    }
}

fn validate_effective_date_range(effective: &EffectiveConstraints) -> Result<()> {
    let from = effective.values.get(&ConstraintField::FromDate);
    let to = effective.values.get(&ConstraintField::ToDate);
    if let (Some(TypedFactValue::Date(from)), Some(TypedFactValue::Date(to))) = (from, to) {
        let from = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("temporal_invalid_date: invalid from_date"))?;
        let to = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map_err(|_| anyhow::anyhow!("temporal_invalid_date: invalid to_date"))?;
        if from > to {
            bail!("temporal_range_reversed: start date is after end date");
        }
    }
    Ok(())
}

pub(super) fn executable_capability<'a>(
    catalog: &'a KnowledgeCatalog,
    capability_id: &str,
) -> Result<&'a CapabilityKnowledge> {
    catalog
        .capabilities
        .iter()
        .find(|item| item.id == capability_id && item.status == "approved_mvp")
        .ok_or_else(|| anyhow::anyhow!("selected capability is not executable"))
}

fn effective_parameter(effective: &EffectiveConstraints, name: &str) -> Option<Value> {
    let value = |field| effective.values.get(&field);
    match name {
        "from_date" => typed_string(value(ConstraintField::FromDate)).map(Value::String),
        "to_date" => typed_string(value(ConstraintField::ToDate)).map(Value::String),
        "currency_code" => typed_string(value(ConstraintField::CurrencyCode)).map(Value::String),
        "product_ids" => typed_ids(value(ConstraintField::ProductIds)).map(|ids| json!(ids)),
        "product_id" => typed_ids(value(ConstraintField::ProductIds))
            .and_then(|ids| ids.first().copied())
            .map(|id| json!(id))
            .or_else(|| {
                typed_string(value(ConstraintField::Product))
                    .and_then(|v| v.parse().ok())
                    .map(|id: i64| json!(id))
            }),
        "client_id" => match value(ConstraintField::ClientId) {
            Some(TypedFactValue::ClientId(id)) => Some(json!(id)),
            _ => None,
        },
        "product_name" => typed_string(value(ConstraintField::Product)).map(Value::String),
        "search" | "name" => [
            ConstraintField::PersonName,
            ConstraintField::Office,
            ConstraintField::Product,
        ]
        .into_iter()
        .find_map(|field| typed_string(value(field)))
        .map(Value::String),
        "office" | "office_name" => typed_string(value(ConstraintField::Office)).map(Value::String),
        "latest_transaction_amount" => {
            typed_string(value(ConstraintField::TransactionAmount)).map(Value::String)
        }
        "limit" | "top_n" => match (
            value(ConstraintField::LimitMode),
            value(ConstraintField::LimitValue),
        ) {
            (
                Some(TypedFactValue::LimitMode(LimitMode::Limit | LimitMode::TopN)),
                Some(TypedFactValue::Integer(limit)),
            ) => Some(json!(limit)),
            _ => None,
        },
        _ => None,
    }
}

fn typed_string(value: Option<&TypedFactValue>) -> Option<String> {
    match value? {
        TypedFactValue::Date(v)
        | TypedFactValue::Decimal(v)
        | TypedFactValue::CurrencyCode(v)
        | TypedFactValue::Metric(v)
        | TypedFactValue::PersonName(v)
        | TypedFactValue::Office(v)
        | TypedFactValue::Product(v) => Some(v.clone()),
        _ => None,
    }
}

fn typed_ids(value: Option<&TypedFactValue>) -> Option<Vec<i64>> {
    match value? {
        TypedFactValue::IdList(ListPatch::Replace(ids)) => Some(ids.clone()),
        _ => None,
    }
}

pub(super) fn validate_snapshot_parameters(query: &QueryKnowledge, params: &Value) -> Result<()> {
    let params = params
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("snapshot parameters must be an object"))?;
    for name in params.keys() {
        if !query.parameters.iter().any(|parameter| {
            parameter.name == *name
                && !matches!(
                    parameter.source.as_deref(),
                    Some("authorized_scope" | "transient_sensitive_input")
                )
        }) {
            bail!("snapshot contains unexpected parameter {name}");
        }
    }
    for parameter in &query.parameters {
        if matches!(
            parameter.source.as_deref(),
            Some("authorized_scope" | "transient_sensitive_input")
        ) {
            continue;
        }
        let Some(value) = params.get(&parameter.name) else {
            if parameter.required {
                bail!("snapshot missing parameter {}", parameter.name);
            }
            continue;
        };
        let valid = match parameter.kind.as_str() {
            "date" | "string" | "decimal" => value.is_string(),
            "integer" => value.as_i64().is_some(),
            "array_bigint" => value
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_i64().is_some())),
            _ => false,
        };
        if !valid {
            bail!("snapshot parameter {} has invalid type", parameter.name);
        }
    }
    Ok(())
}

pub(super) fn params_from_verified(
    query: &QueryKnowledge,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
    policies: &[ParameterPolicy],
    ctx: Option<&EvaluationContext>,
) -> Result<Value> {
    let mut params = serde_json::Map::new();
    let person_name = deterministic_extraction.and_then(|extraction| {
        entity_value_from(&extraction.entities, AssistantEntityType::PersonName)
    });
    let office = entity_value(intent, AssistantEntityType::Office);
    let product = entity_value(intent, AssistantEntityType::Product);
    let trusted = deterministic_extraction.map(|extraction| &extraction.constraints);
    let currency = trusted.and_then(|constraints| constraints.currency_code.as_deref());

    for parameter in &query.parameters {
        if matches!(
            parameter.source.as_deref(),
            Some("authorized_scope" | "transient_sensitive_input")
        ) {
            continue;
        }
        let value = match parameter.name.as_str() {
            "from_date" => trusted
                .and_then(|constraints| constraints.from_date.as_ref().map(|value| json!(value))),
            "to_date" => trusted
                .and_then(|constraints| constraints.to_date.as_ref().map(|value| json!(value))),
            "currency_code" => currency.map(|value| json!(value)),
            "product_ids" => intent
                .constraints
                .product_ids
                .as_ref()
                .map(|value| json!(value)),
            "product_id" => intent
                .constraints
                .product_ids
                .as_ref()
                .and_then(|ids| ids.first())
                .map(|value| json!(value))
                .or_else(|| {
                    product
                        .and_then(|value| value.parse::<i64>().ok())
                        .map(|value| json!(value))
                }),
            "client_id" => entity_value(intent, AssistantEntityType::ClientId)
                .and_then(|value| value.parse::<i64>().ok())
                .map(|value| json!(value)),
            "product_name" => product.map(|value| json!(value)),
            "search" | "name" => person_name.or(office).or(product).map(|value| json!(value)),
            "office" | "office_name" => office.map(|value| json!(value)),
            "latest_transaction_amount" => trusted
                .and_then(|constraints| constraints.transaction_amount.as_ref())
                .map(|value| json!(value)),
            "limit" | "top_n" => trusted.and_then(quantity_limit).map(|value| json!(value)),
            _ => None,
        }
        .or_else(|| resolve_policy_default(policies, ctx, &parameter.name))
        .or_else(|| default_required_parameter(parameter));
        if let Some(value) = value {
            params.insert(parameter.name.clone(), value);
        } else if parameter.required {
            bail!("missing parameter {}", parameter.name);
        }
    }

    clamp_hard_caps(&mut params, policies);
    Ok(Value::Object(params))
}

/// Look up a per-parameter policy default and evaluate it against `ctx`.
/// Returns `None` when the parameter has no policy, no default, is still
/// required, or when no evaluation context was supplied.
pub(super) fn resolve_policy_default(
    policies: &[ParameterPolicy],
    ctx: Option<&EvaluationContext>,
    name: &str,
) -> Option<Value> {
    let ctx = ctx?;
    let policy = policies.iter().find(|p| p.name == name)?;
    if policy.required {
        return None;
    }
    let default = policy.default.as_ref()?;
    Some(resolved_to_value(default.evaluate(ctx)))
}

fn resolved_to_value(resolved: ResolvedValue) -> Value {
    match resolved {
        ResolvedValue::Date(d) => json!(d.to_string()),
        ResolvedValue::Integer(i) => json!(i),
        ResolvedValue::IntegerArray(ids) => json!(ids),
        // Unbounded: no user-supplied cap. Bound as i64::MAX so callers that
        // require an integer parameter (e.g. `LIMIT $n`) still bind; the SQL
        // repository clamps this to the effective row cap (declared hard_cap or
        // the configured global backstop) before binding.
        // ponytail: i64::MAX sentinel, upgrade to LIMIT-omitting SQL if a real
        // "no limit" query appears.
        ResolvedValue::Unbounded => json!(i64::MAX),
    }
}

pub(super) fn verify_capability_metric(
    capability_metrics: &[String],
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<()> {
    let Some(metric) =
        deterministic_extraction.and_then(|extraction| extraction.constraints.metric.as_deref())
    else {
        return Ok(());
    };
    let trusted = normalize_metric(metric);
    if capability_metrics
        .iter()
        .any(|metric| normalize_metric(metric) == trusted)
    {
        Ok(())
    } else {
        bail!("selected capability does not match requested metric {metric}")
    }
}

fn normalize_metric(metric: &str) -> String {
    match metric.replace('_', ".").as_str() {
        "savings.account.count" => "savings.account_count".into(),
        "savings.balance" => "savings.balance_total".into(),
        "deposit.volume" | "savings.deposit.volume" => "savings.deposit_total".into(),
        other => other.into(),
    }
}

fn entity_value(intent: &AssistantIntent, entity_type: AssistantEntityType) -> Option<&str> {
    entity_value_from(&intent.entities, entity_type)
}

fn entity_value_from(
    entities: &[crate::assistant::AssistantEntity],
    entity_type: AssistantEntityType,
) -> Option<&str> {
    entities
        .iter()
        .find(|entity| entity.entity_type == entity_type)
        .map(|entity| entity.value.trim())
        .filter(|value| !value.is_empty())
}

fn quantity_limit(constraints: &crate::assistant::AssistantConstraints) -> Option<i64> {
    quantity_limit_from(constraints.quantity.as_ref())
}

fn quantity_limit_from(quantity: Option<&Quantity>) -> Option<i64> {
    match quantity? {
        Quantity::Limit { value } | Quantity::TopN { value } => Some(*value),
        Quantity::Default => None,
        Quantity::All => None,
    }
}
