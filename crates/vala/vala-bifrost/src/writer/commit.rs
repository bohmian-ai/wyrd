use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use arrow::array::RecordBatch;
use iceberg::spec::DataFile;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::base_writer::data_file_writer::DataFileWriterBuilder;
use iceberg::writer::file_writer::ParquetWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, DefaultLocationGenerator,
};
use iceberg::writer::file_writer::rolling_writer::RollingFileWriterBuilder;
use iceberg::writer::partitioning::unpartitioned_writer::UnpartitionedWriter;
use iceberg_catalog_sql::SqlCatalog;
use sqlx::PgPool;
use sqlx::types::Uuid;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::AuditEvent;

use crate::error::BifrostError;
use crate::types::{TableScope, TableUid};
use crate::writer::file_writer::bifrost_writer_properties;
use crate::writer::{BifrostWriteContext, CommitKey};

/// Stable engine-instance identity stamped into `writer_owner` on every
/// precommit row. A fresh `Uuid::new_v4()` per process start means two pods
/// never share an owner, so fencing-token collisions are impossible.
pub(crate) static WRITER_INSTANCE: LazyLock<Uuid> = LazyLock::new(Uuid::new_v4);

/// Writer lease TTL in seconds. The guarded renewal before `fast_append`
/// must succeed within this window after the previous stamp. Configurable
/// in a later stage; 30 s is safe for current workloads.
const WRITER_LEASE_SECS: i64 = 30;

/// Fault-injection points for test-only commit path overrides.
///
/// Gated to test and bench builds; must never be reachable in production. The
/// crate's own `pg_tests` crash-recovery suite reaches it via `cfg(test)`; the
/// `bench-bin` arm keeps it available to bench binaries (a separate crate that
/// cannot see `cfg(test)` items).
///
/// TODO(vala-bifrost): fault injection is currently exercised only by the inline
/// `pg_tests` crash-recovery suite. Wire it into the write-path benches when
/// fault-latency benchmarking lands.
#[cfg(any(test, feature = "bench-bin"))]
#[derive(Clone, Debug)]
pub enum Lease {
    Expired,
    RenewedFuture,
    Normal,
}

#[cfg(any(test, feature = "bench-bin"))]
#[derive(Clone, Debug)]
pub enum FaultPoint {
    /// Insert precommit row + lease, optionally expire the lease, then fail
    /// before writing Parquet.
    AfterPreCommitRow { lease: Lease },
    /// Write files + commit to Iceberg successfully, then fail before
    /// `finalize_committed` (simulates crash between Iceberg commit and SQL finalize).
    AfterIcebergCommit,
    /// Write files, then fail before `renew_writer_fence` / `commit_to_iceberg`.
    BeforeIcebergCommit,
    /// Renew lease, expire it manually (to let recovery claim), then proceed
    /// to `fast_append` so `finalize_committed` matches zero rows.
    AfterRenewBeforeAppend,
}

/// Phase 1 outcome of [`claim_batch`].
enum Phase1 {
    Replay { snapshot_id: i64 },
    Fresh { owner: Uuid, fencing_token: i64 },
}

/// One request's commit unit inside a group flush.
///
/// Carries its durable [`CommitKey`] (`key.tenant` is the DATA tenant bound for
/// this unit's precommit + finalize), the audit-attribution context it arrived
/// with, its already-system-stamped batches, and the optional audit-outbox event
/// to append in the same tx as its finalize (S3.C5).
pub struct CommitGroup {
    /// `{tenant, batch_id}`; `key.tenant` is the DATA tenant bound for precommit
    /// + finalize.
    pub key: CommitKey,
    /// Origin / actor / `request_id` / `card_ref` for audit attribution.
    pub ctx: BifrostWriteContext,
    /// Batches already stamped with system columns, including `data_tenant_id`.
    pub batches: Vec<RecordBatch>,
    /// Per-key audit event, appended in this key's finalize tx (C5); `None` on
    /// the internal/system path.
    pub audit: Option<AuditEvent>,
}

/// Outcome of a [`run_group_commit`] flush: the one shared snapshot id (or `None`
/// when every key replayed and no append happened), the fresh keys now durable,
/// and the idempotent replay hits.
pub struct GroupCommitOutcome {
    /// `None` when EVERY key replayed (no append happened).
    pub snapshot_id: Option<i64>,
    /// Fresh keys now durable.
    pub committed: Vec<CommitKey>,
    /// Idempotent replay hits.
    pub replayed: Vec<CommitKey>,
}

