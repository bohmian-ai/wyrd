//! Authenticated `DataFusion` physical-plan extension codec for Oracle followers.

use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::error::Result as DataFusionResult;
use datafusion::physical_expr::PhysicalExpr;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex};

use arrow::datatypes::SchemaRef;
use datafusion::common::{DataFusionError, Result};
use datafusion::execution::TaskContext;
use datafusion::physical_plan::{
    DisplayAs, DisplayFormatType, ExecutionPlan, PlanProperties, SendableRecordBatchStream,
};
use datafusion_proto::physical_plan::PhysicalExtensionCodec;
use prost::Message;
use sha2::{Digest as _, Sha256};

/// Version of the private Oracle physical extension envelope.
pub const ORACLE_PHYSICAL_CODEC_VERSION: u32 = 1;
/// Only extension type accepted by the Oracle follower.
pub const ORACLE_REMOTE_SCAN_TAG: &str = "wyrd.oracle.remote_scan";
/// Extension type for the authenticated tenant tripwire surrounding follower sources.
pub const ORACLE_TENANT_TRIPWIRE_TAG: &str = "wyrd.oracle.tenant_tripwire";
/// Wire tag for a leader-owned leaf that bound no rows for this attempt.
///
/// The leader's drained live tail is planned unconditionally, because the drain
/// runs after admission and planning cannot know whether a table has live rows.
/// When the bound tail is empty the leaf contributes nothing, so a stage
/// carrying it can still be dispatched by encoding it as an empty leaf of the
/// same schema. A tail that bound rows has no wire form and refuses instead.
pub const ORACLE_EMPTY_LEAF_TAG: &str = "wyrd.oracle.empty_leaf";

/// Fingerprints the exact versioned physical-plan bytes shared by all followers.
#[must_use]
pub fn physical_plan_fingerprint(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"wyrd.oracle.physical-plan.v1\0");
    digest.update(ORACLE_PHYSICAL_CODEC_VERSION.to_be_bytes());
    digest.update(bytes);
    format!("sha256:{:x}", digest.finalize())
}

/// Encodes one follower subtree into the exact bytes dispatched to a worker,
/// together with the codec fingerprint bound into its ticket and footer.
///
/// The leader and any harness that dispatches a fragment must produce
/// byte-identical output, because the fingerprint is what a follower checks
/// the received plan against. Keeping the encoder and the fingerprint in one
/// operation is what makes that impossible to get wrong at a call site.
///
/// # Errors
///
/// Returns the `DataFusion` error raised when the subtree contains a node the
/// Oracle extension codec cannot encode.
pub fn encode_follower_subtree(plan: Arc<dyn ExecutionPlan>) -> Result<(Vec<u8>, String)> {
    let bytes = datafusion_proto::bytes::physical_plan_to_bytes_with_extension_codec(
        plan,
        &OraclePhysicalExtensionCodec::encoder(),
    )?
    .to_vec();
    let fingerprint = physical_plan_fingerprint(&bytes);
    Ok((bytes, fingerprint))
}

/// Versioned extension envelope stored in a `DataFusion` extension node.
#[derive(Clone, PartialEq, Message)]
struct OracleExtensionEnvelope {
    /// Private codec version.
    #[prost(uint32, tag = "1")]
    codec_version: u32,
    /// Stable extension type identifier.
    #[prost(string, tag = "2")]
    type_tag: String,
    /// Type-specific prost payload.
    #[prost(bytes, tag = "3")]
    payload: Vec<u8>,
}

/// Identity payload for one remote scan placeholder.
#[derive(Clone, PartialEq, Message)]
pub(crate) struct RemoteScanPayload {
    /// Stable request-local scan identifier.
    #[prost(string, tag = "1")]
    pub(crate) scan_id: String,
    /// Fingerprint of the expected provider schema.
    #[prost(string, tag = "2")]
    pub(crate) schema_fingerprint: String,
    /// JSON-encoded [`FollowerScanAssignment`], present only for Analytical.
    ///
    /// The Interactive follower receives its assignments alongside the plan in
    /// a separately signed `ExecuteFragmentRequest`, so its placeholders leave
    /// this empty. An Analytical stage has no such envelope: upstream ships
    /// only the serialized plan, and the stage ticket digests that plan, so
    /// carrying the assignment *inside* the placeholder is what binds it to the
    /// leader's signature. An empty value means "no assignment", never "an
    /// assignment selecting nothing".
    #[prost(bytes, tag = "3")]
    pub(crate) assignment_json: Vec<u8>,
    /// Closure schema the placeholder advertises, as `datafusion-proto` bytes.
    ///
    /// Only an Analytical placeholder needs this. Its Interactive counterpart
    /// is replaced by a pre-resolved provider that already carries a schema,
    /// while the Analytical decode has to publish plan properties before the
    /// provider is resolved.
    #[prost(bytes, tag = "4")]
    pub(crate) closure_schema: Vec<u8>,
    /// Output partitions the placeholder advertises. Zero is normalized to one.
    #[prost(uint32, tag = "5")]
    pub(crate) partitions: u32,
}

/// Serialized authenticated tripwire facts; its input remains a native extension child.
#[derive(Clone, PartialEq, Message)]
struct TenantTripwirePayload {
    /// JSON-encoded internal authenticated query context.
    #[prost(bytes, tag = "1")]
    context_json: Vec<u8>,
    /// Canonical tenant-free table label used by security audit.
    #[prost(string, tag = "2")]
    table: String,
}

/// IO-free description of one supported physical extension.
pub(crate) enum PreflightExtension {
    /// One authenticated role-local source placeholder.
    RemoteScan(RemoteScanPayload),
    /// One tenant tripwire whose child remains in the native plan tree.
    TenantTripwire {
        /// Authenticated query facts encoded by the leader.
        ///
        /// Boxed because this context dwarfs every sibling variant; keeping it
        /// inline would make each `PreflightExtension` pay its full size.
        context: Box<super::AuthorizedQueryContext>,
        /// Canonical table label used by security audit.
        table: String,
    },
}

/// The one pinned source occurrence a placeholder was planned against.
///
/// The provider knows all four facts while it builds the leaf; keeping them
/// together is what lets the post-admission binder select the exact cut and
/// tier this occurrence delegates without parsing or recomputing its scan
/// identity.
#[derive(Debug, Clone)]
pub(super) struct PlannedRemoteSource {
    /// The one frozen participant every task of this leaf's stage routes to.
    pub(super) destination: super::dispatcher::DispatchCandidate,
    /// Authenticated data tenant the cut was pinned for.
    pub(super) tenant: wyrd_spec::DataTenantId,
    /// Canonical fully-qualified table name of that cut.
    pub(super) table: String,
    /// The one persisted tier of the cut this occurrence delegates.
    pub(super) tier: super::RemotePersistedTier,
}

