//! Durable verification state for runtime tests and journeys.
//!
//! [`VerificationFixture`] seeds one tenant with the rows the verification
//! runtime consumes — the SYSTEM writer, active Service owners, Verifier
//! Cards, projected bindings, and queued runs — through the same tenant
//! queries registration and the HTTP surface use, and reads run state back.
//! It never executes, publishes, or settles work; that is the runtime's job.

use serde_json::Value;
use sqlx::Error as SqlxError;
use uuid::Uuid;
use wyrd_runtime::PermissionSet;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_spec::DataTenantId;
use wyrd_spec::card::verifier::OWNER_OCCURRENCE_KEY;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{BindingId, CardUid, VerificationRunId};
use wyrd_spec::registry::RegistrationOperationId;
use wyrd_spec::verification::{DriftWindow, VerificationRunTarget};
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_card_row, insert_registration_operation,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::verification::{
    BindingActivation, FrozenTarget, NewBinding, project_bindings, record_machine_authentication,
};
use wyrd_sql::queries::verifier_runs::{EnqueueOutcome, RunRequest, VerifierRunQueue};
use wyrd_sql::row_types::cards::CardStatus;
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

/// Failure seeding or reading verification state.
#[derive(Debug, thiserror::Error)]
pub enum VerificationFixtureError {
    /// A tenant connection could not be opened or committed.
    #[error("tenant connection failed: {0}")]
    Connection(#[from] SqlError),
    /// A statement failed.
    #[error("verification fixture statement failed: {0}")]
    Query(#[from] SqlxError),
    /// A Card fixture or registry write failed.
    #[error("verification fixture card failed: {0}")]
    Card(String),
    /// The queue refused or deduplicated a run the fixture expected to create.
    #[error("run was not enqueued: {0}")]
    NotEnqueued(String),
}

/// One run's durable control-plane state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRow {
    /// Stored status, such as `pending` or `completed`.
    pub status: String,
    /// Attempts charged so far.
    pub attempts: i32,
    /// Result the run completed with.
    pub result_id: Option<Uuid>,
    /// Stored error code of a retried or failed attempt.
    pub error_code: Option<String>,
}

/// One observation-created run, named by its Verifier for journey assertions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationRun {
    /// The run.
    pub run: VerificationRunId,
    /// Name of the Verifier Card the run executes.
    pub verifier: String,
    /// The frozen `record_id` of the observation that created the run.
    pub record_id: String,
    /// The frozen server-managed `wyrd_event_time` of that observation.
    pub event_time: chrono::DateTime<chrono::Utc>,
    /// The run's durable control-plane state.
    pub state: RunRow,
    /// Operator dispatches the run created.
    pub dispatches: i64,
}

/// The stored columns [`VerificationFixture::observation_runs`] selects.
#[derive(sqlx::FromRow)]
struct ObservationRunRow {
    /// The run ID.
    run_id: Uuid,
    /// The Verifier Card name.
    verifier: String,
    /// The frozen observation record ID.
    record_id: String,
    /// The frozen observation event time.
    event_time: chrono::DateTime<chrono::Utc>,
    /// Stored status.
    status: String,
    /// Attempts charged.
    attempts: i32,
    /// Completed result.
    result_id: Option<Uuid>,
    /// Stored error code.
    error_code: Option<String>,
    /// Operator dispatches of the run.
    dispatches: i64,
}

/// Seeds and reads verification state in one tenant.
#[derive(Clone)]
pub struct VerificationFixture {
    /// Wyrd Postgres owner; every statement runs in a tenant transaction it
    /// opens.
    postgres: WyrdPostgres,
    /// The tenant under test.
    tenant: DataTenantId,
    /// The tenant's SYSTEM writer, whose ID Scribe stamps on every published
    /// result row.
    system_principal: Uuid,
    /// Registering principal recorded on seeded Cards and manual runs.
    actor: Principal,
}

impl VerificationFixture {
    /// Provision `tenant` for verification: built-in roles and the SYSTEM
    /// writer every result publication mints its token for.
    ///
    /// Keeps a clone of `postgres`, the server's Wyrd Postgres owner, and
    /// opens every later tenant transaction through it.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when provisioning fails.
    pub async fn provision(
        postgres: &WyrdPostgres,
        tenant: DataTenantId,
    ) -> Result<Self, VerificationFixtureError> {
        let mut conn = postgres.tenant_conn(tenant).await?;
        wyrd_auth::seed::seed_builtin_roles_for_tenant(&mut conn, tenant)
            .await
            .map_err(|error| VerificationFixtureError::Card(error.to_string()))?;
        let system_principal =
            wyrd_sql::queries::auth::provision_system_principal(&mut conn).await?;
        conn.commit().await?;
        Ok(Self {
            postgres: postgres.clone(),
            tenant,
            system_principal,
            actor: Principal::new(
                PrincipalId::new(Uuid::now_v7()),
                PrincipalKind::User,
                tenant,
                Vec::new(),
                PermissionSet::new(),
            ),
        })
    }

