//! Pre-execution qualification orchestration for the Bifrost capacity program.
//!
//! This module owns everything a qualification run needs assembled before it is
//! executed (T32 executes; this module implements): the D70 dataset-shape
//! selection rule, the locked run budgets and their enforcement, the run record
//! that pins a run to one qualified source commit / dataset digest / shape /
//! storage configuration for digest-validated reuse, and the
//! `wyrd.bifrost.qualification/v1` manifest writer with its
//! `qualified_source_commit`/`evidence_commit` seal semantics.
//!
//! The cohesive owner is [`QualificationRun`]: it holds run identity, the
//! selected shape, the budgets, and the run root, and exposes the operations a
//! run driver composes — calibration-driven shape selection, once-per-run
//! materialization through an injected [`RunDatasetProvisioner`] seam, serial
//! family dispatch through an injected [`FamilyExecutor`] seam, and manifest
//! assembly. The seams keep the orchestration unit-testable with doubles: the
//! real adapters that bind to a live cluster are the run driver's (T32) wiring,
//! while the orchestration order, once-per-run reuse, budget enforcement, and
//! manifest assembly are proved here without a server.

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::bench_dataset::DatasetShape;
use super::bench_report::EnvironmentIdentity;

/// Closed schema identifier emitted by every qualification manifest.
const QUALIFICATION_MANIFEST_SCHEMA: &str = "wyrd.bifrost.qualification/v1";
/// Closed schema identifier emitted by every persisted run record.
const RUN_RECORD_SCHEMA: &str = "wyrd.bifrost.qualification-run/v1";
/// Locked query-accounting marker recorded in every qualification manifest.
const ORACLE_QUERY_ACCOUNTING: &str = "pod_local_v1";

/// Number of UTC day partitions in every qualification-tier dataset (D70).
const QUALIFICATION_DAYS: u32 = 4;
/// Locked qualification `rows_per_day` ladder searched in ascending order (D70).
const SHAPE_LADDER: [u64; 3] = [200_000, 800_000, 3_200_000];
/// Logical bytes attributed to one dataset row for scan-cost arithmetic (D70).
const BYTES_PER_LOGICAL_ROW: u64 = 298;
/// Multiple of the effective Oracle memory budget the Q4 two-day scan must reach.
const MEMORY_SCAN_MULTIPLE: u64 = 4;
/// Number of trailing UTC days scanned by Q4 (the final two-day analytical scan).
const Q4_SCAN_DAYS: u64 = 2;
/// Safety factor applied to the calibration-derived materialization estimate.
const MATERIALIZATION_SAFETY_FACTOR: f64 = 2.0;

/// One mebibyte in bytes.
const MIB: u64 = 1024 * 1024;
/// One gibibyte in bytes.
const GIB: u64 = 1024 * MIB;
/// D79 lower clamp on the resolved Oracle child memory budget (256 MiB).
const ORACLE_BUDGET_FLOOR_BYTES: u64 = 256 * MIB;
/// D79 upper clamp on the resolved Oracle child memory budget (8 GiB).
const ORACLE_BUDGET_CEIL_BYTES: u64 = 8 * GIB;

/// Locked setup deadline: materialization must complete within 30 minutes.
const DEFAULT_SETUP_SECONDS: u64 = 30 * 60;
/// Locked total-runtime budget: the whole qualification run within 3 hours.
const DEFAULT_TOTAL_SECONDS: u64 = 3 * 60 * 60;
/// Locked disk budget under the run root: 32 GiB.
const DEFAULT_DISK_BYTES: u64 = 32 * GIB;

/// Effective Oracle memory budget used by the D70 shape-selection rule.
///
/// Resolved from the same runtime configuration surface the server uses: an
/// explicit `OracleRuntimeConfig::memory_limit_bytes`, else the D79 default
/// (25% of pod memory clamped to `[256 MiB, 8 GiB]`), else — when the runtime
/// cannot resolve a bounded pool — the unbounded branch that drops the
/// memory-multiple constraint and records `memory_budget_basis: "unbounded"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBudget {
    /// A bounded effective budget in bytes with the basis it was resolved from.
    Bounded {
        /// Effective budget in bytes.
        bytes: u64,
        /// Whether the value came from explicit config or the D79 default.
        basis: MemoryBudgetBasis,
    },
    /// The runtime treats the pool as unbounded; the memory multiple is dropped.
    Unbounded,
}

/// Provenance of a resolved bounded [`MemoryBudget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryBudgetBasis {
    /// An explicit `OracleRuntimeConfig::memory_limit_bytes` value.
    Configured,
    /// The D79 default derived from probed pod memory.
    Default,
}

impl MemoryBudget {
    /// Resolve the effective budget from explicit config and probed pod memory.
    ///
    /// Mirrors the server's D79 resolution read-only: an explicit configured
    /// value wins (floored at the 256 MiB minimum the server enforces), else the
    /// default is `pod_memory / 4` clamped to `[256 MiB, 8 GiB]`, else — when pod
    /// memory cannot be probed — [`MemoryBudget::Unbounded`].
    #[must_use]
    pub fn resolve(config_limit_bytes: Option<u64>, pod_memory_bytes: Option<u64>) -> Self {
        if let Some(configured) = config_limit_bytes {
            return Self::Bounded {
                bytes: configured.max(ORACLE_BUDGET_FLOOR_BYTES),
                basis: MemoryBudgetBasis::Configured,
            };
        }
        match pod_memory_bytes {
            Some(pod) => Self::Bounded {
                bytes: (pod / 4).clamp(ORACLE_BUDGET_FLOOR_BYTES, ORACLE_BUDGET_CEIL_BYTES),
                basis: MemoryBudgetBasis::Default,
            },
            None => Self::Unbounded,
        }
    }

    /// Return the stable manifest string recorded for this budget basis.
    #[must_use]
    fn basis_label(self) -> &'static str {
        match self {
            Self::Bounded {
                basis: MemoryBudgetBasis::Configured,
                ..
            } => "configured",
            Self::Bounded {
                basis: MemoryBudgetBasis::Default,
                ..
            } => "default",
            Self::Unbounded => "unbounded",
        }
    }

    /// Return the bounded byte value, or `None` when unbounded.
    #[must_use]
    fn bounded_bytes(self) -> Option<u64> {
        match self {
            Self::Bounded { bytes, .. } => Some(bytes),
            Self::Unbounded => None,
        }
    }
}

/// Measured throughput from the in-run calibration slice.
///
/// Produced by materializing one tenant x 32 batches of the smoke shape into a
/// scratch table through the T29 materializer and recording the achieved rows
/// per second; the scratch data is removed and this observation never enters a
/// measured stage. It is the sole input to the D70 materialization-time
/// estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    /// Achieved rows per second over the calibration slice.
    pub rows_per_second: f64,
}

