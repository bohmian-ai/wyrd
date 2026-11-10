//! Server-hosted Verifier block capabilities: format extraction, bounded
//! per-call workspaces, citation validation, the Python sandbox, and the
//! built-in Agent tools.
//!
//! These are the concrete implementations behind the
//! [`vala_verify::blocks`] executor seams. They hold no request state of their
//! own: each execution composes them with its accepted authority and pinned
//! evidence, and every limit comes from [`HostedLimits`].

use monty_types::memory_limit_with_headroom;
use std::time::Duration;

pub mod blocks;
pub mod evidence;
pub mod publications;
pub mod registry;
pub mod sandbox;
pub mod tools;
pub mod traces;
pub mod worker;
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

/// Every free permit of `permits`, held until the returned permit drops, so
/// a test can saturate an idle lane.
///
/// # Panics
/// Panics when `permits` is closed or a permit is taken concurrently.
#[cfg(feature = "test-support")]
pub(crate) fn hold_all(
    permits: &std::sync::Arc<tokio::sync::Semaphore>,
) -> tokio::sync::OwnedSemaphorePermit {
    let free = u32::try_from(permits.available_permits()).unwrap_or(u32::MAX);
    std::sync::Arc::clone(permits)
        .try_acquire_many_owned(free)
        .expect("a test saturates an open, idle admission")
}

/// An extractor refusal as the terminal block error it always is.
pub(crate) fn extract_refused(error: wyrd_loader::ExtractError) -> vala_verify::blocks::BlockError {
    vala_verify::blocks::BlockError::Terminal { reason: error.0 }
}

/// The Deployment limits the hosted capabilities enforce.
///
/// Defaults are the approved Deployment limits; server startup may lower or
/// raise them through its configuration before constructing the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedLimits {
    /// Largest input file, in bytes.
    pub file_max_bytes: u64,
    /// Largest combined input-file bytes per execution, and the most bytes
    /// one execution's accepted publications may retain together, so every
    /// publication set stays mountable as successor input.
    pub files_max_bytes: u64,
    /// Most publications one execution may accept across all its attempts.
    pub publications_max: u32,
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
    /// Most concurrently running direct `verify` executions per process;
    /// queued executions are bounded by the runtime's own execution slots.
    pub direct_executions: usize,
    /// Capacity reserved for direct `verify` executions.
    pub direct: LaneLimits,
    /// Capacity reserved for queued executions, continuations included.
    pub queued: LaneLimits,
    /// Prewarmed Python worker processes of each lane.
    pub monty_min_workers: usize,
    /// Monty allocation budget per Python operation, in bytes.
    pub monty_memory_bytes: u64,
    /// Memory ceiling of one worker process, in bytes; must hold the
    /// operation budget plus Monty's allocator headroom.
    pub monty_process_memory_bytes: u64,
    /// Combined memory ceiling of every live worker process of both lanes,
    /// in bytes.
    pub monty_pool_memory_bytes: u64,
    /// One complete Python operation, from checkout wait to result.
    pub code_timeout: Duration,
    /// Cumulative Python-operation time within one Agent invocation.
    pub agent_code_budget: Duration,
    /// Overall deadline of one immediate `verify` request.
    pub verify_timeout: Duration,
}

impl HostedLimits {
    /// The archive and extraction limits shared with client-side extraction.
    #[must_use]
    pub const fn extract(&self) -> wyrd_loader::ExtractLimits {
        wyrd_loader::ExtractLimits {
            archive_max_bytes: self.archive_max_bytes,
            archive_max_entries: self.archive_max_entries,
            archive_max_ratio: self.archive_max_ratio,
            extract_max_pages: self.extract_max_pages,
            extract_text_max_bytes: self.extract_text_max_bytes,
        }
    }

    /// Whether one operation's allocator ceiling (budget plus Monty's
    /// headroom) fits the worker process ceiling.
    #[must_use]
    pub fn worker_fits(&self) -> bool {
        memory_limit_with_headroom(usize::try_from(self.monty_memory_bytes).ok(), false)
            .and_then(|ceiling| u64::try_from(ceiling).ok())
            .is_some_and(|ceiling| ceiling <= self.monty_process_memory_bytes)
    }

