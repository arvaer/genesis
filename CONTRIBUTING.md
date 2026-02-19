# v0 Acceptance Checklist (Hard Gate)

This repository is **not** a general dev platform yet. v0 exists to prove one thing:

> A deterministic, transactional symbolic mutation engine with a structural patch ledger can outperform file+diff as the collaboration primitive.

Any change that does not strengthen that proof is out of scope.

---

## Core Invariants (Must Never Break)

### ✅ I1 — No effects execute during evaluation
- `eval` must **only** return `Eff<Value>` (value + ordered effect log).
- Any real-world action (IO, time, random, network, subprocess) must be expressed as an `Effect` record and executed **only** at commit time by an executor.

**Reject if:** evaluator calls filesystem/network/time/random directly.

---

### ✅ I2 — Deterministic replay
Given:
- evaluator version hash
- genesis graph snapshot
- patch ledger

Replaying must produce:
- identical final graph hash
- identical per-patch effect-log hashes

**Reject if:** replay depends on environment, wall clock, nondeterministic iteration order, or external toolchain variance.

---

### ✅ I3 — Transaction atomicity
A patch commit is all-or-nothing:
- either patch + effects are committed
- or nothing changes

**Reject if:** partial commit is possible, or rollback does not restore identical pre-state.

---

### ✅ I4 — Conflict rule v0
If two transactions overlap node scopes, reject. No merging in v0.

**Reject if:** code introduces “auto-merge”, “best effort apply”, or partial scope resolution.

---

## Scope Guardrails (Keep v0 Small)

### ✅ S1 — Single language only: tiny Lisp
- s-expression AST is authoritative.
- Text is a projection for UI, not the source of truth.

**Reject if:** a second language frontend is added before v0 passes the stress target.

---

### ✅ S2 — Structural deltas only
v0 deltas are **whole-node replacements**:
- ReplaceSig
- ReplaceBody

**Reject if:** subtree-delta systems, macro rewriting engines, refactoring DSLs, or generalized “edit scripts” are introduced in v0.

---

### ✅ S3 — Ordered effect log only
No effect DAG scheduling in v0.

**Reject if:** effect dependencies, parallel execution planners, topological sorts, or partial effect commits are added.

---

### ✅ S4 — No distributed runtime in v0
Single machine only.

**Reject if:** networking, consensus, replication, CRDTs, or multi-host coordination appears in v0.

---

## Required CLI Capabilities

### ✅ C1 — `init`
Must create a deterministic genesis graph snapshot with a few functions.

### ✅ C2 — `harness`
Must run a deterministic stress test:
- default: 100 agents
- default: 10,000 attempted mutations
- reports: commits, rejects (stale parent), rejects (conflict), rejects (validation), avg invalidation closure size, runtime

### ✅ C3 — `replay`
Must:
- load ledger from disk
- replay from genesis
- assert final graph hash matches expected
- print final hash + summary

---

## Required Tests (Must Pass)

### ✅ T1 — Parser roundtrip
Parse → print → parse preserves structure.

### ✅ T2 — Evaluator determinism
Same input yields identical:
- Value
- Effect log

### ✅ T3 — Ledger replay invariant
Commit N patches → save ledger → replay → hashes match.

### ✅ T4 — Conflict rejection
Two overlapping scope patches cannot both commit.

### ✅ T5 — Stale parent rejection
Patch with old parent hash is rejected (agent must rebase).

---

## Performance/Scale Target (v0 “Done”)

v0 is accepted only when all are true:

- ✅ `harness` completes 100 agents / 10k mutations on a single machine without panics
- ✅ replay succeeds and matches hashes
- ✅ deterministic results across two consecutive runs with the same seeds
- ✅ no effects execute during evaluation (verified by code inspection + tests)

---

## Code Quality Gate

Before merging:
- `cargo fmt`
- `cargo clippy` (no new warnings; ideally zero warnings)
- `cargo test`

---

## Explicit Non-Goals for v0 (Auto-Reject)

If a PR adds any of the following, it is out of scope:

- hardware kernel / bootloader / drivers / scheduler work
- effect DAG scheduling
- multi-language frontends
- “voting”, governance, reputation systems
- LLM integration (unless it only plugs into the already-working harness interface)
- complex type systems beyond basic arity/name resolution
- macro systems beyond minimal Lisp forms needed for v0
- distributed runtime features (networking, replication, CRDTs)

---

## Decision Rule

If a proposed change does not *directly* improve:
- deterministic replay,
- transactional patch safety,
- structural mutation throughput,
- dependency invalidation correctness,
- or observability (debuggability/visualization),

then it is out of scope for v0.