/// Execute one 2PC commit of `batches` under `batch_id` for `table`.
///
/// Two phases, each with a clear short-circuit:
/// 1. [`claim_batch`] inspects the prior 2PC anchor. An already-committed batch
///    replays its recorded snapshot with no write — returning `(snapshot, None)`
///    so the caller keeps its current table ref. A prior failure (`failed`),
///    in-flight precommit, or already-aborted row is rejected. Aborted rows
///    return `CommitConflict` — the `batch_id` must not be reused.
/// 2. [`write_and_finalize`] writes Parquet, fast-appends to Iceberg, and records
///    the terminal FSM transition (`committed`, or `failed` on write error).
///
/// On a fresh write the returned [`Table`] is the post-append snapshot the caller
/// must adopt for its next commit; on replay it is `None`. Registry invalidation
/// (epoch bump + cache eviction) is the caller's responsibility after success.
///
/// # Errors
/// Returns [`BifrostError`] when the control-plane txn, Parquet write, or Iceberg
/// append fails, or when the `batch_id` collides with a failed/in-flight anchor.
///
/// `audit` is the transactional audit-outbox event to append in the same tx as
/// the successful finalize (S3.C5). It is `None` on paths that must NOT self-feed
/// the audit spine — most importantly the relay flush (`origin == "audit-relay"`,
/// review M-11) — and on the internal/system re-flush. On the idempotent replay
/// path no new op occurred, so no audit row is appended.
#[allow(clippy::too_many_arguments)]
pub async fn run_commit(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    batches: Vec<RecordBatch>,
    batch_id: [u8; 16],
    origin: &str,
    actor: &str,
    tenant: DataTenantId,
    audit: Option<AuditEvent>,
) -> Result<(i64, Option<Table>), BifrostError> {
    let table_fqn = table.identifier().to_string();

    // Legacy single-key path: the caller (coordinator) passes the registration
    // anchor as `tenant`, so control_bind == the bound tenant. Behavior-preserving.
    let key = CommitKey::new(tenant, batch_id);
    let phase1 = claim_batch(pool, table_uid, &key, origin, actor, tenant, &table_fqn).await?;

    match phase1 {
        Phase1::Replay { snapshot_id } => Ok((snapshot_id, None)),
        Phase1::Fresh {
            owner,
            fencing_token,
        } => {
            let (snapshot_id, updated_table) = write_and_finalize(
                pool,
                catalog,
                table,
                table_uid,
                batches,
                &batch_id,
                tenant,
                owner,
                fencing_token,
                audit,
            )
            .await?;
            Ok((snapshot_id, Some(updated_table)))
        }
    }
}

