# Classifier Semantic Gap Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix classifier so ambiguous prompts (like "customer savings activity this week" — where all top candidates score within ~0.02) resolve to a **clarification with numbered options + an Others escape hatch**, instead of an unhelpful `unsupported` verdict, and improve retrieval embeddings so semantic (not lexical) matches drive the ranking.

**Architecture:** Three layers change:
(a) enrich each capability's `retrieval_text` with human-readable `display_name`, `description`, `examples`, and domain concept synonyms so Voyage embeddings can match natural-language paraphrases;
(b) load classifier thresholds from a new `knowledge/policies/classification.yaml` policy file (versioned, tunable without recompile) instead of magic numbers in Rust;
(c) rewrite the ambiguity gate in `classify_from_candidates` to use **gap-based** logic (top1−top2 gap + absolute floor + always-append Others option) rather than the current fixed `< 0.55` cutoff.

**Tech Stack:** Rust 2024, sqlx, serde, `serde_yaml`, `voyageai` embeddings (existing), axum. All changes stay inside `crates/chat`. No new workspace deps.

## Global Constraints

- Runtime layer order stays `route → service → repository → database`; no sqlx in classifier code.
- No new workspace crates. Reuse existing loader/validator pattern under `knowledge/policies/`.
- Vector index rebuild pipelines already exist (both `POST /vector-index/rebuild` **and** startup rebuild via `CATALOG_SYNC_ON_STARTUP=true`); this plan does not add new sync mechanisms — only makes retrieval_text richer, so operators re-run existing rebuild after deploy.
- **Read-only Fineract contract** stays intact — classifier changes touch no Fineract SQL.
- YAML loading must not panic at boot — validation errors surface via `KnowledgeValidator::validate` as `Err(anyhow::Error)`.
- All existing 69 tests must stay green. New behavior needs new tests, not weakened old ones.
- No printed emojis in code or docs.

---

## File map

| File | Change | Responsibility |
|---|---|---|
| `crates/chat/src/knowledge/model.rs` | Modify | Add `display_name`, `description` to `CapabilityKnowledge`; add `ClassificationPolicy` struct; extend `KnowledgeCatalog` with `classification: ClassificationPolicy` |
| `crates/chat/src/knowledge/catalog/loader.rs` | Modify | Load `knowledge/policies/classification.yaml` |
| `crates/chat/src/knowledge/catalog/validator.rs` | Modify | Validate policy: `gap ∈ (0,1)`, `floor ∈ (0,1)`, `others_label` non-empty |
| `crates/chat/src/knowledge/retrieval.rs` | Modify | `build_capability_document` includes new fields + domain concept synonyms |
| `crates/chat/src/chat/classifier.rs` | Modify | Add `OTHER_OPTION_LABEL`; expose helper `others_option()` |
| `crates/chat/src/chat/service/job.rs` | Modify | `classify_from_candidates` uses `catalog.classification` policy; always appends Others option in every clarification path |
| `knowledge/policies/classification.yaml` | Create | Threshold policy (gap, floor, others label) |
| `knowledge/responses/clarification.yaml` | Modify | Add `others_label` template and `ambiguous_activity_report` template |
| `knowledge/capabilities/**/*.yaml` | Modify | Add `display_name` + `description` to the 11 approved capabilities (values already exist for some — this makes the schema uniform) |
| `docs/reporting-capabilities.md` | Modify | Document threshold policy + Others contract |
| `crates/chat/src/chat/classifier/tests.rs` | Modify | Update fixtures to include Others option, add gap-based scenarios |
| `crates/chat/tests/classification_semantic.rs` | Create | Integration test — ambiguous prompts return clarify+Others (no Voyage call: seeds a mock catalog) |
| `docs/superpowers/plans/2026-07-04-classifier-semantic-gap.md` | Create | This plan |

---

## Task 1: Extend catalog model with display_name, description, and classification policy

**Files:**
- Modify: `crates/chat/src/knowledge/model.rs`
- Test: `crates/chat/src/knowledge/tests.rs`

**Interfaces:**
- Consumes: existing `KnowledgeCatalog` struct
- Produces:
  - `CapabilityKnowledge.display_name: Option<String>` (deserialize as `#[serde(default)]`)
  - `CapabilityKnowledge.description: Option<String>` (deserialize as `#[serde(default)]`)
  - `ClassificationPolicy { min_gap: f32, min_floor: f32, others_label: String, others_key: String }`
  - `KnowledgeCatalog.classification: ClassificationPolicy` (required field, loaded from `policies/classification.yaml`)

- [ ] **Step 1: Write the failing test**

Append to `crates/chat/src/knowledge/tests.rs`:

```rust
#[test]
fn classification_policy_loaded_from_real_catalog() {
    let workspace_root = workspace_root();
    let catalog = KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");

    let policy = &catalog.classification;
    assert!(policy.min_gap > 0.0 && policy.min_gap < 1.0);
    assert!(policy.min_floor > 0.0 && policy.min_floor < 1.0);
    assert_eq!(policy.others_key, "other_activity");
    assert!(!policy.others_label.trim().is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib knowledge::tests::classification_policy_loaded_from_real_catalog`