/// Outcome of the D70 dataset-shape selection rule for one run.
///
/// Records everything the run manifest must retain about the choice: the chosen
/// shape, the basis of the effective memory budget, the Q4 two-day logical scan
/// size the memory multiple was checked against, and the safety-factored
/// materialization estimate that had to fit the setup deadline.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSelection {
    /// The selected dataset shape (`days = 4`, ladder `rows_per_day`).
    pub shape: DatasetShape,
    /// Manifest string naming how the effective memory budget was resolved.
    pub memory_budget_basis: String,
    /// The bounded effective memory budget in bytes, or `None` when unbounded.
    pub effective_memory_budget_bytes: Option<u64>,
    /// Q4 two-day logical scan size in bytes for the chosen shape.
    pub q4_scan_bytes: u64,
    /// Safety-factored estimated materialization time in seconds.
    pub estimated_materialization_seconds: f64,
    /// Calibration rows per second the estimate was derived from.
    pub calibration_rows_per_second: f64,
}

/// Failure raised when no ladder shape satisfies the D70 rule within budgets.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum ShapeSelectionError {
    /// No ladder rung meets the memory multiple and fits the setup deadline.
    #[error(
        "no qualification shape in ladder {ladder:?} satisfies the {multiple}x memory multiple \
         over budget {budget_bytes:?} bytes within the {setup_deadline_seconds}s setup deadline \
         at {rows_per_second} rows/s"
    )]
    NoFeasibleShape {
        /// The searched `rows_per_day` ladder.
        ladder: [u64; 3],
        /// The required memory multiple.
        multiple: u64,
        /// The effective memory budget, or `None` when unbounded.
        budget_bytes: Option<u64>,
        /// The setup deadline the estimate had to fit.
        setup_deadline_seconds: u64,
        /// The calibration throughput used for the estimate.
        rows_per_second: f64,
    },
    /// The calibration throughput was not a positive, finite rows-per-second.
    #[error("calibration rows_per_second must be positive and finite, got {0}")]
    InvalidCalibration(f64),
}

/// Compute the Q4 two-day logical scan size in bytes for a `rows_per_day` rung.
///
/// Q4 scans the final two UTC days grouped by `device_id % 256`, so its logical
/// scan is `2 * rows_per_day` rows weighted by the D70 per-row byte estimate.
#[must_use]
fn q4_scan_bytes(rows_per_day: u64) -> u64 {
    Q4_SCAN_DAYS
        .saturating_mul(rows_per_day)
        .saturating_mul(BYTES_PER_LOGICAL_ROW)
}

/// Estimate the safety-factored materialization time for a `rows_per_day` rung.
///
/// The dataset is `days * rows_per_day` rows; dividing by the measured
/// calibration throughput and multiplying by the locked safety factor yields the
/// seconds the estimate must not exceed the setup deadline.
#[must_use]
fn materialization_estimate_seconds(rows_per_day: u64, rows_per_second: f64) -> f64 {
    let total_rows = u64::from(QUALIFICATION_DAYS).saturating_mul(rows_per_day);
    (total_rows as f64 / rows_per_second) * MATERIALIZATION_SAFETY_FACTOR
}

/// Select the qualification `rows_per_day` per the locked D70 rule.
///
/// Searches [`SHAPE_LADDER`] in ascending order. In the bounded branch the chosen
/// rung is the smallest whose Q4 two-day logical scan is at least
/// [`MEMORY_SCAN_MULTIPLE`] times the effective memory budget and whose
/// safety-factored materialization estimate fits `setup_deadline_seconds`. In
/// the unbounded branch the memory multiple is dropped and the chosen rung is the
/// largest whose estimate fits the deadline. The scan condition is monotonic
/// increasing and the estimate condition monotonic decreasing in `rows_per_day`,
/// so the feasible set is contiguous and the rule is deterministic.
///
/// # Errors
/// Returns [`ShapeSelectionError::InvalidCalibration`] when the calibration
/// throughput is not positive and finite, and
/// [`ShapeSelectionError::NoFeasibleShape`] when no rung satisfies both
/// conditions (the escalation case, plan authority).
pub fn select_qualification_shape(
    budget: MemoryBudget,
    calibration: Calibration,
    setup_deadline_seconds: u64,
) -> Result<ShapeSelection, ShapeSelectionError> {
    let rows_per_second = calibration.rows_per_second;
    if !rows_per_second.is_finite() || rows_per_second <= 0.0 {
        return Err(ShapeSelectionError::InvalidCalibration(rows_per_second));
    }
    let deadline = setup_deadline_seconds as f64;
    let fits_deadline = |rows_per_day: u64| {
        materialization_estimate_seconds(rows_per_day, rows_per_second) <= deadline
    };

    let chosen = match budget {
        MemoryBudget::Bounded { bytes, .. } => {
            let threshold = bytes.saturating_mul(MEMORY_SCAN_MULTIPLE);
            SHAPE_LADDER
                .into_iter()
                .find(|&rows| q4_scan_bytes(rows) >= threshold && fits_deadline(rows))
        }
        MemoryBudget::Unbounded => SHAPE_LADDER
            .into_iter()
            .rev()
            .find(|&rows| fits_deadline(rows)),
    };

    let rows_per_day = chosen.ok_or(ShapeSelectionError::NoFeasibleShape {
        ladder: SHAPE_LADDER,
        multiple: MEMORY_SCAN_MULTIPLE,
        budget_bytes: budget.bounded_bytes(),
        setup_deadline_seconds,
        rows_per_second,
    })?;

    // `QUALIFICATION_DAYS` is 4 and every ladder rung divides one UTC day, so
    // construction cannot fail; surface any future divergence as a no-fit rather
    // than panicking on an invariant the ladder is defined to uphold.
    let shape = DatasetShape::new(QUALIFICATION_DAYS, rows_per_day).map_err(|_| {
        ShapeSelectionError::NoFeasibleShape {
            ladder: SHAPE_LADDER,
            multiple: MEMORY_SCAN_MULTIPLE,
            budget_bytes: budget.bounded_bytes(),
            setup_deadline_seconds,
            rows_per_second,
        }
    })?;

    Ok(ShapeSelection {
        shape,
        memory_budget_basis: budget.basis_label().to_owned(),
        effective_memory_budget_bytes: budget.bounded_bytes(),
        q4_scan_bytes: q4_scan_bytes(rows_per_day),
        estimated_materialization_seconds: materialization_estimate_seconds(
            rows_per_day,
            rows_per_second,
        ),
        calibration_rows_per_second: rows_per_second,
    })
}

/// Locked qualification run budgets, recorded in the manifest and enforced.
///
/// The defaults are plan authority: a 30-minute setup deadline (materialization
/// must finish within it before any measured stage runs), a 3-hour total-runtime
/// budget for the whole run, and a 32 GiB disk budget accounted under the run
/// root at family boundaries. Only explicit CLI overrides — themselves recorded
/// — may change them; this type carries whatever values a run resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunBudgets {
    /// Setup deadline; materialization must complete within it.
    #[serde(with = "duration_seconds")]
    pub setup: Duration,
    /// Total-runtime budget for the whole qualification run.
    #[serde(with = "duration_seconds")]
    pub total: Duration,
    /// Disk budget in bytes accounted under the run root.
    pub disk_bytes: u64,
}

