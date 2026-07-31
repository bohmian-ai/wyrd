//! Typed server configuration with TOML file + environment variable loading.
//!
//! Load order: env overrides > TOML file > compiled defaults.

use std::collections::{BTreeSet, HashMap};
use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use wyrd_spec::TenantSlug;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_telemetry::TelemetryConfig;

/// Errors raised during configuration loading or validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Config file at `path` could not be read from disk.
    #[error("config file at {path} could not be read")]
    ReadToml {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Config file at `path` could not be parsed as TOML.
    #[error("config file at {path} could not be parsed as TOML")]
    ParseToml {
        /// Path that failed to parse.
        path: PathBuf,
        /// Underlying TOML deserialization error.
        #[source]
        source: toml::de::Error,
    },
    /// `WYRD_CONFIG_PATH` points at a path that does not exist on disk.
    #[error("WYRD_CONFIG_PATH points at {path}, which does not exist")]
    MissingExplicitTomlPath {
        /// The missing path.
        path: PathBuf,
    },
    /// The signing-key file named by `WYRD_SIGNING_KEY_FILE` could not be read.
    #[error("signing-key file at {path} could not be read")]
    ReadSigningKey {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The sealing-key file named by `WYRD_SEALING_KEY_FILE` could not be read.
    #[error("sealing-key file at {path} could not be read")]
    ReadSealingKey {
        /// Path that failed to read.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// An environment variable is set but contains no value.
    #[error("environment variable {key} is set but empty")]
    EmptyEnvVar {
        /// Variable name.
        key: String,
    },
    /// An environment variable value could not be parsed into the expected type.
    #[error("environment variable {key} could not be parsed: {message}")]
    BadEnvVar {
        /// Variable name.
        key: String,
        /// Human-readable parse failure description.
        message: String,
    },
    /// Two or more mutually exclusive environment variables were both set.
    #[error("environment variables {keys:?} are mutually exclusive; set only one")]
    ConflictingEnvVars {
        /// Variable names that conflict.
        keys: Vec<String>,
    },
    /// HTTP and gRPC bind addresses resolve to the same socket address.
    #[error("HTTP and gRPC bind addresses must differ ({bind})")]
    BindCollision {
        /// The colliding address.
        bind: SocketAddr,
    },
    /// A configuration field failed range or cross-field validation.
    #[error("validation failure: {message}")]
    Invalid {
        /// Human-readable description of the failure.
        message: String,
    },
}

// ──────────────────────────────────────────────────────────────────────────────
// Deployment profile
// ──────────────────────────────────────────────────────────────────────────────

/// Deployment profile that selects hardened or relaxed runtime defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentProfile {
    /// Local development defaults. Relaxed gates, reflection enabled.
    #[default]
    Development,
    /// Production hardened defaults. Reflection disabled, preview gates off.
    Production,
}

impl DeploymentProfile {
    /// Returns `true` when the active profile is `production`.
    #[must_use]
    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }
}

/// Which network transports the server binds and serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum ServeMode {
    /// Serve HTTP and gRPC (default).
    #[default]
    Both,
    /// Serve HTTP only.
    Http,
    /// Serve gRPC only.
    Grpc,
}

impl ServeMode {
    /// True when this mode binds the HTTP listener.
    #[must_use]
    pub fn serves_http(self) -> bool {
        matches!(self, Self::Both | Self::Http)
    }

    /// True when this mode binds the gRPC listener.
    #[must_use]
    pub fn serves_grpc(self) -> bool {
        matches!(self, Self::Both | Self::Grpc)
    }
}

/// Transport-selection configuration.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    /// Transports to bind. Defaults to `Both`.
    #[serde(default)]
    pub mode: ServeMode,
}

/// Boot-time bounds and execution-lane sizing for Scribe.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScribeRuntimeConfig {
    /// Tokio coordination worker count.
    #[serde(default = "default_scribe_coordination_threads")]
    pub coordination_threads: usize,
    /// Ingress CPU worker count.
    #[serde(default = "default_scribe_ingress_cpu_threads")]
    pub ingress_cpu_threads: usize,
    /// Persistence CPU worker count for Parquet preparation and bounded replay work.
    #[serde(default = "default_scribe_persistence_cpu_threads")]
    pub persistence_cpu_threads: usize,
    /// WAL IO worker count.
    #[serde(default = "default_scribe_wal_io_threads")]
    pub wal_io_threads: usize,
    /// Optional Scribe memory budget. When absent, cgroup detection is authoritative.
    #[serde(default)]
    pub memory_limit_bytes: Option<usize>,
    /// Optional Scribe WAL disk budget. When absent, filesystem capacity is authoritative.
    #[serde(default)]
    pub wal_disk_limit_bytes: Option<u64>,
}

/// Independently deployable Bifrost server role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum BifrostRuntimeRole {
    /// WAL-backed ingest and tail service.
    Scribe,
    /// Maintenance and sealed-file lifecycle service.
    Forge,
    /// Retained query execution and peer service.
    Oracle,
}

/// Oracle execution bounds owned by the server boot configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRuntimeConfig {
    /// Private peer advertisement address.
    #[serde(default = "default_oracle_advertise_addr")]
    pub advertise_addr: String,
    /// CPU budget used for admission calibration.
    #[serde(default = "default_oracle_cpu_cores")]
    pub cpu_cores: f64,
    /// Optional memory budget.
    #[serde(default)]
    pub memory_limit_bytes: Option<usize>,
    /// Spill byte ceiling.
    #[serde(default = "default_oracle_spill_limit_bytes")]
    pub spill_limit_bytes: u64,
    /// Concurrent planning permits.
    #[serde(default = "default_oracle_planning_permits")]
    pub planning_permits: usize,
    /// Admission waiters.
    #[serde(default = "default_oracle_admission_waiters")]
    pub admission_waiters: usize,
    /// Maximum remote workers, excluding the leader.
    #[serde(default = "default_oracle_max_workers_per_query")]
    pub max_workers_per_query: usize,
    /// Maximum encoded frame size.
    #[serde(default = "default_oracle_max_frame_bytes")]
    pub max_frame_bytes: usize,
    /// Calibration profile path.
    #[serde(default)]
    pub calibration_profile: PathBuf,
    /// Whether development may start Oracle from an absent or candidate profile.
    ///
    /// Production ignores this switch and always requires an approved profile.
    #[serde(default)]
    pub allow_unapproved_profile: bool,
}

fn default_oracle_advertise_addr() -> String {
    "127.0.0.1:50052".to_owned()
}
fn default_oracle_cpu_cores() -> f64 {
    1.0
}
fn default_oracle_spill_limit_bytes() -> u64 {
    1 << 30
}
fn default_oracle_planning_permits() -> usize {
    2
}
fn default_oracle_admission_waiters() -> usize {
    64
}
fn default_oracle_max_workers_per_query() -> usize {
    2
}
fn default_oracle_max_frame_bytes() -> usize {
    8 * 1024 * 1024
}

impl Default for OracleRuntimeConfig {
    fn default() -> Self {
        Self {
            advertise_addr: default_oracle_advertise_addr(),
            cpu_cores: default_oracle_cpu_cores(),
            memory_limit_bytes: None,
            spill_limit_bytes: default_oracle_spill_limit_bytes(),
            planning_permits: default_oracle_planning_permits(),
            admission_waiters: default_oracle_admission_waiters(),
            max_workers_per_query: default_oracle_max_workers_per_query(),
            max_frame_bytes: default_oracle_max_frame_bytes(),
            calibration_profile: PathBuf::new(),
            allow_unapproved_profile: false,
        }
    }
}

/// Complete benchmark-produced evidence required before Oracle activation.
///
/// The server does not select calibration values. It only verifies that the
/// benchmark owner supplied the complete schema and evidence references before
/// a maintainer may mark the profile approved.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationProfile {
    /// Version of the calibration document schema understood by this server.
    schema_version: u16,
    /// Maintainer-controlled activation status.
    status: OracleCalibrationStatus,
    /// Content digest of the benchmark report that produced this profile.
    generated_from: String,
    /// Source revision exercised by the benchmark.
    source_revision: String,
    /// Hardware, operating-system, and runtime identity.
    environment: OracleCalibrationEnvironment,
    /// Reproducible workload inputs and measurement windows.
    workload: OracleCalibrationWorkload,
    /// Topology, tenant, class, and visibility coverage.
    matrix: OracleCalibrationMatrix,
    /// Measured slot shape used to derive the proposal.
    slot: OracleCalibrationSlot,
    /// Measured per-class allocation shape.
    class: OracleCalibrationClasses,
    /// Proposed runtime limits, each paired with an evidence case identifier.
    proposal: toml::Table,
    /// Required outcome measurements, each paired with an evidence case identifier.
    measurements: toml::Table,
}

/// Environment identity recorded by an Oracle calibration run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationEnvironment {
    /// Hardware profile used by the run.
    hardware: String,
    /// Operating-system profile used by the run.
    os: String,
    /// Rust/runtime profile used by the run.
    runtime: String,
}

/// Reproducible workload identity recorded by an Oracle calibration run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationWorkload {
    /// Content hashes for all workload definitions and fixtures.
    hashes: Vec<String>,
    /// Random seeds used by measured cases.
    seeds: Vec<u64>,
    /// Input data volumes exercised by measured cases.
    data_volumes_bytes: Vec<u64>,
    /// Warmup interval excluded from measurement.
    warmup_seconds: u64,
    /// Measurement interval used for reported results.
    measurement_seconds: u64,
}

/// Coverage matrix recorded by an Oracle calibration run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationMatrix {
    /// Cluster topologies exercised by the run.
    topology: Vec<String>,
    /// Tenant modes exercised by the run.
    tenant: Vec<String>,
    /// Query classes exercised by the run.
    class: Vec<String>,
    /// Visibility modes exercised by the run.
    visibility: Vec<String>,
}

/// Measured slot shape recorded by an Oracle calibration run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationSlot {
    /// CPU cores assigned to one measured slot.
    cpu_cores: f64,
    /// Memory bytes assigned to one measured slot.
    memory_bytes: u64,
    /// Fraction of capacity available after safety headroom.
    headroom: f64,
}

/// Per-class allocation shapes recorded by an Oracle calibration run.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationClasses {
    /// Interactive-query allocation shape.
    interactive: OracleCalibrationClass,
    /// Analytical-query allocation shape.
    analytical: OracleCalibrationClass,
}

/// Measured allocation shape for one query class.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OracleCalibrationClass {
    /// Fraction of measured capacity assigned to the class.
    share: f64,
    /// Minimum slots needed to admit the class.
    minimum_slots: u64,
}

