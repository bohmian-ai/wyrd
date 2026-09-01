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
use vala_bifrost_redux::scribe::geometry::{ScribeGeometry, ScribeGeometryError};
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

/// Internal process composition for the Forge maintenance topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
#[clap(rename_all = "kebab-case")]
pub enum BifrostTarget {
    /// Serve APIs, schedule Forge work, and run the embedded worker.
    #[default]
    All,
    /// Serve APIs and schedule Forge work without executing tasks.
    Server,
    /// Serve only Oracle query, lifecycle, and persisted follower capabilities.
    Oracle,
    /// Serve only Scribe ingest, tail, and live follower capabilities.
    Scribe,
    /// Run Forge workers without opening public API listeners.
    ForgeWorker,
}

impl BifrostTarget {
    /// Returns whether this role owns public API listeners.
    #[must_use]
    pub(crate) fn serves_api(self) -> bool {
        matches!(self, Self::All | Self::Server | Self::Oracle | Self::Scribe)
    }

    /// Returns whether this target must open the private Bifrost peer listener.
    ///
    /// Peer-listener activation follows selected roles, never the transport
    /// `ServeMode`: any Scribe- or Oracle-bearing target participates in the
    /// peer plane and must be dialable by its peers, while a Forge worker keeps
    /// using its durable assignment path and opens no peer socket.
    #[must_use]
    pub fn serves_peer(self) -> bool {
        matches!(self, Self::All | Self::Server | Self::Oracle | Self::Scribe)
    }
}

/// Forge worker capacity and operational tuning for the current process role.
///
/// Every field except `worker_concurrency` is optional and defaults to the
/// value compiled into `vala_bifrost_redux::forge::ForgeConfig::default()` (or,
/// for `maintenance_interval_secs`, the boot maintenance-interval default). A
/// `[forge]` section that sets nothing therefore reproduces today's compiled
/// behavior byte-for-byte; the resolved values are assembled and validated once
/// at boot in `crate::boot`. Durations are expressed in whole seconds. Unknown
/// keys are rejected at parse time by `deny_unknown_fields`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeRuntimeConfig {
    /// Number of bounded Forge worker executors this process spawns.
    ///
    /// Controls parallelism only. Must be positive (rejected in
    /// [`WyrdServerConfig::validate`]). Default 1.
    #[serde(default = "default_forge_worker_concurrency")]
    pub worker_concurrency: usize,
    /// Maximum active tasks one tenant may hold concurrently (the D78 fairness
    /// bound). When unset it resolves to `worker_concurrency`, preserving
    /// today's coupled behavior; set it to tune per-tenant admission
    /// independently of executor parallelism. May be above or below
    /// `worker_concurrency`. Must be positive when set. Default: tracks
    /// `worker_concurrency`.
    #[serde(default)]
    pub per_tenant_active_cap: Option<usize>,
    /// Age (seconds) after which old Iceberg snapshots become eligible for
    /// expiry. Must be positive when set. Default 432000 (120 hours).
    #[serde(default)]
    pub snapshot_retention_secs: Option<u64>,
    /// Number of snapshots retained along each current/ref ancestry. Must be
    /// positive and must not exceed the internal retained-snapshot traversal
    /// cap. Default 1.
    #[serde(default)]
    pub retain_last: Option<usize>,
    /// Age (seconds) after which an unreferenced object may be deleted by
    /// orphan GC. Must be positive when set. Default 86400 (24 hours).
    #[serde(default)]
    pub orphan_gc_ttl_secs: Option<u64>,
    /// Count of accumulated commits past `retain_last` that makes snapshot
    /// expiry due on its own, independent of compaction backlog. Must be at
    /// least 1 when set. Default 32.
    #[serde(default)]
    pub maintenance_trigger_snapshot_count: Option<usize>,
    /// Oldest-retained-snapshot age (seconds) past which snapshot expiry
    /// becomes due when at least one commit exists past `retain_last`. Paired
    /// with `maintenance_trigger_snapshot_count` as a count-OR-interval
    /// trigger. Must be positive when set. Default 3600 (1 hour).
    #[serde(default)]
    pub maintenance_trigger_interval_secs: Option<u64>,
    /// Maximum object-store listing pages one orphan-GC candidate scan walks
    /// before yielding cleanly to a successor run. Must be at least 1 when set.
    /// Default 1024.
    #[serde(default)]
    pub orphan_gc_max_list_pages: Option<usize>,
    /// Wall-clock budget (seconds) for one orphan-GC run before it yields as
    /// Partial. Must be positive when set. Default 120 (2 minutes).
    #[serde(default)]
    pub orphan_gc_run_budget_secs: Option<u64>,
    /// Interval (seconds) between Forge maintenance scheduler ticks. Must be
    /// positive when set. Default 60.
    #[serde(default)]
    pub maintenance_interval_secs: Option<u64>,
    /// Maximum number of input files a single compaction tick processes. Must
    /// be positive and at least `max_files_per_bin` (an internal limit). Default
    /// 1024.
    #[serde(default)]
    pub max_files_per_tick: Option<usize>,
    /// Maximum input bytes a single compaction tick processes. Must be positive
    /// and within the internal oversized-singleton ceiling. Default 1073741824
    /// (1 GiB).
    #[serde(default)]
    pub max_bytes_per_tick: Option<u64>,
}

const fn default_forge_worker_concurrency() -> usize {
    1
}

impl Default for ForgeRuntimeConfig {
    fn default() -> Self {
        Self {
            worker_concurrency: default_forge_worker_concurrency(),
            per_tenant_active_cap: None,
            snapshot_retention_secs: None,
            retain_last: None,
            orphan_gc_ttl_secs: None,
            maintenance_trigger_snapshot_count: None,
            maintenance_trigger_interval_secs: None,
            orphan_gc_max_list_pages: None,
            orphan_gc_run_budget_secs: None,
            maintenance_interval_secs: None,
            max_files_per_tick: None,
            max_bytes_per_tick: None,
        }
    }
}

impl ForgeRuntimeConfig {
    /// Resolve the per-tenant active cap, falling back to `worker_concurrency`.
    ///
    /// This is the single place the D78 per-tenant fairness bound is derived
    /// from operator config: an unset `per_tenant_active_cap` tracks
    /// `worker_concurrency` so existing deployments keep today's behavior, while
    /// an explicit value decouples the bound from executor parallelism. The
    /// result feeds `ForgeWorkerConfig::per_tenant_active_cap` at worker spawn.
    #[must_use]
    pub fn resolved_per_tenant_active_cap(&self) -> usize {
        self.per_tenant_active_cap
            .unwrap_or(self.worker_concurrency)
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
    /// Optional Scribe WAL disk budget. When absent, filesystem capacity is authoritative.
    #[serde(default)]
    pub wal_disk_limit_bytes: Option<u64>,
    /// Optional past-window bound (seconds) for caller-supplied `wyrd_event_time` validation.
    ///
    /// A caller-supplied `wyrd_event_time` older than this many seconds before server receipt
    /// time is rejected with `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`. When absent the D85
    /// default of 30 days applies. Per-tenant overrides are not supported.
    #[serde(default)]
    pub event_time_past_window_secs: Option<u64>,
    /// Optional future-window bound (seconds) for caller-supplied `wyrd_event_time` validation.
    ///
    /// A caller-supplied `wyrd_event_time` more than this many seconds ahead of server receipt
    /// time is rejected with `WYRD_VALA_400_EVENT_TIME_OUT_OF_RANGE`. When absent the D85
    /// default of 24 hours applies. Per-tenant overrides are not supported.
    #[serde(default)]
    pub event_time_future_window_secs: Option<u64>,
    /// Maximum encoded bytes accepted for one native or OTLP request.
    ///
    /// Scribe reserves a replayable envelope for one request of this size at
    /// boot and refuses to start when the node cannot cover it, so this is a
    /// capacity decision rather than only a validation bound.
    #[serde(default = "default_ingest_request_bytes")]
    pub ingest_request_bytes: usize,
    /// Encoded bytes in one non-empty Scribe WAL segment before rotation.
    ///
    /// This governs WAL segment size only. It does not size a generation, a
    /// row group, a hot object, or a Forge rewrite output.
    #[serde(default = "default_scribe_wal_segment_bytes")]
    pub wal_segment_bytes: u64,
    /// Pod-wide Arrow budget shared by every active shard generation.
    ///
    /// Divided evenly across the fixed sixteen shards and then capped by
    /// [`Self::generation_rotation_ceiling_bytes`] to derive the rotation limit
    /// each shard applies. It is a limit rather than sixteen reservations, so
    /// lowering it narrows every shard together instead of letting the first
    /// shards to fill exclude the rest.
    #[serde(default = "default_scribe_active_generation_budget_bytes")]
    pub active_generation_budget_bytes: u64,
    /// Absolute per-shard active-generation rotation ceiling.
    ///
    /// Applied after the pod-wide budget divides, so a large budget can never
    /// turn one shard into an unbounded memory owner.
    #[serde(default = "default_scribe_generation_rotation_ceiling_bytes")]
    pub generation_rotation_ceiling_bytes: u64,
    /// Maximum active shard-generation age before rotation.
    #[serde(default = "default_scribe_generation_max_age_secs")]
    pub generation_max_age_secs: u64,
    /// Optional per-`SealKey` size that seals one key earlier than its shard.
    ///
    /// A key may seal earlier than the shard it belongs to; it may never seal
    /// later, so a value above the derived per-shard rotation limit is refused.
    #[serde(default)]
    pub seal_key_early_seal_bytes: Option<usize>,
    /// Optional per-`SealKey` age that seals one key earlier than its shard.
    #[serde(default)]
    pub seal_key_max_age_secs: Option<u64>,
    /// Encoded Parquet target for one assembled Scribe hot object.
    ///
    /// Independent of every rotation limit: a generation rotates to bound
    /// memory, while staging assembles across generations toward this size.
    #[serde(default = "default_scribe_staging_target_file_size_bytes")]
    pub staging_target_file_size_bytes: u64,
    /// Maximum field count in one canonical native IPC schema.
    #[serde(default = "default_ingest_native_fields")]
    pub ingest_native_fields: usize,
    /// Maximum record-batch/source count in one canonical native IPC stream.
    #[serde(default = "default_ingest_native_sources")]
    pub ingest_native_sources: usize,
    /// Maximum logical rows or signal records in one request.
    #[serde(default = "default_ingest_rows")]
    pub ingest_rows: usize,
    /// Maximum OTLP resource groups in one request.
    #[serde(default = "default_ingest_otlp_resources")]
    pub ingest_otlp_resources: usize,
    /// Maximum OTLP instrumentation-scope groups in one request.
    #[serde(default = "default_ingest_otlp_scopes")]
    pub ingest_otlp_scopes: usize,
    /// Maximum OTLP signal records in one request.
    #[serde(default = "default_ingest_otlp_records")]
    pub ingest_otlp_records: usize,
    /// Maximum OTLP attribute nodes in one request.
    #[serde(default = "default_ingest_otlp_attributes")]
    pub ingest_otlp_attributes: usize,
    /// Maximum cumulative OTLP key, value, body, and identifier bytes.
    #[serde(default = "default_ingest_otlp_value_bytes")]
    pub ingest_otlp_value_bytes: usize,
    /// Maximum recursive OTLP `AnyValue` nesting depth.
    #[serde(default = "default_ingest_otlp_value_depth")]
    pub ingest_otlp_value_depth: usize,
    /// Maximum distinct event-day partitions in one request.
    #[serde(default = "default_ingest_time_partitions")]
    pub ingest_time_partitions: usize,
    /// Fixed WAL header and digest workspace bytes retained by an ingress root.
    #[serde(default = "default_ingest_wal_workspace_bytes")]
    pub ingest_wal_workspace_bytes: usize,
}

/// Independently deployable Bifrost server role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[clap(rename_all = "snake_case")]
pub enum BifrostRuntimeRole {
    /// WAL-backed ingest and tail service.
    Scribe,
    /// Maintenance scheduling and sealed-file coordination service.
    ForgeCoordinator,
    /// Bounded maintenance task execution service.
    ForgeWorker,
    /// Retained query execution and peer service.
    Oracle,
}

/// Immutable validated Bifrost role set derived from one public process target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BifrostRoles {
    /// Closed selected role set.
    selected: BTreeSet<BifrostRuntimeRole>,
}