/// Leaf placeholder substituted for one Wyrd-owned source before serialization.
#[derive(Debug, Clone)]
pub struct RemoteSourcePlaceholderExec {
    /// Stable request-local identity.
    scan_id: String,
    /// Expected provider schema fingerprint.
    schema_fingerprint: String,
    /// Empty executable carrying the placeholder schema and plan properties.
    empty: datafusion::physical_plan::empty::EmptyExec,
    /// Closed projection closure computed by the provider's classifier —
    /// see `oracle::exec::required_columns_closure`. Defaults to empty until
    /// [`Self::with_closure`] attaches the real value computed during
    /// `OracleTableProvider::scan`; the caller must never dispatch a wire
    /// assignment with an empty closure (that would drop the hidden tenant
    /// column), so an empty value here is a signal to keep the assignment's
    /// existing safe full-schema default rather than overwrite it.
    required_columns: Vec<String>,
    /// Closed leaf predicates computed by the same classifier, in filter order.
    predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    /// Complete Analytical assignment this placeholder carries to its follower.
    ///
    /// `None` on the Interactive path, where the dispatcher signs and sends
    /// assignments beside the plan. `Some` on the Analytical path, where the
    /// placeholder is the only thing that crosses the wire and the stage
    /// ticket's digest over the plan is what binds the assignment.
    assignment: Option<Box<wyrd_spec::vala::api::FollowerScanAssignment>>,
    /// The pinned cut, tier, and frozen participant of this scan occurrence.
    ///
    /// `None` before a source is attached, which is the Interactive
    /// dispatcher's own leaf. Once attached it is private stage authority: the
    /// route handler refuses any stage whose remote leaves do not all name this
    /// exact peer, so a leaf can never be executed by a participant the cut did
    /// not authorize, and the binder resolves the cut it belongs to from this
    /// binding rather than by recomputing a scan identity.
    source: Option<Box<PlannedRemoteSource>>,
    /// The real leader-readable leaf this placeholder substituted, if any.
    ///
    /// Present whenever the provider had a plan the leader can execute itself.
    /// It is deliberately not exposed through [`ExecutionPlan::children`], so
    /// the node stays a leaf for the distributed planner's scale-up event and
    /// keeps receiving one variant per stage task. It never crosses the wire:
    /// encoding emits the follower assignment instead.
    local: Option<Arc<dyn ExecutionPlan>>,
    /// This variant's ordinal within its stage's final task count.
    task_index: usize,
    /// Final task count of the stage this variant belongs to, at least one.
    task_count: usize,
}

impl RemoteSourcePlaceholderExec {
    /// Creates one remote source placeholder with no attached predicate
    /// closure. Callers that have a real classifier result should chain
    /// [`Self::with_closure`] immediately.
    #[must_use]
    pub fn new(
        scan_id: impl Into<String>,
        schema_fingerprint: impl Into<String>,
        schema: SchemaRef,
    ) -> Self {
        Self {
            scan_id: scan_id.into(),
            schema_fingerprint: schema_fingerprint.into(),
            empty: datafusion::physical_plan::empty::EmptyExec::new(schema),
            required_columns: Vec::new(),
            predicates: Vec::new(),
            assignment: None,
            source: None,
            local: None,
            task_index: 0,
            task_count: 1,
        }
    }

    /// Attaches the leader-readable leaf this placeholder substituted.
    ///
    /// Callers pass the plan the provider would otherwise have returned. With
    /// it attached the same substituted leaf serves both paths, which is what
    /// lets the planner — not the provider — decide whether this cut is read
    /// locally or by a follower.
    #[must_use]
    pub(super) fn with_local(mut self, local: Arc<dyn ExecutionPlan>) -> Self {
        // The placeholder must advertise the local plan's partitioning, not the
        // single partition an `EmptyExec` reports. `DataFusion` executes only
        // the partitions a node claims, so understating them silently drops
        // every leaf beyond the first — a union of an empty published scan and
        // a populated hot scan would return the published side alone.
        self.empty = self
            .empty
            .with_partitions(local.properties().partitioning.partition_count());
        self.local = Some(local);
        self
    }

    /// Freezes the pinned cut, tier, and participant this occurrence reads.
    #[must_use]
    pub(super) fn with_source(mut self, source: PlannedRemoteSource) -> Self {
        self.source = Some(Box::new(source));
        self
    }

    /// Returns the frozen destination, if one was attached.
    #[must_use]
    pub(super) fn destination(&self) -> Option<&super::dispatcher::DispatchCandidate> {
        self.source.as_ref().map(|source| &source.destination)
    }

    /// Returns the planned source key this leaf binds through, if it is remote.
    ///
    /// A placeholder without a frozen source is not a planned Oracle source —
    /// it is the Interactive dispatcher's own leaf — so it names no key. The
    /// key is derived from the leaf's own retained facts rather than stored, so
    /// a closure attached after the source can never leave the two disagreeing.
    /// Task variants of one occurrence derive an identical key, because a share
    /// divides work rather than authority.
    #[must_use]
    pub(super) fn source_key(&self) -> Option<super::bindings::OracleSourceKey> {
        self.source.as_ref().map(|source| {
            super::bindings::OracleSourceKey::Follower(Box::new(
                super::bindings::FollowerSourceKey {
                    scan_id: self.scan_id.clone(),
                    destination: source.destination.clone(),
                    tenant: source.tenant,
                    table: source.table.clone(),
                    tier: source.tier,
                    schema_fingerprint: self.schema_fingerprint.clone(),
                    required_columns: self.required_columns.clone(),
                    predicates: self.predicates.clone(),
                },
            ))
        })
    }

    /// Narrows this leaf to one task's share of its stage.
    ///
    /// The split handler produces one variant per final stage task. The share
    /// is applied to the bound assignment at encode time rather than here,
    /// because the assignment does not exist until after admission.
    #[must_use]
    pub(super) fn with_task_share(mut self, task_index: usize, task_count: usize) -> Self {
        self.task_count = task_count.max(1);
        self.task_index = task_index.min(self.task_count - 1);
        self
    }

