//! Fits pending Drift baselines from their exact Data Card artifacts.
//!
//! [`BaselineFitter`] is the one implementation-specific background helper of
//! the verification runtime. Each pass lists tenants with a due fit through
//! the operator pool, claims one fit per tenant under a PostgreSQL-clock lease
//! through [`DriftBaselineQueue`], resolves the exact Verifier and baseline
//! Data Card versions the row pins, reads that Data Card's registered
//! `data/data.parquet` artifact from storage, fits it with the existing
//! `vala-drift` fitter off the async runtime, and settles the same row
//! `ready` or `failed`. Every fit holds one slot of the Verifier permits the
//! runner shares, taken before the claim and kept through settlement, so fits
//! and runs together respect the global and per-tenant ceilings; many
//! processes share the queue through `SKIP LOCKED` claims. Decoding stops at a
//! decoded-byte budget, and the blocking decode and fit observe shutdown and
//! the execution timeout, which the fitter awaits before it settles the lease.

use std::sync::Arc;
use std::time::Duration;

use arrow::array::RecordBatch;
use arrow::compute::concat_batches;
use datafusion::parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use datafusion::parquet::file::reader::ChunkReader;
use serde_json::Value;
use tokio_util::sync::CancellationToken;
use wyrd_spec::DataTenantId;
use wyrd_spec::card::drift::DriftSpec;
use wyrd_spec::card::verifier::VerifierImplementation;
use wyrd_spec::envelope::Spec;
use wyrd_spec::ids::CardUid;
use wyrd_spec::verification::VerificationError;
use wyrd_sql::queries::cards::get_card_by_uid;
use wyrd_sql::queries::drift_baselines::{ClaimedBaseline, DriftBaselineQueue};
use wyrd_sql::queries::storage::artifact_metadata;
use wyrd_sql::row_types::cards::CardStatus;
use wyrd_sql::{OperatorPool, SqlError, WyrdPostgres};
use wyrd_storage::StorageHandle;
use wyrd_storage::tenant_path;

#[cfg(feature = "test-support")]
use super::CapabilityCrash;
#[cfg(feature = "test-support")]
use super::health::RuntimeCapability;
use super::permits::VerifierPermits;

/// Stable error code when the pinned Verifier or Data Card cannot be used.
pub const BASELINE_DATA_UNAVAILABLE: &str = "baseline_data_unavailable";
/// Stable error code when the Data Card has no usable Parquet artifact.
pub const BASELINE_ARTIFACT_INVALID: &str = "baseline_artifact_invalid";
/// Stable error code when the existing fitter rejects the baseline data.
pub const BASELINE_FIT_FAILED: &str = "baseline_fit_failed";

/// The registered artifact every Parquet-stored Data interface writes.
const DATA_ARTIFACT: &str = "data/data.parquet";
/// Largest baseline artifact one fit reads into memory.
// ponytail: whole-file in-memory fit; stream row groups if baselines outgrow this.
const MAX_ARTIFACT_BYTES: i64 = 256 * 1024 * 1024;
/// Largest decoded Arrow size one fit may hold, checked batch by batch.
const MAX_DECODED_BYTES: usize = 256 * 1024 * 1024;
/// Tenants examined per pass, longest-waiting first.
const TENANTS_PER_PASS: i64 = 64;

/// Build a structured fit failure.
fn fit_error(code: &str, message: impl Into<String>) -> VerificationError {
    VerificationError {
        code: code.to_owned(),
        message: message.into(),
    }
}

/// Owner of the baseline fit loop.
pub struct BaselineFitter {
    /// Wyrd Postgres owner for tenant claims, Card reads, and settlements.
    postgres: WyrdPostgres,
    /// Operator pool for the cross-tenant due list.
    operator: OperatorPool,
    /// Artifact storage the Data Card's Parquet is read from.
    storage: Arc<StorageHandle>,
    /// Baseline queue transitions and retry policy.
    queue: DriftBaselineQueue,
    /// How long one claim holds a fit before another process may reclaim it.
    lease: chrono::Duration,
    /// Idle wait between passes.
    poll_interval: Duration,
    /// Global and per-tenant capacity shared with the Verifier runner.
    permits: Arc<VerifierPermits>,
    /// Deadline of one fit; exceeding it stops the fit and fails the row.
    execution_timeout: Duration,
    /// Test-only crash switch.
    #[cfg(feature = "test-support")]
    crash: Option<CapabilityCrash>,
}

