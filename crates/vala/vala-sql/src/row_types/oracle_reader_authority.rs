//! Validated values behind Oracle's durable reader authority.
//!
//! Three things are durable: which table a maintenance decision serializes on,
//! which fenced Oracle epoch holds a lease, and what that epoch's conservative
//! protection frontier for one table is. This module owns the pure half —
//! identity, state, ancestry, and the versioned digests — so the query owners
//! stay statements and the validation rules have exactly one home.
//!
//! Nothing here performs IO or decides whether a snapshot may be expired. It
//! decides whether the evidence a caller supplies or Postgres returns is
//! well-formed enough to be trusted, and fails closed when it is not.

use sha2::{Digest, Sha256};
use wyrd_spec::DataTenantId;

use crate::SqlError;

/// Encoding version stamped on every frontier header this build writes.
pub const FRONTIER_ENCODING_VERSION: i32 = 1;
/// Encoding version stamped on every ancestry member this build writes.
pub const ANCESTRY_DIGEST_VERSION: i32 = 1;
/// Domain separator for the version 1 ancestry digest preimage.
const ANCESTRY_DIGEST_DOMAIN: &[u8] = b"wyrd.oracle.ancestry/v1\0";
/// Domain separator for the version 1 frontier digest preimage.
const FRONTIER_DIGEST_DOMAIN: &[u8] = b"wyrd.oracle.frontier/v1\0";

/// Constructs a fail-closed invariant error without leaking row payloads.
fn invariant(detail: &str) -> SqlError {
    SqlError::InvariantViolation {
        detail: detail.to_owned(),
    }
}

/// The durable identity every reader-protection statement is keyed by.
///
/// `table_uid` is the identity; the catalog, namespace, and table names are
/// checked payload that makes registry drift visible rather than silent. A
/// consumer that reconstructs a namespace string and finds it disagreeing with
/// the registered table has contradictory evidence, not a naming preference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableAuthorityIdentity {
    /// Tenant owning both the table and its protection rows.
    pub tenant: DataTenantId,
    /// Durable 16-byte table UID from `vala.bifrost_tables`.
    pub table_uid: [u8; 16],
    /// Exact catalog wire name; only the single Bifrost catalog is valid.
    pub catalog_name: String,
    /// Logical Bifrost namespace of the registered table.
    pub namespace_name: String,
    /// Physical table name inside that namespace.
    pub table_name: String,
}

impl TableAuthorityIdentity {
    /// Validates the checked payload before any statement binds it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the catalog is not the
    /// single Bifrost catalog or when a name segment is blank.
    pub fn validate(&self, expected_catalog: &str) -> Result<(), SqlError> {
        if self.catalog_name != expected_catalog {
            return Err(invariant(
                "reader protection names a catalog other than the Bifrost catalog",
            ));
        }
        if self.namespace_name.trim().is_empty() || self.table_name.trim().is_empty() {
            return Err(invariant("reader protection names a blank table identity"));
        }
        Ok(())
    }

    /// Canonical ordering key used to lock several tables without deadlock.
    ///
    /// Multi-table admission sorts by `(tenant UUID bytes, table UID bytes)`
    /// and never waits on a lower key while holding a higher one, so two
    /// queries requesting the same pair in opposite orders still serialize.
    #[must_use]
    pub fn order_key(&self) -> ([u8; 16], [u8; 16]) {
        (*uuid::Uuid::from(self.tenant).as_bytes(), self.table_uid)
    }
}

/// Closed durable state of one fenced Oracle reader epoch.
///
/// Retirement is deletion of the row, not a fifth state: an epoch that has been
/// invalidated and has released every table leaves no evidence behind, which is
/// what keeps a safely retired epoch from becoming a permanent maintenance
/// root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OracleEpochState {
    /// The lease exists and no read has been admitted under it yet.
    Acquired,
    /// Every dependency is established and the epoch may admit reads.
    Active,
    /// Admission is closed and descendants are being joined.
    Draining,
    /// The epoch can never renew or admit again; its protection may be released.
    Invalidated,
}