    /// Narrows one bound assignment to this variant's share of its stage.
    ///
    /// The split handler records only an index and a count, because the files
    /// do not exist when a leaf is split. This is where that share becomes
    /// concrete, so no task ever carries the complete file set and no file
    /// reaches two tasks. Every other authority and closure field is carried
    /// through unchanged: the share divides work, never permission.
    #[must_use]
    pub(super) fn narrow(
        &self,
        mut assignment: wyrd_spec::vala::api::FollowerScanAssignment,
    ) -> wyrd_spec::vala::api::FollowerScanAssignment {
        let tasks = self.task_count.max(1);
        assignment.persisted.files = assignment
            .persisted
            .files
            .into_iter()
            .skip(self.task_index)
            .step_by(tasks)
            .collect();
        assignment
    }

    /// Attaches the complete Analytical assignment this leaf carries.
    ///
    /// Only the Analytical leader calls this. The assignment names every file
    /// the stage may read; upstream's scale-up handler narrows it to one task's
    /// share before the plan is serialized, so what reaches a follower is that
    /// follower's own slice and nothing wider.
    #[must_use]
    pub fn with_assignment(
        mut self,
        assignment: wyrd_spec::vala::api::FollowerScanAssignment,
    ) -> Self {
        self.assignment = Some(Box::new(assignment));
        self
    }

    /// Returns the attached Analytical assignment, if this is an Analytical leaf.
    #[must_use]
    pub fn assignment(&self) -> Option<&wyrd_spec::vala::api::FollowerScanAssignment> {
        self.assignment.as_deref()
    }

    /// Returns the partition count this placeholder advertises to planning.
    #[must_use]
    pub fn partitions(&self) -> usize {
        self.empty.properties().partitioning.partition_count()
    }

    /// Advertises the session's target partition count for this source.
    ///
    /// Physical planning decides where to parallelize from the partitioning a
    /// leaf reports. A single-partition leaf makes `DataFusion` insert a
    /// round-robin repartition directly above it, and that repartition then
    /// becomes the innermost exchange -- so the split boundary lands under the
    /// partial aggregate and the follower receives a bare scan instead of a
    /// reduced one. Reporting the session's target partitions keeps the boundary
    /// at the exchange above the partial aggregate.
    ///
    /// The count is planning-only. On the leader this placeholder is replaced by
    /// the remote scan, whose partitioning is the selected participant count; on
    /// a follower it is replaced by the resolved provider's own partitioning.
    #[must_use]
    pub fn with_partitions(mut self, partitions: usize) -> Self {
        self.empty = self.empty.with_partitions(partitions.max(1));
        self
    }

    /// Attaches the provider's closed predicate/projection closure computed
    /// during `scan()`. This is the value the dispatcher reads back off the
    /// planned physical tree to fill the wire `FollowerScanAssignment`.
    #[must_use]
    pub fn with_closure(
        mut self,
        required_columns: Vec<String>,
        predicates: Vec<wyrd_spec::vala::assignment_authority::ScanPredicate>,
    ) -> Self {
        self.required_columns = required_columns;
        self.predicates = predicates;
        self
    }

    /// Reports whether this leaf resolves a live Scribe stream rather than a
    /// set of immutable persisted files.
    ///
    /// A Scribe source is a single live memtable cut, so it cannot be split
    /// across tasks the way a file list can: it resolves exactly once. The
    /// assignment is authoritative when one is already attached; before
    /// binding, the deterministic scan identity is, because both sides mint it
    /// from the same stream identity.
    #[must_use]
    pub(super) fn is_scribe(&self) -> bool {
        self.assignment.as_ref().map_or_else(
            || self.scan_id.contains(":scribe:"),
            |assignment| assignment.scribe_provider_cut.is_some(),
        )
    }

    /// Returns the stable scan identity.
    #[must_use]
    pub fn scan_id(&self) -> &str {
        &self.scan_id
    }

    /// Returns the expected provider schema fingerprint carried to the follower.
    ///
    /// This identifies the table's *complete* canonical physical schema and
    /// never narrows with the projection closure: a follower validates its own
    /// resolved catalog schema against this value, then derives the leaf
    /// closure schema from that schema plus the signed column names.
    #[must_use]
    pub fn schema_fingerprint(&self) -> &str {
        &self.schema_fingerprint
    }

    /// Returns the attached projection closure, or an empty slice when no
    /// closure was attached (the caller must treat that as "no update",
    /// never as "project nothing").
    #[must_use]
    pub fn required_columns(&self) -> &[String] {
        &self.required_columns
    }

    /// Returns the attached closed predicates, in filter order.
    #[must_use]
    pub fn predicates(&self) -> &[wyrd_spec::vala::assignment_authority::ScanPredicate] {
        &self.predicates
    }

    /// Returns the leader-readable leaf this placeholder substituted, if any.
    ///
    /// The local plan is deliberately hidden from [`ExecutionPlan::children`]
    /// so the distributed planner keeps treating this node as a leaf and gives
    /// it one variant per stage task. Any walk over the retained root would
    /// therefore stop here and see no source at all, which is wrong for
    /// anything accounting for what the leader actually executed. Scan
    /// evidence reads the substituted leaf back through this accessor.
    #[must_use]
    pub fn local_plan(&self) -> Option<&Arc<dyn ExecutionPlan>> {
        self.local.as_ref()
    }
}

impl DisplayAs for RemoteSourcePlaceholderExec {
    /// Formats only non-secret placeholder identity.
    ///
    /// # Errors
    /// Returns the formatter error if the destination cannot accept the identity.
    fn fmt_as(
        &self,
        _display: DisplayFormatType,
        formatter: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(formatter, "RemoteSourcePlaceholderExec: {}", self.scan_id)
    }
}

impl ExecutionPlan for RemoteSourcePlaceholderExec {
    /// Visits every physical expression this plan owns.
    ///
    /// This plan owns no `PhysicalExpr`, so the traversal reports
    /// [`TreeNodeRecursion::Continue`] without invoking `f`.
    ///
    /// # Errors
    /// Never returns an error; the signature is fixed by the trait.
    fn apply_expressions(
        &self,
        _f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
    ) -> DataFusionResult<TreeNodeRecursion> {
        Ok(TreeNodeRecursion::Continue)
    }

    /// Returns the stable diagnostic operator name.
    fn name(&self) -> &'static str {
        "RemoteSourcePlaceholderExec"
    }
    /// Returns the placeholder's cached physical properties.
    fn properties(&self) -> &Arc<PlanProperties> {
        self.empty.properties()
    }
    /// A remote scan is always a leaf.
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        Vec::new()
    }
    /// Rejects children and preserves the immutable leaf.
    ///
    /// # Errors
    /// Returns a plan error when a caller attempts to attach any child.
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.is_empty() {
            Ok(self)
        } else {
            Err(DataFusionError::Plan(
                "remote scan must remain a leaf".to_owned(),
            ))
        }
    }
    /// Executes as empty only before role-local follower reconstruction.
    ///
    /// # Errors
    /// Returns the wrapped empty plan's execution error for an invalid partition
    /// or task context.
    fn execute(
        &self,
        partition: usize,
        context: Arc<TaskContext>,
    ) -> Result<SendableRecordBatchStream> {
        match self.local.as_ref() {
            Some(local) => local.execute(partition, context),
            None => self.empty.execute(partition, context),
        }
    }
}

