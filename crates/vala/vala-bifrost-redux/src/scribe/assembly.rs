//! Cross-generation staging claims — what gets merged into one hot object.
//!
//! Batch-ID routing deliberately spreads one logical `(tenant, table,
//! partition)` key across all sixteen shards, and each shard rotates its
//! generation on its own aggregate size and age. One key therefore arrives on
//! the staging volume as many small immutable members produced by different
//! shards at different times. Publishing each of those as its own hot object is
//! exactly the amplification this refactor exists to remove: one object PUT,
//! one `file_list` row, and one Oracle footer open per 32 MiB slice.
//!
//! This module owns the decision of *which* durable members become *one*
//! object. It answers three questions and nothing else:
//!
//! - **May these members combine?** Only when their complete
//!   [`ScribeAssemblyKey`] matches. The key carries the tenant, canonical
//!   table, schema fingerprint, physical layout, physical partition, node and
//!   writer epoch, so two members combine if and only if one merged file could
//!   have been written from either of them without changing a single durable
//!   fact. Nothing in this module can widen that comparison.
//! - **Which members, in which order?** The smallest complete prefix of the
//!   key's ready members whose encoded bytes reach the configured target. A
//!   member is never split across claims and never joins a claim after
//!   membership is fixed, so a claim's contents are a function of the ready
//!   index at the moment it is taken.
//! - **Whose turn is it?** One tenant per round, one key per tenant turn. A
//!   tenant whose key is still filling toward the target cannot hold the
//!   claim budget while another tenant's aged residue waits.
//!
//! Claim identity is derived, not allocated: [`StagingClaimId`] hashes the
//! assembly key together with the sorted member set and each member's exact
//! byte and row counts. A replay that rebuilds the same ready index takes the
//! same claim with the same identity, which is what lets publication recognize
//! its own prior work instead of writing a second object for the same rows.
//!
//! The assembler is IO-free and holds no durable state. Members reach it only
//! after their local run and manifest are fsynced and query-registered, and the
//! claim it returns is durable only once its owner writes and fsyncs the claim
//! membership. Both boundaries belong to the staging and publication owners
//! that call this type.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use chrono::{DateTime, Utc};
use sha2::{Digest as _, Sha256};
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::{NullOrderWire, SortDirectionWire};

use crate::catalog::TableRef;
use crate::catalog::layout::{PhysicalLayout, TimePartition};
use crate::schema::fingerprint::SchemaFingerprint;
use crate::scribe::stream_identity::{NodeId, WriterEpoch};

/// Exact compatibility scope shared by every member of one assembly claim.
///
/// Every field is a reason two members could produce different bytes or
/// different durable facts in a merged object: a different tenant or table is a
/// different owner, a different schema fingerprint or layout is a different
/// sort order and column set, a different partition is a different file-list
/// row, and a different node or writer epoch is a different LSN stream. The key
/// is compared whole; there is no shorter logical form and no field a caller
/// may omit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScribeAssemblyKey {
    /// Tenant owning every row in the claim.
    tenant: DataTenantId,
    /// Canonical table every member belongs to.
    table: TableRef,
    /// Fingerprint of the user schema the members were encoded against.
    schema_fingerprint: SchemaFingerprint,
    /// Digest of the canonical physical layout fixing sort and partitioning.
    layout_fingerprint: [u8; 32],
    /// Exact physical time partition represented by every member.
    partition: TimePartition,
    /// Node whose local staging volume holds the members.
    node_id: NodeId,
    /// Fenced writer epoch whose LSN stream produced the members.
    writer_epoch: WriterEpoch,
}

impl ScribeAssemblyKey {
    /// Builds the key from the canonical layout the table is registered with.
    ///
    /// The layout is reduced to a digest here rather than stored whole so the
    /// key stays cheap to hash and compare, and so two structurally identical
    /// layouts resolved by different code paths produce the same key.
    #[must_use]
    pub fn new(
        tenant: DataTenantId,
        table: TableRef,
        schema_fingerprint: SchemaFingerprint,
        layout: &PhysicalLayout,
        partition: TimePartition,
        node_id: NodeId,
        writer_epoch: WriterEpoch,
    ) -> Self {
        Self {
            tenant,
            table,
            schema_fingerprint,
            layout_fingerprint: layout_fingerprint(layout),
            partition,
            node_id,
            writer_epoch,
        }
    }

    /// Rebuilds a key from fields recovered from a durable staged record.
    ///
    /// Recovery has the layout digest the member was staged under, not the
    /// layout itself, and that digest is the fact that matters: a member may
    /// only join a claim whose sort order and partitioning are the ones it was
    /// written with, even if the table has since been re-registered.
    #[must_use]
    pub const fn from_parts(
        tenant: DataTenantId,
        table: TableRef,
        schema_fingerprint: SchemaFingerprint,
        layout_fingerprint: [u8; 32],
        partition: TimePartition,
        node_id: NodeId,
        writer_epoch: WriterEpoch,
    ) -> Self {
        Self {
            tenant,
            table,
            schema_fingerprint,
            layout_fingerprint,
            partition,
            node_id,
            writer_epoch,
        }
    }

    /// Returns the user-schema fingerprint the members were encoded against.
    #[must_use]
    pub const fn schema_fingerprint(&self) -> SchemaFingerprint {
        self.schema_fingerprint
    }

    /// Returns the digest of the canonical physical layout.
    #[must_use]
    pub const fn layout_fingerprint(&self) -> [u8; 32] {
        self.layout_fingerprint
    }

    /// Returns the node whose staging volume holds the members.
    #[must_use]
    pub const fn node_id(&self) -> NodeId {
        self.node_id
    }

    /// Returns the fenced writer epoch that produced the members.
    #[must_use]
    pub const fn writer_epoch(&self) -> WriterEpoch {
        self.writer_epoch
    }

