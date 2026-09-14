//! Hakari-managed feature unification for Wyrd Rust test and Clippy builds.
//!
//! Workspace members take this crate as a path-only dev-dependency, so every
//! test or `--all-targets` build resolves third-party crates (for example
//! `sqlx-core`, `tokio`, `datafusion`) with one feature set and reuses one
//! artifact instead of recompiling per lane. It has no code; its generated
//! `Cargo.toml` carries the unified dependency lines. `cargo publish` strips
//! path-only dev-dependencies, so published crates never depend on it.