impl OracleEpochState {
    /// Maps one state to its durable SQL discriminator.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Acquired => "acquired",
            Self::Active => "active",
            Self::Draining => "draining",
            Self::Invalidated => "invalidated",
        }
    }

    /// Parses one durable SQL discriminator.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for unknown stored state text,
    /// which is corrupt evidence rather than an absent epoch.
    pub fn parse(text: &str) -> Result<Self, SqlError> {
        match text {
            "acquired" => Ok(Self::Acquired),
            "active" => Ok(Self::Active),
            "draining" => Ok(Self::Draining),
            "invalidated" => Ok(Self::Invalidated),
            _ => Err(invariant("unknown Oracle reader epoch state")),
        }
    }
}

/// One epoch's durable lease as Postgres currently holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OracleEpochRow {
    /// Physical node holding the epoch.
    pub node_id: uuid::Uuid,
    /// Exact `vala.cluster_nodes` Oracle fence this epoch was acquired under.
    pub fencing_token: i64,
    /// Current lifecycle state.
    pub state: OracleEpochState,
    /// Monotonic revision incremented once per persisted state edge or renewal.
    pub state_revision: i64,
    /// Database time at acquisition.
    pub acquired_at: chrono::DateTime<chrono::Utc>,
    /// Database time at activation, absent until the epoch activates.
    pub activated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Database time of the most recent successful renewal.
    pub renewed_at: chrono::DateTime<chrono::Utc>,
    /// Database time after which the lease no longer authorizes source IO.
    pub lease_expires_at: chrono::DateTime<chrono::Utc>,
    /// Database time at invalidation, absent until the epoch is invalidated.
    pub invalidated_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// One statement's view of database time next to the lease it just confirmed.
///
/// Both instants come from the same `statement_timestamp()`, so the remaining
/// lease is a single database fact. A caller never compares its own wall clock
/// against `lease_expires_at`; it converts this remainder onto its monotonic
/// clock instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleLeaseSample {
    /// `statement_timestamp()` observed by the confirming statement.
    pub database_now: chrono::DateTime<chrono::Utc>,
    /// Lease expiry that statement confirmed or wrote.
    pub lease_expires_at: chrono::DateTime<chrono::Utc>,
    /// Revision the confirming statement left behind.
    pub state_revision: i64,
}

/// One comparable Iceberg chain this epoch still needs for a table.
///
/// The retained head is the newest active local cut on the chain and the
/// protected snapshot is the oldest; `ancestry_path` is the inclusive
/// newest-to-oldest parent walk that proves they are on one chain. Snapshot IDs
/// are never compared numerically to infer ancestry, which is why the proof is
/// carried rather than recomputed by every consumer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionMember {
    /// Oldest active cut on this chain; everything from here up must survive.
    pub protected_snapshot_id: i64,
    /// Iceberg timestamp of the protected snapshot.
    pub protected_snapshot_timestamp_ms: i64,
    /// Newest active cut on this chain.
    pub retained_head_snapshot_id: i64,
    /// Iceberg timestamp of the retained head.
    pub retained_head_timestamp_ms: i64,
    /// Inclusive newest-to-oldest parent walk from head to protected snapshot.
    pub ancestry_path: Vec<i64>,
    /// Encoding version of [`ProtectionMember::ancestry_digest`].
    pub ancestry_digest_version: i32,
    /// Version 1 digest binding this member to its tenant and table.
    pub ancestry_digest: [u8; 32],
}

impl ProtectionMember {
    /// Builds one member and stamps its version 1 digest.
    ///
    /// The caller supplies the ancestry it read from immutable Iceberg
    /// metadata; this constructor fixes the endpoints from that path so the
    /// stored endpoints and the stored proof can never disagree.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the path is empty or when
    /// either timestamp is negative.
    pub fn new(
        identity: &TableAuthorityIdentity,
        ancestry_path: Vec<i64>,
        retained_head_timestamp_ms: i64,
        protected_snapshot_timestamp_ms: i64,
    ) -> Result<Self, SqlError> {
        let (Some(&retained_head_snapshot_id), Some(&protected_snapshot_id)) =
            (ancestry_path.first(), ancestry_path.last())
        else {
            return Err(invariant("reader protection member has an empty ancestry"));
        };
        let member = Self {
            protected_snapshot_id,
            protected_snapshot_timestamp_ms,
            retained_head_snapshot_id,
            retained_head_timestamp_ms,
            ancestry_path,
            ancestry_digest_version: ANCESTRY_DIGEST_VERSION,
            ancestry_digest: [0; 32],
        };
        let ancestry_digest = member.compute_digest(identity);
        let member = Self {
            ancestry_digest,
            ..member
        };
        member.validate(identity)?;
        Ok(member)
    }