    /// Returns a stable digest over every field of the key.
    ///
    /// Used as the staged namespace's directory component so no tenant- or
    /// user-controlled string ever becomes a path.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"wyrd.scribe.assembly-key.v1");
        self.hash_into(&mut hasher);
        hasher.finalize().into()
    }

    /// Returns the tenant that owns every member under this key.
    ///
    /// The claim scheduler needs it to rotate turns between tenants; it is not
    /// a substitute for comparing the whole key.
    #[must_use]
    pub const fn tenant(&self) -> DataTenantId {
        self.tenant
    }

    /// Returns the canonical table every member belongs to.
    #[must_use]
    pub const fn table(&self) -> &TableRef {
        &self.table
    }

    /// Returns the exact physical partition represented by the members.
    #[must_use]
    pub const fn partition(&self) -> TimePartition {
        self.partition
    }

    /// Feeds every field of the key into a claim-identity hash.
    ///
    /// Each field is length-prefixed or fixed-width so no two distinct keys can
    /// produce the same byte stream by concatenation.
    fn hash_into(&self, hasher: &mut Sha256) {
        hasher.update(self.tenant.as_uuid().as_bytes());
        let fqn = self.table.fqn();
        hasher.update((fqn.len() as u64).to_be_bytes());
        hasher.update(fqn.as_bytes());
        hasher.update(self.schema_fingerprint.0);
        hasher.update(self.layout_fingerprint);
        hasher.update([self.partition.granularity_tag()]);
        hasher.update(self.partition.start_unix_micros().to_be_bytes());
        hasher.update(self.node_id.as_bytes());
        hasher.update(self.writer_epoch.as_i64().to_be_bytes());
    }
}

/// Digests one canonical physical layout into a comparable fingerprint.
///
/// Hashes the wire projection, which is the same canonical form the catalog
/// stores and re-resolves, so a layout that round-trips through registration
/// keeps its fingerprint. Direction and null order are matched explicitly
/// rather than serialized so a rename in the public wire enum cannot silently
/// change a durable claim identity.
fn layout_fingerprint(layout: &PhysicalLayout) -> [u8; 32] {
    let wire = layout.to_wire();
    let mut hasher = Sha256::new();
    hasher.update([layout.granularity().tag()]);
    hasher.update((wire.sort_keys.len() as u64).to_be_bytes());
    for key in &wire.sort_keys {
        hasher.update((key.column.len() as u64).to_be_bytes());
        hasher.update(key.column.as_bytes());
        hasher.update([match key.direction {
            SortDirectionWire::Asc => 1,
            SortDirectionWire::Desc => 2,
        }]);
        hasher.update([match key.null_order {
            NullOrderWire::First => 1,
            NullOrderWire::Last => 2,
        }]);
    }
    hasher.update((wire.bloom_columns.len() as u64).to_be_bytes());
    for column in &wire.bloom_columns {
        hasher.update((column.len() as u64).to_be_bytes());
        hasher.update(column.as_bytes());
    }
    hasher.finalize().into()
}

/// Immutable identity of one staged member within its assembly key.
///
/// A member is exactly one frozen `SealKey` bucket of one shard generation, so
/// the producing shard and that shard's generation ordinal identify it
/// completely once the assembly key fixes tenant, table and partition. The
/// ordering is the tie-break used after persisted ready time, which makes the
/// ready index total and therefore replayable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StagedMemberId {
    /// Pod-local shard lane that froze the member.
    shard: u16,
    /// Monotonic generation ordinal of that shard.
    generation: u64,
}

impl StagedMemberId {
    /// Builds a member identity from its producing shard and generation.
    #[must_use]
    pub const fn new(shard: u16, generation: u64) -> Self {
        Self { shard, generation }
    }

    /// Returns the pod-local shard lane that froze the member.
    #[must_use]
    pub const fn shard(self) -> u16 {
        self.shard
    }

    /// Returns the shard generation ordinal that produced the member.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// One durable local member offered to the assembler.
///
/// A member reaches the ready index only after its sorted run and manifest are
/// fsynced, checksum-validated and query-registered, so every field here is a
/// fact already recorded on disk rather than an estimate the assembler is free
/// to revise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadyMember {
    /// Immutable identity of the member within its key.
    id: StagedMemberId,
    /// Encoded bytes the member's runs occupy on the staging volume.
    encoded_bytes: u64,
    /// Exact rows the member carries.
    rows: u64,
    /// Instant the member became durable and queryable.
    ready_at: DateTime<Utc>,
}

impl ReadyMember {
    /// Records one durable member's exact staging facts.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::EmptyMember`] when the member reports zero
    /// encoded bytes or zero rows. Such a member cannot exist — a bucket is
    /// frozen only when it is nonempty — and admitting it would let a claim
    /// grow without ever approaching its target.
    pub fn new(
        id: StagedMemberId,
        encoded_bytes: u64,
        rows: u64,
        ready_at: DateTime<Utc>,
    ) -> Result<Self, AssemblyError> {
        if encoded_bytes == 0 || rows == 0 {
            return Err(AssemblyError::EmptyMember {
                shard: id.shard(),
                generation: id.generation(),
            });
        }
        Ok(Self {
            id,
            encoded_bytes,
            rows,
            ready_at,
        })
    }

    /// Returns the member's immutable identity.
    #[must_use]
    pub const fn id(self) -> StagedMemberId {
        self.id
    }

    /// Returns the encoded staging bytes the member occupies.
    #[must_use]
    pub const fn encoded_bytes(self) -> u64 {
        self.encoded_bytes
    }

    /// Returns the exact rows the member carries.
    #[must_use]
    pub const fn rows(self) -> u64 {
        self.rows
    }

    /// Returns the instant the member became durable and queryable.
    #[must_use]
    pub const fn ready_at(self) -> DateTime<Utc> {
        self.ready_at
    }

    /// Returns the total order position used by the ready index.
    ///
    /// Persisted ready time first so a claim drains the oldest rows, then the
    /// immutable identity so two members persisted in the same instant still
    /// order identically on every replay.
    fn order(self) -> (DateTime<Utc>, StagedMemberId) {
        (self.ready_at, self.id)
    }
}

/// Deterministic identity of one assembly claim.
///
/// Derived from the assembly key and the exact sorted member set, never
/// allocated. Two runs that observe the same ready members take the same claim
/// identity, so a publication that restarts mid-flight recognizes its own
/// artifacts instead of writing a second copy of the same rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StagingClaimId([u8; 32]);

