# Capability Coverage Matrix: Contract test policy

Source: `docs-old/capability-coverage-matrix.md`

## Contract test policy

Every row in the matrix must have at least one contract-test entry in `crates/chat/tests/fixtures/prompts.yaml` — one prompt per row × column cell that is `implemented`, plus one prompt per row that is `planned` (asserting `planned_unimplemented`), plus one prompt per `deferred` row (asserting `unsupported/deferred_domain`).

> Follow-up implementation task (not a doc task): the fixtures file `crates/chat/tests/fixtures/prompts.yaml` does not exist yet. Track its creation as a Rust-side milestone.