/// Physical extension codec that encodes placeholders and consumes prebuilt providers.
pub struct OraclePhysicalExtensionCodec {
    /// Complete role-local provider registry, consumed once per scan.
    providers: Mutex<HashMap<String, Arc<dyn ExecutionPlan>>>,
    /// Exact follower audit owner used only when decoding a tenant tripwire.
    audit: Option<Arc<dyn super::OracleAudit>>,
    /// Analytical leaf binding, present only on an Analytical follower session.
    ///
    /// Interactive followers resolve every provider before decode and register
    /// them in `providers`. Analytical followers cannot: their plan arrives
    /// through upstream's worker, which builds the session before it has the
    /// plan bytes. This binding lets decode construct a lazily resolving leaf
    /// instead of demanding a provider that could not yet exist.
    analytical: Option<AnalyticalLeafBinding>,
    /// Session lock the leader resolves a planned leaf's bound assignment from.
    ///
    /// `None` on every decode-side codec and on the Interactive path, where the
    /// dispatcher attaches the signed assignment to the placeholder itself.
    /// `Some` on the single planner's leader session, where the placeholder is
    /// planned before admission and the assignment only exists after the
    /// audited drain publishes it.
    bindings: Option<Arc<super::bindings::OracleExecutionLock>>,
}

/// Per-session capability an Analytical follower needs to build its own leaves.
#[derive(Clone)]
pub struct AnalyticalLeafBinding {
    /// Role this node executes as, selecting the resolver's source family.
    role: wyrd_spec::vala::api::ClusterRole,
    /// Process resolver that turns a signed assignment into a local provider.
    resolver: Arc<dyn super::follower::FollowerSourceResolver>,
    /// Audit owner the tenant tripwire above each source fails closed against.
    audit: Arc<dyn super::OracleAudit>,
    /// This node's reader epoch, which every decoded leaf protects under.
    ///
    /// `None` only on a node that runs no Oracle epoch, where a decoded leaf
    /// carrying a snapshot-bearing cut is refused rather than read unprotected.
    reader_authority: Option<Arc<super::reader_pins::OracleReaderAuthority>>,
    /// Graph-scoped owner every decoded leaf hands its reader guard to.
    ///
    /// `None` on the node-wide binding the ingress retains, because no single
    /// graph owns that one. The per-session clone
    /// [`AnalyticalLeafBinding::for_graph`] builds always carries it, and a leaf
    /// that acquires protection without one is refused rather than allowed to
    /// hold a claim the graph's teardown cannot release.
    guard_sink: Option<Arc<super::analytical_scan::GraphReaderGuardSink>>,
}

impl AnalyticalLeafBinding {
    /// Creates the Analytical decode binding for one follower session.
    #[must_use]
    pub fn new(
        role: wyrd_spec::vala::api::ClusterRole,
        resolver: Arc<dyn super::follower::FollowerSourceResolver>,
        audit: Arc<dyn super::OracleAudit>,
        reader_authority: Option<Arc<super::reader_pins::OracleReaderAuthority>>,
    ) -> Self {
        Self {
            role,
            resolver,
            audit,
            reader_authority,
            guard_sink: None,
        }
    }

    /// Binds this node-wide capability to the one graph a session serves.
    ///
    /// The session builder resolves the graph before it installs the codec, so
    /// this is where a leaf's reader guard acquires an owner whose lifetime is
    /// the graph rather than the upstream task cache.
    #[must_use]
    pub(super) fn for_graph(
        mut self,
        graph: super::analytical::AnalyticalGraphKey,
        registry: Arc<super::analytical::AnalyticalRuntimeRegistry>,
    ) -> Self {
        self.guard_sink = Some(Arc::new(super::analytical_scan::GraphReaderGuardSink::new(
            graph, registry,
        )));
        self
    }
}

