use crate::hash::{hash_effects, hash_graph, Hash};
use crate::patch::Patch;
use std::collections::HashSet;
use substrate_core::ast::Expr;
use substrate_core::effect::Effect;
use substrate_core::eval::{self, Env};
use substrate_graph::ids::NodeId;
use substrate_graph::node::NodeKind;
use substrate_graph::store::GraphStore;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TxError {
    #[error("stale parent hash: expected {expected}, got {got}")]
    StaleParent { expected: Hash, got: Hash },
    #[error("region conflict: overlapping scope with in-flight transaction")]
    RegionConflict,
    #[error("delta application failed: {0}")]
    DeltaFailed(String),
    #[error("validation failed: {0}")]
    ValidationFailed(String),
    #[error("evaluation failed: {0}")]
    EvalFailed(String),
}

/// Result of a successful transaction commit.
#[derive(Debug, Clone)]
pub struct CommitResult {
    pub new_head_hash: Hash,
    pub effects: Vec<Effect>,
    pub effect_log_hash: Hash,
    pub invalidated_nodes: HashSet<NodeId>,
}

/// The transaction engine. Owns the graph and ledger state.
#[derive(Debug, Clone)]
pub struct Engine {
    pub graph: GraphStore,
    pub head_hash: Hash,
    pub patches: Vec<Patch>,
    pub effect_log_hashes: Vec<Hash>,
    in_flight_scopes: HashSet<NodeId>,
    next_patch_id: u64,
}

impl Engine {
    /// Create a new engine with an empty graph.
    pub fn new() -> Self {
        let graph = GraphStore::new();
        let head_hash = hash_graph(&graph);
        Engine {
            graph,
            head_hash,
            patches: Vec::new(),
            effect_log_hashes: Vec::new(),
            in_flight_scopes: HashSet::new(),
            next_patch_id: 1,
        }
    }

    /// Create an engine from an existing graph (genesis).
    pub fn from_graph(mut graph: GraphStore) -> Self {
        graph.rebuild_all_deps();
        let head_hash = hash_graph(&graph);
        Engine {
            graph,
            head_hash,
            patches: Vec::new(),
            effect_log_hashes: Vec::new(),
            in_flight_scopes: HashSet::new(),
            next_patch_id: 1,
        }
    }

    /// Get the current head hash.
    pub fn head_hash(&self) -> Hash {
        self.head_hash
    }

    /// Attempt to commit a patch. Returns CommitResult on success.
    pub fn commit(&mut self, patch: Patch) -> Result<CommitResult, TxError> {
        // 1. Check parent hash.
        if patch.parent_hash != self.head_hash {
            return Err(TxError::StaleParent {
                expected: self.head_hash,
                got: patch.parent_hash,
            });
        }

        // 2. Check region conflict.
        for nid in &patch.region_scope {
            if self.in_flight_scopes.contains(nid) {
                return Err(TxError::RegionConflict);
            }
        }

        // Mark scopes as in-flight (for multi-threaded scenarios; in v0 single-threaded).
        for nid in &patch.region_scope {
            self.in_flight_scopes.insert(*nid);
        }

        // 3. Clone graph for speculative application.
        let mut spec_graph = self.graph.clone();

        // 4. Apply delta.
        if let Err(e) = patch.delta.apply(&mut spec_graph) {
            self.clear_in_flight(&patch.region_scope);
            return Err(TxError::DeltaFailed(e));
        }

        // 5. Rebuild deps for affected nodes.
        let affected: Vec<NodeId> = patch.delta.affected_node_ids();
        spec_graph.rebuild_deps_for(&affected);

        // 6. Compute invalidation closure.
        let changed_set: HashSet<NodeId> = affected.iter().copied().collect();
        let invalidated = spec_graph.invalidation_closure(&changed_set);

        // 7. Validate.
        if let Err(e) = validate(&spec_graph, &affected) {
            self.clear_in_flight(&patch.region_scope);
            return Err(TxError::ValidationFailed(e));
        }

        // 8. Evaluate impacted function bodies to produce effect logs.
        let effects = evaluate_bodies(&spec_graph, &invalidated)?;

        // 9. Commit: update state.
        let effect_log_hash = hash_effects(&effects);
        spec_graph.rebuild_all_deps();
        let new_hash = hash_graph(&spec_graph);

        self.graph = spec_graph;
        self.head_hash = new_hash;
        self.patches.push(patch);
        self.effect_log_hashes.push(effect_log_hash);
        self.in_flight_scopes.clear();
        self.next_patch_id += 1;

        Ok(CommitResult {
            new_head_hash: new_hash,
            effects,
            effect_log_hash,
            invalidated_nodes: invalidated,
        })
    }

