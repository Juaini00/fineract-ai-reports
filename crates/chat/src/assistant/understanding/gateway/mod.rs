//! Layer 1 (LLM Gateway) — extracts a structured, sanitized view of the user's
//! turn without touching SQL, catalog internals, or per-parameter defaults.
//! See `docs/superpowers/specs/2026-07-24-llm-extraction-gateway-design.md` §4.

pub mod client;
pub mod prompt;
pub mod schema;

pub use client::{GatewayClient, GatewayError};
pub use prompt::{CapabilitySummary, build_gateway_prompt, capability_summary};
pub use schema::{
    GatewayCandidate, GatewayEntity, LlmGatewayExtraction, QuantityHint, QuantityInferred,
    TemporalHint, TemporalInferred, TemporalRangeHint,
};
