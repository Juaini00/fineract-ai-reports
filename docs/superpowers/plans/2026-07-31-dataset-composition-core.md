# Dataset Composition Core (Phase A1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the dataset model, SQL-expression grammar validator, and SQL composition engine, and prove that every one of the 32 existing capabilities composes to byte-identical SQL — without wiring any of it into the request path.

**Architecture:** A dataset declares a source (joins), plus three whitelists: filter slots, shapes, and order-by expressions. Composition assembles one statement from authored files and declared expressions only. A *degenerate* dataset — one shape, no fragment, no filters — is how every existing capability is represented, and composition returns its source SQL verbatim, which makes Phase A equivalence provable by string comparison rather than by querying a database.

**Tech Stack:** Rust 2024, `crates/chat`, `serde`/`serde_yaml`, `sqlx` (Postgres), `anyhow`. Tests are `cargo test -p chat`.

## Global Constraints

- Workspace is locked to three crates — `app`, `core`, `chat`. Do not add a crate. (`CLAUDE.md`)
- No `sqlx` calls in handlers or services — repositories only. (`CLAUDE.md`)
- Every character of executable SQL must originate in a file on disk or a declared `expr` in YAML. The LLM contributes only ids and values. (spec §Composition)
- Existing SQL guards are preserved verbatim: `select_only`, `single_statement`, `parameterized_only`, `require_office_filter`.
- Knowledge stays as YAML under `knowledge/`; SQL under `queries/`. (`CLAUDE.md`)
- Files 200–400 lines typical, 800 max; prefer many small focused files. (coding-style)
- Pre-commit hook runs `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`. Both must pass or the commit is rejected.
- No behaviour change in this plan. Nothing added here is called from the request path.

## File Structure

| File | Responsibility |
|---|---|
| `crates/chat/src/knowledge/dataset/mod.rs` | Module re-exports only |
| `crates/chat/src/knowledge/dataset/model.rs` | `DatasetKnowledge`, `FilterSlot`, `FilterOperator`, `ShapeOption`, `OrderByOption`, `DatasetOutputField` |
| `crates/chat/src/knowledge/dataset/grammar.rs` | `validate_sql_expr` — the trust boundary for declared SQL fragments |
| `crates/chat/src/knowledge/dataset/legacy.rs` | `degenerate_dataset` — derives a dataset from an existing capability + query |
| `crates/chat/src/knowledge/dataset/compose.rs` | `compose` — assembles the final statement and parameter order |
| `crates/chat/tests/dataset_equivalence.rs` | The Phase A oracle: all 32 capabilities compose verbatim |

`crates/chat/src/knowledge/mod.rs` gains `pub mod dataset;`.

---

### Task 1: SQL expression grammar validator

The security boundary. `filters[].expr` and `order_by[].expr` are concatenated into SQL, so they need a strict grammar even though they are authored rather than user-supplied. This task ships first because every later task depends on it.

