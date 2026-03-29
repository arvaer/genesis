;;;; node.lisp — Stage 0
;;;; The atom of Genesis: content-addressed, immutable nodes.
;;;; Every node is identified by SHA-256(kind || name || refs || body).
;;;; Never call MAKE-NODE directly. Use GENESIS-NODE.

(in-package :genesis)

;;; TODO Stage 0: implement sha256-hex via ironclad
;;; TODO Stage 0: defstruct node (kind name refs body hash)
;;; TODO Stage 0: compute-node-hash — canonical serialization
;;; TODO Stage 0: genesis-node constructor (auto-computes hash)
;;; TODO Stage 0: 3 inline tests (determinism, collision, hash field)
