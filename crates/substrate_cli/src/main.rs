use anyhow::Result;
use clap::{Parser, Subcommand};
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use substrate_core::ast::Expr;
use substrate_graph::node::NodeKind;
use substrate_graph::store::GraphStore;
use substrate_ledger::delta::StructuralDelta;
use substrate_ledger::hash::{hash_graph, Hash};
use substrate_ledger::patch::{Patch, PatchMetadata};
use substrate_ledger::replay;
use substrate_ledger::tx::Engine;
use substrate_runtime::exec::{EffectExecutor, NoopExecutor};

#[derive(Parser)]
#[command(name = "substrate", about = "Agent-native dev substrate v0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create genesis graph with sample functions.
    Init {
        #[arg(long, default_value = "./ledger")]
        dir: PathBuf,
    },
    /// Run agent harness: N agents attempt M mutations.
    Harness {
        #[arg(long, default_value = "100")]
        agents: usize,
        #[arg(long, default_value = "10000")]
        mutations: usize,
        #[arg(long, default_value = "./ledger")]
        dir: PathBuf,
    },
    /// Replay ledger and verify hashes.
    Replay {
        #[arg(long, default_value = "./ledger")]
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init { dir } => cmd_init(&dir),
        Commands::Harness {
            agents,
            mutations,
            dir,
        } => cmd_harness(&dir, agents, mutations),
        Commands::Replay { dir } => cmd_replay(&dir),
    }
}

