;;;; capability.lisp — Stage 8
;;;; Capability-gated mutation. seL4/E-language lineage.
;;;; Capabilities only narrow — widening is an error.

(in-package :genesis)

;;; TODO Stage 8: :cap node kind with body spec (:permits :kinds :scope)
;;; TODO Stage 8: narrow-env — derive child env by removing permissions