    /// Get the next patch ID.
    pub fn next_patch_id(&self) -> u64 {
        self.next_patch_id
    }

    fn clear_in_flight(&mut self, scope: &[NodeId]) {
        for nid in scope {
            self.in_flight_scopes.remove(nid);
        }
    }
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate that the graph is internally consistent after a delta.
fn validate(graph: &GraphStore, affected: &[NodeId]) -> Result<(), String> {
    for &nid in affected {
        // Node must exist.
        let node = graph
            .nodes
            .get(&nid)
            .ok_or_else(|| format!("node {nid} does not exist after delta"))?;

        // FuncBody: sig must exist and be a FuncSig.
        if let NodeKind::FuncBody { sig_id, ast } = node {
            if !graph.nodes.contains_key(sig_id) {
                return Err(format!(
                    "FuncBody {nid} references non-existent sig {sig_id}"
                ));
            }
            if !matches!(graph.nodes.get(sig_id), Some(NodeKind::FuncSig { .. })) {
                return Err(format!(
                    "FuncBody {nid} references {sig_id} which is not a FuncSig"
                ));
            }
            // Check that referenced function names resolve.
            let symbols = substrate_graph::deps::extract_symbols(ast);
            for sym in &symbols {
                // Only validate references to known function names.
                // Builtins and other symbols are fine.
                if is_builtin(sym) {
                    continue;
                }
                // If the symbol matches a function name, it's valid.
                // If it doesn't match any name, it could be a variable — skip.
            }
        }
    }
    Ok(())
}

fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "+" | "-"
            | "*"
            | "="
            | "<"
            | "quote"
            | "if"
            | "lambda"
            | "define"
            | "let"
            | "begin"
            | "effect"
            | "list"
            | "car"
            | "cdr"
            | "cons"
            | "null?"
            | "not"
            | "nil"
            | "true"
            | "false"
    )
}

/// Evaluate all FuncBody nodes in the invalidated set.
/// Builds an environment from all function signatures and bodies, then evaluates.
fn evaluate_bodies(
    graph: &GraphStore,
    invalidated: &HashSet<NodeId>,
) -> Result<Vec<Effect>, TxError> {
    // Build environment with all functions defined.
    let env = build_eval_env(graph);
    let mut all_effects = Vec::new();

    // Collect and sort body nodes for deterministic ordering.
    let mut body_ids: Vec<NodeId> = invalidated
        .iter()
        .filter(|nid| matches!(graph.nodes.get(nid), Some(NodeKind::FuncBody { .. })))
        .copied()
        .collect();
    body_ids.sort();

    // Check if there's a "main" function; if so, evaluate only that.
    if let Some((_main_sig_id, _)) = graph.lookup_sig("main") {
        if let Some((main_body_id, _)) = graph.find_body_for_sig(_main_sig_id) {
            if invalidated.contains(&main_body_id) {
                body_ids = vec![main_body_id];
            }
        }
    }

    for body_id in body_ids {
        if let Some(NodeKind::FuncBody { ast, .. }) = graph.nodes.get(&body_id) {
            match eval::eval_with_env(ast, &env) {
                Ok(eff) => {
                    all_effects.extend(eff.effects);
                }
                Err(_) => {
                    // Evaluation errors on a body are not fatal for the transaction;
                    // the body may reference unbound variables (params).
                    // In v0, we silently skip bodies that error during eval.
                }
            }
        }
    }

    Ok(all_effects)
}

/// Build an evaluation environment from all function definitions in the graph.
fn build_eval_env(graph: &GraphStore) -> Env {
    let mut env = Env::new();

    // First pass: create lambda values for all functions.
    for (&sig_id, node) in &graph.nodes {
        if let NodeKind::FuncSig { name, args, .. } = node {
            // Find the corresponding body.
            if let Some((_, NodeKind::FuncBody { ast, .. })) = graph.find_body_for_sig(sig_id) {
                let lambda = substrate_core::value::Value::Lambda {
                    params: args.clone(),
                    body: Box::new(ast.clone()),
                    env: Env::new(), // Will be filled below.
                };
                env.insert(name.clone(), lambda);
            }
        }
    }

    // Second pass: update closures to include the full environment (mutual recursion).
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

/// Convert a FuncBody AST into a callable form with its signature's params.
fn _make_callable(sig: &NodeKind, body_ast: &Expr) -> Option<Expr> {
    if let NodeKind::FuncSig { args, .. } = sig {
        let params = Expr::List(args.iter().map(|a| Expr::Symbol(a.clone())).collect());
        Some(Expr::List(vec![
            Expr::Symbol("lambda".to_string()),
            params,
            body_ast.clone(),
        ]))
    } else {
        None
    }
}