/// Execute one group flush: N per-tenant precommit txns → ONE Iceberg
/// `fast_append` → N per-tenant finalize txns.
///
/// Each [`CommitGroup`] is a distinct durable commit unit keyed by its
/// [`CommitKey`] (`{tenant, batch_id}`). All the fresh groups' Parquet data files
/// land in a SINGLE Iceberg snapshot, so N requests amortize one catalog commit.
/// The precommit and finalize txns bind to each group's DATA tenant
/// (`group.key.tenant` → `data_tenant_id`, the RLS anchor + dedup discriminator) so
/// a `SystemShared` table keeps every tenant on its own
/// `(data_tenant_id, table_uid, batch_id)` row instead of collapsing two tenants'
/// identical `batch_id`s. `control_bind` (`scope.control_bind`) is only the
/// `SYSTEM_OWNER` registration anchor for the `bifrost_tables` foreign key.
///
/// Flow:
/// 1. Prepare (N tenant-scoped txns) via [`claim_batch`]. A `Replay` key records
///    its snapshot and drops from the write set; a `Fresh` key is kept with its
///    `(owner, fencing_token)`.
/// 2. If no fresh groups survive (all replayed) return with `snapshot_id: None`
///    and no Iceberg append.
/// 3. Fence + write once: renew EVERY fresh group's fence BEFORE writing any
///    Parquet, dropping a lost-fence group before its bytes hit object storage
///    (keeps orphaned Parquet at zero on the fence-loss path); then
///    [`write_batches`] each surviving group into one shared `Vec<DataFile>`.
/// 4. Append once via [`commit_group_to_iceberg`], stamping `wyrd_commit_keys`.
/// 5. Finalize (N tenant-scoped txns) via `finalize_committed`, appending each
///    group's audit event in its finalize tx; a post-append fence loss records
///    the residual and is left to the recovery oracle.
///
/// # Errors
/// Returns [`BifrostError`] when a control-plane txn, the Parquet write, or the
/// Iceberg append fails, or when a `batch_id` collides with a failed / in-flight /
/// aborted anchor (see the 02.4 note below on per-key collision isolation).
#[allow(clippy::too_many_lines)]
pub async fn run_group_commit(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    scope: TableScope,
    groups: Vec<CommitGroup>,
) -> Result<GroupCommitOutcome, BifrostError> {
    let table_fqn = table.identifier().to_string();

    let mut replayed: Vec<CommitKey> = Vec::new();
    // Fresh groups kept together with their claimed fence.
    let mut fresh: Vec<(CommitGroup, Uuid, i64)> = Vec::new();

    // 1. Prepare: one tenant-scoped precommit txn per group.
    for group in groups {
        // control_bind is the registration anchor (SYSTEM_OWNER for SystemShared,
        // the data tenant for TenantOwned) = the bifrost_tables FK key; the bind
        // stays the DATA tenant so RLS keeps each tenant on its own commit row.
        let phase1 = claim_batch(
            pool,
            table_uid,
            &group.key,
            &group.ctx.origin,
            &group.ctx.actor,
            scope.control_bind(group.key.tenant),
            &table_fqn,
        )
        // 02.4: per-key collision isolation (fail only that key's reply and keep
        // the flush going) needs the coordinator's pending-reply map; here a
        // claim collision propagates and fails the whole call.
        .await?;

        match phase1 {
            Phase1::Replay { snapshot_id: _ } => replayed.push(group.key),
            Phase1::Fresh {
                owner,
                fencing_token,
            } => fresh.push((group, owner, fencing_token)),
        }
    }

    // 2. All replayed → no append.
    if fresh.is_empty() {
        return Ok(GroupCommitOutcome {
            snapshot_id: None,
            committed: Vec::new(),
            replayed,
        });
    }

    // 3a. Fence every fresh group BEFORE writing any Parquet: a group whose fence
    // is lost drops out here, before its bytes are written, so no orphaned files.
    let mut survivors: Vec<(CommitGroup, Uuid, i64)> = Vec::with_capacity(fresh.len());
    for (group, owner, fencing_token) in fresh {
        let mut conn = vala_sql::TenantConn::acquire(pool, group.key.tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let held = vala_sql::queries::olap_catalog::renew_writer_fence(
            &mut conn,
            table_uid.as_bytes(),
            &group.key.batch_id,
            owner,
            fencing_token,
            WRITER_LEASE_SECS,
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        if held {
            survivors.push((group, owner, fencing_token));
        } else {
            tracing::warn!(
                batch_id = %group.key.batch_uuid(),
                "writer fence lost before group append; dropping group before write"
            );
        }
    }

    // A lost fence on every fresh group leaves nothing to append.
    if survivors.is_empty() {
        return Ok(GroupCommitOutcome {
            snapshot_id: None,
            committed: Vec::new(),
            replayed,
        });
    }

    // 3b. Write each surviving group's Parquet into one shared data-file set.
    let mut all_files: Vec<DataFile> = Vec::new();
    for (group, _, _) in &survivors {
        let files = write_batches(table, group.batches.clone()).await?;
        all_files.extend(files);
    }

    // 4. ONE Iceberg append for the whole group, stamping wyrd_commit_keys.
    let fresh_keys: Vec<CommitKey> = survivors.iter().map(|(g, _, _)| g.key).collect();
    let (snapshot_id, _updated_table) =
        commit_group_to_iceberg(catalog, table, all_files, &fresh_keys).await?;

    // 5. Finalize: one tenant-scoped txn per surviving group.
    let mut committed: Vec<CommitKey> = Vec::new();
    for (group, owner, fencing_token) in survivors {
        let mut conn = vala_sql::TenantConn::acquire(pool, group.key.tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let finalized = vala_sql::queries::olap_catalog::finalize_committed(
            &mut conn,
            table_uid.as_bytes(),
            &group.key.batch_id,
            snapshot_id,
            owner,
            fencing_token,
        )
        .await
        .map_err(BifrostError::Sql)?;

        if finalized {
            // Append the audit-outbox row in the SAME tx as the finalize (S3.C5):
            // an append failure fails the op closed.
            if let Some(event) = group.audit.as_ref() {
                vala_sql::queries::audit_outbox::append_audit(&mut conn, event)
                    .await
                    .map_err(|e| {
                        tracing::error!(error = %e, "audit outbox append failed; refusing commit");
                        BifrostError::AuditUnavailable("audit outbox append failed".to_string())
                    })?;
            }
            conn.commit().await.map_err(BifrostError::Sql)?;
            committed.push(group.key);
        } else {
            // Fence lost AFTER the append: record the residual; the recovery
            // oracle owns this row, so it is not pushed to `committed`.
            let _ = vala_sql::queries::olap_catalog::record_fence_loss_after_append(
                &mut conn,
                table_uid.as_bytes(),
                &group.key.batch_id,
                owner,
                fencing_token,
                snapshot_id,
                false,
                None,
            )
            .await;
            let _ = conn.commit().await;
            tracing::warn!(
                snapshot_id,
                batch_id = %group.key.batch_uuid(),
                "writer fence lost after group append; snapshot key may be orphaned"
            );
        }
    }

    Ok(GroupCommitOutcome {
        snapshot_id: Some(snapshot_id),
        committed,
        replayed,
    })
}

#[cfg(any(test, feature = "bench-bin"))]
async fn expire_writer_lease(
    pool: &PgPool,
    table_uid: &TableUid,
    batch_id: &[u8; 16],
    tenant: DataTenantId,
) -> Result<(), BifrostError> {
    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;
    let result = sqlx::query(
        "UPDATE vala.olap_commits \
         SET writer_lease_expires_at = now() - interval '1 second' \
         WHERE table_uid = $1 AND batch_id = $2",
    )
    .bind(table_uid.as_bytes().as_slice())
    .bind(batch_id.as_slice())
    .execute(&mut **conn.transaction())
    .await
    .map_err(|e| BifrostError::Sql(vala_sql::SqlError::from(e)))?;
    if result.rows_affected() != 1 {
        return Err(BifrostError::Internal(format!(
            "expire_writer_lease: expected 1 row updated, got {}; table_uid={table_uid:?} batch_id={batch_id:?}",
            result.rows_affected()
        )));
    }
    conn.commit().await.map_err(BifrostError::Sql)
}

/// Phase 1 of [`run_commit`]: dispatch on the prior 2PC anchor, then claim the
/// batch for a fresh write and record the initial writer lease.
///
/// - [`Phase1::Replay`] — the batch already committed; the caller must replay.
/// - [`Phase1::Fresh`] — no prior anchor; a `precommit` row + writer lease has
///   been written and the caller should proceed with the write.
/// - `Err(..)` — collision with a prior failure, in-flight precommit, or aborted row.
///
/// # Errors
/// Returns [`BifrostError::Sql`] on control-plane failure,
/// [`BifrostError::MetadataMismatch`] when a `committed` row has a NULL
/// `snapshot_id`, or the collision errors above.
async fn claim_batch(
    pool: &PgPool,
    table_uid: &TableUid,
    key: &CommitKey,
    origin: &str,
    actor: &str,
    control_bind: DataTenantId,
    table_fqn: &str,
) -> Result<Phase1, BifrostError> {
    let mut conn = vala_sql::TenantConn::acquire(pool, key.tenant)
        .await
        .map_err(BifrostError::Sql)?;

    let existing = vala_sql::queries::olap_catalog::lookup_idempotent(
        &mut conn,
        table_uid.as_bytes(),
        &key.batch_id,
    )
    .await
    .map_err(BifrostError::Sql)?;

    match existing {
        Some(row) if row.state == "committed" => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            let snapshot_id = row.snapshot_id.ok_or_else(|| {
                BifrostError::MetadataMismatch(format!(
                    "committed olap_commits row for {table_fqn} has NULL snapshot_id"
                ))
            })?;
            Ok(Phase1::Replay { snapshot_id })
        }
        Some(row) if row.state == "failed" => {
            conn.commit().await.map_err(BifrostError::Sql)?;
            Err(BifrostError::DuplicateFailedBatch(
                uuid::Uuid::from_bytes(key.batch_id).to_string(),
            ))
        }
        Some(_) => {
            // Covers in-flight precommit, aborted (dead terminal — batch_id must not be
            // reused), and any future non-committed state.
            conn.commit().await.map_err(BifrostError::Sql)?;
            Err(BifrostError::CommitConflict(table_fqn.to_string()))
        }
        None => {
            vala_sql::queries::olap_catalog::precommit_with_bind(
                &mut conn,
                table_uid.as_bytes(),
                control_bind.as_uuid(),
                &key.batch_id,
                origin,
                actor,
            )
            .await
            .map_err(BifrostError::Sql)?;

            let owner = *WRITER_INSTANCE;
            let fencing_token =
                vala_sql::queries::olap_catalog::mint_writer_fencing_token(&mut conn)
                    .await
                    .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_catalog::record_writer_lease(
                &mut conn,
                table_uid.as_bytes(),
                &key.batch_id,
                owner,
                fencing_token,
                WRITER_LEASE_SECS,
            )
            .await
            .map_err(BifrostError::Sql)?;

            conn.commit().await.map_err(BifrostError::Sql)?;
            Ok(Phase1::Fresh {
                owner,
                fencing_token,
            })
        }
    }
}

/// Phase 2 of [`run_commit`]: write the claimed `batches` and record the terminal
/// FSM transition.
///
/// Writes Parquet and fast-appends to Iceberg. The guarded lease renewal
/// (`renew_writer_fence`) immediately before `fast_append` ensures an expired
/// or recovery-claimed lease fails closed — the writer cannot append after its
/// fence is lost. On success the anchor moves to `committed` with the discovered
/// snapshot id and the post-append [`Table`] is returned. On write failure the
/// anchor moves to `failed` (best-effort).
///
/// # Errors
/// Returns the underlying [`BifrostError`] from the Parquet write or Iceberg
/// append, or [`BifrostError::Sql`] when the `committed` finalize fails.
#[allow(clippy::too_many_arguments)]
async fn write_and_finalize(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    batches: Vec<RecordBatch>,
    batch_id: &[u8; 16],
    tenant: DataTenantId,
    owner: Uuid,
    fencing_token: i64,
    audit: Option<AuditEvent>,
) -> Result<(i64, Table), BifrostError> {
    let files = match write_batches(table, batches).await {
        Err(e) => {
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let _ = vala_sql::queries::olap_catalog::finalize_failed(
                &mut conn,
                table_uid.as_bytes(),
                batch_id,
                "WYRD_VALA_500_BIFROST_INTERNAL",
                &e.to_string(),
                owner,
                fencing_token,
            )
            .await;
            let _ = conn.commit().await;
            return Err(e);
        }
        Ok(files) => files,
    };

    // Guarded lease renewal: proves the fence is still held before the Iceberg
    // append. Fails closed on expired lease (writer_lease_expires_at <= now())
    // or when recovery has claimed the row (recovery_fencing_token IS NOT NULL).
    let held = {
        let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let held = vala_sql::queries::olap_catalog::renew_writer_fence(
            &mut conn,
            table_uid.as_bytes(),
            batch_id,
            owner,
            fencing_token,
            WRITER_LEASE_SECS,
        )
        .await
        .map_err(BifrostError::Sql)?;
        conn.commit().await.map_err(BifrostError::Sql)?;
        held
    };

    if !held {
        // Best-effort delete the just-written files to avoid orphaned Parquet.
        for file in &files {
            let _ = table.file_io().delete(file.file_path()).await;
        }
        return Err(BifrostError::CommitConflict(
            "writer fence lost before iceberg append".to_string(),
        ));
    }

    let (snapshot_id, updated_table) = commit_to_iceberg(catalog, table, files, batch_id).await?;

    let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
        .await
        .map_err(BifrostError::Sql)?;

    let finalized = vala_sql::queries::olap_catalog::finalize_committed(
        &mut conn,
        table_uid.as_bytes(),
        batch_id,
        snapshot_id,
        owner,
        fencing_token,
    )
    .await
    .map_err(BifrostError::Sql)?;

    // Append the audit-outbox row in the SAME tx as the finalize (S3.C5): a
    // rolled-back commit leaves no audit row, and an audit-append failure
    // fails the op closed (WYRD_VALA_500_AUDIT_UNAVAILABLE). Only the fenced,
    // truly-finalized commit is audited; a lost fence (finalized == false) is
    // the recovery path's concern and appends nothing here.
    if finalized && let Some(event) = audit.as_ref() {
        vala_sql::queries::audit_outbox::append_audit(&mut conn, event)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "audit outbox append failed; refusing commit");
                BifrostError::AuditUnavailable("audit outbox append failed".to_string())
            })?;
    }

    conn.commit().await.map_err(BifrostError::Sql)?;

    if !finalized {
        // Fence was lost AFTER the append (the bounded post-append residual).
        // Record the audit row; best-effort rollback is not available in this
        // iceberg-rust version, so rolled_back is always false here.
        let mut audit_conn = vala_sql::TenantConn::acquire(pool, tenant)
            .await
            .map_err(BifrostError::Sql)?;
        let _ = vala_sql::queries::olap_catalog::record_fence_loss_after_append(
            &mut audit_conn,
            table_uid.as_bytes(),
            batch_id,
            owner,
            fencing_token,
            snapshot_id,
            false,
            None,
        )
        .await;
        let _ = audit_conn.commit().await;
        tracing::warn!(
            snapshot_id,
            "writer fence lost after iceberg append; snapshot may be orphaned"
        );
        return Err(BifrostError::CommitConflict(
            "writer fence lost after iceberg append".to_string(),
        ));
    }

    Ok((snapshot_id, updated_table))
}

