//! Bounded Forge operation state: persist and query open operations.
//!
//! The [`ForgeOperations`] handle owns every prepared, terminal, and open-query
//! SQL workflow for one `(resource, family)` pair. Callers own the
//! [`TenantConn`] transaction and must commit or roll back explicitly; the
//! handle never issues transaction-control SQL.
//!
//! Every transition path acquires a transaction-scoped advisory lock on
//! `(tenant, resource, family, operation_id)` before reading state or appending
//! audit. The lock releases only when the caller's transaction commits or rolls
//! back, serializing concurrent first-Prepared calls for the same operation.
//!
//! This module co-writes audit and projection inside the caller's transaction:
//! the audit row commits atomically with the state mutation. Identical replay
//! is idempotent (no duplicate audit), while identity or timing mismatches fail
//! closed with [`SqlError::Conflict`].

// raw-query grep allowlist: forge internal tables post-date the sqlx offline cache; run `mise run sqlx:prepare` to promote to macros.

use std::str::FromStr;

use chrono::Utc;
use sqlx::types::Uuid;
use sqlx::{PgConnection, Postgres, Transaction};
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    AuditDetail, AuditEvent, AuditResult, ForgeIcebergRewritePhase, ForgeOrphanGcPhase,
    ForgeScribePromotionPhase, ForgeSnapshotExpirePhase, audit_detail_canonical_json,
};

use crate::queries::audit_outbox::{OperatorAudit, append_audit};
use crate::queries::oracle_reader_authority::{
    BIFROST_CATALOG_NAME, HeaderDbRow, MemberDbRow, digest32, invariant, sorted_members,
};
use crate::row_types::forge_operations::{
    ForgeClaimTable, ForgeExpirationAuthority, ForgeExpirationPreparation,
    ForgeExpirationResetOutcome, ForgeExpirationResetRequest, ForgeExpirationSettlementRequest,
    ForgeOperationFamily, ForgeOperationPhase, ForgeOperationStateRow, ForgeOperationStateSqlRow,
    ForgeOperationTransition, ForgeSnapshotExpirationClaim, ForgeSnapshotExpirationClaimSqlRow,
    OpenForgeOperationPage,
};
use crate::row_types::forge_tasks::{ForgeTaskEvidence, evidence_to_value};
use crate::row_types::oracle_reader_authority::{
    ProtectionFrontier, ProtectionMember, ProtectionRecord, TableAuthorityIdentity,
};
use crate::{OperatorPool, SqlError, TenantConn};

/// Scoped Forge operation state handle for one `(resource, family)`.
///
/// The handle retains the validated resource and family identity and is the
/// sole entry point for every transition and query on that identity's open
/// operations. Callers borrow a [`TenantConn`] for each method call; the
/// handle itself is stateless beyond its identity.
///
/// Every method validates that the supplied event's detail matches the retained
/// resource and family before touching the database.
pub struct ForgeOperations<'resource> {
    /// Canonical tenant/table resource identity validated at construction.
    resource: &'resource str,
    /// Closed operation family that bounds the allowed operations and detail
    /// variants.
    family: ForgeOperationFamily,
}

impl<'resource> ForgeOperations<'resource> {
    /// Creates a new handle scoped to `(resource, family)`.
    ///
    /// `resource` must be non-empty; the handle borrows the string for the
    /// lifetime of every method call.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when `resource` is empty.
    pub fn new(resource: &'resource str, family: ForgeOperationFamily) -> Result<Self, SqlError> {
        if resource.is_empty() {
            return Err(SqlError::Conflict {
                detail: "resource must be non-empty".to_owned(),
            });
        }
        Ok(Self { resource, family })
    }

    /// Appends one Prepared audit event and inserts or reopens the operation
    /// state row.
    ///
    /// The method validates the event against the handle's resource and family,
    /// acquires the advisory lock for `(resource, family, operation_id)`,
    /// selects the current state row under the lock, and applies the Prepared
    /// transition matrix:
    ///
    /// | Current row | Incoming Prepared | Result |
    /// |---|---|---|
    /// | Absent | Valid identity/detail | Append audit; insert Prepared; `Applied(seq)` |
    /// | Prepared | Canonical detail matches stored | No write; `AlreadyApplied` |
    /// | Reset | Any replay | `Conflict`; retry requires a new generation/operation |
    /// | Committed / Recovered | Canonical detail matches stored | No write; `AlreadyApplied(terminal_seq)` |
    /// | Any row | Conflicting detail/identity | `Conflict`; no durable change |
    ///
    /// The caller owns the transaction. On success the caller must commit to
    /// make the audit and state durable.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] on invalid event identity, transition
    /// collision, or detail mismatch.
    /// Returns [`SqlError::InvariantViolation`] when stored data is malformed.
    /// Returns [`SqlError::Query`] when locking, audit append, or projection IO
    /// fails.
    ///
    /// # Cancellation
    ///
    /// The caller owns the transaction and its transaction-scoped advisory
    /// lock. Cancellation drops this future without committing; dropping the
    /// caller transaction rolls back any audit append or projection mutation
    /// and releases the lock. Retrying the same canonical event is idempotent,
    /// returning the existing sequence after a prior commit or applying the
    /// transition after a rollback.
    pub async fn append_prepared(
        &self,
        conn: &mut TenantConn<'_>,
        event: &AuditEvent,
    ) -> Result<ForgeOperationTransition, SqlError> {
        let (detail, _phase, operation_id) = validate_event(
            event,
            self.resource,
            self.family,
            ForgeOperationPhase::Prepared,
        )?;

        self.acquire_operation_lock(conn.transaction(), operation_id)
            .await?;

        let row = self
            .select_state_for_update(conn.transaction(), operation_id)
            .await?;

        match row {
            None => {
                // Absent row: first Prepared for this operation.
                let seq = append_audit(conn, event).await?;
                self.insert_prepared(conn.transaction(), operation_id, &detail, seq)
                    .await?;
                Ok(ForgeOperationTransition::Applied { audit_seq: seq })
            }
            Some(sql_row) => {
                let state_row: ForgeOperationStateRow = sql_row.try_into()?;
                let event_canonical = audit_detail_canonical_json(&detail);
                let stored_canonical = audit_detail_canonical_json(&state_row.prepared_detail);

                if event_canonical != stored_canonical {
                    return Err(SqlError::Conflict {
                        detail: "prepared detail does not match stored prepared detail".to_owned(),
                    });
                }

                match state_row.phase {
                    ForgeOperationPhase::Prepared => {
                        // Idempotent replay: same operation already prepared.
                        Ok(ForgeOperationTransition::AlreadyApplied {
                            audit_seq: state_row.prepared_audit_seq,
                        })
                    }
                    ForgeOperationPhase::Reset => {
                        Err(SqlError::Conflict {
                            detail: "Reset Forge generation cannot be reopened; retry requires a new operation and output generation".to_owned(),
                        })
                    }
                    ForgeOperationPhase::Committed | ForgeOperationPhase::Recovered => {
                        // Terminal state: return the terminal sequence.
                        let terminal_seq = state_row.terminal_audit_seq.ok_or_else(|| {
                            SqlError::InvariantViolation {
                                detail: format!(
                                    "{:?} row missing terminal_audit_seq",
                                    state_row.phase
                                ),
                            }
                        })?;
                        Ok(ForgeOperationTransition::AlreadyApplied {
                            audit_seq: terminal_seq,
                        })
                    }
                }
            }
        }
    }

