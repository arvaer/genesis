;;;; store.lisp — Stage 1
;;;; Single source of truth. In-memory hash table. Store only grows.
;;;; Nodes are keyed by their SHA-256 hash string.

(in-package :genesis)

;;; TODO Stage 1: defvar *store* (make-hash-table :test #'equal)
;;; TODO Stage 1: store-node — returns hash as handle
;;; TODO Stage 1: lookup-node — hash → node or nil
;;; TODO Stage 1: node-exists-p — predicate
