use crate::ids::NodeId;
use crate::node::NodeKind;
use std::collections::{HashMap, HashSet};
use substrate_core::ast::Expr;

/// Extract symbol references from an AST expression.
pub fn extract_symbols(expr: &Expr) -> HashSet<String> {
    let mut symbols = HashSet::new();
    collect_symbols(expr, &mut symbols);
    symbols
}

fn collect_symbols(expr: &Expr, out: &mut HashSet<String>) {
    match expr {
        Expr::Symbol(s) => {
            out.insert(s.clone());
        }
        Expr::Number(_) => {}
        Expr::List(elems) => {
            // Skip the first element of special forms that bind names.
            if let Some(Expr::Symbol(head)) = elems.first() {
                match head.as_str() {
                    "quote" => return, // quoted data, not references
                    "lambda" => {
                        // Don't count params as references.
                        // Body references only.
                        if elems.len() == 3 {
                            collect_symbols(&elems[2], out);
                        }
                        return;
                    }
                    "define" => {
                        // The value part may reference things.
                        if elems.len() == 3 {
                            collect_symbols(&elems[2], out);
                        }
                        return;
                    }
                    _ => {}
                }
            }
            for e in elems {
                collect_symbols(e, out);
            }
        }
    }
}

/// Build dependency edges for a node given the current graph's name index.
/// Returns the set of NodeIds this node depends on.
pub fn compute_deps_for_node(
    _node_id: NodeId,
    node: &NodeKind,
    name_index: &HashMap<String, NodeId>,
) -> HashSet<NodeId> {
    let mut deps = HashSet::new();
    match node {
        NodeKind::FuncBody { sig_id, ast } => {
            // Body depends on its own signature.
            deps.insert(*sig_id);
            // Body depends on callees' signatures.
            let symbols = extract_symbols(ast);
            for sym in &symbols {
                if let Some(&callee_sig_id) = name_index.get(sym) {
                    if callee_sig_id != *sig_id {
                        deps.insert(callee_sig_id);
                    }
                }
            }
        }
        NodeKind::FuncSig { .. } => {
            // Signatures don't have dependencies in v0.
        }
    }
    deps
}

/// Rebuild all dependency edges for the graph.
pub fn rebuild_all_deps(
    nodes: &HashMap<NodeId, NodeKind>,
    name_index: &HashMap<String, NodeId>,
) -> (
    HashMap<NodeId, HashSet<NodeId>>,
    HashMap<NodeId, HashSet<NodeId>>,
) {
    let mut forward: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();
    let mut reverse: HashMap<NodeId, HashSet<NodeId>> = HashMap::new();

    for (&nid, node) in nodes {
        let deps = compute_deps_for_node(nid, node, name_index);
        for &dep in &deps {
            reverse.entry(dep).or_default().insert(nid);
        }
        forward.insert(nid, deps);
    }

    (forward, reverse)
}
