//! Live qualification-run driver binding T31's orchestration seams to a cluster.
//!
//! T31 ([`super::bench_qualification`]) shipped [`QualificationRun`] behind two
//! injected seams — [`RunDatasetProvisioner`] and [`FamilyExecutor`] — and left
//! the live cluster adapters and the driver entry to T32. This module is that
//! bounded glue: [`LiveQualificationDriver`] owns one run's
//! [`SharedRunResources`] (T48) and implements both seams over it, and
//! [`execute_qualification_run`] is the whole-run entry the
//! `bench:bifrost:qualification` lane drives through the
//! `bench_bifrost_qualification` binary.
//!
//! The driver adds no new contract, budget, selection rule, or validator
//! semantics; it composes the existing owners. The ordered flow it drives is:
//! provision the run's shared resources once, measure a throughput calibration
//! slice on a standalone throwaway cluster, resolve the live effective Oracle
//! memory budget and select the D70 shape, persist the run record, then run
//! [`QualificationRun::orchestrate`] — which materializes the dataset once over
//! the shared resources and dispatches every family over them — and finally
//! record the honest total wall-clock and write the pre-seal manifest.
//!
//! Calibration deliberately runs on a *standalone* cluster rather than the
//! shared resources: the materializer always writes the fixed
//! [`super::bench_materializer::QUALIFICATION_TABLE`], so a calibration
//! materialization over the shared fixture would collide with the run's single
//! real materialization. The standalone calibration cluster is discarded, so the
//! shared resources carry exactly one materialized dataset, as the once-per-run
//! contract requires.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use wyrd_server::config::OracleRuntimeConfig;

use super::bench_dataset::{
    BifrostQualificationDataset, DatasetShape, dataset_manifest_digest, smoke_dataset_shape,
    workload_profiles_digest,
};
use super::bench_families::{
    environment_identity, git_head, git_tree_clean, run_family_with_resources,
};
use super::bench_materializer::{BackpressurePolicy, BifrostDatasetMaterializer};
use super::bench_qualification::{
    Calibration, FamilyExecutor, FamilyOutcome, ManifestError, MemoryBudget, OrchestrationConfig,
    ProvisionOutcome, QualificationRun, QualificationRunError, QualificationRunRecord, RunBudgets,
    RunDatasetProvisioner, RunFingerprint, RunRecordError, ShapeSelectionError,
    select_qualification_shape,
};
use super::bench_report::{TopologyIdentity, sha256_hex};
use super::bench_runner::{RunnerFamily, RunnerInvocation, RunnerTier, topology_for_pods};
use super::cluster::{SharedRunResources, WyrdTestCluster};

/// Default artifact root the qualification run writes beneath.
///
/// Mirrors `bench_runner::DEFAULT_OUTPUT_ROOT`; the driver's own arg parser
/// carries the constant because a bench binary consumes only the public surface
/// and cannot read the crate-private runner constant.
pub const DEFAULT_QUALIFICATION_OUTPUT_ROOT: &str = "target/bifrost-benchmarks/qualification";

/// The four qualification families run serially, in order.
///
/// Each entry binds a manifest family label to the fixture workload id, topology
/// id, and served [`RunnerFamily`] the live executor dispatches for it, matching
/// the per-family `bench:bifrost:qualification:<family>` lanes: `ingest` and the
/// Oracle `oracle`/`mixed` families on `one-pod`, the distributed weak-scaling
/// sweep on `six-pod`.
const QUALIFICATION_FAMILIES: [(&str, &str, &str, RunnerFamily); 4] = [
    ("ingest", "ingest-typical", "one-pod", RunnerFamily::Ingest),
    ("oracle", "q1", "one-pod", RunnerFamily::Query),
    (
        "distributed",
        "distributed-q1",
        "six-pod",
        RunnerFamily::Distributed,
    ),
    ("mixed", "mixed-50-50", "one-pod", RunnerFamily::Mixed),
];

