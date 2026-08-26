//! MCP capability binary: agent-facing Bifrost tool journeys.
//!
//! Proves the MCP surface a Wyrd agent actually calls — Bifrost RBAC scope
//! enforcement on the read tools, and the authoritative schema a zero-row
//! query returns.
//!
//! Setup: Postgres plus a booted `WyrdTestServer`.
//!
//! Owning lane: `mise run test:bifrost:journey:mcp` runs this binary whole
//! with `--include-ignored`, and `mise run test:bifrost:journey` runs it as
//! one of the tier-1 capabilities. It is the single owner of MCP journeys, so a new
//! one is registered by adding a `mod` line below — never by adding a
//! `mise.toml` line.
//!
//! Out of scope: the server-side behavior the tools call into, which the
//! `wyrd-testing` capability binaries own.

mod layout;
mod rbac;