impl Default for RunBudgets {
    /// Return the locked default budgets (30 min setup, 3 h total, 32 GiB disk).
    fn default() -> Self {
        Self {
            setup: Duration::from_secs(DEFAULT_SETUP_SECONDS),
            total: Duration::from_secs(DEFAULT_TOTAL_SECONDS),
            disk_bytes: DEFAULT_DISK_BYTES,
        }
    }
}

impl RunBudgets {
    /// Enforce the total-runtime budget given the elapsed run time.
    ///
    /// # Errors
    /// Returns [`BudgetError::Total`] naming the budget when `elapsed` exceeds
    /// [`Self::total`], so the run fails with the breached budget identified
    /// rather than an anonymous timeout.
    pub fn enforce_total(self, elapsed: Duration) -> Result<(), BudgetError> {
        if elapsed > self.total {
            return Err(BudgetError::Total {
                elapsed_seconds: elapsed.as_secs(),
                budget_seconds: self.total.as_secs(),
            });
        }
        Ok(())
    }

    /// Enforce the setup deadline given the elapsed materialization time.
    ///
    /// # Errors
    /// Returns [`BudgetError::Setup`] naming the budget when `elapsed` exceeds
    /// [`Self::setup`]; the run driver applies this before any measured stage.
    pub fn enforce_setup(self, elapsed: Duration) -> Result<(), BudgetError> {
        if elapsed > self.setup {
            return Err(BudgetError::Setup {
                elapsed_seconds: elapsed.as_secs(),
                budget_seconds: self.setup.as_secs(),
            });
        }
        Ok(())
    }

    /// Enforce the disk budget given a measured byte usage under the run root.
    ///
    /// # Errors
    /// Returns [`BudgetError::Disk`] naming the budget when `used_bytes` exceeds
    /// [`Self::disk_bytes`]; callers measure usage with [`directory_size_bytes`]
    /// at family boundaries.
    pub fn enforce_disk(self, used_bytes: u64) -> Result<(), BudgetError> {
        if used_bytes > self.disk_bytes {
            return Err(BudgetError::Disk {
                used_bytes,
                budget_bytes: self.disk_bytes,
            });
        }
        Ok(())
    }
}

/// A named budget breach that fails the qualification run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BudgetError {
    /// The setup deadline was exceeded before measurement.
    #[error("setup budget exceeded: {elapsed_seconds}s > {budget_seconds}s")]
    Setup {
        /// Elapsed setup seconds.
        elapsed_seconds: u64,
        /// The setup budget in seconds.
        budget_seconds: u64,
    },
    /// The total-runtime budget was exceeded.
    #[error("total runtime budget exceeded: {elapsed_seconds}s > {budget_seconds}s")]
    Total {
        /// Elapsed run seconds.
        elapsed_seconds: u64,
        /// The total budget in seconds.
        budget_seconds: u64,
    },
    /// The disk budget under the run root was exceeded.
    #[error("disk budget exceeded: {used_bytes} bytes > {budget_bytes} bytes")]
    Disk {
        /// Measured bytes under the run root.
        used_bytes: u64,
        /// The disk budget in bytes.
        budget_bytes: u64,
    },
}

/// Recursively sum the sizes of regular files under `root`.
///
/// This is a point-in-time size walk used for disk accounting at family
/// boundaries, not continuous monitoring. Symlinks are not followed and
/// directory entry sizes are excluded; only regular-file lengths are summed.
///
/// # Errors
/// Returns any [`std::io::Error`] from reading the directory tree, so a run that
/// cannot account its disk usage fails rather than proceeding blind.
pub fn directory_size_bytes(root: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    if !root.exists() {
        return Ok(0);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                total = total.saturating_add(entry.metadata()?.len());
            }
        }
    }
    Ok(total)
}

/// Serde helper serializing a [`Duration`] as whole seconds.
mod duration_seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a duration as its whole-second count.
    ///
    /// # Errors
    /// Propagates the serializer's own error.
    pub(super) fn serialize<S: Serializer>(
        value: &Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.as_secs())
    }

    /// Deserialize a whole-second count into a duration.
    ///
    /// # Errors
    /// Propagates the deserializer's own error.
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Duration, D::Error> {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

/// The four identity fields a per-family rerun must match to reuse a run.
///
/// A `bench:bifrost:qualification:<family>` rerun opens an existing run only when
/// the current environment's qualified source commit, dataset digest, dataset
/// shape, and storage configuration all equal the values the run recorded. This
/// fingerprint is the comparison unit; any mismatch is refused with a field
/// diagnostic and reuse never crosses source commits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunFingerprint {
    /// 40-hex commit of the immutable qualified source worktree.
    pub qualified_source_commit: String,
    /// SHA-256 digest of the chosen shape's canonical dataset manifest.
    pub dataset_digest: String,
    /// The chosen dataset shape.
    pub shape: DatasetShape,
    /// Storage-configuration identity (for example the storage mode label).
    pub storage_config: String,
}

impl RunFingerprint {
    /// Refuse reuse unless every identity field matches `current`.
    ///
    /// Compares the recorded fingerprint (`self`) against the current
    /// environment field by field, returning the first mismatch as a diagnostic
    /// so a rerun refuses before any cluster starts.
    ///
    /// # Errors
    /// Returns [`ReuseError`] naming the first field whose recorded value
    /// differs from `current`; reuse is only permitted when all four match.
    pub fn verify_reuse(&self, current: &RunFingerprint) -> Result<(), ReuseError> {
        if self.qualified_source_commit != current.qualified_source_commit {
            return Err(ReuseError::Mismatch {
                field: "qualified_source_commit",
                recorded: self.qualified_source_commit.clone(),
                current: current.qualified_source_commit.clone(),
            });
        }
        if self.dataset_digest != current.dataset_digest {
            return Err(ReuseError::Mismatch {
                field: "dataset_digest",
                recorded: self.dataset_digest.clone(),
                current: current.dataset_digest.clone(),
            });
        }
        if self.shape != current.shape {
            return Err(ReuseError::Mismatch {
                field: "shape",
                recorded: format!("{:?}", self.shape),
                current: format!("{:?}", current.shape),
            });
        }
        if self.storage_config != current.storage_config {
            return Err(ReuseError::Mismatch {
                field: "storage_config",
                recorded: self.storage_config.clone(),
                current: current.storage_config.clone(),
            });
        }
        Ok(())
    }
}

/// A field-diagnostic refusal to reuse a run for a per-family rerun.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ReuseError {
    /// A recorded identity field does not match the current environment.
    #[error(
        "qualification run reuse refused: {field} mismatch (recorded={recorded}, current={current})"
    )]
    Mismatch {
        /// The identity field that diverged.
        field: &'static str,
        /// The value recorded in the run record.
        recorded: String,
        /// The value in the current environment.
        current: String,
    },
}