impl BaselineFitter {
    /// Build a fitter over the Wyrd Postgres owner, the operator pool, and storage.
    ///
    /// The fitter carries no coordination clock: due times, leases, and retry
    /// deadlines are PostgreSQL's; `lease` is only a length and
    /// `poll_interval` only this process's idle wait. `permits` is the one
    /// capacity owner the Verifier runner also draws from.
    #[must_use]
    pub fn new(
        postgres: WyrdPostgres,
        operator: OperatorPool,
        storage: Arc<StorageHandle>,
        permits: Arc<VerifierPermits>,
        lease: Duration,
        execution_timeout: Duration,
        poll_interval: Duration,
    ) -> Self {
        Self {
            postgres,
            operator,
            storage,
            queue: DriftBaselineQueue::default(),
            lease: chrono::Duration::from_std(lease).unwrap_or(chrono::Duration::MAX),
            poll_interval,
            permits,
            execution_timeout,
            #[cfg(feature = "test-support")]
            crash: None,
        }
    }

    /// Let `crash` panic this fitter's loop.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub fn with_crash(mut self, crash: CapabilityCrash) -> Self {
        self.crash = Some(crash);
        self
    }

    /// Run passes every poll interval until `stop` is cancelled.
    ///
    /// A failed pass is logged and retried on the next interval; the loop
    /// only returns on `stop`.
    ///
    /// # Panics
    /// Panics under `test-support` when a test armed a fitter crash.
    ///
    /// # Cancellation
    /// A fit in progress when `stop` fires is released back to `pending` with
    /// its attempt refunded.
    pub async fn run(self: Arc<Self>, stop: CancellationToken) {
        loop {
            #[cfg(feature = "test-support")]
            if let Some(crash) = &self.crash {
                crash.check(RuntimeCapability::Fitter);
            }
            if let Err(error) = self.pass(&stop).await {
                tracing::warn!(%error, "drift baseline fit pass failed");
            }
            tokio::select! {
                () = stop.cancelled() => return,
                () = tokio::time::sleep(self.poll_interval) => {}
            }
        }
    }

    /// Fit at most one due baseline of every tenant with due work.
    ///
    /// Returns how many fits were settled. Each tenant's fit first takes a
    /// shared permit, held through settlement; a tenant at its ceiling is
    /// skipped with its row unclaimed, and the pass ends when global capacity
    /// is exhausted. A tenant whose claim or settlement fails is logged and
    /// skipped; no tenant starts once `stop` is cancelled.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the cross-tenant due list cannot be read.
    pub async fn pass(&self, stop: &CancellationToken) -> Result<usize, SqlError> {
        let tenants = self
            .queue
            .tenants_with_due_fits(&self.operator, TENANTS_PER_PASS)
            .await?;
        let mut settled = 0;
        for tenant in tenants {
            if stop.is_cancelled() || self.permits.saturated() {
                break;
            }
            let Some(_permit) = self.permits.try_acquire(tenant) else {
                continue;
            };
            match self.fit_next(tenant, stop).await {
                Ok(true) => settled += 1,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(%tenant, %error, "drift baseline fit failed to settle")
                }
            }
        }
        Ok(settled)
    }

    /// Claim, fit, and settle the next due baseline of `tenant`.
    ///
    /// The claim commits before fitting so its lease is visible to other
    /// processes. On `stop` or the execution timeout the fit is cancelled and
    /// awaited until its blocking work has returned, so nothing keeps running
    /// once the lease is settled: a stopped fit is released, a timed-out fit
    /// fails, and a finished fit is completed or failed through the claimed
    /// lease. Returns whether a fit was claimed.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the claim or settlement transaction fails;
    /// the lease then expires and the row is reclaimed.
    async fn fit_next(
        &self,
        tenant: DataTenantId,
        stop: &CancellationToken,
    ) -> Result<bool, SqlError> {
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let Some(claimed) = self.queue.claim(&mut conn, self.lease).await? else {
            return Ok(false);
        };
        conn.commit().await?;
        let cancel = CancellationToken::new();
        let mut fit = std::pin::pin!(self.fit(tenant, &claimed, &cancel));
        let interrupted = tokio::select! {
            fitted = &mut fit => Ok(fitted),
            () = stop.cancelled() => Err(None),
            () = tokio::time::sleep(self.execution_timeout) => Err(Some(fit_error(
                BASELINE_FIT_FAILED,
                "the baseline fit exceeded the execution timeout",
            ))),
        };
        let fitted = match interrupted {
            Ok(fitted) => Some(fitted),
            Err(reason) => {
                cancel.cancel();
                drop(fit.await);
                reason.map(Err)
            }
        };
        let mut conn = self.postgres.tenant_conn(tenant).await?;
        let verifier_uid = &claimed.lease.verifier_uid;
        match fitted {
            None => {
                self.queue.release(&mut conn, &claimed.lease).await?;
            }
            Some(Ok(fitted)) => {
                self.queue
                    .complete(&mut conn, &claimed.lease, &fitted)
                    .await?;
                tracing::info!(%tenant, %verifier_uid, "drift baseline fitted");
            }
            Some(Err(error)) => {
                tracing::warn!(%tenant, %verifier_uid, code = %error.code, message = %error.message, "drift baseline fit failed");
                self.queue.fail(&mut conn, &claimed.lease, &error).await?;
            }
        }
        conn.commit().await?;
        Ok(true)
    }

    /// Fit one claimed baseline and return its serialized fitted profile.
    ///
    /// The blocking decode checks `cancel` between record batches and the
    /// fit polls it before each feature and inside its loops, so cancelling
    /// makes the returned future finish promptly.
    ///
    /// # Errors
    /// Returns the structured failure the row records: an unusable Verifier
    /// or Data Card, a missing, oversized, or unreadable Parquet artifact, a
    /// decoded size over the budget, a fitter rejection, or cancellation.
    async fn fit(
        &self,
        tenant: DataTenantId,
        claimed: &ClaimedBaseline,
        cancel: &CancellationToken,
    ) -> Result<Value, VerificationError> {
        let (spec, path) = self.resolve(tenant, claimed).await?;
        let bytes = self
            .storage
            .operator()
            .read(&path)
            .await
            .map_err(|error| fit_error(BASELINE_ARTIFACT_INVALID, error.to_string()))?
            .to_bytes();
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let batch = decode_bounded(bytes, MAX_DECODED_BYTES, &cancel)?;
            let fitted = vala_drift::fit_baseline_until(&batch, &spec, &|| cancel.is_cancelled())
                .map_err(|error| fit_error(BASELINE_FIT_FAILED, error.to_string()))?;
            serde_json::to_value(&fitted)
                .map_err(|error| fit_error(BASELINE_FIT_FAILED, error.to_string()))
        })
        .await
        .map_err(|error| fit_error(BASELINE_FIT_FAILED, error.to_string()))?
    }

    /// Resolve the pinned Verifier's Drift spec and the Data Card's artifact path.
    ///
    /// The Data Card must still be readable (`active` or `deprecated`) and
    /// hold a registered `data/data.parquet` within the in-memory bound.
    ///
    /// # Errors
    /// Returns [`BASELINE_DATA_UNAVAILABLE`] for an unreadable or wrong-kind
    /// Card and [`BASELINE_ARTIFACT_INVALID`] for a missing or oversized
    /// artifact.
    async fn resolve(
        &self,
        tenant: DataTenantId,
        claimed: &ClaimedBaseline,
    ) -> Result<(DriftSpec, String), VerificationError> {
        let unavailable = |message: String| fit_error(BASELINE_DATA_UNAVAILABLE, message);
        let mut conn = self
            .postgres
            .tenant_conn(tenant)
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        let verifier = get_card_by_uid(&mut conn, &claimed.lease.verifier_uid)
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        let Spec::Verifier(verifier) = verifier.spec else {
            return Err(unavailable(
                "the baseline owner is not a Verifier".to_owned(),
            ));
        };
        let VerifierImplementation::Drift(spec) = verifier.implementation else {
            return Err(unavailable(
                "the baseline owner is not a Drift Verifier".to_owned(),
            ));
        };
        let data = get_card_by_uid(&mut conn, &claimed.data_card_uid)
            .await
            .map_err(|error| unavailable(error.to_string()))?;
        if !matches!(data.status, CardStatus::Active | CardStatus::Deprecated) {
            return Err(unavailable(format!(
                "baseline Data Card {} is not active",
                claimed.data_card_uid
            )));
        }
        let path = artifact_path(tenant, &claimed.data_card_uid);
        let metadata = artifact_metadata::get(&mut conn, &path)
            .await
            .map_err(|error| unavailable(error.to_string()))?
            .ok_or_else(|| {
                fit_error(
                    BASELINE_ARTIFACT_INVALID,
                    format!("baseline Data Card has no registered {DATA_ARTIFACT}"),
                )
            })?;
        if metadata.size_bytes > MAX_ARTIFACT_BYTES {
            return Err(fit_error(
                BASELINE_ARTIFACT_INVALID,
                format!(
                    "baseline artifact is {} bytes; the limit is {MAX_ARTIFACT_BYTES}",
                    metadata.size_bytes
                ),
            ));
        }
        Ok((spec, path))
    }
}