    /// Whether the live workers of both lanes, each counted at its process
    /// ceiling, fit the pool ceiling together.
    #[must_use]
    pub fn lanes_fit(&self) -> bool {
        u64::try_from(self.direct.python_workers + self.queued.python_workers)
            .ok()
            .and_then(|workers| workers.checked_mul(self.monty_process_memory_bytes))
            .is_some_and(|total| total <= self.monty_pool_memory_bytes)
    }
}

/// The admission capacity one execution lane owns alone.
///
/// Direct and queued executions each hold a lane; neither borrows the
/// other's free capacity, so a saturated queue cannot starve direct `verify`
/// and direct traffic cannot take what queued work was given. Shared CPU,
/// Postgres, gateway, and provider capacity are not partitioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneLimits {
    /// Most concurrent workspaces.
    pub workspaces: usize,
    /// Most live Python worker processes.
    pub python_workers: usize,
    /// Most Python operations waiting for a worker.
    pub python_waiters: usize,
    /// Most concurrent gateway model calls; a further call waits for one to
    /// finish before it reaches gateway admission.
    pub model_calls: usize,
}

impl Default for HostedLimits {
    /// The approved Deployment limit defaults.
    fn default() -> Self {
        /// One mebibyte, the unit the byte limits are stated in.
        const MIB: u64 = 1024 * 1024;
        Self {
            file_max_bytes: 20 * MIB,
            files_max_bytes: 100 * MIB,
            publications_max: 1_000,
            archive_max_bytes: 200 * MIB,
            archive_max_entries: 10_000,
            archive_max_ratio: 100,
            extract_max_pages: 500,
            extract_text_max_bytes: 10 * 1024 * 1024,
            scratch_max_bytes: 250 * MIB,
            work_write_max_bytes: 50 * MIB,
            tool_result_max_bytes: 64 * 1024,
            direct_executions: 16,
            direct: LaneLimits {
                workspaces: 4,
                python_workers: 2,
                python_waiters: 4,
                model_calls: 16,
            },
            queued: LaneLimits {
                workspaces: 12,
                python_workers: 3,
                python_waiters: 12,
                model_calls: 48,
            },
            monty_min_workers: 1,
            monty_memory_bytes: 256 * MIB,
            monty_process_memory_bytes: 384 * MIB,
            monty_pool_memory_bytes: 2048 * MIB,
            code_timeout: Duration::from_secs(10),
            agent_code_budget: Duration::from_secs(60),
            verify_timeout: Duration::from_secs(60),
        }
    }
}

/// Memory ceilings bound both lanes' live workers on every platform.
#[cfg(test)]
mod tests {
    use super::HostedLimits;

    /// The default lanes split the former single pool: five 384 MiB workers
    /// in 2 GiB, 16 workspaces, and 16 waiters, with two prewarmed workers
    /// in all. One more worker in either lane, a pool below the lanes, or a
    /// process ceiling below the allocator ceiling refuses.
    ///
    /// # Panics
    /// Panics when a lane default or a memory ceiling is ignored.
    #[test]
    fn lanes_split_the_memory_ceilings() {
        let limits = HostedLimits::default();
        assert!(limits.worker_fits() && limits.lanes_fit());
        assert_eq!(
            (
                limits.direct.python_workers + limits.queued.python_workers,
                limits.direct.workspaces + limits.queued.workspaces,
                limits.direct.python_waiters + limits.queued.python_waiters,
                2 * limits.monty_min_workers,
            ),
            (5, 16, 16, 2)
        );
        let mut wider = limits;
        wider.queued.python_workers += 1;
        assert!(!wider.lanes_fit(), "six workers exceed 2 GiB");
        assert!(
            !HostedLimits {
                monty_pool_memory_bytes: limits.monty_process_memory_bytes,
                ..limits
            }
            .lanes_fit()
        );
        assert!(
            !HostedLimits {
                monty_process_memory_bytes: limits.monty_memory_bytes,
                ..limits
            }
            .worker_fits()
        );
    }
}
