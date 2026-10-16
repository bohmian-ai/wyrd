//! Server-hosted Verifier block capabilities: format extraction, bounded
//! per-call workspaces, citation validation, the Python sandbox, and the
//! built-in Agent tools.
//!
//! These are the concrete implementations behind the
//! [`vala_eval::blocks`] executor seams. They hold no request state of their
//! own: each execution composes them with its accepted authority and pinned
//! evidence, and every limit comes from [`HostedLimits`].

use monty_types::memory_limit_with_headroom;
use std::time::Duration;

pub mod blocks;
pub mod evidence;
pub mod extract;
pub mod registry;
pub mod sandbox;
pub mod tools;
pub mod traces;
pub mod workspace;

/// `caller` under a fresh request ID, for one Oracle statement.
///
/// The Oracle tracks running queries by tenant and request ID, and one
/// execution issues statements concurrently: parallel trace checks and the
/// Agent loop's concurrent `bifrost.query` calls. Each statement therefore
/// runs under its own request ID while keeping the caller's tenant,
/// principal, delegation chain, and executor.
#[must_use]
pub(crate) fn statement_caller(
    caller: &crate::components::auth::Caller,
) -> crate::components::auth::Caller {
    crate::components::auth::Caller {
        request_id: wyrd_spec::request_id::RequestId::now_v7(),
        ..caller.clone()
    }
}

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
    /// Memory ceiling of one worker process, in bytes; must hold the
    /// operation budget plus Monty's allocator headroom.
    pub monty_process_memory_bytes: u64,
    /// Combined memory ceiling of every live worker process, in bytes.
    pub monty_pool_memory_bytes: u64,
    /// One complete Python operation, from checkout wait to result.
    pub code_timeout: Duration,
    /// Cumulative Python-operation time within one Agent invocation.
    pub agent_code_budget: Duration,
    /// Overall deadline of one immediate `verify` request.
    pub verify_timeout: Duration,
}

impl HostedLimits {
    /// Most live workers the memory ceilings allow: `monty_max_workers`,
    /// lowered until every live worker's process ceiling fits the pool
    /// ceiling together.
    ///
    /// Returns `None` when one operation's allocator ceiling (budget plus
    /// Monty's headroom) exceeds the process ceiling, or the pool ceiling
    /// cannot hold one worker.
    #[must_use]
    pub fn live_workers(&self) -> Option<usize> {
        let ceiling =
            memory_limit_with_headroom(usize::try_from(self.monty_memory_bytes).ok(), false)?;
        if u64::try_from(ceiling).ok()? > self.monty_process_memory_bytes {
            return None;
        }
        let fit =
            usize::try_from(self.monty_pool_memory_bytes / self.monty_process_memory_bytes.max(1))
                .ok()?;
        Some(fit.min(self.monty_max_workers)).filter(|&workers| workers > 0)
    }
}

impl Default for HostedLimits {
    /// The approved Deployment limit defaults.
    fn default() -> Self {
        /// One mebibyte, the unit the byte limits are stated in.
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
            verify_timeout: Duration::from_secs(60),
        }
    }
}

/// Memory ceilings bound the live worker count on every platform.
#[cfg(test)]
mod tests {
    use super::HostedLimits;

    /// The defaults fit five 384 MiB workers in 2 GiB; a pool ceiling below
    /// one worker or a process ceiling below the allocator ceiling refuses.
    ///
    /// # Panics
    /// Panics when the worker cap ignores either ceiling.
    #[test]
    fn memory_ceilings_cap_live_workers() {
        let limits = HostedLimits::default();
        assert_eq!(limits.live_workers(), Some(5));
        assert_eq!(
            HostedLimits {
                monty_max_workers: 3,
                ..limits
            }
            .live_workers(),
            Some(3)
        );
        for impossible in [
            HostedLimits {
                monty_pool_memory_bytes: limits.monty_process_memory_bytes - 1,
                ..limits
            },
            HostedLimits {
                monty_process_memory_bytes: limits.monty_memory_bytes,
                ..limits
            },
        ] {
            assert_eq!(impossible.live_workers(), None);
        }
    }
}