/// Closed activation status accepted from an Oracle calibration profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OracleCalibrationStatus {
    /// Benchmark evidence exists but has not been approved for production.
    Candidate,
    /// A maintainer approved the measured profile for production activation.
    Approved,
}

/// Proposal leaves required by the schema-v1 Oracle calibration contract.
const ORACLE_CALIBRATION_PROPOSALS: &[&str] = &[
    "slot.cpu_cores_per_slot",
    "slot.memory_bytes_per_slot",
    "slot.headroom_factor",
    "class.interactive.share",
    "class.interactive.minimum_slots",
    "class.analytical.share",
    "class.analytical.minimum_slots",
    "tenant.single_tenant_limit",
    "tenant.multi_tenant_default_limit",
    "classification.assumed_scan_bytes_per_second",
    "classification.analytical_threshold_millis",
    "placement.max_attempts",
    "placement.deadline_millis",
    "placement.jitter_min_millis",
    "placement.jitter_max_millis",
    "lease.cluster_ttl_seconds",
    "lease.renew_interval_seconds",
    "reservation.pending_ttl_seconds",
    "membership.expiration_seconds",
    "tail.fence_ttl_seconds",
    "tail.page_rows",
    "tail.page_encoded_bytes",
    "distribution.max_workers_per_query",
    "distribution.fragment_target_rows",
    "distribution.fragment_target_bytes",
    "distribution.max_fragment_bytes",
    "distribution.max_frame_bytes",
    "distribution.max_in_flight_fragments",
    "distribution.max_worker_concurrency",
    "memory.oracle_limit_bytes",
    "memory.class_limits",
    "spill.limit_bytes",
    "performance.p95_query_millis",
    "performance.p99_query_millis",
    "performance.p95_ttfb_millis",
    "performance.p99_ttfb_millis",
    "performance.minimum_rows_per_second",
    "performance.last_stable_concurrency",
    "performance.maximum_tail_page_millis",
    "performance.maximum_object_store_throttle_rate",
];

/// Measurement leaves required by the schema-v1 Oracle calibration contract.
const ORACLE_CALIBRATION_MEASUREMENTS: &[&str] = &[
    "latency.p50_query_millis",
    "latency.p95_query_millis",
    "latency.p99_query_millis",
    "throughput.rows_per_second",
    "correctness.passed_cases",
    "resource.peak_memory_bytes",
    "retry.attempts",
    "rejection.count",
    "audit.records",
    "terminal.count",
];

impl OracleCalibrationProfile {
    /// Validate completeness and internal bounds without choosing runtime values.
    ///
    /// # Errors
    ///
    /// Returns a message when identity, workload, matrix, measured slot/class
    /// shape, proposal evidence, or outcome measurements are absent or invalid.
    fn validate_complete(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("schema_version must be 1".to_owned());
        }
        for (name, value) in [
            ("generated_from", self.generated_from.as_str()),
            ("source_revision", self.source_revision.as_str()),
            ("environment.hardware", self.environment.hardware.as_str()),
            ("environment.os", self.environment.os.as_str()),
            ("environment.runtime", self.environment.runtime.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{name} must not be empty"));
            }
        }
        validate_non_empty_strings("workload.hashes", &self.workload.hashes)?;
        if self.workload.seeds.is_empty() {
            return Err("workload.seeds must not be empty".to_owned());
        }
        if self.workload.data_volumes_bytes.is_empty()
            || self.workload.data_volumes_bytes.contains(&0)
        {
            return Err("workload.data_volumes_bytes must contain positive values".to_owned());
        }
        if self.workload.warmup_seconds == 0 || self.workload.measurement_seconds == 0 {
            return Err(
                "workload warmup_seconds and measurement_seconds must be positive".to_owned(),
            );
        }
        validate_non_empty_strings("matrix.topology", &self.matrix.topology)?;
        validate_non_empty_strings("matrix.tenant", &self.matrix.tenant)?;
        validate_non_empty_strings("matrix.class", &self.matrix.class)?;
        validate_non_empty_strings("matrix.visibility", &self.matrix.visibility)?;

        if !self.slot.cpu_cores.is_finite() || self.slot.cpu_cores <= 0.0 {
            return Err("slot.cpu_cores must be finite and positive".to_owned());
        }
        if self.slot.memory_bytes == 0 {
            return Err("slot.memory_bytes must be positive".to_owned());
        }
        if !self.slot.headroom.is_finite() || self.slot.headroom <= 0.0 || self.slot.headroom > 1.0
        {
            return Err("slot.headroom must be finite and in (0, 1]".to_owned());
        }
        validate_calibration_class("class.interactive", &self.class.interactive)?;
        validate_calibration_class("class.analytical", &self.class.analytical)?;

        validate_evidence_table("proposal", &self.proposal, ORACLE_CALIBRATION_PROPOSALS)?;
        validate_evidence_table(
            "measurements",
            &self.measurements,
            ORACLE_CALIBRATION_MEASUREMENTS,
        )?;
        let max_workers =
            calibration_evidence_value(&self.proposal, "distribution.max_workers_per_query")?
                .as_integer()
                .ok_or_else(|| {
                    "proposal.distribution.max_workers_per_query.value must be an integer"
                        .to_owned()
                })?;
        if !(0..=63).contains(&max_workers) {
            return Err(
                "proposal.distribution.max_workers_per_query.value must be in 0..=63".to_owned(),
            );
        }
        Ok(())
    }
}

/// Validate that a list contains at least one non-empty string.
///
/// # Errors
///
/// Returns a message when the list is empty or contains a blank value.
fn validate_non_empty_strings(name: &str, values: &[String]) -> Result<(), String> {
    if values.is_empty() || values.iter().any(|value| value.trim().is_empty()) {
        return Err(format!("{name} must contain non-empty values"));
    }
    Ok(())
}

/// Validate one measured class allocation shape.
///
/// # Errors
///
/// Returns a message when the share is not a finite fraction or the minimum
/// slot count is zero.
fn validate_calibration_class(name: &str, value: &OracleCalibrationClass) -> Result<(), String> {
    if !value.share.is_finite() || value.share <= 0.0 || value.share > 1.0 {
        return Err(format!("{name}.share must be finite and in (0, 1]"));
    }
    if value.minimum_slots == 0 {
        return Err(format!("{name}.minimum_slots must be positive"));
    }
    Ok(())
}

/// Validate every required evidence leaf in one calibration table.
///
/// # Errors
///
/// Returns a message when a required dotted path is absent, malformed, has an
/// empty case identifier, or includes unsupported sibling fields.
fn validate_evidence_table(
    table_name: &str,
    table: &toml::Table,
    required_paths: &[&str],
) -> Result<(), String> {
    for path in required_paths {
        let leaf = calibration_table_path(table, path)?;
        if leaf.len() != 2 || !leaf.contains_key("value") || !leaf.contains_key("evidence_case_id")
        {
            return Err(format!(
                "{table_name}.{path} must contain exactly value and evidence_case_id"
            ));
        }
        let case_id = leaf["evidence_case_id"]
            .as_str()
            .ok_or_else(|| format!("{table_name}.{path}.evidence_case_id must be a string"))?;
        if case_id.trim().is_empty() {
            return Err(format!(
                "{table_name}.{path}.evidence_case_id must not be empty"
            ));
        }
    }
    Ok(())
}

/// Resolve a dotted calibration path to its evidence leaf table.
///
/// # Errors
///
/// Returns a message when any path segment is absent or not a table.
fn calibration_table_path<'a>(
    table: &'a toml::Table,
    path: &str,
) -> Result<&'a toml::Table, String> {
    let mut current = table;
    let mut segments = path.split('.').peekable();
    while let Some(segment) = segments.next() {
        let value = current
            .get(segment)
            .ok_or_else(|| format!("missing required calibration field {path}"))?;
        let next = value
            .as_table()
            .ok_or_else(|| format!("calibration field {path} must be an evidence table"))?;
        if segments.peek().is_none() {
            return Ok(next);
        }
        current = next;
    }
    Err(format!("invalid empty calibration path {path}"))
}

/// Resolve the measured value stored at a required proposal path.
///
/// # Errors
///
/// Returns a message when the evidence leaf or its value is absent.
fn calibration_evidence_value<'a>(
    table: &'a toml::Table,
    path: &str,
) -> Result<&'a toml::Value, String> {
    calibration_table_path(table, path)?
        .get("value")
        .ok_or_else(|| format!("calibration field {path} is missing value"))
}

/// Role selection and nested runtime bounds for Bifrost.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostRuntimeConfig {
    /// Roles enabled by this process.
    #[serde(default = "default_bifrost_roles")]
    pub roles: BTreeSet<BifrostRuntimeRole>,
    /// Scribe runtime bounds.
    #[serde(default)]
    pub scribe: ScribeRuntimeConfig,
    /// Oracle runtime bounds.
    #[serde(default)]
    pub oracle: OracleRuntimeConfig,
}

fn default_bifrost_roles() -> BTreeSet<BifrostRuntimeRole> {
    [
        BifrostRuntimeRole::Scribe,
        BifrostRuntimeRole::Forge,
        BifrostRuntimeRole::Oracle,
    ]
    .into_iter()
    .collect()
}

impl Default for BifrostRuntimeConfig {
    fn default() -> Self {
        Self {
            roles: default_bifrost_roles(),
            scribe: ScribeRuntimeConfig::default(),
            oracle: OracleRuntimeConfig::default(),
        }
    }
}

fn default_scribe_coordination_threads() -> usize {
    2
}

fn default_scribe_ingress_cpu_threads() -> usize {
    let available = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    (available.saturating_sub(2).max(2) / 3).max(1)
}

fn default_scribe_persistence_cpu_threads() -> usize {
    let available = std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get);
    let budget = available.saturating_sub(2).max(2);
    budget
        .saturating_sub(default_scribe_ingress_cpu_threads())
        .max(1)
}

fn default_scribe_wal_io_threads() -> usize {
    4
}

impl Default for ScribeRuntimeConfig {
    fn default() -> Self {
        Self {
            coordination_threads: default_scribe_coordination_threads(),
            ingress_cpu_threads: default_scribe_ingress_cpu_threads(),
            persistence_cpu_threads: default_scribe_persistence_cpu_threads(),
            wal_io_threads: default_scribe_wal_io_threads(),
            memory_limit_bytes: None,
            wal_disk_limit_bytes: None,
        }
    }
}