impl BifrostRoles {
    /// Constructs the exact effective roles for one public process target.
    #[must_use]
    pub fn for_target(target: BifrostTarget) -> Self {
        let selected = match target {
            BifrostTarget::All => [
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::ForgeCoordinator,
                BifrostRuntimeRole::ForgeWorker,
                BifrostRuntimeRole::Oracle,
            ]
            .into_iter()
            .collect(),
            BifrostTarget::Server => [
                BifrostRuntimeRole::Scribe,
                BifrostRuntimeRole::ForgeCoordinator,
                BifrostRuntimeRole::Oracle,
            ]
            .into_iter()
            .collect(),
            BifrostTarget::Oracle => [BifrostRuntimeRole::Oracle].into_iter().collect(),
            BifrostTarget::Scribe => [BifrostRuntimeRole::Scribe].into_iter().collect(),
            BifrostTarget::ForgeWorker => [BifrostRuntimeRole::ForgeWorker].into_iter().collect(),
        };
        Self { selected }
    }

    /// Reports whether the exact role is selected.
    #[must_use]
    pub fn contains(&self, role: &BifrostRuntimeRole) -> bool {
        self.selected.contains(role)
    }

    /// Reports whether this selected graph owns the public Bifrost listener.
    #[must_use]
    pub fn serves_api(&self) -> bool {
        self.contains(&BifrostRuntimeRole::Scribe) || self.contains(&BifrostRuntimeRole::Oracle)
    }

    /// Returns selected roles in stable order.
    pub fn iter(&self) -> impl Iterator<Item = &BifrostRuntimeRole> {
        self.selected.iter()
    }

    /// Returns the number of selected concrete roles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.selected.len()
    }

    /// Reports whether no Bifrost role is selected.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Reports whether this target owns the shared public Gate.
    #[must_use]
    pub fn serves_gate(&self) -> bool {
        self.contains(&BifrostRuntimeRole::Scribe) || self.contains(&BifrostRuntimeRole::Oracle)
    }
}

/// Oracle execution bounds owned by the server boot configuration.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRuntimeConfig {
    /// CPU budget used for admission calibration.
    #[serde(default = "default_oracle_cpu_cores")]
    pub cpu_cores: f64,
    /// Concurrent planning permits.
    #[serde(default = "default_oracle_planning_permits")]
    pub planning_permits: usize,
    /// Admission waiters.
    #[serde(default = "default_oracle_admission_waiters")]
    pub admission_waiters: usize,
    /// Maximum absolute time a query may wait in the local admission queues.
    #[serde(default = "default_oracle_max_queue_wait_ms")]
    pub max_queue_wait_ms: u64,
    /// Exact delegated units requested after a complete local miss.
    #[serde(default = "default_oracle_delegated_allocation_units")]
    pub delegated_allocation_units: u32,
    /// Background delegated-block renewal cadence in milliseconds.
    #[serde(default = "default_oracle_delegated_renewal_ms")]
    pub delegated_renewal_ms: u64,
    /// Maximum delegated-block validity in milliseconds.
    #[serde(default = "default_oracle_delegated_validity_ms")]
    pub delegated_validity_ms: u64,
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
    /// Root directory for the locally durable Oracle audit WAL.
    #[serde(default)]
    pub audit_wal_root: Option<PathBuf>,
    /// Maximum accepted records retained before relay.
    #[serde(default = "default_audit_wal_max_records")]
    pub audit_wal_max_records: usize,
    /// Maximum accepted WAL bytes retained before relay.
    #[serde(default = "default_audit_wal_max_bytes")]
    pub audit_wal_max_bytes: u64,
    /// Maximum oldest-record age before fail-closed admission.
    #[serde(default = "default_audit_wal_max_age_seconds")]
    pub audit_wal_max_age_seconds: u64,
    /// Maximum records delivered in one relay pass.
    #[serde(default = "default_audit_relay_batch_records")]
    pub audit_relay_batch_records: usize,
    /// Postgres attempt timeout in milliseconds.
    #[serde(default = "default_audit_relay_attempt_timeout_ms")]
    pub audit_relay_attempt_timeout_ms: u64,
    /// Initial bounded retry backoff in milliseconds.
    #[serde(default = "default_audit_relay_backoff_initial_ms")]
    pub audit_relay_backoff_initial_ms: u64,
    /// Maximum bounded retry backoff in milliseconds.
    #[serde(default = "default_audit_relay_backoff_max_ms")]
    pub audit_relay_backoff_max_ms: u64,
    /// Shutdown drain deadline in milliseconds.
    #[serde(default = "default_audit_relay_shutdown_timeout_ms")]
    pub audit_relay_shutdown_timeout_ms: u64,
}

fn default_oracle_cpu_cores() -> f64 {
    1.0
}
fn default_oracle_planning_permits() -> usize {
    2
}
fn default_oracle_admission_waiters() -> usize {
    64
}
/// Default maximum absolute Oracle admission queue wait in milliseconds.
fn default_oracle_max_queue_wait_ms() -> u64 {
    250
}
/// Default exact demand amount; allocation is still bounded by durable availability.
///
/// Each allocation costs one `PostgreSQL` round trip regardless of how many
/// units it returns, so a single-unit block forces one round trip per query and
/// cannot keep pace with concurrent readers. A batch amortizes that cost.
fn default_oracle_delegated_allocation_units() -> u32 {
    vala_bifrost_redux::oracle::DEFAULT_DELEGATED_ALLOCATION_UNITS
}
/// Default renewal cadence inherited from role heartbeat membership.
fn default_oracle_delegated_renewal_ms() -> u64 {
    u64::try_from(vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL.as_millis())
        .expect("role heartbeat interval fits u64 milliseconds")
}
/// Default maximum validity inherited from role liveness membership.
fn default_oracle_delegated_validity_ms() -> u64 {
    u64::try_from(vala_bifrost_redux::cluster::ROLE_LIVENESS_CUTOFF.as_millis())
        .expect("role liveness cutoff fits u64 milliseconds")
}
fn default_oracle_max_workers_per_query() -> usize {
    2
}
fn default_oracle_max_frame_bytes() -> usize {
    8 * 1024 * 1024
}
/// Default maximum accepted WAL records.
fn default_audit_wal_max_records() -> usize {
    100_000
}
/// Default maximum accepted WAL bytes.
fn default_audit_wal_max_bytes() -> u64 {
    1 << 30
}
/// Default maximum oldest accepted record age.
fn default_audit_wal_max_age_seconds() -> u64 {
    300
}
/// Default relay batch size.
fn default_audit_relay_batch_records() -> usize {
    128
}
/// Default relay attempt timeout.
fn default_audit_relay_attempt_timeout_ms() -> u64 {
    5_000
}
/// Default initial relay backoff.
fn default_audit_relay_backoff_initial_ms() -> u64 {
    50
}
/// Default maximum relay backoff.
fn default_audit_relay_backoff_max_ms() -> u64 {
    5_000
}
/// Default relay shutdown timeout.
fn default_audit_relay_shutdown_timeout_ms() -> u64 {
    10_000
}

impl Default for OracleRuntimeConfig {
    fn default() -> Self {
        Self {
            cpu_cores: default_oracle_cpu_cores(),
            planning_permits: default_oracle_planning_permits(),
            admission_waiters: default_oracle_admission_waiters(),
            max_queue_wait_ms: default_oracle_max_queue_wait_ms(),
            delegated_allocation_units: default_oracle_delegated_allocation_units(),
            delegated_renewal_ms: default_oracle_delegated_renewal_ms(),
            delegated_validity_ms: default_oracle_delegated_validity_ms(),
            max_workers_per_query: default_oracle_max_workers_per_query(),
            max_frame_bytes: default_oracle_max_frame_bytes(),
            calibration_profile: PathBuf::new(),
            allow_unapproved_profile: false,
            audit_wal_root: None,
            audit_wal_max_records: default_audit_wal_max_records(),
            audit_wal_max_bytes: default_audit_wal_max_bytes(),
            audit_wal_max_age_seconds: default_audit_wal_max_age_seconds(),
            audit_relay_batch_records: default_audit_relay_batch_records(),
            audit_relay_attempt_timeout_ms: default_audit_relay_attempt_timeout_ms(),
            audit_relay_backoff_initial_ms: default_audit_relay_backoff_initial_ms(),
            audit_relay_backoff_max_ms: default_audit_relay_backoff_max_ms(),
            audit_relay_shutdown_timeout_ms: default_audit_relay_shutdown_timeout_ms(),
        }
    }
}

