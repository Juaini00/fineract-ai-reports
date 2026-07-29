//! Layer 1 (LLM Gateway) — extracts a structured, sanitized view of the user's
//! turn without touching SQL, catalog internals, or per-parameter defaults.
//! See `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md` §4.

pub mod schema;

pub use schema::{
    GatewayCandidate, GatewayEntity, LlmGatewayExtraction, QuantityHint, QuantityInferred,
    TemporalHint, TemporalInferred, TemporalRangeHint,
};
