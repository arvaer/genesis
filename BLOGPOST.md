# Building a Kernel for AI Code Agents: 10,000 Deterministic Mutations in 1.2 Seconds

Text diffs are the wrong abstraction for AI agents writing code.

When a human edits a file, a line-level diff is a reasonable record of what changed. But when dozens of autonomous agents are mutating a shared codebase concurrently, text diffs become a liability. They're positional, fragile, and impossible to verify deterministically. Two agents editing different functions in the same file produce conflicting diffs even when their changes are logically independent. Worse, you can't replay a sequence of text diffs and guarantee you'll arrive at the same state -- whitespace, line endings, and merge order all introduce nondeterminism.

We built something different: a transactional mutation engine where code is a semantic graph, changes are structural deltas, and every operation is deterministically replayable. Then we threw 100 agents at it, had them make 10,000 mutations, and replayed the entire ledger to verify bit-identical hashes. It took 1.2 seconds.

This post walks through the design, the invariants that make it work, and what we learned building it.

---

## The Core Idea: Code as a Content-Addressed Graph

Instead of files containing text, the authoritative representation of a program is a **semantic graph**. Each node in the graph is either a function signature or a function body:

```
NodeKind::FuncSig { name: "add", args: ["a", "b"], ... }
NodeKind::FuncBody { sig_id: N1, ast: (+ a b) }
```

A function signature declares what a function looks like from the outside -- its name, parameters, and (eventually) type information. A function body contains the actual AST. These are separate nodes because an agent can change a function's implementation without touching its signature, and the system can use that distinction to limit the blast radius of a change.

The graph tracks dependencies between nodes. If `double` calls `add`, then `double`'s body node has a dependency edge pointing to `add`'s signature node. When `add`'s signature changes, the engine walks these reverse dependency edges to find everything that needs revalidation. When only `add`'s body changes, callers are unaffected -- their dependency is on the signature, not the implementation.

This is a simple insight but it has deep consequences. It means the system can answer "what broke?" after any mutation, instantly and deterministically, without re-evaluating the entire program.

## Effects Are Data, Not Actions

The most unusual design choice is the effect model. In this system, evaluation **never executes side effects**. Instead, the evaluator produces an ordered log of effect records:

```rust
pub struct Eff<T> {
    pub value: T,
    pub effects: Vec<Effect>,
}

pub struct Effect {
    pub cap_id: CapabilityId,
    pub op: String,
    pub args: Vec<Value>,
}
```

When Lisp code calls `(effect 0 write 42)`, nothing happens. No bytes are written anywhere. Instead, the evaluator appends `Effect { cap_id: 0, op: "write", args: [42] }` to the log and returns `Nil`. The complete log is threaded through the evaluation via the `Eff` type -- a value paired with its accumulated effects.

`Eff` forms a monad. `Eff::pure` wraps a value with no effects. `Eff::map` transforms the value while preserving the log. `Eff::bind` sequences two effectful computations by concatenating their logs:

```rust
pub fn bind<U, F: FnOnce(T) -> Eff<U>>(self, f: F) -> Eff<U> {
    let mut effects = self.effects;
    let next = f(self.value);
    effects.extend(next.effects);
    Eff { value: next.value, effects }
}
```

This design means evaluation is pure. Same expression, same environment, same result -- always. The effect log can be hashed, compared, replayed, and audited. After a transaction commits, the log is handed to an `EffectExecutor` which can do whatever it wants with the effects -- execute them, simulate them, or (in our case) silently discard them. The evaluator doesn't care and doesn't need to know.

This separation sounds academic until you need to verify that 10,000 mutations produced the same effects when replayed from scratch. Then it's the only thing that works.

## The Transaction Protocol

Every mutation flows through a transaction protocol that enforces four invariants:

**1. No effects during evaluation.** Already covered. Effects are data.

**2. Deterministic replay.** Given the same genesis graph and the same sequence of patches, the system reproduces bit-identical graph hashes and per-patch effect-log hashes.

**3. Transaction atomicity.** A patch either commits fully or not at all. There is no partial state.

**4. Conflict rejection.** If two patches touch overlapping sets of nodes, the second one is rejected outright. No merging in v0.

Here's the commit path:

```
1. Verify parent_hash == current head hash
2. Check region_scope doesn't overlap in-flight transactions
3. Clone the graph (speculative snapshot)
4. Apply the structural delta to the clone
5. Rebuild dependency edges for affected nodes
6. Compute invalidation closure (transitive dependents)
7. Validate: nodes exist, references resolve, types match
8. Evaluate impacted function bodies → produce effect log
9. Hash the effect log
10. If everything passes: swap in the new graph, advance the head hash,
    append the patch to the ledger
11. If anything fails: discard the clone, state unchanged
```