/// Persisted run record binding a `<run-id>` to its immutable identity.
///
/// Written once when a full-suite run selects its shape and materializes, then
/// opened by per-family reruns to validate reuse. It records the run identity
/// fingerprint, the selected shape's calibration and estimate, and the resolved
/// budgets, so a rerun can prove it is operating against the same source, data,
/// and configuration before it acquires the dataset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationRunRecord {
    /// Closed run-record schema identifier.
    pub schema_version: String,
    /// The run identity.
    pub run_id: String,
    /// The reuse fingerprint (source commit, dataset digest, shape, storage).
    pub fingerprint: RunFingerprint,
    /// Whether the qualified source worktree tree was clean at run start.
    pub source_tree_clean: bool,
    /// Calibration throughput recorded from the in-run scratch slice.
    pub calibration_rows_per_second: f64,
    /// Safety-factored materialization estimate for the chosen shape (seconds).
    pub estimated_materialization_seconds: f64,
    /// How the effective Oracle memory budget was resolved.
    pub memory_budget_basis: String,
    /// The resolved run budgets.
    pub budgets: RunBudgets,
}

impl QualificationRunRecord {
    /// File name of the run record under the run root.
    const FILE_NAME: &'static str = "qualification-run.json";

    /// Assemble a run record from a run's identity, selection, and budgets.
    ///
    /// Stamps the closed [`RUN_RECORD_SCHEMA`] identifier and copies the reuse
    /// fingerprint and D70 selection evidence a per-family rerun validates before
    /// it acquires the once-materialized dataset.
    #[must_use]
    pub fn new(
        run_id: String,
        fingerprint: RunFingerprint,
        source_tree_clean: bool,
        selection: &ShapeSelection,
        budgets: RunBudgets,
    ) -> Self {
        Self {
            schema_version: RUN_RECORD_SCHEMA.to_owned(),
            run_id,
            fingerprint,
            source_tree_clean,
            calibration_rows_per_second: selection.calibration_rows_per_second,
            estimated_materialization_seconds: selection.estimated_materialization_seconds,
            memory_budget_basis: selection.memory_budget_basis.clone(),
            budgets,
        }
    }

    /// Serialize the run record to canonical pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// Returns [`RunRecordError::Serialize`] when serialization fails.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, RunRecordError> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| RunRecordError::Serialize(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Persist the run record under `run_root` as `qualification-run.json`.
    ///
    /// # Errors
    /// Returns [`RunRecordError::Serialize`] on serialization failure or
    /// [`RunRecordError::Io`] when the run root cannot be created or written.
    pub fn persist(&self, run_root: &Path) -> Result<(), RunRecordError> {
        let bytes = self.to_canonical_json()?;
        std::fs::create_dir_all(run_root).map_err(|e| RunRecordError::Io(e.to_string()))?;
        std::fs::write(run_root.join(Self::FILE_NAME), bytes)
            .map_err(|e| RunRecordError::Io(e.to_string()))
    }

    /// Open and parse the run record persisted under `run_root`.
    ///
    /// # Errors
    /// Returns [`RunRecordError::Io`] when the record cannot be read and
    /// [`RunRecordError::Parse`] when its bytes are not a valid run record.
    pub fn open(run_root: &Path) -> Result<Self, RunRecordError> {
        let bytes = std::fs::read(run_root.join(Self::FILE_NAME))
            .map_err(|e| RunRecordError::Io(e.to_string()))?;
        serde_json::from_slice(&bytes).map_err(|e| RunRecordError::Parse(e.to_string()))
    }
}

/// Failure reading, writing, or parsing a persisted run record.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RunRecordError {
    /// The run record could not be serialized.
    #[error("run record serialization failed: {0}")]
    Serialize(String),
    /// The run record could not be read or written.
    #[error("run record io failed: {0}")]
    Io(String),
    /// The run record bytes could not be parsed.
    #[error("run record parse failed: {0}")]
    Parse(String),
}

/// One report artifact linked by the qualification manifest.
///
/// Records the family label, the run-root-relative path of the canonical report
/// file, and the SHA-256 digest of that file's bytes, so the validator can
/// recompute the digest from the on-disk report and reject any drift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportDigestEntry {
    /// Report family label (for example `oracle`, `distributed`).
    pub family: String,
    /// Run-root-relative path of the canonical report JSON file.
    pub path: String,
    /// SHA-256 digest of the report file bytes.
    pub digest: String,
}

/// Measured wall-clock durations and disk usage recorded for the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActualDurations {
    /// Measured setup (materialization) seconds.
    pub setup_seconds: u64,
    /// Measured total-run seconds.
    pub total_seconds: u64,
    /// Measured bytes used under the run root at completion.
    pub disk_bytes_used: u64,
}

/// Rolled-up correctness, saturation, and recovery verdicts for the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunVerdicts {
    /// Whether every family's every stage passed correctness.
    pub correctness_passed: bool,
    /// Whether any family observed a saturation boundary.
    pub saturation_reached: bool,
    /// Whether every recovery replay that ran recovered.
    pub recovery_recovered: bool,
}

/// Sealed review reference embedded by the seal step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewReference {
    /// Path to the human review artifact.
    pub path: String,
    /// SHA-256 digest of the review artifact bytes.
    pub sha256: String,
}

/// The `wyrd.bifrost.qualification/v1` manifest linking a run's evidence.
///
/// Assembled after all families run (review and `evidence_commit` absent), then
/// mutated by [`Self::seal`] to embed the review reference and evidence commit.
/// Every field the plan's Domain contracts declare normative is present: source
/// binding, tree-clean flag, dataset shape and digest, workload digest,
/// topology/hardware/object-store identity, budgets and actual durations,
/// command lines, the four linked report digests, the correctness/saturation/
/// recovery verdicts, and the locked `oracle_query_accounting` marker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationManifest {
    /// Closed manifest schema identifier (`wyrd.bifrost.qualification/v1`).
    pub schema_version: String,
    /// Run identity.
    pub run_id: String,
    /// 40-hex commit of the immutable qualified source worktree.
    pub qualified_source_commit: String,
    /// Whether the qualified source tree was clean at run start.
    pub source_tree_clean: bool,
    /// Promotion evidence commit; absent until the seal step.
    pub evidence_commit: Option<String>,
    /// The selected dataset shape.
    pub dataset_shape: DatasetShape,
    /// SHA-256 digest of the canonical dataset manifest for the shape.
    pub dataset_digest: String,
    /// SHA-256 digest of the workload fixtures.
    pub workload_digest: String,
    /// Topology identity captured with the run.
    pub topology: super::bench_report::TopologyIdentity,
    /// Hardware/object-store environment identity captured with the run.
    pub environment: EnvironmentIdentity,
    /// The resolved run budgets.
    pub budgets: RunBudgets,
    /// The measured durations and disk usage.
    pub actual_durations: ActualDurations,
    /// The exact command lines that produced the run.
    pub command_lines: Vec<String>,
    /// The four linked report digests.
    pub reports: Vec<ReportDigestEntry>,
    /// Rolled-up correctness/saturation/recovery verdicts.
    pub verdicts: RunVerdicts,
    /// Locked query-accounting marker (`pod_local_v1`).
    pub oracle_query_accounting: String,
    /// Sealed review reference; absent until the seal step.
    pub review: Option<ReviewReference>,
}