/// Parsed argument surface for the `bench_bifrost_qualification` binary.
///
/// The whole-run driver takes only a run id (grouping every artifact for the
/// run) and an artifact root; unlike [`RunnerInvocation`] it selects no single
/// workload or topology, because it drives every family itself. A bare `--bench`
/// flag (appended by `cargo bench` to a `harness = false` target) is tolerated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualificationDriverArgs {
    /// Run id grouping every artifact for this qualification run.
    pub run_id: String,
    /// Artifact root the run's report tree and manifest are written beneath.
    pub output_root: PathBuf,
}

impl QualificationDriverArgs {
    /// Parse the driver flag surface from process arguments (program name excluded).
    ///
    /// `--run-id` is required so artifacts are deterministically grouped;
    /// `--output-root` defaults to [`DEFAULT_QUALIFICATION_OUTPUT_ROOT`]. A bare
    /// `--bench` flag is ignored so the binary runs unchanged under `cargo bench`.
    ///
    /// # Errors
    /// Returns [`QualificationDriverArgsError::MissingValue`] when a flag lacks
    /// its value, [`QualificationDriverArgsError::UnknownFlag`] for an
    /// unrecognized flag, and [`QualificationDriverArgsError::MissingFlag`] when
    /// `--run-id` is absent.
    pub fn parse<I: IntoIterator<Item = String>>(
        args: I,
    ) -> Result<Self, QualificationDriverArgsError> {
        let mut run_id = None;
        let mut output_root = None;
        let mut iter = args.into_iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--run-id" => {
                    run_id = Some(
                        iter.next()
                            .ok_or(QualificationDriverArgsError::MissingValue("--run-id"))?,
                    );
                }
                "--output-root" => {
                    output_root = Some(PathBuf::from(
                        iter.next()
                            .ok_or(QualificationDriverArgsError::MissingValue("--output-root"))?,
                    ));
                }
                "--bench" => {}
                other => {
                    return Err(QualificationDriverArgsError::UnknownFlag(other.to_owned()));
                }
            }
        }
        Ok(Self {
            run_id: run_id.ok_or(QualificationDriverArgsError::MissingFlag("--run-id"))?,
            output_root: output_root
                .unwrap_or_else(|| PathBuf::from(DEFAULT_QUALIFICATION_OUTPUT_ROOT)),
        })
    }
}