**Files:**
- Create: `crates/chat/src/knowledge/dataset/grammar.rs`
- Create: `crates/chat/src/knowledge/dataset/mod.rs`
- Modify: `crates/chat/src/knowledge/mod.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `pub fn validate_sql_expr(expr: &str) -> Result<(), String>` — `Ok(())` when `expr` is a comma-separated list of terms, each `identifier` or `identifier.identifier`, optionally followed by `ASC`/`DESC`, optionally followed by `NULLS FIRST`/`NULLS LAST`. `Err(reason)` otherwise. Used by Task 4 and by the catalog validator in Plan A2.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/grammar.rs` containing only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_declared_expression_forms() {
        for expr in [
            "sac.charge_due_date",
            "amount",
            "sac.created_on_utc DESC",
            "sac.created_on_utc DESC, sac.id DESC",
            "sac.charge_due_date ASC NULLS LAST",
            "sac.charge_due_date ASC NULLS LAST, sac.id DESC",
        ] {
            assert!(validate_sql_expr(expr).is_ok(), "should accept: {expr}");
        }
    }

    #[test]
    fn rejects_anything_that_could_extend_the_statement() {
        for expr in [
            "sac.id; DROP TABLE m_client",
            "sac.id -- comment",
            "sac.id /* comment */",
            "(SELECT 1)",
            "count(sac.id)",
            "sac.id DESC UNION SELECT 1",
            "sac.id'",
            "sac.id\nDROP",
            "a.b.c",
            "",
            "   ",
            ",",
            "sac.id,,sac.name",
            "sac.id SIDEWAYS",
            "sac.id DESC NULLS SOMEWHERE",
            "1",
            "sac.1id",
        ] {
            assert!(validate_sql_expr(expr).is_err(), "should reject: {expr}");
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib grammar`
Expected: FAIL — compile error, `validate_sql_expr` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/grammar.rs`, above the test module:

```rust
//! Grammar for declared SQL expressions.
//!
//! `filters[].expr` and `order_by[].expr` are concatenated into executable SQL.
//! They are authored, not user-supplied, but concatenation makes them a trust
//! boundary regardless: this module is what stops a mistyped or malicious
//! declaration from extending the statement.

/// Accepts a comma-separated list of ordering terms. Each term is a bare or
/// table-qualified identifier, optionally `ASC`/`DESC`, optionally
/// `NULLS FIRST`/`NULLS LAST`. Everything else is rejected.
pub fn validate_sql_expr(expr: &str) -> Result<(), String> {
    if expr.trim().is_empty() {
        return Err("expression is empty".into());
    }
    for forbidden in [";", "--", "/*", "*/", "'", "\"", "(", ")"] {
        if expr.contains(forbidden) {
            return Err(format!("expression contains forbidden token `{forbidden}`"));
        }
    }
    for term in expr.split(',') {
        validate_term(term)?;
    }
    Ok(())
}

fn validate_term(term: &str) -> Result<(), String> {
    let mut words = term.split_whitespace();
    let Some(identifier) = words.next() else {
        return Err("expression has an empty term".into());
    };
    validate_identifier(identifier)?;

    let mut rest: Vec<&str> = words.collect();
    if matches!(
        rest.first().map(|word| word.to_ascii_uppercase()).as_deref(),
        Some("ASC") | Some("DESC")
    ) {
        rest.remove(0);
    }
    match rest.len() {
        0 => Ok(()),
        2 if rest[0].eq_ignore_ascii_case("NULLS")
            && (rest[1].eq_ignore_ascii_case("FIRST") || rest[1].eq_ignore_ascii_case("LAST")) =>
        {
            Ok(())
        }
        _ => Err(format!("unexpected tokens in term `{}`", term.trim())),
    }
}

fn validate_identifier(identifier: &str) -> Result<(), String> {
    let parts: Vec<&str> = identifier.split('.').collect();
    if parts.len() > 2 {
        return Err(format!("identifier `{identifier}` has too many qualifiers"));
    }
    for part in parts {
        let mut chars = part.chars();
        let valid_start = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !valid_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!("invalid identifier `{identifier}`"));
        }
    }
    Ok(())
}
```

Create `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod grammar;
```

Add to `crates/chat/src/knowledge/mod.rs`, alongside the existing `pub mod` lines:

```rust
pub mod dataset;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib grammar`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/knowledge/dataset/ crates/chat/src/knowledge/mod.rs
git commit -m "feat(knowledge): add declared SQL expression grammar validator"
```

---

### Task 2: Dataset model

**Files:**
- Create: `crates/chat/src/knowledge/dataset/model.rs`
- Modify: `crates/chat/src/knowledge/dataset/mod.rs`

**Interfaces:**
- Consumes: `crate::assistant::RequestShape` (existing), `crate::knowledge::model::{QueryParameter, Sensitivity}` (existing).
- Produces: `DatasetKnowledge`, `FilterSlot`, `FilterOperator`, `ShapeOption`, `OrderByOption`, `DatasetOutputField`. Field names and types below are relied on verbatim by Tasks 3, 4 and 5.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/model.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
id: savings.account_charges
database: fineract
source_sql: queries/savings/account_charges.source.sql
tables: [m_savings_account_charge, m_client]
filters:
  - id: due_date
    expr: sac.charge_due_date
    type: date
    operators: [eq, lt, between]
shapes:
  - id: list
    request_shape:
      operation: list
      subject: savings_account_charge
      grouping: none
      output: list
    order_by: [created_desc]
order_by:
  - id: created_desc
    expr: sac.created_on_utc DESC, sac.id DESC
output_fields:
  - name: savings_account_charge_id
    type: bigint
    sensitivity: public_business
    core: true
  - name: client_display_name
    type: string
    sensitivity: pii
parameters:
  - name: office_ids
    type: array_bigint
    required: true
    source: authorized_scope
"#;

    #[test]
    fn parses_dataset_yaml() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert_eq!(dataset.id, "savings.account_charges");
        assert_eq!(dataset.filters.len(), 1);
        assert_eq!(dataset.filters[0].id, "due_date");
        assert_eq!(
            dataset.filters[0].operators,
            vec![FilterOperator::Eq, FilterOperator::Lt, FilterOperator::Between]
        );
        assert_eq!(dataset.shapes.len(), 1);
        assert_eq!(dataset.shapes[0].order_by, vec!["created_desc".to_string()]);
        assert!(dataset.shapes[0].fragment.is_none());
        assert_eq!(dataset.order_by[0].expr, "sac.created_on_utc DESC, sac.id DESC");
    }

    #[test]
    fn core_defaults_to_false_and_is_read_when_present() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert!(dataset.output_fields[0].core);
        assert!(!dataset.output_fields[1].core);
    }

    #[test]
    fn core_field_names_returns_only_core_fields() {
        let dataset: DatasetKnowledge = serde_yaml::from_str(SAMPLE).unwrap();
        assert_eq!(dataset.core_field_names(), vec!["savings_account_charge_id"]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib dataset::model`
Expected: FAIL — compile error, `DatasetKnowledge` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/model.rs`:

```rust
//! The dataset contract: one source, plus whitelists for filters, shapes and
//! ordering. See docs/superpowers/specs/2026-07-31-dataset-model-design.md.

use serde::Deserialize;

use crate::assistant::RequestShape;
use crate::knowledge::model::{QueryParameter, Sensitivity};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DatasetKnowledge {
    pub id: String,
    pub database: String,
    /// Path to the authored source SQL (joins + office scope), relative to the
    /// repository root when it starts with `queries/`.
    pub source_sql: String,

    #[serde(default)]
    pub tables: Vec<String>,

    #[serde(default)]
    pub filters: Vec<FilterSlot>,

    pub shapes: Vec<ShapeOption>,

    #[serde(default)]
    pub order_by: Vec<OrderByOption>,

    #[serde(default)]
    pub output_fields: Vec<DatasetOutputField>,

    #[serde(default)]
    pub parameters: Vec<QueryParameter>,

    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

impl DatasetKnowledge {
    /// Fields rendered for every request, regardless of projection hints.
    pub fn core_field_names(&self) -> Vec<String> {
        self.output_fields
            .iter()
            .filter(|field| field.core)
            .map(|field| field.name.clone())
            .collect()
    }

    pub fn shape(&self, shape_id: &str) -> Option<&ShapeOption> {
        self.shapes.iter().find(|shape| shape.id == shape_id)
    }

    pub fn order_by_expr(&self, order_by_id: &str) -> Option<&str> {
        self.order_by
            .iter()
            .find(|option| option.id == order_by_id)
            .map(|option| option.expr.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilterSlot {
    /// The id the LLM refers to. Never a SQL identifier.
    pub id: String,
    /// The SQL column expression. Validated by `grammar::validate_sql_expr`.
    pub expr: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub operators: Vec<FilterOperator>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterOperator {
    Eq,
    Lt,
    Lte,
    Gt,
    Gte,
    Between,
}

impl FilterOperator {
    /// SQL operator text. `Between` is expanded by the composer, not here.
    pub fn as_sql(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
            Self::Between => "BETWEEN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ShapeOption {
    pub id: String,
    pub request_shape: RequestShape,
    /// Path to the authored SQL fragment applied over the `base` CTE. `None`
    /// means degenerate passthrough: the source SQL is already complete.
    #[serde(default)]
    pub fragment: Option<String>,
    #[serde(default)]
    pub order_by: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct OrderByOption {
    pub id: String,
    pub expr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DatasetOutputField {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub sensitivity: Sensitivity,
    /// Rendered for every request. Non-core fields are opt-in via projection.
    #[serde(default)]
    pub core: bool,
}
```

Update `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod grammar;
pub mod model;
```

`DatasetKnowledge` derives `PartialEq` and holds `Vec<QueryParameter>`, so
`QueryParameter` must derive it too. In `crates/chat/src/knowledge/model.rs`,
change:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct QueryParameter {
```

to:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct QueryParameter {
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib dataset::model`
Expected: PASS — 3 tests.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/knowledge/dataset/
git commit -m "feat(knowledge): add dataset contract model"
```

---

### Task 3: Derive a degenerate dataset from a legacy capability

Every existing capability is a dataset with one shape, no fragment, and no filters. Deriving this in code rather than authoring 32 YAML files is what makes Phase A mechanical and reviewable.

**Files:**
- Create: `crates/chat/src/knowledge/dataset/legacy.rs`
- Modify: `crates/chat/src/knowledge/dataset/mod.rs`

**Interfaces:**
- Consumes: `DatasetKnowledge`, `ShapeOption`, `DatasetOutputField` from Task 2; `CapabilityKnowledge`, `QueryKnowledge` from `crate::knowledge::model`.
- Produces: `pub fn degenerate_dataset(capability: &CapabilityKnowledge, query: &QueryKnowledge) -> DatasetKnowledge`. Task 5 relies on this exact signature.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/legacy.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::model::{
        CapabilityDefaults, CapabilityGuards, CapabilityKnowledge, QueryKnowledge, QueryOutputField,
        QueryParameter, Sensitivity,
    };

    fn capability() -> CapabilityKnowledge {
        CapabilityKnowledge {
            id: "savings_deposit_total".into(),
            status: "approved_mvp".into(),
            domain: "savings".into(),
            query_id: "savings.deposit_total".into(),
            output_mode: "summary".into(),
            request_shape: RequestShape {
                operation: RequestOperation::Total,
                subject: RequestSubject::SavingsTransaction,
                grouping: RequestGrouping::None,
                output: RequestOutput::Scalar,
                pii: RequestPii::None,
            },
            display_name: None,
            description: None,
            data_areas: Vec::new(),
            metrics: Vec::new(),
            examples: Vec::new(),
            required_parameters: Vec::new(),
            optional_parameters: Vec::new(),
            defaults: CapabilityDefaults::default(),
            guards: CapabilityGuards::default(),
            parameter_policies: Vec::new(),
        }
    }

    fn query() -> QueryKnowledge {
        QueryKnowledge {
            id: "savings.deposit_total".into(),
            database: "fineract".into(),
            sql_file: "queries/savings/deposit_total.sql".into(),
            data_areas: Vec::new(),
            tables: vec!["m_savings_account_transaction".into()],
            metrics: Vec::new(),
            parameters: vec![QueryParameter {
                name: "office_ids".into(),
                kind: "array_bigint".into(),
                required: true,
                source: Some("authorized_scope".into()),
            }],
            output_fields: vec![QueryOutputField {
                name: "total_deposit_amount".into(),
                kind: "decimal".into(),
                sensitivity: Sensitivity::PublicBusiness,
            }],
            timeout_ms: Some(3000),
        }
    }

    #[test]
    fn derives_a_single_shape_with_no_fragment_and_no_filters() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.id, "savings.deposit_total");
        assert!(dataset.filters.is_empty(), "degenerate dataset has no filter slots");
        assert!(dataset.order_by.is_empty(), "ordering stays inside the source SQL");
        assert_eq!(dataset.shapes.len(), 1);
        assert!(
            dataset.shapes[0].fragment.is_none(),
            "no fragment means the source SQL is already complete"
        );
        assert_eq!(dataset.shapes[0].request_shape, capability().request_shape);
    }

    #[test]
    fn carries_source_parameters_and_timeout_unchanged() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.source_sql, "queries/savings/deposit_total.sql");
        assert_eq!(dataset.database, "fineract");
        assert_eq!(dataset.parameters, query().parameters);
        assert_eq!(dataset.timeout_ms, Some(3000));
        assert_eq!(dataset.tables, query().tables);
    }

    #[test]
    fn every_output_field_is_core_so_projection_is_a_no_op() {
        let dataset = degenerate_dataset(&capability(), &query());
        assert_eq!(dataset.output_fields.len(), 1);
        assert!(
            dataset.output_fields.iter().all(|field| field.core),
            "Phase A must not change which columns render"
        );
        assert_eq!(dataset.core_field_names(), vec!["total_deposit_amount"]);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib dataset::legacy`
Expected: FAIL — compile error, `degenerate_dataset` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/legacy.rs`:

```rust
//! Bridges the existing capability model onto the dataset model.
//!
//! A capability today freezes source, filter, shape and projection together.
//! That is exactly a dataset with one shape, no fragment, and no filter slots —
//! so the conversion is mechanical and needs no authored YAML. Composition of
//! such a dataset returns the source SQL verbatim, which is what makes Phase A
//! equivalence provable by string comparison.

use crate::knowledge::dataset::model::{DatasetKnowledge, DatasetOutputField, ShapeOption};
use crate::knowledge::model::{CapabilityKnowledge, QueryKnowledge};

/// The shape id used for every legacy-derived dataset.
pub const LEGACY_SHAPE_ID: &str = "legacy";

pub fn degenerate_dataset(
    capability: &CapabilityKnowledge,
    query: &QueryKnowledge,
) -> DatasetKnowledge {
    DatasetKnowledge {
        id: query.id.clone(),
        database: query.database.clone(),
        source_sql: query.sql_file.clone(),
        tables: query.tables.clone(),
        // No filter slots: the legacy WHERE clause is baked into the source SQL.
        filters: Vec::new(),
        shapes: vec![ShapeOption {
            id: LEGACY_SHAPE_ID.to_string(),
            request_shape: capability.request_shape.clone(),
            // No fragment: the source SQL already selects, orders and limits.
            fragment: None,
            order_by: Vec::new(),
        }],
        // Ordering stays inside the source SQL for the degenerate case.
        order_by: Vec::new(),
        output_fields: query
            .output_fields
            .iter()
            .map(|field| DatasetOutputField {
                name: field.name.clone(),
                kind: field.kind.clone(),
                sensitivity: field.sensitivity,
                // Phase A must not change which columns render, so every field
                // is core and projection is a no-op until a dataset is merged.
                core: true,
            })
            .collect(),
        parameters: query.parameters.clone(),
        timeout_ms: query.timeout_ms,
    }
}
```

Update `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod grammar;
pub mod legacy;
pub mod model;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib dataset::legacy`
Expected: PASS — 3 tests.

`RequestShape` already derives `Clone, Default, PartialEq, Eq`, and
`CapabilityDefaults` / `CapabilityGuards` already derive `Default`, so the test
fixtures compile without touching those types.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/knowledge/dataset/
git commit -m "feat(knowledge): derive degenerate datasets from legacy capabilities"
```

---

### Task 4: SQL composition

**Files:**
- Create: `crates/chat/src/knowledge/dataset/compose.rs`
- Modify: `crates/chat/src/knowledge/dataset/mod.rs`

**Interfaces:**
- Consumes: `DatasetKnowledge`, `ShapeOption`, `FilterOperator` from Task 2; `validate_sql_expr` from Task 1.
- Produces: `pub struct ComposedSql { pub sql: String, pub filter_binds: Vec<FilterBind> }` and `pub fn compose(dataset: &DatasetKnowledge, shape_id: &str, order_by_id: Option<&str>, source_sql: &str, fragment_sql: Option<&str>) -> anyhow::Result<ComposedSql>`. Task 5 relies on this signature.

Note the caller supplies file *contents*, not paths. Composition stays pure and filesystem-free, so it is unit-testable without fixtures on disk.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/compose.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{
        DatasetKnowledge, FilterOperator, FilterSlot, OrderByOption, ShapeOption,
    };

    fn shape(id: &str, fragment: Option<&str>, order_by: Vec<&str>) -> ShapeOption {
        ShapeOption {
            id: id.into(),
            request_shape: RequestShape {
                operation: RequestOperation::List,
                subject: RequestSubject::SavingsAccountCharge,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::None,
            },
            fragment: fragment.map(str::to_string),
            order_by: order_by.into_iter().map(str::to_string).collect(),
        }
    }

    fn dataset(filters: Vec<FilterSlot>, shapes: Vec<ShapeOption>) -> DatasetKnowledge {
        DatasetKnowledge {
            id: "test.dataset".into(),
            database: "fineract".into(),
            source_sql: "queries/test.sql".into(),
            tables: vec!["m_client".into()],
            filters,
            shapes,
            order_by: vec![OrderByOption {
                id: "created_desc".into(),
                expr: "sac.created_on_utc DESC, sac.id DESC".into(),
            }],
            output_fields: Vec::new(),
            parameters: Vec::new(),
            timeout_ms: None,
        }
    }

    #[test]
    fn degenerate_dataset_returns_source_sql_verbatim() {
        let source = "SELECT a, b\nFROM m_client c\nWHERE c.office_id = ANY($1::bigint[])\nORDER BY c.id\nLIMIT $2;";
        let data = dataset(Vec::new(), vec![shape("legacy", None, Vec::new())]);

        let composed = compose(&data, "legacy", None, source, None).unwrap();

        assert_eq!(composed.sql, source, "Phase A must not alter legacy SQL");
        assert!(composed.filter_binds.is_empty());
    }

    #[test]
    fn wraps_source_in_a_cte_and_appends_fragment_and_order_by() {
        let source = "SELECT a FROM m_client c WHERE c.office_id = ANY($1::bigint[])";
        let data = dataset(
            Vec::new(),
            vec![shape("list", Some("unused"), vec!["created_desc"])],
        );

        let composed = compose(&data, "list", Some("created_desc"), source, Some("SELECT * FROM base")).unwrap();

        assert_eq!(
            composed.sql,
            "WITH base AS (\nSELECT a FROM m_client c WHERE c.office_id = ANY($1::bigint[])\n)\nSELECT * FROM base\nORDER BY sac.created_on_utc DESC, sac.id DESC"
        );
    }

    #[test]
    fn emits_one_null_passthrough_predicate_per_filter_operator() {
        let filters = vec![FilterSlot {
            id: "due_date".into(),
            expr: "sac.charge_due_date".into(),
            kind: "date".into(),
            operators: vec![FilterOperator::Eq, FilterOperator::Lt],
        }];
        let data = dataset(filters, vec![shape("list", Some("f"), Vec::new())]);

        let composed = compose(&data, "list", None, "SELECT a FROM t WHERE x = $1", Some("SELECT * FROM base")).unwrap();

        assert!(composed.sql.contains("($2::date IS NULL OR sac.charge_due_date = $2)"));
        assert!(composed.sql.contains("($3::date IS NULL OR sac.charge_due_date < $3)"));
        assert_eq!(composed.filter_binds.len(), 2);
        assert_eq!(composed.filter_binds[0].filter_id, "due_date");
        assert_eq!(composed.filter_binds[0].operator, FilterOperator::Eq);
        assert_eq!(composed.filter_binds[0].placeholder, 2);
        assert_eq!(composed.filter_binds[1].placeholder, 3);
    }

    #[test]
    fn rejects_an_order_by_expression_that_fails_the_grammar() {
        let mut data = dataset(Vec::new(), vec![shape("list", Some("f"), vec!["evil"])]);
        data.order_by.push(OrderByOption {
            id: "evil".into(),
            expr: "sac.id; DROP TABLE m_client".into(),
        });

        let error = compose(&data, "list", Some("evil"), "SELECT a FROM t", Some("SELECT * FROM base"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("forbidden token"), "got: {error}");
    }

    #[test]
    fn rejects_unknown_shape_and_unknown_order_by() {
        let data = dataset(Vec::new(), vec![shape("list", Some("f"), Vec::new())]);

        assert!(compose(&data, "nope", None, "SELECT a", Some("SELECT * FROM base")).is_err());
        assert!(compose(&data, "list", Some("nope"), "SELECT a", Some("SELECT * FROM base")).is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib dataset::compose`
Expected: FAIL — compile error, `compose` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/compose.rs`:

```rust
//! Assembles one statement from authored SQL plus declared expressions.
//!
//! Inputs are file *contents*, never paths, so composition stays pure. Every
//! character of the result originates in an authored file or a declared `expr`
//! that has passed `grammar::validate_sql_expr`.

use anyhow::{Result, bail};

use crate::knowledge::dataset::grammar::validate_sql_expr;
use crate::knowledge::dataset::model::{DatasetKnowledge, FilterOperator};

/// One `$n` placeholder reserved for a declared filter slot + operator pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterBind {
    pub filter_id: String,
    pub operator: FilterOperator,
    /// 1-based positional placeholder index in the composed statement.
    pub placeholder: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedSql {
    pub sql: String,
    pub filter_binds: Vec<FilterBind>,
}

pub fn compose(
    dataset: &DatasetKnowledge,
    shape_id: &str,
    order_by_id: Option<&str>,
    source_sql: &str,
    fragment_sql: Option<&str>,
) -> Result<ComposedSql> {
    let Some(shape) = dataset.shape(shape_id) else {
        bail!("dataset {} has no shape {shape_id}", dataset.id);
    };

    // Degenerate dataset: the source SQL is already a complete statement.
    // Returning it verbatim is what keeps Phase A byte-identical.
    if shape.fragment.is_none() && dataset.filters.is_empty() && order_by_id.is_none() {
        return Ok(ComposedSql {
            sql: source_sql.to_string(),
            filter_binds: Vec::new(),
        });
    }

    let Some(fragment_sql) = fragment_sql else {
        bail!("shape {shape_id} of dataset {} requires a fragment", dataset.id);
    };

    let mut placeholder = dataset.parameters.len();
    let mut predicates = String::new();
    let mut filter_binds = Vec::new();
    for filter in &dataset.filters {
        validate_sql_expr(&filter.expr).map_err(|reason| {
            anyhow::anyhow!("dataset {} filter {}: {reason}", dataset.id, filter.id)
        })?;
        for operator in &filter.operators {
            placeholder += 1;
            predicates.push_str(&predicate(&filter.expr, *operator, &filter.kind, placeholder));
            filter_binds.push(FilterBind {
                filter_id: filter.id.clone(),
                operator: *operator,
                placeholder,
            });
        }
    }

    let order_by_clause = match order_by_id {
        Some(id) => {
            let Some(expr) = dataset.order_by_expr(id) else {
                bail!("dataset {} has no order_by {id}", dataset.id);
            };
            validate_sql_expr(expr).map_err(|reason| {
                anyhow::anyhow!("dataset {} order_by {id}: {reason}", dataset.id)
            })?;
            format!("\nORDER BY {expr}")
        }
        None => String::new(),
    };

    let sql = format!("WITH base AS (\n{source_sql}{predicates}\n)\n{fragment_sql}{order_by_clause}");

    Ok(ComposedSql { sql, filter_binds })
}

/// Null-passthrough predicate. An inactive filter binds NULL and the predicate
/// short-circuits, so the statement text is identical whether or not a filter
/// is used. That property is what keeps the validator's cross product at
/// `shapes x order_by` instead of `2^filters x shapes x order_by`.
fn predicate(expr: &str, operator: FilterOperator, kind: &str, placeholder: usize) -> String {
    let cast = match kind {
        "date" => "::date",
        "integer" => "::bigint",
        "boolean" => "::bool",
        _ => "::text",
    };
    match operator {
        FilterOperator::Between => format!(
            "\n  AND (${placeholder}{cast} IS NULL OR ${next}{cast} IS NULL OR {expr} BETWEEN ${placeholder} AND ${next})",
            next = placeholder + 1
        ),
        _ => format!(
            "\n  AND (${placeholder}{cast} IS NULL OR {expr} {op} ${placeholder})",
            op = operator.as_sql()
        ),
    }
}
```

Update `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod compose;
pub mod grammar;
pub mod legacy;
pub mod model;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib dataset::compose`
Expected: PASS — 5 tests.

The `Between` arm reserves two placeholders but `filter_binds` records one entry with the first index; the second is `placeholder + 1`. Confirm the `emits_one_null_passthrough_predicate_per_filter_operator` test still passes, since it uses only `Eq` and `Lt`.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/src/knowledge/dataset/
git commit -m "feat(knowledge): add dataset SQL composition"
```

---

### Task 5: Equivalence oracle across all 32 capabilities

The Phase A acceptance criterion. Proves that deriving a degenerate dataset from every approved capability and composing it reproduces the legacy SQL exactly.

**Files:**
- Create: `crates/chat/tests/dataset_equivalence.rs`

**Interfaces:**
- Consumes: `degenerate_dataset` and `LEGACY_SHAPE_ID` from Task 3, `compose` from Task 4, `KnowledgeLoader` from `chat::knowledge::catalog::loader`.
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/tests/dataset_equivalence.rs`:

```rust
//! Phase A acceptance: every approved capability, converted to a degenerate
//! dataset and composed, must reproduce its legacy SQL exactly. This is the
//! oracle that lets later phases merge datasets without guessing whether
//! behaviour changed.

use std::path::{Path, PathBuf};

use chat::knowledge::catalog::loader::KnowledgeLoader;
use chat::knowledge::dataset::compose::compose;
use chat::knowledge::dataset::legacy::{LEGACY_SHAPE_ID, degenerate_dataset};
use chat::knowledge::model::{KnowledgeCatalog, QueryKnowledge};

fn repo_root() -> PathBuf {
    // crates/chat -> repository root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate is two levels below the repository root")
        .to_path_buf()
}

fn load_catalog() -> KnowledgeCatalog {
    let root = repo_root();
    KnowledgeLoader::new(root.join("knowledge"), root.join("queries"))
        .load()
        .expect("catalog loads")
}

fn read_sql(query: &QueryKnowledge) -> String {
    let path = repo_root().join(&query.sql_file);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

#[test]
fn every_approved_capability_composes_to_its_legacy_sql() {
    let catalog = load_catalog();
    let approved: Vec<_> = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .collect();

    assert!(
        approved.len() >= 30,
        "expected the full approved catalog, found {}",
        approved.len()
    );

    for capability in approved {
        let query = catalog
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)
            .unwrap_or_else(|| panic!("capability {} has no query", capability.id));

        let dataset = degenerate_dataset(capability, query);
        let source = read_sql(query);

        let composed = compose(&dataset, LEGACY_SHAPE_ID, None, &source, None)
            .unwrap_or_else(|err| panic!("compose {}: {err}", capability.id));

        assert_eq!(
            composed.sql, source,
            "capability {} composed SQL differs from its legacy SQL",
            capability.id
        );
        assert!(
            composed.filter_binds.is_empty(),
            "capability {} must bind no filters in Phase A",
            capability.id
        );
    }
}

#[test]
fn every_derived_dataset_keeps_the_full_output_contract() {
    let catalog = load_catalog();

    for capability in catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
    {
        let query = catalog
            .queries
            .iter()
            .find(|query| query.id == capability.query_id)
            .unwrap_or_else(|| panic!("capability {} has no query", capability.id));

        let dataset = degenerate_dataset(capability, query);

        let declared: Vec<&str> = query
            .output_fields
            .iter()
            .map(|field| field.name.as_str())
            .collect();
        let core: Vec<String> = dataset.core_field_names();

        assert_eq!(
            core, declared,
            "capability {} must render every declared column in Phase A",
            capability.id
        );
        assert_eq!(dataset.parameters, query.parameters);
        assert_eq!(dataset.timeout_ms, query.timeout_ms);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --test dataset_equivalence`
Expected: FAIL — compile errors on any module not re-exported publicly.

If `chat::knowledge::dataset` is not reachable from an integration test, confirm `crates/chat/src/lib.rs` declares `pub mod knowledge;` and that `crates/chat/src/knowledge/mod.rs` declares `pub mod dataset;`. Both must be `pub`.

- [ ] **Step 3: Make the modules reachable**

No production logic changes. Ensure these declarations exist and are public:

```rust
// crates/chat/src/knowledge/mod.rs
pub mod dataset;
```

```rust
// crates/chat/src/knowledge/dataset/mod.rs
pub mod compose;
pub mod grammar;
pub mod legacy;
pub mod model;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --test dataset_equivalence`
Expected: PASS — 2 tests, iterating every `approved_mvp` capability.

If a capability fails equality, do **not** relax the assertion. Composition is returning something other than the verbatim source, which means the degenerate branch in `compose` was not taken — check that the derived dataset has no filters, no `order_by`, and a `None` fragment.

- [ ] **Step 5: Commit**

```bash
cargo fmt -p chat
git add crates/chat/tests/dataset_equivalence.rs crates/chat/src/knowledge/
git commit -m "test(knowledge): prove every capability composes to its legacy SQL"
```

---

### Task 6: Cross-product PREPARE for authored datasets

Extends the existing runtime validation so that any authored dataset — none exist yet; the first arrives in Phase B — has every `shape x order_by` combination prepared against Fineract at startup, rather than discovered by a user request.

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/validator.rs:680-718` (`validate_runtime`)
- Create: `crates/chat/src/knowledge/dataset/plan.rs`
- Modify: `crates/chat/src/knowledge/dataset/mod.rs`

**Interfaces:**
- Consumes: `DatasetKnowledge` from Task 2, `compose` from Task 4.
- Produces: `pub fn executable_combinations(dataset: &DatasetKnowledge) -> Vec<(String, Option<String>)>` returning `(shape_id, order_by_id)` pairs. Consumed by `validate_runtime` and by Plan A2's execution path.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/plan.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{DatasetKnowledge, OrderByOption, ShapeOption};

    fn shape(id: &str, fragment: Option<&str>, order_by: Vec<&str>) -> ShapeOption {
        ShapeOption {
            id: id.into(),
            request_shape: RequestShape {
                operation: RequestOperation::List,
                subject: RequestSubject::SavingsAccountCharge,
                grouping: RequestGrouping::None,
                output: RequestOutput::List,
                pii: RequestPii::None,
            },
            fragment: fragment.map(str::to_string),
            order_by: order_by.into_iter().map(str::to_string).collect(),
        }
    }

    fn dataset(shapes: Vec<ShapeOption>, order_by: Vec<&str>) -> DatasetKnowledge {
        DatasetKnowledge {
            id: "test.dataset".into(),
            database: "fineract".into(),
            source_sql: "queries/test.sql".into(),
            tables: Vec::new(),
            filters: Vec::new(),
            shapes,
            order_by: order_by
                .into_iter()
                .map(|id| OrderByOption {
                    id: id.into(),
                    expr: format!("t.{id}"),
                })
                .collect(),
            output_fields: Vec::new(),
            parameters: Vec::new(),
            timeout_ms: None,
        }
    }

    #[test]
    fn degenerate_dataset_has_exactly_one_combination_with_no_ordering() {
        let data = dataset(vec![shape("legacy", None, Vec::new())], Vec::new());
        assert_eq!(
            executable_combinations(&data),
            vec![("legacy".to_string(), None)]
        );
    }

    #[test]
    fn expands_each_shape_across_its_declared_order_by_options() {
        let data = dataset(
            vec![
                shape("list", Some("f"), vec!["a", "b"]),
                shape("total", Some("f"), Vec::new()),
            ],
            vec!["a", "b"],
        );

        assert_eq!(
            executable_combinations(&data),
            vec![
                ("list".to_string(), Some("a".to_string())),
                ("list".to_string(), Some("b".to_string())),
                ("total".to_string(), None),
            ]
        );
    }

    #[test]
    fn filters_do_not_multiply_the_combination_count() {
        use crate::knowledge::dataset::model::{FilterOperator, FilterSlot};

        let mut data = dataset(vec![shape("list", Some("f"), vec!["a"])], vec!["a"]);
        data.filters = vec![
            FilterSlot {
                id: "due_date".into(),
                expr: "t.due_date".into(),
                kind: "date".into(),
                operators: vec![FilterOperator::Eq, FilterOperator::Lt],
            },
            FilterSlot {
                id: "is_paid".into(),
                expr: "t.is_paid".into(),
                kind: "boolean".into(),
                operators: vec![FilterOperator::Eq],
            },
        ];

        assert_eq!(
            executable_combinations(&data).len(),
            1,
            "null-passthrough keeps statement text identical regardless of filters"
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib dataset::plan`
Expected: FAIL — compile error, `executable_combinations` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/plan.rs`:

```rust
//! Enumerates every executable statement a dataset can produce.
//!
//! Filters deliberately do not appear here. Because they are emitted as
//! null-passthrough predicates, statement text is identical whether a filter is
//! active or not, so the cross product stays `shapes x order_by` rather than
//! `2^filters x shapes x order_by`.

use crate::knowledge::dataset::model::DatasetKnowledge;

pub fn executable_combinations(dataset: &DatasetKnowledge) -> Vec<(String, Option<String>)> {
    let mut combinations = Vec::new();
    for shape in &dataset.shapes {
        if shape.order_by.is_empty() {
            combinations.push((shape.id.clone(), None));
            continue;
        }
        for order_by_id in &shape.order_by {
            combinations.push((shape.id.clone(), Some(order_by_id.clone())));
        }
    }
    combinations
}
```

Update `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod compose;
pub mod grammar;
pub mod legacy;
pub mod model;
pub mod plan;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib dataset::plan`
Expected: PASS — 3 tests.

- [ ] **Step 5: Wire dataset preparation into `validate_runtime`**

In `crates/chat/src/knowledge/catalog/validator.rs`, add this function below the existing `validate_runtime`, and call it from the end of `validate_runtime` just before `Ok(())`:

```rust
/// Prepares every executable `shape x order_by` combination of every authored
/// dataset. Composition errors are catalog errors and the catalog is fully
/// enumerable at boot, so a user request must never be what discovers a broken
/// fragment.
async fn validate_dataset_runtime(
    catalog: &KnowledgeCatalog,
    fineract_pool: &PgPool,
) -> Result<()> {
    use crate::knowledge::dataset::compose::compose;
    use crate::knowledge::dataset::plan::executable_combinations;

    for dataset in &catalog.datasets {
        if dataset.database != "fineract" {
            continue;
        }
        let source_path = resolve_catalog_sql_path(catalog, &dataset.source_sql);
        let source = std::fs::read_to_string(&source_path)
            .with_context(|| format!("read source_sql for dataset {}", dataset.id))?;

        for (shape_id, order_by_id) in executable_combinations(dataset) {
            let fragment = match dataset.shape(&shape_id).and_then(|s| s.fragment.clone()) {
                Some(path) => {
                    let fragment_path = resolve_catalog_sql_path(catalog, &path);
                    Some(
                        std::fs::read_to_string(&fragment_path)
                            .with_context(|| format!("read fragment {path}"))?,
                    )
                }
                None => None,
            };
            let composed = compose(
                dataset,
                &shape_id,
                order_by_id.as_deref(),
                &source,
                fragment.as_deref(),
            )?;
            let sql = composed.sql.trim().trim_end_matches(';').to_string();
            fineract_pool
                .prepare(AssertSqlSafe(sql).into_sql_str())
                .await
                .map_err(|err| {
                    anyhow::anyhow!(
                        "dataset {} shape {shape_id} order_by {order_by_id:?} failed prepare: {err}",
                        dataset.id
                    )
                })?;
        }
    }
    Ok(())
}
```

Add the shared path helper next to the existing `resolve_sql_path` in the same file, and reuse the same `queries/`-prefix convention:

```rust
/// Resolves a repo-relative SQL path declared in a dataset, using the same
/// convention as `resolve_sql_path` for queries.
fn resolve_catalog_sql_path(catalog: &KnowledgeCatalog, relative: &str) -> PathBuf {
    if relative.starts_with("queries/") {
        catalog
            .query_path
            .parent()
            .unwrap_or(&catalog.query_path)
            .join(relative)
    } else {
        catalog.query_path.join(relative)
    }
}
```

Add `pub datasets: Vec<DatasetKnowledge>` to `KnowledgeCatalog` in `crates/chat/src/knowledge/model.rs` (importing `crate::knowledge::dataset::model::DatasetKnowledge`), and populate it in `KnowledgeLoader::load` with:

```rust
let datasets = self.load_yaml_dir::<DatasetKnowledge>("datasets")?;
```

placed alongside the existing `load_yaml_dir` calls, and added to the `KnowledgeCatalog { .. }` construction. `knowledge/datasets/` does not exist yet, and `load_yaml_dir` already returns an empty vector for a missing directory, so this is a no-op until Phase B.

- [ ] **Step 6: Run the full chat test suite**

Run: `cargo test -p chat`
Expected: PASS. `validate_dataset_runtime` iterates an empty `datasets` vector, so no behaviour changes.

- [ ] **Step 7: Verify the app still boots and routes**

```bash
cargo run -p app
```

Expected: startup logs `knowledge catalog index already current` or `knowledge catalog synced`, then `AI Reporting Service is ready to accept requests`. Confirm no `failed prepare` errors. Stop the app.

- [ ] **Step 8: Commit**

```bash
cargo fmt -p chat
cargo clippy --workspace --all-targets -- -D warnings
git add crates/chat/src/knowledge/
git commit -m "feat(knowledge): prepare dataset shape/order-by cross product at startup"
```

---

### Task 7: Static dataset validation rules

Closes the spec's remaining validator requirements. These run at catalog load,
before any SQL is prepared, so an authored dataset in Phase B cannot reach the
database in a malformed state.

**Files:**
- Create: `crates/chat/src/knowledge/dataset/validate.rs`
- Modify: `crates/chat/src/knowledge/dataset/mod.rs`
- Modify: `crates/chat/src/knowledge/catalog/validator.rs` (`validate`, around line 46)

**Interfaces:**
- Consumes: `DatasetKnowledge` from Task 2, `validate_sql_expr` from Task 1.
- Produces: `pub fn validate_dataset(dataset: &DatasetKnowledge) -> anyhow::Result<()>`.

- [ ] **Step 1: Write the failing test**

Create `crates/chat/src/knowledge/dataset/validate.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::{
        RequestGrouping, RequestOperation, RequestOutput, RequestPii, RequestShape, RequestSubject,
    };
    use crate::knowledge::dataset::model::{
        DatasetOutputField, FilterOperator, FilterSlot, OrderByOption, ShapeOption,
    };
    use crate::knowledge::model::Sensitivity;

    fn valid() -> DatasetKnowledge {
        DatasetKnowledge {
            id: "savings.account_charges".into(),
            database: "fineract".into(),
            source_sql: "queries/savings/account_charges.source.sql".into(),
            tables: vec!["m_savings_account_charge".into()],
            filters: vec![FilterSlot {
                id: "due_date".into(),
                expr: "sac.charge_due_date".into(),
                kind: "date".into(),
                operators: vec![FilterOperator::Eq],
            }],
            shapes: vec![ShapeOption {
                id: "list".into(),
                request_shape: RequestShape {
                    operation: RequestOperation::List,
                    subject: RequestSubject::SavingsAccountCharge,
                    grouping: RequestGrouping::None,
                    output: RequestOutput::List,
                    pii: RequestPii::None,
                },
                fragment: Some("queries/savings/account_charges.list.frag.sql".into()),
                order_by: vec!["created_desc".into()],
            }],
            order_by: vec![OrderByOption {
                id: "created_desc".into(),
                expr: "sac.created_on_utc DESC".into(),
            }],
            output_fields: vec![DatasetOutputField {
                name: "savings_account_charge_id".into(),
                kind: "bigint".into(),
                sensitivity: Sensitivity::PublicBusiness,
                core: true,
            }],
            parameters: Vec::new(),
            timeout_ms: None,
        }
    }

    #[test]
    fn accepts_a_well_formed_dataset() {
        assert!(validate_dataset(&valid()).is_ok());
    }

    #[test]
    fn rejects_a_shape_referencing_an_undeclared_order_by() {
        let mut dataset = valid();
        dataset.shapes[0].order_by = vec!["nope".into()];
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("order_by"), "got: {error}");
    }

    #[test]
    fn rejects_a_dataset_with_no_core_output_field() {
        let mut dataset = valid();
        dataset.output_fields[0].core = false;
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("core"), "got: {error}");
    }

    #[test]
    fn rejects_duplicate_filter_and_shape_ids() {
        let mut dataset = valid();
        let duplicate = dataset.filters[0].clone();
        dataset.filters.push(duplicate);
        assert!(validate_dataset(&dataset).is_err());

        let mut dataset = valid();
        let duplicate = dataset.shapes[0].clone();
        dataset.shapes.push(duplicate);
        assert!(validate_dataset(&dataset).is_err());
    }

    #[test]
    fn rejects_expressions_that_fail_the_grammar() {
        let mut dataset = valid();
        dataset.filters[0].expr = "sac.id; DROP TABLE m_client".into();
        assert!(validate_dataset(&dataset).is_err());

        let mut dataset = valid();
        dataset.order_by[0].expr = "sac.id /* x */".into();
        assert!(validate_dataset(&dataset).is_err());
    }

    #[test]
    fn rejects_a_shape_without_a_fragment_when_the_dataset_declares_filters() {
        let mut dataset = valid();
        dataset.shapes[0].fragment = None;
        let error = validate_dataset(&dataset).unwrap_err().to_string();
        assert!(error.contains("fragment"), "got: {error}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib dataset::validate`
Expected: FAIL — compile error, `validate_dataset` not found.

- [ ] **Step 3: Write minimal implementation**

Prepend to `crates/chat/src/knowledge/dataset/validate.rs`:

```rust
//! Static dataset rules, enforced at catalog load before any SQL is prepared.

use std::collections::HashSet;

use anyhow::{Result, bail};

use crate::knowledge::dataset::grammar::validate_sql_expr;
use crate::knowledge::dataset::model::DatasetKnowledge;

pub fn validate_dataset(dataset: &DatasetKnowledge) -> Result<()> {
    let mut filter_ids = HashSet::new();
    for filter in &dataset.filters {
        if !filter_ids.insert(filter.id.as_str()) {
            bail!("dataset {} declares filter {} twice", dataset.id, filter.id);
        }
        if filter.operators.is_empty() {
            bail!(
                "dataset {} filter {} declares no operators",
                dataset.id,
                filter.id
            );
        }
        validate_sql_expr(&filter.expr)
            .map_err(|reason| anyhow::anyhow!("dataset {} filter {}: {reason}", dataset.id, filter.id))?;
    }

    let mut order_by_ids = HashSet::new();
    for option in &dataset.order_by {
        if !order_by_ids.insert(option.id.as_str()) {
            bail!("dataset {} declares order_by {} twice", dataset.id, option.id);
        }
        validate_sql_expr(&option.expr).map_err(|reason| {
            anyhow::anyhow!("dataset {} order_by {}: {reason}", dataset.id, option.id)
        })?;
    }

    if dataset.shapes.is_empty() {
        bail!("dataset {} declares no shapes", dataset.id);
    }
    let mut shape_ids = HashSet::new();
    for shape in &dataset.shapes {
        if !shape_ids.insert(shape.id.as_str()) {
            bail!("dataset {} declares shape {} twice", dataset.id, shape.id);
        }
        for reference in &shape.order_by {
            if !order_by_ids.contains(reference.as_str()) {
                bail!(
                    "dataset {} shape {} references undeclared order_by {reference}",
                    dataset.id,
                    shape.id
                );
            }
        }
        // A dataset with filter slots always composes a CTE, which needs a
        // fragment to select from it. Only a fully degenerate dataset may omit one.
        if shape.fragment.is_none() && !dataset.filters.is_empty() {
            bail!(
                "dataset {} shape {} must declare a fragment because the dataset declares filters",
                dataset.id,
                shape.id
            );
        }
    }

    if !dataset.output_fields.is_empty() && !dataset.output_fields.iter().any(|field| field.core) {
        bail!(
            "dataset {} declares no core output field, so projection would render an empty table",
            dataset.id
        );
    }

    Ok(())
}
```

Update `crates/chat/src/knowledge/dataset/mod.rs`:

```rust
pub mod compose;
pub mod grammar;
pub mod legacy;
pub mod model;
pub mod plan;
pub mod validate;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p chat --lib dataset::validate`
Expected: PASS — 6 tests.

- [ ] **Step 5: Call it from the catalog validator**

In `crates/chat/src/knowledge/catalog/validator.rs`, inside `KnowledgeValidator::validate` (around line 46), add before the final `Ok(())`:

```rust
for dataset in &catalog.datasets {
    crate::knowledge::dataset::validate::validate_dataset(dataset)?;
}
```

- [ ] **Step 6: Run the full suite and boot the app**

Run: `cargo test -p chat`
Expected: PASS. `catalog.datasets` is empty, so the loop is a no-op.

```bash
cargo run -p app
```
Expected: boots clean and reports ready. Stop the app.

- [ ] **Step 7: Commit**

```bash
cargo fmt -p chat
cargo clippy --workspace --all-targets -- -D warnings
git add crates/chat/src/knowledge/
git commit -m "feat(knowledge): validate dataset declarations at catalog load"
```

---

## Definition of Done

- [ ] `cargo test -p chat` passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes.
- [ ] `crates/chat/tests/dataset_equivalence.rs` proves all 32 approved capabilities compose to byte-identical SQL.
- [ ] `cargo run -p app` boots clean and answers `saya ingin tahu charge paling baru yg telah dibuat?` exactly as before this plan — nothing in this plan is wired into the request path.
- [ ] `knowledge/datasets/` remains absent; no authored dataset YAML is introduced.
- [ ] Static dataset rules (Task 7) reject undeclared `order_by` references, missing `core` fields, duplicate ids, and expressions failing the grammar.

## Out of Scope (Plan A2)

Switching execution to the dataset path; projection (`core ∪ requested`) in `presentation/builder.rs`; `filter_hints` / `column_hints` / `shape_hint` on `LlmGatewayExtraction`; resolver hint validation and the rejection matrix; `shape_score` over shape sets; removing `#[serde(default)]` from `AssistantIntent.intent`.