impl StagingClaimId {
    /// Derives the identity of a claim over `members` under `key`.
    ///
    /// Members are hashed in ready order with their byte and row counts, so a
    /// claim over the same members in a different order, or over the same
    /// members with different contents, is a different claim.
    fn derive(key: &ScribeAssemblyKey, members: &[ReadyMember]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"wyrd.scribe.staging-claim.v1");
        key.hash_into(&mut hasher);
        hasher.update((members.len() as u64).to_be_bytes());
        for member in members {
            hasher.update(member.id.shard().to_be_bytes());
            hasher.update(member.id.generation().to_be_bytes());
            hasher.update(member.encoded_bytes.to_be_bytes());
            hasher.update(member.rows.to_be_bytes());
        }
        Self(hasher.finalize().into())
    }

    /// Returns the raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Parses the identity back from the lowercase hex a durable record holds.
    ///
    /// Recovery reads claim ownership out of staged records, so the rendered
    /// form has to round-trip exactly; anything else is a corrupt record rather
    /// than a claim this pod may resume.
    #[must_use]
    pub fn from_hex(value: &str) -> Option<Self> {
        if value.len() != 64 {
            return None;
        }
        let mut bytes = [0_u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let start = index * 2;
            *byte = u8::from_str_radix(value.get(start..start + 2)?, 16).ok()?;
        }
        Some(Self(bytes))
    }
}

impl std::fmt::Display for StagingClaimId {
    /// Renders the identity as lowercase hex for paths and durable manifests.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Why a claim was taken when it was.
///
/// The vocabulary is closed and fixed-cardinality: it is the metric label and
/// the close reason recorded in the artifact's durable evidence. `Target` means
/// the claim reached the configured object size; every other cause means the
/// claim is a deliberate residue that is smaller than target because waiting
/// longer would cost more than the smaller object does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimCause {
    /// The ready prefix reached the configured encoded target.
    Target,
    /// The oldest ready member exceeded the configured maximum dwell.
    Dwell,
    /// The key's physical partition closed and will receive no more members.
    PartitionClosed,
    /// Staging or scratch pressure required releasing durable bytes early.
    Pressure,
    /// Graceful drain is settling every admitted member before shutdown.
    Drain,
}

impl ClaimCause {
    /// Returns the fixed-cardinality label naming this cause.
    ///
    /// Safe as a metric label: it carries no tenant, table, partition or path.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::Dwell => "dwell",
            Self::PartitionClosed => "partition_closed",
            Self::Pressure => "pressure",
            Self::Drain => "drain",
        }
    }

    /// Reports whether the claim closed at its configured target size.
    ///
    /// Everything else is a residue, and residues are the amplification the
    /// target exists to bound — worth counting separately.
    #[must_use]
    pub const fn is_target(self) -> bool {
        matches!(self, Self::Target)
    }
}

/// One immutable set of members chosen to become one hot object set.
///
/// Membership is fixed at construction. A member persisted after the claim was
/// taken belongs to the next claim, which is what keeps claim identity a
/// function of its contents rather than of when the merge happened to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagingClaim {
    /// Derived identity of this claim.
    id: StagingClaimId,
    /// Exact compatibility scope every member shares.
    key: ScribeAssemblyKey,
    /// Members in ready order.
    members: Vec<ReadyMember>,
    /// Summed encoded staging bytes across the members.
    encoded_bytes: u64,
    /// Summed rows across the members.
    rows: u64,
    /// Why the claim was taken when it was.
    cause: ClaimCause,
}

impl StagingClaim {
    /// Returns the claim's derived identity.
    #[must_use]
    pub const fn id(&self) -> StagingClaimId {
        self.id
    }

    /// Returns the compatibility scope shared by every member.
    #[must_use]
    pub const fn key(&self) -> &ScribeAssemblyKey {
        &self.key
    }

    /// Returns the claim's members in ready order.
    #[must_use]
    pub fn members(&self) -> &[ReadyMember] {
        &self.members
    }

    /// Returns the summed encoded staging bytes the merge will read.
    #[must_use]
    pub const fn encoded_bytes(&self) -> u64 {
        self.encoded_bytes
    }

    /// Returns the summed rows the claim will write.
    #[must_use]
    pub const fn rows(&self) -> u64 {
        self.rows
    }

    /// Returns why the claim was taken when it was.
    #[must_use]
    pub const fn cause(&self) -> ClaimCause {
        self.cause
    }
}

/// Why the assembler refused a registration or a claim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssemblyError {
    /// A staged member reported no bytes or no rows.
    #[error(
        "Scribe staged member from shard {shard} generation {generation} carries no encoded bytes or rows"
    )]
    EmptyMember {
        /// Shard that produced the member.
        shard: u16,
        /// Generation ordinal of that shard.
        generation: u64,
    },
    /// A member identity was offered to one key twice.
    #[error(
        "Scribe staged member from shard {shard} generation {generation} is already owned by its assembly key"
    )]
    DuplicateMember {
        /// Shard that produced the member.
        shard: u16,
        /// Generation ordinal of that shard.
        generation: u64,
    },
    /// Every configured claim slot is already held by an unsettled claim.
    #[error(
        "Scribe staging cannot open another assembly claim: {budget} claim slots are all outstanding"
    )]
    ClaimBudgetExhausted {
        /// Configured simultaneous-claim ceiling.
        budget: usize,
    },
    /// A settlement named a claim the assembler does not hold.
    #[error("Scribe staging claim {claim} is not outstanding")]
    UnknownClaim {
        /// Identity the caller named.
        claim: String,
    },
    /// A configured control was zero or otherwise unusable.
    #[error("Scribe staging assembler control `{control}` must be greater than zero")]
    Zero {
        /// Name of the offending control.
        control: &'static str,
    },
}

/// Validated controls governing when and how many claims are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StagingAssemblerConfig {
    /// Encoded bytes a claim must reach to close at target.
    target_file_size_bytes: u64,
    /// Longest a ready member may wait before it is released as residue.
    max_dwell: Duration,
    /// Claims that may be outstanding at once across every tenant.
    claim_items_budget: usize,
}