/// Decode a Parquet artifact into one batch within `budget` decoded bytes.
///
/// Reads record batches one at a time, adding each batch's Arrow memory size
/// to a running total, and stops as soon as the total exceeds `budget` or
/// `cancel` fires, so at most one batch past the budget is ever held. The
/// surviving batches are concatenated for the fitter.
///
/// # Errors
/// Returns [`BASELINE_ARTIFACT_INVALID`] when the bytes are not readable
/// Parquet or decode past `budget`, and [`BASELINE_FIT_FAILED`] when
/// `cancel` fired.
fn decode_bounded<R: ChunkReader + 'static>(
    bytes: R,
    budget: usize,
    cancel: &CancellationToken,
) -> Result<RecordBatch, VerificationError> {
    let invalid =
        |error: &dyn std::fmt::Display| fit_error(BASELINE_ARTIFACT_INVALID, error.to_string());
    let reader =
        ParquetRecordBatchReaderBuilder::try_new(bytes).map_err(|error| invalid(&error))?;
    let schema = Arc::clone(reader.schema());
    let mut batches = Vec::new();
    let mut decoded = 0_usize;
    for batch in reader.build().map_err(|error| invalid(&error))? {
        if cancel.is_cancelled() {
            return Err(fit_error(
                BASELINE_FIT_FAILED,
                "the baseline fit was cancelled",
            ));
        }
        let batch = batch.map_err(|error| invalid(&error))?;
        decoded = decoded.saturating_add(batch.get_array_memory_size());
        if decoded > budget {
            return Err(fit_error(
                BASELINE_ARTIFACT_INVALID,
                format!("baseline artifact decodes past the {budget}-byte budget"),
            ));
        }
        batches.push(batch);
    }
    concat_batches(&schema, &batches).map_err(|error| invalid(&error))
}

