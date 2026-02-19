use crate::ids::NodeId;
use serde::{Deserialize, Serialize};
use substrate_core::ast::Expr;

/// The kind of node in the semantic graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    FuncSig {
        name: String,
        args: Vec<String>,
        #[serde(default)]
        ret: Option<String>,
        #[serde(default)]
        effects: Vec<String>,
    },
    FuncBody {
        sig_id: NodeId,
        ast: Expr,
    },
}

impl NodeKind {
    /// Returns the function name if this is a FuncSig.
    pub fn sig_name(&self) -> Option<&str> {
        match self {
            NodeKind::FuncSig { name, .. } => Some(name),
            _ => None,
        }
    }
}
