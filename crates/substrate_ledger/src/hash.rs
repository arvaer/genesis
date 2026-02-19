use serde::{Deserialize, Serialize};
use std::fmt;

/// Content hash (blake3, 32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hash(pub [u8; 32]);

impl Hash {
    pub const ZERO: Hash = Hash([0u8; 32]);

    /// Hash arbitrary bytes.
    pub fn of(data: &[u8]) -> Self {
        Hash(*blake3::hash(data).as_bytes())
    }

    /// Hash a string.
    pub fn of_str(s: &str) -> Self {
        Self::of(s.as_bytes())
    }

    /// Convert to hex string.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }

    /// Parse from hex string.
    pub fn from_hex(s: &str) -> Option<Self> {
        hex_decode(s).map(Hash)
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", &self.to_hex()[..16])
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl Serialize for Hash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Hash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Hash::from_hex(&s).ok_or_else(|| serde::de::Error::custom("invalid hex hash"))
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn hex_decode(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_digit(chunk[0])?;
        let lo = hex_digit(chunk[1])?;
        bytes[i] = (hi << 4) | lo;
    }
    Some(bytes)
}

fn hex_digit(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Compute a content hash of the entire graph state.
/// Deterministic: sorts node IDs, hashes their serialized content.
pub fn hash_graph(graph: &substrate_graph::store::GraphStore) -> Hash {
    let mut hasher = blake3::Hasher::new();
    let mut ids: Vec<_> = graph.nodes.keys().collect();
    ids.sort();
    for id in ids {
        let node = &graph.nodes[id];
        hasher.update(&id.0.to_le_bytes());
        let json = serde_json::to_string(node).unwrap_or_default();
        hasher.update(json.as_bytes());
    }
    // Also hash dependency edges for completeness.
    let mut dep_ids: Vec<_> = graph.forward_deps.keys().collect();
    dep_ids.sort();
    for id in dep_ids {
        hasher.update(b"dep:");
        hasher.update(&id.0.to_le_bytes());
        let mut targets: Vec<_> = graph.forward_deps[id].iter().collect();
        targets.sort();
        for t in targets {
            hasher.update(&t.0.to_le_bytes());
        }
    }
    Hash(*hasher.finalize().as_bytes())
}

/// Hash an effect log for replay verification.
pub fn hash_effects(effects: &[substrate_core::effect::Effect]) -> Hash {
    let json = serde_json::to_string(effects).unwrap_or_default();
    Hash::of(json.as_bytes())
}