impl StagingAssemblerConfig {
    /// Validates one complete set of assembler controls.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::Zero`] when the target, the dwell, or the claim
    /// budget is zero. A zero target would close every claim on its first
    /// member, a zero dwell would close every claim as residue before it could
    /// ever fill, and a zero budget would make the assembler unable to hand out
    /// any work at all.
    pub fn new(
        target_file_size_bytes: u64,
        max_dwell: Duration,
        claim_items_budget: usize,
    ) -> Result<Self, AssemblyError> {
        if target_file_size_bytes == 0 {
            return Err(AssemblyError::Zero {
                control: "staging_target_file_size_bytes",
            });
        }
        if max_dwell.is_zero() {
            return Err(AssemblyError::Zero {
                control: "staging_max_dwell",
            });
        }
        if claim_items_budget == 0 {
            return Err(AssemblyError::Zero {
                control: "staging_claim_items_budget",
            });
        }
        Ok(Self {
            target_file_size_bytes,
            max_dwell,
            claim_items_budget,
        })
    }

    /// Returns the encoded bytes a claim must reach to close at target.
    #[must_use]
    pub const fn target_file_size_bytes(self) -> u64 {
        self.target_file_size_bytes
    }

    /// Returns the longest a ready member may wait before residue release.
    #[must_use]
    pub const fn max_dwell(self) -> Duration {
        self.max_dwell
    }

    /// Returns how many claims may be outstanding at once.
    #[must_use]
    pub const fn claim_items_budget(self) -> usize {
        self.claim_items_budget
    }
}

/// Ready members of one assembly key, in claim order.
#[derive(Debug, Default)]
struct KeyReadyIndex {
    /// Members ordered by persisted ready time then immutable identity.
    members: Vec<ReadyMember>,
}

impl KeyReadyIndex {
    /// Inserts one member at its ordered position.
    fn insert(&mut self, member: ReadyMember) {
        let position = self
            .members
            .partition_point(|existing| existing.order() < member.order());
        self.members.insert(position, member);
    }

    /// Returns the smallest complete prefix whose bytes reach `target`.
    ///
    /// Returns `None` while the key is still filling: a shorter prefix would
    /// publish a smaller object than configured, which is the amplification the
    /// target exists to prevent. Only a residue cause may take a short prefix.
    fn target_prefix(&self, target: u64) -> Option<usize> {
        let mut summed: u64 = 0;
        for (index, member) in self.members.iter().enumerate() {
            summed = summed.saturating_add(member.encoded_bytes());
            if summed >= target {
                return Some(index + 1);
            }
        }
        None
    }

    /// Reports whether the oldest ready member has waited longer than `dwell`.
    fn dwell_expired(&self, now: DateTime<Utc>, dwell: Duration) -> bool {
        let Some(oldest) = self.members.first() else {
            return false;
        };
        let Ok(dwell) = chrono::Duration::from_std(dwell) else {
            return false;
        };
        now.signed_duration_since(oldest.ready_at()) >= dwell
    }
}

/// One staged member as recovery found it.
///
/// Restart has to restore two different things: members that are free to be
/// claimed, and members a claim already owns. Restoring the second kind as if
/// it were the first would let a restarted pod take a second claim over rows an
/// interrupted publication may already have written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveredMember {
    /// Durable, validated, and free to join a new claim.
    Ready(ReadyMember),
    /// Already owned by a claim whose publication has not settled.
    Claimed {
        /// Identity of the claim that owns the member.
        claim: StagingClaimId,
        /// The member itself, kept for its bytes, rows and ready time.
        member: ReadyMember,
    },
}

/// Tenant-fair owner of the durable ready index and its assembly claims.
///
/// Holds every member that is durable but not yet published, decides which
/// members become one object, and bounds how many of those merges may be in
/// flight. It performs no IO and owns no files: registration means a run and
/// manifest are already durable, and a returned claim becomes durable only when
/// its caller fsyncs the membership.
#[derive(Debug)]
pub struct StagingAssembler {
    /// Validated controls governing target, dwell and claim concurrency.
    config: StagingAssemblerConfig,
    /// Ready members per assembly key.
    ready: HashMap<ScribeAssemblyKey, KeyReadyIndex>,
    /// Tenants with at least one key holding ready members, in turn order.
    tenant_order: VecDeque<DataTenantId>,
    /// Keys per tenant, in turn order within that tenant.
    keys_by_tenant: HashMap<DataTenantId, VecDeque<ScribeAssemblyKey>>,
    /// Members currently owned by an outstanding claim, by claim identity.
    outstanding: HashMap<StagingClaimId, Vec<(ScribeAssemblyKey, StagedMemberId)>>,
    /// Every member the assembler currently owns, ready or claimed.
    owned: HashSet<(ScribeAssemblyKey, StagedMemberId)>,
}

impl StagingAssembler {
    /// Builds an empty assembler over validated controls.
    #[must_use]
    pub fn new(config: StagingAssemblerConfig) -> Self {
        Self {
            config,
            ready: HashMap::new(),
            tenant_order: VecDeque::new(),
            keys_by_tenant: HashMap::new(),
            outstanding: HashMap::new(),
            owned: HashSet::new(),
        }
    }

    /// Registers one durable member as available for assembly.
    ///
    /// The member joins its key's ready index at its ordered position, and its
    /// tenant and key enter the turn queues on the empty-to-nonempty transition
    /// only, so a key that has been waiting does not lose its place because a
    /// new member arrived for it.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::DuplicateMember`] when the assembler already
    /// owns that member under that key, either in the ready index or inside an
    /// outstanding claim. Registering a member twice would publish its rows
    /// twice.
    pub fn register_ready(
        &mut self,
        key: &ScribeAssemblyKey,
        member: ReadyMember,
    ) -> Result<(), AssemblyError> {
        let ownership = (key.clone(), member.id());
        if self.owned.contains(&ownership) {
            return Err(AssemblyError::DuplicateMember {
                shard: member.id().shard(),
                generation: member.id().generation(),
            });
        }
        let tenant = key.tenant();
        let keys = self.keys_by_tenant.entry(tenant).or_default();
        if !keys.contains(key) {
            keys.push_back(key.clone());
        }
        if !self.tenant_order.contains(&tenant) {
            self.tenant_order.push_back(tenant);
        }
        self.ready.entry(key.clone()).or_default().insert(member);
        self.owned.insert(ownership);
        Ok(())
    }

