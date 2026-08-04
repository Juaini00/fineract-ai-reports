use std::collections::{HashMap, HashSet, VecDeque};

use petgraph::{
    Direction,
    algo::{is_cyclic_directed, toposort},
    graph::{DiGraph, NodeIndex},
};

use super::contract::{ExecutionWorkflow, NodeId};

/// The graph contains a cycle outside a bounded iteration primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CyclicGraph;

/// The workflow's authoritative dependency graph. Compiler and verifier use this wrapper,
/// rather than maintaining a parallel hand-written adjacency implementation.
pub struct WorkflowGraph {
    graph: DiGraph<NodeId, ()>,
    by_id: HashMap<NodeId, NodeIndex>,
}

impl WorkflowGraph {
    pub fn new(workflow: &ExecutionWorkflow) -> Self {
        let mut graph = DiGraph::new();
        let mut by_id = HashMap::new();
        for node in &workflow.nodes {
            by_id.insert(node.id.clone(), graph.add_node(node.id.clone()));
        }
        for edge in &workflow.edges {
            if let (Some(from), Some(to)) = (by_id.get(&edge.from), by_id.get(&edge.to)) {
                graph.add_edge(*from, *to, ());
            }
        }
        Self { graph, by_id }
    }
    pub fn is_cyclic(&self) -> bool {
        is_cyclic_directed(&self.graph)
    }
    pub fn topological_order(&self) -> Result<Vec<NodeId>, CyclicGraph> {
        toposort(&self.graph, None)
            .map(|nodes| {
                nodes
                    .into_iter()
                    .map(|node| self.graph[node].clone())
                    .collect()
            })
            .map_err(|_| CyclicGraph)
    }
    pub fn reachable_from(&self, entry: &NodeId) -> HashSet<NodeId> {
        let Some(start) = self.by_id.get(entry).copied() else {
            return HashSet::new();
        };
        let mut seen = HashSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            for next in self.graph.neighbors_directed(node, Direction::Outgoing) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        seen.into_iter()
            .map(|node| self.graph[node].clone())
            .collect()
    }
    pub fn runnable(&self, completed: &HashSet<NodeId>) -> Vec<NodeId> {
        self.graph
            .node_indices()
            .filter(|node| {
                let id = &self.graph[*node];
                !completed.contains(id)
                    && self
                        .graph
                        .neighbors_directed(*node, Direction::Incoming)
                        .all(|parent| completed.contains(&self.graph[parent]))
            })
            .map(|node| self.graph[node].clone())
            .collect()
    }
    /// The largest dependency-ready wave. This is the graph's load-bearing
    /// fan-out width used by verifier V6; it is not inferred from node count.
    pub fn max_runnable_width(&self) -> usize {
        let mut completed = HashSet::new();
        let mut width = 0;
        loop {
            let runnable = self.runnable(&completed);
            if runnable.is_empty() {
                return width;
            }
            width = width.max(runnable.len());
            completed.extend(runnable);
        }
    }
    pub fn entry_nodes(&self) -> Vec<NodeId> {
        self.graph
            .externals(Direction::Incoming)
            .map(|node| self.graph[node].clone())
            .collect()
    }
    pub fn contains(&self, id: &NodeId) -> bool {
        self.by_id.contains_key(id)
    }
}