    /// The tenant under test.
    #[must_use]
    pub const fn tenant(&self) -> DataTenantId {
        self.tenant
    }

    /// The ID of the tenant's SYSTEM writer provisioned by [`Self::provision`].
    #[must_use]
    pub const fn system_principal(&self) -> Uuid {
        self.system_principal
    }

    /// Register one active Service Card named `name` and project its
    /// principal; returns the Card UID and the owner principal.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when a write fails.
    pub async fn service(
        &self,
        name: &str,
    ) -> Result<(CardUid, PrincipalId), VerificationFixtureError> {
        let card = card("Service", name, &serde_json::json!({}))?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let uid = self.insert(&mut conn, &card).await?;
        let principal = upsert_service_account_from_card(&mut conn, &uid, &card, &self.actor)
            .await
            .map_err(|error| VerificationFixtureError::Card(error.to_string()))?;
        conn.commit().await?;
        Ok((uid, principal))
    }

    /// Register one active Custom Drift Verifier Card named `name`.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when a write fails.
    pub async fn drift_verifier(&self, name: &str) -> Result<CardUid, VerificationFixtureError> {
        let spec = serde_json::json!({
            "implementation": {
                "kind": "drift",
                "spec": {
                    "method": "Custom",
                    "signal": { "kind": "Metric", "name": "latency_ms" },
                    "condition": { "kind": "Above", "limit": 250.0 }
                }
            }
        });
        let card = card("Verifier", name, &spec)?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let uid = self.insert(&mut conn, &card).await?;
        conn.commit().await?;
        Ok(uid)
    }

    /// Register one active Custom Drift Verifier named `name` whose profile
    /// scores the `metric` series mean against `baseline` with `threshold`.
    ///
    /// Unlike [`Self::drift_verifier`], the production Drift engine can
    /// execute it.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when a write fails.
    pub async fn custom_drift_verifier(
        &self,
        name: &str,
        metric: &str,
        baseline: f64,
        threshold: f64,
    ) -> Result<CardUid, VerificationFixtureError> {
        let spec = serde_json::json!({
            "implementation": {
                "kind": "drift",
                "spec": {
                    "method": "Custom",
                    "signal": { "kind": "Metric", "name": metric },
                    "condition": { "kind": "Statistical" },
                    "profile": {
                        "kind": "Custom",
                        "metric_name": metric,
                        "baseline_value": baseline,
                        "alert_threshold": threshold
                    }
                }
            }
        });
        let card = card("Verifier", name, &spec)?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let uid = self.insert(&mut conn, &card).await?;
        conn.commit().await?;
        Ok(uid)
    }

    /// Register one active Verifier Card named `name` with `spec`.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when `spec` is not a Verifier spec
    /// or a write fails.
    pub async fn verifier(
        &self,
        name: &str,
        spec: &Value,
    ) -> Result<CardUid, VerificationFixtureError> {
        let card = card("Verifier", name, spec)?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let uid = self.insert(&mut conn, &card).await?;
        conn.commit().await?;
        Ok(uid)
    }

    /// Register one PSI or SPC Verifier named `name` with `spec` whose
    /// baseline is already `ready` with the serialized `fitted` profile.
    ///
    /// Registers a stand-in baseline Data Card, then drives the baseline row
    /// through its real pending, claim, and complete transitions, so readiness
    /// is exactly what a fitter would leave without running one.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError::Card`] when another baseline is
    /// claimed first or the completion is stale, or a write error.
    pub async fn fitted_verifier(
        &self,
        name: &str,
        spec: &Value,
        fitted: &Value,
    ) -> Result<CardUid, VerificationFixtureError> {
        let verifier = self.verifier(name, spec).await?;
        let data = card(
            "Data",
            &format!("{name}-baseline"),
            &serde_json::json!({
                "interface": { "kind": "Pandas",
                               "meta": { "framework_version": "2.2.2", "compression": "Snappy" } },
                "schema": { "columns": [{ "name": "value", "dtype": "float64" }] },
                "stats": { "byte_count": 1, "sha256": "0".repeat(64) }
            }),
        )?;
        let queue = DriftBaselineQueue::default();
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let data = self.insert(&mut conn, &data).await?;
        queue.insert_pending(&mut conn, &verifier, &data).await?;
        conn.commit().await?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let claimed = queue
            .claim(&mut conn, chrono::Duration::minutes(1))
            .await?
            .filter(|claimed| claimed.lease.verifier_uid == verifier)
            .ok_or_else(|| {
                VerificationFixtureError::Card(format!("baseline of {verifier} was not claimed"))
            })?;
        queue.complete(&mut conn, &claimed.lease, fitted).await?;
        conn.commit().await?;
        Ok(verifier)
    }