    /// Rebuilds one key's ownership from what recovery found on the volume.
    ///
    /// Ready members re-enter the ready index in their persisted order. Members
    /// an unsettled claim owns are restored as that claim, so the claim keeps
    /// its slot, its members cannot be claimed again, and the publication that
    /// was interrupted resumes under the identity it already wrote.
    ///
    /// Restoring is deliberately not bounded by the claim budget: those claims
    /// are already durable, and refusing them would strand their members. A
    /// budget that recovery overshoots simply admits no new claim until enough
    /// of the restored ones settle.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::DuplicateMember`] when the same member is
    /// restored twice, which would mean two durable records claim the same
    /// rows.
    pub fn restore(
        &mut self,
        key: &ScribeAssemblyKey,
        members: impl IntoIterator<Item = RecoveredMember>,
    ) -> Result<(), AssemblyError> {
        for recovered in members {
            match recovered {
                RecoveredMember::Ready(member) => self.register_ready(key, member)?,
                RecoveredMember::Claimed { claim, member } => {
                    let ownership = (key.clone(), member.id());
                    if self.owned.contains(&ownership) {
                        return Err(AssemblyError::DuplicateMember {
                            shard: member.id().shard(),
                            generation: member.id().generation(),
                        });
                    }
                    self.owned.insert(ownership.clone());
                    self.outstanding.entry(claim).or_default().push(ownership);
                }
            }
        }
        Ok(())
    }

    /// Takes the next claim any tenant is entitled to, if one is due.
    ///
    /// One tenant is served per call and then moves to the tail of the tenant
    /// order, and within that tenant one key is served and moves to the tail of
    /// its key order. A key that is still filling toward target and has not
    /// exceeded its dwell yields its turn rather than publishing a small
    /// object, so a stalled key costs its own turn and nothing else.
    ///
    /// Returns `None` when no key is due, which is the normal steady state for
    /// keys that are still accumulating.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::ClaimBudgetExhausted`] when a key is due but
    /// every configured claim slot is already held. The caller settles a claim
    /// to make progress; the ready index and the turn order are untouched. A
    /// full budget with nothing due is not an error — there is no work being
    /// refused.
    pub fn next_claim(
        &mut self,
        now: DateTime<Utc>,
    ) -> Result<Option<StagingClaim>, AssemblyError> {
        for _ in 0..self.tenant_order.len() {
            let Some(tenant) = self.tenant_order.pop_front() else {
                break;
            };
            if let Some((key, cause, take)) = self.due_key_for(tenant, now) {
                if let Err(error) = self.check_claim_budget() {
                    self.tenant_order.push_front(tenant);
                    return Err(error);
                }
                let claim = self.take_claim(&key, cause, take);
                self.rotate_key(tenant, &key);
                self.tenant_order.push_back(tenant);
                self.prune_tenant(tenant);
                return Ok(Some(claim));
            }
            self.tenant_order.push_back(tenant);
        }
        Ok(None)
    }

    /// Claims every currently ready member of one key for a residue cause.
    ///
    /// This is the entry point for the decisions the assembler cannot see for
    /// itself: the partition closed, staging is under pressure, or the pod is
    /// draining. Members that become ready afterwards belong to a later claim.
    ///
    /// Returns `None` when the key holds no ready members.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::ClaimBudgetExhausted`] when the key holds ready
    /// members but every configured claim slot is already held. A key with
    /// nothing ready returns `None` rather than reporting pressure.
    pub fn claim_residue(
        &mut self,
        key: &ScribeAssemblyKey,
        cause: ClaimCause,
    ) -> Result<Option<StagingClaim>, AssemblyError> {
        let ready = self.ready.get(key).map_or(0, |index| index.members.len());
        if ready == 0 {
            return Ok(None);
        }
        self.check_claim_budget()?;
        let claim = self.take_claim(key, cause, ready);
        let tenant = key.tenant();
        self.rotate_key(tenant, key);
        self.prune_tenant(tenant);
        Ok(Some(claim))
    }

    /// Settles one outstanding claim and returns its claim slot to the budget.
    ///
    /// Called after the claim's objects are published and its contributing
    /// members are retired. The members leave the assembler's ownership here,
    /// which is also what allows a shard to reuse those identities after a
    /// restart replays the same generations.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::UnknownClaim`] when the identity is not
    /// outstanding, so a double settlement cannot silently free a slot twice.
    pub fn settle_claim(&mut self, claim: StagingClaimId) -> Result<(), AssemblyError> {
        let members =
            self.outstanding
                .remove(&claim)
                .ok_or_else(|| AssemblyError::UnknownClaim {
                    claim: claim.to_string(),
                })?;
        for ownership in members {
            self.owned.remove(&ownership);
        }
        Ok(())
    }

    /// Returns how many claims are currently outstanding.
    #[must_use]
    pub fn outstanding_claims(&self) -> usize {
        self.outstanding.len()
    }

    /// Returns the ready members currently held for one key, in claim order.
    #[must_use]
    pub fn ready_members(&self, key: &ScribeAssemblyKey) -> &[ReadyMember] {
        self.ready
            .get(key)
            .map_or(&[], |index| index.members.as_slice())
    }

    /// Returns every key that currently holds at least one ready member.
    ///
    /// Drain and partition-close use this to sweep what target and dwell would
    /// otherwise keep waiting: a member that is durable but unpublished costs
    /// staging capacity, so at shutdown every remaining key is claimed as
    /// residue rather than left for a dwell that will never expire.
    #[must_use]
    pub fn ready_keys(&self) -> Vec<ScribeAssemblyKey> {
        self.ready
            .iter()
            .filter(|(_, index)| !index.members.is_empty())
            .map(|(key, _)| key.clone())
            .collect()
    }

    /// Refuses when every configured claim slot is already outstanding.
    ///
    /// # Errors
    ///
    /// Returns [`AssemblyError::ClaimBudgetExhausted`] at the ceiling.
    fn check_claim_budget(&self) -> Result<(), AssemblyError> {
        if self.outstanding.len() >= self.config.claim_items_budget() {
            return Err(AssemblyError::ClaimBudgetExhausted {
                budget: self.config.claim_items_budget(),
            });
        }
        Ok(())
    }