/// Create a genesis graph with sample functions.
fn create_genesis_graph() -> GraphStore {
    let mut graph = GraphStore::new();

    // Function: add(a, b) => (+ a b)
    let add_sig_id = graph.alloc_id();
    let add_body_id = graph.alloc_id();
    graph.insert(
        add_sig_id,
        NodeKind::FuncSig {
            name: "add".to_string(),
            args: vec!["a".to_string(), "b".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        add_body_id,
        NodeKind::FuncBody {
            sig_id: add_sig_id,
            ast: Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Symbol("a".to_string()),
                Expr::Symbol("b".to_string()),
            ]),
        },
    );

    // Function: double(x) => (* x 2)
    let dbl_sig_id = graph.alloc_id();
    let dbl_body_id = graph.alloc_id();
    graph.insert(
        dbl_sig_id,
        NodeKind::FuncSig {
            name: "double".to_string(),
            args: vec!["x".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        dbl_body_id,
        NodeKind::FuncBody {
            sig_id: dbl_sig_id,
            ast: Expr::List(vec![
                Expr::Symbol("*".to_string()),
                Expr::Symbol("x".to_string()),
                Expr::Number(2),
            ]),
        },
    );

    // Function: abs(n) => (if (< n 0) (- 0 n) n)
    let abs_sig_id = graph.alloc_id();
    let abs_body_id = graph.alloc_id();
    graph.insert(
        abs_sig_id,
        NodeKind::FuncSig {
            name: "abs".to_string(),
            args: vec!["n".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        abs_body_id,
        NodeKind::FuncBody {
            sig_id: abs_sig_id,
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

    // Function: square(x) => (* x x)
    let sq_sig_id = graph.alloc_id();
    let sq_body_id = graph.alloc_id();
    graph.insert(
        sq_sig_id,
        NodeKind::FuncSig {
            name: "square".to_string(),
            args: vec!["x".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        sq_body_id,
        NodeKind::FuncBody {
            sig_id: sq_sig_id,
            ast: Expr::List(vec![
                Expr::Symbol("*".to_string()),
                Expr::Symbol("x".to_string()),
                Expr::Symbol("x".to_string()),
            ]),
        },
    );

    // Function: identity(x) => x
    let id_sig_id = graph.alloc_id();
    let id_body_id = graph.alloc_id();
    graph.insert(
        id_sig_id,
        NodeKind::FuncSig {
            name: "identity".to_string(),
            args: vec!["x".to_string()],
            ret: None,
            effects: vec![],
        },
    );
    graph.insert(
        id_body_id,
        NodeKind::FuncBody {
            sig_id: id_sig_id,
            ast: Expr::Symbol("x".to_string()),
        },
    );

    graph.rebuild_all_deps();
    graph
}

fn cmd_init(dir: &std::path::Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    let graph = create_genesis_graph();
    let genesis_hash = hash_graph(&graph);

    // Save genesis graph.
    let genesis_json = serde_json::to_string_pretty(&graph)?;
    fs::write(dir.join("genesis.json"), &genesis_json)?;

    // Save empty ledger.
    let ledger_data = LedgerFile {
        genesis_hash,
        patches: vec![],
        effect_log_hashes: vec![],
    };
    let ledger_json = serde_json::to_string_pretty(&ledger_data)?;
    fs::write(dir.join("ledger.json"), &ledger_json)?;

    println!("Genesis graph created with {} nodes", graph.nodes.len());
    println!("Genesis hash: {genesis_hash}");
    println!("Ledger initialized at {}", dir.display());
    Ok(())
}

fn cmd_harness(dir: &std::path::Path, num_agents: usize, total_mutations: usize) -> Result<()> {
    // Load or create genesis.
    let graph = if dir.join("genesis.json").exists() {
        let data = fs::read_to_string(dir.join("genesis.json"))?;
        serde_json::from_str(&data)?
    } else {
        fs::create_dir_all(dir)?;
        let g = create_genesis_graph();
        let genesis_json = serde_json::to_string_pretty(&g)?;
        fs::write(dir.join("genesis.json"), &genesis_json)?;
        g
    };

    let mut engine = Engine::from_graph(graph);
    let executor = NoopExecutor;

    let mut commits = 0u64;
    let mut rejects_conflict = 0u64;
    let mut rejects_stale = 0u64;
    let mut rejects_validation = 0u64;
    let mut total_invalidation_size = 0u64;
    let mutations_per_agent = total_mutations / num_agents;

    for agent_id in 0..num_agents {
        // Deterministic seed per agent.
        let seed_bytes = blake3::hash(format!("agent-{agent_id}").as_bytes());
        let seed: [u8; 32] = *seed_bytes.as_bytes();
        let mut rng = StdRng::from_seed(seed);

        for _ in 0..mutations_per_agent {
            let body_ids = engine.graph.body_node_ids();
            if body_ids.is_empty() {
                break;
            }

            // Pick a random body node.
            let target_idx = rng.gen_range(0..body_ids.len());
            let target_id = body_ids[target_idx];

            // Get current AST and mutate it.
            let current_ast = match engine.graph.nodes.get(&target_id) {
                Some(NodeKind::FuncBody { ast, .. }) => ast.clone(),
                _ => continue,
            };

            let new_ast = mutate_ast(&current_ast, &mut rng);

            let patch = Patch {
                id: engine.next_patch_id(),
                parent_hash: engine.head_hash(),
                region_scope: vec![target_id],
                delta: StructuralDelta::ReplaceBody {
                    node_id: target_id,
                    new_ast,
                },
                metadata: PatchMetadata {
                    author: format!("agent-{agent_id}"),
                    ts: None,
                },
            };

            match engine.commit(patch) {
                Ok(result) => {
                    commits += 1;
                    total_invalidation_size += result.invalidated_nodes.len() as u64;
                    let _ = executor.execute(&result.effects);
                }
                Err(substrate_ledger::tx::TxError::StaleParent { .. }) => {
                    rejects_stale += 1;
                }
                Err(substrate_ledger::tx::TxError::RegionConflict) => {
                    rejects_conflict += 1;
                }
                Err(substrate_ledger::tx::TxError::ValidationFailed(_)) => {
                    rejects_validation += 1;
                }
                Err(_) => {
                    rejects_validation += 1;
                }
            }
        }
    }

    // Save ledger.
    let ledger_data = LedgerFile {
        genesis_hash: hash_graph(&create_genesis_graph()),
        patches: engine.patches.clone(),
        effect_log_hashes: engine.effect_log_hashes.clone(),
    };
    let ledger_json = serde_json::to_string_pretty(&ledger_data)?;
    fs::write(dir.join("ledger.json"), &ledger_json)?;

    // Save final graph.
    let graph_json = serde_json::to_string_pretty(&engine.graph)?;
    fs::write(dir.join("genesis.json"), &graph_json)?;

    let avg_invalidation = if commits > 0 {
        total_invalidation_size as f64 / commits as f64
    } else {
        0.0
    };

    println!("=== Harness Results ===");
    println!("Agents: {num_agents}");
    println!("Total mutation attempts: {total_mutations}");
    println!("Commits: {commits}");
    println!("Rejects (conflict): {rejects_conflict}");
    println!("Rejects (stale parent): {rejects_stale}");
    println!("Rejects (validation): {rejects_validation}");
    println!("Avg invalidation size: {avg_invalidation:.2}");
    println!("Final head hash: {}", engine.head_hash());
    Ok(())
}

fn cmd_replay(dir: &std::path::Path) -> Result<()> {
    let ledger_str = fs::read_to_string(dir.join("ledger.json"))?;
    let ledger_data: LedgerFile = serde_json::from_str(&ledger_str)?;

    // Load genesis graph: reconstruct from scratch for determinism.
    let genesis = create_genesis_graph();
    let genesis_hash = hash_graph(&genesis);

    if genesis_hash != ledger_data.genesis_hash {
        anyhow::bail!(
            "genesis hash mismatch: computed {genesis_hash}, ledger has {}",
            ledger_data.genesis_hash
        );
    }

    let start = std::time::Instant::now();
    let result = replay::replay(genesis, &ledger_data.patches)
        .map_err(|e| anyhow::anyhow!("replay failed: {e}"))?;
    let elapsed = start.elapsed();

    println!("=== Replay Results ===");
    println!("Patches replayed: {}", result.patches_applied);
    println!("Final graph hash: {}", result.final_graph_hash);
    println!("Replay time: {elapsed:?}");

    // Verify effect log hashes match.
    if result.effect_log_hashes.len() != ledger_data.effect_log_hashes.len() {
        anyhow::bail!(
            "effect log count mismatch: replay={}, ledger={}",
            result.effect_log_hashes.len(),
            ledger_data.effect_log_hashes.len()
        );
    }
    for (i, (replay_hash, ledger_hash)) in result
        .effect_log_hashes
        .iter()
        .zip(ledger_data.effect_log_hashes.iter())
        .enumerate()
    {
        if replay_hash != ledger_hash {
            anyhow::bail!(
                "effect log hash mismatch at patch {i}: replay={replay_hash}, ledger={ledger_hash}"
            );
        }
    }

    println!("All effect-log hashes match.");
    println!("Replay invariant verified.");
    Ok(())
}

/// Deterministic random AST mutator.
fn mutate_ast(ast: &Expr, rng: &mut StdRng) -> Expr {
    let mutation_type = rng.gen_range(0..5);
    match mutation_type {
        0 => {
            // Replace a number literal with a different one.
            replace_number(ast, rng)
        }
        1 => {
            // Wrap expression in (+ expr 1).
            Expr::List(vec![
                Expr::Symbol("+".to_string()),
                ast.clone(),
                Expr::Number(rng.gen_range(1..10)),
            ])
        }
        2 => {
            // Swap an operator.
            swap_operator(ast, rng)
        }
        3 => {
            // Wrap in (if (< expr 0) 0 expr).
            Expr::List(vec![
                Expr::Symbol("if".to_string()),
                Expr::List(vec![
                    Expr::Symbol("<".to_string()),
                    ast.clone(),
                    Expr::Number(0),
                ]),
                Expr::Number(0),
                ast.clone(),
            ])
        }
        _ => {
            // Replace with a simple expression.
            Expr::List(vec![
                Expr::Symbol("+".to_string()),
                Expr::Number(rng.gen_range(0..100)),
                Expr::Number(rng.gen_range(0..100)),
            ])
        }
    }
}

fn replace_number(ast: &Expr, rng: &mut StdRng) -> Expr {
    match ast {
        Expr::Number(_) => Expr::Number(rng.gen_range(-50..50)),
        Expr::Symbol(s) => Expr::Symbol(s.clone()),
        Expr::List(elems) => {
            if elems.is_empty() {
                return ast.clone();
            }
            // Pick a random element to mutate.
            let idx = rng.gen_range(0..elems.len());
            let mut new_elems = elems.clone();
            new_elems[idx] = replace_number(&elems[idx], rng);
            Expr::List(new_elems)
        }
    }
}

fn swap_operator(ast: &Expr, rng: &mut StdRng) -> Expr {
    let ops = ["+", "-", "*"];
    match ast {
        Expr::List(elems) if !elems.is_empty() => {
            let mut new_elems = elems.clone();
            if let Expr::Symbol(ref s) = elems[0] {
                if ops.contains(&s.as_str()) {
                    let new_op = ops[rng.gen_range(0..ops.len())];
                    new_elems[0] = Expr::Symbol(new_op.to_string());
                }
            }
            Expr::List(new_elems)
        }
        _ => ast.clone(),
    }
}

/// Serializable ledger file format.
#[derive(Debug, Serialize, Deserialize)]
struct LedgerFile {
    genesis_hash: Hash,
    patches: Vec<Patch>,
    effect_log_hashes: Vec<Hash>,
}
