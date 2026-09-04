//! First-party Rust MCP connectivity for Wyrd.
//!
//! This crate is client-side only: it connects to the `/mcp` endpoint the Wyrd
//! server mounts. It implements exactly one `rmcp` extension point — the
//! Streamable HTTP client transport trait — so Wyrd's per-request credential
//! and correlation headers ride along with the official SDK's protocol
//! handling. It does not wrap `rmcp`'s client, handler, lifecycle, tool, or
//! protocol types, and it serves no MCP surface of its own.

pub mod client;