/// Static identity and measured inputs required to assemble a manifest.
///
/// Groups the fields that do not come from per-family execution so
/// [`QualificationManifest::assemble`] takes one cohesive argument rather than a
/// long parameter list.
#[derive(Debug, Clone, PartialEq)]
pub struct ManifestInputs {
    /// Run identity.
    pub run_id: String,
    /// 40-hex qualified source commit.
    pub qualified_source_commit: String,
    /// Whether the source tree was clean at run start.
    pub source_tree_clean: bool,
    /// The selected dataset shape.
    pub dataset_shape: DatasetShape,
    /// SHA-256 digest of the dataset manifest for the shape.
    pub dataset_digest: String,
    /// SHA-256 digest of the workload fixtures.
    pub workload_digest: String,
    /// Topology identity.
    pub topology: super::bench_report::TopologyIdentity,
    /// Environment identity.
    pub environment: EnvironmentIdentity,
    /// The resolved budgets.
    pub budgets: RunBudgets,
    /// The measured durations and disk usage.
    pub actual_durations: ActualDurations,
    /// The exact command lines that produced the run.
    pub command_lines: Vec<String>,
}

impl QualificationManifest {
    /// File name of the manifest under the run root.
    const FILE_NAME: &'static str = "qualification-manifest.json";

    /// Assemble a pre-seal manifest from static inputs, reports, and verdicts.
    ///
    /// The result has `review` and `evidence_commit` absent by construction; the
    /// seal step supplies them. Field and vector order are deterministic, so two
    /// assemblies from equal inputs serialize to identical bytes.
    #[must_use]
    pub fn assemble(
        inputs: ManifestInputs,
        reports: Vec<ReportDigestEntry>,
        verdicts: RunVerdicts,
    ) -> Self {
        Self {
            schema_version: QUALIFICATION_MANIFEST_SCHEMA.to_owned(),
            run_id: inputs.run_id,
            qualified_source_commit: inputs.qualified_source_commit,
            source_tree_clean: inputs.source_tree_clean,
            evidence_commit: None,
            dataset_shape: inputs.dataset_shape,
            dataset_digest: inputs.dataset_digest,
            workload_digest: inputs.workload_digest,
            topology: inputs.topology,
            environment: inputs.environment,
            budgets: inputs.budgets,
            actual_durations: inputs.actual_durations,
            command_lines: inputs.command_lines,
            reports,
            verdicts,
            oracle_query_accounting: ORACLE_QUERY_ACCOUNTING.to_owned(),
            review: None,
        }
    }

    /// Seal the manifest with a review reference and evidence commit.
    ///
    /// Consumes and returns the manifest with `review` and `evidence_commit`
    /// populated; the pre-seal fields are otherwise unchanged. This is the only
    /// operation that populates those two fields, matching the validator's phase
    /// contract (`pre-review` requires both absent; `sealed` requires both
    /// present).
    #[must_use]
    pub fn seal(
        mut self,
        review_path: String,
        review_sha256: String,
        evidence_commit: String,
    ) -> Self {
        self.review = Some(ReviewReference {
            path: review_path,
            sha256: review_sha256,
        });
        self.evidence_commit = Some(evidence_commit);
        self
    }

    /// Serialize the manifest to canonical pretty JSON with a trailing newline.
    ///
    /// # Errors
    /// Returns [`ManifestError::Serialize`] when serialization fails.
    pub fn to_canonical_json(&self) -> Result<Vec<u8>, ManifestError> {
        let mut bytes =
            serde_json::to_vec_pretty(self).map_err(|e| ManifestError::Serialize(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Persist the manifest under `run_root` as `qualification-manifest.json`.
    ///
    /// # Errors
    /// Returns [`ManifestError::Serialize`] on serialization failure or
    /// [`ManifestError::Io`] when the run root cannot be created or written.
    pub fn write(&self, run_root: &Path) -> Result<PathBuf, ManifestError> {
        let bytes = self.to_canonical_json()?;
        std::fs::create_dir_all(run_root).map_err(|e| ManifestError::Io(e.to_string()))?;
        let path = run_root.join(Self::FILE_NAME);
        std::fs::write(&path, bytes).map_err(|e| ManifestError::Io(e.to_string()))?;
        Ok(path)
    }
}

/// Failure serializing or writing a qualification manifest.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    /// The manifest could not be serialized.
    #[error("manifest serialization failed: {0}")]
    Serialize(String),
    /// The manifest could not be written.
    #[error("manifest io failed: {0}")]
    Io(String),
}

/// Outcome of provisioning the run dataset exactly once.
///
/// Returned by [`RunDatasetProvisioner::provision`]: the dataset digest the
/// materialized data hashes to (recorded for reuse validation) and the measured
/// setup elapsed the orchestrator enforces against the setup deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvisionOutcome {
    /// SHA-256 digest of the canonical dataset manifest for the shape.
    pub dataset_digest: String,
    /// Measured materialization elapsed.
    pub setup_elapsed: Duration,
}

/// Outcome of running one benchmark family against the run dataset.
///
/// Returned by [`FamilyExecutor::run_family`]: the written report's family
/// label, its run-root-relative path and byte digest (linked by the manifest),
/// and the correctness/saturation/recovery signals folded into the run verdicts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FamilyOutcome {
    /// Report family label.
    pub family: String,
    /// Run-root-relative path of the written report file.
    pub report_path: String,
    /// SHA-256 digest of the written report file bytes.
    pub report_digest: String,
    /// Whether every stage of this family passed correctness.
    pub correctness_passed: bool,
    /// Whether this family observed a saturation boundary.
    pub saturation_reached: bool,
    /// Whether every recovery replay that ran recovered.
    pub recovery_recovered: bool,
}

/// Seam that materializes the run dataset exactly once into the run root.
///
/// The real adapter wraps the T29 [`super::bench_materializer`] against a booted
/// cluster; unit tests inject a double so the once-per-run guarantee and budget
/// enforcement are proved without a server.
#[async_trait]
pub trait RunDatasetProvisioner {
    /// Materialize `shape` into `run_root` and report the digest and elapsed.
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Provision`] when materialization fails;
    /// the orchestrator additionally fails the run when the reported elapsed
    /// breaches the setup deadline.
    async fn provision(
        &self,
        shape: DatasetShape,
        run_root: &Path,
    ) -> Result<ProvisionOutcome, QualificationRunError>;
}

/// Seam that runs one benchmark family against the provisioned dataset.
///
/// The real adapter dispatches the T30 family runner; unit tests inject a double
/// so serial dispatch order and verdict aggregation are proved without a server.
#[async_trait]
pub trait FamilyExecutor {
    /// Run `family` against the dataset identified by `dataset_digest`.
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Family`] when the family runner fails.
    async fn run_family(
        &self,
        family: &str,
        dataset_digest: &str,
    ) -> Result<FamilyOutcome, QualificationRunError>;
}