    /// Finds the first key of `tenant` that is due, rotating past keys that are not.
    ///
    /// Returns the key, why it is due, and how many of its ready members the
    /// claim takes.
    fn due_key_for(
        &mut self,
        tenant: DataTenantId,
        now: DateTime<Utc>,
    ) -> Option<(ScribeAssemblyKey, ClaimCause, usize)> {
        let keys = self.keys_by_tenant.get(&tenant)?;
        for key in keys.clone() {
            let Some(index) = self.ready.get(&key) else {
                continue;
            };
            if let Some(take) = index.target_prefix(self.config.target_file_size_bytes()) {
                return Some((key, ClaimCause::Target, take));
            }
            if index.dwell_expired(now, self.config.max_dwell()) {
                return Some((key, ClaimCause::Dwell, index.members.len()));
            }
        }
        None
    }

    /// Removes `take` ready members from `key` and records the resulting claim.
    ///
    /// The prefix is removed rather than marked so no later claim can see it,
    /// and the members stay in `owned` until the claim settles so a replayed
    /// registration of the same member is still refused while it is in flight.
    fn take_claim(
        &mut self,
        key: &ScribeAssemblyKey,
        cause: ClaimCause,
        take: usize,
    ) -> StagingClaim {
        let members: Vec<ReadyMember> = {
            let index = self
                .ready
                .get_mut(key)
                .expect("claimed key holds a ready index");
            index.members.drain(..take).collect()
        };
        if self
            .ready
            .get(key)
            .is_some_and(|index| index.members.is_empty())
        {
            self.ready.remove(key);
        }
        let encoded_bytes = members.iter().fold(0_u64, |total, member| {
            total.saturating_add(member.encoded_bytes())
        });
        let rows = members
            .iter()
            .fold(0_u64, |total, member| total.saturating_add(member.rows()));
        let id = StagingClaimId::derive(key, &members);
        self.outstanding.insert(
            id,
            members
                .iter()
                .map(|member| (key.clone(), member.id()))
                .collect(),
        );
        StagingClaim {
            id,
            key: key.clone(),
            members,
            encoded_bytes,
            rows,
            cause,
        }
    }

    /// Moves a served key to the tail of its tenant's order, dropping it when empty.
    fn rotate_key(&mut self, tenant: DataTenantId, key: &ScribeAssemblyKey) {
        let Some(keys) = self.keys_by_tenant.get_mut(&tenant) else {
            return;
        };
        keys.retain(|queued| queued != key);
        if self.ready.contains_key(key) {
            keys.push_back(key.clone());
        }
    }