    /// Register one Operator Card named `name` with `spec` directly, skipping
    /// registration's connection-authority check; runtime tests use it for
    /// connection-less HTTP Operators.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when `spec` is not an Operator
    /// spec or a write fails.
    pub async fn operator(
        &self,
        name: &str,
        spec: &Value,
    ) -> Result<CardUid, VerificationFixtureError> {
        let card = card("Operator", name, spec)?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let uid = self.insert(&mut conn, &card).await?;
        conn.commit().await?;
        Ok(uid)
    }

    /// Project one scheduled binding of `verifier` owned by the Service
    /// `owner` and verifying `subject`, dispatching `operators` on failure.
    ///
    /// A `subject` equal to `owner` is an owner-level binding keyed by
    /// [`OWNER_OCCURRENCE_KEY`]; any other subject is a component binding keyed
    /// by the `component` alias, so owner and subject identities stay distinct
    /// through the binding, its runs, and their results.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the projection fails.
    pub async fn bind_schedule(
        &self,
        owner: &CardUid,
        subject: &CardUid,
        verifier: &CardUid,
        cron: &str,
        operators: Vec<FrozenTarget>,
    ) -> Result<BindingId, VerificationFixtureError> {
        self.bind(
            owner,
            subject,
            verifier,
            BindingActivation::Schedule {
                cron: cron.to_owned(),
                tz: None,
            },
            operators,
        )
        .await
    }

    /// Project one `observations_ready` binding of `verifier` on `owner`
    /// itself, with no Operators.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the projection fails.
    pub async fn bind_observations(
        &self,
        owner: &CardUid,
        verifier: &CardUid,
    ) -> Result<BindingId, VerificationFixtureError> {
        self.bind(
            owner,
            owner,
            verifier,
            BindingActivation::ObservationsReady,
            Vec::new(),
        )
        .await
    }

    /// Project one binding of `verifier` on `subject` under `owner` with
    /// `activation` and `operators`; the owner's own occurrence when `subject`
    /// is `owner`, otherwise a `component` occurrence.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the projection fails or
    /// projects no binding.
    async fn bind(
        &self,
        owner: &CardUid,
        subject: &CardUid,
        verifier: &CardUid,
        activation: BindingActivation,
        operators: Vec<FrozenTarget>,
    ) -> Result<BindingId, VerificationFixtureError> {
        let occurrence = if subject == owner {
            OWNER_OCCURRENCE_KEY
        } else {
            "component"
        };
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let bindings = project_bindings(
            &mut conn,
            owner,
            &CardKind::Service,
            &[NewBinding {
                subject_occurrence_key: occurrence.to_owned(),
                subject_card_uid: subject.clone(),
                verifier_uid: verifier.clone(),
                trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
                operators,
                activation,
            }],
        )
        .await?;
        conn.commit().await?;
        bindings
            .first()
            .copied()
            .ok_or_else(|| VerificationFixtureError::Card("no binding projected".to_owned()))
    }