/// Static identity threaded into the assembled manifest by the orchestrator.
///
/// Carries the fields that are fixed for the run rather than produced per family
/// (source binding, digests already known before execution, identity, command
/// lines), keeping [`QualificationRun::orchestrate`] one cohesive call.
#[derive(Debug, Clone, PartialEq)]
pub struct OrchestrationConfig {
    /// Whether the qualified source tree was clean at run start.
    pub source_tree_clean: bool,
    /// SHA-256 digest of the workload fixtures.
    pub workload_digest: String,
    /// Topology identity captured with the run.
    pub topology: super::bench_report::TopologyIdentity,
    /// Environment identity captured with the run.
    pub environment: EnvironmentIdentity,
    /// The exact command lines that produced the run.
    pub command_lines: Vec<String>,
    /// The families to run serially, in order.
    pub families: Vec<String>,
}

/// Cohesive owner of one qualification run's pre-execution orchestration.
///
/// Holds run identity, the D70-selected shape, the resolved budgets, and the run
/// root, and drives the ordered run: provision the dataset once, run every family
/// serially against it, enforce the total and disk budgets, and assemble the
/// pre-seal manifest. Provisioning and family execution are injected seams, so
/// the ordering, once-per-run guarantee, budget enforcement, and manifest
/// assembly are unit-testable with doubles while the live adapters bind a cluster
/// under T32.
#[derive(Debug, Clone, PartialEq)]
pub struct QualificationRun {
    /// Run identity grouping every artifact for this run.
    run_id: String,
    /// 40-hex commit of the immutable qualified source worktree.
    qualified_source_commit: String,
    /// The D70-selected dataset shape.
    shape: DatasetShape,
    /// The resolved run budgets.
    budgets: RunBudgets,
    /// The run root (`target/bifrost-benchmarks/qualification/<run-id>/`).
    root: PathBuf,
}

impl QualificationRun {
    /// Construct a run over an already-selected shape, budgets, and root.
    #[must_use]
    pub fn new(
        run_id: String,
        qualified_source_commit: String,
        shape: DatasetShape,
        budgets: RunBudgets,
        root: PathBuf,
    ) -> Self {
        Self {
            run_id,
            qualified_source_commit,
            shape,
            budgets,
            root,
        }
    }