impl fmt::Debug for AnalyticalLeafBinding {
    /// Renders only the non-secret role, never resolver internals.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalyticalLeafBinding")
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for OraclePhysicalExtensionCodec {
    /// Redacts providers and audit internals while preserving codec shape.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OraclePhysicalExtensionCodec")
            .field("audit", &self.audit.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for OraclePhysicalExtensionCodec {
    /// Creates an empty encoding-only codec.
    fn default() -> Self {
        Self {
            providers: Mutex::new(HashMap::new()),
            audit: None,
            analytical: None,
            bindings: None,
        }
    }
}

impl OraclePhysicalExtensionCodec {
    /// Creates an encoding-only codec.
    #[must_use]
    pub fn encoder() -> Self {
        Self::default()
    }

    /// Creates a decoding codec with the complete validated provider registry.
    #[must_use]
    pub fn decoder(providers: HashMap<String, Arc<dyn ExecutionPlan>>) -> Self {
        Self {
            providers: Mutex::new(providers),
            audit: None,
            analytical: None,
            bindings: None,
        }
    }

    /// Creates the Analytical codec used on both sides of a stage boundary.
    ///
    /// The same value encodes leaves on its way out and decodes them on its way
    /// in, because upstream installs one user codec per session and a follower
    /// running a middle stage does both.
    #[must_use]
    pub fn analytical(binding: AnalyticalLeafBinding) -> Self {
        Self {
            providers: Mutex::new(HashMap::new()),
            audit: Some(Arc::clone(&binding.audit)),
            analytical: Some(binding),
            bindings: None,
        }
    }

    /// Attaches the session lock this codec resolves planned leaf sources through.
    ///
    /// A leader-side placeholder is planned before admission and therefore
    /// carries no assignment of its own. The lock is the one place the bound
    /// assignment arrives, after admission and the audited drain, so encoding
    /// resolves through it rather than through anything the plan captured.
    #[must_use]
    pub(super) fn with_bindings(
        mut self,
        bindings: Arc<super::bindings::OracleExecutionLock>,
    ) -> Self {
        self.bindings = Some(bindings);
        self
    }

    /// Creates a decoding codec with exact providers and follower security audit ownership.
    #[must_use]
    pub fn decoder_with_audit(
        providers: HashMap<String, Arc<dyn ExecutionPlan>>,
        audit: Arc<dyn super::OracleAudit>,
    ) -> Self {
        Self {
            providers: Mutex::new(providers),
            audit: Some(audit),
            analytical: None,
            bindings: None,
        }
    }

    /// Requires the native decoder to consume every authenticated provider exactly once.
    ///
    /// # Errors
    /// Returns a plan error when the decoded tree did not reference every preflighted scan.
    pub fn require_complete_consumption(&self) -> Result<()> {
        let providers = self
            .providers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if providers.is_empty() {
            Ok(())
        } else {
            Err(DataFusionError::Plan(format!(
                "decoded physical plan left {} authenticated provider(s) unused",
                providers.len()
            )))
        }
    }

    /// Validates one envelope without constructing or resolving providers.
    ///
    /// # Errors
    /// Returns a plan error for malformed, unsupported, empty, or duplicate identities.
    pub fn preflight(envelopes: &[Vec<u8>]) -> Result<Vec<(String, String)>> {
        let mut seen = HashSet::new();
        envelopes
            .iter()
            .map(|bytes| {
                let payload = Self::decode_payload(bytes)?;
                if payload.scan_id.is_empty()
                    || payload.schema_fingerprint.is_empty()
                    || !seen.insert(payload.scan_id.clone())
                {
                    return Err(DataFusionError::Plan(
                        "invalid or duplicate remote scan identity".to_owned(),
                    ));
                }
                Ok((payload.scan_id, payload.schema_fingerprint))
            })
            .collect()
    }

    /// Decodes and validates the fixed extension envelope.
    ///
    /// # Errors
    /// Returns a plan error when the envelope or payload is malformed or unsupported.
    pub(crate) fn decode_payload(bytes: &[u8]) -> Result<RemoteScanPayload> {
        let envelope = Self::decode_envelope(bytes)?;
        if envelope.type_tag != ORACLE_REMOTE_SCAN_TAG {
            return Err(DataFusionError::Plan(
                "Oracle extension is not a remote scan".to_owned(),
            ));
        }
        RemoteScanPayload::decode(envelope.payload.as_slice())
            .map_err(|error| DataFusionError::Plan(format!("invalid remote scan payload: {error}")))
    }

    /// Decodes one supported extension for the follower's IO-free preflight.
    ///
    /// # Errors
    /// Returns a plan error when the envelope, payload, or tripwire context is invalid.
    pub(crate) fn preflight_extension(bytes: &[u8]) -> Result<PreflightExtension> {
        let envelope = Self::decode_envelope(bytes)?;
        match envelope.type_tag.as_str() {
            ORACLE_REMOTE_SCAN_TAG => RemoteScanPayload::decode(envelope.payload.as_slice())
                .map(PreflightExtension::RemoteScan)
                .map_err(|error| {
                    DataFusionError::Plan(format!("invalid remote scan payload: {error}"))
                }),
            ORACLE_TENANT_TRIPWIRE_TAG => {
                let payload = TenantTripwirePayload::decode(envelope.payload.as_slice()).map_err(
                    |error| DataFusionError::Plan(format!("invalid tripwire payload: {error}")),
                )?;
                let context = serde_json::from_slice(&payload.context_json).map_err(|error| {
                    DataFusionError::Plan(format!("invalid tripwire context: {error}"))
                })?;
                if payload.table.trim().is_empty() {
                    return Err(DataFusionError::Plan(
                        "tenant tripwire table is empty".to_owned(),
                    ));
                }
                Ok(PreflightExtension::TenantTripwire {
                    context: Box::new(context),
                    table: payload.table,
                })
            }
            _ => Err(DataFusionError::Plan(
                "unsupported Oracle physical extension type".to_owned(),
            )),
        }
    }

    /// Decodes the common versioned extension envelope without assuming its closed type.
    fn decode_envelope(bytes: &[u8]) -> Result<OracleExtensionEnvelope> {
        let envelope = OracleExtensionEnvelope::decode(bytes).map_err(|error| {
            DataFusionError::Plan(format!("invalid Oracle extension envelope: {error}"))
        })?;
        if envelope.codec_version != ORACLE_PHYSICAL_CODEC_VERSION {
            return Err(DataFusionError::Plan(
                "unsupported Oracle extension version".to_owned(),
            ));
        }
        Ok(envelope)
    }
}

/// Projects one placeholder into its wire payload.
///
/// An Interactive placeholder carries identity only; the follower already holds
/// the signed assignment and a pre-resolved provider. An Analytical placeholder
/// additionally carries its assignment, its closure schema, and its advertised
/// partition count, because on that path the serialized plan is the *only*
/// thing that crosses the wire.
///
/// # Errors
///
/// Returns [`DataFusionError::Plan`] when the assignment cannot be JSON-encoded
/// or the closure schema cannot be projected into `datafusion-proto` form.
fn remote_scan_payload(
    scan: &RemoteSourcePlaceholderExec,
    bindings: Option<&super::bindings::OracleExecutionLock>,
) -> Result<RemoteScanPayload> {
    let bound = scan
        .source_key()
        .zip(bindings.and_then(std::sync::OnceLock::get))
        .map(|(key, bindings)| bindings.follower_assignment(&key).cloned())
        .transpose()?;
    let assignment = bound.as_ref().or_else(|| scan.assignment());
    let Some(assignment) = assignment else {
        return Ok(RemoteScanPayload {
            scan_id: scan.scan_id.clone(),
            schema_fingerprint: scan.schema_fingerprint.clone(),
            assignment_json: Vec::new(),
            closure_schema: Vec::new(),
            partitions: 0,
        });
    };
    // One narrowing site for both paths: the leader's bound assignment and the
    // Interactive dispatcher's attached one are divided the same way.
    let assignment = scan.narrow(assignment.clone());
    let assignment_json = serde_json::to_vec(&assignment).map_err(|error| {
        DataFusionError::Plan(format!("analytical assignment encoding failed: {error}"))
    })?;
    let schema =
        datafusion_proto::protobuf::Schema::try_from(scan.schema().as_ref()).map_err(|error| {
            DataFusionError::Plan(format!(
                "analytical closure schema encoding failed: {error}"
            ))
        })?;
    Ok(RemoteScanPayload {
        scan_id: scan.scan_id.clone(),
        schema_fingerprint: scan.schema_fingerprint.clone(),
        assignment_json,
        closure_schema: schema.encode_to_vec(),
        partitions: u32::try_from(scan.partitions()).unwrap_or(u32::MAX),
    })
}

/// Rebuilds the Analytical closure schema and assignment from one wire payload.
///
/// # Errors
///
/// Returns [`DataFusionError::Plan`] when the payload carries no assignment,
/// when either encoded field is malformed, or when the closure schema cannot be
/// projected back into Arrow form.
fn analytical_leaf_parts(
    payload: &RemoteScanPayload,
) -> Result<(
    wyrd_spec::vala::api::FollowerScanAssignment,
    SchemaRef,
    usize,
)> {
    if payload.assignment_json.is_empty() {
        return Err(DataFusionError::Plan(format!(
            "analytical remote scan {} carries no assignment",
            payload.scan_id
        )));
    }
    let assignment: wyrd_spec::vala::api::FollowerScanAssignment =
        serde_json::from_slice(&payload.assignment_json).map_err(|error| {
            DataFusionError::Plan(format!("invalid analytical assignment: {error}"))
        })?;
    if assignment.scan_id != payload.scan_id
        || assignment.schema_fingerprint != payload.schema_fingerprint
    {
        return Err(DataFusionError::Plan(
            "analytical assignment identity differs from its placeholder".to_owned(),
        ));
    }
    let schema = datafusion_proto::protobuf::Schema::decode(payload.closure_schema.as_slice())
        .map_err(|error| {
            DataFusionError::Plan(format!("invalid analytical closure schema: {error}"))
        })?;
    let schema = arrow::datatypes::Schema::try_from(&schema).map_err(|error| {
        DataFusionError::Plan(format!("invalid analytical closure schema: {error}"))
    })?;
    Ok((
        assignment,
        Arc::new(schema),
        usize::try_from(payload.partitions).unwrap_or(1).max(1),
    ))
}

impl PhysicalExtensionCodec for OraclePhysicalExtensionCodec {
    /// Reconstructs only a pre-resolved role-local provider for the authenticated scan.
    ///
    /// # Errors
    /// Returns a plan error for a non-leaf envelope, malformed payload, or absent
    /// authenticated provider.
    fn try_decode(
        &self,
        buf: &[u8],
        inputs: &[Arc<dyn ExecutionPlan>],
        _ctx: &TaskContext,
        _proto_converter: &dyn datafusion_proto::physical_plan::PhysicalProtoConverterExtension,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let envelope = Self::decode_envelope(buf)?;
        match envelope.type_tag.as_str() {
            ORACLE_EMPTY_LEAF_TAG => {
                if !inputs.is_empty() {
                    return Err(DataFusionError::Plan(
                        "empty leaf extension must be a leaf".to_owned(),
                    ));
                }
                let payload =
                    RemoteScanPayload::decode(envelope.payload.as_slice()).map_err(|error| {
                        DataFusionError::Plan(format!("invalid empty leaf payload: {error}"))
                    })?;
                let schema =
                    datafusion_proto::protobuf::Schema::decode(payload.closure_schema.as_slice())
                        .map_err(|error| {
                        DataFusionError::Plan(format!("invalid empty leaf schema: {error}"))
                    })?;
                let schema = arrow::datatypes::Schema::try_from(&schema).map_err(|error| {
                    DataFusionError::Plan(format!("invalid empty leaf schema: {error}"))
                })?;
                Ok(Arc::new(datafusion::physical_plan::empty::EmptyExec::new(
                    Arc::new(schema),
                )))
            }
            ORACLE_REMOTE_SCAN_TAG => {
                if !inputs.is_empty() {
                    return Err(DataFusionError::Plan(
                        "remote scan extension must be a leaf".to_owned(),
                    ));
                }
                let payload =
                    RemoteScanPayload::decode(envelope.payload.as_slice()).map_err(|error| {
                        DataFusionError::Plan(format!("invalid remote scan payload: {error}"))
                    })?;
                let pre_resolved = self
                    .providers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .remove(&payload.scan_id);
                if let Some(provider) = pre_resolved {
                    return Ok(provider);
                }
                // An Interactive follower has already registered every provider
                // it was authorized to read, so an absent entry is a refusal.
                // An Analytical follower has none: its assignment travels inside
                // this payload, under the same ticket digest that authorized the
                // raw message, and its provider is resolved on first execution.
                let Some(binding) = self.analytical.as_ref() else {
                    return Err(DataFusionError::Plan(format!(
                        "missing authenticated provider for scan {}",
                        payload.scan_id
                    )));
                };
                let (assignment, schema, partitions) = analytical_leaf_parts(&payload)?;
                Ok(Arc::new(super::analytical_scan::AnalyticalScanExec::new(
                    assignment,
                    binding.role,
                    Arc::clone(&binding.resolver),
                    binding.reader_authority.as_ref().map(Arc::clone),
                    binding.guard_sink.as_ref().map(Arc::clone),
                    schema,
                    partitions,
                )))
            }
            ORACLE_TENANT_TRIPWIRE_TAG => {
                let [input] = inputs else {
                    return Err(DataFusionError::Plan(
                        "tenant tripwire extension requires one input".to_owned(),
                    ));
                };
                let payload = TenantTripwirePayload::decode(envelope.payload.as_slice()).map_err(
                    |error| DataFusionError::Plan(format!("invalid tripwire payload: {error}")),
                )?;
                let context = serde_json::from_slice(&payload.context_json).map_err(|error| {
                    DataFusionError::Plan(format!("invalid tripwire context: {error}"))
                })?;
                let audit = self.audit.as_ref().ok_or_else(|| {
                    DataFusionError::Plan("follower tripwire audit owner is absent".to_owned())
                })?;
                Ok(Arc::new(super::exec::TenantTripwireExec::new(
                    Arc::clone(input),
                    context,
                    payload.table,
                    Arc::clone(audit),
                )?))
            }
            _ => Err(DataFusionError::Plan(
                "unsupported Oracle physical extension type".to_owned(),
            )),
        }
    }

    /// Encodes only the Wyrd remote-scan placeholder in the fixed v1 envelope.
    ///
    /// # Errors
    /// Returns a plan error for any other extension type or protobuf failure.
    fn try_encode(
        &self,
        node: Arc<dyn ExecutionPlan>,
        buf: &mut Vec<u8>,
        _proto_converter: &dyn datafusion_proto::physical_plan::PhysicalProtoConverterExtension,
    ) -> Result<()> {
        let (type_tag, payload) = if let Some(scan) =
            node.downcast_ref::<RemoteSourcePlaceholderExec>()
        {
            (
                ORACLE_REMOTE_SCAN_TAG,
                remote_scan_payload(scan, self.bindings.as_deref())?.encode_to_vec(),
            )
        } else if let Some(drained) = node
            .downcast_ref::<super::analytical_scan::AnalyticalScanExec>()
            .and_then(super::analytical_scan::AnalyticalScanExec::local_drained_key)
        {
            let bound = self
                .bindings
                .as_deref()
                .and_then(std::sync::OnceLock::get)
                .map(|bindings| bindings.local_batches(drained));
            // An unbound tail carries nothing either: the refusal is reserved
            // for a tail that actually resolved rows on this node.
            if bound.is_some_and(|batches| {
                batches.is_ok_and(|batches| batches.iter().any(|batch| batch.num_rows() > 0))
            }) {
                return Err(DataFusionError::Plan(
                    "leader drained tail holds rows and cannot cross the wire".to_owned(),
                ));
            }
            let schema = datafusion_proto::protobuf::Schema::try_from(node.schema().as_ref())
                .map_err(|error| {
                    DataFusionError::Plan(format!("empty leaf schema encoding failed: {error}"))
                })?;
            (
                ORACLE_EMPTY_LEAF_TAG,
                RemoteScanPayload {
                    scan_id: String::new(),
                    schema_fingerprint: String::new(),
                    assignment_json: Vec::new(),
                    closure_schema: schema.encode_to_vec(),
                    partitions: 0,
                }
                .encode_to_vec(),
            )
        } else if let Some(tripwire) = node.downcast_ref::<super::exec::TenantTripwireExec>() {
            (
                ORACLE_TENANT_TRIPWIRE_TAG,
                TenantTripwirePayload {
                    context_json: serde_json::to_vec(tripwire.context()).map_err(|error| {
                        DataFusionError::Plan(format!("tripwire context encoding failed: {error}"))
                    })?,
                    table: tripwire.table().to_owned(),
                }
                .encode_to_vec(),
            )
        } else {
            return Err(DataFusionError::Plan(
                "unsupported Oracle physical extension".to_owned(),
            ));
        };
        OracleExtensionEnvelope {
            codec_version: ORACLE_PHYSICAL_CODEC_VERSION,
            type_tag: type_tag.to_owned(),
            payload,
        }
        .encode(buf)
        .map_err(|error| {
            DataFusionError::Plan(format!("Oracle extension encoding failed: {error}"))
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behavioral proof for physical-plan encoding and provider ownership.
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use datafusion::datasource::MemTable;
    use datafusion::execution::context::SessionContext;
    use datafusion::physical_plan::execution_plan::{
        ChildrenPropertiesMode, ReplaceChildrenOptions,
    };
    use datafusion::physical_plan::{collect, displayable};
    use datafusion_proto::bytes::{
        physical_plan_from_bytes_with_extension_codec, physical_plan_to_bytes_with_extension_codec,
    };
    use datafusion_proto::protobuf::{
        AggregateExecNode, PhysicalExtensionNode, PhysicalPlanNode,
        physical_plan_node::PhysicalPlanType,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Provider wrapper whose destruction proves failed decode releases acquired state.
    #[derive(Debug)]
    struct DropObservedExec {
        /// Executable empty provider delegated by this wrapper.
        empty: datafusion::physical_plan::empty::EmptyExec,
        /// Number of wrapper drops.
        drops: Arc<AtomicUsize>,
        /// Number of physical execution attempts.
        executions: Arc<AtomicUsize>,
    }

    impl DropObservedExec {
        /// Creates a provider with one observable ownership token.
        #[must_use]
        fn new(schema: SchemaRef, drops: Arc<AtomicUsize>, executions: Arc<AtomicUsize>) -> Self {
            Self {
                empty: datafusion::physical_plan::empty::EmptyExec::new(schema),
                drops,
                executions,
            }
        }
    }

    impl Drop for DropObservedExec {
        /// Records release of the acquired provider.
        fn drop(&mut self) {
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }

    impl DisplayAs for DropObservedExec {
        /// Formats the non-secret test provider identity.
        ///
        /// # Errors
        /// Returns the formatter error if the destination cannot accept the identity.
        fn fmt_as(
            &self,
            _display: DisplayFormatType,
            formatter: &mut fmt::Formatter<'_>,
        ) -> fmt::Result {
            formatter.write_str("DropObservedExec")
        }
    }

    impl ExecutionPlan for DropObservedExec {
        /// Visits every physical expression this plan owns.
        ///
        /// This plan owns no `PhysicalExpr`, so the traversal reports
        /// [`TreeNodeRecursion::Continue`] without invoking `f`.
        ///
        /// # Errors
        /// Never returns an error; the signature is fixed by the trait.
        fn apply_expressions(
            &self,
            _f: &mut dyn FnMut(&Arc<dyn PhysicalExpr>) -> DataFusionResult<TreeNodeRecursion>,
        ) -> DataFusionResult<TreeNodeRecursion> {
            Ok(TreeNodeRecursion::Continue)
        }

        /// Returns the stable test operator name.
        fn name(&self) -> &'static str {
            "DropObservedExec"
        }
        /// Delegates cached properties.
        fn properties(&self) -> &Arc<PlanProperties> {
            self.empty.properties()
        }
        /// This provider is a leaf.
        fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
            Vec::new()
        }
        /// Preserves the leaf only when no children are supplied.
        ///
        /// # Errors
        /// Returns a plan error when a caller supplies a child.
        fn with_new_children(
            self: Arc<Self>,
            children: Vec<Arc<dyn ExecutionPlan>>,
        ) -> Result<Arc<dyn ExecutionPlan>> {
            if children.is_empty() {
                Ok(self)
            } else {
                Err(DataFusionError::Plan(
                    "drop-observed provider must remain a leaf".to_owned(),
                ))
            }
        }
        /// Delegates physical execution to the empty provider.
        ///
        /// # Errors
        /// Returns the delegated `DataFusion` execution error.
        fn execute(
            &self,
            partition: usize,
            context: Arc<TaskContext>,
        ) -> Result<SendableRecordBatchStream> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            self.empty.execute(partition, context)
        }
    }

    /// Encodes one canonical extension envelope for focused codec tests.
    fn envelope(scan_id: &str) -> Vec<u8> {
        OracleExtensionEnvelope {
            codec_version: ORACLE_PHYSICAL_CODEC_VERSION,
            type_tag: ORACLE_REMOTE_SCAN_TAG.to_owned(),
            payload: RemoteScanPayload {
                scan_id: scan_id.to_owned(),
                schema_fingerprint: "sha256:schema".to_owned(),
                ..RemoteScanPayload::default()
            }
            .encode_to_vec(),
        }
        .encode_to_vec()
    }

    /// Replaces the single physical source leaf with an Oracle extension placeholder.
    ///
    /// # Errors
    /// Returns a plan error if the native tree cannot accept reconstructed children.
    fn replace_source(
        plan: Arc<dyn ExecutionPlan>,
        source: &mut Option<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        let children = plan.children();
        if children.is_empty() {
            if source.is_some() {
                return Err(DataFusionError::Plan(
                    "test aggregate tree unexpectedly has multiple source leaves".to_owned(),
                ));
            }
            *source = Some(Arc::clone(&plan));
            return Ok(Arc::new(RemoteSourcePlaceholderExec::new(
                "scan",
                super::super::assignment_schema_fingerprint(plan.schema().as_ref()),
                plan.schema(),
            )));
        }
        let replaced = children
            .into_iter()
            .map(|child| replace_source(Arc::clone(child), source))
            .collect::<Result<Vec<_>>>()?;
        plan.replace_children(
            replaced,
            ReplaceChildrenOptions::new(ChildrenPropertiesMode::Recompute),
        )
    }

    /// A real native aggregate/sort/limit tree round-trips and executes identically.
    ///
    /// # Panics
    /// Panics if fixture construction, physical planning, codec round-trip, or
    /// execution violates its test invariant.
    #[tokio::test]
    async fn physical_plan_round_trip_preserves_aggregate_sort_and_limit() {
        let schema = Arc::new(Schema::new(vec![
            Field::new("group_name", DataType::Utf8, false),
            Field::new("value", DataType::Int64, false),
        ]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![
                Arc::new(StringArray::from(vec!["a", "a", "b"])),
                Arc::new(Int64Array::from(vec![2, 5, 3])),
            ],
        )
        .expect("valid aggregate input");
        let context = SessionContext::new();
        context
            .register_table(
                "source",
                Arc::new(MemTable::try_new(schema, vec![vec![batch]]).expect("valid table")),
            )
            .expect("table registration succeeds");
        let native = context
            .sql("SELECT group_name, SUM(value) AS total FROM source GROUP BY group_name ORDER BY total DESC LIMIT 1")
            .await
            .expect("valid aggregate SQL")
            .create_physical_plan()
            .await
            .expect("physical planning succeeds");
        let mut source = None;
        let outbound = replace_source(native, &mut source).expect("source replacement succeeds");
        let display = displayable(outbound.as_ref()).indent(true).to_string();
        assert!(display.contains("AggregateExec"));
        assert!(display.contains("SortExec"));
        assert!(
            display.contains("GlobalLimitExec") || display.contains("fetch=1"),
            "limit is absent from {display}"
        );
        assert!(display.contains("RemoteSourcePlaceholderExec"));
        let bytes = physical_plan_to_bytes_with_extension_codec(
            outbound,
            &OraclePhysicalExtensionCodec::encoder(),
        )
        .expect("native plan encodes");
        let providers = HashMap::from([(
            "scan".to_owned(),
            source.expect("one source leaf was captured"),
        )]);
        let codec = OraclePhysicalExtensionCodec::decoder(providers);
        let task = context.task_ctx();
        let decoded = physical_plan_from_bytes_with_extension_codec(&bytes, &task, &codec)
            .expect("native plan decodes");
        codec
            .require_complete_consumption()
            .expect("provider is consumed once");
        let rows = collect(decoded, task).await.expect("decoded plan executes");
        assert_eq!(rows.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let totals = rows[0]
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("aggregate total is int64");
        assert_eq!(totals.value(0), 7);
    }

    /// Every malformed tree or request-component contradiction fails before effects.
    ///
    /// # Panics
    /// Panics if malformed envelopes or any authenticated follower contradiction
    /// reaches resolver, decode, execution, or output effects.
    #[tokio::test]
    async fn preflight_rejects_malformed_tree_before_resolver_io() {
        assert!(OraclePhysicalExtensionCodec::preflight(&[vec![0]]).is_err());
        assert!(
            OraclePhysicalExtensionCodec::preflight(&[envelope("scan"), envelope("scan")]).is_err()
        );
        super::super::follower::tests::assert_complete_preflight_matrix().await;
    }

    /// A post-resolution native decode failure drops providers before execution/output.
    ///
    /// # Panics
    /// Panics if provider ownership or effect counters violate the fixture invariant.
    #[test]
    fn post_resolution_decode_failure_prevents_execution_and_output() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]));
        let drops = Arc::new(AtomicUsize::new(0));
        let executions = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn ExecutionPlan> = Arc::new(DropObservedExec::new(
            schema,
            Arc::clone(&drops),
            Arc::clone(&executions),
        ));
        let codec =
            OraclePhysicalExtensionCodec::decoder(HashMap::from([("scan".to_owned(), provider)]));
        let malformed = PhysicalPlanNode {
            physical_plan_type: Some(PhysicalPlanType::Aggregate(Box::new(AggregateExecNode {
                mode: 999,
                input: Some(Box::new(PhysicalPlanNode {
                    physical_plan_type: Some(PhysicalPlanType::Extension(PhysicalExtensionNode {
                        node: envelope("scan"),
                        inputs: Vec::new(),
                    })),
                })),
                ..Default::default()
            }))),
        }
        .encode_to_vec();
        let decoded = physical_plan_from_bytes_with_extension_codec(
            &malformed,
            &TaskContext::default(),
            &codec,
        );
        assert!(decoded.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
    }

    /// Proves a placeholder holding a local plan reads it on the leader.
    ///
    /// A placeholder that only ever yields `EmptyExec` forces the planner to
    /// put a network boundary above every substituted leaf, because the leader
    /// can no longer read its own cut. Carrying the real leaf lets one
    /// substitution serve both paths: the leader executes it directly when the
    /// planner leaves the leaf in the head stage, and the codec still encodes
    /// the assignment when the stage is dispatched.
    #[tokio::test]
    async fn placeholder_executes_its_local_plan_on_the_leader() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(Int64Array::from(vec![7_i64, 8]))],
        )
        .expect("the fixture batch matches its schema");
        let table = MemTable::try_new(Arc::clone(&schema), vec![vec![batch]])
            .expect("the fixture table matches its schema");
        let ctx = SessionContext::new();
        let local = ctx
            .read_table(Arc::new(table))
            .expect("the fixture table registers")
            .create_physical_plan()
            .await
            .expect("the fixture table plans");

        let placeholder = RemoteSourcePlaceholderExec::new("oracle:t:persisted", "fp", schema)
            .with_local(Arc::clone(&local));

        let rows = collect(Arc::new(placeholder), ctx.task_ctx())
            .await
            .expect("the placeholder executes its local plan")
            .iter()
            .map(arrow::array::RecordBatch::num_rows)
            .sum::<usize>();
        assert_eq!(rows, 2, "the leader must read the local plan's rows");
    }
}
