use crate::ids::NodeId;
use std::collections::{HashMap, HashSet, VecDeque};

/// Compute the transitive closure of nodes that need invalidation
/// when the given set of changed nodes is modified.
/// Traverses reverse_deps to find all dependents.
pub fn invalidation_closure(
    changed: &HashSet<NodeId>,
    reverse_deps: &HashMap<NodeId, HashSet<NodeId>>,
) -> HashSet<NodeId> {
    let mut invalidated = changed.clone();
    let mut queue: VecDeque<NodeId> = changed.iter().copied().collect();

    while let Some(nid) = queue.pop_front() {
        if let Some(dependents) = reverse_deps.get(&nid) {
            for &dep in dependents {
                if invalidated.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }
    }

    invalidated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_invalidation() {
        let mut reverse = HashMap::new();
        reverse.insert(NodeId(1), HashSet::from([NodeId(2), NodeId(3)]));
        reverse.insert(NodeId(2), HashSet::from([NodeId(4)]));

        let changed = HashSet::from([NodeId(1)]);
        let result = invalidation_closure(&changed, &reverse);
        assert!(result.contains(&NodeId(1)));
        assert!(result.contains(&NodeId(2)));
        assert!(result.contains(&NodeId(3)));
        assert!(result.contains(&NodeId(4)));
    }
}