    /// Recomputes this member's version 1 digest over the canonical preimage.
    ///
    /// The preimage binds tenant and table identity into the digest, so a
    /// member row copied between tables or tenants no longer verifies.
    #[must_use]
    fn compute_digest(&self, identity: &TableAuthorityIdentity) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(ANCESTRY_DIGEST_DOMAIN);
        hasher.update(uuid::Uuid::from(identity.tenant).as_bytes());
        hasher.update(identity.table_uid);
        hasher.update(self.retained_head_snapshot_id.to_be_bytes());
        hasher.update(self.protected_snapshot_id.to_be_bytes());
        let path_len = u32::try_from(self.ancestry_path.len()).unwrap_or(u32::MAX);
        hasher.update(path_len.to_be_bytes());
        for snapshot_id in &self.ancestry_path {
            hasher.update(snapshot_id.to_be_bytes());
        }
        hasher.finalize().into()
    }

    /// Proves one member is internally consistent and bound to this table.
    ///
    /// Every rule here is a fail-closed check on evidence Forge will later
    /// treat as protection: an unknown digest version, a path whose endpoints
    /// disagree with the stored endpoints, a repeated snapshot in the walk, or
    /// a digest that does not reproduce are all corruption, and none of them
    /// may degrade into "this table is unprotected".
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for an unknown digest version,
    /// an empty or self-repeating path, endpoints that disagree with the path,
    /// a retained head older than the protected snapshot, a negative timestamp,
    /// or a digest that does not reproduce from the preimage.
    pub fn validate(&self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        if self.ancestry_digest_version != ANCESTRY_DIGEST_VERSION {
            return Err(invariant(
                "reader protection member uses an unknown ancestry digest version",
            ));
        }
        if self.protected_snapshot_timestamp_ms < 0 || self.retained_head_timestamp_ms < 0 {
            return Err(invariant(
                "reader protection member carries a negative snapshot timestamp",
            ));
        }
        if self.retained_head_timestamp_ms < self.protected_snapshot_timestamp_ms {
            return Err(invariant(
                "reader protection member retains a head older than the snapshot it protects",
            ));
        }
        let (Some(&head), Some(&protected)) =
            (self.ancestry_path.first(), self.ancestry_path.last())
        else {
            return Err(invariant("reader protection member has an empty ancestry"));
        };
        if head != self.retained_head_snapshot_id || protected != self.protected_snapshot_id {
            return Err(invariant(
                "reader protection member endpoints disagree with its ancestry path",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        for snapshot_id in &self.ancestry_path {
            if !seen.insert(*snapshot_id) {
                return Err(invariant(
                    "reader protection member repeats a snapshot in its ancestry path",
                ));
            }
        }
        if self.ancestry_path.len() == 1 && head != protected {
            return Err(invariant(
                "reader protection member endpoints disagree with its ancestry path",
            ));
        }
        if self.compute_digest(identity) != self.ancestry_digest {
            return Err(invariant(
                "reader protection member ancestry digest does not reproduce",
            ));
        }
        Ok(())
    }

    /// Reports whether this member's chain already covers `snapshot_id`.
    ///
    /// Coverage is membership in the proven path, never a timestamp or
    /// snapshot-ID comparison: an unrelated lineage can carry both a newer
    /// timestamp and a larger ID without protecting anything.
    #[must_use]
    pub fn covers(&self, snapshot_id: i64) -> bool {
        self.ancestry_path.contains(&snapshot_id)
    }
}

/// One epoch's complete conservative protection for one table.
///
/// A frontier is a set of members rather than a single oldest snapshot because
/// Iceberg lineages fork: two active cuts on incomparable chains cannot be
/// covered by any one snapshot, and collapsing them would silently stop
/// protecting one of the two.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProtectionFrontier {
    /// Every comparable chain this epoch still needs, in canonical order.
    pub members: Vec<ProtectionMember>,
}

impl ProtectionFrontier {
    /// Builds a frontier from validated members in canonical digest order.
    ///
    /// Members are sorted by their digest bytes so the same logical frontier
    /// always produces the same header digest regardless of the order local
    /// query guards happened to be reduced in.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when any member fails
    /// [`ProtectionMember::validate`] or when two members protect the same
    /// snapshot.
    pub fn new(
        identity: &TableAuthorityIdentity,
        mut members: Vec<ProtectionMember>,
    ) -> Result<Self, SqlError> {
        let mut protected = std::collections::BTreeSet::new();
        for member in &members {
            member.validate(identity)?;
            if !protected.insert(member.protected_snapshot_id) {
                return Err(invariant(
                    "reader protection frontier repeats a protected snapshot",
                ));
            }
        }
        members.sort_by_key(|member| member.ancestry_digest);
        Ok(Self { members })
    }

    /// Reports whether the frontier is empty, which is a release rather than
    /// an absence of evidence.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    /// Reports whether some proven chain already covers `snapshot_id`.
    #[must_use]
    pub fn covers(&self, snapshot_id: i64) -> bool {
        self.members.iter().any(|member| member.covers(snapshot_id))
    }

    /// Computes the version 1 header digest over the canonical member order.
    #[must_use]
    pub fn digest(&self, identity: &TableAuthorityIdentity) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(FRONTIER_DIGEST_DOMAIN);
        hasher.update(uuid::Uuid::from(identity.tenant).as_bytes());
        hasher.update(identity.table_uid);
        let count = u32::try_from(self.members.len()).unwrap_or(u32::MAX);
        hasher.update(count.to_be_bytes());
        let mut digests: Vec<[u8; 32]> = self.members.iter().map(|m| m.ancestry_digest).collect();
        digests.sort_unstable();
        for digest in digests {
            hasher.update(digest);
        }
        hasher.finalize().into()
    }
}