Expected: FAIL with `no field 'classification' on KnowledgeCatalog` or missing YAML.

- [ ] **Step 3: Add fields to model**

Edit `crates/chat/src/knowledge/model.rs`. Locate `pub struct CapabilityKnowledge` (around line 93) and add before `#[serde(default)] pub data_areas`:

```rust
    #[serde(default)]
    pub display_name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,
```

At the bottom of the file (near other `pub struct` blocks) add:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct ClassificationPolicy {
    pub min_gap: f32,
    pub min_floor: f32,
    pub others_key: String,
    pub others_label: String,
}
```

Then locate `pub struct KnowledgeCatalog` and add a new field:

```rust
    pub classification: ClassificationPolicy,
```

- [ ] **Step 4: Add a temporary default so the workspace compiles until Task 3 wires the loader**

At the very bottom of `crates/chat/src/knowledge/model.rs` append:

```rust
impl Default for ClassificationPolicy {
    fn default() -> Self {
        Self {
            min_gap: 0.05,
            min_floor: 0.40,
            others_key: "other_activity".to_string(),
            others_label: "Something else — let me describe it differently".to_string(),
        }
    }
}
```

- [ ] **Step 5: Run compilation to confirm shape**

Run: `cargo check -p chat`
Expected: PASS. (Test will still fail until Task 3.)

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/knowledge/model.rs crates/chat/src/knowledge/tests.rs
git commit -m "feat(knowledge): add display_name, description, and ClassificationPolicy to model"
```

---

## Task 2: Create classification policy YAML

**Files:**
- Create: `knowledge/policies/classification.yaml`
- Modify: `knowledge/responses/clarification.yaml`

**Interfaces:**
- Consumes: nothing (data file)
- Produces: on-disk YAML matching `ClassificationPolicy` shape from Task 1.

- [ ] **Step 1: Write the classification policy file**

Create `knowledge/policies/classification.yaml`:

```yaml
id: classification_policy
source_doc: docs/reporting-capabilities.md#9-classification-thresholds
# Gap-based classifier decision.
#
# Given top-N capability retrieval scores sorted DESC by confidence:
#   * If top1.confidence < min_floor → outcome = unsupported (off-domain).
#   * Else if (top1.confidence - top2.confidence) >= min_gap → outcome = matched
#     with capability = top1.
#   * Else → outcome = clarification_required with options = top-N mapped to
#     capability + `others_key` at the end. User picks the number OR "Others"
#     to describe a different intent.
min_gap: 0.05
min_floor: 0.40
others_key: other_activity
others_label: "Something else — let me describe it differently"
checks:
  - id: gap_in_open_interval
    rule: min_gap must be in the open interval (0, 1).
  - id: floor_in_open_interval
    rule: min_floor must be in the open interval (0, 1).
  - id: others_key_non_empty
    rule: others_key must be non-empty and not collide with any capability id.
  - id: others_label_non_empty
    rule: others_label must be non-empty.
```

- [ ] **Step 2: Extend clarification response templates**

Edit `knowledge/responses/clarification.yaml`. Under `templates:` add two new keys (keep existing ones):

```yaml
  ambiguous_activity_report: "Which report would you like? Reply with the option number, or choose Others to describe it in your own words."
  others_prompt: "Please describe what you would like to know."
```

- [ ] **Step 3: Verify YAML parses**

Run: `cargo test -p chat --lib knowledge::tests::load_and_validate_project_knowledge_catalog`
Expected: PASS (the loader will silently ignore the new YAML until Task 3 wires it).

- [ ] **Step 4: Commit**

```bash
git add knowledge/policies/classification.yaml knowledge/responses/clarification.yaml
git commit -m "feat(knowledge): add classification thresholds policy and Others templates"
```

---

## Task 3: Wire the policy file into KnowledgeLoader + Validator

**Files:**
- Modify: `crates/chat/src/knowledge/catalog/loader.rs`
- Modify: `crates/chat/src/knowledge/catalog/validator.rs`
- Test: `crates/chat/src/knowledge/catalog/validator/tests.rs` (or existing `validator.rs` inline tests)

**Interfaces:**
- Consumes: `ClassificationPolicy` (Task 1), `knowledge/policies/classification.yaml` (Task 2)
- Produces: `KnowledgeLoader::load()` returns catalog with populated `classification`. Missing file → error, not silent default.

- [ ] **Step 1: Write the failing test**

Add to `crates/chat/src/knowledge/catalog/validator.rs` `mod tests`:

```rust
#[test]
fn rejects_out_of_range_gap() {
    let policy = crate::knowledge::model::ClassificationPolicy {
        min_gap: 0.0,
        min_floor: 0.4,
        others_key: "other_activity".to_string(),
        others_label: "Others".to_string(),
    };
    let error = validate_classification_policy(&policy)
        .expect_err("gap 0.0 must fail");
    assert!(error.to_string().contains("min_gap"));
}

#[test]
fn rejects_others_key_colliding_with_capability_id() {
    let policy = crate::knowledge::model::ClassificationPolicy {
        min_gap: 0.05,
        min_floor: 0.4,
        others_key: "savings_deposit_total".to_string(),
        others_label: "Others".to_string(),
    };
    let capability_ids = vec!["savings_deposit_total".to_string()];
    let error = validate_classification_policy_against_catalog(&policy, &capability_ids)
        .expect_err("colliding others_key must fail");
    assert!(error.to_string().contains("others_key"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p chat --lib knowledge::catalog::validator::tests::rejects_out_of_range_gap`
Expected: FAIL with `validate_classification_policy` not found.

- [ ] **Step 3: Implement loader change**

Edit `crates/chat/src/knowledge/catalog/loader.rs`. Locate the `load()` method that constructs `KnowledgeCatalog`. Before returning:

```rust
let classification = self.load_yaml::<crate::knowledge::model::ClassificationPolicy>(
    self.knowledge_root
        .join("policies")
        .join("classification.yaml"),
)?;
```

Add `classification` to the `KnowledgeCatalog { ... }` construction. If `load_yaml` doesn't exist with that shape, use the existing helper that returns `Result<T>` (grep for `serde_yaml::from_str` in loader.rs and mirror the pattern).

- [ ] **Step 4: Implement validator helpers**

Edit `crates/chat/src/knowledge/catalog/validator.rs`. Above `mod tests` add:

```rust
pub(crate) fn validate_classification_policy(
    policy: &crate::knowledge::model::ClassificationPolicy,
) -> anyhow::Result<()> {
    if !(policy.min_gap > 0.0 && policy.min_gap < 1.0) {
        anyhow::bail!("classification_policy.min_gap must be in (0, 1); got {}", policy.min_gap);
    }
    if !(policy.min_floor > 0.0 && policy.min_floor < 1.0) {
        anyhow::bail!("classification_policy.min_floor must be in (0, 1); got {}", policy.min_floor);
    }
    if policy.others_key.trim().is_empty() {
        anyhow::bail!("classification_policy.others_key must be non-empty");
    }
    if policy.others_label.trim().is_empty() {
        anyhow::bail!("classification_policy.others_label must be non-empty");
    }
    Ok(())
}

pub(crate) fn validate_classification_policy_against_catalog(
    policy: &crate::knowledge::model::ClassificationPolicy,
    capability_ids: &[String],
) -> anyhow::Result<()> {
    validate_classification_policy(policy)?;
    if capability_ids.iter().any(|id| id == &policy.others_key) {
        anyhow::bail!(
            "classification_policy.others_key '{}' must not collide with any capability id",
            policy.others_key
        );
    }
    Ok(())
}
```

Then inside `KnowledgeValidator::validate`, after existing checks add:

