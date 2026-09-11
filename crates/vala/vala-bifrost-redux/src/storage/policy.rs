//! The validated, immutable policy one Bifrost storage owner applies.

use std::time::Duration;

use crate::storage::error::BifrostStorageError;

/// One mebibyte, for the metadata-cache bounds below.
const MIB: u64 = 1024 * 1024;

/// The configured, still-unvalidated storage-I/O scalars.
///
/// Every field is optional because every field has a defensible default, and a
/// deployment that has no opinion about object-store retry behaviour should not
/// be made to invent one. Validation and the Oracle cache resolution happen in
/// [`BifrostStoragePolicy::resolve`], once, at boot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BifrostStorageConfig {
    /// Per-attempt request timeout in milliseconds.
    pub request_timeout_ms: Option<u64>,
    /// Additional attempts a bounded read may make after its first.
    pub max_retries: Option<u32>,
    /// Wall-clock ceiling on a complete retry sequence, in milliseconds.
    pub max_retry_elapsed_ms: Option<u64>,
    /// Coarse ceiling on concurrent backend requests for the whole node.
    pub max_concurrent_requests: Option<usize>,
    /// Charged-byte ceiling for decoded metadata.
    ///
    /// `Some(0)` disables retention explicitly. `None` means "decide for me",
    /// which for an Oracle-serving node is a share of managed memory and for
    /// every other node is nothing at all.
    pub metadata_cache_bytes: Option<u64>,
}

/// One storage policy, immutable for the owner's lifetime.
///
/// `Copy` rather than `Arc`-shared: it is five scalars, it never changes after
/// boot, and every read path wants it without an indirection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BifrostStoragePolicy {
    /// Per-attempt request timeout applied by the operator's timeout layer.
    request_timeout: Duration,
    /// Additional attempts a bounded read may make after its first.
    max_retries: u32,
    /// Wall-clock ceiling on a complete retry sequence, not one attempt.
    max_retry_elapsed: Duration,
    /// Coarse ceiling on concurrent backend requests for the whole node.
    max_concurrent_requests: usize,
    /// Charged-byte ceiling for decoded metadata; zero disables retention.
    metadata_cache_bytes: u64,
}

impl BifrostStoragePolicy {
    /// Validates one configuration into the policy an owner may apply.
    ///
    /// Every bound is checked here, at boot, rather than at each use: a node
    /// that cannot form a coherent storage policy must fail before it is
    /// readable, not serve queries under a silently clamped one. `managed_bytes`
    /// is the process's managed-memory total, which the metadata cache is a
    /// share of and never a peer to.
    ///
    /// An absent cache value on an Oracle-serving node resolves to a modest
    /// share of managed memory — enough that repeated scans of one cut stop
    /// re-decoding, small enough that footers never crowd out the queries they
    /// exist to speed up. Every other node resolves to no cache at all,
    /// because it reads no hot footers.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::InvalidConfiguration`] when a scalar is
    /// outside its bound, when the retry ceiling is shorter than one attempt's
    /// timeout, or when the requested cache cannot fit the managed-memory
    /// share it must come from.
    pub fn resolve(
        config: BifrostStorageConfig,
        managed_bytes: u64,
        serves_oracle: bool,
    ) -> Result<Self, BifrostStorageError> {
        let invalid = |detail: &str| BifrostStorageError::InvalidConfiguration {
            detail: detail.to_owned(),
        };
        let request_timeout_ms = config.request_timeout_ms.unwrap_or(30_000);
        if !(100..=300_000).contains(&request_timeout_ms) {
            return Err(invalid(
                "storage request timeout must be between 100ms and 300s",
            ));
        }
        let max_retries = config.max_retries.unwrap_or(3);
        if max_retries > 10 {
            return Err(invalid("storage retries must not exceed 10"));
        }
        let max_retry_elapsed_ms = config.max_retry_elapsed_ms.unwrap_or(60_000);
        if !(100..=300_000).contains(&max_retry_elapsed_ms) {
            return Err(invalid(
                "storage retry elapsed ceiling must be between 100ms and 300s",
            ));
        }
        if max_retry_elapsed_ms < request_timeout_ms {
            return Err(invalid(
                "storage retry elapsed ceiling must cover one request timeout",
            ));
        }
        let max_concurrent_requests = config.max_concurrent_requests.unwrap_or(128);
        if !(1..=1024).contains(&max_concurrent_requests) {
            return Err(invalid(
                "storage request concurrency must be between 1 and 1024",
            ));
        }
        let metadata_cache_bytes =
            Self::resolve_cache_bytes(config.metadata_cache_bytes, managed_bytes, serves_oracle)?;
        Ok(Self {
            request_timeout: Duration::from_millis(request_timeout_ms),
            max_retries,
            max_retry_elapsed: Duration::from_millis(max_retry_elapsed_ms),
            max_concurrent_requests,
            metadata_cache_bytes,
        })
    }

