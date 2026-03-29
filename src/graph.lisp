;;;; graph.lisp — Stage 2
;;;; DAG traversal and acyclicity enforcement.

(in-package :genesis)

;;; TODO Stage 2: collect-subgraph — DFS over refs, returns all reachable nodes
;;; TODO Stage 2: dag-p — cycle detection via DFS with in-stack set