impl OracleRuntimeConfig {
    /// Translates and validates the delegated-capacity lifecycle configuration.
    ///
    /// # Errors
    ///
    /// Returns a message when allocation is zero, renewal is not shorter than
    /// validity, or validity exceeds the Oracle role-liveness cutoff.
    pub(crate) fn delegated_admission_config(
        &self,
    ) -> Result<vala_bifrost_redux::oracle::DelegatedOracleAdmissionConfig, String> {
        vala_bifrost_redux::oracle::DelegatedOracleAdmissionConfig {
            allocation_units: self.delegated_allocation_units,
            renewal_interval: Duration::from_millis(self.delegated_renewal_ms),
            validity: Duration::from_millis(self.delegated_validity_ms),
            queue_capacity: self.admission_waiters,
        }
        .validate()
        .map_err(|error| error.to_string())
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

/// Primitive admission values passed from server boot into Redux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OracleAdmissionTranslation {
    /// Interactive class slots after headroom and share allocation.
    pub interactive_slots: u32,
    /// Analytical class slots after headroom and share allocation.
    pub analytical_slots: u32,
    /// Single-tenant concurrent ceiling.
    pub single_tenant_ceiling: u32,
    /// Multi-tenant concurrent ceiling.
    pub multi_tenant_ceiling: u32,
    /// Queue capacity copied from runtime configuration.
    pub queue_capacity: u32,
    /// Absolute queue wait cap.
    pub max_queue_wait: Duration,
}

/// Translate validated calibration evidence into private Redux primitives.
///
/// # Errors
/// Returns a message when headroom, class shares, or derived capacities are invalid.
fn translate_oracle_calibration(
    profile: &OracleCalibrationProfile,
    runtime: &OracleRuntimeConfig,
    raw_slots: u32,
) -> Result<OracleAdmissionTranslation, String> {
    if !profile.slot.headroom.is_finite() || !(0.0..1.0).contains(&profile.slot.headroom) {
        return Err("slot.headroom must be finite and in [0, 1)".to_owned());
    }
    for (name, share) in [
        ("class.interactive.share", profile.class.interactive.share),
        ("class.analytical.share", profile.class.analytical.share),
    ] {
        if !share.is_finite() || !(0.0..=1.0).contains(&share) {
            return Err(format!("{name} must be finite and in [0, 1]"));
        }
    }
    let usable_value = f64::from(raw_slots) * (1.0 - profile.slot.headroom);
    let usable = checked_floor_u32(usable_value, "usable Oracle slots")?;
    if usable < 2 {
        return Err("usable Oracle slots must be at least 2".to_owned());
    }
    let interactive_minimum = checked_minimum_slots(
        profile.class.interactive.minimum_slots,
        "class.interactive.minimum_slots",
    )?;
    let analytical_minimum = checked_minimum_slots(
        profile.class.analytical.minimum_slots,
        "class.analytical.minimum_slots",
    )?;
    let interactive = checked_floor_u32(
        f64::from(usable) * profile.class.interactive.share,
        "interactive class allocation",
    )?
    .max(interactive_minimum)
    .max(1);
    let analytical = checked_floor_u32(
        f64::from(usable) * profile.class.analytical.share,
        "analytical class allocation",
    )?
    .max(analytical_minimum)
    .max(1);
    let allocation_sum = interactive
        .checked_add(analytical)
        .ok_or_else(|| "Oracle class allocation sum exceeds u32".to_owned())?;
    if allocation_sum > usable {
        return Err(format!(
            "Oracle class allocation sum {allocation_sum} exceeds usable slots {usable}"
        ));
    }
    let (interactive_slots, analytical_slots) = (interactive, analytical);
    let single = proposal_u32(&profile.proposal, "tenant.single_tenant_limit")?;
    let multi = proposal_u32(&profile.proposal, "tenant.multi_tenant_default_limit")?;
    Ok(OracleAdmissionTranslation {
        interactive_slots,
        analytical_slots,
        single_tenant_ceiling: single.min(interactive_slots.max(analytical_slots)),
        multi_tenant_ceiling: multi.min(interactive_slots.max(analytical_slots)),
        queue_capacity: u32::try_from(runtime.admission_waiters)
            .map_err(|_| "queue capacity exceeds u32".to_owned())?,
        max_queue_wait: Duration::from_millis(runtime.max_queue_wait_ms),
    })
}

/// Converts a finite non-negative slot calculation without saturating casts.
///
/// # Errors
/// Returns an error when the value is non-finite, negative, or exceeds `u32`.
fn checked_floor_u32(value: f64, name: &str) -> Result<u32, String> {
    if !value.is_finite() || value < 0.0 {
        return Err(format!("{name} must be finite and non-negative"));
    }
    let floored = value.floor();
    if floored > f64::from(u32::MAX) {
        return Err(format!("{name} exceeds u32"));
    }
    u32::try_from(floored as u64).map_err(|_| format!("{name} exceeds u32"))
}

/// Converts and validates one measured class minimum.
///
/// # Errors
/// Returns an error when the minimum is zero or exceeds `u32`.
fn checked_minimum_slots(value: u64, name: &str) -> Result<u32, String> {
    let minimum = u32::try_from(value).map_err(|_| format!("{name} exceeds u32"))?;
    if minimum == 0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(minimum)
}

/// Loads the validated calibration profile for server boot translation.
///
/// # Errors
/// Returns a message when a configured profile cannot be read or decoded.
pub(crate) fn load_oracle_admission_translation(
    runtime: &OracleRuntimeConfig,
    raw_slots: u32,
) -> Result<Option<OracleAdmissionTranslation>, String> {
    if runtime.calibration_profile.as_os_str().is_empty() {
        return Ok(None);
    }
    let contents = std::fs::read_to_string(&runtime.calibration_profile)
        .map_err(|error| format!("failed to read Oracle calibration profile: {error}"))?;
    let profile: OracleCalibrationProfile = toml::from_str(&contents)
        .map_err(|error| format!("failed to parse Oracle calibration profile: {error}"))?;
    translate_oracle_calibration(&profile, runtime, raw_slots).map(Some)
}

/// Reads one positive integer calibration proposal leaf.
///
/// # Errors
/// Returns an error when the leaf is missing, non-numeric, or non-positive.
fn proposal_u64(table: &toml::Table, path: &str) -> Result<u64, String> {
    let value = calibration_evidence_value(table, path)?;
    value
        .as_integer()
        .or_else(|| value.as_float().map(|value| value as i64))
        .filter(|value| *value > 0)
        .map(|value| value as u64)
        .ok_or_else(|| format!("proposal.{path}.value must be positive"))
}

/// Reads one calibration proposal leaf constrained to a `u32` capacity.
///
/// # Errors
/// Returns an error when the leaf is invalid or exceeds `u32`.
fn proposal_u32(table: &toml::Table, path: &str) -> Result<u32, String> {
    u32::try_from(proposal_u64(table, path)?)
        .map_err(|_| format!("proposal.{path}.value exceeds u32"))
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
        if !self.slot.headroom.is_finite() || self.slot.headroom < 0.0 || self.slot.headroom >= 1.0
        {
            return Err("slot.headroom must be finite and in [0, 1)".to_owned());
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostRuntimeConfig {
    /// Portable absolute resource caps consumed by the Bifrost-owned detector.
    #[serde(default)]
    pub resources: BifrostResourceConfig,
    /// Scribe runtime bounds.
    #[serde(default)]
    pub scribe: ScribeRuntimeConfig,
    /// Oracle runtime bounds.
    #[serde(default)]
    pub oracle: OracleRuntimeConfig,
    /// Storage I/O bounds applied by this node's one Bifrost storage owner.
    #[serde(default)]
    pub storage: BifrostStorageIoConfig,
    /// Role-neutral private peer plane shared by Scribe and Oracle.
    #[serde(default)]
    pub peer: BifrostPeerConfig,
}

/// Signing and verification material for the independent peer-ticket keyring.
///
/// Peer purpose tickets are signed with a key that is deliberately separate
/// from the north-south workload/JWT signing key, so a user or API token can
/// never be minted into peer authority. All three inputs are file paths;
/// inline private-key values are prohibited.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeerTicketKeyringConfig {
    /// Key ID stamped into every ticket this process issues.
    #[serde(default)]
    pub active_key_id: Option<String>,
    /// PKCS#8 PEM Ed25519 private key used for issuance.
    #[serde(default)]
    pub signing_key_path: Option<PathBuf>,
    /// Versioned JSON manifest of accepted verification keys.
    #[serde(default)]
    pub verifying_keyring_path: Option<PathBuf>,
}

impl PeerTicketKeyringConfig {
    /// Reports whether every keyring input is present.
    #[must_use]
    fn is_complete(&self) -> bool {
        self.active_key_id
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
            && self.signing_key_path.is_some()
            && self.verifying_keyring_path.is_some()
    }

    /// Reports whether no keyring input is present.
    #[must_use]
    fn is_absent(&self) -> bool {
        self.active_key_id.is_none()
            && self.signing_key_path.is_none()
            && self.verifying_keyring_path.is_none()
    }
}

/// Role-neutral configuration for the private Bifrost peer listener and transport.
///
/// One `wyrd-server` process owns exactly one peer plane. The same certificate,
/// trust root, workload credential, and ticket keyring serve both directions:
/// the private listener presents them to accept inbound peer traffic, and the
/// outbound transport presents them when dialing another replica. Nothing here
/// is Oracle- or Scribe-specific.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostPeerConfig {
    /// Socket address the private peer listener binds.
    #[serde(default = "default_peer_bind")]
    pub bind: SocketAddr,
    /// Exact peer URI this replica publishes into role membership.
    #[serde(default)]
    pub advertise_addr: Option<String>,
    /// Dedicated Bifrost peer certificate authority trust root.
    #[serde(default)]
    pub ca_certificate_path: Option<PathBuf>,
    /// Dual-EKU leaf chain presented as both server and client identity.
    #[serde(default)]
    pub certificate_chain_path: Option<PathBuf>,
    /// Private key paired with `certificate_chain_path`.
    #[serde(default)]
    pub private_key_path: Option<PathBuf>,
    /// DNS SAN every peer certificate must carry and every dial verifies.
    #[serde(default)]
    pub server_name: Option<String>,
    /// Workload API key authenticating this process as the peer Service principal.
    #[serde(default, skip_serializing)]
    pub api_key: Option<String>,
    /// Independent peer-ticket signing and verification material.
    #[serde(default)]
    pub ticket: PeerTicketKeyringConfig,
    /// Maximum concurrent canonical denial-audit records for refused peer traffic.
    #[serde(default = "default_peer_denial_audit_concurrency")]
    pub denial_audit_concurrency: usize,
}

impl Default for BifrostPeerConfig {
    /// Produces the unconfigured peer plane used by non-peer targets and tests.
    fn default() -> Self {
        Self {
            bind: default_peer_bind(),
            advertise_addr: None,
            ca_certificate_path: None,
            certificate_chain_path: None,
            private_key_path: None,
            server_name: None,
            api_key: None,
            ticket: PeerTicketKeyringConfig::default(),
            denial_audit_concurrency: default_peer_denial_audit_concurrency(),
        }
    }
}

impl BifrostPeerConfig {
    /// Reports whether every mandatory peer input is present.
    ///
    /// A peer-bearing target requires all of them; a partially configured peer
    /// plane is a boot failure rather than a silently degraded listener.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.ca_certificate_path.is_some()
            && self.certificate_chain_path.is_some()
            && self.private_key_path.is_some()
            && self
                .server_name
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            && self
                .advertise_addr
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            && self
                .api_key
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            && self.ticket.is_complete()
    }

    /// Reports whether no peer input at all is present.
    ///
    /// Used to distinguish "this deployment has not configured the peer plane"
    /// from "this deployment configured it incompletely"; only the latter is
    /// reported as a partial-configuration error.
    #[must_use]
    fn is_absent(&self) -> bool {
        self.ca_certificate_path.is_none()
            && self.certificate_chain_path.is_none()
            && self.private_key_path.is_none()
            && self.server_name.is_none()
            && self.advertise_addr.is_none()
            && self.api_key.is_none()
            && self.ticket.is_absent()
    }
}

/// Canonical deployed private peer port.
///
/// Public gRPC keeps `50051`; the private peer plane is a separate socket on
/// `50052` so a Service or NetworkPolicy can name exactly one of them.
fn default_peer_bind() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 50052))
}

/// Default bound on concurrent canonical denial-audit work for refused peers.
///
/// Invalid peer traffic must not amplify into unbounded audit tasks, so the
/// refusal path is capped well below normal request concurrency.
fn default_peer_denial_audit_concurrency() -> usize {
    16
}