/// Write `batches` to Parquet data files under `table`, returning the resulting
/// [`DataFile`] descriptors for the Iceberg append.
///
/// Casts each batch to the authoritative Arrow schema derived from the table's
/// Iceberg schema (which carries the `PARQUET:field_id` metadata and exact column
/// types the Iceberg writer requires) before writing.
///
/// # Errors
/// Returns [`BifrostError`] when schema derivation, a column cast, or the
/// underlying Iceberg/Parquet write fails.
async fn write_batches(
    table: &Table,
    batches: Vec<RecordBatch>,
) -> Result<Vec<DataFile>, BifrostError> {
    let file_io = table.file_io().clone();
    let table_metadata = table.metadata();
    let iceberg_schema = table_metadata.current_schema().clone();

    // The Iceberg writer requires:
    // 1. Arrow field metadata contains `PARQUET:field_id` so the NaN visitor
    //    can match fields by ID (FieldMatchMode::Id).
    // 2. Column types exactly match the Iceberg-derived Arrow schema (e.g.
    //    timestamps must use "+00:00" not "UTC").
    //
    // We derive the authoritative typed Arrow schema from the Iceberg schema
    // (which carries both constraints), then cast each batch's columns to
    // that schema before writing.
    let typed_schema = Arc::new(
        iceberg::arrow::schema_to_arrow_schema(&iceberg_schema).map_err(BifrostError::Iceberg)?,
    );

    let location_gen =
        DefaultLocationGenerator::new(table_metadata).map_err(BifrostError::Iceberg)?;
    let file_name_gen = DefaultFileNameGenerator::new(
        "bifrost".to_string(),
        Some(uuid::Uuid::now_v7().simple().to_string()),
        iceberg::spec::DataFileFormat::Parquet,
    );

    let writer_props = bifrost_writer_properties();
    let parquet_builder = ParquetWriterBuilder::new(writer_props, iceberg_schema);

    let rolling_builder = RollingFileWriterBuilder::new_with_default_file_size(
        parquet_builder,
        file_io,
        location_gen,
        file_name_gen,
    );

    let data_file_builder = DataFileWriterBuilder::new(rolling_builder);
    let mut writer = UnpartitionedWriter::new(data_file_builder);

    for batch in batches {
        // Align by field name, not position: `stamp_system_columns` appends only
        // the system columns and cannot know a table's per-policy correlation
        // columns (`run_id`, `card_uid`, `principal_id`), so the stamped batch is a
        // subset of the physical schema. Match each physical field by name; cast
        // present columns to the Iceberg-derived type, and fill an absent nullable
        // column (the server-resolved-or-null correlation columns) with a typed
        // NULL array. An absent non-nullable column is a programmer error.
        let nrows = batch.num_rows();
        let cast_columns: Vec<arrow::array::ArrayRef> = typed_schema
            .fields()
            .iter()
            .map(|field| match batch.column_by_name(field.name()) {
                Some(col) => arrow::compute::cast(col, field.data_type()).map_err(|e| {
                    BifrostError::Internal(format!("cast column {}: {e}", field.name()))
                }),
                None if field.is_nullable() => {
                    Ok(arrow::array::new_null_array(field.data_type(), nrows))
                }
                None => Err(BifrostError::Internal(format!(
                    "physical column {} absent from stamped batch and not nullable",
                    field.name()
                ))),
            })
            .collect::<Result<_, _>>()?;
        let typed = RecordBatch::try_new(typed_schema.clone(), cast_columns)
            .map_err(|e| BifrostError::Internal(format!("retype batch: {e}")))?;
        writer.write(typed).await.map_err(BifrostError::Iceberg)?;
    }

    writer.close().await.map_err(BifrostError::Iceberg)
}

