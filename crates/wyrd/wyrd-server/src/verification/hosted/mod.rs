//! Server-hosted Verifier block capabilities: format extraction, bounded
//! per-call workspaces, citation validation, the Python sandbox, and the
//! built-in Agent tools.
//!
//! These are the concrete implementations behind the
//! [`vala_eval::blocks`] executor seams. They hold no request state of their
//! own: each execution composes them with its accepted authority and pinned
//! evidence, and every limit comes from [`HostedLimits`].

use std::time::Duration;

pub mod evidence;
pub mod extract;
pub mod sandbox;
pub mod workspace;

/// The Deployment limits the hosted capabilities enforce.
///
/// Defaults are the approved Deployment limits; server startup may lower or
/// raise them through its configuration before constructing the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedLimits {
    /// Largest input file, in bytes.
    pub file_max_bytes: u64,
    /// Largest combined input-file bytes per execution.
    pub files_max_bytes: u64,
    /// Largest decompressed bytes per archive, including OOXML containers.
    pub archive_max_bytes: u64,
    /// Most entries per archive.
    pub archive_max_entries: usize,
    /// Largest decompressed/compressed ratio per archive.
    pub archive_max_ratio: u64,
    /// Most PDF pages or PPTX slides per extraction.
    pub extract_max_pages: usize,
    /// Most extracted text bytes per extraction.
    pub extract_text_max_bytes: usize,
    /// Largest materialized bytes per workspace.
    pub scratch_max_bytes: u64,
    /// Largest bytes of one file code or tools write under `/work`.
    pub work_write_max_bytes: u64,
    /// Largest serialized tool result, in bytes.
    pub tool_result_max_bytes: usize,
    /// Most concurrent workspaces per process.
    pub max_workspaces: usize,
    /// Prewarmed Python worker processes.
    pub monty_min_workers: usize,
    /// Most live Python worker processes.
    pub monty_max_workers: usize,
    /// Most Python operations waiting for a worker.
    pub monty_max_waiters: usize,
    /// Monty allocation budget per Python operation, in bytes.
    pub monty_memory_bytes: u64,
    /// Hard address-space ceiling of one worker process, in bytes.
    pub monty_process_memory_bytes: u64,
    /// Combined resident-memory ceiling of every worker process, in bytes.
    pub monty_pool_memory_bytes: u64,
    /// One complete Python operation, from checkout wait to result.
    pub code_timeout: Duration,
    /// Cumulative Python-operation time within one Agent invocation.
    pub agent_code_budget: Duration,
}

impl Default for HostedLimits {
    /// The approved Deployment limit defaults.
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Self {
            file_max_bytes: 20 * MIB,
            files_max_bytes: 100 * MIB,
            archive_max_bytes: 200 * MIB,
            archive_max_entries: 10_000,
            archive_max_ratio: 100,
            extract_max_pages: 500,
            extract_text_max_bytes: 10 * 1024 * 1024,
            scratch_max_bytes: 250 * MIB,
            work_write_max_bytes: 50 * MIB,
            tool_result_max_bytes: 64 * 1024,
            max_workspaces: 16,
            monty_min_workers: 2,
            monty_max_workers: 8,
            monty_max_waiters: 16,
            monty_memory_bytes: 256 * MIB,
            monty_process_memory_bytes: 384 * MIB,
            monty_pool_memory_bytes: 2048 * MIB,
            code_timeout: Duration::from_secs(10),
            agent_code_budget: Duration::from_secs(60),
        }
    }
}
