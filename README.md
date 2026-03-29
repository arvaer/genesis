# Genesis

A transactional, capability-gated, content-addressed development substrate.

Genesis is a trusted kernel where an LLM proposes changes (deltas) and the kernel validates, commits, or rejects them — solving the confused deputy problem for LLM-driven development.

## Core Ideas

- **Content-addressed DAG**: Every node is identified by `SHA-256(kind || name || refs || body)`. The store only grows — no mutation, no deletion, only tombstones.
- **Five primitive verbs**: `create`, `inspect`, `change`, `revert`, `destroy`. Everything is built from these.
- **Capability model**: seL4/E-language lineage. Capabilities only narrow. Widening is an error. An agent with `:inspect-only` cap *cannot* create, no matter how it tries.
- **Delta protocol**: LLM proposes `(defstruct delta verb args rationale capability)`. Kernel validates cap, then executes — or rejects with a clean error.

## Stack

- **SBCL** + **SLIME** — Common Lisp
- **Ironclad** — SHA-256 hashing
- **Style**: `defstruct`, closures. No CLOS, no `loop`.

## Build Order

| Stage | File | What |
|-------|------|------|
| 0 | `src/node.lisp` | `defstruct node`, `genesis-node`, SHA-256 hash |
| 1 | `src/store.lisp` | `*store*` hash table, `store-node`, `lookup-node` |
| 2 | `src/graph.lisp` | `collect-subgraph`, `dag-p` cycle detection |
| 3 | `src/verbs.lisp` | `verb-create`, `verb-inspect` |
| 4 | `src/verbs.lisp` | `verb-change` — version chain |
| 5 | `src/verbs.lisp` | `verb-revert` — walk history |
| 6 | `src/verbs.lisp` | `verb-destroy` — tombstone |
| 7 | `src/kernel.lisp` | Unified dispatcher |
| 8 | `src/capability.lisp` | Cap nodes, `narrow-env` |
| 9 | `src/delta.lisp` | `defstruct delta`, `apply-delta` |
| 10 | `src/demo.lisp` | End-to-end confused deputy demo |

## Getting Started

```bash
# Install SBCL + Quicklisp, then:
(ql:quickload :ironclad)
(ql:quickload :babel)
# Load the system
(asdf:load-system :genesis)
```

**Stage 0 entry point (Mon 3/30, 8am):**
Open `src/node.lisp`. Write `sha256-hex`, then `defstruct node`, then `genesis-node`. Run the 3 inline tests. Done when all pass.
