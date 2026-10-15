//! Server-hosted Verifier block capabilities: format extraction, bounded
//! per-call workspaces, citation validation, the Python sandbox, and the
//! built-in Agent tools.
//!
//! These are the concrete implementations behind the
//! [`vala_eval::blocks`] executor seams. They hold no request state of their
//! own: each execution composes them with its accepted authority and pinned
//! evidence, and every limit comes from [`HostedLimits`].

pub mod evidence;
pub mod extract;

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
        }
    }
}
