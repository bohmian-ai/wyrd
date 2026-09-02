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
use wyrd_spec::vala::api::{
    AuditDetail, AuditEvent, ForgeIcebergRewritePhase, ForgeManifestRewritePhase,
    ForgeOrphanGcPhase, ForgeScribePromotionPhase, ForgeSnapshotExpirePhase,
    audit_detail_canonical_json,
};

use crate::queries::audit_outbox::append_audit;
use crate::row_types::forge_operations::{
    ForgeOperationFamily, ForgeOperationPhase, ForgeOperationStateRow, ForgeOperationStateSqlRow,
    ForgeOperationTransition, OpenForgeOperationPage,
};
use crate::{SqlError, TenantConn};

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

        self.acquire_operation_lock(conn, operation_id).await?;

        let row = self.select_state_for_update(conn, operation_id).await?;

        match row {
            None => {
                // Absent row: first Prepared for this operation.
                let seq = append_audit(conn, event).await?;
                self.insert_prepared(conn, operation_id, &detail, seq)
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

        self.acquire_operation_lock(conn, operation_id).await?;

        let sql_row = self
            .select_state_for_update(conn, operation_id)
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
        self.apply_terminal(conn, operation_id, &detail, terminal_phase, seq)
            .await?;

        Ok(ForgeOperationTransition::Applied { audit_seq: seq })
    }

    /// Returns at most `cap` fully validated open (Prepared) operations.
    ///
    /// The method selects `cap + 1` state rows from the partial
    /// `forge_operation_state_open` index, point-joins each to its prepared
    /// audit evidence, and validates every row (including the overflow
    /// sentinel). An `overflowed = true` page means the caller should narrow
    /// the resource/family scope or paginate.
    ///
    /// # Errors
    ///
    /// Returns [`SqlError::Conflict`] when `cap` is zero.
    /// Returns [`SqlError::InvariantViolation`] when stored state or audit
    /// evidence fails decoding/parity validation.
    /// Returns [`SqlError::Query`] when the bounded state or point-evidence
    /// query fails.
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
            WITH selected AS MATERIALIZED (
                SELECT *
                  FROM vala.forge_operation_state
                 WHERE data_tenant_id = wyrd.current_tenant()
                   AND resource = $1
                   AND family = $2
                   AND phase = 'prepared'
                 ORDER BY prepared_at, operation_id
                 LIMIT $3
            )
            SELECT selected.*,
                   audit.operation AS prepared_audit_operation,
                   audit.resource AS prepared_audit_resource,
                   audit.detail AS prepared_audit_detail
              FROM selected
              LEFT JOIN vala.audit_outbox AS audit
                ON audit.data_tenant_id = wyrd.current_tenant()
               AND audit.seq = selected.prepared_audit_seq
             ORDER BY selected.prepared_at, selected.operation_id
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
            "WITH selected AS MATERIALIZED (SELECT * FROM vala.forge_operation_state WHERE data_tenant_id=wyrd.current_tenant() AND resource=$1 AND family=$2 AND phase='reset' ORDER BY updated_at,operation_id LIMIT $3) SELECT selected.*,audit.operation AS prepared_audit_operation,audit.resource AS prepared_audit_resource,audit.detail AS prepared_audit_detail FROM selected LEFT JOIN vala.audit_outbox audit ON audit.data_tenant_id=wyrd.current_tenant() AND audit.seq=selected.prepared_audit_seq ORDER BY selected.updated_at,selected.operation_id",
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
        conn: &mut TenantConn<'_>,
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
        .execute(&mut **conn.transaction())
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
        conn: &mut TenantConn<'_>,
        operation_id: Uuid,
    ) -> Result<Option<ForgeOperationStateSqlRow>, SqlError> {
        // We still use FOR UPDATE inside the existing advisory lock to guard
        // against concurrent upsert races. The advisory lock ensures serialized
        // access, and FOR UPDATE provides an additional row-level safety layer.
        let row: Option<ForgeOperationStateSqlRow> = sqlx::query_as(
            r#"
        SELECT state.*,
               audit.operation AS prepared_audit_operation,
               audit.resource AS prepared_audit_resource,
               audit.detail AS prepared_audit_detail
          FROM vala.forge_operation_state AS state
          LEFT JOIN vala.audit_outbox AS audit
            ON audit.data_tenant_id = wyrd.current_tenant()
           AND audit.seq = state.prepared_audit_seq
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
        .fetch_optional(&mut **conn.transaction())
        .await
        .map_err(SqlError::from)?;

        Ok(row)
    }

    /// Inserts a new Prepared state row for the given operation.
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
        conn: &mut TenantConn<'_>,
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
        "#,
        )
        .bind(self.resource)
        .bind(self.family.as_str())
        .bind(operation_id)
        .bind(detail_json.to_string())
        .bind(prepared_seq)
        .bind(now)
        .execute(&mut **conn.transaction())
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
        conn: &mut TenantConn<'_>,
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
        .execute(&mut **conn.transaction())
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
        AuditDetail::ForgeManifestRewrite {
            operation_id,
            group,
            ..
        } => ("forge_manifest_rewrite", operation_id, group),
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
        AuditDetail::ForgeManifestRewrite { phase, .. } => match phase {
            ForgeManifestRewritePhase::Prepared => ForgeOperationPhase::Prepared,
            ForgeManifestRewritePhase::Committed => ForgeOperationPhase::Committed,
            ForgeManifestRewritePhase::Recovered => ForgeOperationPhase::Recovered,
            ForgeManifestRewritePhase::Reset => ForgeOperationPhase::Reset,
        },
        _ => {
            return Err(SqlError::Conflict {
                detail: "expected Forge detail for phase validation".to_owned(),
            });
        }
    };

    Ok(phase)
}