    /// The run root path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Drive the ordered run and return the assembled pre-seal manifest.
    ///
    /// Provisions the dataset exactly once through `provisioner`, enforcing the
    /// setup deadline against the reported elapsed; then runs every configured
    /// family serially through `executor`, collecting report digests and folding
    /// the per-family correctness, saturation, and recovery signals into the run
    /// verdicts; then enforces the total-runtime budget against `elapsed` and the
    /// disk budget against a size walk of the run root; and finally assembles the
    /// pre-seal manifest (review and evidence commit absent). The dataset is
    /// materialized once and reused across families, satisfying the one-
    /// materialization-per-run contract.
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Budget`] when the setup, total, or disk
    /// budget is breached (naming the budget), [`QualificationRunError::Provision`]
    /// or [`QualificationRunError::Family`] when a seam fails,
    /// [`QualificationRunError::Disk`] when the run root cannot be measured, and
    /// [`QualificationRunError::Manifest`] when the manifest cannot be assembled.
    pub async fn orchestrate<P, F>(
        &self,
        provisioner: &P,
        executor: &F,
        config: OrchestrationConfig,
        elapsed: Duration,
    ) -> Result<QualificationManifest, QualificationRunError>
    where
        P: RunDatasetProvisioner + Sync,
        F: FamilyExecutor + Sync,
    {
        let provisioned = provisioner.provision(self.shape, &self.root).await?;
        self.budgets
            .enforce_setup(provisioned.setup_elapsed)
            .map_err(QualificationRunError::Budget)?;

        let mut reports = Vec::with_capacity(config.families.len());
        let mut correctness_passed = true;
        let mut saturation_reached = false;
        let mut recovery_recovered = true;
        for family in &config.families {
            let outcome = executor
                .run_family(family, &provisioned.dataset_digest)
                .await?;
            correctness_passed &= outcome.correctness_passed;
            saturation_reached |= outcome.saturation_reached;
            recovery_recovered &= outcome.recovery_recovered;
            reports.push(ReportDigestEntry {
                family: outcome.family,
                path: outcome.report_path,
                digest: outcome.report_digest,
            });
        }

        self.budgets
            .enforce_total(elapsed)
            .map_err(QualificationRunError::Budget)?;
        let disk_used = directory_size_bytes(&self.root)
            .map_err(|e| QualificationRunError::Disk(e.to_string()))?;
        self.budgets
            .enforce_disk(disk_used)
            .map_err(QualificationRunError::Budget)?;

        let inputs = ManifestInputs {
            run_id: self.run_id.clone(),
            qualified_source_commit: self.qualified_source_commit.clone(),
            source_tree_clean: config.source_tree_clean,
            dataset_shape: self.shape,
            dataset_digest: provisioned.dataset_digest,
            workload_digest: config.workload_digest,
            topology: config.topology,
            environment: config.environment,
            budgets: self.budgets,
            actual_durations: ActualDurations {
                setup_seconds: provisioned.setup_elapsed.as_secs(),
                total_seconds: elapsed.as_secs(),
                disk_bytes_used: disk_used,
            },
            command_lines: config.command_lines,
        };
        Ok(QualificationManifest::assemble(
            inputs,
            reports,
            RunVerdicts {
                correctness_passed,
                saturation_reached,
                recovery_recovered,
            },
        ))
    }
}

/// Umbrella failure raised while orchestrating a qualification run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QualificationRunError {
    /// A named budget was breached.
    #[error(transparent)]
    Budget(#[from] BudgetError),
    /// Dataset provisioning (materialization) failed.
    #[error("dataset provisioning failed: {0}")]
    Provision(String),
    /// A family runner failed.
    #[error("family execution failed: {0}")]
    Family(String),
    /// The run root could not be measured for disk accounting.
    #[error("disk accounting failed: {0}")]
    Disk(String),
    /// The manifest could not be assembled or written.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::bench_report::TopologyIdentity;
    use super::*;

    /// A 40-hex commit stand-in for tests.
    const TEST_COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    /// A 64-hex digest stand-in for tests.
    const TEST_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

    /// Build a bounded budget of `bytes` for the D70 selection rule.
    fn bounded(bytes: u64) -> MemoryBudget {
        MemoryBudget::Bounded {
            bytes,
            basis: MemoryBudgetBasis::Default,
        }
    }

    /// A brisk calibration used where selection is not deadline-bound.
    fn calib(rows_per_second: f64) -> Calibration {
        Calibration { rows_per_second }
    }

    #[test]
    fn selects_smallest_rung_meeting_memory_multiple() {
        // 4 * 10_000_000 = 40 MiB-ish <= rung0 Q4 scan (119.2 MB): rung0 wins.
        let selection = select_qualification_shape(bounded(10_000_000), calib(100_000.0), 1800)
            .expect("rung0 selectable");
        assert_eq!(selection.shape.rows_per_day, 200_000);
        assert_eq!(selection.memory_budget_basis, "default");
    }

    #[test]
    fn selects_middle_rung_when_budget_outgrows_smallest() {
        // 4 * 100_000_000 = 400 MB: exceeds rung0 (119.2 MB), fits rung1 (476.8 MB).
        let selection = select_qualification_shape(bounded(100_000_000), calib(100_000.0), 1800)
            .expect("rung1 selectable");
        assert_eq!(selection.shape.rows_per_day, 800_000);
    }

    #[test]
    fn selects_largest_rung_when_budget_outgrows_middle() {
        // 4 * 400_000_000 = 1.6 GB: exceeds rung1 (476.8 MB), fits rung2 (1.907 GB).
        let selection = select_qualification_shape(bounded(400_000_000), calib(100_000.0), 1800)
            .expect("rung2 selectable");
        assert_eq!(selection.shape.rows_per_day, 3_200_000);
    }

    #[test]
    fn no_shape_when_budget_exceeds_largest_scan() {
        // 4 * 500_000_000 = 2.0 GB: no rung's Q4 scan reaches it.
        let error = select_qualification_shape(bounded(500_000_000), calib(100_000.0), 1800)
            .expect_err("memory multiple unsatisfiable");
        assert!(matches!(error, ShapeSelectionError::NoFeasibleShape { .. }));
    }

    #[test]
    fn no_shape_when_estimate_exceeds_setup_deadline() {
        // Memory admits rung0 but the deadline is far too small for any rung.
        let error = select_qualification_shape(bounded(10_000_000), calib(10.0), 1)
            .expect_err("deadline unsatisfiable");
        assert!(matches!(error, ShapeSelectionError::NoFeasibleShape { .. }));
    }

    #[test]
    fn unbounded_selects_largest_rung_within_deadline() {
        // rung estimates at 100_000 rows/s: rung0 16s, rung1 64s, rung2 256s.
        let selection = select_qualification_shape(MemoryBudget::Unbounded, calib(100_000.0), 100)
            .expect("unbounded selectable");
        assert_eq!(selection.shape.rows_per_day, 800_000);
        assert_eq!(selection.memory_budget_basis, "unbounded");
        assert_eq!(selection.effective_memory_budget_bytes, None);
    }

    #[test]
    fn unbounded_no_shape_when_no_estimate_fits() {
        let error = select_qualification_shape(MemoryBudget::Unbounded, calib(10.0), 1)
            .expect_err("no unbounded rung fits");
        assert!(matches!(error, ShapeSelectionError::NoFeasibleShape { .. }));
    }

    #[test]
    fn rejects_nonpositive_calibration() {
        let error = select_qualification_shape(MemoryBudget::Unbounded, calib(0.0), 1800)
            .expect_err("zero throughput rejected");
        assert!(matches!(error, ShapeSelectionError::InvalidCalibration(_)));
    }

    #[test]
    fn memory_budget_resolves_configured_and_default_and_unbounded() {
        assert_eq!(
            MemoryBudget::resolve(Some(GIB), None),
            MemoryBudget::Bounded {
                bytes: GIB,
                basis: MemoryBudgetBasis::Configured
            }
        );
        // Configured below the floor is raised to the floor.
        assert_eq!(
            MemoryBudget::resolve(Some(1), None),
            MemoryBudget::Bounded {
                bytes: ORACLE_BUDGET_FLOOR_BYTES,
                basis: MemoryBudgetBasis::Configured
            }
        );
        // Default is a clamped quarter of pod memory.
        assert_eq!(
            MemoryBudget::resolve(None, Some(16 * GIB)),
            MemoryBudget::Bounded {
                bytes: 4 * GIB,
                basis: MemoryBudgetBasis::Default
            }
        );
        assert_eq!(MemoryBudget::resolve(None, None), MemoryBudget::Unbounded);
    }

    #[test]
    fn budgets_enforce_named_breaches() {
        let budgets = RunBudgets::default();
        assert_eq!(budgets.setup, Duration::from_secs(1800));
        assert_eq!(budgets.total, Duration::from_secs(10800));
        assert_eq!(budgets.disk_bytes, 32 * GIB);

        assert!(budgets.enforce_setup(Duration::from_secs(1800)).is_ok());
        assert!(matches!(
            budgets.enforce_setup(Duration::from_secs(1801)),
            Err(BudgetError::Setup { .. })
        ));
        assert!(budgets.enforce_total(Duration::from_secs(10800)).is_ok());
        assert!(matches!(
            budgets.enforce_total(Duration::from_secs(10801)),
            Err(BudgetError::Total { .. })
        ));
        assert!(budgets.enforce_disk(32 * GIB).is_ok());
        assert!(matches!(
            budgets.enforce_disk(32 * GIB + 1),
            Err(BudgetError::Disk { .. })
        ));
    }

    /// Build a reuse fingerprint for the default qualification shape.
    fn fingerprint() -> RunFingerprint {
        RunFingerprint {
            qualified_source_commit: TEST_COMMIT.to_owned(),
            dataset_digest: TEST_DIGEST.to_owned(),
            shape: DatasetShape::new(4, 200_000).expect("valid shape"),
            storage_config: "local".to_owned(),
        }
    }

    #[test]
    fn reuse_accepts_identical_fingerprint() {
        assert!(fingerprint().verify_reuse(&fingerprint()).is_ok());
    }

    #[test]
    fn reuse_refuses_each_mismatched_field() {
        let recorded = fingerprint();

        let mut commit = fingerprint();
        commit.qualified_source_commit = "f".repeat(40);
        assert!(matches!(
            recorded.verify_reuse(&commit),
            Err(ReuseError::Mismatch {
                field: "qualified_source_commit",
                ..
            })
        ));

        let mut digest = fingerprint();
        digest.dataset_digest = "a".repeat(64);
        assert!(matches!(
            recorded.verify_reuse(&digest),
            Err(ReuseError::Mismatch {
                field: "dataset_digest",
                ..
            })
        ));

        let mut shape = fingerprint();
        shape.shape = DatasetShape::new(4, 800_000).expect("valid shape");
        assert!(matches!(
            recorded.verify_reuse(&shape),
            Err(ReuseError::Mismatch { field: "shape", .. })
        ));

        let mut storage = fingerprint();
        storage.storage_config = "s3".to_owned();
        assert!(matches!(
            recorded.verify_reuse(&storage),
            Err(ReuseError::Mismatch {
                field: "storage_config",
                ..
            })
        ));
    }

    /// Build deterministic manifest inputs for assembly tests.
    fn manifest_inputs() -> ManifestInputs {
        ManifestInputs {
            run_id: "run-xyz".to_owned(),
            qualified_source_commit: TEST_COMMIT.to_owned(),
            source_tree_clean: true,
            dataset_shape: DatasetShape::new(4, 200_000).expect("valid shape"),
            dataset_digest: TEST_DIGEST.to_owned(),
            workload_digest: "b".repeat(64),
            topology: TopologyIdentity {
                topology_id: "topo-1".to_owned(),
                oracle_pods: 1,
            },
            environment: EnvironmentIdentity {
                cpu: None,
                memory_bytes: None,
                os: "linux".to_owned(),
                storage_mode: "local".to_owned(),
                unavailable_reason: Some("probes unimplemented".to_owned()),
            },
            budgets: RunBudgets::default(),
            actual_durations: ActualDurations {
                setup_seconds: 60,
                total_seconds: 600,
                disk_bytes_used: 1024,
            },
            command_lines: vec!["mise run bench:bifrost:qualification".to_owned()],
        }
    }

    /// Build the four report entries in a fixed order.
    fn report_entries() -> Vec<ReportDigestEntry> {
        ["ingest", "oracle", "distributed", "mixed"]
            .into_iter()
            .map(|family| ReportDigestEntry {
                family: family.to_owned(),
                path: format!("{family}-qualification.json"),
                digest: TEST_DIGEST.to_owned(),
            })
            .collect()
    }

    #[test]
    fn manifest_assembly_is_deterministic_and_preseal() {
        let verdicts = RunVerdicts {
            correctness_passed: true,
            saturation_reached: false,
            recovery_recovered: true,
        };
        let first = QualificationManifest::assemble(manifest_inputs(), report_entries(), verdicts);
        let second = QualificationManifest::assemble(manifest_inputs(), report_entries(), verdicts);
        assert_eq!(
            first.to_canonical_json().expect("serialize first"),
            second.to_canonical_json().expect("serialize second")
        );
        assert_eq!(first.schema_version, QUALIFICATION_MANIFEST_SCHEMA);
        assert_eq!(first.oracle_query_accounting, ORACLE_QUERY_ACCOUNTING);
        assert!(first.review.is_none());
        assert!(first.evidence_commit.is_none());
    }

    #[test]
    fn seal_populates_review_and_evidence_commit() {
        let manifest = QualificationManifest::assemble(
            manifest_inputs(),
            report_entries(),
            RunVerdicts {
                correctness_passed: true,
                saturation_reached: false,
                recovery_recovered: true,
            },
        );
        let sealed = manifest.seal(
            "review.md".to_owned(),
            "c".repeat(64),
            "89abcdef89abcdef89abcdef89abcdef89abcdef".to_owned(),
        );
        assert_eq!(sealed.review.as_ref().expect("review").path, "review.md");
        assert_eq!(
            sealed.evidence_commit.as_deref(),
            Some("89abcdef89abcdef89abcdef89abcdef89abcdef")
        );
    }

    /// Provisioner double that counts calls and reports a fixed digest/elapsed.
    struct CountingProvisioner {
        /// Number of times [`RunDatasetProvisioner::provision`] was invoked.
        calls: AtomicUsize,
        /// Elapsed the double reports as the setup duration.
        setup_elapsed: Duration,
    }

    #[async_trait]
    impl RunDatasetProvisioner for CountingProvisioner {
        async fn provision(
            &self,
            _shape: DatasetShape,
            _run_root: &Path,
        ) -> Result<ProvisionOutcome, QualificationRunError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ProvisionOutcome {
                dataset_digest: TEST_DIGEST.to_owned(),
                setup_elapsed: self.setup_elapsed,
            })
        }
    }

    /// Family executor double that records dispatch order.
    struct RecordingExecutor {
        /// Families in the exact order they were dispatched.
        order: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl FamilyExecutor for RecordingExecutor {
        async fn run_family(
            &self,
            family: &str,
            _dataset_digest: &str,
        ) -> Result<FamilyOutcome, QualificationRunError> {
            self.order
                .lock()
                .expect("order lock")
                .push(family.to_owned());
            Ok(FamilyOutcome {
                family: family.to_owned(),
                report_path: format!("{family}-qualification.json"),
                report_digest: TEST_DIGEST.to_owned(),
                correctness_passed: true,
                saturation_reached: false,
                recovery_recovered: true,
            })
        }
    }

    /// Orchestration config running four families over a nonexistent run root.
    ///
    /// The run root does not exist, so the disk walk reports zero bytes and the
    /// disk budget is satisfied without touching the filesystem.
    fn orchestration_config() -> OrchestrationConfig {
        OrchestrationConfig {
            source_tree_clean: true,
            workload_digest: "b".repeat(64),
            topology: TopologyIdentity {
                topology_id: "topo-1".to_owned(),
                oracle_pods: 1,
            },
            environment: EnvironmentIdentity {
                cpu: None,
                memory_bytes: None,
                os: "linux".to_owned(),
                storage_mode: "local".to_owned(),
                unavailable_reason: Some("probes unimplemented".to_owned()),
            },
            command_lines: vec!["mise run bench:bifrost:qualification".to_owned()],
            families: ["ingest", "oracle", "distributed", "mixed"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        }
    }

    #[tokio::test]
    async fn orchestrate_materializes_once_and_dispatches_serially() {
        let provisioner = CountingProvisioner {
            calls: AtomicUsize::new(0),
            setup_elapsed: Duration::from_secs(60),
        };
        let executor = RecordingExecutor {
            order: Mutex::new(Vec::new()),
        };
        let run = QualificationRun::new(
            "run-xyz".to_owned(),
            TEST_COMMIT.to_owned(),
            DatasetShape::new(4, 200_000).expect("valid shape"),
            RunBudgets::default(),
            PathBuf::from("/nonexistent/bifrost-qualification/run-xyz"),
        );
        let manifest = run
            .orchestrate(
                &provisioner,
                &executor,
                orchestration_config(),
                Duration::from_secs(600),
            )
            .await
            .expect("orchestration succeeds");

        assert_eq!(provisioner.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            *executor.order.lock().expect("order"),
            vec!["ingest", "oracle", "distributed", "mixed"]
        );
        assert_eq!(manifest.reports.len(), 4);
        assert_eq!(manifest.reports[0].family, "ingest");
        assert!(manifest.verdicts.correctness_passed);
        assert!(!manifest.verdicts.saturation_reached);
        assert_eq!(manifest.actual_durations.disk_bytes_used, 0);
    }

    #[tokio::test]
    async fn orchestrate_fails_when_setup_deadline_breached() {
        let provisioner = CountingProvisioner {
            calls: AtomicUsize::new(0),
            setup_elapsed: Duration::from_secs(1801),
        };
        let executor = RecordingExecutor {
            order: Mutex::new(Vec::new()),
        };
        let run = QualificationRun::new(
            "run-xyz".to_owned(),
            TEST_COMMIT.to_owned(),
            DatasetShape::new(4, 200_000).expect("valid shape"),
            RunBudgets::default(),
            PathBuf::from("/nonexistent/bifrost-qualification/run-xyz"),
        );
        let error = run
            .orchestrate(
                &provisioner,
                &executor,
                orchestration_config(),
                Duration::from_secs(600),
            )
            .await
            .expect_err("setup breach fails the run");
        assert!(matches!(
            error,
            QualificationRunError::Budget(BudgetError::Setup { .. })
        ));
        // No family runs once setup fails.
        assert!(executor.order.lock().expect("order").is_empty());
    }
}
