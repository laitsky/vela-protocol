## 2026-05-01 - generated proptest tuple arity

QEDGen generated a structurally valid proptest model for the Vela security
spec, but the backend emitted one flat tuple strategy with 26 abstract state
fields. The generated Rust does not compile because proptest's tuple Strategy
implementation does not cover that arity.

Hypothesis: QEDGen should chunk generated state strategies or emit nested
tuples/struct field strategies for large brownfield security models.
