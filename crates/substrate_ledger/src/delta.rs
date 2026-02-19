use serde::{Deserialize, Serialize};
use substrate_core::ast::Expr;
use substrate_graph::ids::NodeId;
use substrate_graph::node::NodeKind;

/// A structural delta: whole-node replacement only in v0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StructuralDelta {
    ReplaceSig {
        node_id: NodeId,
        new_sig: FuncSigData,
    },
    ReplaceBody {
        node_id: NodeId,
        new_ast: Expr,
    },
}

/// Serializable function signature data (mirrors NodeKind::FuncSig fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FuncSigData {
    pub name: String,
    pub args: Vec<String>,
    pub ret: Option<String>,
    pub effects: Vec<String>,
}

impl StructuralDelta {
    /// The node IDs affected by this delta.
    pub fn affected_node_ids(&self) -> Vec<NodeId> {
        match self {
            StructuralDelta::ReplaceSig { node_id, .. } => vec![*node_id],
            StructuralDelta::ReplaceBody { node_id, .. } => vec![*node_id],
        }
    }

    /// Apply this delta to a graph store. Returns error if node doesn't exist.
    pub fn apply(&self, graph: &mut substrate_graph::store::GraphStore) -> Result<(), String> {
        match self {
            StructuralDelta::ReplaceSig { node_id, new_sig } => {
                if !graph.nodes.contains_key(node_id) {
                    return Err(format!("node {node_id} does not exist"));
                }
                let node = NodeKind::FuncSig {
                    name: new_sig.name.clone(),
                    args: new_sig.args.clone(),
                    ret: new_sig.ret.clone(),
                    effects: new_sig.effects.clone(),
                };
                graph.replace(*node_id, node);
                Ok(())
            }
            StructuralDelta::ReplaceBody { node_id, new_ast } => {
                if !graph.nodes.contains_key(node_id) {
                    return Err(format!("node {node_id} does not exist"));
                }
                let sig_id = match graph.nodes.get(node_id) {
                    Some(NodeKind::FuncBody { sig_id, .. }) => *sig_id,
                    _ => return Err(format!("node {node_id} is not a FuncBody")),
                };
                let node = NodeKind::FuncBody {
                    sig_id,
                    ast: new_ast.clone(),
                };
                graph.replace(*node_id, node);
                Ok(())
            }
        }
    }
}
