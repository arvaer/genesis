use crate::deps::{compute_deps_for_node, rebuild_all_deps};
use crate::ids::NodeId;
use crate::invalidate::invalidation_closure;
use crate::node::NodeKind;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// The semantic graph store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStore {
    pub nodes: HashMap<NodeId, NodeKind>,
    pub name_index: HashMap<String, NodeId>,
    pub forward_deps: HashMap<NodeId, HashSet<NodeId>>,
    pub reverse_deps: HashMap<NodeId, HashSet<NodeId>>,
    next_id: u64,
}

impl GraphStore {
    pub fn new() -> Self {
        GraphStore {
            nodes: HashMap::new(),
            name_index: HashMap::new(),
            forward_deps: HashMap::new(),
            reverse_deps: HashMap::new(),
            next_id: 1,
        }
    }

    /// Allocate a new unique NodeId.
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    /// Insert a node. Updates name_index if it's a FuncSig.
    pub fn insert(&mut self, id: NodeId, node: NodeKind) {
        if let NodeKind::FuncSig { ref name, .. } = node {
            self.name_index.insert(name.clone(), id);
        }
        self.nodes.insert(id, node);
        // Update next_id if needed.
        if id.0 >= self.next_id {
            self.next_id = id.0 + 1;
        }
    }

    /// Replace a node's content. Returns the old node if it existed.
    pub fn replace(&mut self, id: NodeId, node: NodeKind) -> Option<NodeKind> {
        // Remove old name index entry if replacing a sig.
        if let Some(NodeKind::FuncSig { ref name, .. }) = self.nodes.get(&id) {
            self.name_index.remove(&name.clone());
        }
        if let NodeKind::FuncSig { ref name, .. } = node {
            self.name_index.insert(name.clone(), id);
        }
        self.nodes.insert(id, node)
    }

    /// Rebuild dependency edges for specific nodes.
    pub fn rebuild_deps_for(&mut self, node_ids: &[NodeId]) {
        for &nid in node_ids {
            // Remove old forward edges and their reverse entries.
            if let Some(old_deps) = self.forward_deps.remove(&nid) {
                for dep in &old_deps {
                    if let Some(rev) = self.reverse_deps.get_mut(dep) {
                        rev.remove(&nid);
                    }
                }
            }
            // Compute new forward deps.
            if let Some(node) = self.nodes.get(&nid) {
                let deps = compute_deps_for_node(nid, node, &self.name_index);
                for &dep in &deps {
                    self.reverse_deps.entry(dep).or_default().insert(nid);
                }
                self.forward_deps.insert(nid, deps);
            }
        }
    }

    /// Rebuild all dependency edges from scratch.
    pub fn rebuild_all_deps(&mut self) {
        let (fwd, rev) = rebuild_all_deps(&self.nodes, &self.name_index);
        self.forward_deps = fwd;
        self.reverse_deps = rev;
    }

    /// Compute the invalidation closure for changed nodes.
    pub fn invalidation_closure(&self, changed: &HashSet<NodeId>) -> HashSet<NodeId> {
        invalidation_closure(changed, &self.reverse_deps)
    }

    /// Get all FuncBody node IDs.
    pub fn body_node_ids(&self) -> Vec<NodeId> {
        self.nodes
            .iter()
            .filter_map(|(&id, node)| {
                if matches!(node, NodeKind::FuncBody { .. }) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all node IDs.
    pub fn all_node_ids(&self) -> Vec<NodeId> {
        self.nodes.keys().copied().collect()
    }

    /// Look up a function signature by name.
    pub fn lookup_sig(&self, name: &str) -> Option<(NodeId, &NodeKind)> {
        self.name_index
            .get(name)
            .and_then(|&id| self.nodes.get(&id).map(|n| (id, n)))
    }

    /// Find the body node for a given signature ID.
    pub fn find_body_for_sig(&self, sig_id: NodeId) -> Option<(NodeId, &NodeKind)> {
        self.nodes.iter().find_map(|(&id, node)| {
            if let NodeKind::FuncBody { sig_id: s, .. } = node {
                if *s == sig_id {
                    return Some((id, node));
                }
            }
            None
        })
    }
}

impl Default for GraphStore {
    fn default() -> Self {
        Self::new()
    }
}