impl ScribeRuntimeConfig {
    /// Validate that every configured bound can provide bounded operation.
    pub fn validate(&self) -> Result<(), String> {
        let thread_values = [
            ("coordination_threads", self.coordination_threads),
            ("ingress_cpu_threads", self.ingress_cpu_threads),
            ("persistence_cpu_threads", self.persistence_cpu_threads),
            ("wal_io_threads", self.wal_io_threads),
        ];
        if let Some((name, _value)) = thread_values.into_iter().find(|(_, value)| *value == 0) {
            return Err(format!("scribe.{name} must be at least 1"));
        }
        let min_bytes = 256 * 1024 * 1024;
        if let Some(value) = self.memory_limit_bytes
            && value < min_bytes
        {
            return Err("scribe.memory_limit_bytes must be at least 268435456 bytes".to_owned());
        }
        if let Some(value) = self.wal_disk_limit_bytes
            && value == 0
        {
            return Err("scribe.wal_disk_limit_bytes must be at least 1".to_owned());
        }
        Ok(())
    }
}

/// Prometheus metrics server configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsConfig {
    /// Whether to bind the metrics listener. Defaults to true.
    #[serde(default = "default_metrics_enabled")]
    pub enabled: bool,
    /// Explicit bind address. When `None`, resolves to loopback on the HTTP
    /// port + 1. The endpoint is unauthenticated, so the default deliberately
    /// stays on loopback rather than inheriting the (possibly public) HTTP IP.
    #[serde(default)]
    pub bind: Option<SocketAddr>,
}

fn default_metrics_enabled() -> bool {
    true
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: default_metrics_enabled(),
            bind: None,
        }
    }
}

impl MetricsConfig {
    /// Resolve the concrete metrics bind. When `bind` is unset, use
    /// **loopback** (`127.0.0.1`) on `http_bind`'s port + 1 — NOT `http_bind`'s
    /// IP, which may be `0.0.0.0`. Unauthenticated `/metrics` must not be
    /// public by default.
    ///
    /// Returns `None` when the auto-computed port would overflow (HTTP on port
    /// 65535 has no room for `+ 1`).
    #[must_use]
    pub fn resolved_bind(&self, http_bind: SocketAddr) -> Option<SocketAddr> {
        if let Some(bind) = self.bind {
            return Some(bind);
        }
        let port = http_bind.port().checked_add(1)?;
        Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }

    /// True when the resolved bind is exposed beyond loopback (public IP or the
    /// unspecified `0.0.0.0`/`::` address). Used to warn in production.
    ///
    /// Returns `false` when the bind address cannot be resolved (port overflow).
    #[must_use]
    pub fn is_public_bind(&self, http_bind: SocketAddr) -> bool {
        self.resolved_bind(http_bind)
            .map(|addr| !addr.ip().is_loopback())
            .unwrap_or(false)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Config structs
// ──────────────────────────────────────────────────────────────────────────────

/// Top-level server configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WyrdServerConfig {
    /// Active deployment profile.
    #[serde(default)]
    pub deployment_profile: DeploymentProfile,
    /// HTTP server bind configuration.
    #[serde(default)]
    pub http: HttpConfig,
    /// gRPC server bind configuration.
    #[serde(default)]
    pub grpc: GrpcConfig,
    /// Telemetry and tracing configuration.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Database connection pool configuration.
    #[serde(default)]
    pub pools: PoolsConfig,
    /// Request limits and timeouts.
    #[serde(default)]
    pub limits: LimitsConfig,
    /// Graceful shutdown configuration.
    #[serde(default)]
    pub shutdown: ShutdownConfig,
    /// Transport-selection configuration.
    #[serde(default)]
    pub serve: ServeConfig,
    /// Role-aware Bifrost runtime configuration.
    #[serde(default)]
    pub bifrost: BifrostRuntimeConfig,
    /// Prometheus metrics server configuration.
    #[serde(default)]
    pub metrics: MetricsConfig,
    /// Readiness probe configuration.
    #[serde(default)]
    pub readiness: ReadinessConfig,
    /// Authentication gate configuration.
    #[serde(default)]
    pub auth: AuthConfig,
    /// Trusted OIDC issuers for this deployment.
    #[serde(default)]
    pub trusted_issuers: Vec<IssuerEntry>,
    /// Workload identity bindings for this deployment.
    #[serde(default)]
    pub workload_bindings: Vec<WorkloadBindingEntry>,
}

/// HTTP server bind configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    /// Socket address to bind the HTTP listener.
    #[serde(default = "default_http_bind")]
    pub bind: SocketAddr,
}

/// gRPC server bind configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrpcConfig {
    /// Socket address to bind the gRPC listener.
    #[serde(default = "default_grpc_bind")]
    pub bind: SocketAddr,
    /// Whether to expose gRPC server reflection.
    #[serde(default)]
    pub reflection_enabled: bool,
}

/// Database connection pool configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolsConfig {
    /// Maximum number of pooled database connections.
    #[serde(default = "default_pool_max")]
    pub max_connections: u32,
    /// Maximum time in milliseconds to wait when acquiring a connection.
    #[serde(default = "default_pool_acquire_ms")]
    pub acquire_ms: u64,
}

/// Per-request limits and timeout configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsConfig {
    /// Maximum request body size in bytes.
    ///
    /// The body-limit middleware buffers up to this many bytes per request;
    /// worst-case process memory is `body_bytes × concurrency`.
    #[serde(default = "default_body_bytes")]
    pub body_bytes: usize,
    /// Per-request processing timeout in milliseconds.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Maximum number of in-flight concurrent requests.
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

impl LimitsConfig {
    /// Convert to the runtime-ready [`crate::state::LimitsConfig`] representation.
    #[must_use]
    pub fn into_state(self) -> crate::state::LimitsConfig {
        crate::state::LimitsConfig {
            body_bytes: self.body_bytes,
            timeout: Duration::from_millis(self.timeout_ms),
            concurrency: self.concurrency,
        }
    }
}

/// Graceful shutdown configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShutdownConfig {
    /// Time in milliseconds to wait for in-flight requests to drain.
    #[serde(default = "default_shutdown_drain_ms")]
    pub drain_ms: u64,
}

/// Readiness probe configuration.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessConfig {
    /// Interval in milliseconds between readiness checks.
    #[serde(default = "default_readiness_tick_ms")]
    pub tick_ms: u64,
    /// Per-probe timeout in milliseconds.
    #[serde(default = "default_readiness_probe_timeout_ms")]
    pub probe_timeout_ms: u64,
}

/// Authentication gate configuration.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// When true, auth routes that require preview-gated card registration are allowed.
    #[serde(default)]
    pub allow_preview: bool,
    /// Slug of the implicit tenant this self-hosted deployment serves.
    ///
    /// Boot has no request `Host` to derive the tenant from, so the operator
    /// declares it here (or via `WYRD_SERVER_TENANT_SLUG`). Required when any
    /// `[[trusted_issuers]]` or `[[workload_bindings]]` entry is configured; the
    /// slug is resolved at boot through the same `resolve_by_slug_for_app` path
    /// the request handlers use, so the bound tenant matches request-time lookups.
    #[serde(default)]
    pub tenant_slug: Option<TenantSlug>,
    /// Wyrd's own Ed25519 signing key PEM, used to mint and verify Wyrd JWTs.
    ///
    /// Env-injected only — never read from the TOML file. Loaded at config time
    /// from `WYRD_SIGNING_KEY_FILE` (path to a mounted secret; primary) or
    /// `WYRD_SIGNING_KEY_PEM` (inline PEM; fallback). The paired public key is
    /// derived from this private key at boot, so no public key is configured
    /// separately. `None` when unset; production boot fails closed without it.
    #[serde(skip)]
    pub signing_key: Option<SecretString>,
    /// Base64-encoded 32-byte AES-256-GCM sealing key for issuer client secrets.
    ///
    /// Env-injected only — never read from the TOML file. Loaded at config time
    /// from `WYRD_SEALING_KEY_FILE` (path to a mounted secret; primary) or
    /// `WYRD_SEALING_KEY_BASE64` (inline base64; fallback). Boot decodes this to
    /// a 32-byte key. `None` when unset; boot fails closed if any seeded issuer
    /// carries a client secret without a sealing key configured.
    ///
    /// **Single-key limitation:** there is currently no key-id column or keyring.
    /// All rows are encrypted under this one key; live rotation without downtime
    /// is not yet supported. If the key leaks: (1) rotate the client secrets at
    /// the IdP, (2) re-register the issuers with the new secrets, (3) rotate this
    /// env var. The existing `client_secret_enc` rows then encrypt stale secrets
    /// and are harmless. Tracked in issue #72.
    #[serde(skip)]
    pub sealing_key: Option<SecretString>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Trusted-issuer and workload-binding config DTOs
// ──────────────────────────────────────────────────────────────────────────────

/// How this deployment authenticates to an OIDC provider's token endpoint.
///
/// Maps 1:1 to `wyrd_auth_oidc::ClientAuth`. Boot converts this DTO and moves
/// secret values into the domain type. Secret-bearing variants store a redacted
/// [`SecretString`]; the value is never shown in `Debug` output.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientAuthEntry {
    /// HTTP Basic auth with a shared client secret (RFC 6749 §2.3.1).
    SecretBasic(SecretString),
    /// Secret sent in the token-endpoint POST body (RFC 6749 §2.3.1).
    SecretPost(SecretString),
    /// Private-key JWT client assertion (RFC 7523).
    PrivateKeyJwt,
    /// Public PKCE-only client — no client secret or assertion.
    Public,
}

/// Dotted claim paths for extracting normalized claims from verified tokens.
///
/// `subject` defaults to `"sub"`. No path-format validation is performed here;
/// boot wraps these strings via `wyrd_auth_oidc::ClaimPath::new`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimMappingEntry {
    /// Claim path for the external subject. Defaults to `"sub"`.
    #[serde(default = "ClaimMappingEntry::default_subject")]
    pub subject: String,
    /// Optional claim path for an email address.
    pub email: Option<String>,
    /// Optional claim path for a groups/roles array (e.g. `realm_access.roles`).
    pub groups: Option<String>,
}

impl ClaimMappingEntry {
    fn default_subject() -> String {
        "sub".to_string()
    }
}

impl Default for ClaimMappingEntry {
    fn default() -> Self {
        Self {
            subject: Self::default_subject(),
            email: None,
            groups: None,
        }
    }
}

