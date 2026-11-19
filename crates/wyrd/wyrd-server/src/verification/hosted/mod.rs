//! Server-hosted Verifier block capabilities: format extraction, bounded
//! per-call workspaces, citation validation, the Python sandbox, and the
//! built-in Agent tools.
//!
//! These are the concrete implementations behind the
//! [`vala_verify::blocks`] executor seams. They hold no request state of their
//! own: each execution composes them with its accepted authority and pinned
//! evidence, and every limit comes from [`HostedLimits`].

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
/// a test can saturate an idle admission.
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
    /// Largest compact JSON argument object of one direct or nested tool
    /// call, `code.run` source included, in bytes.
    pub tool_argument_max_bytes: usize,
    /// Most direct and nested tool requests of one logical Agent
    /// invocation, across its retries.
    pub agent_tool_max_calls: u32,
    /// Most concurrently running direct `verify` executions per process;
    /// queued executions are bounded by the runtime's own execution slots.
    pub direct_executions: usize,
    /// Most concurrent gateway model calls of every direct and queued
    /// execution in the process; a further call waits for one to finish
    /// before it reaches gateway admission.
    pub model_calls: usize,
    /// Python worker processes the pool prewarms when process memory holds
    /// them; a target, not a floor, since idle workers are reaped under
    /// pressure.
    pub monty_min_workers: usize,
    /// Monty allocation budget per Python operation, in bytes.
    pub monty_memory_bytes: u64,
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
            tool_argument_max_bytes: 64 * 1024,
            agent_tool_max_calls: 128,
            direct_executions: 256,
            model_calls: 256,
            monty_min_workers: 1,
            monty_memory_bytes: 256 * MIB,
            code_timeout: Duration::from_secs(10),
            agent_code_budget: Duration::from_secs(60),
            verify_timeout: Duration::from_secs(60),
        }
    }
}