```rust
let capability_ids: Vec<String> = catalog
    .capabilities
    .iter()
    .map(|c| c.id.clone())
    .collect();
validate_classification_policy_against_catalog(&catalog.classification, &capability_ids)?;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p chat --lib`
Expected: all pass, including the two new validator tests and `classification_policy_loaded_from_real_catalog` from Task 1.

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/knowledge/catalog/loader.rs crates/chat/src/knowledge/catalog/validator.rs
git commit -m "feat(knowledge): load and validate classification policy at startup"
```

---

## Task 4: Enrich capability retrieval_text with description, examples, domain synonyms

**Files:**
- Modify: `crates/chat/src/knowledge/retrieval.rs`
- Modify: `knowledge/capabilities/savings/deposit_total.yaml` (add `display_name`, `description` if not present — pattern for others)
- Test: `crates/chat/src/knowledge/tests.rs`

**Interfaces:**
- Consumes: `CapabilityKnowledge.display_name`, `.description`, `.examples` (Task 1); `DomainKnowledge.concepts` (already exists)
- Produces: richer `RetrievalDocument.retrieval_text` for each capability. Vector index rebuild picks up new text via existing `content_hash` diff.

- [ ] **Step 1: Write the failing test**

Add to `crates/chat/src/knowledge/tests.rs`:

```rust
#[test]
fn capability_document_includes_description_and_examples() {
    let workspace_root = workspace_root();
    let catalog = crate::knowledge::catalog::loader::KnowledgeLoader::new(
        workspace_root.join("knowledge"),
        workspace_root.join("queries"),
    )
    .load()
    .expect("load knowledge catalog");

    let documents = crate::knowledge::retrieval::RetrievalDocumentBuilder::build(&catalog);
    let doc = documents
        .iter()
        .find(|d| d.source_id == "savings_deposit_total")
        .expect("savings_deposit_total document exists");

    // Description was added in Task 5's YAML edits.
    assert!(
        doc.retrieval_text
            .to_lowercase()
            .contains("total savings deposits"),
        "retrieval_text missing description: {}",
        doc.retrieval_text
    );
    // Examples flatten into retrieval text.
    assert!(
        doc.retrieval_text.contains("total deposit this month"),
        "retrieval_text missing example: {}",
        doc.retrieval_text
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p chat --lib capability_document_includes_description_and_examples`
Expected: FAIL — retrieval_text does not include the description phrasing.

- [ ] **Step 3: Update `build_capability_document`**

Edit `crates/chat/src/knowledge/retrieval.rs`. Replace the existing `build_capability_document` body (currently the block at line 185+) with a version that includes display_name, description, and — critically — flattens the domain's concept synonyms so paraphrases like "customer savings activity" retrieve savings capabilities.

Below is the exact replacement. Keep the surrounding function signature the same:

```rust
fn build_capability_document(
    capability: &CapabilityKnowledge,
    domain: Option<&DomainKnowledge>,
) -> RetrievalDocument {
    let title = format!("Capability {}", capability.id);
    let concept_synonyms = domain
        .map(|d| {
            d.concepts
                .iter()
                .flat_map(|c| c.synonyms.iter().cloned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let retrieval_text = compact_lines([
        format!("Capability {}", capability.id),
        format!(
            "Display name {}",
            capability.display_name.as_deref().unwrap_or(&capability.id)
        ),
        capability
            .description
            .as_deref()
            .unwrap_or("")
            .to_string(),
        format!("Status {}", capability.status),
        format!("Domain {}", capability.domain),
        format!("Query {}", capability.query_id),
        optional_list("Data areas", &capability.data_areas),
        optional_list("Metrics", &capability.metrics),
        optional_list("Examples", &capability.examples),
        optional_list("Domain concepts", &concept_synonyms),
        optional_list("Required parameters", &capability.required_parameters),
        optional_list("Optional parameters", &capability.optional_parameters),
    ]);

    RetrievalDocument {
        source_type: RetrievalSourceType::Capability,
        source_id: capability.id.clone(),
        title,
        retrieval_text,
        metadata_json: json!({
            "status": capability.status,
            "display_name": capability.display_name,
            "description": capability.description,
            "domain": capability.domain,
            "query_id": capability.query_id,
            "output_mode": capability.output_mode,
            "data_areas": capability.data_areas,
            "metrics": capability.metrics,
            "examples": capability.examples,
            "required_parameters": capability.required_parameters,
            "optional_parameters": capability.optional_parameters,
        }),
    }
}
```

Then update the caller at the top of `retrieval.rs` (`RetrievalDocumentBuilder::build`) to look up the domain per capability. Replace:

```rust
documents.extend(catalog.capabilities.iter().map(build_capability_document));
```

with:

```rust
documents.extend(catalog.capabilities.iter().map(|capability| {
    let domain = catalog
        .domains
        .iter()
        .find(|d| d.id == capability.domain);
    build_capability_document(capability, domain)
}));
```

- [ ] **Step 4: Add display_name and description to real YAML**

For each of the 11 approved capabilities that currently lacks these fields, edit the YAML to add them near the top. Example for `knowledge/capabilities/savings/deposit_total.yaml` (values already present per current file):

```yaml
display_name: Savings Deposit Total
description: Total savings deposits aggregated over a date range for the caller's authorized offices.
```

Do this uniformly for every file under `knowledge/capabilities/**/*.yaml`. Use the concept's plain-English description; the exact wording is not tested, only presence.

Reference wording (write these into the YAMLs; each must be one line):

| File | display_name | description |
|---|---|---|
| `savings/balance_summary.yaml` | Savings Balance Summary | Snapshot of active savings portfolio: account count, total balance, average balance, max balance. |
| `savings/deposit_total.yaml` | Savings Deposit Total | Total savings deposits aggregated over a date range for the caller's authorized offices. |
| `savings/deposit_top_n.yaml` | Top Savings Deposits | Largest N deposit transactions for a date range, one row per transaction. |
| `savings/deposit_monthly_breakdown.yaml` | Savings Deposits by Month | Monthly aggregate deposit totals across a date range. |
| `savings/deposit_monthly_top_n.yaml` | Top Savings Deposits per Month | Largest N deposits within each month across the date range. |
| `savings/withdrawal_total.yaml` | Savings Withdrawal Total | Total savings withdrawals aggregated over a date range. |
| `savings/withdrawal_top_n.yaml` | Top Savings Withdrawals | Largest N withdrawal transactions for a date range. |
| `savings/withdrawal_monthly_breakdown.yaml` | Savings Withdrawals by Month | Monthly aggregate withdrawal totals across a date range. |
| `savings/withdrawal_monthly_top_n.yaml` | Top Savings Withdrawals per Month | Largest N withdrawals within each month across the date range. |
| `organization/office_summary.yaml` | Office Summary | Office directory snapshot with active staff counts per office. |
| `client/lifecycle_summary.yaml` | Client Lifecycle Summary | Aggregate active vs closed client counts per office. |

- [ ] **Step 5: Run tests**

Run: `cargo test -p chat --lib capability_document_includes_description_and_examples`
Expected: PASS.

Run: `cargo test -p chat --lib`
Expected: all existing unit tests still pass. `retrieval_documents_cover_all_capabilities` (in tests.rs) already tolerates content changes.

- [ ] **Step 6: Commit**

```bash
git add crates/chat/src/knowledge/retrieval.rs knowledge/capabilities/
git commit -m "feat(knowledge): include display_name, description, and domain synonyms in capability retrieval text"
```

---

## Task 5: Rewrite classify_from_candidates with gap-based logic + Others option

**Files:**
- Modify: `crates/chat/src/chat/service/job.rs` (function `classify_from_candidates` around line 420)
- Modify: `crates/chat/src/chat/classifier.rs` (add `others_option` constructor)
- Test: `crates/chat/src/chat/classifier/tests.rs`

**Interfaces:**
- Consumes: `catalog.classification: ClassificationPolicy` (Task 3), `RetrievedKnowledgeCandidate`
- Produces: `ClassificationResult` where clarification `options` **always** ends with an `OTHER_ACTIVITY_CAPABILITY` entry labeled from `policy.others_label`, and the ambiguity decision uses gap+floor logic.

- [ ] **Step 1: Write the failing test**

Append to `crates/chat/src/chat/classifier/tests.rs`:

```rust
#[test]
fn others_option_appended_to_clarification_options() {
    let options = vec![ClarificationOption {
        label: "Total deposits".into(),
        capability: "savings_deposit_total".into(),
        output_mode: Some("total".into()),
    }];
    let with_others = append_others_option(options, "Something else");
    assert_eq!(with_others.len(), 2);
    assert_eq!(with_others.last().unwrap().capability, OTHER_ACTIVITY_CAPABILITY);
    assert_eq!(with_others.last().unwrap().label, "Something else");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p chat --lib classifier::tests::others_option_appended_to_clarification_options`
Expected: FAIL — `append_others_option` not defined.

- [ ] **Step 3: Add helper to classifier.rs**

Edit `crates/chat/src/chat/classifier.rs`. Near the top, next to `pub const OTHER_ACTIVITY_CAPABILITY`, add:

```rust
pub fn append_others_option(
    mut options: Vec<ClarificationOption>,
    others_label: &str,
) -> Vec<ClarificationOption> {
    let already_present = options
        .iter()
        .any(|opt| opt.capability == OTHER_ACTIVITY_CAPABILITY);
    if !already_present {
        options.push(ClarificationOption {
            label: others_label.to_string(),
            capability: OTHER_ACTIVITY_CAPABILITY.to_string(),
            output_mode: None,
        });
    }
    options
}
```

Run: `cargo test -p chat --lib classifier::tests::others_option_appended_to_clarification_options`
Expected: PASS.

- [ ] **Step 4: Write the failing gap-based test**

Append to `crates/chat/src/chat/classifier/tests.rs`:

```rust
#[test]
fn ambiguous_scores_yield_clarification_with_others() {
    use crate::knowledge::model::ClassificationPolicy;
    let policy = ClassificationPolicy {
        min_gap: 0.05,
        min_floor: 0.40,
        others_key: OTHER_ACTIVITY_CAPABILITY.to_string(),
        others_label: "Something else".to_string(),
    };
    // Two candidates within the gap and above the floor → clarify.
    let outcome = decide_from_scores(&policy, &[0.49, 0.47], &["a", "b"]);
    assert_eq!(outcome, DecideOutcome::Clarify);

    // Wide gap above floor → match top.
    let outcome = decide_from_scores(&policy, &[0.60, 0.40], &["a", "b"]);
    assert_eq!(outcome, DecideOutcome::Match { capability: "a".into() });

    // Top below floor → unsupported.
    let outcome = decide_from_scores(&policy, &[0.30, 0.20], &["a", "b"]);
    assert_eq!(outcome, DecideOutcome::Unsupported);
}
```

- [ ] **Step 5: Run to verify it fails**

Run: `cargo test -p chat --lib classifier::tests::ambiguous_scores_yield_clarification_with_others`
Expected: FAIL — `decide_from_scores` and `DecideOutcome` not defined.

- [ ] **Step 6: Add the pure decision function**

Edit `crates/chat/src/chat/classifier.rs`. Below `append_others_option`, add:

```rust
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DecideOutcome {
    Match { capability: String },
    Clarify,
    Unsupported,
}

pub fn decide_from_scores(
    policy: &crate::knowledge::model::ClassificationPolicy,
    sorted_scores: &[f32],
    sorted_capabilities: &[&str],
) -> DecideOutcome {
    let top = sorted_scores.first().copied().unwrap_or(0.0);
    if top < policy.min_floor {
        return DecideOutcome::Unsupported;
    }
    let second = sorted_scores.get(1).copied().unwrap_or(0.0);
    let gap = top - second;
    if gap >= policy.min_gap {
        if let Some(cap) = sorted_capabilities.first() {
            return DecideOutcome::Match {
                capability: (*cap).to_string(),
            };
        }
    }
    DecideOutcome::Clarify
}
```

Run: `cargo test -p chat --lib classifier::tests::ambiguous_scores_yield_clarification_with_others`
Expected: PASS.

- [ ] **Step 7: Wire policy into `classify_from_candidates` in job.rs**

Edit `crates/chat/src/chat/service/job.rs`. Locate `fn classify_from_candidates` (around line 420). Replace the body:

```rust
fn classify_from_candidates(
    &self,
    message: &str,
    today: chrono::NaiveDate,
    allowed_capabilities: &[String],
    candidates: &[RetrievedKnowledgeCandidate],
) -> Option<ClassificationResult> {
    use crate::chat::classifier::{
        append_others_option, decide_from_scores, DecideOutcome,
    };

    let top = candidates.first()?;
    let top_capability = self.catalog_capability_for_candidate(top)?;

    let classification_candidates = candidates
        .iter()
        .filter_map(|candidate| {
            self.catalog_capability_for_candidate(candidate)
                .map(|capability| (candidate, capability))
        })
        .map(|(candidate, capability)| ClassificationCandidate {
            capability: capability.id.clone(),
            confidence: vector_confidence(candidate.distance),
            source_type: Some(candidate.source_type.clone()),
        })
        .collect::<Vec<_>>();

    let sorted_scores: Vec<f32> = classification_candidates
        .iter()
        .map(|c| c.confidence)
        .collect();
    let sorted_ids: Vec<&str> = classification_candidates
        .iter()
        .map(|c| c.capability.as_str())
        .collect();

    let policy = &self.catalog.classification;
    let outcome = decide_from_scores(policy, &sorted_scores, &sorted_ids);

    match outcome {
        DecideOutcome::Unsupported => None,
        DecideOutcome::Match { capability } => {
            let capability = self.catalog_capability(&capability)?;
            let confidence = sorted_scores.first().copied().unwrap_or(0.0);
            Some(classify_retrieved_capability(
                message,
                today,
                &capability.domain,
                &capability.id,
                &capability.output_mode,
                confidence,
                classification_candidates,
            ))
        }
        DecideOutcome::Clarify => {
            let close_capabilities = candidates
                .iter()
                .filter_map(|candidate| self.catalog_capability_for_candidate(candidate))
                .collect::<Vec<_>>();
            let mut options = if is_activity_request(message) {
                self.activity_options(message, &top_capability.domain, allowed_capabilities)
            } else {
                close_capabilities
                    .into_iter()
                    .map(|capability| capability_option(capability, message))
                    .collect::<Vec<_>>()
            };
            options = append_others_option(options, &policy.others_label);
            let confidence = sorted_scores.first().copied().unwrap_or(0.0);
            Some(clarify_retrieved_capabilities(
                message,
                today,
                Some(top_capability.domain.clone()),
                options,
                confidence,
                classification_candidates,
            ))
        }
    }
}
```

- [ ] **Step 8: Update classify_clarification_response to route Others answers**

Edit `crates/chat/src/chat/classifier.rs`. Locate `classify_clarification_response`. The existing check `if option.capability == OTHER_ACTIVITY_CAPABILITY` must ask the user to describe their intent instead of returning `Unsupported`. Change the branch (around line 54) to:

```rust
if option.capability == OTHER_ACTIVITY_CAPABILITY {
    return ClassificationResult {
        outcome: ClassificationOutcome::ClarificationRequired,
        domain: original.domain.clone(),
        capability: None,
        confidence: 0.6,
        params: original.params.clone(),
        clarification: Some(
            "Please describe what you would like to know.".to_string(),
        ),
        options: Vec::new(),
        source: Some("clarification_other_selected".to_string()),
        candidates: original.candidates.clone(),
    };
}
```

- [ ] **Step 9: Update the existing test that expected `Unsupported` for Others**

Existing test in `classifier/tests.rs`:

```rust
#[test]
fn classifies_numeric_clarification_option() {
    // ...currently expects Matched savings_deposit_top_n for input "2"
}
```

Add a new one alongside it:

```rust
#[test]
fn selecting_others_yields_clarification_prompting_user_to_describe() {
    let original = clarify_retrieved_capabilities(
        "Something",
        today(),
        Some("savings".to_string()),
        append_others_option(
            vec![ClarificationOption {
                label: "Total deposits".to_string(),
                capability: "savings_deposit_total".to_string(),
                output_mode: Some("total".to_string()),
            }],
            "Others",
        ),
        0.5,
        Vec::new(),
    );

    let result = classify_clarification_response(&original, "Others");
    assert_eq!(result.outcome, ClassificationOutcome::ClarificationRequired);
    assert_eq!(result.source.as_deref(), Some("clarification_other_selected"));
}
```

- [ ] **Step 10: Run full unit test suite**

Run: `cargo test -p chat --lib`
Expected: PASS. Fix any failures that come from the old absolute-threshold behavior — those tests should be **updated** (not deleted) to reflect the new gap-based decision. For each failing test, adjust the fixture scores to be either widely spread (Match) or tightly clustered (Clarify) as intended by the test's original name.

- [ ] **Step 11: Commit**

```bash
git add crates/chat/src/chat/classifier.rs crates/chat/src/chat/service/job.rs crates/chat/src/chat/classifier/tests.rs
git commit -m "feat(chat): gap-based classifier decision with Others escape hatch"
```

---

## Task 6: Integration test — ambiguous prompt returns clarify + Others

**Files:**
- Create: `crates/chat/tests/classification_semantic.rs`

**Interfaces:**
- Consumes: existing `common::spawn_app()`, real Postgres, real classification policy YAML.
- Produces: end-to-end assertion that an ambiguous prompt does **not** end as `unsupported` with empty options — instead, the job's `state_json.classification.options` includes at least one capability + Others.

Since integration tests avoid Voyage, the ambiguous case is triggered via the catalog-lexical fallback path (Voyage embed will fail on the empty `VOYAGEAI_API_KEY`, falling to `catalog_lexical_candidates`). That path also runs `classify_from_candidates` and therefore exercises the new policy.

- [ ] **Step 1: Write the test file**

Create `crates/chat/tests/classification_semantic.rs`:

```rust
//! Verifies the gap-based classifier decision and Others escape hatch. Uses
//! the catalog-lexical fallback (Voyage disabled in the harness), so all
//! assertions are deterministic without external services.

mod common;

use common::{TestApp, spawn_app};
use serde_json::{Value, json};
use std::time::{Duration, Instant};

const POLL_TIMEOUT: Duration = Duration::from_secs(15);
const POLL_INTERVAL: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
async fn ambiguous_prompt_produces_options_including_others() {
    let app = spawn_app().await;
    let key = app
        .provision_api_key(
            &["savings_deposit_total", "savings_deposit_top_n"],
            vec![1, 2, 3],
            false,
        )
        .await;

    let job = create_job(&app, &key.raw, "Show customer savings activity this week").await;
    let terminal = wait_for_terminal(&app, &key.raw, &job).await;

    let outcome = terminal["state_json"]["classification"]["outcome"]
        .as_str()
        .unwrap_or("");
    assert_ne!(
        outcome, "unsupported",
        "ambiguous prompt should not be unsupported: {terminal}"
    );

    let options = terminal["state_json"]["classification"]["options"]
        .as_array()
        .expect("options is array");
    assert!(
        !options.is_empty(),
        "clarification should present at least one option: {terminal}"
    );
    let has_others = options
        .iter()
        .any(|opt| opt["capability"] == "other_activity");
    assert!(has_others, "Others option missing: {options:?}");
}

async fn create_job(app: &TestApp, api_key: &str, message: &str) -> Value {
    let resp = app
        .post_json("/chat/jobs", Some(api_key), &json!({ "message": message }))
        .await;
    assert_eq!(
        resp.status(),
        201,
        "create_job failed: {}",
        resp.text().await.unwrap_or_default()
    );
    let body: Value = resp.json().await.unwrap();
    body["data"].clone()
}

async fn wait_for_terminal(app: &TestApp, api_key: &str, initial: &Value) -> Value {
    let job_id = initial["job_id"].as_str().unwrap();
    let deadline = Instant::now() + POLL_TIMEOUT;
    loop {
        let resp = app
            .get(&format!("/chat/jobs/{job_id}"), Some(api_key))
            .await;
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.unwrap();
        let status = body["data"]["status"].as_str().unwrap_or("").to_string();
        if !matches!(status.as_str(), "queued" | "running") {
            return body["data"].clone();
        }
        if Instant::now() >= deadline {
            panic!("job did not reach terminal state: {body}");
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
```

- [ ] **Step 2: Run the integration test**

Run: `cargo test -p chat --test classification_semantic`
Expected: PASS. If the deprecated behavior was matching this specific string, the test flags exactly that regression.

- [ ] **Step 3: Commit**

```bash
git add crates/chat/tests/classification_semantic.rs
git commit -m "test(chat): integration test asserting ambiguous prompts clarify with Others"
```

---

## Task 7: Documentation + operator note

**Files:**
- Modify: `docs/reporting-capabilities.md` (append §9)
- Modify: `docs/scenarios/06-chat-clarification-and-unsupported.md`

**Interfaces:**
- Consumes: nothing (docs)
- Produces: written contract explaining the gap-based thresholds and the Others option.

- [ ] **Step 1: Append to `docs/reporting-capabilities.md`**

Append at the end of the file:

```markdown
## 9. Classification thresholds

Loaded from `knowledge/policies/classification.yaml` at boot. Any change requires a vector-index rebuild — either via `POST /vector-index/rebuild` **or** setting `CATALOG_SYNC_ON_STARTUP=true` and restarting the app.

- `min_gap`: minimum confidence gap between the top and second candidate for a **matched** outcome. Below this, classifier clarifies with the top-N candidates.
- `min_floor`: minimum absolute confidence for the top candidate. Below this, classifier returns **unsupported** (off-domain).
- `others_key`: reserved capability id (`other_activity`) that must not collide with any approved capability. Selecting the Others option in a clarification asks the user to describe their intent in their own words.
- `others_label`: the human-readable label shown as the last option in every clarification list.

Tuning tips: raise `min_gap` (e.g., 0.08) if too many prompts match when they should clarify; raise `min_floor` (e.g., 0.45) if too many off-domain prompts squeak through as matches.
```

- [ ] **Step 2: Update scenario 06**

In `docs/scenarios/06-chat-clarification-and-unsupported.md` locate the section that shows the expected `options` array. Add:

```markdown
Every clarification MUST include an `Others` entry with `capability: "other_activity"` as its last option. Selecting Others (either by number or by typing "Others") produces a follow-up `ClarificationRequired` result with `source: "clarification_other_selected"` prompting the user to describe their intent freely — it does NOT terminate the job as `unsupported`.
```

- [ ] **Step 3: Commit**

```bash
git add docs/reporting-capabilities.md docs/scenarios/06-chat-clarification-and-unsupported.md
git commit -m "docs: describe gap-based classification thresholds and Others contract"
```

---

## Task 8: Deploy checklist and vector index rebuild note

**Files:**
- Modify: `AGENTS.md` (append a "Post-deploy" note if the file exists; otherwise skip this task)

**Interfaces:**
- Consumes: nothing
- Produces: operator instruction that retrieval_text changes require an embedding rebuild.

- [ ] **Step 1: Append operator note**

Check if `AGENTS.md` exists at repo root. If it does, append near the top:

```markdown
## Post-classifier-change deployment

Any change to a capability's `display_name`, `description`, or the retrieval text template must be followed by:

1. **Preferred:** operator runs `POST /vector-index/rebuild` after deploy. Existing embeddings are replaced atomically.
2. **Alternative:** set `CATALOG_SYNC_ON_STARTUP=true` in the deployment env and roll the pod. The app compares `content_hash` and rebuilds automatically if changed.

Never both at once during a single deploy — pick one.
```

If `AGENTS.md` does not exist, skip this task and mark it complete.

- [ ] **Step 2: Verify or skip**

Run: `ls AGENTS.md 2>&1`
- If found: proceed to commit.
- If not found: `git status` should show no changes; move to next task.

- [ ] **Step 3: Commit (if changed)**

```bash
git add AGENTS.md
git commit -m "docs(agents): rebuild retrieval index after classifier retrieval-text changes"
```

---

## Task 9: Final green run and manual smoke

**Files:**
- None (verification only)

**Interfaces:**
- Consumes: everything from Tasks 1–8.
- Produces: signed-off green build.

- [ ] **Step 1: Full workspace check**

Run: `cargo fmt --all -- --check`
Expected: PASS.

Run: `cargo clippy --workspace --all-targets --locked -- -D warnings`
Expected: PASS.

Run: `cargo test --workspace --locked`
Expected: PASS — all previously-passing tests plus the new tests from Tasks 1, 3, 4, 5, 6.

- [ ] **Step 2: Confirm capability counts unchanged**

Run: `cargo test -p chat --test catalog_validation`
Expected: PASS — asserts `data_areas=13, domains=7, capabilities=11, queries=11`. Task 4 added description fields but not new rows.

- [ ] **Step 3: Optional local smoke — rebuild vector index**

If Voyage credentials are set locally (`VOYAGEAI_API_KEY` in `.env`), start the app and hit:

```bash
curl -X POST http://127.0.0.1:3007/vector-index/rebuild \
  -H "Authorization: Bearer $API_KEY"
```

Verify `data.document_count > 0` and no error. This confirms embeddings pick up the new retrieval_text.

If no Voyage creds — skip. CI + integration tests already covered the deterministic path.

- [ ] **Step 4: Final commit and PR**

If any docs / lint fixes accumulated, commit them:

```bash
git add -A
git status
```

Ensure the tree is clean (no uncommitted changes), then push the branch and open a PR titled: `feat(chat): gap-based classifier with Others escape hatch and richer retrieval text`.

---

## Self-Review

**Spec coverage:** Every diagnosis point from the ambiguous JSON is addressed —
- semantic understanding (retrieval_text enrichment) → Task 4
- scoring gap logic → Task 5
- Others option always present → Task 5 (helper) + Task 6 (integration guarantee) + Task 7 (documented contract)
- policy externalization → Tasks 1 + 2 + 3

**Placeholder scan:** No "TBD" / "similar to" / "handle appropriately" instructions. Every code block is complete and self-contained.

**Type consistency:** `ClassificationPolicy` fields (`min_gap`, `min_floor`, `others_key`, `others_label`) named the same in Tasks 1, 2, 3, 5. `DecideOutcome::{Match, Clarify, Unsupported}` and `decide_from_scores(policy, sorted_scores, sorted_capabilities)` signatures identical in Tasks 5 (impl) and 5 (tests). `append_others_option(options, others_label)` signature matches call site in `classify_from_candidates`. `OTHER_ACTIVITY_CAPABILITY` constant reused (unchanged from current code).