    /// Drops a tenant from the turn order once it holds no keys with ready members.
    fn prune_tenant(&mut self, tenant: DataTenantId) {
        let empty = self
            .keys_by_tenant
            .get(&tenant)
            .is_none_or(VecDeque::is_empty);
        if empty {
            self.keys_by_tenant.remove(&tenant);
            self.tenant_order.retain(|queued| *queued != tenant);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::layout::TimeGranularity;
    use crate::namespaces::BifrostNamespace;
    use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
    use wyrd_spec::vala::api::{PhysicalLayoutWire, TimeGranularityWire};

    /// Builds the physical schema every fixture layout resolves against.
    fn fixture_schema() -> Schema {
        Schema::new(vec![Field::new(
            "wyrd_event_time",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )])
    }

    /// Resolves one canonical layout, optionally with a declared granularity.
    ///
    /// # Panics
    ///
    /// Panics when the fixture declaration does not resolve, which would be a
    /// fixture bug rather than assembler behavior.
    fn layout(granularity: Option<TimeGranularityWire>) -> PhysicalLayout {
        let schema = fixture_schema();
        let declared = granularity.map(|partition_granularity| PhysicalLayoutWire {
            partition_granularity,
            sort_keys: Vec::new(),
            bloom_columns: Vec::new(),
        });
        PhysicalLayout::resolve("vala.bifrost.events", &schema, declared.as_ref())
            .expect("fixture layout resolves")
    }

    /// Builds one exact hourly partition from a fixed boundary offset.
    ///
    /// # Panics
    ///
    /// Panics when the offset is not an hour boundary.
    fn partition(hours: i64) -> TimePartition {
        let start = chrono::DateTime::from_timestamp(1_772_150_400 + hours * 3_600, 0)
            .expect("a fixed representable instant");
        TimePartition::new(TimeGranularity::Hour, start).expect("a fixed hour boundary")
    }

    /// Builds one assembly key over the supplied tenant and partition.
    fn key_for(tenant: DataTenantId, hours: i64) -> ScribeAssemblyKey {
        ScribeAssemblyKey::new(
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            SchemaFingerprint([7; 32]),
            &layout(None),
            partition(hours),
            NodeId::new(uuid::Uuid::from_u128(9)),
            WriterEpoch::new(4),
        )
    }

    /// Builds one ready member at a fixed offset from the fixture epoch.
    ///
    /// # Panics
    ///
    /// Panics when the member facts are empty, which would be a fixture bug.
    fn member(shard: u16, generation: u64, bytes: u64, ready_seconds: i64) -> ReadyMember {
        let ready_at = chrono::DateTime::from_timestamp(1_772_150_400 + ready_seconds, 0)
            .expect("a fixed representable instant");
        ReadyMember::new(
            StagedMemberId::new(shard, generation),
            bytes,
            bytes / 10,
            ready_at,
        )
        .expect("a nonempty fixture member")
    }

    /// Returns the fixture instant `seconds` after the fixture epoch.
    ///
    /// # Panics
    ///
    /// Panics when the offset is not representable.
    fn at(seconds: i64) -> DateTime<Utc> {
        chrono::DateTime::from_timestamp(1_772_150_400 + seconds, 0)
            .expect("a fixed representable instant")
    }

    /// Builds an assembler with the supplied target and a ten-minute dwell.
    ///
    /// # Panics
    ///
    /// Panics when the fixture controls do not validate.
    fn assembler(target: u64, budget: usize) -> StagingAssembler {
        StagingAssembler::new(
            StagingAssemblerConfig::new(target, Duration::from_mins(10), budget)
                .expect("fixture controls validate"),
        )
    }

    /// A claim's identity is a function of its members, not of how they arrived.
    ///
    /// Batch routing spreads one key over many shards and rotations, so the
    /// same four members can be persisted, observed, and replayed in any order.
    /// If identity depended on that order, a restart mid-publication would take
    /// a "new" claim over rows an object already holds and publish them twice.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn staging_claim_identity_is_cross_generation_deterministic() {
        let tenant = DataTenantId::new_v7();
        let key = key_for(tenant, 0);
        let members = [
            member(3, 7, 400, 10),
            member(11, 7, 400, 20),
            member(3, 9, 400, 30),
        ];

        let mut forward = assembler(1_000, 4);
        for entry in members {
            forward
                .register_ready(&key, entry)
                .expect("a fresh member registers");
        }
        let first = forward
            .next_claim(at(40))
            .expect("claim budget is free")
            .expect("the prefix reaches target");

        // The same members persisted in the reverse order — a different shard
        // finishing first after a restart — produce the same claim.
        let mut reversed = assembler(1_000, 4);
        for entry in members.iter().rev() {
            reversed
                .register_ready(&key, *entry)
                .expect("a fresh member registers");
        }
        let replayed = reversed
            .next_claim(at(40))
            .expect("claim budget is free")
            .expect("the prefix reaches target");

        assert_eq!(first.id(), replayed.id());
        assert_eq!(first.members(), replayed.members());
        assert_eq!(first.encoded_bytes(), 1_200);
        assert!(first.cause().is_target());

        // A member with different contents is a different claim, so a partial
        // re-encode can never be mistaken for the published one.
        let mut altered = assembler(1_000, 4);
        for entry in [member(3, 7, 400, 10), member(11, 7, 401, 20), members[2]] {
            altered
                .register_ready(&key, entry)
                .expect("a fresh member registers");
        }
        let different = altered
            .next_claim(at(40))
            .expect("claim budget is free")
            .expect("the prefix reaches target");
        assert_ne!(first.id(), different.id());

        // An incompatible key never contributes, even for the same tenant and
        // table: a different partition is a different file-list row.
        let other_partition = key_for(tenant, 1);
        let mut mixed = assembler(1_000, 4);
        for entry in members {
            mixed
                .register_ready(&key, entry)
                .expect("a fresh member registers");
        }
        mixed
            .register_ready(&other_partition, member(3, 7, 900, 5))
            .expect("the same member identity is free under another key");
        let claimed = mixed
            .next_claim(at(40))
            .expect("claim budget is free")
            .expect("one of the two keys is due");
        assert_eq!(claimed.key(), &key);
        assert_eq!(claimed.id(), first.id());
    }

    /// Target claims take the smallest sufficient prefix; the rest waits, then leaves.
    ///
    /// This is the whole point of the staging target: publish a full-sized
    /// object when there is one to publish, and never let the leftover sit on
    /// the staging volume forever waiting for a partner that may not come.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn staging_assembler_merges_rolls_and_releases_residue() {
        let tenant = DataTenantId::new_v7();
        let key = key_for(tenant, 0);
        let mut assembler = assembler(1_000, 4);
        for entry in [
            member(1, 1, 600, 0),
            member(2, 1, 600, 5),
            member(3, 1, 300, 10),
        ] {
            assembler
                .register_ready(&key, entry)
                .expect("a fresh member registers");
        }

        let target = assembler
            .next_claim(at(20))
            .expect("claim budget is free")
            .expect("two members reach the target");
        assert_eq!(target.cause(), ClaimCause::Target);
        assert_eq!(target.members().len(), 2);
        assert_eq!(target.encoded_bytes(), 1_200);
        assert_eq!(
            target
                .members()
                .iter()
                .map(|member| member.id())
                .collect::<Vec<_>>(),
            vec![StagedMemberId::new(1, 1), StagedMemberId::new(2, 1)]
        );

        // The 300-byte remainder is below target, so nothing is due yet.
        assert_eq!(assembler.ready_members(&key).len(), 1);
        assert!(
            assembler
                .next_claim(at(30))
                .expect("claim budget is free")
                .is_none()
        );

        // Once it has waited out its dwell it is released as residue rather
        // than held for a partner.
        let residue = assembler
            .next_claim(at(620))
            .expect("claim budget is free")
            .expect("the aged remainder is due");
        assert_eq!(residue.cause(), ClaimCause::Dwell);
        assert_eq!(residue.encoded_bytes(), 300);
        assert!(!residue.cause().is_target());
        assert!(assembler.ready_members(&key).is_empty());
        assert_ne!(residue.id(), target.id());
    }

    /// One tenant filling toward target never costs another tenant its turn.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn a_stalled_key_yields_its_turn_to_another_tenant() {
        let stalled = DataTenantId::new_v7();
        let ready = DataTenantId::new_v7();
        let stalled_key = key_for(stalled, 0);
        let ready_key = key_for(ready, 0);
        let mut assembler = assembler(1_000, 4);
        assembler
            .register_ready(&stalled_key, member(1, 1, 100, 0))
            .expect("a fresh member registers");
        assembler
            .register_ready(&ready_key, member(1, 1, 1_000, 1))
            .expect("a fresh member registers");