/// Fast-append `data_files` to `table` as a single Iceberg transaction.
///
/// Stamps `wyrd_batch_id` in snapshot summary properties so the recovery
/// oracle can identify which snapshot corresponds to which batch. Returns
/// the new snapshot id alongside the updated [`Table`].
///
/// # Errors
/// Returns [`BifrostError::Iceberg`] when the transaction build or commit fails.
async fn commit_to_iceberg(
    catalog: &SqlCatalog,
    table: &Table,
    data_files: Vec<DataFile>,
    batch_id: &[u8; 16],
) -> Result<(i64, Table), BifrostError> {
    let mut props = HashMap::new();
    props.insert(
        "wyrd_batch_id".to_string(),
        uuid::Uuid::from_bytes(*batch_id).simple().to_string(),
    );
    append_snapshot(catalog, table, data_files, props).await
}

/// Fast-append `data_files` for a group flush as a single Iceberg transaction.
///
/// Stamps ONE snapshot summary property, `wyrd_commit_keys`: a JSON array with one
/// `{"data_tenant_id","batch_id"}` object per fresh [`CommitKey`], both rendered as
/// UUID simple-hex. `data_tenant_id` is the DATA tenant (`key.tenant`) — the M02
/// dedup discriminator the 02.3 recovery oracle matches a stale precommit's
/// `{data_tenant_id, batch_id}` pair against. It is NOT the `control_bind` column
/// (the `SYSTEM_OWNER` registration anchor for a shared table), which does not
/// discriminate tenants and so is useless for recovery. The JSON is built manually
/// because `serde_json` is not in the production library build; simple-hex values
/// need no escaping. Returns the new snapshot id alongside the updated [`Table`].
///
/// # Errors
/// Returns [`BifrostError::Iceberg`] when the transaction build or commit fails.
async fn commit_group_to_iceberg(
    catalog: &SqlCatalog,
    table: &Table,
    data_files: Vec<DataFile>,
    keys: &[CommitKey],
) -> Result<(i64, Table), BifrostError> {
    use std::fmt::Write as _;

    let mut objs = String::new();
    for (i, key) in keys.iter().enumerate() {
        if i > 0 {
            objs.push(',');
        }
        let tenant_hex = key.tenant.as_uuid().simple();
        let batch_hex = uuid::Uuid::from_bytes(key.batch_id).simple();
        let _ = write!(
            objs,
            "{{\"data_tenant_id\":\"{tenant_hex}\",\"batch_id\":\"{batch_hex}\"}}"
        );
    }
    let commit_keys = format!("[{objs}]");

    let mut props = HashMap::new();
    props.insert("wyrd_commit_keys".to_string(), commit_keys);
    append_snapshot(catalog, table, data_files, props).await
}