    /// Appends one terminal audit event and transitions the operation state
    /// from Prepared to the terminal phase.
    ///
    /// The method validates the event, acquires the advisory lock, selects the
    /// existing state row (must be Prepared), applies the terminal transition:
    ///
    /// | Current row | Incoming terminal | Result |
    /// |---|---|---|
    /// | Prepared | Valid family terminal | Append audit; update projection; `Applied(seq)` |
    /// | Same terminal phase | Identical canonical detail | No write; `AlreadyApplied` |
    /// | Absent, Reset, different terminal, or conflict | Any | `Conflict` |
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the state row is absent, not
    /// Prepared, or the detail mismatches.
    /// Returns [`SqlError::InvariantViolation`] when stored data is malformed.
    /// Returns [`SqlError::Query`] when locking, audit append, or projection IO
    /// fails.
    ///
    /// # Cancellation
    ///
    /// The caller owns the transaction and its transaction-scoped advisory
    /// lock. Cancellation cannot commit partial progress: dropping the caller
    /// transaction rolls back both the terminal audit append and state update
    /// and releases the lock. Retrying an identical committed transition is
    /// idempotent and returns its existing terminal sequence.
    pub async fn append_terminal(
        &self,
        conn: &mut TenantConn<'_>,
        event: &AuditEvent,
    ) -> Result<ForgeOperationTransition, SqlError> {
        let phase_suffix = extract_phase_suffix(&event.operation, self.family.operation_prefix())
            .ok_or_else(|| SqlError::Conflict {
            detail: format!(
                "operation {} does not match family prefix {}",
                event.operation,
                self.family.operation_prefix()
            ),
        })?;

        let terminal_phase =
            ForgeOperationPhase::from_str(phase_suffix).map_err(|_| SqlError::Conflict {
                detail: format!("unknown terminal phase in operation {}", event.operation),
            })?;

        if terminal_phase == ForgeOperationPhase::Prepared {
            return Err(SqlError::Conflict {
                detail: "append_terminal called with Prepared event; use append_prepared"
                    .to_owned(),
            });
        }

        // Reset is only valid for the scribe_promotion and iceberg_rewrite
        // families: they are the only ones whose external commit can be proven
        // not to have happened, making the prepared inputs visible again.
        if terminal_phase == ForgeOperationPhase::Reset
            && !matches!(
                self.family,
                ForgeOperationFamily::ScribePromotion | ForgeOperationFamily::IcebergRewrite
            )
        {
            return Err(SqlError::Conflict {
                detail: format!(
                    "Reset transition is not valid for family {}",
                    self.family.as_str()
                ),
            });
        }

        let (detail, _, operation_id) =
            validate_event(event, self.resource, self.family, terminal_phase)?;

        self.acquire_operation_lock(conn.transaction(), operation_id)
            .await?;

        let sql_row = self
            .select_state_for_update(conn.transaction(), operation_id)
            .await?
            .ok_or_else(|| SqlError::Conflict {
                detail: "cannot append terminal for absent operation state".to_owned(),
            })?;

        let state_row: ForgeOperationStateRow = sql_row.try_into()?;

        // If the operation is already in the same terminal phase with identical
        // canonical detail, it is an idempotent replay.
        if state_row.phase == terminal_phase {
            let event_canonical = audit_detail_canonical_json(&detail);
            let stored_canonical = audit_detail_canonical_json(&state_row.current_detail);
            if event_canonical == stored_canonical {
                let terminal_seq =
                    state_row
                        .terminal_audit_seq
                        .ok_or_else(|| SqlError::InvariantViolation {
                            detail: format!(
                                "{:?} state row missing terminal_audit_seq",
                                state_row.phase
                            ),
                        })?;
                return Ok(ForgeOperationTransition::AlreadyApplied {
                    audit_seq: terminal_seq,
                });
            }
        }

        // Only Prepared rows can accept a new terminal transition.
        if state_row.phase != ForgeOperationPhase::Prepared {
            return Err(SqlError::Conflict {
                detail: format!(
                    "cannot append terminal: operation is in {:?} state",
                    state_row.phase
                ),
            });
        }

        let seq = append_audit(conn, event).await?;
        self.apply_terminal(
            conn.transaction(),
            operation_id,
            &detail,
            terminal_phase,
            seq,
        )
        .await?;

        Ok(ForgeOperationTransition::Applied { audit_seq: seq })
    }