Step 3 is the key to atomicity. The engine works on a cloned snapshot. If validation or evaluation fails at any point, the clone is dropped and the real graph is untouched. There's no rollback logic because there's nothing to roll back -- the mutation was never applied to the real state.

The parent hash check (step 1) is how the system detects stale patches. Every patch records the head hash it was built against. If another patch committed in between, the hash won't match and the patch is rejected. The agent can retry with a fresh snapshot.

The region scope check (step 2) prevents two concurrent patches from touching the same nodes. In v0 this is a simple set intersection. If your patch touches node 7 and another in-flight patch also touches node 7, yours is rejected. No attempt at merging, no conflict resolution, just a clean rejection. The agent retries.

## Content Addressing

The graph is content-addressed using blake3. The hash function is deterministic by construction: node IDs are sorted, each node is serialized to JSON, and the bytes are fed to a streaming hasher in a fixed order:

```rust
pub fn hash_graph(graph: &GraphStore) -> Hash {
    let mut hasher = blake3::Hasher::new();
    let mut ids: Vec<_> = graph.nodes.keys().collect();
    ids.sort();
    for id in ids {
        let node = &graph.nodes[id];
        hasher.update(&id.0.to_le_bytes());
        let json = serde_json::to_string(node).unwrap_or_default();
        hasher.update(json.as_bytes());
    }
    // Dependency edges are also hashed
    // ...
    Hash(*hasher.finalize().as_bytes())
}
```

This means the graph hash is a function of its content, not its history. Two graphs with identical nodes and edges will produce the same hash regardless of how they got there. The ledger stores the hash chain: each patch records its parent hash, and replay verifies the chain link by link.

Effect logs get the same treatment. After evaluating the impacted function bodies, the resulting `Vec<Effect>` is serialized to JSON and hashed. This hash is stored alongside the patch. During replay, the system re-evaluates the same bodies and compares the effect-log hash. If they differ, something broke determinism.

## The Agent Harness: 100 Agents, 10,000 Mutations

To stress-test the system, we built a harness that simulates 100 autonomous agents making a total of 10,000 mutations. Each agent gets a deterministic RNG seed derived from its ID via blake3:

```rust
let seed_bytes = blake3::hash(format!("agent-{agent_id}").as_bytes());
let mut rng = StdRng::from_seed(seed);
```

On each turn, an agent:
1. Picks a random function body node from the graph
2. Generates a mutated AST using one of five strategies:
   - Replace a number literal with a random value
   - Wrap the expression in `(+ expr N)`
   - Swap an arithmetic operator (`+` → `-` → `*`)
   - Wrap in a conditional: `(if (< expr 0) 0 expr)`
   - Replace entirely with `(+ rand rand)`
3. Submits a patch targeting that node

The mutations are syntactically valid by construction -- each strategy produces a well-formed `Expr`. The system handles the rest: dependency tracking, invalidation, validation, evaluation, and hashing.

### Results

```
=== Harness Results ===
Agents: 100
Total mutation attempts: 10000
Commits: 10000
Rejects (conflict): 0
Rejects (stale parent): 0
Rejects (validation): 0
Avg invalidation size: 1.00
Final head hash: 3af4f0acbe8b8919f6ffbecdfa33348223396773eb3609ded2fb541f8be28411
```

10,000 commits, zero rejects. In v0 the kernel is single-threaded, so agents submit sequentially and never encounter stale parents. The average invalidation size of 1.0 means each mutation only invalidated the node it touched -- no cascading recomputation.

Then we replayed the entire ledger from the genesis graph:

```
=== Replay Results ===
Patches replayed: 10000
Final graph hash: 3af4f0acbe8b8919f6ffbecdfa33348223396773eb3609ded2fb541f8be28411
Replay time: 1.165716407s
All effect-log hashes match.
Replay invariant verified.
```

Same final hash. Every per-patch effect-log hash matches. 10,000 patches replayed in 1.17 seconds. The replay invariant holds.

This is the property that matters. It means the ledger is a complete, verifiable record of how the program evolved. You can take the genesis graph and the ledger to a different machine, replay it, and arrive at the exact same state. You can audit any individual patch by replaying up to that point and checking the effect log. You can bisect regressions by binary-searching the patch history.

## What the Architecture Looks Like in Rust

The system is a Cargo workspace with five crates, chosen to keep dependency boundaries clean:

```
substrate_core     AST, parser, evaluator, Eff<T>, Effect, Value
substrate_graph    NodeId, NodeKind, GraphStore, dependency tracking, invalidation
substrate_ledger   Hash, Patch, StructuralDelta, transaction Engine, replay
substrate_runtime  Capabilities, EffectExecutor trait, NoopExecutor
substrate_cli      Binary: init, harness, replay commands
```

