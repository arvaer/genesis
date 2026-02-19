use crate::hash::{hash_effects, hash_graph, Hash};
use crate::patch::Patch;
use std::collections::HashSet;
use substrate_core::eval::{self, Env};
use substrate_graph::ids::NodeId;
use substrate_graph::node::NodeKind;
use substrate_graph::store::GraphStore;

/// Result of replaying a ledger.
#[derive(Debug)]
pub struct ReplayResult {
    pub final_graph_hash: Hash,
    pub effect_log_hashes: Vec<Hash>,
    pub patches_applied: usize,
}

/// Replay a ledger from a genesis graph, applying patches sequentially.
/// Returns the final graph hash and per-patch effect-log hashes.
pub fn replay(genesis: GraphStore, patches: &[Patch]) -> Result<ReplayResult, String> {
    let mut graph = genesis;
    graph.rebuild_all_deps();
    let mut current_hash = hash_graph(&graph);
    let mut effect_log_hashes = Vec::new();

    for (i, patch) in patches.iter().enumerate() {
        // Verify parent hash.
        if patch.parent_hash != current_hash {
            return Err(format!(
                "patch {} parent hash mismatch: expected {}, got {}",
                i, current_hash, patch.parent_hash
            ));
        }

        // Apply delta.
        patch
            .delta
            .apply(&mut graph)
            .map_err(|e| format!("patch {} delta failed: {e}", i))?;

        // Rebuild deps.
        let affected = patch.delta.affected_node_ids();
        graph.rebuild_deps_for(&affected);

        // Compute invalidation closure.
        let changed_set: HashSet<NodeId> = affected.iter().copied().collect();
        let invalidated = graph.invalidation_closure(&changed_set);

        // Evaluate to get effect log (same logic as tx.rs).
        let effects = evaluate_bodies_for_replay(&graph, &invalidated);
        let effect_hash = hash_effects(&effects);
        effect_log_hashes.push(effect_hash);

        // Recompute graph hash.
        graph.rebuild_all_deps();
        current_hash = hash_graph(&graph);
    }

    Ok(ReplayResult {
        final_graph_hash: current_hash,
        effect_log_hashes,
        patches_applied: patches.len(),
    })
}

/// Evaluate bodies for replay (mirrors tx.rs evaluate_bodies).
fn evaluate_bodies_for_replay(
    graph: &GraphStore,
    invalidated: &HashSet<NodeId>,
) -> Vec<substrate_core::effect::Effect> {
    let env = build_eval_env(graph);
    let mut all_effects = Vec::new();

    let mut body_ids: Vec<NodeId> = invalidated
        .iter()
        .filter(|nid| matches!(graph.nodes.get(nid), Some(NodeKind::FuncBody { .. })))
        .copied()
        .collect();
    body_ids.sort();

    // Check for main function.
    if let Some((main_sig_id, _)) = graph.lookup_sig("main") {
        if let Some((main_body_id, _)) = graph.find_body_for_sig(main_sig_id) {
            if invalidated.contains(&main_body_id) {
                body_ids = vec![main_body_id];
            }
        }
    }

    for body_id in body_ids {
        if let Some(NodeKind::FuncBody { ast, .. }) = graph.nodes.get(&body_id) {
            if let Ok(eff) = eval::eval_with_env(ast, &env) {
                all_effects.extend(eff.effects);
            }
        }
    }

    all_effects
}

fn build_eval_env(graph: &GraphStore) -> Env {
    let mut env = Env::new();

    for (&sig_id, node) in &graph.nodes {
        if let NodeKind::FuncSig { name, args, .. } = node {
            if let Some((_, NodeKind::FuncBody { ast, .. })) = graph.find_body_for_sig(sig_id) {
                let lambda = substrate_core::value::Value::Lambda {
                    params: args.clone(),
                    body: Box::new(ast.clone()),
                    env: Env::new(),
                };
                env.insert(name.clone(), lambda);
            }
        }
    }

    let env_snapshot = env.clone();
    for val in env.values_mut() {
        if let substrate_core::value::Value::Lambda {
            env: ref mut closure_env,
            ..
        } = val
        {
            *closure_env = env_snapshot.clone();
        }
    }

    env
}