    /// Returns at most `cap` fully validated open (Prepared) operations.
    ///
    /// The method selects `cap + 1` state rows from the partial
    /// `forge_operation_state_open` index and validates every row (including
    /// the overflow sentinel). An `overflowed = true` page means the caller
    /// should narrow the resource/family scope or paginate. The read touches
    /// only the state projection: `vala.audit_outbox` is a delivery table with
    /// its own retention, and Forge recovery must not depend on it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when `cap` is zero.
    /// Returns [`SqlError::InvariantViolation`] when stored state fails
    /// decoding or identity validation.
    /// Returns [`SqlError::Query`] when the bounded state query fails.
    ///
    /// # Cancellation
    ///
    /// The read performs no durable writes and never acquires the Forge
    /// advisory lock. Cancellation leaves only the caller-owned read
    /// transaction to roll back or drop; retrying the same read is safe.
    pub async fn list_open(
        &self,
        conn: &mut TenantConn<'_>,
        cap: usize,
    ) -> Result<OpenForgeOperationPage, SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "cap must be greater than zero".to_owned(),
            });
        }

        let limit = i64::try_from(cap)
            .ok()
            .and_then(|c| c.checked_add(1))
            .ok_or_else(|| SqlError::Conflict {
                detail: "cap overflow".to_owned(),
            })?;

        let rows: Vec<ForgeOperationStateSqlRow> = sqlx::query_as(
            r#"
            SELECT *
              FROM vala.forge_operation_state
             WHERE data_tenant_id = wyrd.current_tenant()
               AND resource = $1
               AND family = $2
               AND phase = 'prepared'
             ORDER BY prepared_at, operation_id
             LIMIT $3
            "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;

        // Decode and validate every row, including the overflow sentinel.
        let mut decoded: Vec<ForgeOperationStateRow> = Vec::with_capacity(rows.len());
        for sql_row in rows {
            let state_row: ForgeOperationStateRow = sql_row.try_into()?;
            decoded.push(state_row);
        }

        let overflowed = decoded.len() > cap;
        decoded.truncate(cap);

        Ok(OpenForgeOperationPage {
            operations: decoded,
            overflowed,
        })
    }

    /// Reads one exact operation's validated state row by primary key.
    ///
    /// Snapshot-expiration reconciliation resolves its operation from the
    /// task-bound claim index and then needs that one operation's immutable
    /// prepared detail. This is the primary-key read for that step: no scan, no
    /// cap, and no candidate-equality heuristic. Like every other state read it
    /// touches only `vala.forge_operation_state`.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::InvariantViolation`] when the stored row fails
    /// decoding or identity validation, and [`SqlError::Query`] when the read
    /// fails.
    ///
    /// # Cancellation
    ///
    /// The read writes nothing; cancellation leaves no durable trace.
    pub async fn operation(
        &self,
        conn: &mut TenantConn<'_>,
        operation_id: Uuid,
    ) -> Result<Option<ForgeOperationStateRow>, SqlError> {
        let row: Option<ForgeOperationStateSqlRow> = sqlx::query_as(
            r"SELECT * FROM vala.forge_operation_state
               WHERE data_tenant_id = wyrd.current_tenant()
                 AND resource = $1 AND family = $2 AND operation_id = $3",
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        row.map(TryInto::try_into).transpose()
    }

    /// Lists a bounded page of terminal Reset operations as never-published proof.
    ///
    /// # Errors
    ///
    /// Returns conflict for a zero/overflowed cap, invariant errors for malformed
    /// durable details, or SQL errors while reading the tenant projection.
    ///
    /// # Cancellation
    ///
    /// This read-only operation has no durable partial progress.
    pub async fn list_reset(
        &self,
        conn: &mut TenantConn<'_>,
        cap: usize,
    ) -> Result<OpenForgeOperationPage, SqlError> {
        if cap == 0 {
            return Err(SqlError::Conflict {
                detail: "cap must be greater than zero".to_owned(),
            });
        }
        let limit = i64::try_from(cap)
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| SqlError::Conflict {
                detail: "cap overflow".to_owned(),
            })?;
        let rows: Vec<ForgeOperationStateSqlRow> = sqlx::query_as(
            "SELECT * FROM vala.forge_operation_state WHERE data_tenant_id=wyrd.current_tenant() AND resource=$1 AND family=$2 AND phase='reset' ORDER BY updated_at,operation_id LIMIT $3",
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(limit)
        .fetch_all(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;
        let mut decoded = rows
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<ForgeOperationStateRow>, _>>()?;
        let overflowed = decoded.len() > cap;
        decoded.truncate(cap);
        Ok(OpenForgeOperationPage {
            operations: decoded,
            overflowed,
        })
    }
}

// ---------------------------------------------------------------------------
// Private SQL helpers owned by ForgeOperations
// ---------------------------------------------------------------------------

impl<'resource> ForgeOperations<'resource> {
    /// Acquires a transaction-scoped advisory lock on `(resource, family, operation_id)`.
    ///
    /// Every transition path acquires this lock before selecting state or appending
    /// audit. The lock releases only when the caller's transaction commits or rolls
    /// back. This serializes concurrent first-Prepared calls for the same operation
    /// so exactly one writer sees the absent row.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Query`] when the lock query fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation before acquisition leaves no lock. After acquisition the
    /// lock belongs to the caller's transaction and releases only when that
    /// transaction commits, rolls back, or is dropped.
    async fn acquire_operation_lock(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<(), SqlError> {
        sqlx::query(
            r#"
        SELECT pg_advisory_xact_lock(
            hashtextextended(
                jsonb_build_array(
                    wyrd.current_tenant()::text,
                    $1::text,
                    $2::text,
                    $3::uuid::text
                )::text,
                0
            )
        )
        "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Selects the current state row for one operation under the existing
    /// transaction-level lock, or returns `None` when no row exists.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the query fails.
    ///
    /// # Cancellation
    ///
    /// Cancellation performs no writes. The caller-owned transaction retains
    /// any advisory lock until its own commit, rollback, or drop boundary.
    async fn select_state_for_update(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<Option<ForgeOperationStateSqlRow>, SqlError> {
        // We still use FOR UPDATE inside the existing advisory lock to guard
        // against concurrent upsert races. The advisory lock ensures serialized
        // access, and FOR UPDATE provides an additional row-level safety layer.
        let row: Option<ForgeOperationStateSqlRow> = sqlx::query_as(
            r#"
        SELECT state.*
          FROM vala.forge_operation_state AS state
         WHERE state.data_tenant_id = wyrd.current_tenant()
           AND state.resource = $1
           AND state.family = $2
           AND state.operation_id = $3
         FOR UPDATE OF state
        "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(SqlError::from)?;

        Ok(row)
    }

    /// Inserts a new Prepared state row for the given operation, reopening one
    /// that a previous pass released.
    ///
    /// A `reset` operation is released, not terminal: the same table and the
    /// same selection deterministically derive the same operation identity, so
    /// a retry after a release must be able to prepare again. The upsert
    /// reopens only a `reset` row; `committed` and `recovered` rows stay
    /// terminal and are refused by the caller's replay gate before this runs.
    ///
    /// Caller must hold the advisory lock and have committed the audit append
    /// in the same transaction.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the insert fails.
    ///
    /// # Cancellation
    ///
    /// The insert participates in the caller's transaction. Cancellation
    /// cannot commit the preceding audit append; dropping the transaction
    /// rolls back both, and an identical retry can safely start again.
    async fn insert_prepared(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
        detail: &AuditDetail,
        prepared_seq: i64,
    ) -> Result<(), SqlError> {
        let now = Utc::now();
        let detail_json =
            serde_json::to_value(detail).map_err(|e| SqlError::InvariantViolation {
                detail: format!("failed to serialize prepared detail: {e}"),
            })?;

        sqlx::query(
            r#"
        INSERT INTO vala.forge_operation_state
            (data_tenant_id, resource, family, operation_id, phase,
             prepared_detail, current_detail,
             prepared_audit_seq, terminal_audit_seq,
             prepared_at, updated_at)
        VALUES (wyrd.current_tenant(), $1, $2, $3, 'prepared',
                $4::jsonb, $4::jsonb,
                $5, NULL,
                $6, $6)
        ON CONFLICT (data_tenant_id, resource, family, operation_id) DO UPDATE
            SET phase = 'prepared',
                current_detail = EXCLUDED.prepared_detail,
                prepared_audit_seq = EXCLUDED.prepared_audit_seq,
                terminal_audit_seq = NULL,
                prepared_at = EXCLUDED.prepared_at,
                updated_at = EXCLUDED.updated_at
            WHERE vala.forge_operation_state.phase = 'reset'
        "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .bind(detail_json.to_string())
        .bind(prepared_seq)
        .bind(now)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;

        Ok(())
    }

    /// Applies a terminal transition to a Prepared operation.
    ///
    /// Updates `phase`, `current_detail`, `terminal_audit_seq`, and `updated_at`.
    /// The `prepared_detail`, `prepared_audit_seq`, and `prepared_at` remain
    /// unchanged.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError`] when the update fails.
    ///
    /// # Cancellation
    ///
    /// The update participates in the caller's transaction. Cancellation
    /// cannot commit only the terminal audit; dropping the transaction rolls
    /// back audit and terminal projection state together.
    async fn apply_terminal(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
        detail: &AuditDetail,
        terminal_phase: ForgeOperationPhase,
        terminal_seq: i64,
    ) -> Result<(), SqlError> {
        let now = Utc::now();
        let detail_json =
            serde_json::to_value(detail).map_err(|e| SqlError::InvariantViolation {
                detail: format!("failed to serialize terminal detail: {e}"),
            })?;

        sqlx::query(
            r#"
        UPDATE vala.forge_operation_state
           SET phase = $4,
               current_detail = $5::jsonb,
               terminal_audit_seq = $6,
               updated_at = $7
         WHERE data_tenant_id = wyrd.current_tenant()
           AND resource = $1
           AND family = $2
           AND operation_id = $3
        "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .bind(terminal_phase.as_str())
        .bind(detail_json.to_string())
        .bind(terminal_seq)
        .bind(now)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Event validation
// ---------------------------------------------------------------------------

/// Validates that an audit event matches the expected family, resource, and
/// phase, and returns the typed detail and the operation ID.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the event operation, detail variant, or
/// resource does not match the expected forge family and phase.
fn validate_event(
    event: &AuditEvent,
    expected_resource: &str,
    family: ForgeOperationFamily,
    expected_phase: ForgeOperationPhase,
) -> Result<(AuditDetail, ForgeOperationPhase, Uuid), SqlError> {
    let prefix = family.operation_prefix();

    // Validate the operation string has the correct prefix.
    if !event.operation.starts_with(prefix) {
        return Err(SqlError::Conflict {
            detail: format!(
                "operation {} does not match family prefix {}",
                event.operation, prefix
            ),
        });
    }

    // Extract and validate the phase suffix.
    let phase_str =
        extract_phase_suffix(&event.operation, prefix).ok_or_else(|| SqlError::Conflict {
            detail: format!(
                "cannot extract phase suffix from operation {}",
                event.operation
            ),
        })?;
    let phase = ForgeOperationPhase::from_str(phase_str).map_err(|_| SqlError::Conflict {
        detail: format!("unknown phase {phase_str} in operation {}", event.operation),
    })?;

    // Validate the phase matches what the method expects.
    if phase != expected_phase {
        return Err(SqlError::Conflict {
            detail: format!(
                "event phase {phase_str} does not match expected {:?}",
                expected_phase
            ),
        });
    }

    // Extract and validate the detail from the event.
    let detail = event.detail.as_ref().ok_or_else(|| SqlError::Conflict {
        detail: "event must carry a typed audit detail".to_owned(),
    })?;

    let (actual_op_id, actual_group) =
        extract_detail_identity(detail, family.expected_detail_kind())?;

    let detail_phase = extract_detail_phase(detail)?;
    if detail_phase != expected_phase {
        return Err(SqlError::Conflict {
            detail: format!(
                "detail phase {:?} does not match expected {:?}",
                detail_phase, expected_phase
            ),
        });
    }

    // Validate group matches resource.
    if actual_group != expected_resource {
        return Err(SqlError::Conflict {
            detail: format!(
                "detail group {} does not match resource {}",
                actual_group, expected_resource
            ),
        });
    }

    Ok((detail.clone(), phase, actual_op_id))
}

/// Extracts the phase suffix from an operation string after removing the
/// family-specific prefix and dot separator.
fn extract_phase_suffix<'a>(operation: &'a str, prefix: &str) -> Option<&'a str> {
    let dot_prefix = format!("{prefix}.");
    operation.strip_prefix(&dot_prefix)
}

/// Extracts the `operation_id` and `group` from the expected forge detail kind.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the detail is not the expected forge
/// variant.
fn extract_detail_identity<'a>(
    detail: &'a AuditDetail,
    expected_kind: &str,
) -> Result<(Uuid, &'a str), SqlError> {
    let (actual_kind, operation_id, group) = match detail {
        AuditDetail::ForgeScribePromotion {
            operation_id,
            group,
            ..
        } => ("forge_scribe_promotion", operation_id, group),
        AuditDetail::ForgeIcebergRewrite {
            operation_id,
            group,
            ..
        } => ("forge_iceberg_rewrite", operation_id, group),
        AuditDetail::ForgeSnapshotExpire {
            operation_id,
            group,
            ..
        } => ("forge_snapshot_expire", operation_id, group),
        AuditDetail::ForgeOrphanGc {
            operation_id,
            group,
            ..
        } => ("forge_orphan_gc", operation_id, group),
        _ => {
            return Err(SqlError::Conflict {
                detail: format!("expected forge detail kind {expected_kind}, got another variant"),
            });
        }
    };

    if actual_kind != expected_kind {
        return Err(SqlError::Conflict {
            detail: format!("expected forge detail kind {expected_kind}, got {actual_kind}"),
        });
    }

    Ok((*operation_id, group.as_str()))
}

/// Extracts the closed operation phase carried by a typed Forge detail.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when `detail` is not a Forge detail.
fn extract_detail_phase(detail: &AuditDetail) -> Result<ForgeOperationPhase, SqlError> {
    let phase = match detail {
        AuditDetail::ForgeScribePromotion { phase, .. } => match phase {
            ForgeScribePromotionPhase::Prepared => ForgeOperationPhase::Prepared,
            ForgeScribePromotionPhase::Committed => ForgeOperationPhase::Committed,
            ForgeScribePromotionPhase::Recovered => ForgeOperationPhase::Recovered,
            ForgeScribePromotionPhase::Reset => ForgeOperationPhase::Reset,
        },
        AuditDetail::ForgeIcebergRewrite { phase, .. } => match phase {
            ForgeIcebergRewritePhase::Prepared => ForgeOperationPhase::Prepared,
            ForgeIcebergRewritePhase::Committed => ForgeOperationPhase::Committed,
            ForgeIcebergRewritePhase::Recovered => ForgeOperationPhase::Recovered,
            ForgeIcebergRewritePhase::Reset => ForgeOperationPhase::Reset,
        },
        AuditDetail::ForgeSnapshotExpire { phase, .. } => match phase {
            ForgeSnapshotExpirePhase::Prepared => ForgeOperationPhase::Prepared,
            ForgeSnapshotExpirePhase::Committed => ForgeOperationPhase::Committed,
            ForgeSnapshotExpirePhase::Recovered => ForgeOperationPhase::Recovered,
        },
        AuditDetail::ForgeOrphanGc { phase, .. } => match phase {
            ForgeOrphanGcPhase::Prepared => ForgeOperationPhase::Prepared,
            ForgeOrphanGcPhase::Committed => ForgeOperationPhase::Committed,
            ForgeOrphanGcPhase::Recovered => ForgeOperationPhase::Recovered,
        },
        _ => {
            return Err(SqlError::Conflict {
                detail: "expected Forge detail for phase validation".to_owned(),
            });
        }
    };

    Ok(phase)
}

// ---------------------------------------------------------------------------
// Serialized snapshot expiration: claim lifecycle on the operator pool
// ---------------------------------------------------------------------------

/// Lock order every snapshot-expiration transaction below takes, in order:
///
/// 1. the current live Forge table lease row (fence assertion),
/// 2. the exact task/attempt/current-owner row,
/// 3. the table maintenance-authority row that serializes reader widening
///    against destructive maintenance for one tenant-qualified table,
/// 4. the operation state row (advisory lock, then `FOR UPDATE`),
/// 5. the claim rows for that operation, and
/// 6. the tenant audit chain head.
///
/// Every method here runs on the operator pool because the claim table grants
/// `INSERT`/`DELETE` to `wyrd_platform_admin` only; the transaction binds
/// `wyrd.current_tenant()` first so RLS-shaped predicates and the tenant audit
/// chain behave exactly as they do on a tenant connection.
impl ForgeOperations<'_> {
    /// Atomically prepares one snapshot-expiration selection.
    ///
    /// In one operator transaction this asserts the caller's live lease fence,
    /// pins the exact running attempt, takes the table's maintenance-authority
    /// row, refuses when any surviving reader protection frontier still covers
    /// a selected snapshot, appends the Prepared operation audit, inserts the
    /// Prepared operation state, claims every selected snapshot, and moves the
    /// task to Prepared with its evidence and audit.
    ///
    /// Replaying the identical preparation writes nothing and returns the
    /// existing prepared sequence. An identical selection whose previous pass
    /// was reset is prepared again, reopening that released operation; a
    /// committed or recovered operation is refused.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the family is not
    /// `snapshot_expire`, either audit event does not name this operation or
    /// task, the lease fence is lost, the task/attempt/owner/table identity
    /// does not match, a surviving protection frontier covers a selected
    /// snapshot, or the operation is already resolved.
    /// Returns [`SqlError::InvariantViolation`] when stored state is malformed.
    /// Returns [`SqlError::Query`] for statement failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the uncommitted operator transaction, releasing every
    /// lock and discarding audit, state, claims, and the task transition
    /// together. An identical retry is safe.
    pub async fn prepare_snapshot_expiration(
        &self,
        operator: &OperatorPool,
        tenant: DataTenantId,
        request: &ForgeExpirationPreparation<'_>,
    ) -> Result<ForgeOperationTransition, SqlError> {
        self.require_snapshot_expire()?;
        let (detail, _, operation_id) = validate_event(
            request.operation_event,
            self.resource,
            self.family,
            ForgeOperationPhase::Prepared,
        )?;
        let selected = selected_snapshot_ids(&detail)?;
        validate_task_event(request.task_event, request.authority.task_id, "prepared")?;
        request.evidence.validate(false)?;

        let mut tx = operator.pool().begin().await.map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        assert_lease_fence(&mut tx, request.authority).await?;
        lock_expiration_task(
            &mut tx,
            request.authority,
            request.table,
            &["running", "prepared"],
        )
        .await?;
        let identity = lock_table_authority(&mut tx, tenant, request.table).await?;
        refuse_protected_snapshots(&mut tx, &identity, &selected).await?;

        self.acquire_operation_lock(&mut tx, operation_id).await?;
        if let Some(sql_row) = self.select_state_for_update(&mut tx, operation_id).await? {
            let state_row: ForgeOperationStateRow = sql_row.try_into()?;
            if audit_detail_canonical_json(&detail)
                != audit_detail_canonical_json(&state_row.prepared_detail)
                || !matches!(
                    state_row.phase,
                    ForgeOperationPhase::Prepared | ForgeOperationPhase::Reset
                )
            {
                return Err(SqlError::Conflict {
                    detail: "snapshot expiration preparation does not replay the stored operation"
                        .to_owned(),
                });
            }
            if state_row.phase == ForgeOperationPhase::Prepared {
                return Ok(ForgeOperationTransition::AlreadyApplied {
                    audit_seq: state_row.prepared_audit_seq,
                });
            }
        }

        let seq = OperatorAudit::new(tenant, &mut tx)
            .append(request.operation_event)
            .await?;
        self.insert_prepared(&mut tx, operation_id, &detail, seq)
            .await?;
        self.insert_claims(&mut tx, operation_id, &selected, request)
            .await?;
        let changed = sqlx::query(
            "UPDATE vala.forge_tasks SET state='prepared',evidence=$4::jsonb,updated_at=statement_timestamp() WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND state='running' AND attempt_id=$2 AND claimed_by=$3",
        )
        .bind(request.authority.task_id)
        .bind(request.authority.attempt_id)
        .bind(request.authority.worker_id)
        .bind(evidence_to_value(request.evidence).to_string())
        .execute(&mut *tx)
        .await
        .map_err(SqlError::from)?
        .rows_affected();
        exact_one(changed, "snapshot expiration prepared task transition")?;
        OperatorAudit::new(tenant, &mut tx)
            .append(request.task_event)
            .await?;

        tx.commit().await.map_err(SqlError::from)?;
        Ok(ForgeOperationTransition::Applied { audit_seq: seq })
    }

    /// Atomically settles one prepared snapshot expiration as Committed or
    /// Recovered.
    ///
    /// The settling worker need not be the preparing one: the claim rows carry
    /// the preparation identity as historical evidence, while the fence, task
    /// attempt, and owner are revalidated against the caller's *current*
    /// authority. Settlement appends the terminal operation audit, resolves the
    /// operation state, deletes every claim, moves the task to Succeeded with
    /// its final cleanup evidence, appends the task audit, and advances the
    /// table's planning demand — all in one transaction.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the family, audit identity, lease
    /// fence, task identity, table identity, operation phase, or the claim set
    /// does not exactly match the prepared selection.
    /// Returns [`SqlError::InvariantViolation`] when stored state is malformed.
    /// Returns [`SqlError::Query`] for statement failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the uncommitted transaction; the operation stays
    /// Prepared with its claims intact and the identical settlement can retry.
    pub async fn settle_snapshot_expiration(
        &self,
        operator: &OperatorPool,
        tenant: DataTenantId,
        request: &ForgeExpirationSettlementRequest<'_>,
    ) -> Result<ForgeOperationTransition, SqlError> {
        self.require_snapshot_expire()?;
        let terminal_phase = request.settlement.phase();
        let (detail, _, operation_id) = validate_event(
            request.operation_event,
            self.resource,
            self.family,
            terminal_phase,
        )?;
        validate_task_event(request.task_event, request.authority.task_id, "succeeded")?;
        request.evidence.validate(false)?;

        let mut tx = operator.pool().begin().await.map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        assert_lease_fence(&mut tx, request.authority).await?;
        let task_state = task_state(&mut tx, request.authority.task_id).await?;
        lock_table_authority(&mut tx, tenant, request.table).await?;
        self.acquire_operation_lock(&mut tx, operation_id).await?;
        let state_row: ForgeOperationStateRow = self
            .select_state_for_update(&mut tx, operation_id)
            .await?
            .ok_or_else(|| SqlError::Conflict {
                detail: "cannot settle an absent snapshot expiration".to_owned(),
            })?
            .try_into()?;
        let claims = self.claims_for_operation(&mut tx, operation_id).await?;

        if state_row.phase == terminal_phase
            && audit_detail_canonical_json(&detail)
                == audit_detail_canonical_json(&state_row.current_detail)
            && task_state == "succeeded"
            && claims.is_empty()
        {
            let terminal_seq =
                state_row
                    .terminal_audit_seq
                    .ok_or_else(|| SqlError::InvariantViolation {
                        detail: "settled snapshot expiration is missing its terminal audit seq"
                            .to_owned(),
                    })?;
            return Ok(ForgeOperationTransition::AlreadyApplied {
                audit_seq: terminal_seq,
            });
        }

        self.require_resolvable_prepared(&state_row, task_state.as_str())?;
        self.require_claim_identity(
            &claims,
            request.authority.task_id,
            request.authority.attempt_id,
            request.table,
            &state_row.prepared_detail,
        )?;
        lock_expiration_task(&mut tx, request.authority, request.table, &["prepared"]).await?;

        let seq = OperatorAudit::new(tenant, &mut tx)
            .append(request.operation_event)
            .await?;
        self.apply_terminal(&mut tx, operation_id, &detail, terminal_phase, seq)
            .await?;
        self.delete_claims(&mut tx, operation_id).await?;
        self.resolve_task(
            &mut tx,
            tenant,
            "succeeded",
            request.authority,
            request.task_event,
            Some(request.evidence),
        )
        .await?;
        advance_planning_demand(&mut tx, tenant, request.table).await?;

        tx.commit().await.map_err(SqlError::from)?;
        Ok(ForgeOperationTransition::Applied { audit_seq: seq })
    }

    /// Atomically releases one prepared snapshot expiration that never
    /// committed.
    ///
    /// The reset is internal: `ForgeSnapshotExpirePhase` has no public Reset
    /// variant, so the state row keeps its immutable Prepared detail as
    /// `current_detail` and records the release only through the `reset` column
    /// phase and its terminal audit sequence. The same transaction deletes every
    /// claim, cancels the task, and advances the table's planning demand so the
    /// selection can be recomputed against fresh reader protection.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the family, audit identity, lease
    /// fence, task identity, table identity, operation phase, or claim set does
    /// not exactly match the prepared selection.
    /// Returns [`SqlError::InvariantViolation`] when stored state is malformed.
    /// Returns [`SqlError::Query`] for statement failures.
    ///
    /// # Cancellation
    ///
    /// Cancellation drops the uncommitted transaction; the operation stays
    /// Prepared with its claims intact and the identical reset can retry.
    pub async fn reset_snapshot_expiration(
        &self,
        operator: &OperatorPool,
        tenant: DataTenantId,
        request: &ForgeExpirationResetRequest<'_>,
    ) -> Result<ForgeExpirationResetOutcome, SqlError> {
        self.require_snapshot_expire()?;
        let operation_id = validate_reset_event(request.operation_event, self.resource)?;
        validate_task_event(request.task_event, request.authority.task_id, "cancelled")?;

        let mut tx = operator.pool().begin().await.map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        assert_lease_fence(&mut tx, request.authority).await?;
        let task_state = task_state(&mut tx, request.authority.task_id).await?;
        lock_table_authority(&mut tx, tenant, request.table).await?;
        self.acquire_operation_lock(&mut tx, operation_id).await?;
        let state_row: ForgeOperationStateRow = self
            .select_state_for_update(&mut tx, operation_id)
            .await?
            .ok_or_else(|| SqlError::Conflict {
                detail: "cannot reset an absent snapshot expiration".to_owned(),
            })?
            .try_into()?;
        let claims = self.claims_for_operation(&mut tx, operation_id).await?;

        if state_row.phase == ForgeOperationPhase::Reset
            && state_row.terminal_audit_seq.is_some()
            && task_state == "cancelled"
            && claims.is_empty()
        {
            return Ok(ForgeExpirationResetOutcome::AlreadyApplied);
        }

        self.require_resolvable_prepared(&state_row, task_state.as_str())?;
        self.require_claim_identity(
            &claims,
            request.authority.task_id,
            request.authority.attempt_id,
            request.table,
            &state_row.prepared_detail,
        )?;
        if audit_detail_canonical_json(request.operation_event.detail.as_ref().ok_or_else(
            || SqlError::Conflict {
                detail: "reset event must carry the prepared detail".to_owned(),
            },
        )?) != audit_detail_canonical_json(&state_row.prepared_detail)
        {
            return Err(SqlError::Conflict {
                detail: "snapshot expiration reset must carry the immutable prepared detail"
                    .to_owned(),
            });
        }
        lock_expiration_task(&mut tx, request.authority, request.table, &["prepared"]).await?;

        let seq = OperatorAudit::new(tenant, &mut tx)
            .append(request.operation_event)
            .await?;
        self.apply_terminal(
            &mut tx,
            operation_id,
            &state_row.prepared_detail,
            ForgeOperationPhase::Reset,
            seq,
        )
        .await?;
        self.delete_claims(&mut tx, operation_id).await?;
        self.resolve_task(
            &mut tx,
            tenant,
            "cancelled",
            request.authority,
            request.task_event,
            None,
        )
        .await?;
        let demand_generation = advance_planning_demand(&mut tx, tenant, request.table).await?;

        tx.commit().await.map_err(SqlError::from)?;
        Ok(ForgeExpirationResetOutcome::Applied { demand_generation })
    }

    /// Returns this task's unresolved claims, letting reconciliation find the
    /// exact operation a crashed attempt prepared without re-deriving it.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Query`] when the task-bound index read fails.
    ///
    /// # Cancellation
    ///
    /// The read writes nothing; cancellation leaves no durable trace.
    pub async fn claims_for_task(
        &self,
        operator: &OperatorPool,
        tenant: DataTenantId,
        task_id: Uuid,
    ) -> Result<Vec<ForgeSnapshotExpirationClaim>, SqlError> {
        self.require_snapshot_expire()?;
        let mut tx = operator.pool().begin().await.map_err(SqlError::from)?;
        bind_tenant(&mut tx, tenant).await?;
        let rows: Vec<ForgeSnapshotExpirationClaimSqlRow> = sqlx::query_as(
            "SELECT resource,operation_id,snapshot_id,task_id,attempt_id,worker_id,lease_key,lease_fencing_token,table_uid,catalog_name,namespace_name,table_name,table_uuid FROM vala.forge_snapshot_expiration_claims WHERE data_tenant_id=wyrd.current_tenant() AND task_id=$1 ORDER BY operation_id,snapshot_id",
        )
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(SqlError::from)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }
}

// ---------------------------------------------------------------------------
// Snapshot-expiration claim validation and private helpers
// ---------------------------------------------------------------------------

impl ForgeOperations<'_> {
    /// Rejects a snapshot-expiration workflow invoked on another family.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] unless the handle owns `snapshot_expire`.
    fn require_snapshot_expire(&self) -> Result<(), SqlError> {
        if self.family == ForgeOperationFamily::SnapshotExpire {
            Ok(())
        } else {
            Err(SqlError::Conflict {
                detail: format!(
                    "snapshot expiration claims are not valid for family {}",
                    self.family.as_str()
                ),
            })
        }
    }

    /// Requires every claim row to reproduce exactly one prepared operation and
    /// its immutable preparation identity.
    ///
    /// Claim rows are historical evidence written once, at preparation. A
    /// takeover changes *current* execution authority, never that evidence, so
    /// the historical worker, lease key, and lease fence are required only to
    /// agree across rows and are deliberately never compared with the caller's
    /// live authority. Everything that identifies *which* operation the rows
    /// belong to — resource, operation, table, originating task and attempt,
    /// and the exact selected snapshot set — must match both the current
    /// workflow and the stored Prepared detail before any row is trusted.
    ///
    /// `family` is deliberately neither projected nor revalidated: the table's
    /// `CHECK (family = 'snapshot_expire')`, the family-qualified statements in
    /// this module, and [`Self::require_snapshot_expire`] already make a family
    /// contradiction unreachable.
    ///
    /// Settlement, reset, and Forge reconciliation share this one
    /// implementation so a claim-identity rule cannot drift between them.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when the claim set is empty, any two rows
    /// disagree on their prepared identity, a row does not name this resource,
    /// operation, table, task, or attempt, or the sorted snapshot IDs are not
    /// exactly the prepared selection.
    pub fn require_claim_identity(
        &self,
        claims: &[ForgeSnapshotExpirationClaim],
        task_id: Uuid,
        attempt_id: Uuid,
        table: &ForgeClaimTable,
        prepared_detail: &AuditDetail,
    ) -> Result<(), SqlError> {
        let Some(first) = claims.first() else {
            return Err(SqlError::Conflict {
                detail: "snapshot expiration claim set is empty".to_owned(),
            });
        };
        if claims.iter().any(|claim| {
            claim.resource != first.resource
                || claim.operation_id != first.operation_id
                || claim.table != first.table
                || claim.prepared_by != first.prepared_by
        }) {
            return Err(SqlError::Conflict {
                detail: "snapshot expiration claim rows disagree on their prepared identity"
                    .to_owned(),
            });
        }
        let AuditDetail::ForgeSnapshotExpire { operation_id, .. } = prepared_detail else {
            return Err(SqlError::Conflict {
                detail: "prepared detail is not a snapshot expiration".to_owned(),
            });
        };
        if first.resource != self.resource
            || first.operation_id != *operation_id
            || &first.table != table
            || first.prepared_by.task_id != task_id
            || first.prepared_by.attempt_id != attempt_id
        {
            return Err(SqlError::Conflict {
                detail: "snapshot expiration claims do not reproduce the prepared operation"
                    .to_owned(),
            });
        }
        let mut claimed: Vec<i64> = claims.iter().map(|claim| claim.snapshot_id).collect();
        claimed.sort_unstable();
        if claimed != selected_snapshot_ids(prepared_detail)? {
            return Err(SqlError::Conflict {
                detail: "snapshot expiration claims do not reproduce the prepared selection"
                    .to_owned(),
            });
        }
        Ok(())
    }

    /// Requires an unresolved Prepared operation whose task is still Prepared.
    ///
    /// Claim identity is a separate concern owned by
    /// [`Self::require_claim_identity`]; this check answers only whether the
    /// durable phase pair still admits a terminal transition.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] when the operation is already resolved or
    /// the task is not Prepared.
    fn require_resolvable_prepared(
        &self,
        state_row: &ForgeOperationStateRow,
        task_state: &str,
    ) -> Result<(), SqlError> {
        if state_row.phase != ForgeOperationPhase::Prepared || task_state != "prepared" {
            return Err(SqlError::Conflict {
                detail: format!(
                    "snapshot expiration is not resolvable: operation {:?}, task {task_state}",
                    state_row.phase
                ),
            });
        }
        Ok(())
    }

    /// Inserts one immutable claim row per selected snapshot.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when the insert violates a constraint,
    /// including the primary key another table-local winner already holds.
    async fn insert_claims(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
        selected: &[i64],
        request: &ForgeExpirationPreparation<'_>,
    ) -> Result<(), SqlError> {
        sqlx::query(
            "INSERT INTO vala.forge_snapshot_expiration_claims (data_tenant_id,resource,family,operation_id,snapshot_id,task_id,attempt_id,worker_id,lease_key,lease_fencing_token,table_uid,catalog_name,namespace_name,table_name,table_uuid) SELECT wyrd.current_tenant(),$1,$2,$3,s,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14 FROM unnest($4::bigint[]) AS s",
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .bind(selected)
        .bind(request.authority.task_id)
        .bind(request.authority.attempt_id)
        .bind(request.authority.worker_id)
        .bind(&request.authority.lease_key)
        .bind(request.authority.lease_fencing_token)
        .bind(request.table.table_uid.as_slice())
        .bind(&request.table.catalog_name)
        .bind(&request.table.namespace_name)
        .bind(&request.table.table_name)
        .bind(request.table.table_uuid)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Reads one operation's unresolved claim rows in ascending snapshot order.
    ///
    /// The full immutable projection is read rather than snapshot IDs alone so
    /// that [`Self::require_claim_identity`] can prove every row reproduces the
    /// same prepared operation before any row's payload is trusted.
    ///
    /// The read takes no row lock: claims are immutable, and the operation's
    /// advisory lock plus its `FOR UPDATE` state row already serialize the only
    /// transaction that may insert or delete them.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when the read fails and
    /// [`SqlError::InvariantViolation`] when a stored row is malformed.
    async fn claims_for_operation(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<Vec<ForgeSnapshotExpirationClaim>, SqlError> {
        let rows: Vec<ForgeSnapshotExpirationClaimSqlRow> = sqlx::query_as(
            "SELECT resource,operation_id,snapshot_id,task_id,attempt_id,worker_id,lease_key,lease_fencing_token,table_uid,catalog_name,namespace_name,table_name,table_uuid FROM vala.forge_snapshot_expiration_claims WHERE data_tenant_id=wyrd.current_tenant() AND resource=$1 AND family=$2 AND operation_id=$3 ORDER BY snapshot_id",
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(SqlError::from)?;
        rows.into_iter().map(TryInto::try_into).collect()
    }

    /// Deletes every claim an operation still holds as part of resolving it.
    ///
    /// # Errors
    /// Returns [`SqlError::Query`] when the delete fails.
    async fn delete_claims(
        &self,
        conn: &mut PgConnection,
        operation_id: Uuid,
    ) -> Result<(), SqlError> {
        sqlx::query(
            "DELETE FROM vala.forge_snapshot_expiration_claims WHERE data_tenant_id=wyrd.current_tenant() AND resource=$1 AND family=$2 AND operation_id=$3",
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .execute(&mut *conn)
        .await
        .map_err(SqlError::from)?;
        Ok(())
    }

    /// Applies the exact Prepared-to-terminal task transition and its audit.
    ///
    /// # Errors
    /// Returns [`SqlError::Conflict`] unless exactly one row matched the exact
    /// task, attempt, and current owner, and [`SqlError`] for audit failures.
    async fn resolve_task(
        &self,
        conn: &mut Transaction<'_, Postgres>,
        tenant: DataTenantId,
        next: &str,
        authority: &ForgeExpirationAuthority,
        event: &AuditEvent,
        evidence: Option<&ForgeTaskEvidence>,
    ) -> Result<(), SqlError> {
        let encoded = evidence.map(|value| evidence_to_value(value).to_string());
        let changed = sqlx::query(
            "UPDATE vala.forge_tasks SET state=$4,evidence=COALESCE($5::jsonb,evidence),attempt_id=NULL,claimed_by=NULL,claim_expires_at=NULL,watermark_snapshot_id=NULL,watermark_timestamp_ms=NULL,updated_at=statement_timestamp() WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND state='prepared' AND attempt_id=$2 AND claimed_by=$3",
        )
        .bind(authority.task_id)
        .bind(authority.attempt_id)
        .bind(authority.worker_id)
        .bind(next)
        .bind(&encoded)
        .execute(&mut **conn)
        .await
        .map_err(SqlError::from)?
        .rows_affected();
        exact_one(changed, "snapshot expiration task resolution")?;
        OperatorAudit::new(tenant, conn).append(event).await?;
        Ok(())
    }
}

/// Binds `wyrd.current_tenant()` on an operator transaction so tenant-shaped
/// predicates, the audit chain, and RLS `WITH CHECK` clauses behave exactly as
/// they do on a [`TenantConn`].
///
/// # Errors
/// Returns [`SqlError::Query`] when the binding statement fails.
pub(crate) async fn bind_tenant(
    tx: &mut Transaction<'_, Postgres>,
    tenant: DataTenantId,
) -> Result<(), SqlError> {
    sqlx::query(wyrd_sql::tenant_conn::BIND_CURRENT_TENANT_SQL)
        .bind(tenant.to_string())
        .execute(&mut **tx)
        .await
        .map_err(SqlError::from)?;
    Ok(())
}

/// Locks the caller's maintenance lease row and proves it still owns the fence.
///
/// `vala.maintenance_leases` is a cross-tenant control-plane table with no RLS,
/// and the SECURITY DEFINER assertion function is granted to `wyrd_app` only,
/// so the operator path takes the same `FOR UPDATE` lock directly.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when the fence is lost and
/// [`SqlError::Query`] when the statement fails.
pub(crate) async fn assert_lease_fence(
    tx: &mut Transaction<'_, Postgres>,
    authority: &ForgeExpirationAuthority,
) -> Result<(), SqlError> {
    let owned: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM vala.maintenance_leases WHERE lease_key=$1 AND owner=$2 AND fencing_token=$3 AND expires_at>clock_timestamp() FOR UPDATE)",
    )
    .bind(&authority.lease_key)
    .bind(authority.worker_id)
    .bind(authority.lease_fencing_token)
    .fetch_one(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    if owned {
        Ok(())
    } else {
        Err(SqlError::InvariantViolation {
            detail: format!("maintenance lease fence lost for `{}`", authority.lease_key),
        })
    }
}

/// Reads one Forge task's current state under a row lock.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the task does not exist for this tenant
/// and [`SqlError::Query`] when the statement fails.
async fn task_state(tx: &mut Transaction<'_, Postgres>, task_id: Uuid) -> Result<String, SqlError> {
    sqlx::query_scalar(
        "SELECT state FROM vala.forge_tasks WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND strategy='snapshot_expiry' FOR UPDATE",
    )
    .bind(task_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?
    .ok_or_else(|| SqlError::Conflict {
        detail: "snapshot expiration names no such Forge task for this tenant".to_owned(),
    })
}

/// Pins the exact snapshot-expiry task, attempt, current owner, unexpired
/// claim, and registered table under a row lock, in one of `expected_states`.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when no row matches that exact identity.
async fn lock_expiration_task(
    tx: &mut Transaction<'_, Postgres>,
    authority: &ForgeExpirationAuthority,
    table: &ForgeClaimTable,
    expected_states: &[&str],
) -> Result<(), SqlError> {
    let states: Vec<String> = expected_states.iter().map(|s| (*s).to_owned()).collect();
    let matched: Option<Uuid> = sqlx::query_scalar(
        "SELECT task_id FROM vala.forge_tasks WHERE task_id=$1 AND data_tenant_id=wyrd.current_tenant() AND strategy='snapshot_expiry' AND state=ANY($2) AND attempt_id=$3 AND claimed_by=$4 AND claim_expires_at>statement_timestamp() AND catalog_name=$5 AND namespace_name=$6 AND table_name=$7 FOR UPDATE",
    )
    .bind(authority.task_id)
    .bind(&states)
    .bind(authority.attempt_id)
    .bind(authority.worker_id)
    .bind(&table.catalog_name)
    .bind(&table.namespace_name)
    .bind(&table.table_name)
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    if matched.is_some() {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: format!(
                "snapshot expiration did not match an exact {expected_states:?} task, attempt, owner, and table"
            ),
        })
    }
}

/// Takes the table's maintenance-authority row, the one-row-per-table boundary
/// that gives reader widening and destructive maintenance a single winner.
///
/// Shared by every fenced Forge maintenance workflow that must exclude reader
/// widening for the duration of its own transaction.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the table has no authority row or its
/// registered UID disagrees with the request, and [`SqlError`] on statement or
/// identity-validation failure.
pub(crate) async fn lock_table_authority(
    tx: &mut Transaction<'_, Postgres>,
    tenant: DataTenantId,
    table: &ForgeClaimTable,
) -> Result<TableAuthorityIdentity, SqlError> {
    let registered: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT table_uid FROM vala.bifrost_table_maintenance_authority WHERE data_tenant_id=wyrd.current_tenant() AND catalog_name=$1 AND namespace_name=$2 AND table_name=$3 FOR UPDATE",
    )
    .bind(&table.catalog_name)
    .bind(&table.namespace_name)
    .bind(&table.table_name)
    .fetch_optional(&mut **tx)
    .await
    .map_err(SqlError::from)?;
    let registered = registered.ok_or_else(|| SqlError::Conflict {
        detail: "Forge maintenance names a table with no maintenance authority row".to_owned(),
    })?;
    if registered.as_slice() != table.table_uid.as_slice() {
        return Err(SqlError::Conflict {
            detail: "Forge maintenance table UID disagrees with the registered table".to_owned(),
        });
    }
    let identity = TableAuthorityIdentity {
        tenant,
        table_uid: table.table_uid,
        catalog_name: table.catalog_name.clone(),
        namespace_name: table.namespace_name.clone(),
        table_name: table.table_name.clone(),
    };
    identity.validate(BIFROST_CATALOG_NAME)?;
    Ok(identity)
}

/// Refuses the whole preparation when any surviving reader protection frontier
/// still covers a selected snapshot.
///
/// The refusal is deliberate: `operation_id` is derived from the exact
/// selection and the Prepared audit detail is immutable, so silently narrowing
/// the selection here would invalidate the identity the caller committed to.
/// Coverage is proven ancestry-path membership — Iceberg remains the only
/// ancestry authority, so this never re-derives lineage in SQL.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when a frontier covers a selected snapshot,
/// and [`SqlError::InvariantViolation`] when a stored protection fails
/// validation.
async fn refuse_protected_snapshots(
    tx: &mut Transaction<'_, Postgres>,
    identity: &TableAuthorityIdentity,
    selected: &[i64],
) -> Result<(), SqlError> {
    let records = list_table_protection_in_operator_tx(tx, identity).await?;
    for record in &records {
        if let Some(covered) = selected
            .iter()
            .copied()
            .find(|snapshot| record.frontier.covers(*snapshot))
        {
            return Err(SqlError::Conflict {
                detail: format!(
                    "snapshot {covered} is still covered by a surviving reader protection frontier"
                ),
            });
        }
    }
    Ok(())
}

/// Reads and validates every epoch's complete protection record for one table
/// inside the preparation's own operator transaction.
///
/// Preparation already holds the table's `bifrost_table_maintenance_authority`
/// row lock on an operator transaction whose `wyrd.current_tenant()` binding is
/// established, and the frontier must be read under that same lock: a
/// protection published between an earlier read and the lock would otherwise be
/// invisible. The statements, ordering, and row validation mirror the Oracle
/// owner's tenant-connection reads exactly — same tenant predicate, same
/// `node_id, fencing_token` epoch order, same `protected_snapshot_id` member
/// order, same header-identity, digest, and domain validation.
///
/// # Errors
/// Returns [`SqlError::InvariantViolation`] when a stored header names another
/// table, when a header or member fails validation, or when a header disappears
/// inside this transaction, and [`SqlError`] when a statement fails.
pub(crate) async fn list_table_protection_in_operator_tx(
    tx: &mut Transaction<'_, Postgres>,
    identity: &TableAuthorityIdentity,
) -> Result<Vec<ProtectionRecord>, SqlError> {
    identity.validate(BIFROST_CATALOG_NAME)?;
    let epochs: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
        r"
        SELECT node_id, fencing_token
          FROM vala.oracle_table_protections
         WHERE data_tenant_id = wyrd.current_tenant()
           AND table_uid = $1
         ORDER BY node_id, fencing_token
        ",
    )
    .bind(identity.table_uid.as_slice())
    .fetch_all(&mut **tx)
    .await
    .map_err(SqlError::from)?;

    let mut records = Vec::with_capacity(epochs.len());
    for (node_id, fencing_token) in epochs {
        let header: Option<HeaderDbRow> = sqlx::query_as(
            r"
            SELECT catalog_name, namespace_name, table_name, revision,
                   frontier_encoding_version, frontier_digest, updated_at
              FROM vala.oracle_table_protections
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND node_id = $2
               AND fencing_token = $3
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .fetch_optional(&mut **tx)
        .await
        .map_err(SqlError::from)?;
        let header = header.ok_or_else(|| {
            invariant("reader protection header disappeared inside one transaction")
        })?;
        if header.catalog_name != identity.catalog_name
            || header.namespace_name != identity.namespace_name
            || header.table_name != identity.table_name
        {
            return Err(invariant(
                "stored reader protection names a different table than the caller",
            ));
        }

        let member_rows: Vec<MemberDbRow> = sqlx::query_as(
            r"
            SELECT protected_snapshot_id, protected_snapshot_timestamp_ms,
                   retained_head_snapshot_id, retained_head_timestamp_ms,
                   ancestry_path, ancestry_digest_version, ancestry_digest
              FROM vala.oracle_table_protection_members
             WHERE data_tenant_id = wyrd.current_tenant()
               AND table_uid = $1
               AND node_id = $2
               AND fencing_token = $3
             ORDER BY protected_snapshot_id
            ",
        )
        .bind(identity.table_uid.as_slice())
        .bind(node_id)
        .bind(fencing_token)
        .fetch_all(&mut **tx)
        .await
        .map_err(SqlError::from)?;
        let mut members = Vec::with_capacity(member_rows.len());
        for row in member_rows {
            let member = ProtectionMember {
                protected_snapshot_id: row.protected_snapshot_id,
                protected_snapshot_timestamp_ms: row.protected_snapshot_timestamp_ms,
                retained_head_snapshot_id: row.retained_head_snapshot_id,
                retained_head_timestamp_ms: row.retained_head_timestamp_ms,
                ancestry_path: row.ancestry_path,
                ancestry_digest_version: row.ancestry_digest_version,
                ancestry_digest: digest32(row.ancestry_digest)?,
            };
            member.validate(identity)?;
            members.push(member);
        }

        let record = ProtectionRecord {
            revision: header.revision,
            frontier_encoding_version: header.frontier_encoding_version,
            frontier_digest: digest32(header.frontier_digest)?,
            updated_at: header.updated_at,
            frontier: ProtectionFrontier {
                members: sorted_members(members),
            },
        };
        record.validate(identity)?;
        records.push(record);
    }
    Ok(records)
}

/// Advances the table's periodic planning demand so a resolved expiration is
/// never durable without a request to replan against the new metadata.
///
/// # Errors
/// Returns [`SqlError::Query`] when the upsert fails.
async fn advance_planning_demand(
    tx: &mut Transaction<'_, Postgres>,
    tenant: DataTenantId,
    table: &ForgeClaimTable,
) -> Result<i64, SqlError> {
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO vala.forge_planning_demands (data_tenant_id,catalog_name,namespace_name,table_name,last_source) VALUES ($1,$2,$3,$4,'periodic') ON CONFLICT (data_tenant_id,catalog_name,namespace_name,table_name) DO UPDATE SET last_requested_at=statement_timestamp(),last_source='periodic',generation=vala.forge_planning_demands.generation+1,acknowledged_snapshot_id=NULL,acknowledged_commit_count=NULL RETURNING generation",
    )
    .bind(tenant.as_uuid())
    .bind(&table.catalog_name)
    .bind(&table.namespace_name)
    .bind(&table.table_name)
    .fetch_one(&mut **tx)
    .await
    .map_err(SqlError::from)
}

/// Returns the exact ascending snapshot selection carried by a snapshot-expiry
/// detail.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the detail is another variant or the
/// selection is empty, unsorted, or repeats a snapshot.
fn selected_snapshot_ids(detail: &AuditDetail) -> Result<Vec<i64>, SqlError> {
    let AuditDetail::ForgeSnapshotExpire {
        selected_snapshot_ids,
        ..
    } = detail
    else {
        return Err(SqlError::Conflict {
            detail: "expected a snapshot-expiry audit detail".to_owned(),
        });
    };
    if selected_snapshot_ids.is_empty()
        || selected_snapshot_ids
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(SqlError::Conflict {
            detail: "snapshot expiration selection must be non-empty and strictly ascending"
                .to_owned(),
        });
    }
    Ok(selected_snapshot_ids.clone())
}

/// Validates that a task lifecycle audit event names this exact task and next
/// state, mirroring the tenant-connection task workflows.
///
/// # Errors
/// Returns [`SqlError::Conflict`] for mismatched audit identity.
fn validate_task_event(event: &AuditEvent, task_id: Uuid, next: &str) -> Result<(), SqlError> {
    if event.resource != format!("forge-task:{task_id}")
        || event.operation != format!("forge.task.{next}")
    {
        return Err(SqlError::Conflict {
            detail: "Forge task audit event does not match task identity and transition".to_owned(),
        });
    }
    Ok(())
}

/// Validates the internal snapshot-expiry reset event and returns its
/// operation ID.
///
/// The event carries the immutable Prepared detail because the public
/// `ForgeSnapshotExpirePhase` has no Reset variant; only the operation string
/// and the failed result distinguish it.
///
/// # Errors
/// Returns [`SqlError::Conflict`] when the operation string, result, or detail
/// identity does not name this resource's reset.
fn validate_reset_event(event: &AuditEvent, expected_resource: &str) -> Result<Uuid, SqlError> {
    if event.operation != "forge.snapshot_expire.reset" || event.result != AuditResult::Failure {
        return Err(SqlError::Conflict {
            detail: "snapshot expiration reset requires a failed forge.snapshot_expire.reset event"
                .to_owned(),
        });
    }
    let detail = event.detail.as_ref().ok_or_else(|| SqlError::Conflict {
        detail: "reset event must carry the prepared detail".to_owned(),
    })?;
    let (operation_id, group) = extract_detail_identity(detail, "forge_snapshot_expire")?;
    if group != expected_resource {
        return Err(SqlError::Conflict {
            detail: format!("detail group {group} does not match resource {expected_resource}"),
        });
    }
    if extract_detail_phase(detail)? != ForgeOperationPhase::Prepared {
        return Err(SqlError::Conflict {
            detail: "snapshot expiration reset must carry the immutable prepared detail".to_owned(),
        });
    }
    Ok(operation_id)
}

/// Requires an exact single-row transition.
///
/// # Errors
/// Returns [`SqlError::Conflict`] unless exactly one row changed.
fn exact_one(changed: u64, operation: &str) -> Result<(), SqlError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(SqlError::Conflict {
            detail: format!("{operation} did not match exact state, attempt, and owner"),
        })
    }
}