/// Config DTO for one `[[trusted_issuers]]` entry.
///
/// Maps to `wyrd_auth_oidc::TrustedIssuer` minus boot-derived fields
/// (`jwks_uri`, `tenant_id`). Boot commit 02 converts this DTO and fills those
/// fields via OIDC discovery.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerEntry {
    /// OIDC issuer URL (normalized by boot; must be non-empty).
    pub issuer: String,
    /// Wyrd's OAuth 2.0 `client_id` at this IdP.
    pub client_id: String,
    /// Expected `aud` claim value in tokens from this issuer.
    pub expected_audience: String,
    /// How Wyrd authenticates to this IdP's token endpoint.
    pub client_auth: ClientAuthEntry,
    /// Claim path mapping. Defaults to `{ subject = "sub" }`.
    #[serde(default)]
    pub claim_mapping: ClaimMappingEntry,
    /// Per-issuer map from IdP group strings to Wyrd role names.
    #[serde(default)]
    pub group_role_map: HashMap<String, Vec<String>>,
    /// Baseline Wyrd role names granted to every federated user from this issuer.
    #[serde(default)]
    pub default_roles: Vec<String>,
    /// Whether tokens from this issuer represent human users or machine workloads.
    #[serde(default)]
    pub principal_kind: IssuerTokenPolicy,
    /// JWKS key-cache TTL in seconds. `None` means boot applies its default.
    pub jwks_ttl_secs: Option<u64>,
}

/// Config DTO for one `[[workload_bindings]]` entry.
///
/// Maps to `wyrd_auth_oidc::WorkloadBinding` minus boot-derived fields
/// (`tenant_id`, resolved `card_ref`). Boot commit 03 converts this DTO.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadBindingEntry {
    /// OIDC issuer URL of the issuer that signs tokens for this workload.
    pub issuer: String,
    /// Verified external subject claim value (e.g. a Kubernetes service account).
    pub subject: String,
    /// Optional audience constraint for additional lookup precision.
    pub audience: Option<String>,
    /// Card kind backing the workload principal — `service` or `agent`. Any
    /// other kind is rejected at boot (it can never resolve a principal).
    pub kind: String,
    /// Card name.
    pub name: String,
    /// Space that pins the card identity.
    pub space: String,
    /// Exact card version (no version range).
    pub version: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Default helpers (private)
// ──────────────────────────────────────────────────────────────────────────────

fn default_http_bind() -> SocketAddr {
    "0.0.0.0:8080"
        .parse()
        .expect("static HTTP bind address is valid")
}

fn default_grpc_bind() -> SocketAddr {
    "127.0.0.1:50051"
        .parse()
        .expect("static gRPC bind address is valid")
}

fn default_pool_max() -> u32 {
    32
}

fn default_pool_acquire_ms() -> u64 {
    5_000
}

fn default_body_bytes() -> usize {
    1_048_576
}

fn default_timeout_ms() -> u64 {
    30_000
}

fn default_concurrency() -> usize {
    1_024
}

fn default_shutdown_drain_ms() -> u64 {
    15_000
}

fn default_readiness_tick_ms() -> u64 {
    5_000
}

fn default_readiness_probe_timeout_ms() -> u64 {
    1_500
}

// ──────────────────────────────────────────────────────────────────────────────
// Default impls
// ──────────────────────────────────────────────────────────────────────────────

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            bind: default_http_bind(),
        }
    }
}

impl Default for GrpcConfig {
    fn default() -> Self {
        Self {
            bind: default_grpc_bind(),
            reflection_enabled: false,
        }
    }
}

impl Default for PoolsConfig {
    fn default() -> Self {
        Self {
            max_connections: default_pool_max(),
            acquire_ms: default_pool_acquire_ms(),
        }
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            body_bytes: default_body_bytes(),
            timeout_ms: default_timeout_ms(),
            concurrency: default_concurrency(),
        }
    }
}

