//! Low-cardinality Gate counters for admission and dispatch diagnostics.

use std::sync::atomic::{AtomicU64, Ordering};

/// Point-in-time Gate counters.
///
/// The counters intentionally contain no token, payload, or observation data.
/// Tenant, table, and request correlation belong in the structured span for a
/// single request, not in an unbounded metric label.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GateTelemetrySnapshot {
    /// Authentication attempts received by a mounted Gate.
    pub auth_attempts: u64,
    /// Authentication attempts rejected before body processing.
    pub auth_rejections: u64,
    /// Native frames entering Gate dispatch.
    pub native_frames: u64,
    /// Native frames rejected by Gate or Scribe.
    pub native_rejections: u64,
    /// OTLP exports entering Gate dispatch.
    pub otlp_exports: u64,
    /// OTLP exports rejected by projection, catalog, or Scribe.
    pub otlp_rejections: u64,
    /// Catalog lookups that failed.
    pub catalog_failures: u64,
    /// Projection jobs that failed before Scribe dispatch.
    pub projection_failures: u64,
    /// Scribe dispatches that failed after Gate validation.
    pub scribe_failures: u64,
    /// Rows accepted by Scribe from OTLP exports.
    pub accepted_rows: u64,
    /// Rows rejected by OTLP projection.
    pub rejected_rows: u64,
    /// Successful close transitions.
    pub close_events: u64,
}

#[derive(Debug, Default)]
pub struct GateTelemetry {
    auth_attempts: AtomicU64,
    auth_rejections: AtomicU64,
    native_frames: AtomicU64,
    native_rejections: AtomicU64,
    otlp_exports: AtomicU64,
    otlp_rejections: AtomicU64,
    catalog_failures: AtomicU64,
    projection_failures: AtomicU64,
    scribe_failures: AtomicU64,
    accepted_rows: AtomicU64,
    rejected_rows: AtomicU64,
    close_events: AtomicU64,
}

impl GateTelemetry {
    pub(crate) fn auth_attempt(&self) {
        self.auth_attempts.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn auth_rejection(&self) {
        self.auth_rejections.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn native_frame(&self) {
        self.native_frames.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn native_rejection(&self) {
        self.native_rejections.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn otlp_export(&self) {
        self.otlp_exports.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn otlp_rejection(&self) {
        self.otlp_rejections.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn catalog_failure(&self) {
        self.catalog_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn projection_failure(&self) {
        self.projection_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn scribe_failure(&self) {
        self.scribe_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_otlp_rows(&self, accepted: i64, rejected: i64) {
        self.accepted_rows
            .fetch_add(u64::try_from(accepted).unwrap_or(0), Ordering::Relaxed);
        self.rejected_rows
            .fetch_add(u64::try_from(rejected).unwrap_or(0), Ordering::Relaxed);
    }

    pub(crate) fn close_event(&self) {
        self.close_events.fetch_add(1, Ordering::Relaxed);
    }

    #[must_use]
    pub fn snapshot(&self) -> GateTelemetrySnapshot {
        GateTelemetrySnapshot {
            auth_attempts: self.auth_attempts.load(Ordering::Relaxed),
            auth_rejections: self.auth_rejections.load(Ordering::Relaxed),
            native_frames: self.native_frames.load(Ordering::Relaxed),
            native_rejections: self.native_rejections.load(Ordering::Relaxed),
            otlp_exports: self.otlp_exports.load(Ordering::Relaxed),
            otlp_rejections: self.otlp_rejections.load(Ordering::Relaxed),
            catalog_failures: self.catalog_failures.load(Ordering::Relaxed),
            projection_failures: self.projection_failures.load(Ordering::Relaxed),
            scribe_failures: self.scribe_failures.load(Ordering::Relaxed),
            accepted_rows: self.accepted_rows.load(Ordering::Relaxed),
            rejected_rows: self.rejected_rows.load(Ordering::Relaxed),
            close_events: self.close_events.load(Ordering::Relaxed),
        }
    }
}
