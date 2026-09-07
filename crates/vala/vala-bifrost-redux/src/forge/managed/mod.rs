//! Forge's managed rewrite: plan and produce candidate objects, publish nothing.
//!
//! This is the one place Bifrost hands a table to the pinned managed
//! compaction core and takes back a set of objects that exist in storage and
//! belong to no snapshot. Everything the core needs — the policy, the leased
//! runtime, the memory pool, the scratch root, the cancellation token, the
//! observer, and the attempt identity that names every object it writes —
//! is bound here, before any object IO, and nothing about publication is.
//!
//! The split matters because publication is the only irreversible step. Task 2
//! owns it, through Scribe promotion, and it is a different transaction with a
//! different failure model. A rewrite that ends here leaves objects that are
//! reclaimable by construction: they are named for their attempt, they are
//! under the recipe path, and no snapshot references them.

pub(crate) mod executor;
pub(crate) mod fingerprint;
pub(crate) mod handoff;
pub(crate) mod identity;
pub(crate) mod memory;
pub(crate) mod observer;
pub(crate) mod policy;
pub(crate) mod queue;

pub use executor::ForgeRewriteAttempt;
pub use fingerprint::{ForgeRewriteEvidence, ForgeRewriteOutcome, ForgeUnsettledOutput};
pub use handoff::RewriteHandoff;