    /// Record a qualifying machine authentication of `owner`, which activates
    /// its bindings and arms their schedule cursors at database time.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the write fails.
    pub async fn activate(&self, owner: PrincipalId) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        record_machine_authentication(&mut conn, owner).await?;
        conn.commit().await?;
        Ok(())
    }

    /// Bring one run's live deadline to database statement time, leaving an
    /// absent deadline absent.
    ///
    /// A run's lease and retry deadlines belong to PostgreSQL, so a test whose
    /// subject is expiry arranges the database row it exercises instead of
    /// moving any process clock. Whichever of the two deadlines the run
    /// actually holds becomes immediately due; the other stays null.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the update fails.
    pub async fn expire_deadlines(
        &self,
        run: VerificationRunId,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        sqlx::query(
            "UPDATE wyrd.verifier_runs \
                SET lease_expires_at = CASE WHEN lease_expires_at IS NULL \
                                            THEN NULL ELSE statement_timestamp() END, \
                    next_attempt_at = CASE WHEN next_attempt_at IS NULL \
                                           THEN NULL ELSE statement_timestamp() END \
              WHERE run_id = $1",
        )
        .bind(run.as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(())
    }

    /// Bring a still-`retrying` run's retry deadline to database statement
    /// time, leaving a run in any other status untouched.
    ///
    /// The status guard sits in the same `UPDATE`, so a poller acting on a
    /// stale snapshot cannot expire the fresh lease of a run a live runtime
    /// has just reclaimed; [`Self::expire_deadlines`] is for tests whose
    /// subject is the lease itself.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the update fails.
    pub async fn make_retry_due(
        &self,
        run: VerificationRunId,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        sqlx::query(
            "UPDATE wyrd.verifier_runs SET next_attempt_at = statement_timestamp() \
              WHERE run_id = $1 AND status = 'retrying'",
        )
        .bind(run.as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(())
    }

    /// Bring one armed schedule cursor due no earlier than the present on
    /// both the database and this process's clock.
    ///
    /// Dueness is PostgreSQL's decision, so a scheduling test places the
    /// cursor where the due predicate fires rather than moving a process
    /// clock. The cursor is also the occurrence's exclusive window end, which
    /// the Drift engine compares with `wyrd_event_time` values an in-process
    /// Scribe stamped from this process's clock. The database clock may lag
    /// that clock (a VM-backed Postgres), so the cursor is the later of
    /// `statement_timestamp()` and this process's `now`: every observation
    /// acknowledged before the call falls inside the window, and the
    /// scheduler claims the occurrence once the database clock reaches the
    /// cursor.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the update fails.
    pub async fn make_binding_due(
        &self,
        binding: BindingId,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        sqlx::query(
            "UPDATE wyrd.verification_bindings \
                SET next_run_at = GREATEST(statement_timestamp(), $2) WHERE binding_id = $1",
        )
        .bind(binding.as_uuid())
        .bind(chrono::Utc::now())
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(())
    }

    /// Rewrite `verifier`'s ready fitted profile without its `format`, as a
    /// profile fitted before the current PSI/SPC semantics is stored.
    ///
    /// Stands in for a Verifier version fitted under earlier semantics, which
    /// the engine must refuse rather than rescore; nothing else changes.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError::Card`] when `verifier` has no ready
    /// fitted profile, or a connection or statement error.
    pub async fn retire_fitted_format(
        &self,
        verifier: &CardUid,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let updated = sqlx::query(
            "UPDATE wyrd.drift_baselines \
                SET fitted = (SELECT jsonb_object_agg(key, value - 'format') \
                                FROM jsonb_each(fitted)) \
              WHERE verifier_uid = $1 AND state = 'ready'",
        )
        .bind(verifier.as_uuid())
        .execute(&mut **conn.transaction())
        .await?
        .rows_affected();
        conn.commit().await?;
        if updated == 1 {
            Ok(())
        } else {
            Err(VerificationFixtureError::Card(format!(
                "verifier {verifier} has no ready fitted profile"
            )))
        }
    }

    /// Enqueue one manual direct run of `verifier` over `subject` for `window`,
    /// due at database statement time.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError::NotEnqueued`] when the queue
    /// refuses the run, or a connection or statement error.
    pub async fn enqueue_direct(
        &self,
        verifier: &CardUid,
        subject: &CardUid,
        window: DriftWindow,
    ) -> Result<VerificationRunId, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let outcome = VerifierRunQueue::default()
            .enqueue(
                &mut conn,
                &RunRequest::Manual {
                    requested_by: self.actor.id,
                    target: VerificationRunTarget::Verifier {
                        verifier_uid: verifier.clone(),
                        subject_card_uid: subject.clone(),
                    },
                    window,
                },
            )
            .await?;
        conn.commit().await?;
        match outcome {
            EnqueueOutcome::Enqueued(run) => Ok(run),
            other => Err(VerificationFixtureError::NotEnqueued(format!("{other:?}"))),
        }
    }

    /// Read one run's durable state.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the run cannot be read.
    pub async fn run(&self, run: VerificationRunId) -> Result<RunRow, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let (status, attempts, result_id, error_code): (String, i32, Option<Uuid>, Option<String>) =
            sqlx::query_as(
                "SELECT status, attempts, result_id, error->>'code' \
                 FROM wyrd.verifier_runs WHERE run_id = $1",
            )
            .bind(run.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await?;
        Ok(RunRow {
            status,
            attempts,
            result_id,
            error_code,
        })
    }

    /// Every run of this tenant, oldest first.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the runs cannot be read.
    pub async fn runs(&self) -> Result<Vec<VerificationRunId>, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let ids: Vec<Uuid> =
            sqlx::query_scalar("SELECT run_id FROM wyrd.verifier_runs ORDER BY created_at, run_id")
                .fetch_all(&mut **conn.transaction())
                .await?;
        ids.into_iter()
            .map(|id| {
                VerificationRunId::new(id)
                    .map_err(|error| VerificationFixtureError::Card(error.to_string()))
            })
            .collect()
    }

    /// Every observation-created run of this tenant with its Verifier name,
    /// frozen input, state, and dispatch count, oldest first.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the runs cannot be read.
    pub async fn observation_runs(&self) -> Result<Vec<ObservationRun>, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let rows: Vec<ObservationRunRow> = sqlx::query_as(
            "SELECT r.run_id, c.name AS verifier, r.input_record_id AS record_id, \
                    r.input_event_time AS event_time, r.status, r.attempts, r.result_id, \
                    r.error->>'code' AS error_code, \
                    (SELECT count(*) FROM wyrd.operator_dispatches d \
                      WHERE d.run_id = r.run_id) AS dispatches \
               FROM wyrd.verifier_runs r JOIN wyrd.cards c ON c.card_uid = r.verifier_uid \
              WHERE r.origin = 'observation' ORDER BY r.created_at, r.run_id",
        )
        .fetch_all(&mut **conn.transaction())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ObservationRun {
                    run: VerificationRunId::new(row.run_id)
                        .map_err(|error| VerificationFixtureError::Card(error.to_string()))?,
                    verifier: row.verifier,
                    record_id: row.record_id,
                    event_time: row.event_time,
                    state: RunRow {
                        status: row.status,
                        attempts: row.attempts,
                        result_id: row.result_id,
                        error_code: row.error_code,
                    },
                    dispatches: row.dispatches,
                })
            })
            .collect()
    }

    /// Insert `card` as an active registered row and return its UID.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when a registry write fails.
    async fn insert(
        &self,
        conn: &mut TenantConn<'_>,
        card: &Card,
    ) -> Result<CardUid, VerificationFixtureError> {
        let operation_id = RegistrationOperationId::new(Uuid::now_v7());
        let key = Uuid::now_v7().to_string();
        insert_registration_operation(
            conn,
            NewRegistrationOperation {
                operation_id,
                principal_id: self.actor.id,
                idempotency_key: &key,
                request_hash: "request-hash",
            },
        )
        .await
        .map_err(|error| VerificationFixtureError::Card(error.to_string()))?;
        let card_uid = CardUid::from_uuid(Uuid::now_v7())
            .map_err(|error| VerificationFixtureError::Card(error.to_string()))?;
        let uid = insert_card_row(
            conn,
            NewCardRow {
                card,
                card_uid,
                principal_id: self.actor.id,
                operation_id,
                status: CardStatus::Pending,
                spec_hash: "spec-hash",
                artifact_hash: None,
            },
        )
        .await
        .map_err(|error| VerificationFixtureError::Card(error.to_string()))?
        .card_uid;
        sqlx::query("UPDATE wyrd.cards SET status = 'active' WHERE card_uid = $1")
            .bind(uid.as_uuid())
            .execute(&mut **conn.transaction())
            .await?;
        Ok(uid)
    }
}

/// Build one `kind` Card named `name` at version 1.0.0 with `spec`.
///
/// # Errors
/// Returns [`VerificationFixtureError::Card`] when the fixture JSON does not
/// deserialize as a Card.
fn card(kind: &str, name: &str, spec: &Value) -> Result<Card, VerificationFixtureError> {
    serde_json::from_value(serde_json::json!({
        "apiVersion": "wyrd/v1",
        "kind": kind,
        "metadata": { "name": name, "version": "1.0.0", "space": "default" },
        "spec": spec,
        "relationships": { "outbound": [], "inbound": [] }
    }))
    .map_err(|error| VerificationFixtureError::Card(error.to_string()))
}
