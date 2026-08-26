//! Cluster-load and materializer binary.
//!
//! Surface-scoped test target carved out of the former monolithic `integration`
//! binary. It holds the deterministic public-Gate cluster matrix
//! (`pg_bifrost_cluster_load`, the `test:bifrost:cluster` lane) alongside the
//! bench-gated materializer suite (`pg_bifrost_materializer`, only present under
//! `--features bench`). Modules point at unchanged files via `#[path]`, so every
//! test name, `#[ignore]` gate, and `#[cfg(feature = "bench")]` gate is retained.

mod load;
mod materializer;
