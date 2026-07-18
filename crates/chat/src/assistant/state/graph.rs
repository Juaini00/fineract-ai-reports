use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use petgraph::Graph;
use petgraph::graph::NodeIndex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GraphState {
    ReceiveMessage,
    BuildContextWindow,
    RouteIntent,
    ResolveClarification,
    PlanRetrieval,
    RetrieveKnowledge,
    EvaluateEvidence,
    PlanToolOrCapability,
    GuardExecution,
    ExecuteToolOrSql,
    BuildStructuredResponse,
    RenderResponse,
    CompleteOrWait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalState {
    Completed,
    WaitingForUserInput,
    Unsupported,
    OutOfDomain,
    BlockedByPolicy,
    ContextWindowExceeded,
    FailedOperational,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GraphTransition {
    pub from: GraphState,
    pub to: Option<GraphState>,
    pub terminal: Option<TerminalState>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionRule {
    pub from: GraphState,
    pub to: Option<GraphState>,
    pub terminal: Option<TerminalState>,
}

#[derive(Debug, Clone)]
pub struct AssistantGraphTopology {
    graph: Graph<GraphState, &'static str>,
    nodes: HashMap<GraphState, NodeIndex>,
    edges: HashSet<(GraphState, GraphState)>,
    terminal_edges: HashSet<(GraphState, TerminalState)>,
}

impl Default for AssistantGraphTopology {
    fn default() -> Self {
        let mut graph = Graph::new();
        let mut nodes = HashMap::new();
        for state in [
            GraphState::ReceiveMessage,
            GraphState::BuildContextWindow,
            GraphState::RouteIntent,
            GraphState::ResolveClarification,
            GraphState::PlanRetrieval,
            GraphState::RetrieveKnowledge,
            GraphState::EvaluateEvidence,
            GraphState::PlanToolOrCapability,
            GraphState::GuardExecution,
            GraphState::ExecuteToolOrSql,
            GraphState::BuildStructuredResponse,
            GraphState::RenderResponse,
            GraphState::CompleteOrWait,
        ] {
            nodes.insert(state, graph.add_node(state));
        }
        let mut this = Self {
            graph,
            nodes,
            edges: HashSet::new(),
            terminal_edges: HashSet::new(),
        };
        for (from, to, label) in [
            (
                GraphState::ReceiveMessage,
                GraphState::BuildContextWindow,
                "message_received",
            ),
            (
                GraphState::BuildContextWindow,
                GraphState::RouteIntent,
                "context_built",
            ),
            (
                GraphState::BuildContextWindow,
                GraphState::CompleteOrWait,
                "context_built_wait",
            ),
            (
                GraphState::RouteIntent,
                GraphState::ResolveClarification,
                "clarification_reply",
            ),
            (
                GraphState::RouteIntent,
                GraphState::PlanRetrieval,
                "intent_routed",
            ),
            (
                GraphState::RouteIntent,
                GraphState::CompleteOrWait,
                "intent_terminal",
            ),
            (
                GraphState::ResolveClarification,
                GraphState::CompleteOrWait,
                "clarification_resolved",
            ),
            (
                GraphState::ResolveClarification,
                GraphState::PlanToolOrCapability,
                "clarification_selected",
            ),
            (
                GraphState::PlanRetrieval,
                GraphState::RetrieveKnowledge,
                "retrieval_planned",
            ),
            (
                GraphState::RetrieveKnowledge,
                GraphState::EvaluateEvidence,
                "knowledge_retrieved",
            ),
            (
                GraphState::EvaluateEvidence,
                GraphState::PlanToolOrCapability,
                "evidence_selected",
            ),
            (
                GraphState::EvaluateEvidence,
                GraphState::CompleteOrWait,
                "evidence_terminal",
            ),
            (
                GraphState::PlanToolOrCapability,
                GraphState::GuardExecution,
                "tool_planned",
            ),
            (
                GraphState::GuardExecution,
                GraphState::ExecuteToolOrSql,
                "policy_checked",
            ),
            (
                GraphState::GuardExecution,
                GraphState::BuildStructuredResponse,
                "policy_terminal",
            ),
            (
                GraphState::ExecuteToolOrSql,
                GraphState::BuildStructuredResponse,
                "execution_finished",
            ),
            (
                GraphState::BuildStructuredResponse,
                GraphState::RenderResponse,
                "response_built",
            ),
            (
                GraphState::BuildStructuredResponse,
                GraphState::CompleteOrWait,
                "response_built",
            ),
            (
                GraphState::RenderResponse,
                GraphState::CompleteOrWait,
                "response_rendered",
            ),
        ] {
            this.add_edge(from, to, label);
        }
        for state in [
            TerminalState::Completed,
            TerminalState::WaitingForUserInput,
            TerminalState::Unsupported,
            TerminalState::OutOfDomain,
            TerminalState::BlockedByPolicy,
            TerminalState::FailedOperational,
        ] {
            this.terminal_edges
                .insert((GraphState::CompleteOrWait, state));
        }
        this.terminal_edges.insert((
            GraphState::BuildContextWindow,
            TerminalState::ContextWindowExceeded,
        ));
        this
    }
}

impl AssistantGraphTopology {
    pub fn new() -> Self {
        Self::default()
    }

    fn add_edge(&mut self, from: GraphState, to: GraphState, label: &'static str) {
        self.edges.insert((from, to));
        self.graph
            .add_edge(self.nodes[&from], self.nodes[&to], label);
    }

    pub fn validate_transition(&self, transition: &GraphTransition) -> Result<()> {
        match (transition.to, transition.terminal) {
            (Some(to), None) if self.edges.contains(&(transition.from, to)) => Ok(()),
            (None, Some(terminal))
                if self.terminal_edges.contains(&(transition.from, terminal)) =>
            {
                Ok(())
            }
            (Some(_), Some(_)) => bail!(
                "graph transition cannot have both next state and terminal: {:?}",
                transition
            ),
            (None, None) => bail!(
                "graph transition must have next state or terminal: {:?}",
                transition
            ),
            (Some(to), None) => bail!(
                "illegal graph transition: {:?} -> {:?}",
                transition.from,
                to
            ),
            (None, Some(terminal)) => bail!(
                "illegal terminal graph transition: {:?} -> {:?}",
                transition.from,
                terminal
            ),
        }
    }

    pub fn validate_sequence(&self, transitions: &[GraphTransition]) -> Result<()> {
        if transitions.is_empty() {
            bail!("graph transition sequence is empty");
        }
        for (idx, transition) in transitions.iter().enumerate() {
            self.validate_transition(transition)?;
            if transition.terminal.is_some() && idx + 1 != transitions.len() {
                bail!("terminal graph transition must be final at index {idx}");
            }
            if idx > 0 {
                let previous = &transitions[idx - 1];
                if previous.terminal.is_none() && previous.to != Some(transition.from) {
                    bail!(
                        "disconnected graph transition chain at index {idx}: previous {:?}, next {:?}",
                        previous.to,
                        transition.from
                    );
                }
            }
        }
        Ok(())
    }

    pub fn debug_edges(&self) -> String {
        let mut lines: Vec<String> = self
            .edges
            .iter()
            .map(|(from, to)| format!("{from:?} -> {to:?}"))
            .collect();
        lines.extend(
            self.terminal_edges
                .iter()
                .map(|(from, terminal)| format!("{from:?} -> terminal::{terminal:?}")),
        );
        lines.sort();
        lines.join("\n")
    }
}
