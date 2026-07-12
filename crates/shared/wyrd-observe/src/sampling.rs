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

#[cfg(test)]
mod sampling_tests {
    use std::num::NonZeroU32;

    use crate::{RunId, SamplingPolicy};

    #[test]
    fn one_of_constructor_rejects_zero() {
        assert!(SamplingPolicy::one_of(0).is_none());
        assert!(SamplingPolicy::one_of(1).is_some());
        assert!(SamplingPolicy::one_of(1_000).is_some());
    }

    #[test]
    fn bucket_constructor_rejects_zero_buckets() {
        assert!(SamplingPolicy::bucket(0, 5).is_none());
        assert!(SamplingPolicy::bucket(100, 0).is_some());
        assert!(SamplingPolicy::bucket(100, 100).is_some());
    }

    #[test]
    fn one_of_n_emits_every_nth_iteration() {
        let policy = SamplingPolicy::one_of(3).expect("3 is non-zero");
        let run_id = RunId::from_string("r1".to_owned());

        assert!(policy.should_sample_iteration(&run_id, 0));
        assert!(!policy.should_sample_iteration(&run_id, 1));
        assert!(!policy.should_sample_iteration(&run_id, 2));
        assert!(policy.should_sample_iteration(&run_id, 3));
        assert!(policy.should_sample_iteration(&run_id, 6));
    }

    #[test]
    fn bucket_keep_zero_drops_all() {
        let policy = SamplingPolicy::bucket(100, 0).expect("100 is non-zero");
        let run_id = RunId::from_string("r1".to_owned());

        assert!(!policy.should_sample_iteration(&run_id, 1));
        assert!(!policy.should_sample_iteration(&run_id, 2));
        assert!(!policy.should_sample_iteration(&run_id, 999));
    }

    #[test]
    fn bucket_keep_equal_buckets_emits_all() {
        let policy = SamplingPolicy::bucket(100, 100).expect("100 is non-zero");
        let run_id = RunId::from_string("r1".to_owned());

        assert!(policy.should_sample_iteration(&run_id, 1));
    }

    #[test]
    fn one_of_one_emits_every_event() {
        let policy = SamplingPolicy::OneOf(NonZeroU32::new(1).expect("1 is non-zero"));
        let run_id = RunId::from_string("r1".to_owned());

        for iteration in 0..10 {
            assert!(policy.should_sample_iteration(&run_id, iteration));
        }
    }

    #[test]
    fn critical_tool_always_samples_regardless_of_policy() {
        let policy = SamplingPolicy::one_of(1_000_000).expect("1_000_000 is non-zero");
        let run_id = RunId::from_string("r1".to_owned());

        assert!(policy.should_sample_tool_call(&run_id, "database_write"));
        assert!(policy.should_sample_tool_call(&run_id, "execute_code"));
    }

    #[test]
    fn run_id_bucket_is_deterministic() {
        let id = RunId::from_string("r1".to_owned());

        assert_eq!(id.hash_bucket(100), id.hash_bucket(100));
    }

    #[test]
    fn run_id_bucket_pins_sha256_value() {
        let id = RunId::from_string("r1".to_owned());

        assert_eq!(id.hash_bucket(100), 41);
    }

    #[test]
    fn run_id_bucket_zero_buckets_returns_zero() {
        let id = RunId::from_string("r1".to_owned());

        assert_eq!(id.hash_bucket(0), 0);
    }

    #[test]
    fn one_of_tool_call_successive_calls_alternate() {
        let run_id = RunId::from_string("tool-run".to_owned());
        let policy = SamplingPolicy::one_of(2).expect("2 is non-zero");

        let first = policy.should_sample_tool_call(&run_id, "search");
        let second = policy.should_sample_tool_call(&run_id, "search");

        // OneOf(2) must alternate: the two results must differ.
        assert_ne!(
            first, second,
            "OneOf(2) must alternate between consecutive tool-call checks"
        );
    }

    #[test]
    fn one_of_tool_call_write_tool_always_sampled() {
        let run_id = RunId::from_string("tool-run".to_owned());
        // Divisor large enough that non-write tools would almost never be sampled.
        let policy = SamplingPolicy::one_of(1_000_000).expect("non-zero");

        for _ in 0..5 {
            assert!(
                policy.should_sample_tool_call(&run_id, "file_write"),
                "write tool must always be sampled regardless of OneOf divisor"
            );
            assert!(
                policy.should_sample_tool_call(&run_id, "execute_code"),
                "execute_code must always be sampled regardless of OneOf divisor"
            );
        }
    }
}
