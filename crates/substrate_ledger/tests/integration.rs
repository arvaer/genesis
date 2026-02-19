use substrate_core::ast::Expr;
use substrate_core::eval::{self, Env};
use substrate_core::parse;
use substrate_graph::node::NodeKind;
use substrate_graph::store::GraphStore;
use substrate_ledger::delta::StructuralDelta;
use substrate_ledger::hash::hash_graph;
use substrate_ledger::patch::{Patch, PatchMetadata};
use substrate_ledger::replay;
use substrate_ledger::tx::Engine;

/// Helper: create a genesis graph with sample functions.
fn create_test_graph() -> GraphStore {
    let mut graph = GraphStore::new();

    // add(a, b) => (+ a b)
    let add_sig = graph.alloc_id();
    let add_body = graph.alloc_id();
    graph.insert(
        add_sig,
        NodeKind::FuncSig {
            name: "add".to_string(),
            args: vec!["a".to_string(), "b".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        add_body,
        NodeKind::FuncBody {
            sig_id: add_sig,
            ast: Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Symbol("a".to_string()),
                Expr::Symbol("b".to_string()),
            ]),
        },
    );

    // double(x) => (* x 2)
    let dbl_sig = graph.alloc_id();
    let dbl_body = graph.alloc_id();
    graph.insert(
        dbl_sig,
        NodeKind::FuncSig {
            name: "double".to_string(),
            args: vec!["x".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        dbl_body,
        NodeKind::FuncBody {
            sig_id: dbl_sig,
            ast: Expr::List(vec![
                Expr::Symbol("*".to_string()),
                Expr::Symbol("x".to_string()),
                Expr::Number(2),
            ]),
        },
    );

    // abs(n) => (if (< n 0) (- 0 n) n)
    let abs_sig = graph.alloc_id();
    let abs_body = graph.alloc_id();
    graph.insert(
        abs_sig,
        NodeKind::FuncSig {
            name: "abs".to_string(),
            args: vec!["n".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        abs_body,
        NodeKind::FuncBody {
            sig_id: abs_sig,
            ast: Expr::List(vec![
                Expr::Symbol("if".to_string()),
                Expr::List(vec![
                    Expr::Symbol("<".to_string()),
                    Expr::Symbol("n".to_string()),
                    Expr::Number(0),
                ]),
                Expr::List(vec![
                    Expr::Symbol("-".to_string()),
                    Expr::Number(0),
                    Expr::Symbol("n".to_string()),
                ]),
                Expr::Symbol("n".to_string()),
            ]),
        },
    );

    graph.rebuild_all_deps();
    graph
}

// ============================================================
// TEST 1: core_parse_roundtrip
// ============================================================
#[test]
fn core_parse_roundtrip() {
    let inputs = vec![
        "42",
        "foo",
        "(+ 1 2)",
        "(define add (lambda (a b) (+ a b)))",
        "(if (< x 0) (- 0 x) x)",
        "(effect 0 write 42)",
        "(begin (define x 10) (+ x 1))",
    ];
    for input in inputs {
        let expr = parse::parse(input).unwrap();
        let printed = expr.to_string();
        let reparsed = parse::parse(&printed).unwrap();
        assert_eq!(
            expr, reparsed,
            "roundtrip failed for: {input}\nprinted: {printed}"
        );
    }
}

// ============================================================
// TEST 2: eval_deterministic
// ============================================================
#[test]
fn eval_deterministic() {
    let programs = vec![
        "(+ (* 3 4) (- 10 5))",
        "((lambda (x) (+ x x)) 21)",
        "(if (< 1 2) (+ 10 20) 0)",
        "(effect 0 write (+ 1 2))",
    ];
    for src in programs {
        let expr = parse::parse(src).unwrap();
        let env = Env::new();
        let r1 = eval::eval(&expr, &env).unwrap();
        let r2 = eval::eval(&expr, &env).unwrap();
        assert_eq!(r1.value, r2.value, "non-deterministic value for: {src}");
        assert_eq!(
            r1.effects, r2.effects,
            "non-deterministic effects for: {src}"
        );
    }
}

// ============================================================
// TEST 3: ledger_replay_invariant
// ============================================================
#[test]
fn ledger_replay_invariant() {
    let graph = create_test_graph();
    let mut engine = Engine::from_graph(graph.clone());

    // The body node IDs in our test graph.
    let body_ids = engine.graph.body_node_ids();
    assert!(!body_ids.is_empty());

    // Commit 200 patches: cycle through body nodes with small mutations.
    let mutations: Vec<Expr> = (0..200)
        .map(|i| {
            Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Number(i),
                Expr::Number(i + 1),
            ])
        })
        .collect();

    for (i, new_ast) in mutations.iter().enumerate() {
        let target = body_ids[i % body_ids.len()];
        let patch = Patch {
            id: engine.next_patch_id(),
            parent_hash: engine.head_hash(),
            region_scope: vec![target],
            delta: StructuralDelta::ReplaceBody {
                node_id: target,
                new_ast: new_ast.clone(),
            },
            metadata: PatchMetadata {
                author: "test".to_string(),
                ts: None,
            },
        };
        engine.commit(patch).expect("commit should succeed");
    }

    let final_hash = engine.head_hash();
    let committed_patches = engine.patches.clone();
    let committed_effect_hashes = engine.effect_log_hashes.clone();

    assert_eq!(committed_patches.len(), 200);

    // Replay from genesis.
    let replay_result = replay::replay(graph, &committed_patches).expect("replay should succeed");

    // Assert final graph hashes match.
    assert_eq!(
        final_hash, replay_result.final_graph_hash,
        "final graph hash mismatch: engine={final_hash}, replay={}",
        replay_result.final_graph_hash
    );

    // Assert per-patch effect-log hashes match.
    assert_eq!(
        committed_effect_hashes.len(),
        replay_result.effect_log_hashes.len()
    );
    for (i, (engine_h, replay_h)) in committed_effect_hashes
        .iter()
        .zip(replay_result.effect_log_hashes.iter())
        .enumerate()
    {
        assert_eq!(engine_h, replay_h, "effect log hash mismatch at patch {i}");
    }
}

// ============================================================
// TEST 4: conflict_reject
// ============================================================
#[test]
fn conflict_reject() {
    let graph = create_test_graph();
    let mut engine = Engine::from_graph(graph);

    let body_ids = engine.graph.body_node_ids();
    let target = body_ids[0];

    // First patch: commit successfully.
    let patch1 = Patch {
        id: engine.next_patch_id(),
        parent_hash: engine.head_hash(),
        region_scope: vec![target],
        delta: StructuralDelta::ReplaceBody {
            node_id: target,
            new_ast: Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Number(1),
                Expr::Number(2),
            ]),
        },
        metadata: PatchMetadata {
            author: "agent-1".to_string(),
            ts: None,
        },
    };
    engine.commit(patch1).expect("first commit should succeed");

    // Second patch: same region_scope but different parent_hash (stale).
    // This tests overlapping region with stale parent — should be rejected.
    let old_hash = hash_graph(&create_test_graph());
    let patch2 = Patch {
        id: engine.next_patch_id(),
        parent_hash: old_hash, // Stale!
        region_scope: vec![target],
        delta: StructuralDelta::ReplaceBody {
            node_id: target,
            new_ast: Expr::List(vec![
                Expr::Symbol("*".to_string()),
                Expr::Number(3),
                Expr::Number(4),
            ]),
        },
        metadata: PatchMetadata {
            author: "agent-2".to_string(),
            ts: None,
        },
    };
    let result = engine.commit(patch2);
    assert!(
        result.is_err(),
        "second commit with overlapping scope should be rejected"
    );
    match result {
        Err(substrate_ledger::tx::TxError::StaleParent { .. }) => {
            // Expected: stale parent hash.
        }
        Err(e) => {
            // Also acceptable: region conflict.
            panic!("unexpected error type: {e}");
        }
        Ok(_) => panic!("should have been rejected"),
    }
}