/// Optional storage I/O bounds for this node's one Bifrost storage owner.
///
/// Every field is optional and resolved against the node's managed memory and
/// selected roles at boot, so an unset deployment gets validated defaults and a
/// stated one fails boot rather than clamping silently. Connect-time and
/// HTTP-pool settings are deliberately absent: the already-built
/// `StorageHandle` owns the client, and a second place to configure it would be
/// a second answer to the same question.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostStorageIoConfig {
    /// Per-attempt backend request timeout in milliseconds.
    #[serde(default)]
    pub request_timeout_ms: Option<u64>,
    /// Retries allowed after the first attempt of an idempotent read.
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// Total wall-clock ceiling across one read's attempts, in milliseconds.
    #[serde(default)]
    pub max_retry_elapsed_ms: Option<u64>,
    /// Node-wide ceiling on concurrent backend requests.
    #[serde(default)]
    pub max_concurrent_requests: Option<usize>,
    /// Decoded Parquet metadata cache budget in bytes; zero disables it.
    #[serde(default)]
    pub metadata_cache_bytes: Option<u64>,
}

impl BifrostStorageIoConfig {
    /// Projects this configuration onto the Bifrost storage owner's own shape.
    ///
    /// The server config is the operator-facing surface; the validated policy
    /// lives with the owner that enforces it, and this is the single conversion
    /// between them.
    #[must_use]
    pub const fn to_storage_config(self) -> vala_bifrost_redux::storage::BifrostStorageConfig {
        vala_bifrost_redux::storage::BifrostStorageConfig {
            request_timeout_ms: self.request_timeout_ms,
            max_retries: self.max_retries,
            max_retry_elapsed_ms: self.max_retry_elapsed_ms,
            max_concurrent_requests: self.max_concurrent_requests,
            metadata_cache_bytes: self.metadata_cache_bytes,
        }
    }
}

/// Optional absolute caps for portable Bifrost resource discovery.
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BifrostResourceConfig {
    /// Optional process memory cap; detection may select a tighter bound.
    #[serde(default)]
    pub memory_limit_bytes: Option<usize>,
    /// Optional unmanaged process reserve, never below 256 MiB.
    #[serde(default)]
    pub unmanaged_reserve_bytes: Option<usize>,
    /// Optional disposable scratch cap; filesystem availability may be tighter.
    #[serde(default)]
    pub scratch_limit_bytes: Option<u64>,
    /// Optional effective CPU cap; process/cgroup affinity may be tighter.
    #[serde(default)]
    pub effective_cpu: Option<usize>,
    /// Optional Oracle query slot-unit concurrency limit for this node.
    ///
    /// Unlike the caps above this is a capacity decision rather than a detected
    /// bound, so it may raise as well as lower the default. Leaving it unset
    /// derives twice effective CPU, never below the portable slot-unit floor.
    #[serde(default)]
    pub oracle_query_slot_limit: Option<usize>,
}

/// Derives the dedicated Scribe coordination-runtime worker count.
///
/// The coordination runtime hosts one long-lived task per Scribe shard lane
/// (`SCRIBE_SHARD_COUNT` of them) plus the reconciliation and persistence
/// loops. Those shard owners are not pure channel-awaiters: each performs
/// synchronous Arrow memtable insertion inline and awaits a Postgres `COMMIT`,
/// so a thread count well below the lane count serializes independent lanes.
///
/// The derivation therefore starts from detected parallelism — matching the
/// sibling ingress and persistence derivations, including their `map_or(4, ..)`
/// fallback for platforms that cannot report it — then clamps it between two
/// bounds. The upper bound caps threads at the number of lanes there are to
/// run, so a large host does not spawn coordination threads that can never own
/// a lane. The lower bound preserves the historical floor so a single-core box
/// still gets a second thread to make progress on while one lane blocks in
/// `COMMIT`. The bounds are constant and ordered, so the clamp cannot panic.
fn default_scribe_coordination_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(4, std::num::NonZeroUsize::get)
        .clamp(2, vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT)
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

/// Returns the default for [`ScribeRuntimeConfig::ingest_request_bytes`].
fn default_ingest_request_bytes() -> usize {
    vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES
}

/// Returns the default for [`ScribeRuntimeConfig::wal_segment_bytes`].
fn default_scribe_wal_segment_bytes() -> u64 {
    vala_bifrost_redux::scribe::geometry::DEFAULT_WAL_SEGMENT_BYTES
}

/// Returns the default for [`ScribeRuntimeConfig::active_generation_budget_bytes`].
fn default_scribe_active_generation_budget_bytes() -> u64 {
    vala_bifrost_redux::scribe::geometry::DEFAULT_ACTIVE_GENERATION_BUDGET_BYTES
}

/// Returns the default for [`ScribeRuntimeConfig::generation_rotation_ceiling_bytes`].
fn default_scribe_generation_rotation_ceiling_bytes() -> u64 {
    vala_bifrost_redux::scribe::geometry::DEFAULT_GENERATION_ROTATION_CEILING_BYTES
}

/// Returns the default for [`ScribeRuntimeConfig::generation_max_age_secs`].
fn default_scribe_generation_max_age_secs() -> u64 {
    vala_bifrost_redux::scribe::geometry::DEFAULT_GENERATION_MAX_AGE.as_secs()
}

/// Returns the default for [`ScribeRuntimeConfig::staging_target_file_size_bytes`].
fn default_scribe_staging_target_file_size_bytes() -> u64 {
    vala_bifrost_redux::scribe::geometry::DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES
}

/// Returns the immutable V1 native field hard maximum.
fn default_ingest_native_fields() -> usize {
    vala_bifrost_redux::gate::limits::BIFROST_NATIVE_FIELD_LIMIT
}

/// Returns the immutable V1 native source hard maximum.
fn default_ingest_native_sources() -> usize {
    vala_bifrost_redux::gate::limits::BIFROST_NATIVE_SOURCE_LIMIT
}

/// Returns the immutable V1 logical row hard maximum.
fn default_ingest_rows() -> usize {
    vala_bifrost_redux::gate::limits::BIFROST_INGEST_ROW_LIMIT
}

/// Returns the immutable V1 OTLP resource hard maximum.
fn default_ingest_otlp_resources() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.resources
}

/// Returns the immutable V1 OTLP scope hard maximum.
fn default_ingest_otlp_scopes() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.scopes
}

/// Returns the immutable V1 OTLP record hard maximum.
fn default_ingest_otlp_records() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.records
}

/// Returns the immutable V1 OTLP attribute hard maximum.
fn default_ingest_otlp_attributes() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.attributes
}

/// Returns the immutable V1 OTLP cumulative-value-byte hard maximum.
fn default_ingest_otlp_value_bytes() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.value_bytes
}

/// Returns the immutable V1 OTLP recursive-value-depth hard maximum.
fn default_ingest_otlp_value_depth() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.value_depth
}

/// Returns the immutable V1 event-day hard maximum.
fn default_ingest_time_partitions() -> usize {
    vala_bifrost_redux::gate::limits::OTLP_WIRE_LIMITS.time_partitions
}

/// Returns the immutable V1 WAL-workspace hard maximum.
fn default_ingest_wal_workspace_bytes() -> usize {
    vala_bifrost_redux::gate::limits::BIFROST_WAL_WORKSPACE_LIMIT_BYTES
}

impl Default for ScribeRuntimeConfig {
    fn default() -> Self {
        Self {
            coordination_threads: default_scribe_coordination_threads(),
            ingress_cpu_threads: default_scribe_ingress_cpu_threads(),
            persistence_cpu_threads: default_scribe_persistence_cpu_threads(),
            wal_io_threads: default_scribe_wal_io_threads(),
            wal_disk_limit_bytes: None,
            event_time_past_window_secs: None,
            event_time_future_window_secs: None,
            ingest_request_bytes: default_ingest_request_bytes(),
            wal_segment_bytes: default_scribe_wal_segment_bytes(),
            active_generation_budget_bytes: default_scribe_active_generation_budget_bytes(),
            generation_rotation_ceiling_bytes: default_scribe_generation_rotation_ceiling_bytes(),
            generation_max_age_secs: default_scribe_generation_max_age_secs(),
            seal_key_early_seal_bytes: None,
            seal_key_max_age_secs: None,
            staging_target_file_size_bytes: default_scribe_staging_target_file_size_bytes(),
            ingest_native_fields: default_ingest_native_fields(),
            ingest_native_sources: default_ingest_native_sources(),
            ingest_rows: default_ingest_rows(),
            ingest_otlp_resources: default_ingest_otlp_resources(),
            ingest_otlp_scopes: default_ingest_otlp_scopes(),
            ingest_otlp_records: default_ingest_otlp_records(),
            ingest_otlp_attributes: default_ingest_otlp_attributes(),
            ingest_otlp_value_bytes: default_ingest_otlp_value_bytes(),
            ingest_otlp_value_depth: default_ingest_otlp_value_depth(),
            ingest_time_partitions: default_ingest_time_partitions(),
            ingest_wal_workspace_bytes: default_ingest_wal_workspace_bytes(),
        }
    }
}

