use anyhow::{Result, bail};
use serde_json::{Value, json};

/// Supplies a value for a required row-limit parameter the user did not specify.
pub(super) const DEFAULT_REPORT_LIMIT: i64 = 10;

fn default_required_parameter(parameter: &QueryParameter) -> Option<Value> {
    (parameter.required && matches!(parameter.name.as_str(), "limit" | "top_n"))
        .then(|| json!(DEFAULT_REPORT_LIMIT))
}

use std::collections::BTreeMap;

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
    if let Some(TypedFactValue::Metric(metric)) = effective.values.get(&ConstraintField::Metric)
        && let Some(requested) = catalog.resolve_metric_id(metric)
        && !capability
            .metrics
            .iter()
            .any(|item| catalog.resolve_metric_id(item) == Some(requested))
    {
        bail!("selected capability does not match requested metric {metric}");
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
        let value = bind_parameter(catalog, &effective.values, parameter)
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

/// Bind one query parameter from whatever canonical facts we hold, following
/// the precedence the catalog declares for it.
///
/// This is the single binding site. It replaced three separate Rust matches on
/// the parameter name that had each grown their own idea of what fills what;
/// `knowledge/parameter-bindings/` now answers that once, and the validator
/// refuses to load a parameter it does not cover.
pub(super) fn bind_parameter(
    catalog: &KnowledgeCatalog,
    facts: &BTreeMap<ConstraintField, TypedFactValue>,
    parameter: &QueryParameter,
) -> Option<Value> {
    catalog
        .binding_fields(&parameter.name)
        .iter()
        .find_map(|field| bind_value(&parameter.kind, facts.get(field)?))
}

/// Shape a canonical fact into the JSON the approved SQL expects, using the
/// parameter's declared type rather than its name.
fn bind_value(kind: &str, value: &TypedFactValue) -> Option<Value> {
    match value {
        TypedFactValue::Date(text)
        | TypedFactValue::Decimal(text)
        | TypedFactValue::CurrencyCode(text)
        | TypedFactValue::Metric(text)
        | TypedFactValue::PersonName(text)
        | TypedFactValue::Office(text)
        | TypedFactValue::Product(text)
        | TypedFactValue::ChargeType(text)
        | TypedFactValue::AccountNumber(text) => match kind {
            "integer" => text.parse::<i64>().ok().map(|value| json!(value)),
            _ => Some(json!(text)),
        },
        TypedFactValue::Integer(value) | TypedFactValue::ClientId(value) => Some(json!(value)),
        TypedFactValue::IdList(ListPatch::Replace(ids)) => match kind {
            "integer" => ids.first().map(|id| json!(id)),
            _ => Some(json!(ids)),
        },
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
    catalog: &KnowledgeCatalog,
    query: &QueryKnowledge,
    intent: &AssistantIntent,
    deterministic_extraction: Option<&DeterministicExtraction>,
    policies: &[ParameterPolicy],
    ctx: Option<&EvaluationContext>,
) -> Result<Value> {
    let mut params = serde_json::Map::new();
    let facts = request_facts(Some(intent), deterministic_extraction);

    for parameter in &query.parameters {
        if matches!(
            parameter.source.as_deref(),
            Some("authorized_scope" | "transient_sensitive_input")
        ) {
            continue;
        }
        let value = bind_parameter(catalog, &facts, parameter)
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

/// Canonical facts for one turn, assembled from the deterministic extractor and
/// the model's intent under the existing trust rule: the extractor is verified
/// against the user's own words, so it wins, and the model may only contribute
/// fields it cannot fabricate a plausible-looking wrong answer for.
///
/// A model-claimed date, limit, currency, transaction amount or person name is
/// still discarded — those are the fields a hallucination silently answers the
/// wrong question with, or returns another customer's data for.
///
/// `retrieval::sufficiency` reads the same map to decide whether a candidate
/// capability can honour what the user asked for, so "what the user said" has
/// exactly one definition in this crate; hence `intent` is optional here, since
/// that caller runs on turns where no intent has been routed yet.
pub(crate) fn request_facts(
    intent: Option<&AssistantIntent>,
    extraction: Option<&DeterministicExtraction>,
) -> BTreeMap<ConstraintField, TypedFactValue> {
    let mut facts = BTreeMap::new();
    if let Some(extraction) = extraction {
        let constraints = &extraction.constraints;
        for (field, value) in [
            (ConstraintField::FromDate, constraints.from_date.as_ref()),
            (ConstraintField::ToDate, constraints.to_date.as_ref()),
        ] {
            if let Some(value) = value {
                facts.insert(field, TypedFactValue::Date(value.clone()));
            }
        }
        if let Some(value) = &constraints.currency_code {
            facts.insert(
                ConstraintField::CurrencyCode,
                TypedFactValue::CurrencyCode(value.clone()),
            );
        }
        if let Some(value) = &constraints.transaction_amount {
            facts.insert(
                ConstraintField::TransactionAmount,
                TypedFactValue::Decimal(value.clone()),
            );
        }
        if let Some(limit) = quantity_limit(constraints) {
            facts.insert(ConstraintField::LimitValue, TypedFactValue::Integer(limit));
        }
        for entity in &extraction.entities {
            if let Some((field, value)) = crate::assistant::entity_fact(entity) {
                facts.insert(field, value);
            }
        }
    }
    // The model's own entities are a real source — until now they were carried
    // on the intent and read by three ad-hoc lookups, so anything the router
    // named but the regex missed was simply lost. They fill only gaps: inserted
    // after the extractor, they never overwrite a verified fact.
    let Some(intent) = intent else {
        return facts;
    };
    if let Some(ids) = &intent.constraints.product_ids {
        facts
            .entry(ConstraintField::ProductIds)
            .or_insert_with(|| TypedFactValue::IdList(ListPatch::Replace(ids.clone())));
    }
    for entity in &intent.entities {
        if !MODEL_TRUSTED_ENTITIES.contains(&entity.entity_type) {
            continue;
        }
        if let Some((field, value)) = crate::assistant::entity_fact(entity) {
            facts.entry(field).or_insert(value);
        }
    }
    facts
}

/// Entity kinds the model may supply directly. Each names a thing the SQL
/// matches by equality against a catalog-approved column, so a wrong guess
/// returns no rows rather than the wrong rows — unlike a person name, where a
/// wrong guess quietly returns a different customer.
const MODEL_TRUSTED_ENTITIES: &[AssistantEntityType] = &[
    AssistantEntityType::ClientId,
    AssistantEntityType::Office,
    AssistantEntityType::Product,
    AssistantEntityType::ChargeType,
];

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

/// Guard against executing a capability that measures something other than what
/// the caller asked for — the failure mode where a weekly-charge count question
/// was answered with a list of clients holding unpaid charges.
///
/// Resolution goes through the catalog's metric aliases rather than a local
/// table. An extractor guess the catalog does not recognise at all is treated as
/// no signal, not as a mismatch: the extractor is a substring heuristic, and
/// letting it veto a capability the reranker chose on real evidence is how
/// `savings_balance_summary` came to reject "what is the total savings balance
/// right now?".
pub(super) fn verify_capability_metric(
    catalog: &KnowledgeCatalog,
    capability_metrics: &[String],
    deterministic_extraction: Option<&DeterministicExtraction>,
) -> Result<()> {
    let Some(metric) =
        deterministic_extraction.and_then(|extraction| extraction.constraints.metric.as_deref())
    else {
        return Ok(());
    };
    let Some(requested) = catalog.resolve_metric_id(metric) else {
        return Ok(());
    };
    if capability_metrics
        .iter()
        .any(|declared| catalog.resolve_metric_id(declared) == Some(requested))
    {
        Ok(())
    } else {
        bail!("selected capability does not match requested metric {metric}")
    }
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