        let claim = assembler
            .next_claim(at(2))
            .expect("claim budget is free")
            .expect("the second tenant is at target");
        assert_eq!(claim.key().tenant(), ready);
        assert_eq!(assembler.ready_members(&stalled_key).len(), 1);
    }

    /// Serving one tenant moves it behind every other tenant with work.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn claims_rotate_between_tenants_with_work() {
        let first = DataTenantId::new_v7();
        let second = DataTenantId::new_v7();
        let mut assembler = assembler(1_000, 8);
        for (tenant, generation) in [(first, 1), (second, 1), (first, 2), (second, 2)] {
            let key = key_for(tenant, 0);
            assembler
                .register_ready(&key, member(1, generation, 1_000, generation.cast_signed()))
                .expect("a fresh member registers");
        }

        let served: Vec<DataTenantId> = (0..4)
            .map(|round| {
                assembler
                    .next_claim(at(10 + round))
                    .expect("claim budget is free")
                    .expect("both tenants hold target-sized members")
                    .key()
                    .tenant()
            })
            .collect();
        assert_eq!(served, vec![first, second, first, second]);
    }

    /// A member is never admitted twice, in the ready index or inside a claim.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn a_member_identity_is_admitted_once_per_key() {
        let key = key_for(DataTenantId::new_v7(), 0);
        let mut assembler = assembler(1_000, 4);
        assembler
            .register_ready(&key, member(5, 2, 1_000, 0))
            .expect("a fresh member registers");
        let duplicate = assembler
            .register_ready(&key, member(5, 2, 1_000, 3))
            .expect_err("the identity is already owned");
        assert_eq!(
            duplicate,
            AssemblyError::DuplicateMember {
                shard: 5,
                generation: 2
            }
        );

        let claim = assembler
            .next_claim(at(5))
            .expect("claim budget is free")
            .expect("the member reaches target");
        // Still refused while the claim is in flight: its rows are already
        // being published.
        assembler
            .register_ready(&key, member(5, 2, 1_000, 9))
            .expect_err("a claimed identity stays owned");

        assembler
            .settle_claim(claim.id())
            .expect("the claim is outstanding");
        assembler
            .register_ready(&key, member(5, 2, 1_000, 9))
            .expect("a settled identity is free again");
        assert_eq!(
            assembler
                .settle_claim(claim.id())
                .expect_err("a settled claim cannot settle twice"),
            AssemblyError::UnknownClaim {
                claim: claim.id().to_string()
            }
        );
    }

    /// Outstanding claims are bounded, and settling one returns its slot.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration or claim is refused.
    #[test]
    fn outstanding_claims_stay_within_the_configured_budget() {
        let key = key_for(DataTenantId::new_v7(), 0);
        let mut assembler = assembler(1_000, 1);
        for generation in 1_u64..=2 {
            assembler
                .register_ready(&key, member(1, generation, 1_000, generation.cast_signed()))
                .expect("a fresh member registers");
        }

        let held = assembler
            .next_claim(at(5))
            .expect("claim budget is free")
            .expect("the first member reaches target");
        assert_eq!(assembler.outstanding_claims(), 1);
        assert_eq!(
            assembler
                .next_claim(at(6))
                .expect_err("the single claim slot is held"),
            AssemblyError::ClaimBudgetExhausted { budget: 1 }
        );
        assert_eq!(
            assembler
                .claim_residue(&key, ClaimCause::Drain)
                .expect_err("residue claims share the same budget"),
            AssemblyError::ClaimBudgetExhausted { budget: 1 }
        );

        assembler
            .settle_claim(held.id())
            .expect("the claim is outstanding");
        assert_eq!(assembler.outstanding_claims(), 0);
        let drained = assembler
            .claim_residue(&key, ClaimCause::Drain)
            .expect("the slot is free again")
            .expect("the remaining member is ready");
        assert_eq!(drained.cause(), ClaimCause::Drain);
        assert!(
            assembler
                .claim_residue(&key, ClaimCause::Drain)
                .expect("the budget allows another claim")
                .is_none()
        );
    }

    /// Every assembler control is validated, and a zero control fails closed.
    #[test]
    fn zero_valued_assembler_controls_are_refused() {
        assert_eq!(
            StagingAssemblerConfig::new(0, Duration::from_mins(10), 4)
                .expect_err("a zero target is refused"),
            AssemblyError::Zero {
                control: "staging_target_file_size_bytes"
            }
        );
        assert_eq!(
            StagingAssemblerConfig::new(1_000, Duration::ZERO, 4)
                .expect_err("a zero dwell is refused"),
            AssemblyError::Zero {
                control: "staging_max_dwell"
            }
        );
        assert_eq!(
            StagingAssemblerConfig::new(1_000, Duration::from_mins(10), 0)
                .expect_err("a zero claim budget is refused"),
            AssemblyError::Zero {
                control: "staging_claim_items_budget"
            }
        );
        assert_eq!(
            ReadyMember::new(StagedMemberId::new(1, 1), 0, 10, at(0))
                .expect_err("an empty member is refused"),
            AssemblyError::EmptyMember {
                shard: 1,
                generation: 1
            }
        );
    }

    /// Two layouts with different partition granularity are different keys.
    #[test]
    fn a_changed_layout_changes_the_assembly_key() {
        let tenant = DataTenantId::new_v7();
        let hourly = ScribeAssemblyKey::new(
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            SchemaFingerprint([7; 32]),
            &layout(Some(TimeGranularityWire::Hour)),
            partition(0),
            NodeId::new(uuid::Uuid::from_u128(9)),
            WriterEpoch::new(4),
        );
        let daily = ScribeAssemblyKey::new(
            tenant,
            TableRef::new(BifrostNamespace::Bifrost, "events"),
            SchemaFingerprint([7; 32]),
            &layout(Some(TimeGranularityWire::Day)),
            partition(0),
            NodeId::new(uuid::Uuid::from_u128(9)),
            WriterEpoch::new(4),
        );
        assert_ne!(hourly, daily);
    }

    /// A restored claim keeps its slot and its members until it settles.
    ///
    /// # Panics
    ///
    /// Panics when a fixture registration, restore, or claim is refused.
    #[test]
    fn restored_claims_keep_their_members_and_their_slot() {
        let key = key_for(DataTenantId::new_v7(), 0);
        let claim = StagingClaimId::from_hex(&"b".repeat(64)).expect("the fixture identity parses");
        let owned = member(2, 5, 1_000, 0);
        let mut assembler = assembler(1_000, 1);
        assembler
            .restore(
                &key,
                [
                    RecoveredMember::Claimed {
                        claim,
                        member: owned,
                    },
                    RecoveredMember::Ready(member(2, 6, 1_000, 1)),
                ],
            )
            .expect("recovery restores both members");

        assert_eq!(assembler.outstanding_claims(), 1);
        assert_eq!(assembler.ready_members(&key).len(), 1);
        // The restored claim holds the only slot, so the ready member waits
        // rather than opening a second concurrent publication.
        assert_eq!(
            assembler
                .next_claim(at(5))
                .expect_err("the restored claim holds the budget"),
            AssemblyError::ClaimBudgetExhausted { budget: 1 }
        );
        // Its members are still owned, so a replayed registration is refused.
        assert_eq!(
            assembler
                .register_ready(&key, owned)
                .expect_err("a claimed member is already owned"),
            AssemblyError::DuplicateMember {
                shard: 2,
                generation: 5
            }
        );

        assembler
            .settle_claim(claim)
            .expect("the restored claim settles");
        let next = assembler
            .next_claim(at(6))
            .expect("the slot is free again")
            .expect("the ready member reaches target");
        assert_eq!(next.members().len(), 1);
        assert_eq!(next.members()[0].id(), StagedMemberId::new(2, 6));
    }
}