Total dependencies: `anyhow`, `thiserror`, `serde`, `serde_json`, `blake3`, `rand`, `clap`. No runtime, no async, no heavy frameworks. The engine is a single-threaded `Engine` struct that owns the graph and the patch ledger:

```rust
pub struct Engine {
    pub graph: GraphStore,
    pub head_hash: Hash,
    pub patches: Vec<Patch>,
    pub effect_log_hashes: Vec<Hash>,
    in_flight_scopes: HashSet<NodeId>,
    next_patch_id: u64,
}
```

Everything flows through `Engine::commit()`. It's the only way to mutate state. There's no backdoor, no "just set this field directly" escape hatch. If it doesn't go through `commit`, it doesn't happen.

## The Lisp Is Tiny on Purpose

The language is a minimal Lisp: `quote`, `if`, `lambda`, `define`, `let`, `begin`, arithmetic (`+`, `-`, `*`, `=`, `<`), list operations (`car`, `cdr`, `cons`), and the `effect` form. No macros, no continuations, no tail-call optimization. The parser is about 80 lines. The evaluator is about 350.

This is deliberate. The language exists to exercise the mutation engine, not to be a production programming language. What matters is that it's deterministic, it produces ASTs that can be structurally mutated, and it supports the effect model. A richer language would add complexity without testing any new invariant.

That said, the architecture doesn't depend on Lisp specifically. The `Expr` type could be replaced with any AST that supports serialization and structural comparison. The graph doesn't know or care what language the ASTs come from. This is by design -- the roadmap includes multi-language frontends that parse Python, JavaScript, or other languages into the same semantic graph.

## What We Learned

**Separating signatures from bodies is the right default granularity.** Most mutations touch implementations, not interfaces. When an agent changes how `add` computes its result, callers of `add` don't need revalidation. This keeps the invalidation closure small (average 1.0 in our harness) and makes the system scale linearly with mutation count rather than quadratically with codebase size.

**Content-addressed hashing makes replay trivial.** There's no "did I apply these patches in the right order?" question. The hash chain answers it at every step. If the parent hash doesn't match, the patch doesn't apply. If the final hash doesn't match, something is wrong. Binary answers, no ambiguity.

**Effects-as-data is surprisingly practical.** The initial instinct is that separating effect production from effect execution adds complexity. In practice it removes it. The evaluator is simpler because it doesn't need to handle IO errors or rollback partial effects. The transaction engine is simpler because it can hash the effect log without executing it. Replay is simpler because it only needs the evaluator, not the executor.

**In-memory graph cloning for speculative execution is fast enough.** We worried that cloning the entire graph on every commit would be expensive. For a graph with 10 nodes and 10,000 commits, it's negligible. This won't scale to millions of nodes, but it scales to the interesting range for v0. When it becomes a bottleneck, persistent data structures (hash array mapped tries) can replace the clone with O(log n) structural sharing.

**Zero-merge conflict resolution is freeing.** By refusing to merge conflicting patches, the system avoids an entire class of bugs. Agents retry, and the retry is cheap because they just re-read the current graph and re-generate a mutation. In practice, conflicts are rare when agents target different nodes. When conflicts do occur, the cost of rejection plus retry is lower than the cost of a bad merge.

## What's Next

This is v0. The system works but it's deliberately minimal. The roadmap for v1 includes:

- **Subtree deltas.** Currently, mutations replace entire function bodies. Finer-grained deltas -- replacing a single sub-expression within a body -- would reduce conflict rates and enable more surgical mutations.

- **Type information.** Function signatures currently carry optional `ret` and `effects` fields that aren't enforced. Adding type inference or at least type checking would catch more errors at validation time rather than evaluation time.

- **Multi-language frontends.** The graph is language-agnostic by design. Parsing Python or JavaScript into `FuncSig` + `FuncBody` nodes would let the same mutation engine work across languages.

- **Network distribution.** The ledger is already a serializable, hashchain-linked sequence of patches. Replicating it across machines with a consensus protocol would enable distributed multi-agent development.

- **LLM agent integration.** The random mutator is a placeholder. Replacing it with an LLM that reads the current graph, understands the codebase semantics, and proposes meaningful structural deltas is the actual goal. The mutation engine doesn't care how the delta was generated -- it validates and commits the same way regardless.

The fundamental bet is that AI agents need a different substrate than human developers. Humans work in files and text editors. Agents work better with semantic graphs, structural deltas, and deterministic transactions. This is the kernel for that substrate.

---

*The full source is a Rust workspace: ~1,500 lines across 5 crates, 19 tests, zero clippy warnings. Genesis to 10,000 committed mutations to verified replay in under 2 seconds.*
