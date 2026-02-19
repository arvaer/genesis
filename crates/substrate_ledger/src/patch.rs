use crate::delta::StructuralDelta;
use crate::hash::Hash;
use serde::{Deserialize, Serialize};
use substrate_graph::ids::NodeId;

/// Metadata for a patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchMetadata {
    pub author: String,
    #[serde(default)]
    pub ts: Option<u64>,
}

/// A proposed structural mutation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    pub id: u64,
    pub parent_hash: Hash,
    pub region_scope: Vec<NodeId>,
    pub delta: StructuralDelta,
    pub metadata: PatchMetadata,
}
