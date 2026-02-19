# Substrate v0 -- Agent-Native Dev Substrate

A deterministic, transactional symbolic mutation engine where authoritative state is a semantic graph of Lisp AST nodes. Agents propose structural deltas (not text diffs), transactions apply deltas speculatively, validate deterministically, then commit. Evaluation never executes effects; it only produces an ordered effect log.

## Problem

Traditional code mutation via text diffs is fragile, non-composable, and impossible to verify deterministically. When multiple AI agents mutate a shared codebase, you need:

- **Structural awareness**: mutations operate on AST nodes, not character offsets
- **Deterministic replay**: given the same genesis state and patch ledger, you always get the same result
- **Transactional atomicity**: a patch either commits fully or not at all
- **Effect separation**: evaluation produces a log of effects (IO, network, etc.) as data -- effects are never executed during eval

## Layered Architecture

| Layer | Description | Status |
|-------|-------------|--------|
| L0 | AST + Parser + Evaluator (tiny Lisp) | Implemented |
| L1 | Semantic Graph (nodes, deps, invalidation) | Implemented |
| L2 | Ledger (patches, deltas, transactions, replay) | Implemented |
| L3 | Runtime (capabilities, effect execution) | Implemented (Noop) |
| L4 | Agent Harness (random mutation, conflict resolution) | Implemented |
| L5 | Multi-agent orchestration, networking | Future |

## Absolute Invariants

1. **No effects during eval.** Effects are data (ordered log) returned by eval.
2. **Deterministic replay.** Reapplying ledger from genesis yields bit-identical graph hash and per-patch effect-log hashes.
3. **Transaction atomicity.** Either a patch commits in full or not at all (rollback on validation failure).
4. **Conflict rule v0.** Overlapping `region_scope` node IDs => reject commit (no merges in v0).

## Effect Semantics

The `(effect cap op args...)` Lisp form does **not** execute any side effect. Instead, it appends an `Effect { cap_id, op, args }` record to an ordered log carried through `Eff<Value>`. The log is returned to the caller (the transaction engine), which may pass it to an `EffectExecutor` after commit. The default executor (`NoopExecutor`) discards all effects silently.

This separation means:
- Eval is pure and deterministic
- Effect logs can be replayed, audited, and compared
- Different executors can be swapped in without changing evaluation semantics

## Crate Structure

```
crates/
  substrate_core/    -- Expr, Value, Effect, Eff<T>, parser, evaluator
  substrate_graph/   -- NodeId, NodeKind, GraphStore, dependency tracking, invalidation
  substrate_ledger/  -- Hash, Patch, StructuralDelta, transaction Engine, replay
  substrate_runtime/ -- CapabilityRegistry, EffectExecutor trait, NoopExecutor
  substrate_cli/     -- Binary: init, harness, replay commands
```

## Usage

### Build

```bash
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Initialize a Genesis Graph

```bash
cargo run --release -- init --dir ./ledger
```

Creates a genesis graph with sample functions (`add`, `double`, `abs`, `square`, `identity`) and an empty ledger.

### Run the Agent Harness

```bash
cargo run --release -- harness --agents 100 --mutations 10000 --dir ./ledger
```

Spawns 100 deterministic agent workers that collectively attempt 10,000 mutations. Each agent:
- Gets a deterministic RNG seed: `blake3("agent-{id}")`
- Picks a random function body node
- Generates a mutated AST (number replacement, operator swap, wrapping, etc.)
- Submits a patch with `parent_hash = current_head`
- If the head has advanced, the patch fails (stale parent) and is counted as a reject

Prints stats: commits, rejects (conflict/stale/validation), avg invalidation size.

### Replay the Ledger

```bash
cargo run --release -- replay --dir ./ledger
```

Loads the ledger, replays all patches from genesis, and verifies:
- Final graph hash matches
- Per-patch effect-log hashes match

## Design Choices

- **Single JSON ledger file** (`ledger.json`): simplest option for v0. Contains genesis hash, ordered patches, and effect-log hashes.
- **In-memory graph cloning** for speculative transaction application: simple and correct. O(n) per commit, acceptable for v0 scale.
- **Sorted node IDs** in hash computation: ensures deterministic ordering regardless of HashMap iteration order.
- **Builtin functions** (`+`, `-`, `*`, `=`, `<`, `list`, `car`, `cdr`, `cons`, `null?`, `not`) are hardcoded in the evaluator, not stored in the environment. This keeps the eval path simple.
- **Lambda closures** capture the environment at definition time, enabling mutual recursion through a two-pass environment setup.
- **Effect op as literal symbol**: in `(effect cap op args...)`, the `op` is taken as a literal symbol (not evaluated) for ergonomics. If an expression is provided, it's evaluated.

## Roadmap v1

- **Subtree deltas**: patch individual sub-expressions within a function body, not just whole-node replacement
- **Better typing**: optional type annotations on FuncSig, type inference for the Lisp dialect
- **Multi-language frontends**: parse Python/JS/etc. into the semantic graph alongside Lisp
- **Network distribution**: multi-machine ledger replication with consensus
- **DAG effect ordering**: allow concurrent effect streams with explicit ordering constraints
- **Persistent storage**: replace in-memory graph with a content-addressed store (e.g., LMDB)
- **LLM agent integration**: replace random mutator with LLM-driven code generation
- **Incremental hashing**: avoid full graph rehash on each commit
