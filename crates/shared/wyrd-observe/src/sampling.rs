//! Sampling decisions for high-volume observer events.

use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::RunId;

/// Sampling policy for per-iteration and per-tool-call observations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SamplingPolicy {
    /// Emit every event.
    #[default]
    All,
    /// Emit one event out of every non-zero divisor.
    ///
    /// The [`NonZeroU32`] wrapper prevents zero-modulo sampling.
    OneOf(NonZeroU32),
    /// Emit events for runs assigned to the kept bucket range.
    Bucket {
        /// Number of buckets.
        ///
        /// The [`NonZeroU32`] wrapper prevents zero-modulo sampling.
        buckets: NonZeroU32,
        /// Number of buckets to keep. Zero drops all bucket-sampled events.
        keep: u32,
    },
}

impl SamplingPolicy {
    /// Build a one-of-N sampling policy.
    ///
    /// Returns `None` when `n` is zero because modulo sampling requires a
    /// non-zero divisor.
    #[must_use]
    pub fn one_of(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self::OneOf)
    }

    /// Build a deterministic bucket sampling policy.
    ///
    /// Returns `None` when `buckets` is zero because modulo bucket assignment
    /// requires a non-zero divisor.
    #[must_use]
    pub fn bucket(buckets: u32, keep: u32) -> Option<Self> {
        NonZeroU32::new(buckets).map(|buckets| Self::Bucket { buckets, keep })
    }

    /// Decide whether to emit an iteration observation.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use wyrd_observe::{RunId, SamplingPolicy};
    ///
    /// let run_id = RunId::from_string("run-1".to_owned());
    /// let Some(policy) = SamplingPolicy::one_of(2) else {
    ///     unreachable!("2 is a non-zero divisor");
    /// };
    ///
    /// assert!(policy.should_sample_iteration(&run_id, 2));
    /// assert!(!policy.should_sample_iteration(&run_id, 3));
    /// ```
    #[must_use]
    pub fn should_sample_iteration(&self, run_id: &RunId, iteration: u32) -> bool {
        match self {
            Self::All => true,
            Self::OneOf(n) => iteration.is_multiple_of(n.get()),
            Self::Bucket { buckets, keep } => run_id.hash_bucket(buckets.get()) < *keep,
        }
    }

    /// Decide whether to emit a tool-call observation.
    ///
    /// Critical write-like tools are always sampled before the general policy
    /// is applied.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use wyrd_observe::{RunId, SamplingPolicy};
    ///
    /// let run_id = RunId::from_string("run-1".to_owned());
    /// let Some(policy) = SamplingPolicy::bucket(10, 0) else {
    ///     unreachable!("10 is a non-zero bucket count");
    /// };
    ///
    /// assert!(policy.should_sample_tool_call(&run_id, "execute_code"));
    /// assert!(!policy.should_sample_tool_call(&run_id, "search"));
    /// ```
    #[must_use]
    pub fn should_sample_tool_call(&self, run_id: &RunId, tool: &str) -> bool {
        if tool.ends_with("_write") || tool == "execute_code" {
            return true;
        }
        match self {
            Self::All => true,
            Self::OneOf(n) => {
                static COUNTER: AtomicU64 = AtomicU64::new(0);
                let next = COUNTER.fetch_add(1, Ordering::Relaxed);
                (next as u32).is_multiple_of(n.get())
            }
            Self::Bucket { buckets, keep } => run_id.hash_bucket(buckets.get()) < *keep,
        }
    }
}