/// Fast-append `data_files` to `table` in one transaction, stamping `summary_props`
/// into the snapshot summary. Shared by [`commit_to_iceberg`] and
/// [`commit_group_to_iceberg`] so the transaction-build logic lives in one place.
///
/// # Errors
/// Returns [`BifrostError::Iceberg`] when the transaction build or commit fails.
async fn append_snapshot(
    catalog: &SqlCatalog,
    table: &Table,
    data_files: Vec<DataFile>,
    summary_props: HashMap<String, String>,
) -> Result<(i64, Table), BifrostError> {
    let tx = Transaction::new(table);
    let action = tx
        .fast_append()
        .set_snapshot_properties(summary_props)
        .add_data_files(data_files);
    let tx = action.apply(tx).map_err(BifrostError::Iceberg)?;
    let committed = tx.commit(catalog).await.map_err(BifrostError::Iceberg)?;
    let snapshot_id = committed.metadata().current_snapshot_id().unwrap_or(0);
    Ok((snapshot_id, committed))
}

// ── Test/bench fault injection ───────────────────────────────────────────────

#[cfg(any(test, feature = "bench-bin"))]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub async fn run_commit_with_fault(
    pool: &PgPool,
    catalog: &SqlCatalog,
    table: &Table,
    table_uid: &TableUid,
    batches: Vec<RecordBatch>,
    batch_id: [u8; 16],
    tenant: DataTenantId,
    fault: FaultPoint,
) -> Result<(i64, Option<Table>), BifrostError> {
    let table_fqn = table.identifier().to_string();
    let key = CommitKey::new(tenant, batch_id);
    let phase1 = claim_batch(
        pool, table_uid, &key, "system", "system", tenant, &table_fqn,
    )
    .await?;
    let (owner, fencing_token) = match phase1 {
        Phase1::Replay { snapshot_id } => return Ok((snapshot_id, None)),
        Phase1::Fresh {
            owner,
            fencing_token,
        } => (owner, fencing_token),
    };

    match fault {
        FaultPoint::AfterPreCommitRow { lease } => {
            if matches!(lease, Lease::Expired) {
                // Force the lease to expired so recovery can claim the row.
                expire_writer_lease(pool, table_uid, &batch_id, tenant).await?;
            }
            Err(BifrostError::Internal("fault: AfterPreCommitRow".into()))
        }

        FaultPoint::AfterIcebergCommit => {
            // Write files and commit to Iceberg, but skip finalize_committed.
            let files = write_batches(table, batches).await?;
            let (snapshot_id, updated_table) =
                commit_to_iceberg(catalog, table, files, &batch_id).await?;
            // Intentionally NOT calling finalize_committed — simulates crash.
            Err(BifrostError::Internal(format!(
                "fault: AfterIcebergCommit (snapshot {snapshot_id} committed but not finalized); \
                 table_updated={:?}",
                updated_table.metadata().current_snapshot_id()
            )))
        }

        FaultPoint::BeforeIcebergCommit => {
            // Write files, expire the lease, then fail before renewal.
            let _files = write_batches(table, batches).await?;
            expire_writer_lease(pool, table_uid, &batch_id, tenant).await?;
            Err(BifrostError::Internal("fault: BeforeIcebergCommit".into()))
        }

        FaultPoint::AfterRenewBeforeAppend => {
            // Write files, renew the lease (succeeds), then expire it manually
            // (simulates a stop-the-world pause longer than the TTL after the
            // renewal). The caller then runs recovery + resumes commit.
            let files = write_batches(table, batches).await?;
            let mut conn = vala_sql::TenantConn::acquire(pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            vala_sql::queries::olap_catalog::renew_writer_fence(
                &mut conn,
                table_uid.as_bytes(),
                &batch_id,
                owner,
                fencing_token,
                WRITER_LEASE_SECS,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn.commit().await.map_err(BifrostError::Sql)?;

            // Expire the freshly-renewed lease so recovery can claim it.
            expire_writer_lease(pool, table_uid, &batch_id, tenant).await?;

            // Now proceed to the Iceberg commit (recovery_fencing_token may be
            // set by the time this runs if recovery claimed the row).
            let (snapshot_id, updated_table) =
                commit_to_iceberg(catalog, table, files, &batch_id).await?;

            // finalize_committed will fail (return false) if recovery claimed us.
            let mut conn2 = vala_sql::TenantConn::acquire(pool, tenant)
                .await
                .map_err(BifrostError::Sql)?;
            let finalized = vala_sql::queries::olap_catalog::finalize_committed(
                &mut conn2,
                table_uid.as_bytes(),
                &batch_id,
                snapshot_id,
                owner,
                fencing_token,
            )
            .await
            .map_err(BifrostError::Sql)?;
            conn2.commit().await.map_err(BifrostError::Sql)?;

            if !finalized {
                let mut audit = vala_sql::TenantConn::acquire(pool, tenant)
                    .await
                    .map_err(BifrostError::Sql)?;
                let _ = vala_sql::queries::olap_catalog::record_fence_loss_after_append(
                    &mut audit,
                    table_uid.as_bytes(),
                    &batch_id,
                    owner,
                    fencing_token,
                    snapshot_id,
                    false,
                    None,
                )
                .await;
                let _ = audit.commit().await;
                return Err(BifrostError::CommitConflict(
                    "fault: AfterRenewBeforeAppend — fence lost after append".into(),
                ));
            }
            Ok((snapshot_id, Some(updated_table)))
        }
    }
}

#[cfg(test)]
mod pg_tests;
