(defsystem "genesis"
  :description "A transactional, capability-gated, content-addressed development substrate."
  :author "arvaer"
  :license "MIT"
  :version "0.0.1"
  :depends-on (:ironclad :babel)
  :serial t
  :components ((:file "src/package")
               (:file "src/node")
               (:file "src/store")
               (:file "src/graph")
               (:file "src/verbs")
               (:file "src/kernel")
               (:file "src/capability")
               (:file "src/delta")
               (:file "src/demo")))