/// One durable protection header plus the members it commits to.
///
/// Header and members are read and written together because a header whose
/// digest does not reproduce over its own members is corruption, and Forge must
/// see that as contradictory evidence rather than as a smaller protected set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionRecord {
    /// Monotonic per-table revision this epoch has confirmed.
    pub revision: i64,
    /// Encoding version of the stored frontier.
    pub frontier_encoding_version: i32,
    /// Stored header digest over the member set.
    pub frontier_digest: [u8; 32],
    /// Database time of the commit that wrote this revision.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// The exact frontier this header commits to.
    pub frontier: ProtectionFrontier,
}

impl ProtectionRecord {
    /// Proves one stored record is complete, versioned, and self-consistent.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] for a non-positive revision, an
    /// unknown frontier encoding version, a member that fails validation, or a
    /// header digest that does not reproduce over the stored members.
    pub fn validate(&self, identity: &TableAuthorityIdentity) -> Result<(), SqlError> {
        if self.revision < 1 {
            return Err(invariant("reader protection header has no revision"));
        }
        if self.frontier_encoding_version != FRONTIER_ENCODING_VERSION {
            return Err(invariant(
                "reader protection header uses an unknown frontier encoding version",
            ));
        }
        for member in &self.frontier.members {
            member.validate(identity)?;
        }
        if self.frontier.digest(identity) != self.frontier_digest {
            return Err(invariant(
                "reader protection header digest does not reproduce over its members",
            ));
        }
        Ok(())
    }
}

/// Outcome of one compare-and-set against a table's protection revision.
///
/// A conflict returns the complete current record rather than a bare revision
/// so the coordinator can decide whether the winner already covers its local
/// cut instead of blindly recomputing and retrying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtectionCas {
    /// The expected revision matched and exactly one next revision committed.
    Committed(Box<ProtectionRecord>),
    /// Another writer moved the revision; this is the record it left behind.
    Conflict(Option<Box<ProtectionRecord>>),
}