impl Default for ShutdownConfig {
    fn default() -> Self {
        Self {
            drain_ms: default_shutdown_drain_ms(),
        }
    }
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self {
            tick_ms: default_readiness_tick_ms(),
            probe_timeout_ms: default_readiness_probe_timeout_ms(),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Loader
// ──────────────────────────────────────────────────────────────────────────────

impl WyrdServerConfig {
    /// Load configuration using the standard load order: env > TOML > defaults.
    ///
    /// # Errors
    /// Returns [`ConfigError`] when the TOML file cannot be read or parsed,
    /// an environment variable is malformed, or validation fails.
    pub fn load() -> Result<Self, ConfigError> {
        let mut config = match Self::pick_toml_path()? {
            Some(path) => {
                let contents =
                    std::fs::read_to_string(&path).map_err(|source| ConfigError::ReadToml {
                        path: path.clone(),
                        source,
                    })?;
                toml::from_str::<Self>(&contents)
                    .map_err(|source| ConfigError::ParseToml { path, source })?
            }
            None => Self::default(),
        };
        config.apply_env_overrides()?;
        config.validate()?;
        Ok(config)
    }

    /// Resolve the TOML config file path.
    ///
    /// Priority: `WYRD_CONFIG_PATH` env var → `/etc/wyrd/server.toml` →
    /// `~/.config/wyrd/server.toml`. Returns `None` if no file is found.
    fn pick_toml_path() -> Result<Option<PathBuf>, ConfigError> {
        if let Some(val) = env_opt("WYRD_CONFIG_PATH")? {
            let path = PathBuf::from(val);
            if !path.exists() {
                return Err(ConfigError::MissingExplicitTomlPath { path });
            }
            return Ok(Some(path));
        }

        let candidates = [
            PathBuf::from("/etc/wyrd/server.toml"),
            PathBuf::from(shellexpand::tilde("~/.config/wyrd/server.toml").as_ref()),
        ];

        for path in &candidates {
            if path.exists() {
                return Ok(Some(path.clone()));
            }
        }

        Ok(None)
    }

    /// Apply environment variable overrides to the loaded configuration.
    ///
    /// Unset variables are silently skipped. Empty variables produce
    /// [`ConfigError::EmptyEnvVar`].
    fn apply_env_overrides(&mut self) -> Result<(), ConfigError> {
        if let Some(value) = env_opt("WYRD_BIFROST_ROLES")? {
            let mut roles = BTreeSet::new();
            for raw in value.split(',').map(str::trim) {
                if raw.is_empty() {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_BIFROST_ROLES".to_owned(),
                        message: "role list contains an empty entry".to_owned(),
                    });
                }
                let role = match raw {
                    "scribe" => BifrostRuntimeRole::Scribe,
                    "forge" => BifrostRuntimeRole::Forge,
                    "oracle" => BifrostRuntimeRole::Oracle,
                    _ => {
                        return Err(ConfigError::BadEnvVar {
                            key: "WYRD_BIFROST_ROLES".to_owned(),
                            message: format!("unknown role {raw:?}"),
                        });
                    }
                };
                if !roles.insert(role) {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_BIFROST_ROLES".to_owned(),
                        message: format!("duplicate role {raw:?}"),
                    });
                }
            }
            if roles.is_empty() {
                return Err(ConfigError::BadEnvVar {
                    key: "WYRD_BIFROST_ROLES".to_owned(),
                    message: "role list must not be empty".to_owned(),
                });
            }
            self.bifrost.roles = roles;
        }
        // deployment_profile (APP_ENV: development | staging | production).
        // staging and production both select the hardened production profile, so
        // both fail closed without a signing key; only development is lenient.
        if let Some(val) = env_opt("APP_ENV")? {
            self.deployment_profile = match val.as_str() {
                "development" => DeploymentProfile::Development,
                "staging" | "production" => DeploymentProfile::Production,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "APP_ENV".to_string(),
                        message: format!(
                            "expected 'development', 'staging', or 'production', got {val:?}"
                        ),
                    });
                }
            };
        }

        // http.bind
        if let Some(val) = env_opt("WYRD_SERVER_BIND")? {
            self.http.bind = val
                .parse::<SocketAddr>()
                .map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_SERVER_BIND".to_string(),
                    message: e.to_string(),
                })?;
        }

        // grpc.bind
        if let Some(val) = env_opt("WYRD_GRPC_BIND")? {
            self.grpc.bind = val
                .parse::<SocketAddr>()
                .map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_GRPC_BIND".to_string(),
                    message: e.to_string(),
                })?;
        }

        // grpc.reflection_enabled
        if let Some(val) = env_opt("WYRD_GRPC_REFLECTION")? {
            self.grpc.reflection_enabled = parse_flag(&val, "WYRD_GRPC_REFLECTION")?;
        }

        // telemetry.endpoint
        if let Some(val) = env_opt("WYRD_OTLP_ENDPOINT")? {
            self.telemetry.endpoint = Some(val);
        }

        // telemetry.service_name
        if let Some(val) = env_opt("WYRD_SERVICE_NAME")? {
            self.telemetry.service_name = Some(val);
        }

        // telemetry.filter
        if let Some(val) = env_opt("WYRD_LOG")? {
            self.telemetry.filter = val;
        }

        // telemetry.protocol — only "grpc" is accepted from env
        if let Some(val) = env_opt("WYRD_OTLP_PROTOCOL")? {
            if val != "grpc" {
                return Err(ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_PROTOCOL".to_string(),
                    message: format!(
                        "only 'grpc' is accepted via env var (use telemetry.protocol = 'http' in wyrd.toml for HTTP OTLP), got {val:?}"
                    ),
                });
            }
            self.telemetry.protocol = wyrd_telemetry::OtlpProtocol::Grpc;
        }

        // telemetry.sample_ratio
        if let Some(val) = env_opt("WYRD_OTLP_SAMPLE_RATIO")? {
            self.telemetry.sample_ratio =
                Some(val.parse::<f64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_SAMPLE_RATIO".to_string(),
                    message: e.to_string(),
                })?);
        }

        // telemetry.export_timeout_ms
        if let Some(val) = env_opt("WYRD_OTLP_EXPORT_TIMEOUT_MS")? {
            self.telemetry.export_timeout_ms =
                Some(val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_OTLP_EXPORT_TIMEOUT_MS".to_string(),
                    message: e.to_string(),
                })?);
        }

        // pools.max_connections
        if let Some(val) = env_opt("WYRD_PG_POOL_MAX")? {
            self.pools.max_connections =
                val.parse::<u32>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_PG_POOL_MAX".to_string(),
                    message: e.to_string(),
                })?;
        }

        // pools.acquire_ms
        if let Some(val) = env_opt("WYRD_PG_POOL_ACQUIRE_MS")? {
            self.pools.acquire_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_PG_POOL_ACQUIRE_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.body_bytes
        if let Some(val) = env_opt("WYRD_HTTP_BODY_LIMIT_BYTES")? {
            self.limits.body_bytes = val.parse::<usize>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_BODY_LIMIT_BYTES".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.timeout_ms
        if let Some(val) = env_opt("WYRD_HTTP_REQUEST_TIMEOUT_MS")? {
            self.limits.timeout_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_REQUEST_TIMEOUT_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // limits.concurrency
        if let Some(val) = env_opt("WYRD_HTTP_CONCURRENCY_LIMIT")? {
            self.limits.concurrency = val.parse::<usize>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_HTTP_CONCURRENCY_LIMIT".to_string(),
                message: e.to_string(),
            })?;
        }

        // shutdown.drain_ms
        if let Some(val) = env_opt("WYRD_SHUTDOWN_DRAIN_MS")? {
            self.shutdown.drain_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_SHUTDOWN_DRAIN_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // serve.mode
        if let Some(val) = env_opt("WYRD_SERVE_MODE")? {
            self.serve.mode = match val.as_str() {
                "both" => ServeMode::Both,
                "http" => ServeMode::Http,
                "grpc" => ServeMode::Grpc,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_SERVE_MODE".to_string(),
                        message: format!("expected 'both', 'http', or 'grpc', got {val:?}"),
                    });
                }
            };
        }

        // metrics.enabled
        if let Some(val) = env_opt("WYRD_METRICS_ENABLED")? {
            self.metrics.enabled = parse_flag(&val, "WYRD_METRICS_ENABLED")?;
        }

        // metrics.bind
        if let Some(val) = env_opt("WYRD_METRICS_BIND")? {
            self.metrics.bind =
                Some(
                    val.parse::<SocketAddr>()
                        .map_err(|e| ConfigError::BadEnvVar {
                            key: "WYRD_METRICS_BIND".to_string(),
                            message: e.to_string(),
                        })?,
                );
        }

        // readiness.tick_ms
        if let Some(val) = env_opt("WYRD_READINESS_TICK_MS")? {
            self.readiness.tick_ms = val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_READINESS_TICK_MS".to_string(),
                message: e.to_string(),
            })?;
        }

        // readiness.probe_timeout_ms
        if let Some(val) = env_opt("WYRD_READINESS_PROBE_TIMEOUT_MS")? {
            self.readiness.probe_timeout_ms =
                val.parse::<u64>().map_err(|e| ConfigError::BadEnvVar {
                    key: "WYRD_READINESS_PROBE_TIMEOUT_MS".to_string(),
                    message: e.to_string(),
                })?;
        }

        // auth.allow_preview
        if let Some(val) = env_opt("WYRD_AUTH_ALLOW_PREVIEW")? {
            self.auth.allow_preview = parse_flag(&val, "WYRD_AUTH_ALLOW_PREVIEW")?;
        }

        // auth.tenant_slug
        if let Some(val) = env_opt("WYRD_SERVER_TENANT_SLUG")? {
            let slug = TenantSlug::new(val).map_err(|e| ConfigError::BadEnvVar {
                key: "WYRD_SERVER_TENANT_SLUG".to_string(),
                message: e.to_string(),
            })?;
            self.auth.tenant_slug = Some(slug);
        }

        // auth.signing_key (WYRD_SIGNING_KEY_FILE primary, WYRD_SIGNING_KEY_PEM fallback)
        if let Some(key) = load_signing_key()? {
            self.auth.signing_key = Some(key);
        }

        // auth.sealing_key (WYRD_SEALING_KEY_FILE primary, WYRD_SEALING_KEY_BASE64 fallback)
        if let Some(key) = load_sealing_key()? {
            self.auth.sealing_key = Some(key);
        }

        Ok(())
    }

    /// Validate the final assembled configuration.
    ///
    /// # Errors
    /// Returns [`ConfigError`] for any violated constraint.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.bifrost.roles.is_empty() {
            return Err(ConfigError::Invalid {
                message: "bifrost.roles must not be empty".to_owned(),
            });
        }
        if self.bifrost.oracle.max_workers_per_query > 63 {
            return Err(ConfigError::Invalid {
                message: "bifrost.oracle.max_workers_per_query must be at most 63".to_owned(),
            });
        }
        if self.bifrost.oracle.planning_permits == 0
            || self.bifrost.oracle.admission_waiters == 0
            || self.bifrost.oracle.max_frame_bytes == 0
            || self.bifrost.oracle.spill_limit_bytes == 0
            || !self.bifrost.oracle.cpu_cores.is_finite()
            || self.bifrost.oracle.cpu_cores <= 0.0
        {
            return Err(ConfigError::Invalid {
                message: "bifrost.oracle bounds must be positive and finite".to_owned(),
            });
        }
        self.bifrost
            .scribe
            .validate()
            .map_err(|message| ConfigError::Invalid { message })?;
        self.validate_oracle_calibration()?;

        // 1. HTTP and gRPC bind addresses must differ.
        if self.http.bind == self.grpc.bind {
            return Err(ConfigError::BindCollision {
                bind: self.http.bind,
            });
        }

        // 1b. Metrics bind must not collide with HTTP or gRPC when enabled.
        if self.metrics.enabled {
            let metrics_bind =
                self.metrics
                    .resolved_bind(self.http.bind)
                    .ok_or_else(|| ConfigError::Invalid {
                        message:
                            "metrics port arithmetic overflow: HTTP is on port 65535, leaving \
                              no room for the auto-computed metrics port (http_port + 1)"
                                .to_owned(),
                    })?;
            if metrics_bind == self.http.bind || metrics_bind == self.grpc.bind {
                return Err(ConfigError::BindCollision { bind: metrics_bind });
            }
        }

        // 2. pools.max_connections >= 2
        if self.pools.max_connections < 2 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "pools.max_connections must be >= 2, got {}",
                    self.pools.max_connections
                ),
            });
        }

        // 3. pools.acquire_ms in [100, 60_000]
        if self.pools.acquire_ms < 100 || self.pools.acquire_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "pools.acquire_ms must be in [100, 60000], got {}",
                    self.pools.acquire_ms
                ),
            });
        }

        // 4. limits.body_bytes >= 1 MiB
        if self.limits.body_bytes < 1_048_576 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.body_bytes must be >= 1048576 (1 MiB), got {}",
                    self.limits.body_bytes
                ),
            });
        }

        // 5. limits.timeout_ms in [1_000, 600_000]
        if self.limits.timeout_ms < 1_000 || self.limits.timeout_ms > 600_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.timeout_ms must be in [1000, 600000], got {}",
                    self.limits.timeout_ms
                ),
            });
        }

        // 6. limits.concurrency in [1, 1_048_576]
        if self.limits.concurrency < 1 || self.limits.concurrency > 1_048_576 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "limits.concurrency must be in [1, 1048576], got {}",
                    self.limits.concurrency
                ),
            });
        }

        // 7. shutdown.drain_ms in [1_000, 60_000]
        if self.shutdown.drain_ms < 1_000 || self.shutdown.drain_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "shutdown.drain_ms must be in [1000, 60000], got {}",
                    self.shutdown.drain_ms
                ),
            });
        }

        // 8. readiness.tick_ms in [500, 60_000]
        if self.readiness.tick_ms < 500 || self.readiness.tick_ms > 60_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "readiness.tick_ms must be in [500, 60000], got {}",
                    self.readiness.tick_ms
                ),
            });
        }

        // 9. readiness.probe_timeout_ms in [100, 10_000]
        if self.readiness.probe_timeout_ms < 100 || self.readiness.probe_timeout_ms > 10_000 {
            return Err(ConfigError::Invalid {
                message: format!(
                    "readiness.probe_timeout_ms must be in [100, 10000], got {}",
                    self.readiness.probe_timeout_ms
                ),
            });
        }

        // 10. Production profile hardening.
        if self.deployment_profile.is_production() {
            if self.grpc.reflection_enabled {
                return Err(ConfigError::Invalid {
                    message: "grpc.reflection_enabled must be false in production profile"
                        .to_string(),
                });
            }
            if self.auth.allow_preview {
                return Err(ConfigError::Invalid {
                    message: "auth.allow_preview must be false in production profile".to_string(),
                });
            }
        }

        // 13. telemetry.service_name, when Some, must be non-empty.
        if let Some(name) = &self.telemetry.service_name
            && name.is_empty()
        {
            return Err(ConfigError::Invalid {
                message: "telemetry.service_name must not be empty when set".to_string(),
            });
        }

        // 14. telemetry.endpoint, when set, must be a valid URL with an
        //     accepted scheme and no userinfo, query string, or fragment.
        if let Some(endpoint) = &self.telemetry.endpoint {
            validate_otlp_endpoint(endpoint).map_err(|msg| ConfigError::Invalid {
                message: format!("telemetry.endpoint is invalid: {msg}"),
            })?;
        }

        // 15. telemetry.sample_ratio, when set, must be in [0.0, 1.0].
        if let Some(ratio) = self.telemetry.sample_ratio
            && !(0.0..=1.0).contains(&ratio)
        {
            return Err(ConfigError::Invalid {
                message: format!("telemetry.sample_ratio must be in [0.0, 1.0], got {ratio}"),
            });
        }

        // 16. Each trusted_issuers entry must have non-empty required fields and
        //     coherent client_auth (secret present iff secret_basic/secret_post).
        for (idx, issuer) in self.trusted_issuers.iter().enumerate() {
            let loc = |field: &str| format!("trusted_issuers[{idx}].{field}");

            if issuer.issuer.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("issuer")),
                });
            }
            if issuer.client_id.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("client_id")),
                });
            }
            if issuer.expected_audience.is_empty() {
                return Err(ConfigError::Invalid {
                    message: format!("{} must not be empty", loc("expected_audience")),
                });
            }
            match &issuer.client_auth {
                ClientAuthEntry::SecretBasic(s) | ClientAuthEntry::SecretPost(s) => {
                    if s.expose_secret().is_empty() {
                        return Err(ConfigError::Invalid {
                            message: format!("{} secret must not be empty", loc("client_auth")),
                        });
                    }
                }
                ClientAuthEntry::PrivateKeyJwt | ClientAuthEntry::Public => {}
            }
        }

        // 17. Each workload_bindings entry must have all card-target fields non-empty.
        for (idx, binding) in self.workload_bindings.iter().enumerate() {
            let loc = |field: &str| format!("workload_bindings[{idx}].{field}");

            for (field, value) in [
                ("issuer", binding.issuer.as_str()),
                ("subject", binding.subject.as_str()),
                ("kind", binding.kind.as_str()),
                ("name", binding.name.as_str()),
                ("space", binding.space.as_str()),
                ("version", binding.version.as_str()),
            ] {
                if value.is_empty() {
                    return Err(ConfigError::Invalid {
                        message: format!("{} must not be empty", loc(field)),
                    });
                }
            }
        }

        // 18. Issuers/bindings require an explicit implicit-tenant slug. Boot
        //     binds every issuer/binding to this tenant via the same slug path
        //     the request handlers use; without it boot can resolve no tenant
        //     and must fail closed, so reject the config here.
        if (!self.trusted_issuers.is_empty() || !self.workload_bindings.is_empty())
            && self.auth.tenant_slug.is_none()
        {
            return Err(ConfigError::Invalid {
                message: "[auth] tenant_slug is required when trusted_issuers or \
                          workload_bindings are configured"
                    .to_string(),
            });
        }

        // Metrics endpoint is unauthenticated; warn if it is exposed beyond
        // loopback in production. Not an error — routable scrape is a valid,
        // network-policy-protected choice — but it must be deliberate.
        if self.metrics.enabled
            && self.deployment_profile.is_production()
            && self.metrics.is_public_bind(self.http.bind)
            && let Some(bind) = self.metrics.resolved_bind(self.http.bind)
        {
            tracing::warn!(
                %bind,
                "unauthenticated /metrics is bound to a non-loopback address in \
                 production; ensure a network policy restricts scrape access"
            );
        }

        Ok(())
    }

    /// Validates the activation profile for a configured Oracle role.
    ///
    /// Production accepts only a present schema-v1 profile whose status is
    /// `approved`. Development requires an explicit opt-in before it may use an
    /// absent or candidate profile, which prevents test defaults from silently
    /// diverging from production activation policy.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] when the profile is missing, malformed,
    /// unsupported, unapproved in production, or unapproved without the
    /// development opt-in.
    fn validate_oracle_calibration(&self) -> Result<(), ConfigError> {
        if !self.bifrost.roles.contains(&BifrostRuntimeRole::Oracle) {
            return Ok(());
        }
        if self
            .bifrost
            .oracle
            .calibration_profile
            .as_os_str()
            .is_empty()
        {
            if !self.deployment_profile.is_production()
                && self.bifrost.oracle.allow_unapproved_profile
            {
                return Ok(());
            }
            return Err(ConfigError::Invalid {
                message: "configured Oracle requires bifrost.oracle.calibration_profile; \
                          development may set allow_unapproved_profile=true explicitly"
                    .to_owned(),
            });
        }
        let path = &self.bifrost.oracle.calibration_profile;
        let contents = std::fs::read_to_string(path).map_err(|error| ConfigError::Invalid {
            message: format!(
                "failed to read Oracle calibration profile {}: {error}",
                path.display()
            ),
        })?;
        let profile: OracleCalibrationProfile =
            toml::from_str(&contents).map_err(|error| ConfigError::Invalid {
                message: format!(
                    "failed to parse Oracle calibration profile {}: {error}",
                    path.display()
                ),
            })?;
        profile
            .validate_complete()
            .map_err(|message| ConfigError::Invalid {
                message: format!(
                    "invalid Oracle calibration profile {}: {message}",
                    path.display()
                ),
            })?;
        if profile.status == OracleCalibrationStatus::Approved {
            return Ok(());
        }
        if !self.deployment_profile.is_production() && self.bifrost.oracle.allow_unapproved_profile
        {
            return Ok(());
        }
        Err(ConfigError::Invalid {
            message: "Oracle calibration profile must be approved; development may set \
                      allow_unapproved_profile=true explicitly"
                .to_owned(),
        })
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ──────────────────────────────────────────────────────────────────────────────

/// Read an environment variable, returning `None` if unset and
/// `Err(EmptyEnvVar)` if set but empty.
fn env_opt(key: &str) -> Result<Option<String>, ConfigError> {
    match env::var(key) {
        Ok(v) if v.is_empty() => Err(ConfigError::EmptyEnvVar {
            key: key.to_string(),
        }),
        Ok(v) => Ok(Some(v)),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(ConfigError::BadEnvVar {
            key: key.to_string(),
            message: "value is not valid UTF-8".to_string(),
        }),
    }
}

/// Load Wyrd's own signing-key PEM from the environment.
///
/// `WYRD_SIGNING_KEY_FILE` (a path to a mounted secret) is the primary source;
/// `WYRD_SIGNING_KEY_PEM` (inline PEM) is the fallback. The file form is
/// preferred because a k8s Secret volume keeps the PEM out of the process
/// environment and `env` dumps. Setting both is a configuration error.
fn load_signing_key() -> Result<Option<SecretString>, ConfigError> {
    let file = env_opt("WYRD_SIGNING_KEY_FILE")?;
    let inline = env_opt("WYRD_SIGNING_KEY_PEM")?;
    match (file, inline) {
        (Some(_), Some(_)) => Err(ConfigError::ConflictingEnvVars {
            keys: vec![
                "WYRD_SIGNING_KEY_FILE".to_string(),
                "WYRD_SIGNING_KEY_PEM".to_string(),
            ],
        }),
        (Some(path), None) => {
            let path = PathBuf::from(path);
            let pem = std::fs::read_to_string(&path)
                .map_err(|source| ConfigError::ReadSigningKey { path, source })?;
            Ok(Some(SecretString::from(pem)))
        }
        (None, Some(pem)) => Ok(Some(SecretString::from(pem))),
        (None, None) => Ok(None),
    }
}

/// Load the base64-encoded issuer sealing key from the environment.
///
/// `WYRD_SEALING_KEY_FILE` (a path to a mounted secret) is the primary source;
/// `WYRD_SEALING_KEY_BASE64` (inline base64) is the fallback. The file form is
/// preferred for the same reason as the signing key. Setting both is a
/// configuration error. Surrounding whitespace (e.g. a trailing newline in a
/// mounted secret file) is trimmed; boot decodes the base64 to a 32-byte key.
fn load_sealing_key() -> Result<Option<SecretString>, ConfigError> {
    let file = env_opt("WYRD_SEALING_KEY_FILE")?;
    let inline = env_opt("WYRD_SEALING_KEY_BASE64")?;
    match (file, inline) {
        (Some(_), Some(_)) => Err(ConfigError::ConflictingEnvVars {
            keys: vec![
                "WYRD_SEALING_KEY_FILE".to_string(),
                "WYRD_SEALING_KEY_BASE64".to_string(),
            ],
        }),
        (Some(path), None) => {
            let path = PathBuf::from(path);
            let encoded = std::fs::read_to_string(&path)
                .map_err(|source| ConfigError::ReadSealingKey { path, source })?;
            Ok(Some(SecretString::from(encoded.trim().to_owned())))
        }
        (None, Some(encoded)) => Ok(Some(SecretString::from(encoded.trim().to_owned()))),
        (None, None) => Ok(None),
    }
}

/// Parse a boolean flag from a `0`/`1` string.
fn parse_flag(val: &str, key: &str) -> Result<bool, ConfigError> {
    match val {
        "1" => Ok(true),
        "0" => Ok(false),
        _ => Err(ConfigError::BadEnvVar {
            key: key.to_string(),
            message: format!("expected 0 or 1, got {val:?}"),
        }),
    }
}

/// Validate an OTLP endpoint URL string.
///
/// Accepted schemes: `http`, `https`, `grpc`. Userinfo, query string, and
/// fragment are rejected.
fn validate_otlp_endpoint(url_str: &str) -> Result<(), String> {
    let url = url::Url::parse(url_str).map_err(|e| e.to_string())?;
    let scheme = url.scheme();
    if !["http", "https", "grpc"].contains(&scheme) {
        return Err(format!(
            "scheme must be http, https, or grpc; got {scheme:?}"
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("userinfo (username or password) is not permitted".to_string());
    }
    if url.query().is_some() {
        return Err("query string is not permitted".to_string());
    }
    if url.fragment().is_some() {
        return Err("fragment is not permitted".to_string());
    }
    Ok(())
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Serialize env-var tests so concurrent test threads cannot interfere.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Parse TOML from a string without performing any file I/O.
    fn from_toml_str(s: &str) -> Result<WyrdServerConfig, ConfigError> {
        toml::from_str::<WyrdServerConfig>(s).map_err(|source| ConfigError::ParseToml {
            path: PathBuf::from("<test-string>"),
            source,
        })
    }

    // ── 1. Default config validates ───────────────────────────────────────────

    #[test]
    fn default_config_validates() {
        let mut cfg = WyrdServerConfig::default();
        assert_eq!(cfg.bifrost.roles, default_bifrost_roles());
        cfg.bifrost.oracle.allow_unapproved_profile = true;
        cfg.validate().expect("default config must be valid");
    }

    /// Proves each supported role subset parses without constructing hidden roles.
    #[test]
    fn bifrost_role_sets_parse_exactly() {
        for (source, expected) in [
            ("roles = [\"scribe\"]", vec![BifrostRuntimeRole::Scribe]),
            ("roles = [\"forge\"]", vec![BifrostRuntimeRole::Forge]),
            ("roles = [\"oracle\"]", vec![BifrostRuntimeRole::Oracle]),
            (
                "roles = [\"scribe\", \"forge\", \"oracle\"]",
                vec![
                    BifrostRuntimeRole::Scribe,
                    BifrostRuntimeRole::Forge,
                    BifrostRuntimeRole::Oracle,
                ],
            ),
        ] {
            let config =
                from_toml_str(&format!("[bifrost]\n{source}")).expect("closed role set parses");
            assert_eq!(
                config.bifrost.roles,
                expected.into_iter().collect::<BTreeSet<_>>()
            );
        }
    }

    /// Proves the role environment override rejects malformed or ambiguous lists.
    #[test]
    fn bifrost_role_env_rejects_empty_duplicate_and_unknown_values() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        for value in ["scribe,", "scribe,scribe", "scribe,worker"] {
            temp_env::with_vars([("WYRD_BIFROST_ROLES", Some(value))], || {
                let mut config = WyrdServerConfig::default();
                assert!(
                    config.apply_env_overrides().is_err(),
                    "{value:?} must be rejected"
                );
            });
        }
    }

    /// Proves the environment role set takes precedence over a parsed TOML set.
    #[test]
    fn bifrost_role_env_overrides_toml() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        temp_env::with_vars([("WYRD_BIFROST_ROLES", Some("oracle"))], || {
            let mut config = from_toml_str("[bifrost]\nroles = [\"scribe\"]").expect("TOML parses");
            config
                .apply_env_overrides()
                .expect("closed environment role parses");
            assert_eq!(
                config.bifrost.roles,
                [BifrostRuntimeRole::Oracle].into_iter().collect()
            );
        });
    }

    /// Proves Oracle's protocol and allocation bounds fail closed.
    #[test]
    fn oracle_numeric_bounds_are_validated() {
        let mut config = WyrdServerConfig::default();
        config.bifrost.oracle.allow_unapproved_profile = true;
        config.bifrost.oracle.max_workers_per_query = 64;
        assert!(config.validate().is_err());
        config.bifrost.oracle.max_workers_per_query = 2;
        config.bifrost.oracle.planning_permits = 0;
        assert!(config.validate().is_err());
    }

    /// Builds one complete schema-v1 profile for activation-policy tests.
    fn complete_oracle_calibration(status: &str) -> String {
        let mut profile = format!(
            r#"schema_version = 1
status = "{status}"
generated_from = "sha256:report"
source_revision = "0123456789abcdef"

[environment]
hardware = "test-hardware"
os = "test-os"
runtime = "test-runtime"

[workload]
hashes = ["sha256:workload"]
seeds = [42]
data_volumes_bytes = [1048576]
warmup_seconds = 1
measurement_seconds = 10

[matrix]
topology = ["1", "3", "6"]
tenant = ["single", "multi"]
class = ["interactive", "analytical"]
visibility = ["live", "snapshot"]

[slot]
cpu_cores = 1.0
memory_bytes = 2147483648
headroom = 0.75

[class.interactive]
share = 0.8
minimum_slots = 1

[class.analytical]
share = 0.4
minimum_slots = 2
"#
        );
        for path in ORACLE_CALIBRATION_PROPOSALS {
            profile.push_str(&format!(
                "\n[proposal.{path}]\nvalue = {}\nevidence_case_id = \"case-{path}\"\n",
                if *path == "distribution.max_workers_per_query" {
                    2
                } else {
                    1
                }
            ));
        }
        for path in ORACLE_CALIBRATION_MEASUREMENTS {
            profile.push_str(&format!(
                "\n[measurements.{path}]\nvalue = 1\nevidence_case_id = \"case-{path}\"\n"
            ));
        }
        profile
    }

    /// Proves production accepts only a complete maintainer-approved profile.
    #[test]
    fn oracle_production_calibration_requires_approved_profile() {
        let directory = tempfile::tempdir().expect("calibration temp directory");
        let path = directory.path().join("oracle-calibration.toml");
        std::fs::write(&path, complete_oracle_calibration("candidate"))
            .expect("candidate profile writes");
        let mut config = WyrdServerConfig {
            deployment_profile: DeploymentProfile::Production,
            ..WyrdServerConfig::default()
        };
        config.bifrost.oracle.calibration_profile = path.clone();
        assert!(config.validate().is_err());

        std::fs::write(&path, complete_oracle_calibration("approved"))
            .expect("approved profile writes");
        config
            .validate()
            .expect("approved production calibration validates");
    }

    /// Proves status alone cannot activate Oracle without benchmark evidence.
    #[test]
    fn oracle_calibration_rejects_minimal_approved_profile() {
        let directory = tempfile::tempdir().expect("calibration temp directory");
        let path = directory.path().join("oracle-calibration.toml");
        std::fs::write(&path, "schema_version = 1\nstatus = \"approved\"\n")
            .expect("minimal profile writes");
        let mut config = WyrdServerConfig {
            deployment_profile: DeploymentProfile::Production,
            ..WyrdServerConfig::default()
        };
        config.bifrost.oracle.calibration_profile = path;
        assert!(config.validate().is_err());
    }

    /// Proves every proposal must retain the benchmark case that supports it.
    #[test]
    fn oracle_calibration_rejects_empty_evidence_case() {
        let directory = tempfile::tempdir().expect("calibration temp directory");
        let path = directory.path().join("oracle-calibration.toml");
        let profile = complete_oracle_calibration("approved").replace(
            "evidence_case_id = \"case-slot.cpu_cores_per_slot\"",
            "evidence_case_id = \"\"",
        );
        std::fs::write(&path, profile).expect("invalid profile writes");
        let mut config = WyrdServerConfig {
            deployment_profile: DeploymentProfile::Production,
            ..WyrdServerConfig::default()
        };
        config.bifrost.oracle.calibration_profile = path;
        assert!(config.validate().is_err());
    }

    #[test]
    fn scribe_runtime_defaults_match_bounded_contract() {
        let cfg = ScribeRuntimeConfig::default();
        assert_eq!(cfg.coordination_threads, 2);
        assert_eq!(cfg.memory_limit_bytes, None);
        assert_eq!(cfg.wal_disk_limit_bytes, None);
        cfg.validate().expect("resolved defaults must validate");
    }

    #[test]
    fn scribe_runtime_rejects_zero_and_small_bounds() {
        let cfg = ScribeRuntimeConfig {
            persistence_cpu_threads: 0,
            ..ScribeRuntimeConfig::default()
        };
        assert!(cfg.validate().is_err());

        let mut cfg = ScribeRuntimeConfig {
            memory_limit_bytes: Some(1024),
            ..ScribeRuntimeConfig::default()
        };
        assert!(cfg.validate().is_err());
        cfg.memory_limit_bytes = None;
        cfg.wal_disk_limit_bytes = Some(0);
        assert!(cfg.validate().is_err());
    }

    // ── 2. TOML with unknown legacy field fails with ParseToml ────────────────

    #[test]
    fn unknown_toml_field_fails_parse_toml() {
        let toml = r#"
            port = 9090
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 3. WYRD_SERVER_BIND overrides HTTP bind ───────────────────────────────

    #[test]
    fn env_server_bind_overrides_http() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_BIND", Some("127.0.0.1:9999"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(
                cfg.http.bind,
                "127.0.0.1:9999".parse::<SocketAddr>().unwrap()
            );
        });
    }

    // ── 4. Same-port collision triggers BindCollision ─────────────────────────

    #[test]
    fn same_port_triggers_bind_collision() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars(
            [
                ("WYRD_SERVER_BIND", Some("127.0.0.1:8080")),
                ("WYRD_GRPC_BIND", Some("127.0.0.1:8080")),
            ],
            || {
                let mut cfg = WyrdServerConfig::default();
                cfg.apply_env_overrides().expect("apply succeeds");
                let err = cfg.validate().expect_err("collision must fail");
                assert!(
                    matches!(err, ConfigError::BindCollision { .. }),
                    "expected BindCollision, got {err:?}"
                );
            },
        );
    }

    // ── 5. Production profile + reflection enabled → Invalid ─────────────────

    #[test]
    fn production_reflection_invalid() {
        let toml = r#"
            deployment_profile = "production"
            [grpc]
            bind = "127.0.0.1:50051"
            reflection_enabled = true
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 6. Production profile + allow_preview → Invalid ──────────────────────

    #[test]
    fn production_allow_preview_invalid() {
        let toml = r#"
            deployment_profile = "production"
            [auth]
            allow_preview = true
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 7. shutdown.drain_ms over 60k cap → Invalid ───────────────────────────

    #[test]
    fn shutdown_drain_ms_over_cap_invalid() {
        let toml = r#"
            [shutdown]
            drain_ms = 999999
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 8. Empty env var → EmptyEnvVar ────────────────────────────────────────

    #[test]
    fn empty_env_var_produces_error() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_BIND", Some(""))], || {
            let err = env_opt("WYRD_SERVER_BIND").expect_err("empty var must error");
            assert!(
                matches!(err, ConfigError::EmptyEnvVar { ref key } if key == "WYRD_SERVER_BIND"),
                "expected EmptyEnvVar, got {err:?}"
            );
        });
    }

    // ── 9. tick_ms too low → Invalid ─────────────────────────────────────────

    #[test]
    fn tick_ms_too_low_invalid() {
        let toml = r#"
            [readiness]
            tick_ms = 10
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("must fail");
        assert!(
            matches!(err, ConfigError::Invalid { .. }),
            "expected Invalid, got {err:?}"
        );
    }

    // ── 10. Bad APP_ENV value → BadEnvVar ────────────────────────────────────

    #[test]
    fn bad_app_env_value() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("APP_ENV", Some("qa"))], || {
            let mut cfg = WyrdServerConfig::default();
            let err = cfg
                .apply_env_overrides()
                .expect_err("bad profile must error");
            assert!(
                matches!(err, ConfigError::BadEnvVar { ref key, .. } if key == "APP_ENV"),
                "expected BadEnvVar, got {err:?}"
            );
        });
    }

    #[test]
    fn app_env_staging_selects_hardened_production_profile() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("APP_ENV", Some("staging"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides()
                .expect("staging is a valid APP_ENV");
            assert!(
                cfg.deployment_profile.is_production(),
                "staging must map to the hardened production profile"
            );
        });
    }

    // ── 13. OTLP endpoint with userinfo → Invalid ────────────────────────────

    #[test]
    fn otlp_endpoint_userinfo_invalid() {
        let err = validate_otlp_endpoint("http://user:pass@collector.example.com:4317")
            .expect_err("userinfo must fail");
        assert!(
            err.contains("userinfo"),
            "message should mention userinfo: {err}"
        );
    }

    // ── 14. OTLP endpoint with query string → Invalid ────────────────────────

    #[test]
    fn otlp_endpoint_query_invalid() {
        let err = validate_otlp_endpoint("http://collector.example.com:4317?foo=bar")
            .expect_err("query must fail");
        assert!(err.contains("query"), "message should mention query: {err}");
    }

    // ── 15. Valid OTLP endpoint path → OK ────────────────────────────────────

    #[test]
    fn valid_otlp_endpoint_with_path() {
        validate_otlp_endpoint("http://collector.example.com:4317/v1/traces")
            .expect("endpoint with path must be valid");
        validate_otlp_endpoint("grpc://localhost:4317").expect("grpc endpoint must be valid");
        validate_otlp_endpoint("https://otel.example.com").expect("https endpoint must be valid");
    }

    // ── 16. Absent trusted_issuers defaults to empty ──────────────────────────

    #[test]
    fn trusted_issuers_absent_defaults_to_empty() {
        let cfg = WyrdServerConfig::default();
        assert!(cfg.trusted_issuers.is_empty());
    }

    // ── 17. Absent workload_bindings defaults to empty ────────────────────────

    #[test]
    fn workload_bindings_absent_defaults_to_empty() {
        let cfg = WyrdServerConfig::default();
        assert!(cfg.workload_bindings.is_empty());
    }

    // ── 18. Populated [[trusted_issuers]] round-trips through TOML ───────────

    #[test]
    fn trusted_issuer_toml_round_trip() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com/realms/acme"
            client_id = "wyrd-client"
            expected_audience = "wyrd-client"
            client_auth = { secret_post = "my-secret" }
            principal_kind = "human"
            claim_mapping = { subject = "sub", email = "email", groups = "realm_access.roles" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        assert_eq!(cfg.trusted_issuers.len(), 1);
        let entry = &cfg.trusted_issuers[0];
        assert_eq!(entry.issuer, "https://idp.example.com/realms/acme");
        assert_eq!(entry.client_id, "wyrd-client");
        assert_eq!(entry.expected_audience, "wyrd-client");
        assert_eq!(entry.claim_mapping.subject, "sub");
        assert_eq!(entry.claim_mapping.email.as_deref(), Some("email"));
        assert_eq!(
            entry.claim_mapping.groups.as_deref(),
            Some("realm_access.roles")
        );
    }

    // ── 19. Populated [[workload_bindings]] round-trips through TOML ─────────

    #[test]
    fn workload_binding_toml_round_trip() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            audience = "my-audience"
            kind = "service"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        assert_eq!(cfg.workload_bindings.len(), 1);
        let binding = &cfg.workload_bindings[0];
        assert_eq!(binding.issuer, "https://idp.example.com");
        assert_eq!(binding.subject, "system:serviceaccount:default/my-sa");
        assert_eq!(binding.audience.as_deref(), Some("my-audience"));
        assert_eq!(binding.kind, "service");
        assert_eq!(binding.name, "my-model");
        assert_eq!(binding.space, "prod");
        assert_eq!(binding.version, "1.0.0");
    }

    // ── 20. Unknown field in issuer entry rejected ────────────────────────────

    #[test]
    fn issuer_entry_unknown_field_rejected() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
            unknown_field = "oops"
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 21. Unknown field in binding entry rejected ───────────────────────────

    #[test]
    fn workload_binding_entry_unknown_field_rejected() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:ns/sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
            extra_field = "bad"
        "#;
        let err = from_toml_str(toml).expect_err("unknown field must fail");
        assert!(
            matches!(err, ConfigError::ParseToml { .. }),
            "expected ParseToml, got {err:?}"
        );
    }

    // ── 22. ClientAuthEntry variants all parse correctly ─────────────────────

    #[test]
    fn client_auth_variants_parse() {
        let cases = [
            (r#"client_auth = "public""#, "public"),
            (r#"client_auth = "private_key_jwt""#, "private_key_jwt"),
            (r#"client_auth = { secret_post = "s" }"#, "secret_post"),
            (r#"client_auth = { secret_basic = "s" }"#, "secret_basic"),
        ];
        for (auth_str, label) in cases {
            let toml = format!(
                r#"
                    [[trusted_issuers]]
                    issuer = "https://idp.example.com"
                    client_id = "wyrd"
                    expected_audience = "wyrd"
                    {auth_str}
                "#
            );
            let cfg = from_toml_str(&toml)
                .unwrap_or_else(|e| panic!("{label} variant must parse: {e:?}"));
            assert_eq!(cfg.trusted_issuers.len(), 1, "{label}");
        }
    }

    // ── 23. IssuerTokenPolicy variants parse correctly ───────────────────────

    #[test]
    fn principal_kind_variants_parse() {
        for kind in ["human", "workload"] {
            let toml = format!(
                r#"
                    [[trusted_issuers]]
                    issuer = "https://idp.example.com"
                    client_id = "wyrd"
                    expected_audience = "wyrd"
                    client_auth = "public"
                    principal_kind = "{kind}"
                "#
            );
            let cfg = from_toml_str(&toml)
                .unwrap_or_else(|e| panic!("principal_kind = {kind:?} must parse: {e:?}"));
            assert_eq!(cfg.trusted_issuers.len(), 1);
        }
    }

    // ── 24. Empty issuer URL → Invalid ───────────────────────────────────────

    #[test]
    fn issuer_entry_empty_issuer_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = ""
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty issuer must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("issuer")),
            "expected Invalid(issuer), got {err:?}"
        );
    }

    // ── 25. Empty client_id → Invalid ────────────────────────────────────────

    #[test]
    fn issuer_entry_empty_client_id_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = ""
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty client_id must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("client_id")),
            "expected Invalid(client_id), got {err:?}"
        );
    }

    // ── 26. Empty expected_audience → Invalid ─────────────────────────────────

    #[test]
    fn issuer_entry_empty_expected_audience_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = ""
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg
            .validate()
            .expect_err("empty expected_audience must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("expected_audience")),
            "expected Invalid(expected_audience), got {err:?}"
        );
    }

    // ── 27. Empty secret in secret_post → Invalid ─────────────────────────────

    #[test]
    fn issuer_entry_empty_secret_post_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = { secret_post = "" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty secret must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("client_auth")),
            "expected Invalid(client_auth), got {err:?}"
        );
    }

    // ── 28. Empty issuer in workload binding → Invalid ────────────────────────

    #[test]
    fn binding_entry_empty_issuer_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = ""
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty binding issuer must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("issuer")),
            "expected Invalid(issuer), got {err:?}"
        );
    }

    // ── 29. Empty subject in workload binding → Invalid ───────────────────────

    #[test]
    fn binding_entry_empty_subject_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = ""
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty subject must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("subject")),
            "expected Invalid(subject), got {err:?}"
        );
    }

    // ── 30. Empty card-target field in workload binding → Invalid ─────────────

    #[test]
    fn binding_entry_empty_version_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = ""
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("empty version must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("version")),
            "expected Invalid(version), got {err:?}"
        );
    }

    // ── 31. Client secret never appears in Debug output ───────────────────────

    #[test]
    fn client_secret_not_in_debug_output() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = { secret_post = "super-secret-value" }
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let debug = format!("{cfg:?}");
        assert!(
            !debug.contains("super-secret-value"),
            "secret must be redacted in Debug output, got: {debug}"
        );
    }

    // ── 32. trusted_issuers present without tenant_slug → Invalid ─────────────

    #[test]
    fn issuers_without_tenant_slug_invalid() {
        let toml = r#"
            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("missing tenant_slug must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("tenant_slug")),
            "expected Invalid(tenant_slug), got {err:?}"
        );
    }

    // ── 33. workload_bindings present without tenant_slug → Invalid ───────────

    #[test]
    fn bindings_without_tenant_slug_invalid() {
        let toml = r#"
            [[workload_bindings]]
            issuer = "https://idp.example.com"
            subject = "system:serviceaccount:default/my-sa"
            kind = "model"
            name = "my-model"
            space = "prod"
            version = "1.0.0"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg.validate().expect_err("missing tenant_slug must fail");
        assert!(
            matches!(err, ConfigError::Invalid { ref message } if message.contains("tenant_slug")),
            "expected Invalid(tenant_slug), got {err:?}"
        );
    }

    // ── 34. tenant_slug present satisfies the issuer requirement ──────────────

    #[test]
    fn issuers_with_tenant_slug_valid() {
        let toml = r#"
            [auth]
            tenant_slug = "acme"

            [[trusted_issuers]]
            issuer = "https://idp.example.com"
            client_id = "wyrd"
            expected_audience = "wyrd"
            client_auth = "public"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        cfg.validate().expect("tenant_slug present must validate");
        assert_eq!(
            cfg.auth.tenant_slug.as_ref().map(TenantSlug::as_str),
            Some("acme")
        );
    }

    // ── 35. WYRD_SERVER_TENANT_SLUG overrides [auth] tenant_slug ──────────────

    #[test]
    fn env_tenant_slug_override() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVER_TENANT_SLUG", Some("from-env"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(
                cfg.auth.tenant_slug.as_ref().map(TenantSlug::as_str),
                Some("from-env")
            );
        });
    }

    // ── 36. ServeMode defaults to Both ───────────────────────────────────────

    #[test]
    fn serve_mode_defaults_to_both() {
        let cfg = WyrdServerConfig::default();
        assert_eq!(cfg.serve.mode, ServeMode::Both);
    }

    // ── 37. WYRD_SERVE_MODE env override ────────────────────────────────────

    #[test]
    fn env_serve_mode_overrides() {
        let _guard = ENV_LOCK.lock().unwrap();
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("grpc"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Grpc);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("http"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Http);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("both"))], || {
            let mut cfg = WyrdServerConfig::default();
            cfg.apply_env_overrides().expect("apply succeeds");
            assert_eq!(cfg.serve.mode, ServeMode::Both);
        });
        temp_env::with_vars([("WYRD_SERVE_MODE", Some("invalid"))], || {
            let mut cfg = WyrdServerConfig::default();
            let err = cfg.apply_env_overrides().expect_err("bad value must error");
            assert!(
                matches!(err, ConfigError::BadEnvVar { ref key, .. } if key == "WYRD_SERVE_MODE"),
                "expected BadEnvVar(WYRD_SERVE_MODE), got {err:?}"
            );
        });
    }

    // ── 38. MetricsConfig resolved_bind uses loopback + http_port+1 ──────────

    #[test]
    fn metrics_bind_defaults_to_loopback_port_plus_one() {
        let cfg = MetricsConfig::default();
        let http_bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        let resolved = cfg
            .resolved_bind(http_bind)
            .expect("port 8080 + 1 must not overflow");
        assert_eq!(
            resolved,
            "127.0.0.1:8081".parse::<SocketAddr>().unwrap(),
            "default metrics bind must be loopback:http_port+1"
        );
    }

    #[test]
    fn metrics_bind_overflow_at_port_65535_returns_none() {
        let cfg = MetricsConfig::default();
        let http_bind: SocketAddr = "0.0.0.0:65535".parse().unwrap();
        assert!(
            cfg.resolved_bind(http_bind).is_none(),
            "port 65535 + 1 overflows u16 and must return None"
        );
    }

    // ── 39. MetricsConfig.is_public_bind correctness ─────────────────────────

    #[test]
    fn metrics_default_bind_is_not_public() {
        let http_bind: SocketAddr = "0.0.0.0:8080".parse().unwrap();
        assert!(
            !MetricsConfig::default().is_public_bind(http_bind),
            "default metrics bind must not be public"
        );
        let explicit_public = MetricsConfig {
            enabled: true,
            bind: Some("0.0.0.0:9000".parse().unwrap()),
        };
        assert!(
            explicit_public.is_public_bind(http_bind),
            "explicit 0.0.0.0 metrics bind must be public"
        );
    }

    // ── 40. Metrics bind collision with HTTP → BindCollision ─────────────────

    #[test]
    fn metrics_bind_collision_with_http_invalid() {
        // The default HTTP bind is 0.0.0.0:8080, gRPC is 127.0.0.1:50051.
        // Set metrics.bind to the HTTP port explicitly to trigger a collision.
        let toml = r#"
            [metrics]
            bind = "0.0.0.0:8080"
        "#;
        let cfg = from_toml_str(toml).expect("parses ok");
        let err = cfg
            .validate()
            .expect_err("metrics-HTTP collision must fail");
        assert!(
            matches!(err, ConfigError::BindCollision { .. }),
            "expected BindCollision, got {err:?}"
        );
    }

    // ── 41. ServeMode truth table ─────────────────────────────────────────────

    #[test]
    fn serve_mode_truth_table() {
        assert!(ServeMode::Both.serves_http());
        assert!(ServeMode::Both.serves_grpc());
        assert!(ServeMode::Http.serves_http());
        assert!(!ServeMode::Http.serves_grpc());
        assert!(!ServeMode::Grpc.serves_http());
        assert!(ServeMode::Grpc.serves_grpc());
    }
}