    /// Resolves the decoded-metadata ceiling against managed memory.
    ///
    /// The one-eighth share is the hard rule in both directions: it caps what
    /// an operator may ask for and what an absent value derives to, so no
    /// configuration produces a node whose footers can crowd out its queries.
    ///
    /// # Errors
    /// Returns [`BifrostStorageError::InvalidConfiguration`] when a requested
    /// ceiling is outside 1 MiB..=1 GiB, exceeds the managed-memory share, or
    /// when an Oracle node's managed memory cannot fund the minimum cache.
    fn resolve_cache_bytes(
        requested: Option<u64>,
        managed_bytes: u64,
        serves_oracle: bool,
    ) -> Result<u64, BifrostStorageError> {
        let invalid = |detail: &str| BifrostStorageError::InvalidConfiguration {
            detail: detail.to_owned(),
        };
        let share = managed_bytes / 8;
        match requested {
            Some(0) => Ok(0),
            Some(bytes) => {
                if !(MIB..=1024 * MIB).contains(&bytes) {
                    return Err(invalid(
                        "decoded-metadata cache must be between 1MiB and 1GiB, or zero",
                    ));
                }
                if bytes > share {
                    return Err(invalid(
                        "decoded-metadata cache must not exceed an eighth of managed memory",
                    ));
                }
                Ok(bytes)
            }
            None if !serves_oracle => Ok(0),
            None => {
                let derived = (managed_bytes / 20).clamp(32 * MIB, 512 * MIB).min(share);
                if derived < MIB {
                    return Err(invalid(
                        "managed memory cannot fund an Oracle decoded-metadata cache",
                    ));
                }
                Ok(derived)
            }
        }
    }

    /// Returns the per-attempt request timeout.
    #[must_use]
    pub const fn request_timeout(self) -> Duration {
        self.request_timeout
    }

    /// Returns the additional attempts a bounded read may make.
    #[must_use]
    pub const fn max_retries(self) -> u32 {
        self.max_retries
    }

    /// Returns the wall-clock ceiling on one complete retry sequence.
    #[must_use]
    pub const fn max_retry_elapsed(self) -> Duration {
        self.max_retry_elapsed
    }

    /// Returns the coarse concurrent-request ceiling for the whole node.
    #[must_use]
    pub const fn max_concurrent_requests(self) -> usize {
        self.max_concurrent_requests
    }

    /// Returns the charged-byte ceiling for decoded metadata.
    ///
    /// Zero is the explicit disabled composition: the owner constructs no
    /// resident cache at all and every lookup bypasses retention.
    #[must_use]
    pub const fn metadata_cache_bytes(self) -> u64 {
        self.metadata_cache_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a configuration with only the named cache value set.
    fn cache_config(metadata_cache_bytes: Option<u64>) -> BifrostStorageConfig {
        BifrostStorageConfig {
            metadata_cache_bytes,
            ..BifrostStorageConfig::default()
        }
    }

    /// An unconfigured node resolves usable defaults, an Oracle node derives a
    /// bounded share of managed memory, a non-Oracle node derives none, and
    /// every out-of-bound or incoherent scalar is refused rather than clamped.
    ///
    /// Asserted as one scenario because these are one decision: what a node is
    /// allowed to spend on storage I/O. Splitting the defaults from the refusals
    /// would let an implementation that silently clamps an impossible value
    /// still look correct from the defaults' side.
    ///
    /// # Panics
    /// Panics when a resolution or refusal assertion fails.
    #[test]
    fn storage_policy_resolves_defaults_and_refuses_incoherent_configuration() {
        let managed = 8 * 1024 * MIB;
        let defaults =
            BifrostStoragePolicy::resolve(BifrostStorageConfig::default(), managed, true)
                .expect("an unconfigured Oracle node resolves");
        assert_eq!(defaults.request_timeout(), Duration::from_secs(30));
        assert_eq!(defaults.max_retries(), 3);
        assert_eq!(defaults.max_retry_elapsed(), Duration::from_mins(1));
        assert_eq!(defaults.max_concurrent_requests(), 128);
        // A twentieth of 8GiB is 409MiB, inside the 32MiB..=512MiB band and
        // well under the one-eighth share.
        assert_eq!(defaults.metadata_cache_bytes(), managed / 20);

        let huge =
            BifrostStoragePolicy::resolve(BifrostStorageConfig::default(), 512 * 1024 * MIB, true)
                .expect("a large Oracle node resolves");
        assert_eq!(
            huge.metadata_cache_bytes(),
            512 * MIB,
            "the derived share is capped rather than growing with the node"
        );

        let scribe_only =
            BifrostStoragePolicy::resolve(BifrostStorageConfig::default(), managed, false)
                .expect("an unconfigured non-Oracle node resolves");
        assert_eq!(
            scribe_only.metadata_cache_bytes(),
            0,
            "a node that reads no hot footer spends nothing on decoded metadata"
        );

        assert_eq!(
            BifrostStoragePolicy::resolve(cache_config(Some(0)), managed, true)
                .expect("an explicit zero resolves")
                .metadata_cache_bytes(),
            0,
            "zero is an explicit disabled composition, not an absent value"
        );

        for refused in [
            cache_config(Some(MIB - 1)),
            cache_config(Some(2048 * MIB)),
            // Inside the absolute band, but more than an eighth of a 4MiB node.
            BifrostStorageConfig {
                metadata_cache_bytes: Some(4 * MIB),
                ..BifrostStorageConfig::default()
            },
            BifrostStorageConfig {
                request_timeout_ms: Some(99),
                ..BifrostStorageConfig::default()
            },
            BifrostStorageConfig {
                max_retries: Some(11),
                ..BifrostStorageConfig::default()
            },
            BifrostStorageConfig {
                request_timeout_ms: Some(30_000),
                max_retry_elapsed_ms: Some(1_000),
                ..BifrostStorageConfig::default()
            },
            BifrostStorageConfig {
                max_concurrent_requests: Some(0),
                ..BifrostStorageConfig::default()
            },
        ] {
            let managed = if refused.metadata_cache_bytes == Some(4 * MIB) {
                16 * MIB
            } else {
                managed
            };
            let error = BifrostStoragePolicy::resolve(refused, managed, true)
                .expect_err("an incoherent configuration is refused");
            assert!(matches!(
                error,
                BifrostStorageError::InvalidConfiguration { .. }
            ));
        }
    }
}