// ============================================================
// TEST 5: stale_parent_reject
// ============================================================
#[test]
fn stale_parent_reject() {
    let graph = create_test_graph();
    let mut engine = Engine::from_graph(graph);

    let body_ids = engine.graph.body_node_ids();
    let old_head = engine.head_hash();

    // Commit patch A to advance the head.
    let patch_a = Patch {
        id: engine.next_patch_id(),
        parent_hash: old_head,
        region_scope: vec![body_ids[0]],
        delta: StructuralDelta::ReplaceBody {
            node_id: body_ids[0],
            new_ast: Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Number(99),
                Expr::Number(1),
            ]),
        },
        metadata: PatchMetadata {
            author: "agent-a".to_string(),
            ts: None,
        },
    };
    engine.commit(patch_a).expect("patch A should commit");

    // Now head has advanced. Try to commit with old_head as parent.
    let patch_b = Patch {
        id: engine.next_patch_id(),
        parent_hash: old_head, // Stale!
        region_scope: vec![body_ids[1]],
        delta: StructuralDelta::ReplaceBody {
            node_id: body_ids[1],
            new_ast: Expr::List(vec![
                Expr::Symbol("*".to_string()),
                Expr::Number(5),
                Expr::Number(5),
            ]),
        },
        metadata: PatchMetadata {
            author: "agent-b".to_string(),
            ts: None,
        },
    };
    let result = engine.commit(patch_b);
    assert!(result.is_err(), "stale parent should be rejected");
    match result {
        Err(substrate_ledger::tx::TxError::StaleParent { expected, got }) => {
            assert_ne!(expected, got);
            assert_eq!(got, old_head);
        }
        Err(e) => panic!("unexpected error: {e}"),
        Ok(_) => panic!("should have been rejected"),
    }
}