/// A failure parsing the qualification driver's argument surface.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QualificationDriverArgsError {
    /// A flag that requires a value was given none.
    #[error("flag {0} requires a value")]
    MissingValue(&'static str),
    /// A required flag was not provided.
    #[error("required flag {0} was not provided")]
    MissingFlag(&'static str),
    /// An unrecognized flag was provided.
    #[error("unknown flag {0}")]
    UnknownFlag(String),
}

/// A failure raised while driving a live qualification run end to end.
///
/// Each variant names the stage that failed so a run refusal is diagnosable
/// without a debugger: dirty source, shared-resource provisioning, calibration,
/// shape selection, dataset-digest computation, run-record persistence, the
/// orchestrated run itself, or manifest writing.
#[derive(Debug, Error)]
pub enum QualificationDriverError {
    /// The qualified source tree was dirty; the run is refused before any work.
    #[error("qualified source tree is dirty; refusing to run")]
    DirtyTree,
    /// The run's shared resources could not be provisioned.
    #[error("shared run resources provisioning failed: {0}")]
    SharedResources(String),
    /// The throughput calibration slice failed.
    #[error("calibration failed: {0}")]
    Calibration(String),
    /// No ladder shape satisfied the D70 rule within the budgets.
    #[error(transparent)]
    ShapeSelection(#[from] ShapeSelectionError),
    /// The reuse-fingerprint dataset digest could not be computed.
    #[error("dataset digest computation failed: {0}")]
    Dataset(String),
    /// The run record could not be persisted.
    #[error(transparent)]
    RunRecord(#[from] RunRecordError),
    /// The orchestrated run failed.
    #[error(transparent)]
    Run(#[from] QualificationRunError),
    /// The manifest could not be written.
    #[error(transparent)]
    Manifest(#[from] ManifestError),
}

/// Live adapter binding one run's [`SharedRunResources`] to the T31 seams.
///
/// Owns a borrow of the run's shared resources and the run identity, and
/// implements both [`RunDatasetProvisioner`] (materialize the selected shape
/// once over the shared resources) and [`FamilyExecutor`] (dispatch each family
/// over the same shared resources through [`run_family_with_resources`]). A
/// single instance is passed as both seam arguments to
/// [`QualificationRun::orchestrate`]. It additionally exposes [`Self::calibrate`],
/// the standalone throughput slice the driver runs before shape selection.
pub struct LiveQualificationDriver<'a> {
    /// The run's shared fixture, storage root, and oracle peer credentials.
    resources: &'a SharedRunResources,
    /// Run id that groups every family artifact for this run.
    run_id: String,
    /// Artifact root the run's report tree is written beneath.
    output_root: PathBuf,
    /// Setup deadline bounding every materialization's backpressure policy.
    setup_deadline: Duration,
}

impl<'a> LiveQualificationDriver<'a> {
    /// Construct a driver over one run's shared resources and identity.
    #[must_use]
    pub fn new(
        resources: &'a SharedRunResources,
        run_id: String,
        output_root: PathBuf,
        setup_deadline: Duration,
    ) -> Self {
        Self {
            resources,
            run_id,
            output_root,
            setup_deadline,
        }
    }

    /// Measure the calibration throughput on a standalone throwaway cluster.
    ///
    /// Boots a one-pod standalone cluster (not the run's shared resources),
    /// materializes the smoke shape into a scratch directory, and returns the
    /// achieved rows per second. The scratch cluster and directory are discarded,
    /// so the run's shared resources still carry exactly one materialized dataset.
    /// This observation is the sole input to the D70 materialization estimate and
    /// never enters a measured stage.
    ///
    /// # Errors
    /// Returns [`QualificationDriverError::Calibration`] when the standalone
    /// cluster cannot start, exposes no server, the smoke dataset cannot be
    /// constructed, materialization fails, or shutdown fails.
    pub async fn calibrate(&self) -> Result<Calibration, QualificationDriverError> {
        let spec = topology_for_pods(1)
            .map_err(|error| {
                QualificationDriverError::Calibration(format!(
                    "topology resolution failed: {error}"
                ))
            })?
            .spec();
        let scratch = tempfile::tempdir().map_err(|error| {
            QualificationDriverError::Calibration(format!("scratch directory failed: {error}"))
        })?;
        let cluster = WyrdTestCluster::start_spec_with_forge_completion_observer(spec)
            .await
            .map_err(|error| {
                QualificationDriverError::Calibration(format!(
                    "calibration cluster start failed: {error}"
                ))
            })?;
        let rows_per_second = {
            let tenant = cluster.data_tenant_id();
            let server = cluster.server(0).ok_or_else(|| {
                QualificationDriverError::Calibration(
                    "calibration cluster has no server".to_owned(),
                )
            })?;
            let dataset =
                BifrostQualificationDataset::new(smoke_dataset_shape()).map_err(|error| {
                    QualificationDriverError::Calibration(format!(
                        "smoke dataset construction failed: {error}"
                    ))
                })?;
            let policy = BackpressurePolicy::with_deadline(Instant::now() + self.setup_deadline);
            let materializer = BifrostDatasetMaterializer::from_server(
                server,
                dataset,
                vec![tenant],
                policy,
                scratch.path().join("calibration"),
                CancellationToken::new(),
            )
            .await
            .map_err(|error| {
                QualificationDriverError::Calibration(format!(
                    "calibration materializer init failed: {error}"
                ))
            })?;
            materializer
                .materialize()
                .await
                .map_err(|error| {
                    QualificationDriverError::Calibration(format!(
                        "calibration materialization failed: {error}"
                    ))
                })?
                .rows_per_second
        };
        cluster.shutdown_and_inspect().await.map_err(|error| {
            QualificationDriverError::Calibration(format!(
                "calibration cluster shutdown failed: {error}"
            ))
        })?;
        drop(scratch);
        Ok(Calibration { rows_per_second })
    }

    /// Resolve a manifest family label to its fixture and served-family binding.
    ///
    /// Returns the fixture workload id, topology id, and [`RunnerFamily`] for a
    /// known qualification family label, or `None` for an unknown label.
    #[must_use]
    fn family_binding(family: &str) -> Option<(&'static str, &'static str, RunnerFamily)> {
        QUALIFICATION_FAMILIES
            .into_iter()
            .find(|(label, ..)| *label == family)
            .map(|(_, workload, topology, runner_family)| (workload, topology, runner_family))
    }

    /// Read a written family report into a [`FamilyOutcome`] for the manifest.
    ///
    /// Digests the report file bytes and extracts the correctness, saturation,
    /// and recovery signals generically (the typed report bodies differ per
    /// family but share these fields): every stage's `correctness` must be
    /// `passed`, the body's `saturation_reached` flag is taken as-is, and every
    /// stage's `recovery_replay` (when present) must be `passed`. A report with
    /// no stages fails closed on both correctness and recovery.
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Family`] when the report cannot be read or
    /// parsed as JSON.
    fn read_family_outcome(
        &self,
        family: &str,
        report_path: &Path,
    ) -> Result<FamilyOutcome, QualificationRunError> {
        let run_root = self.output_root.join(&self.run_id);
        let report_rel = report_path
            .strip_prefix(&run_root)
            .unwrap_or(report_path)
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(report_path).map_err(|error| {
            QualificationRunError::Family(format!(
                "reading report {}: {error}",
                report_path.display()
            ))
        })?;
        let report_digest = sha256_hex(&bytes);
        let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
            QualificationRunError::Family(format!(
                "parsing report {}: {error}",
                report_path.display()
            ))
        })?;
        let (correctness_passed, recovery_recovered) =
            match value.get("stages").and_then(Value::as_array) {
                Some(stages) if !stages.is_empty() => {
                    let correctness = stages.iter().all(|stage| {
                        stage.get("correctness").and_then(Value::as_str) == Some("passed")
                    });
                    let recovery = stages
                        .iter()
                        .all(|stage| match stage.get("recovery_replay") {
                            None | Some(Value::Null) => true,
                            Some(Value::String(verdict)) => verdict == "passed",
                            Some(_) => false,
                        });
                    (correctness, recovery)
                }
                _ => (false, false),
            };
        let saturation_reached = value
            .get("body")
            .and_then(|body| body.get("saturation_reached"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let topology = value
            .get("topology")
            .cloned()
            .map(serde_json::from_value::<TopologyIdentity>)
            .transpose()
            .map_err(|error| {
                QualificationRunError::Family(format!(
                    "parsing topology in report {}: {error}",
                    report_path.display()
                ))
            })?
            .ok_or_else(|| {
                QualificationRunError::Family(format!(
                    "report {} has no topology field",
                    report_path.display()
                ))
            })?;
        Ok(FamilyOutcome {
            family: family.to_owned(),
            report_path: report_rel,
            report_digest,
            topology,
            correctness_passed,
            saturation_reached,
            recovery_recovered,
        })
    }
}

#[async_trait(?Send)]
impl RunDatasetProvisioner for LiveQualificationDriver<'_> {
    /// Materialize `shape` once into a one-pod cluster over the shared resources.
    ///
    /// Boots a one-pod cluster over the run's [`SharedRunResources`] so the
    /// once-materialized dataset is visible to every later family cluster over the
    /// same shared fixture, materializes the shape into `<run_root>/materialize`,
    /// and returns the fixture-file dataset digest and the measured setup elapsed.
    /// The returned digest is [`dataset_manifest_digest`] — the value every report
    /// stamps as its `dataset_digest`, so the linked manifest matches its reports
    /// (the per-shape runtime digest is the run record's reuse fingerprint, a
    /// separate consistency domain).
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Provision`] when the dataset cannot be
    /// constructed, the topology cannot resolve, the cluster cannot start or
    /// exposes no server, the materializer cannot initialize or materialize, or
    /// the provisioning cluster cannot shut down.
    async fn provision(
        &self,
        shape: DatasetShape,
        run_root: &Path,
    ) -> Result<ProvisionOutcome, QualificationRunError> {
        let dataset = BifrostQualificationDataset::new(shape).map_err(|error| {
            QualificationRunError::Provision(format!("dataset construction failed: {error}"))
        })?;
        let dataset_digest = dataset_manifest_digest();
        let spec = topology_for_pods(1)
            .map_err(|error| {
                QualificationRunError::Provision(format!("topology resolution failed: {error}"))
            })?
            .spec();
        let setup_start = Instant::now();
        let cluster = WyrdTestCluster::start_spec_with_shared_resources(spec, self.resources)
            .await
            .map_err(|error| {
                QualificationRunError::Provision(format!("cluster start failed: {error}"))
            })?;
        {
            let tenant = cluster.data_tenant_id();
            let server = cluster.server(0).ok_or_else(|| {
                QualificationRunError::Provision("provision cluster has no server".to_owned())
            })?;
            let policy = BackpressurePolicy::with_deadline(Instant::now() + self.setup_deadline);
            let materializer = BifrostDatasetMaterializer::from_server(
                server,
                dataset,
                vec![tenant],
                policy,
                run_root.join("materialize"),
                CancellationToken::new(),
            )
            .await
            .map_err(|error| {
                QualificationRunError::Provision(format!("materializer init failed: {error}"))
            })?;
            materializer.materialize().await.map_err(|error| {
                QualificationRunError::Provision(format!("materialization failed: {error}"))
            })?;
        }
        let setup_elapsed = setup_start.elapsed();
        cluster.shutdown_and_inspect().await.map_err(|error| {
            QualificationRunError::Provision(format!("provision cluster shutdown failed: {error}"))
        })?;
        Ok(ProvisionOutcome {
            dataset_digest,
            setup_elapsed,
        })
    }
}

#[async_trait(?Send)]
impl FamilyExecutor for LiveQualificationDriver<'_> {
    /// Run one family over the shared resources at tier `qualification`.
    ///
    /// Resolves the family's fixture workload/topology and served family, builds a
    /// qualification-tier [`RunnerInvocation`] for the run, and dispatches it
    /// through [`run_family_with_resources`] so the family cluster reads the
    /// once-materialized dataset over the shared resources. The written report is
    /// then read into a [`FamilyOutcome`]. `dataset_digest` is threaded by the
    /// orchestrator for the manifest and is an identity value here: the family
    /// runner re-verifies the full run fingerprint through
    /// `acquire_qualification_dataset` before any cluster starts, so it is not
    /// needed to dispatch.
    ///
    /// # Errors
    /// Returns [`QualificationRunError::Family`] for an unknown family label, a
    /// family-runner failure, or a report that cannot be read or parsed.
    async fn run_family(
        &self,
        family: &str,
        dataset_digest: &str,
    ) -> Result<FamilyOutcome, QualificationRunError> {
        let _ = dataset_digest;
        let (workload_id, topology_id, runner_family) =
            Self::family_binding(family).ok_or_else(|| {
                QualificationRunError::Family(format!("unknown qualification family {family:?}"))
            })?;
        let invocation = RunnerInvocation {
            workload_id: workload_id.to_owned(),
            topology_id: topology_id.to_owned(),
            tier: RunnerTier::Qualification,
            output_root: self.output_root.clone(),
            run_id: self.run_id.clone(),
        };
        let report_path = run_family_with_resources(&invocation, &[runner_family], self.resources)
            .await
            .map_err(|error| QualificationRunError::Family(error.to_string()))?;
        self.read_family_outcome(family, &report_path)
    }
}

/// Compute the per-shape reuse-fingerprint digest for the run record.
///
/// This is the runtime dataset-manifest digest of the selected shape, exactly the
/// value `acquire_qualification_dataset` recomputes when a per-family rerun
/// verifies reuse. It is distinct from the fixture-file [`dataset_manifest_digest`]
/// the reports and the linked manifest carry.
///
/// # Errors
/// Returns [`QualificationDriverError::Dataset`] when the dataset cannot be
/// constructed for `shape`.
fn fingerprint_dataset_digest(shape: DatasetShape) -> Result<String, QualificationDriverError> {
    let dataset = BifrostQualificationDataset::new(shape)
        .map_err(|error| QualificationDriverError::Dataset(error.to_string()))?;
    Ok(dataset.manifest().digest)
}

/// Drive one budgeted qualification run end to end and write its pre-seal manifest.
///
/// Refuses a dirty source tree, provisions the run's shared resources, measures
/// the calibration slice, resolves the live effective Oracle memory budget
/// ([`OracleRuntimeConfig::memory_limit_bytes`], else unbounded — the harness
/// leaves the limit unset and does not probe pod memory), selects the D70 shape,
/// persists the run record, then runs [`QualificationRun::orchestrate`] (one
/// materialization over the shared resources, every family serial over them,
/// budgets enforced). Because `orchestrate` performs the work while taking the
/// total elapsed as a parameter, the honest total wall-clock is measured around
/// the whole run, re-enforced against the total budget, and written into the
/// manifest before it is persisted. Returns the written manifest path.
///
/// The manifest's top-level `topology` records the one-pod base/materialization
/// topology; per-family reports carry their own topology (the distributed sweep
/// runs `six-pod`).
///
/// # Errors
/// Returns [`QualificationDriverError`] naming the stage that failed: a dirty
/// tree, shared-resource provisioning, calibration, shape selection, dataset
/// digest, run-record persistence, the orchestrated run (including a budget
/// breach), or manifest writing.
pub async fn execute_qualification_run(
    args: QualificationDriverArgs,
    command_lines: Vec<String>,
) -> Result<PathBuf, QualificationDriverError> {
    if !git_tree_clean() {
        return Err(QualificationDriverError::DirtyTree);
    }
    let run_start = Instant::now();
    let budgets = RunBudgets::default();
    let resources = SharedRunResources::provision()
        .await
        .map_err(|error| QualificationDriverError::SharedResources(error.to_string()))?;
    let driver = LiveQualificationDriver::new(
        &resources,
        args.run_id.clone(),
        args.output_root.clone(),
        budgets.setup,
    );

    let calibration = driver.calibrate().await?;
    let budget = MemoryBudget::resolve(
        OracleRuntimeConfig::default()
            .memory_limit_bytes
            .map(|bytes| bytes as u64),
        None,
    );
    let selection = select_qualification_shape(budget, calibration, budgets.setup.as_secs())?;

    let source_commit = git_head();
    let source_tree_clean = git_tree_clean();
    let fingerprint = RunFingerprint {
        qualified_source_commit: source_commit.clone(),
        dataset_digest: fingerprint_dataset_digest(selection.shape)?,
        shape: selection.shape,
        storage_config: environment_identity().storage_mode,
    };
    let record = QualificationRunRecord::new(
        args.run_id.clone(),
        fingerprint,
        source_tree_clean,
        &selection,
        budgets,
    );
    let run_root = args.output_root.join(&args.run_id);
    record.persist(&run_root)?;

    let run = QualificationRun::new(
        args.run_id.clone(),
        source_commit,
        selection.shape,
        budgets,
        run_root,
    );
    let config = OrchestrationConfig {
        source_tree_clean,
        workload_digest: workload_profiles_digest(),
        topology: TopologyIdentity {
            topology_id: "one-pod".to_owned(),
            oracle_pods: 1,
        },
        environment: environment_identity(),
        command_lines,
        families: QUALIFICATION_FAMILIES
            .into_iter()
            .map(|(label, ..)| label.to_owned())
            .collect(),
    };

    // `orchestrate` performs provisioning and all families internally, so the
    // honest total wall-clock is unknown at its call site. Pass a zero elapsed
    // (its internal total-budget guard is a no-op at that point), then measure the
    // real total around the whole run, re-enforce the total budget against it, and
    // record it into the manifest before writing.
    let mut manifest = run
        .orchestrate(&driver, &driver, config, Duration::ZERO)
        .await?;
    let total_elapsed = run_start.elapsed();
    budgets
        .enforce_total(total_elapsed)
        .map_err(|error| QualificationDriverError::Run(QualificationRunError::Budget(error)))?;
    manifest.actual_durations.total_seconds = total_elapsed.as_secs();

    let manifest_path = manifest.write(run.root())?;
    drop(resources);
    Ok(manifest_path)
}
