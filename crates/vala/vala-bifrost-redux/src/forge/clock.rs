//! Concrete wall-clock ownership for Forge maintenance decisions.

#[cfg(feature = "test-support")]
use std::sync::Arc;
#[cfg(feature = "test-support")]
use std::sync::atomic::{AtomicI64, Ordering};

#[cfg(feature = "test-support")]
use chrono::Duration;
use chrono::{DateTime, Utc};

use super::error::ForgeError;

/// Supplies the one wall-clock instant captured by a Forge work batch.
#[derive(Clone)]
pub struct ForgeClock {
    /// Concrete production or manually controlled source.
    source: ForgeClockSource,
}

/// Closed implementation set for Forge wall time.
#[derive(Clone)]
enum ForgeClockSource {
    /// Reads the host UTC clock.
    System,
    /// Reads a test-owned UTC millisecond epoch.
    #[cfg(feature = "test-support")]
    Manual(Arc<AtomicI64>),
}

/// Moves a test Forge clock forward without controlling Tokio time.
#[cfg(feature = "test-support")]
#[derive(Clone)]
pub struct ForgeClockControl {
    /// Shared epoch consumed by every Forge rebuilt from the same fixture.
    epoch_millis: Arc<AtomicI64>,
}

impl ForgeClock {
    /// Creates the production clock backed by `Utc::now`.
    #[must_use]
    pub fn system() -> Self {
        Self {
            source: ForgeClockSource::System,
        }
    }

    /// Returns the current validated UTC instant.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when a manually controlled epoch
    /// cannot be represented as a UTC timestamp.
    pub fn now(&self) -> Result<DateTime<Utc>, ForgeError> {
        match &self.source {
            ForgeClockSource::System => Ok(Utc::now()),
            #[cfg(feature = "test-support")]
            ForgeClockSource::Manual(epoch_millis) => DateTime::from_timestamp_millis(
                epoch_millis.load(Ordering::Acquire),
            )
            .ok_or_else(|| ForgeError::InvalidConfig {
                detail: "Forge manual clock epoch is out of range".to_owned(),
            }),
        }
    }

    /// Creates a manual clock and its monotonic test-only control.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn manual(initial: DateTime<Utc>) -> (Self, ForgeClockControl) {
        let epoch_millis = Arc::new(AtomicI64::new(initial.timestamp_millis()));
        (
            Self {
                source: ForgeClockSource::Manual(Arc::clone(&epoch_millis)),
            },
            ForgeClockControl { epoch_millis },
        )
    }
}

#[cfg(feature = "test-support")]
impl ForgeClockControl {
    /// Returns the manually controlled UTC instant without advancing it.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when the stored epoch cannot be
    /// represented as a UTC timestamp.
    pub fn now(&self) -> Result<DateTime<Utc>, ForgeError> {
        DateTime::from_timestamp_millis(self.epoch_millis.load(Ordering::Acquire)).ok_or_else(
            || ForgeError::InvalidConfig {
                detail: "Forge manual clock epoch is out of range".to_owned(),
            },
        )
    }

    /// Sets the shared manual clock without permitting time to move backward.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] when `now` precedes the observed
    /// time or a concurrent update cannot preserve monotonicity.
    pub fn set(&self, now: DateTime<Utc>) -> Result<(), ForgeError> {
        let requested = now.timestamp_millis();
        let mut observed = self.epoch_millis.load(Ordering::Acquire);
        loop {
            if requested < observed {
                return Err(ForgeError::InvalidConfig {
                    detail: "Forge manual clock cannot move backward".to_owned(),
                });
            }
            match self.epoch_millis.compare_exchange(
                observed,
                requested,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(()),
                Err(actual) => observed = actual,
            }
        }
    }

    /// Advances the shared manual clock by a positive checked duration.
    ///
    /// # Errors
    ///
    /// Returns [`ForgeError::InvalidConfig`] for zero, unrepresentable, or
    /// overflowing advances.
    pub fn advance(&self, by: Duration) -> Result<DateTime<Utc>, ForgeError> {
        if by == Duration::zero() {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge manual clock advance must be positive".to_owned(),
            });
        }
        let millis = by.num_milliseconds();
        if millis <= 0 {
            return Err(ForgeError::InvalidConfig {
                detail: "Forge manual clock advance must contain milliseconds".to_owned(),
            });
        }
        let mut observed = self.epoch_millis.load(Ordering::Acquire);
        loop {
            let next = observed
                .checked_add(millis)
                .ok_or_else(|| ForgeError::InvalidConfig {
                    detail: "Forge manual clock advance overflows epoch milliseconds".to_owned(),
                })?;
            match self.epoch_millis.compare_exchange(
                observed,
                next,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return DateTime::from_timestamp_millis(next).ok_or_else(|| {
                        ForgeError::InvalidConfig {
                            detail: "Forge manual clock epoch is out of range".to_owned(),
                        }
                    });
                }
                Err(actual) => observed = actual,
            }
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::ForgeClock;

    /// Proves the test-only clock is monotonic and produces stable batch time.
    #[test]
    fn forge_clock_control_is_monotonic_and_tick_consistent() {
        let initial = chrono::Utc
            .with_ymd_and_hms(2026, 7, 30, 0, 0, 0)
            .single()
            .expect("valid instant");
        let (clock, control) = ForgeClock::manual(initial);
        assert_eq!(clock.now().expect("initial clock"), initial);
        let advanced = control
            .advance(Duration::seconds(1))
            .expect("forward advance");
        assert_eq!(clock.now().expect("advanced clock"), advanced);
        assert!(control.set(initial).is_err());
        assert_eq!(clock.now().expect("stable clock"), advanced);
    }
}
