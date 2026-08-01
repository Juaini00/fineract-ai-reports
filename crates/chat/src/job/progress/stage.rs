//! The closed set of pipeline stages a client may be shown.
//!
//! Labels are fixed server-side constants. User text, SQL, prompt content and
//! row data must never reach a progress event.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Routing,
    Retrieval,
    Reranking,
    Policy,
    Execution,
    Formatting,
}

impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Routing => "routing",
            Self::Retrieval => "retrieval",
            Self::Reranking => "reranking",
            Self::Policy => "policy",
            Self::Execution => "execution",
            Self::Formatting => "formatting",
        }
    }

    /// English fallback label. The client is expected to localise from
    /// `as_str()`; this exists so a bare client still shows something useful.
    pub fn label(self) -> &'static str {
        match self {
            Self::Routing => "Understanding the request",
            Self::Retrieval => "Finding a matching report",
            Self::Reranking => "Choosing the best match",
            Self::Policy => "Checking access permissions",
            Self::Execution => "Running the query",
            Self::Formatting => "Composing the answer",
        }
    }
}
