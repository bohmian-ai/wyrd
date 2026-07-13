use wyrd_spec::ids::DataTenantId;
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;

pub mod buffer;
pub mod commit;
pub mod coordinator;
pub mod file_writer;
pub mod redaction;
pub mod registry_writers;

/// Caller-supplied context for one 2PC commit.
///
/// Threads the durable idempotency key and audit attribution from the ingest
/// orchestrator down to the writer, replacing the engine's former self-minted
/// `batch_id` and the hardcoded `system`/`system` audit columns.
///
/// - `batch_id` — the stream's `wyrd_batch_id` (16-byte `UUIDv7`); the durable
///   dedup key checked against `vala.olap_commits` before any Parquet write.
/// - `origin` / `actor` — audit columns stamped on the precommit row (e.g.
///   `"ingest"` / the resolved principal id).
/// - `request_id` — Wyrd request correlator; carried for the C5 audit append.
/// - `card_ref` — the **writer-identity** card (`Principal::card_ref()`),
///   constant for the whole commit, consumed only by C5 audit attribution. This
///   is NOT the per-row `card_ref` data column and is never stamped into a row.
pub struct BifrostWriteContext {
    /// Durable, per-commit idempotency key (the stream's `wyrd_batch_id`).
    pub batch_id: [u8; 16],
    /// Audit origin (e.g. `"ingest"`; `"system"` on the internal path).
    pub origin: String,
    /// Audit actor (resolved principal id; `"system"` on the internal path).
    pub actor: String,
    /// Request correlator for the commit.
    pub request_id: RequestId,
    /// Writer-identity card for C5 audit attribution; `None` on the internal path.
    pub card_ref: Option<CardRef>,
}

impl BifrostWriteContext {
    /// Context for the Stage-1 internal / recovery re-flush path: `origin` and
    /// `actor` are `"system"`, `card_ref` is `None`, and the `batch_id` /
    /// `request_id` are freshly minted `UUIDv7` values.
    #[must_use]
    pub fn system() -> Self {
        Self {
            batch_id: *uuid::Uuid::now_v7().as_bytes(),
            origin: "system".to_owned(),
            actor: "system".to_owned(),
            request_id: RequestId::now_v7(),
            card_ref: None,
        }
    }
}

/// The durable identity of one 2PC commit unit on a physical table.
///
/// A `SystemShared` physical table takes writes from many tenants, and
/// `batch_id` is client-supplied, so two tenants can present the *same*
/// `batch_id`. Keying idempotency / ack / audit on `batch_id` alone would let
/// one tenant's batch dedupe as the other's replay — silent data loss. The
/// `tenant` discriminator closes that: it is the data tenant whose RLS bind owns
/// the commit's `vala.olap_commits` row (written to both the `data_tenant_id` and
/// `control_bind` columns, which are equal on the write path —
/// `wyrd.current_tenant()` at precommit/finalize). Two tenants' identical client
/// `batch_id`s therefore map to two distinct `CommitKey`s.
///
/// Within one coordinator `table_uid` is fixed, so `CommitKey` is the whole
/// idempotency/ack/audit unit; the covering-table id is carried alongside.
///
/// > Note: this `tenant` is the commit's RLS-bind tenant, **not**
/// > [`TableScope::control_bind`](crate::types::TableScope::control_bind), which
/// > resolves to `SYSTEM_OWNER` for shared *registry* rows. Using that here would
/// > collapse every shared-table tenant onto one key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommitKey {
    /// Data tenant owning the commit's RLS bind (== `control_bind` column).
    pub tenant: DataTenantId,
    /// Client-supplied replay key (the stream's `wyrd_batch_id`).
    pub batch_id: [u8; 16],
}

impl CommitKey {
    /// The commit unit for `tenant`'s `batch_id`. `tenant` is the data tenant the
    /// precommit/finalize `TenantConn` binds to — the value the idempotency key
    /// `(table_uid, control_bind, batch_id)` discriminates on.
    #[must_use]
    pub fn new(tenant: DataTenantId, batch_id: [u8; 16]) -> Self {
        Self { tenant, batch_id }
    }

    /// The `batch_id` rendered as a canonical UUID string (logging / summaries).
    #[must_use]
    pub fn batch_uuid(&self) -> uuid::Uuid {
        uuid::Uuid::from_bytes(self.batch_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M02: two tenants presenting the SAME client `batch_id` on one physical
    /// (shared) table produce two DISTINCT `CommitKey`s, so neither dedupes the
    /// other. A genuine re-send (same tenant, same `batch_id`) is the only equal key.
    #[test]
    fn commit_key_same_batch_id_distinct_tenants() {
        let batch_id = *uuid::Uuid::now_v7().as_bytes();
        let tenant_a = DataTenantId::new_v7();
        let tenant_b = DataTenantId::new_v7();

        let key_a = CommitKey::new(tenant_a, batch_id);
        let key_b = CommitKey::new(tenant_b, batch_id);

        assert_ne!(
            key_a, key_b,
            "same batch_id under different tenants must not collide"
        );
        assert_eq!(
            key_a,
            CommitKey::new(tenant_a, batch_id),
            "same tenant + same batch_id is the idempotent re-send unit"
        );
        assert_eq!(key_a.batch_uuid(), uuid::Uuid::from_bytes(batch_id));
    }
}
