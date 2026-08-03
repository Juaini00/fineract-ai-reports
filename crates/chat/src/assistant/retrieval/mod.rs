pub mod engine;
pub mod evidence;
pub mod reranker;
pub mod sufficiency;

pub use engine::{RetrievalEngine, catalog_fallback, compatible_ids, shape_score};
pub use sufficiency::{capability_honours, drop_insufficient, expressed_filters};
