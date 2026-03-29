;;;; verbs.lisp — Stages 3–6
;;;; The five primitive operations: create, inspect, change, revert, destroy.

(in-package :genesis)

;;; TODO Stage 3: verb-create — validate refs exist, store node, return hash
;;; TODO Stage 3: verb-inspect — lookup by hash
;;; TODO Stage 3: verb-inspect-subgraph — DFS from hash

;;; TODO Stage 4: verb-change — new node, first ref = previous hash

;;; TODO Stage 5: verb-revert — walk first-ref chain to target

;;; TODO Stage 6: verb-destroy — tombstone pattern
;;; TODO Stage 6: destroyed-p — scan for tombstone referencing hash