/// One `(tenant, table, node, fence)` protection key crash recovery must settle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectionKey {
    /// Tenant owning the protected table.
    pub tenant: DataTenantId,
    /// Durable table UID the protection is keyed by.
    pub table_uid: [u8; 16],
    /// Node whose epoch published the protection.
    pub node_id: uuid::Uuid,
    /// Exact Oracle fence of that epoch.
    pub fencing_token: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a fixed identity every digest vector in this module is bound to.
    fn identity() -> TableAuthorityIdentity {
        TableAuthorityIdentity {
            tenant: DataTenantId::SYSTEM_OWNER,
            table_uid: [7; 16],
            catalog_name: "wyrd-redux".to_owned(),
            namespace_name: "vala.bifrost".to_owned(),
            table_name: "events".to_owned(),
        }
    }

    #[test]
    fn member_endpoints_come_from_the_ancestry_path() {
        let member = ProtectionMember::new(&identity(), vec![30, 20, 10], 300, 100)
            .expect("well-formed ancestry");
        assert_eq!(member.retained_head_snapshot_id, 30);
        assert_eq!(member.protected_snapshot_id, 10);
        assert!(member.covers(20));
        assert!(!member.covers(25));
    }

    #[test]
    fn member_digest_binds_tenant_and_table() {
        let id = identity();
        let mut other_table = id.clone();
        other_table.table_uid = [8; 16];
        let mut other_tenant = id.clone();
        other_tenant.tenant = DataTenantId::new_v7();
        let member = ProtectionMember::new(&id, vec![30, 10], 300, 100).expect("member");
        assert!(member.validate(&id).is_ok());
        assert!(member.validate(&other_table).is_err());
        assert!(member.validate(&other_tenant).is_err());
    }

    #[test]
    fn corrupt_member_evidence_fails_closed() {
        let base = ProtectionMember::new(&identity(), vec![30, 20, 10], 300, 100).expect("member");

        let mut tampered = base.clone();
        tampered.protected_snapshot_id = 20;
        assert!(tampered.validate(&identity()).is_err());

        let mut tampered = base.clone();
        tampered.ancestry_digest_version = 2;
        assert!(tampered.validate(&identity()).is_err());

        let mut tampered = base.clone();
        tampered.ancestry_digest[0] ^= 0xff;
        assert!(tampered.validate(&identity()).is_err());

        let mut tampered = base.clone();
        tampered.ancestry_path = vec![30, 20, 20, 10];
        assert!(tampered.validate(&identity()).is_err());

        let mut tampered = base;
        tampered.retained_head_timestamp_ms = 50;
        assert!(tampered.validate(&identity()).is_err());

        assert!(ProtectionMember::new(&identity(), Vec::new(), 1, 1).is_err());
    }

    #[test]
    fn frontier_digest_is_order_independent_and_covers_each_chain() {
        let id = identity();
        let left = ProtectionMember::new(&id, vec![30, 10], 300, 100).expect("member");
        let right = ProtectionMember::new(&id, vec![41, 11], 410, 110).expect("member");
        let forward =
            ProtectionFrontier::new(&id, vec![left.clone(), right.clone()]).expect("frontier");
        let reverse = ProtectionFrontier::new(&id, vec![right, left]).expect("frontier");
        assert_eq!(forward, reverse);
        assert_eq!(forward.digest(&id), reverse.digest(&id));
        assert!(forward.covers(10));
        assert!(forward.covers(41));
        assert!(!forward.covers(12));
    }

    #[test]
    fn frontier_rejects_a_repeated_protected_snapshot() {
        let id = identity();
        let one = ProtectionMember::new(&id, vec![30, 10], 300, 100).expect("member");
        let two = ProtectionMember::new(&id, vec![31, 10], 310, 100).expect("member");
        assert!(ProtectionFrontier::new(&id, vec![one, two]).is_err());
    }

    #[test]
    fn record_validation_rejects_a_digest_that_does_not_reproduce() {
        let id = identity();
        let frontier = ProtectionFrontier::new(
            &id,
            vec![ProtectionMember::new(&id, vec![30, 10], 300, 100).expect("member")],
        )
        .expect("frontier");
        let good = ProtectionRecord {
            revision: 1,
            frontier_encoding_version: FRONTIER_ENCODING_VERSION,
            frontier_digest: frontier.digest(&id),
            updated_at: chrono::Utc::now(),
            frontier,
        };
        assert!(good.validate(&id).is_ok());

        let mut tampered = good.clone();
        tampered.frontier_digest[31] ^= 0xff;
        assert!(tampered.validate(&id).is_err());

        let mut tampered = good.clone();
        tampered.frontier_encoding_version = 2;
        assert!(tampered.validate(&id).is_err());

        let mut tampered = good;
        tampered.revision = 0;
        assert!(tampered.validate(&id).is_err());
    }

    #[test]
    fn identity_rejects_a_foreign_catalog_or_blank_name() {
        let mut id = identity();
        assert!(id.validate("wyrd-redux").is_ok());
        id.catalog_name = "other".to_owned();
        assert!(id.validate("wyrd-redux").is_err());
        let mut id = identity();
        id.table_name = "  ".to_owned();
        assert!(id.validate("wyrd-redux").is_err());
    }
}
