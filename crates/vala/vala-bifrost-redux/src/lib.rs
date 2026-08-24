//! Bifrost Redux — isolated rebuild of the Bifrost ingest and query engine.
//!
//! This crate implements the multi-pod Scribe (WAL + memtable + Parquet seal),
//! Forge (single-writer Iceberg committer + compaction), Oracle (fused scan + read
//! audit), and Gate (auth + dispatch) roles without depending on `vala-bifrost`.
//!
//! Per CONTRACTS §13, this crate MUST NOT depend on `vala-bifrost` in any form.
//! Shared logic is copied, not imported.

#[cfg(feature = "bench-support")]
pub mod bench_support;
pub mod catalog;
pub mod cluster;
pub mod contracts;
pub mod forge;
pub mod gate;
pub mod maintenance;
pub mod namespaces;
pub mod oracle;
mod otlp_contract;
pub mod parquet;
pub mod provider;
pub mod resources;
pub mod schema;
pub mod scribe;
pub mod tables;

/// Exact-partition fixtures shared by unit, integration, and harness tests.
///
/// The constructors live in the library rather than in each test file so every
/// tier builds partitions through the one checked
/// [`TimePartition::new`](crate::catalog::layout::TimePartition::new) path and
/// no test invents its own boundary arithmetic.
#[cfg(any(test, feature = "test-support"))]
pub mod partition_fixtures {
    /// Builds the exact daily partition starting at `year-month-day` UTC.
    ///
    /// Tests that only need *some* valid partition use this instead of
    /// re-deriving a midnight boundary at every call site.
    ///
    /// # Panics
    /// Panics when the supplied date is not a real calendar date.
    pub fn day_partition(year: i32, month: u32, day: u32) -> crate::catalog::layout::TimePartition {
        let start = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .expect("fixture date is a real calendar date")
            .and_hms_opt(0, 0, 0)
            .expect("midnight is a valid time")
            .and_utc();
        crate::catalog::layout::TimePartition::new(
            crate::catalog::layout::TimeGranularity::Day,
            start,
        )
        .expect("midnight is a daily partition boundary")
    }

    /// Builds the exact hourly partition starting at `year-month-day hour` UTC.
    ///
    /// # Panics
    /// Panics when the supplied date or hour is not a real UTC instant.
    pub fn hour_partition(
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
    ) -> crate::catalog::layout::TimePartition {
        let start = chrono::NaiveDate::from_ymd_opt(year, month, day)
            .expect("fixture date is a real calendar date")
            .and_hms_opt(hour, 0, 0)
            .expect("fixture hour is a valid time")
            .and_utc();
        crate::catalog::layout::TimePartition::new(
            crate::catalog::layout::TimeGranularity::Hour,
            start,
        )
        .expect("a whole hour is an hourly partition boundary")
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    pub(crate) use crate::partition_fixtures::{day_partition, hour_partition};
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    use wyrd_spec::DataTenantId;

    pub(crate) fn tenant() -> DataTenantId {
        static TENANT: OnceLock<DataTenantId> = OnceLock::new();
        *TENANT.get_or_init(DataTenantId::new_v7)
    }

    pub(crate) fn nil_tenant() -> DataTenantId {
        DataTenantId::SYSTEM_OWNER
    }

    /// Captures exact span field updates and closure for lifecycle unit tests.
    #[derive(Default)]
    pub(crate) struct SpanCaptureSubscriber {
        /// Monotonic identifier source for captured spans.
        next: AtomicU64,
        /// Metadata retained until each span closes.
        metadata: Mutex<HashMap<u64, &'static tracing::Metadata<'static>>>,
        /// Exact field updates paired with their owning span name.
        pub(crate) records: Arc<Mutex<Vec<(String, String, String)>>>,
        /// Exact names observed when spans release their final reference.
        pub(crate) closed: Arc<Mutex<Vec<String>>>,
    }

    /// Records one span field into the shared capture.
    struct SpanFieldVisitor<'a> {
        /// Exact owning span name.
        span: &'a str,
        /// Shared terminal-record sink.
        records: &'a Mutex<Vec<(String, String, String)>>,
    }

    impl tracing::field::Visit for SpanFieldVisitor<'_> {
        /// Retains string fields without debug quoting.
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.records.lock().expect("span records").push((
                self.span.to_owned(),
                field.name().to_owned(),
                value.to_owned(),
            ));
        }

        /// Retains non-string fields in tracing's canonical debug form.
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.records.lock().expect("span records").push((
                self.span.to_owned(),
                field.name().to_owned(),
                format!("{value:?}"),
            ));
        }
    }

    impl tracing::Subscriber for SpanCaptureSubscriber {
        /// Enables every span emitted inside the scoped test subscriber.
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }

        /// Retains metadata for one newly created span.
        fn new_span(&self, attributes: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
            self.metadata
                .lock()
                .expect("span metadata")
                .insert(id, attributes.metadata());
            tracing::span::Id::from_u64(id)
        }

        /// Captures exact field updates for one live span.
        fn record(&self, id: &tracing::span::Id, record: &tracing::span::Record<'_>) {
            let span = self.metadata.lock().expect("span metadata")[&id.into_u64()]
                .name()
                .to_owned();
            record.record(&mut SpanFieldVisitor {
                span: &span,
                records: &self.records,
            });
        }

        /// Ignores causal links because lifecycle tests assert ownership directly.
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}

        /// Ignores events because lifecycle tests assert span fields and closure.
        fn event(&self, _: &tracing::Event<'_>) {}

        /// Ignores span entry because lifecycle ownership is reference based.
        fn enter(&self, _: &tracing::span::Id) {}

        /// Ignores span exit because lifecycle ownership is reference based.
        fn exit(&self, _: &tracing::span::Id) {}

        /// Records the exact name when a span releases its final reference.
        fn try_close(&self, id: tracing::span::Id) -> bool {
            if let Some(metadata) = self
                .metadata
                .lock()
                .expect("span metadata")
                .remove(&id.into_u64())
            {
                self.closed
                    .lock()
                    .expect("closed spans")
                    .push(metadata.name().to_owned());
            }
            true
        }
    }

    /// Returns whether one exact outcome update was captured for a named span.
    pub(crate) fn has_span_outcome(
        records: &Mutex<Vec<(String, String, String)>>,
        span: &str,
        outcome: &str,
    ) -> bool {
        records
            .lock()
            .expect("span records")
            .iter()
            .any(|record| record == &(span.to_owned(), "outcome".to_owned(), outcome.to_owned()))
    }
}