impl ScribeRuntimeConfig {
    /// Validate that every configured bound can provide bounded operation.
    ///
    /// # Errors
    ///
    /// Returns a field-specific boot error when a thread or ingest bound is
    /// zero, a frozen cardinality bound exceeds its immutable V1 maximum, the
    /// configured request cannot be represented by tonic/WAL v4 framing, or a
    /// configured WAL disk budget is zero.
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
        if let Some(value) = self.wal_disk_limit_bytes
            && value == 0
        {
            return Err("scribe.wal_disk_limit_bytes must be at least 1".to_owned());
        }
        if self.generation_max_age_secs == 0 {
            return Err("scribe.generation_max_age_secs must be at least 1".to_owned());
        }
        if self.seal_key_max_age_secs == Some(0) {
            return Err("scribe.seal_key_max_age_secs must be at least 1".to_owned());
        }
        if self.ingest_request_bytes == 0 {
            return Err("scribe.ingest_request_bytes must be at least 1".to_owned());
        }
        if self.ingest_request_bytes.checked_add(64 * 1024).is_none() {
            return Err(
                "scribe.ingest_request_bytes plus tonic framing allowance exceeds usize".to_owned(),
            );
        }
        if u32::try_from(self.ingest_request_bytes).is_err() {
            return Err(
                "scribe.ingest_request_bytes exceeds WAL v4 payload representability".to_owned(),
            );
        }
        self.scribe_geometry()
            .map_err(|error| format!("scribe geometry configuration is invalid: {error}"))?;
        let ingest_values = [
            (
                "ingest_native_fields",
                self.ingest_native_fields,
                default_ingest_native_fields(),
            ),
            (
                "ingest_native_sources",
                self.ingest_native_sources,
                default_ingest_native_sources(),
            ),
            ("ingest_rows", self.ingest_rows, default_ingest_rows()),
            (
                "ingest_otlp_resources",
                self.ingest_otlp_resources,
                default_ingest_otlp_resources(),
            ),
            (
                "ingest_otlp_scopes",
                self.ingest_otlp_scopes,
                default_ingest_otlp_scopes(),
            ),
            (
                "ingest_otlp_records",
                self.ingest_otlp_records,
                default_ingest_otlp_records(),
            ),
            (
                "ingest_otlp_attributes",
                self.ingest_otlp_attributes,
                default_ingest_otlp_attributes(),
            ),
            (
                "ingest_otlp_value_bytes",
                self.ingest_otlp_value_bytes,
                default_ingest_otlp_value_bytes(),
            ),
            (
                "ingest_otlp_value_depth",
                self.ingest_otlp_value_depth,
                default_ingest_otlp_value_depth(),
            ),
            (
                "ingest_time_partitions",
                self.ingest_time_partitions,
                default_ingest_time_partitions(),
            ),
            (
                "ingest_wal_workspace_bytes",
                self.ingest_wal_workspace_bytes,
                default_ingest_wal_workspace_bytes(),
            ),
        ];
        if let Some((name, _, _)) = ingest_values.iter().find(|(_, value, _)| *value == 0) {
            return Err(format!("scribe.{name} must be at least 1"));
        }
        if let Some((name, value, maximum)) = ingest_values
            .iter()
            .find(|(_, value, maximum)| value > maximum)
        {
            return Err(format!(
                "scribe.{name} must not exceed the V1 hard maximum {maximum} (got {value})"
            ));
        }
        Ok(())
    }

    /// Derives the validated independent Scribe geometry from this configuration.
    ///
    /// This is the single conversion from operator-facing seconds and byte
    /// fields into the checked [`ScribeGeometry`] the Scribe runtime owns, so
    /// no caller can assemble an unvalidated geometry of its own.
    ///
    /// # Errors
    ///
    /// Returns the [`ScribeGeometryError`] naming the geometry field that is
    /// zero, that divides to no per-shard rotation limit at all, or that would
    /// make a per-`SealKey` control fire after its shard has already rotated.
    pub fn scribe_geometry(&self) -> Result<ScribeGeometry, ScribeGeometryError> {
        ScribeGeometry::new(
            self.wal_segment_bytes,
            self.active_generation_budget_bytes,
            self.generation_rotation_ceiling_bytes,
            Duration::from_secs(self.generation_max_age_secs),
            self.seal_key_early_seal_bytes,
            self.seal_key_max_age_secs.map(Duration::from_secs),
            self.staging_target_file_size_bytes,
            self.ingest_request_bytes,
            vala_bifrost_redux::scribe::geometry::DEFAULT_MAXIMUM_ACTIVE_REQUEST_OWNERSHIP_BYTES,
            vala_bifrost_redux::scribe::geometry::DEFAULT_MAXIMUM_IMMUTABLE_MEMBER_OWNERSHIP_BYTES,
            vala_bifrost_redux::scribe::geometry::DEFAULT_MINIMUM_STAGE_MEMBER_BYTES,
            vala_bifrost_redux::scribe::geometry::DEFAULT_MINIMUM_MERGE_LANE_SCRATCH_BYTES,
        )
    }

    /// Freezes the validated operator-selected limits passed to Gate and Scribe.
    ///
    /// # Panics
    ///
    /// Panics only when called before [`Self::validate`] has established that
    /// the tonic framing allowance can be added without overflow.
    #[must_use]
    pub fn ingest_limits(&self) -> vala_bifrost_redux::gate::limits::IngestLimits {
        vala_bifrost_redux::gate::limits::IngestLimits {
            max_frame_bytes: self.ingest_request_bytes,
            max_decoding_message_size: self
                .ingest_request_bytes
                .checked_add(64 * 1024)
                .expect("validated request bound plus tonic framing allowance must fit"),
            otlp: vala_bifrost_redux::gate::limits::OtlpWireLimits {
                request_bytes: self.ingest_request_bytes,
                resources: self.ingest_otlp_resources,
                scopes: self.ingest_otlp_scopes,
                records: self.ingest_otlp_records,
                attributes: self.ingest_otlp_attributes,
                value_bytes: self.ingest_otlp_value_bytes,
                value_depth: self.ingest_otlp_value_depth,
                time_partitions: self.ingest_time_partitions,
            },
            native_fields: self.ingest_native_fields,
            native_sources: self.ingest_native_sources,
            rows: self.ingest_rows,
            wal_workspace_bytes: self.ingest_wal_workspace_bytes,
        }
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
    /// Closed process target derived from `WYRD_TARGET`.
    #[serde(default)]
    pub role: BifrostTarget,
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
    /// Forge worker capacity for `all` and `forge-worker` roles.
    #[serde(default)]
    pub forge: ForgeRuntimeConfig,
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

impl WyrdServerConfig {
    /// Derive internal Bifrost component ownership from the closed public role.
    ///
    /// `All` owns every Bifrost role; `Server` owns Scribe, Forge coordination,
    /// and Oracle; `Oracle` and `Scribe` select only their named role; and
    /// `ForgeWorker` owns only Forge execution. No independent environment or
    /// config field may alter this topology.
    #[must_use]
    pub fn bifrost_roles(&self) -> BifrostRoles {
        BifrostRoles::for_target(self.role)
    }
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
    /// PEM certificate chain served by the gRPC listener.
    #[serde(default)]
    pub certificate_chain_path: Option<PathBuf>,
    /// PEM private key paired with `certificate_chain_path`.
    #[serde(default)]
    pub private_key_path: Option<PathBuf>,
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
            certificate_chain_path: None,
            private_key_path: None,
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
        if env_opt("WYRD_BIFROST_ROLES")?.is_some() {
            return Err(ConfigError::BadEnvVar {
                key: "WYRD_BIFROST_ROLES".to_owned(),
                message: "independent Bifrost role selection was removed; use WYRD_ROLES"
                    .to_owned(),
            });
        }
        if env_opt("WYRD_ROLES")?.is_some() {
            return Err(ConfigError::BadEnvVar {
                key: "WYRD_ROLES".to_owned(),
                message: "use the single closed WYRD_TARGET process target".to_owned(),
            });
        }
        if let Some(value) = env_opt("WYRD_TARGET")? {
            self.role = match value.as_str() {
                "all" => BifrostTarget::All,
                "server" => BifrostTarget::Server,
                "oracle" => BifrostTarget::Oracle,
                "scribe" => BifrostTarget::Scribe,
                "forge-worker" => BifrostTarget::ForgeWorker,
                _ => {
                    return Err(ConfigError::BadEnvVar {
                        key: "WYRD_TARGET".to_owned(),
                        message: format!(
                            "expected 'all', 'server', 'oracle', 'scribe', or 'forge-worker', got {value:?}"
                        ),
                    });
                }
            };
        }
        self.bifrost.resources.memory_limit_bytes = parse_optional_env(
            "WYRD_BIFROST_MEMORY_LIMIT_BYTES",
            self.bifrost.resources.memory_limit_bytes,
        )?;
        self.bifrost.resources.unmanaged_reserve_bytes = parse_optional_env(
            "WYRD_BIFROST_UNMANAGED_RESERVE_BYTES",
            self.bifrost.resources.unmanaged_reserve_bytes,
        )?;
        self.bifrost.resources.scratch_limit_bytes = parse_optional_env(
            "WYRD_BIFROST_SCRATCH_LIMIT_BYTES",
            self.bifrost.resources.scratch_limit_bytes,
        )?;
        self.bifrost.resources.effective_cpu = parse_optional_env(
            "WYRD_BIFROST_EFFECTIVE_CPU",
            self.bifrost.resources.effective_cpu,
        )?;
        self.bifrost.resources.oracle_query_slot_limit = parse_optional_env(
            "WYRD_BIFROST_ORACLE_QUERY_SLOT_LIMIT",
            self.bifrost.resources.oracle_query_slot_limit,
        )?;
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
        if let Some(val) = env_opt("WYRD_GRPC_CERTIFICATE_CHAIN_FILE")? {
            self.grpc.certificate_chain_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_GRPC_PRIVATE_KEY_FILE")? {
            self.grpc.private_key_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_BIND_ADDR")? {
            self.bifrost.peer.bind =
                val.parse::<SocketAddr>()
                    .map_err(|error| ConfigError::Invalid {
                        message: format!(
                            "WYRD_BIFROST_PEER_BIND_ADDR must be a socket address: {error}"
                        ),
                    })?;
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_ADVERTISE_ADDR")? {
            self.bifrost.peer.advertise_addr = Some(val);
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_CA_CERTIFICATE_PATH")? {
            self.bifrost.peer.ca_certificate_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_CERTIFICATE_CHAIN_PATH")? {
            self.bifrost.peer.certificate_chain_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_PRIVATE_KEY_PATH")? {
            self.bifrost.peer.private_key_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_SERVER_NAME")? {
            self.bifrost.peer.server_name = Some(val);
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_API_KEY")? {
            self.bifrost.peer.api_key = Some(val);
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_TICKET_ACTIVE_KEY_ID")? {
            self.bifrost.peer.ticket.active_key_id = Some(val);
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_TICKET_SIGNING_KEY_PATH")? {
            self.bifrost.peer.ticket.signing_key_path = Some(PathBuf::from(val));
        }
        if let Some(val) = env_opt("WYRD_BIFROST_PEER_TICKET_VERIFYING_KEYRING_PATH")? {
            self.bifrost.peer.ticket.verifying_keyring_path = Some(PathBuf::from(val));
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
        let serves_api = self.role.serves_api();
        if self.forge.worker_concurrency == 0 {
            return Err(ConfigError::Invalid {
                message: "forge.worker_concurrency must be positive".to_owned(),
            });
        }
        if self.forge.per_tenant_active_cap == Some(0) {
            return Err(ConfigError::Invalid {
                message: "forge.per_tenant_active_cap must be positive".to_owned(),
            });
        }
        if serves_api {
            if self.bifrost.oracle.max_workers_per_query > 63 {
                return Err(ConfigError::Invalid {
                    message: "bifrost.oracle.max_workers_per_query must be at most 63".to_owned(),
                });
            }
            if self.bifrost.oracle.planning_permits == 0
                || self.bifrost.oracle.admission_waiters == 0
                || self.bifrost.oracle.max_queue_wait_ms == 0
                || self.bifrost.oracle.max_frame_bytes == 0
                || self.bifrost.oracle.audit_wal_max_records == 0
                || self.bifrost.oracle.audit_wal_max_bytes == 0
                || self.bifrost.oracle.audit_wal_max_age_seconds == 0
                || self.bifrost.oracle.audit_relay_batch_records == 0
                || self.bifrost.oracle.audit_relay_attempt_timeout_ms == 0
                || self.bifrost.oracle.audit_relay_backoff_initial_ms == 0
                || self.bifrost.oracle.audit_relay_backoff_max_ms == 0
                || self.bifrost.oracle.audit_relay_shutdown_timeout_ms == 0
                || self.bifrost.oracle.audit_relay_backoff_initial_ms
                    > self.bifrost.oracle.audit_relay_backoff_max_ms
                || !self.bifrost.oracle.cpu_cores.is_finite()
                || self.bifrost.oracle.cpu_cores <= 0.0
            {
                return Err(ConfigError::Invalid {
                    message: "bifrost.oracle bounds must be positive and finite".to_owned(),
                });
            }
            if self.deployment_profile.is_production()
                && self.bifrost.oracle.audit_wal_root.is_none()
            {
                return Err(ConfigError::Invalid {
                    message: "bifrost.oracle.audit_wal_root is required in production".to_owned(),
                });
            }
            self.bifrost
                .scribe
                .validate()
                .map_err(|message| ConfigError::Invalid { message })?;
            self.validate_oracle_calibration()?;

            if self.grpc.certificate_chain_path.is_some() != self.grpc.private_key_path.is_some() {
                return Err(ConfigError::Invalid {
                    message: "grpc certificate_chain_path and private_key_path must be configured together"
                        .to_owned(),
                });
            }
        }

        // The private peer plane is one all-or-nothing contract. A peer-bearing
        // target that configured it partially would otherwise boot a listener
        // that cannot verify, dial, or authorize, so a partial state fails here
        // rather than at first peer contact.
        if !self.bifrost.peer.is_absent() && !self.bifrost.peer.is_complete() {
            return Err(ConfigError::Invalid {
                message: "bifrost.peer requires ca_certificate_path, certificate_chain_path, \
                          private_key_path, server_name, advertise_addr, api_key, and a complete \
                          ticket keyring to be configured together"
                    .to_owned(),
            });
        }
        if self.role.serves_peer() && self.bifrost.peer.denial_audit_concurrency == 0 {
            return Err(ConfigError::Invalid {
                message: "bifrost.peer.denial_audit_concurrency must be positive".to_owned(),
            });
        }
        if self.role.serves_peer()
            && serves_api
            && (self.bifrost.peer.bind == self.http.bind || self.bifrost.peer.bind == self.grpc.bind)
        {
            return Err(ConfigError::BindCollision {
                bind: self.bifrost.peer.bind,
            });
        }

        // 1. HTTP and gRPC bind addresses must differ.
        if serves_api && self.http.bind == self.grpc.bind {
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
            if serves_api && (metrics_bind == self.http.bind || metrics_bind == self.grpc.bind) {
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

        if serves_api {
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

        if serves_api {
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
        }

        // 10. Production profile hardening.
        if serves_api && self.deployment_profile.is_production() {
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
            if self.role.serves_peer() {
                match (&self.grpc.certificate_chain_path, &self.grpc.private_key_path) {
                    (Some(certificate), Some(key))
                        if !certificate.as_os_str().is_empty() && !key.as_os_str().is_empty() => {}
                    _ => {
                        return Err(ConfigError::Invalid {
                            message: "production peer-bearing targets require grpc \
                                      certificate_chain_path and private_key_path"
                                .to_owned(),
                        });
                    }
                }
                if !self.bifrost.peer.is_complete() {
                    return Err(ConfigError::Invalid {
                        message: "production peer-bearing targets require the complete \
                                  bifrost.peer identity, credential, and ticket keyring"
                            .to_owned(),
                    });
                }
                if !self
                    .bifrost
                    .peer
                    .advertise_addr
                    .as_ref()
                    .is_some_and(|value| value.starts_with("https://"))
                {
                    return Err(ConfigError::Invalid {
                        message: "production bifrost.peer.advertise_addr must use https://"
                            .to_owned(),
                    });
                }
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
        if serves_api {
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
        if !self.bifrost_roles().contains(&BifrostRuntimeRole::Oracle) {
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

/// Parses one optional absolute resource override while preserving file config.
///
/// # Errors
///
/// Returns [`ConfigError`] when the environment value is empty, non-Unicode,
/// or cannot be parsed into the requested numeric type.
fn parse_optional_env<T>(key: &str, current: Option<T>) -> Result<Option<T>, ConfigError>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    env_opt(key)?
        .map(|value| {
            value
                .parse::<T>()
                .map(Some)
                .map_err(|error| ConfigError::BadEnvVar {
                    key: key.to_owned(),
                    message: error.to_string(),
                })
        })
        .unwrap_or(Ok(current))
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

    /// Parse a TOML test fixture and explicitly allow an unapproved development Oracle profile.
    ///
    /// Production validation is unchanged; callers testing Oracle activation policy must build
    /// an explicit production configuration and calibration profile instead.
    fn from_toml_str_with_dev_oracle_opt_in(s: &str) -> Result<WyrdServerConfig, ConfigError> {
        let mut config =
            toml::from_str::<WyrdServerConfig>(s).map_err(|source| ConfigError::ParseToml {
                path: PathBuf::from("<test-string>"),
                source,
            })?;
        config.bifrost.oracle.allow_unapproved_profile = true;
        Ok(config)
    }

    // ── 1. Default config validates ───────────────────────────────────────────

    #[test]
    fn default_config_validates() {
        let mut cfg = WyrdServerConfig::default();
        assert_eq!(cfg.role, BifrostTarget::All);
        assert_eq!(cfg.bifrost_roles().len(), 4);
        cfg.bifrost.oracle.allow_unapproved_profile = true;
        cfg.validate().expect("default config must be valid");
    }

    /// Proves the closed public role derives the internal Bifrost topology.
    #[test]
    fn public_roles_derive_internal_bifrost_roles() {
        let cases = [
            (BifrostTarget::All, 4, true),
            (BifrostTarget::Server, 3, true),
            (BifrostTarget::Oracle, 1, true),
            (BifrostTarget::Scribe, 1, true),
            (BifrostTarget::ForgeWorker, 1, false),
        ];
        for (target, count, serves_gate) in cases {
            let roles = BifrostRoles::for_target(target);
            assert_eq!(roles.len(), count, "{target:?}");
            assert_eq!(roles.serves_gate(), serves_gate, "{target:?}");
        }
    }

    /// Proves a dedicated Forge worker ignores malformed settings for services
    /// it does not own while retaining its local concurrency guard.
    #[test]
    fn forge_worker_validation_ignores_api_only_settings() {
        let mut config = WyrdServerConfig {
            deployment_profile: DeploymentProfile::Production,
            role: BifrostTarget::ForgeWorker,
            ..WyrdServerConfig::default()
        };
        config.bifrost.scribe.coordination_threads = 0;
        config.bifrost.oracle.planning_permits = 0;
        config.bifrost.oracle.calibration_profile = PathBuf::from("/\0malformed");
        config.grpc.certificate_chain_path = Some(PathBuf::from("certificate.pem"));
        config.http.bind = config.grpc.bind;
        config.grpc.reflection_enabled = true;
        config.auth.allow_preview = true;
        config.limits.body_bytes = 0;
        config.limits.timeout_ms = 0;
        config.limits.concurrency = 0;
        config.readiness.tick_ms = 0;
        config.readiness.probe_timeout_ms = 0;
        config.metrics.enabled = false;

        config
            .validate()
            .expect("ForgeWorker should validate only owned Forge and metrics settings");
    }

    /// Proves API-owning roles reject the same malformed service settings.
    #[test]
    fn api_roles_reject_forge_worker_only_validation_bypass() {
        for role in [BifrostTarget::All, BifrostTarget::Server] {
            let mut config = WyrdServerConfig {
                deployment_profile: DeploymentProfile::Production,
                role,
                ..WyrdServerConfig::default()
            };
            config.bifrost.scribe.coordination_threads = 0;
            config.grpc.certificate_chain_path = Some(PathBuf::from("certificate.pem"));
            config.grpc.reflection_enabled = true;
            config.auth.allow_preview = true;
            config.limits.body_bytes = 0;
            config.limits.timeout_ms = 0;
            config.limits.concurrency = 0;
            config.readiness.tick_ms = 0;
            config.readiness.probe_timeout_ms = 0;
            config.metrics.enabled = false;

            assert!(
                config.validate().is_err(),
                "{role:?} must reject malformed API-owned settings"
            );
        }
    }

    /// Every promoted `[forge]` operational field is optional; an omitted
    /// section leaves them all `None`, which the boot resolver maps to the
    /// compiled `ForgeConfig` defaults.
    #[test]
    fn forge_operational_fields_default_to_none_when_absent() {
        let config = from_toml_str_with_dev_oracle_opt_in("").expect("empty config parses");
        let forge = &config.forge;
        assert_eq!(forge.worker_concurrency, 1);
        assert_eq!(forge.per_tenant_active_cap, None);
        assert_eq!(forge.snapshot_retention_secs, None);
        assert_eq!(forge.retain_last, None);
        assert_eq!(forge.orphan_gc_ttl_secs, None);
        assert_eq!(forge.maintenance_trigger_snapshot_count, None);
        assert_eq!(forge.maintenance_trigger_interval_secs, None);
        assert_eq!(forge.orphan_gc_max_list_pages, None);
        assert_eq!(forge.orphan_gc_run_budget_secs, None);
        assert_eq!(forge.maintenance_interval_secs, None);
        assert_eq!(forge.max_files_per_tick, None);
        assert_eq!(forge.max_bytes_per_tick, None);
    }

    /// A `[forge]` section parses every promoted operational field onto
    /// `config.forge`.
    #[test]
    fn forge_operational_fields_parse_from_toml() {
        let toml = r#"
[forge]
worker_concurrency = 4
per_tenant_active_cap = 2
snapshot_retention_secs = 7200
retain_last = 3
orphan_gc_ttl_secs = 3600
maintenance_trigger_snapshot_count = 8
maintenance_trigger_interval_secs = 900
orphan_gc_max_list_pages = 64
orphan_gc_run_budget_secs = 30
maintenance_interval_secs = 45
max_files_per_tick = 512
max_bytes_per_tick = 268435456
"#;
        let config = from_toml_str_with_dev_oracle_opt_in(toml).expect("forge section parses");
        let forge = &config.forge;
        assert_eq!(forge.worker_concurrency, 4);
        assert_eq!(forge.per_tenant_active_cap, Some(2));
        assert_eq!(forge.snapshot_retention_secs, Some(7200));
        assert_eq!(forge.retain_last, Some(3));
        assert_eq!(forge.orphan_gc_ttl_secs, Some(3600));
        assert_eq!(forge.maintenance_trigger_snapshot_count, Some(8));
        assert_eq!(forge.maintenance_trigger_interval_secs, Some(900));
        assert_eq!(forge.orphan_gc_max_list_pages, Some(64));
        assert_eq!(forge.orphan_gc_run_budget_secs, Some(30));
        assert_eq!(forge.maintenance_interval_secs, Some(45));
        assert_eq!(forge.max_files_per_tick, Some(512));
        assert_eq!(forge.max_bytes_per_tick, Some(268_435_456));
    }

    /// An unknown key under `[forge]` is rejected at parse time by
    /// `deny_unknown_fields`.
    #[test]
    fn forge_rejects_unknown_field() {
        let toml = "[forge]\nnot_a_real_forge_field = 1\n";
        assert!(from_toml_str_with_dev_oracle_opt_in(toml).is_err());
    }

    /// `resolved_per_tenant_active_cap` falls back to `worker_concurrency` when
    /// unset and honors an explicit override otherwise.
    #[test]
    fn forge_resolved_per_tenant_active_cap_fallback_and_override() {
        let mut forge = ForgeRuntimeConfig {
            worker_concurrency: 6,
            ..ForgeRuntimeConfig::default()
        };
        assert_eq!(forge.resolved_per_tenant_active_cap(), 6);
        forge.per_tenant_active_cap = Some(2);
        assert_eq!(forge.resolved_per_tenant_active_cap(), 2);
    }

    /// A zero `forge.per_tenant_active_cap` fails boot validation fail-closed.
    #[test]
    fn forge_zero_per_tenant_active_cap_is_rejected() {
        let mut config = WyrdServerConfig::default();
        config.bifrost.oracle.allow_unapproved_profile = true;
        config.forge.per_tenant_active_cap = Some(0);
        assert!(config.validate().is_err());
    }

    /// Proves the removed independent role environment is rejected.
    #[test]
    fn bifrost_role_env_is_rejected() {
        let _guard = ENV_LOCK.lock().expect("environment test lock");
        temp_env::with_vars([("WYRD_BIFROST_ROLES", Some("oracle"))], || {
            assert!(WyrdServerConfig::default().apply_env_overrides().is_err());
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
        config.grpc.certificate_chain_path = Some(directory.path().join("server.pem"));
        config.grpc.private_key_path = Some(directory.path().join("server-key.pem"));
        config.bifrost.peer.ca_certificate_path = Some(directory.path().join("peer-ca.pem"));
        config.bifrost.peer.certificate_chain_path = Some(directory.path().join("peer.pem"));
        config.bifrost.peer.private_key_path = Some(directory.path().join("peer-key.pem"));
        config.bifrost.peer.server_name = Some("bifrost-peer.test".to_owned());
        config.bifrost.peer.advertise_addr = Some("https://oracle-0.peers.svc:50052".to_owned());
        config.bifrost.peer.api_key = Some("peer-api-key".to_owned());
        config.bifrost.peer.ticket.active_key_id = Some("peer-2026-09".to_owned());
        config.bifrost.peer.ticket.signing_key_path =
            Some(directory.path().join("peer-ticket-signing.pem"));
        config.bifrost.peer.ticket.verifying_keyring_path =
            Some(directory.path().join("peer-ticket-keyring.json"));
        config.bifrost.oracle.audit_wal_root = Some(directory.path().join("oracle-audit"));
        assert!(config.validate().is_err());

        std::fs::write(&path, complete_oracle_calibration("approved"))
            .expect("approved profile writes");
        config
            .validate()
            .expect("approved production calibration validates");
    }

    /// Proves the production peer plane is complete, mutual, and HTTPS-only.
    ///
    /// Public gRPC TLS and the private peer identity are separate requirements:
    /// a peer-bearing target needs both, and an incompletely configured peer
    /// plane fails boot rather than starting a listener that cannot verify.
    #[test]
    fn peer_production_requires_complete_tls() {
        let directory = tempfile::tempdir().expect("calibration temp directory");
        let calibration = directory.path().join("oracle-calibration.toml");
        std::fs::write(&calibration, complete_oracle_calibration("approved"))
            .expect("approved profile writes");
        let mut config = WyrdServerConfig {
            deployment_profile: DeploymentProfile::Production,
            ..WyrdServerConfig::default()
        };
        config.bifrost.oracle.calibration_profile = calibration;
        assert!(config.validate().is_err());

        config.grpc.certificate_chain_path = Some(directory.path().join("server.pem"));
        assert!(config.validate().is_err());
        config.grpc.private_key_path = Some(directory.path().join("server-key.pem"));
        assert!(config.validate().is_err());
        config.bifrost.peer.ca_certificate_path = Some(directory.path().join("peer-ca.pem"));
        assert!(config.validate().is_err());
        config.bifrost.peer.certificate_chain_path = Some(directory.path().join("peer.pem"));
        assert!(config.validate().is_err());
        config.bifrost.peer.private_key_path = Some(directory.path().join("peer-key.pem"));
        assert!(config.validate().is_err());
        config.bifrost.peer.server_name = Some("bifrost-peer.test".to_owned());
        assert!(config.validate().is_err());
        config.bifrost.peer.api_key = Some("peer-api-key".to_owned());
        assert!(config.validate().is_err());
        config.bifrost.peer.ticket.active_key_id = Some("peer-2026-09".to_owned());
        config.bifrost.peer.ticket.signing_key_path =
            Some(directory.path().join("peer-ticket-signing.pem"));
        config.bifrost.peer.ticket.verifying_keyring_path =
            Some(directory.path().join("peer-ticket-keyring.json"));
        assert!(config.validate().is_err());
        config.bifrost.peer.advertise_addr = Some("http://oracle-0.peers.svc:50052".to_owned());
        assert!(
            config.validate().is_err(),
            "a plaintext advertisement is unroutable for a mutually authenticated peer plane"
        );
        config.bifrost.peer.advertise_addr = Some("https://oracle-0.peers.svc:50052".to_owned());
        config.bifrost.oracle.audit_wal_root = Some(directory.path().join("oracle-audit"));
        config
            .validate()
            .expect("complete production peer configuration validates");
    }

    /// Proves the canonical peer environment names land on the validated fields.
    ///
    /// The advertisement is published into `vala.cluster_nodes` and later dialed
    /// through `Endpoint::from_shared`, which rejects a schemeless authority, so
    /// routing every peer input through `apply_env_overrides` keeps one
    /// validated source of truth instead of unvalidated reads at boot.
    #[test]
    fn peer_environment_names_land_on_validated_fields() {
        assert_eq!(
            WyrdServerConfig::default().bifrost.peer.bind,
            std::net::SocketAddr::from(([0, 0, 0, 0], 50052)),
            "the canonical deployed peer port is 50052"
        );

        let _guard = ENV_LOCK.lock().expect("environment test lock");
        temp_env::with_vars(
            [
                ("WYRD_BIFROST_PEER_BIND_ADDR", Some("127.0.0.1:50152")),
                (
                    "WYRD_BIFROST_PEER_ADVERTISE_ADDR",
                    Some("https://oracle-0.peers.svc:50052"),
                ),
                ("WYRD_BIFROST_PEER_CA_CERTIFICATE_PATH", Some("/peer/ca.pem")),
                (
                    "WYRD_BIFROST_PEER_CERTIFICATE_CHAIN_PATH",
                    Some("/peer/cert.pem"),
                ),
                ("WYRD_BIFROST_PEER_PRIVATE_KEY_PATH", Some("/peer/key.pem")),
                ("WYRD_BIFROST_PEER_SERVER_NAME", Some("bifrost-peer.test")),
                ("WYRD_BIFROST_PEER_API_KEY", Some("peer-api-key")),
                (
                    "WYRD_BIFROST_PEER_TICKET_ACTIVE_KEY_ID",
                    Some("peer-2026-09"),
                ),
                (
                    "WYRD_BIFROST_PEER_TICKET_SIGNING_KEY_PATH",
                    Some("/peer/ticket-signing.pem"),
                ),
                (
                    "WYRD_BIFROST_PEER_TICKET_VERIFYING_KEYRING_PATH",
                    Some("/peer/ticket-keyring.json"),
                ),
            ],
            || {
                let mut config = WyrdServerConfig::default();
                config
                    .apply_env_overrides()
                    .expect("peer overrides apply");
                let peer = &config.bifrost.peer;
                assert_eq!(
                    peer.bind,
                    "127.0.0.1:50152"
                        .parse::<std::net::SocketAddr>()
                        .expect("literal bind address parses")
                );
                assert_eq!(
                    peer.advertise_addr.as_deref(),
                    Some("https://oracle-0.peers.svc:50052")
                );
                assert_eq!(peer.server_name.as_deref(), Some("bifrost-peer.test"));
                assert!(peer.is_complete());
            },
        );
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
    fn scribe_runtime_defaults_match_configured_ingest_contract() {
        let cfg = ScribeRuntimeConfig::default();
        // Asserted as bounds rather than by restating the derivation: an
        // assertion that recomputes the implementation expression can never
        // fail, while these bounds are exactly the properties a wrong formula
        // violates — never below the two-thread floor, never above the number
        // of shard lanes the runtime has to host.
        assert!(
            (2..=vala_bifrost_redux::scribe::routing::SCRIBE_SHARD_COUNT)
                .contains(&cfg.coordination_threads),
            "coordination threads {} must stay within the shard-lane bounds",
            cfg.coordination_threads
        );
        assert_eq!(cfg.wal_disk_limit_bytes, None);
        assert_eq!(
            cfg.ingest_request_bytes,
            vala_bifrost_redux::gate::limits::BIFROST_INGEST_REQUEST_LIMIT_BYTES
        );
        assert_eq!(cfg.wal_segment_bytes, 512 * 1024 * 1024);
        assert_eq!(cfg.active_generation_budget_bytes, 8 * 1024 * 1024 * 1024);
        assert_eq!(cfg.generation_rotation_ceiling_bytes, 512 * 1024 * 1024);
        assert_eq!(cfg.generation_max_age_secs, 600);
        assert_eq!(cfg.seal_key_early_seal_bytes, None);
        assert_eq!(cfg.seal_key_max_age_secs, None);
        assert_eq!(cfg.staging_target_file_size_bytes, 512 * 1024 * 1024);
        assert_eq!(
            cfg.ingest_limits(),
            vala_bifrost_redux::gate::limits::IngestLimits::default()
        );
        cfg.validate().expect("resolved defaults must validate");
    }

    /// One pod-wide budget derives every shard's rotation limit at boot.
    ///
    /// # Panics
    ///
    /// Panics when the derived per-shard limit is not the minimum of the
    /// ceiling and the evenly divided budget, when a geometry that cannot serve
    /// is accepted, or when a per-`SealKey` control is allowed to fire after
    /// its shard would already have rotated.
    #[test]
    fn scribe_geometry_is_derived_from_independent_configured_fields() {
        let mut config = ScribeRuntimeConfig::default();
        let geometry = config
            .scribe_geometry()
            .expect("the defaults form a coherent geometry");
        assert_eq!(
            geometry.shard_generation_rotation_bytes(),
            512 * 1024 * 1024
        );
        assert_eq!(geometry.wal_segment_bytes(), 512 * 1024 * 1024);
        assert_eq!(geometry.staging_target_file_size_bytes(), 512 * 1024 * 1024);

        // Lowering only the pod-wide budget narrows every shard together and
        // leaves the WAL segment and hot-object targets exactly where they were.
        config.active_generation_budget_bytes = 1024 * 1024 * 1024;
        let narrowed = config
            .scribe_geometry()
            .expect("a smaller budget is still coherent");
        assert_eq!(narrowed.shard_generation_rotation_bytes(), 64 * 1024 * 1024);
        assert_eq!(narrowed.wal_segment_bytes(), 512 * 1024 * 1024);
        assert_eq!(narrowed.staging_target_file_size_bytes(), 512 * 1024 * 1024);

        // A per-key control may only seal earlier than the shard it belongs to.
        config.seal_key_early_seal_bytes = Some(65 * 1024 * 1024);
        let error = config
            .validate()
            .expect_err("an early seal above the derived shard limit must be refused");
        assert!(error.contains("seal_key_early_seal_bytes"), "{error}");
    }

    /// Rejects every independently configurable Scribe bound before boot.
    ///
    /// # Panics
    ///
    /// Panics when validation accepts an invalid value or fails to identify its
    /// owning configuration field.
    #[test]
    fn scribe_runtime_rejects_every_invalid_configured_bound() {
        macro_rules! assert_rejected {
            ($field:ident, $value:expr) => {{
                let mut config = ScribeRuntimeConfig::default();
                config.$field = $value;
                let error = config
                    .validate()
                    .expect_err(concat!(stringify!($field), " must be rejected"));
                assert!(error.contains(stringify!($field)), "{error}");
            }};
        }
        assert_rejected!(coordination_threads, 0);
        assert_rejected!(ingress_cpu_threads, 0);
        assert_rejected!(persistence_cpu_threads, 0);
        assert_rejected!(wal_io_threads, 0);
        assert_rejected!(wal_disk_limit_bytes, Some(0));
        assert_rejected!(wal_segment_bytes, 0);
        assert_rejected!(active_generation_budget_bytes, 0);
        assert_rejected!(generation_rotation_ceiling_bytes, 0);
        assert_rejected!(generation_max_age_secs, 0);
        assert_rejected!(staging_target_file_size_bytes, 0);
        assert_rejected!(ingest_request_bytes, 0);
        assert_rejected!(ingest_native_fields, 0);
        assert_rejected!(ingest_native_sources, 0);
        assert_rejected!(ingest_rows, 0);
        assert_rejected!(ingest_otlp_resources, 0);
        assert_rejected!(ingest_otlp_scopes, 0);
        assert_rejected!(ingest_otlp_records, 0);
        assert_rejected!(ingest_otlp_attributes, 0);
        assert_rejected!(ingest_otlp_value_bytes, 0);
        assert_rejected!(ingest_otlp_value_depth, 0);
        assert_rejected!(ingest_time_partitions, 0);
        assert_rejected!(ingest_wal_workspace_bytes, 0);
        assert_rejected!(ingest_native_fields, default_ingest_native_fields() + 1);
        assert_rejected!(ingest_native_sources, default_ingest_native_sources() + 1);
        assert_rejected!(ingest_rows, default_ingest_rows() + 1);
        assert_rejected!(ingest_otlp_resources, default_ingest_otlp_resources() + 1);
        assert_rejected!(ingest_otlp_scopes, default_ingest_otlp_scopes() + 1);
        assert_rejected!(ingest_otlp_records, default_ingest_otlp_records() + 1);
        assert_rejected!(ingest_otlp_attributes, default_ingest_otlp_attributes() + 1);
        assert_rejected!(
            ingest_otlp_value_bytes,
            default_ingest_otlp_value_bytes() + 1
        );
        assert_rejected!(
            ingest_otlp_value_depth,
            default_ingest_otlp_value_depth() + 1
        );
        assert_rejected!(ingest_time_partitions, default_ingest_time_partitions() + 1);
        assert_rejected!(
            ingest_wal_workspace_bytes,
            default_ingest_wal_workspace_bytes() + 1
        );
        assert_rejected!(ingest_request_bytes, usize::MAX);
        #[cfg(target_pointer_width = "64")]
        assert_rejected!(ingest_request_bytes, u32::MAX as usize + 1);
    }

    /// Proves the 200 MiB request value is a default rather than a hard cap.
    #[test]
    fn scribe_runtime_propagates_supported_request_above_default() {
        let request_bytes = default_ingest_request_bytes() + 1024 * 1024;
        let config = ScribeRuntimeConfig {
            ingest_request_bytes: request_bytes,
            ..ScribeRuntimeConfig::default()
        };
        config
            .validate()
            .expect("supported request above the default must validate");

        let limits = config.ingest_limits();
        assert_eq!(limits.max_frame_bytes, request_bytes);
        assert_eq!(limits.max_decoding_message_size, request_bytes + 64 * 1024);
        assert_eq!(limits.otlp.request_bytes, request_bytes);
    }

    /// Proves one lower operator limit is frozen into the shared Gate/Scribe snapshot.
    /// Proves configured lower ingest bounds remain identical across Gate and Scribe.
    #[test]
    fn scribe_runtime_freezes_lower_ingest_limits() {
        let config = ScribeRuntimeConfig {
            ingest_request_bytes: 1024,
            ingest_native_fields: 4,
            ingest_native_sources: 2,
            ingest_rows: 8,
            ingest_otlp_resources: 2,
            ingest_otlp_scopes: 3,
            ingest_otlp_records: 8,
            ingest_otlp_attributes: 16,
            ingest_otlp_value_bytes: 512,
            ingest_otlp_value_depth: 3,
            ingest_time_partitions: 2,
            ingest_wal_workspace_bytes: 256,
            ..ScribeRuntimeConfig::default()
        };
        config.validate().expect("lower V1 limits validate");

        let frozen = config.ingest_limits();
        assert_eq!(frozen.max_frame_bytes, 1024);
        assert_eq!(frozen.native_fields, 4);
        assert_eq!(frozen.native_sources, 2);
        assert_eq!(frozen.rows, 8);
        assert_eq!(frozen.otlp.resources, 2);
        assert_eq!(frozen.otlp.scopes, 3);
        assert_eq!(frozen.otlp.records, 8);
        assert_eq!(frozen.otlp.attributes, 16);
        assert_eq!(frozen.otlp.value_bytes, 512);
        assert_eq!(frozen.otlp.value_depth, 3);
        assert_eq!(frozen.otlp.time_partitions, 2);
        assert_eq!(frozen.wal_workspace_bytes, 256);
    }

    // ── 2. TOML with unknown legacy field fails with ParseToml ────────────────

    #[test]
    fn unknown_toml_field_fails_parse_toml() {
        let toml = r#"
            port = 9090
        "#;
        let err = from_toml_str_with_dev_oracle_opt_in(toml).expect_err("unknown field must fail");
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
                cfg.bifrost.oracle.allow_unapproved_profile = true;
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let err = from_toml_str_with_dev_oracle_opt_in(toml).expect_err("unknown field must fail");
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
        let err = from_toml_str_with_dev_oracle_opt_in(toml).expect_err("unknown field must fail");
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
            let cfg = from_toml_str_with_dev_oracle_opt_in(&toml)
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
            let cfg = from_toml_str_with_dev_oracle_opt_in(&toml)
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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
        let cfg = from_toml_str_with_dev_oracle_opt_in(toml).expect("parses ok");
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

    /// Calibration translation applies headroom, class shares, and parent caps.
    #[test]
    fn oracle_admission_config_translates_calibration() {
        let leaf = |value: i64| {
            let mut table = toml::Table::new();
            table.insert("value".to_owned(), toml::Value::Integer(value));
            table.insert(
                "evidence_case_id".to_owned(),
                toml::Value::String("test".to_owned()),
            );
            toml::Value::Table(table)
        };
        let mut proposal = toml::Table::new();
        let mut tenant = toml::Table::new();
        tenant.insert("single_tenant_limit".to_owned(), leaf(8));
        tenant.insert("multi_tenant_default_limit".to_owned(), leaf(2));
        proposal.insert("tenant".to_owned(), toml::Value::Table(tenant));
        let mut memory = toml::Table::new();
        memory.insert("class_limits".to_owned(), leaf(1024));
        proposal.insert("memory".to_owned(), toml::Value::Table(memory));
        let mut spill = toml::Table::new();
        spill.insert("limit_bytes".to_owned(), leaf(4096));
        proposal.insert("spill".to_owned(), toml::Value::Table(spill));
        let mut profile = OracleCalibrationProfile {
            schema_version: 1,
            status: OracleCalibrationStatus::Candidate,
            generated_from: "test".to_owned(),
            source_revision: "test".to_owned(),
            environment: OracleCalibrationEnvironment {
                hardware: "test".to_owned(),
                os: "test".to_owned(),
                runtime: "test".to_owned(),
            },
            workload: OracleCalibrationWorkload {
                hashes: vec!["test".to_owned()],
                seeds: vec![1],
                data_volumes_bytes: vec![1],
                warmup_seconds: 1,
                measurement_seconds: 1,
            },
            matrix: OracleCalibrationMatrix {
                topology: vec!["test".to_owned()],
                tenant: vec!["test".to_owned()],
                class: vec!["test".to_owned()],
                visibility: vec!["test".to_owned()],
            },
            slot: OracleCalibrationSlot {
                cpu_cores: 1.0,
                memory_bytes: 1024,
                headroom: 0.25,
            },
            class: OracleCalibrationClasses {
                interactive: OracleCalibrationClass {
                    share: 0.5,
                    minimum_slots: 1,
                },
                analytical: OracleCalibrationClass {
                    share: 0.5,
                    minimum_slots: 1,
                },
            },
            proposal,
            measurements: toml::Table::new(),
        };
        let runtime = OracleRuntimeConfig::default();
        let translated = translate_oracle_calibration(&profile, &runtime, 8).expect("translation");
        assert_eq!(translated.interactive_slots, 3);
        assert_eq!(translated.analytical_slots, 3);
        assert!(
            translate_oracle_calibration(&profile, &runtime, 1)
                .expect_err("one usable slot must fail closed")
                .contains("at least 2")
        );
        profile.class.interactive.minimum_slots = 0;
        assert!(
            translate_oracle_calibration(&profile, &runtime, 8)
                .expect_err("zero class minimum must fail closed")
                .contains("must be positive")
        );
        profile.class.interactive.minimum_slots = u64::from(u32::MAX) + 1;
        assert!(
            translate_oracle_calibration(&profile, &runtime, 8)
                .expect_err("oversized class minimum must fail closed")
                .contains("exceeds u32")
        );
        profile.class.interactive.minimum_slots = 1;
        profile.class.analytical.minimum_slots = 1;
        profile.class.interactive.share = 1.0;
        profile.class.analytical.share = 1.0;
        assert!(
            translate_oracle_calibration(&profile, &runtime, 3)
                .expect_err("class allocations exceeding usable slots must fail closed")
                .contains("exceeds usable slots")
        );
    }

    /// Delegated admission defaults remain tied to role membership timing.
    ///
    /// # Panics
    ///
    /// Panics when the production defaults violate their locked relationship.
    #[test]
    fn delegated_admission_defaults_follow_role_liveness() {
        let runtime = OracleRuntimeConfig::default();
        let delegated = runtime
            .delegated_admission_config()
            .expect("default delegated admission is valid");
        assert_eq!(
            delegated.renewal_interval,
            vala_bifrost_redux::cluster::ROLE_HEARTBEAT_INTERVAL
        );
        assert_eq!(
            delegated.validity,
            vala_bifrost_redux::cluster::ROLE_LIVENESS_CUTOFF
        );
    }

    /// Zero, inverted, and over-liveness timing configurations fail closed.
    ///
    /// # Panics
    ///
    /// Panics when any invalid configuration is accepted.
    #[test]
    fn delegated_admission_rejects_invalid_timing() {
        let mut runtime = OracleRuntimeConfig {
            delegated_allocation_units: 0,
            ..OracleRuntimeConfig::default()
        };
        assert!(runtime.delegated_admission_config().is_err());
        runtime.delegated_allocation_units = 1;
        runtime.delegated_renewal_ms = runtime.delegated_validity_ms;
        assert!(runtime.delegated_admission_config().is_err());
        runtime.delegated_renewal_ms = 1;
        runtime.delegated_validity_ms = default_oracle_delegated_validity_ms() + 1;
        assert!(runtime.delegated_admission_config().is_err());
    }
}
