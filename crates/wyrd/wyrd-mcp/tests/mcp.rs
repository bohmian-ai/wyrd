//! MCP capability binary: agent-facing Bifrost tool journeys.
//!
//! Proves the MCP surface a Wyrd agent actually calls — Bifrost RBAC scope
//! enforcement on the read tools, and the authoritative schema a zero-row
//! query returns.
//!
//! Setup: Postgres plus a booted `WyrdTestServer`.
//!
//! Owning lane: `mise run test:bifrost:journey`, which runs this binary whole
//! with `--include-ignored`. It is the single owner of MCP journeys, so a new
//! one is registered by adding a `#[path]` module below — never by adding a
//! `mise.toml` line.
//!
//! Out of scope: the server-side behavior the tools call into, which the
//! `wyrd-testing` capability binaries own.

#[path = "bifrost_layout.rs"]
mod bifrost_layout;
#[path = "bifrost_rbac.rs"]
mod bifrost_rbac;