/// Full tenant-scoped storage path of a Data Card's Parquet artifact.
fn artifact_path(tenant: DataTenantId, data_card_uid: &CardUid) -> String {
    tenant_path::build(tenant, data_card_uid.as_str(), DATA_ARTIFACT)
}

#[cfg(test)]
mod tests {
    //! Proof that baseline decoding is bounded by decoded size and stops on
    //! cancellation before any fitting work begins.

    use std::io::{Seek as _, SeekFrom};

    use arrow::array::StringArray;
    use arrow::datatypes::{DataType, Field, Schema};
    use datafusion::parquet::arrow::ArrowWriter;

    use super::*;

    /// Write `batches` of `rows` identical 1 KiB strings to a Parquet file.
    ///
    /// The repeated value dictionary-encodes to a few bytes per row on disk,
    /// while each decoded batch carries the full strings, so the file is a
    /// small-on-disk, high-expansion artifact. Returns the rewound file and
    /// its length.
    ///
    /// # Panics
    /// Panics when the temporary file cannot be written.
    fn expanding_parquet(batches: usize, rows: usize) -> (std::fs::File, u64) {
        let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Utf8, false)]));
        let mut file = tempfile::tempfile().expect("temporary file");
        let mut writer = ArrowWriter::try_new(
            file.try_clone().expect("file handle"),
            Arc::clone(&schema),
            None,
        )
        .expect("writer");
        let value = "a".repeat(1024);
        for _ in 0..batches {
            let column = StringArray::from_iter_values(std::iter::repeat_n(value.as_str(), rows));
            let batch =
                RecordBatch::try_new(Arc::clone(&schema), vec![Arc::new(column)]).expect("batch");
            writer.write(&batch).expect("batch writes");
        }
        writer.close().expect("writer closes");
        let length = file.seek(SeekFrom::End(0)).expect("length");
        (file, length)
    }

    /// A highly compressible artifact that is far smaller than the budget on
    /// disk fails once its decoded batches pass the budget, while the same
    /// artifact decodes whole under a budget that fits it.
    ///
    /// # Panics
    /// Panics when the budget is not enforced on decoded size.
    #[test]
    fn decode_stops_at_the_decoded_budget() {
        let budget = 4 * 1024 * 1024;
        let (file, length) = expanding_parquet(16, 1024);
        assert!(length < 64 * 1024, "the fixture is small on disk: {length}");
        let error = decode_bounded(file, budget, &CancellationToken::new())
            .expect_err("decoded size past the budget is refused");
        assert_eq!(error.code, BASELINE_ARTIFACT_INVALID);
        assert!(error.message.contains("budget"), "{}", error.message);

        let (file, _) = expanding_parquet(2, 1024);
        let batch = decode_bounded(file, budget, &CancellationToken::new())
            .expect("an artifact within the budget decodes");
        assert_eq!(batch.num_rows(), 2048);
    }

    /// A cancelled decode returns before reading any batch.
    ///
    /// # Panics
    /// Panics when a cancelled decode still produces a batch.
    #[test]
    fn cancelled_decode_returns_before_fitting() {
        let (file, _) = expanding_parquet(1, 8);
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error =
            decode_bounded(file, MAX_DECODED_BYTES, &cancel).expect_err("a cancelled decode stops");
        assert_eq!(error.code, BASELINE_FIT_FAILED);
    }
}
