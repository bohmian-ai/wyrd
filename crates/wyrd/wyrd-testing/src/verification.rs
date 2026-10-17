//! Durable verification state for runtime tests and journeys.
//!
//! [`VerificationFixture`] seeds one tenant with the rows the verification
//! runtime consumes — the SYSTEM writer, active Service owners, Verifier
//! Cards, projected bindings, and queued executions — through the same
//! tenant queries registration and the queue writer use, and reads execution
//! state back.
//! It never executes, publishes, or settles work; that is the runtime's job.

use std::collections::BTreeMap;

use serde_json::Value;
use sqlx::Error as SqlxError;
use uuid::Uuid;
use wyrd_runtime::principal::{Principal, PrincipalId, PrincipalKind};
use wyrd_runtime::{AcceptedAuthority, PermissionSet};
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::card::verifier::{OWNER_OCCURRENCE_KEY, ScheduledInput, VerifierSpec};
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::ids::{BindingId, CardUid, ExecutionId, SlotName};
use wyrd_spec::registry::RegistrationOperationId;
use wyrd_spec::verification::execution::{InputDeclaration, VerificationRequest};
use wyrd_sql::queries::cards::{
    NewCardRow, NewRegistrationOperation, insert_card_row, insert_registration_operation,
    upsert_service_account_from_card,
};
use wyrd_sql::queries::drift_baselines::DriftBaselineQueue;
use wyrd_sql::queries::verification::{
    FrozenTarget, NewBinding, project_bindings, record_machine_authentication,
};
use wyrd_sql::queries::verifier_executions::{
    Submission, SubmissionOutcome, VerifierExecutionQueue,
};
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
    /// The queue refused or replayed a submission the fixture expected to
    /// accept.
    #[error("submission was not accepted: {0}")]
    NotEnqueued(String),
}

/// One execution's durable control-plane state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRow {
    /// Stored status, such as `pending` or `completed`.
    pub status: String,
    /// Attempts charged so far.
    pub attempts: i32,
    /// Result the execution completed with.
    pub result_id: Option<Uuid>,
    /// Stored error code of a retried or failed attempt.
    pub error_code: Option<String>,
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
    /// Registering principal recorded on seeded Cards and submissions.
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
        let system_principal = wyrd_sql::queries::auth::provision_system_principal(
            &mut conn,
            Attribution::unresolved(),
        )
        .await?;
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
        let principal = upsert_service_account_from_card(
            &mut conn,
            &uid,
            &card,
            Attribution::direct(self.actor.id),
        )
        .await
        .map_err(|error| VerificationFixtureError::Card(error.to_string()))?;
        conn.commit().await?;
        Ok((uid, principal))
    }

    /// Register one active Custom Drift Verifier Card named `name` whose
    /// `samples` slot reads the `latency_ms` metric.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when a write fails.
    pub async fn drift_verifier(&self, name: &str) -> Result<CardUid, VerificationFixtureError> {
        self.verifier(
            name,
            &drift_spec(
                "latency_ms",
                serde_json::json!({
                    "method": "Custom",
                    "signal": { "kind": "Metric", "name": "latency_ms" },
                    "condition": { "kind": "Above", "limit": 250.0 }
                }),
            ),
        )
        .await
    }

    /// Register one active Custom Drift Verifier named `name` whose profile
    /// scores the `metric` series mean against `baseline` with `threshold`.
    ///
    /// Unlike [`Self::drift_verifier`], the production Drift engine can
    /// score it.
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
        self.verifier(
            name,
            &drift_spec(
                metric,
                serde_json::json!({
                    "method": "Custom",
                    "signal": { "kind": "Metric", "name": metric },
                    "condition": { "kind": "Statistical" },
                    "profile": {
                        "kind": "Custom",
                        "metric_name": metric,
                        "baseline_value": baseline,
                        "alert_threshold": threshold
                    }
                }),
            ),
        )
        .await
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

    /// Register one PSI or SPC Verifier named `name` with `spec` whose first
    /// drift requirement's baseline is already `ready` with the serialized
    /// `fitted` profile.
    ///
    /// Registers a stand-in baseline Data Card, then drives the baseline row
    /// through its real pending, claim, and complete transitions, so readiness
    /// is exactly what a fitter would leave without running one.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError::Card`] when `spec` has no drift
    /// requirement, another baseline is claimed first, or the completion is
    /// stale, or a write error.
    pub async fn fitted_verifier(
        &self,
        name: &str,
        spec: &Value,
        fitted: &Value,
    ) -> Result<CardUid, VerificationFixtureError> {
        let requirement = serde_json::from_value::<VerifierSpec>(spec.clone())
            .map_err(|error| VerificationFixtureError::Card(error.to_string()))?
            .drift_checks()
            .next()
            .map(|(requirement, _)| requirement.clone())
            .ok_or_else(|| VerificationFixtureError::Card(format!("{name} has no drift check")))?;
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
        queue
            .insert_pending(&mut conn, &verifier, &requirement, &data)
            .await?;
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
    /// `owner` and verifying `subject`, filling each samples slot from
    /// `inputs` and dispatching `operators` on failure.
    ///
    /// A `subject` equal to `owner` is an owner-level binding keyed by
    /// [`OWNER_OCCURRENCE_KEY`]; any other subject is a component binding keyed
    /// by the `component` alias, so owner and subject identities stay distinct
    /// through the binding, its executions, and their results.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the projection fails or
    /// projects no binding.
    pub async fn bind_schedule(
        &self,
        owner: &CardUid,
        subject: &CardUid,
        verifier: &CardUid,
        cron: &str,
        inputs: BTreeMap<SlotName, ScheduledInput>,
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
                card_uid: subject.clone(),
                verifier_card_uid: verifier.clone(),
                trigger: FrozenTarget::Digest("sha256:trigger".to_owned()),
                operators,
                cron: cron.to_owned(),
                tz: None,
                inputs,
            }],
        )
        .await?;
        conn.commit().await?;
        bindings
            .first()
            .copied()
            .ok_or_else(|| VerificationFixtureError::Card("no binding projected".to_owned()))
    }

    /// The scheduled input map of [`Self::drift_verifier`]: its `samples`
    /// slot reads the `metric` series.
    ///
    /// # Panics
    /// Panics when `metric` is not a valid feature name.
    #[must_use]
    pub fn metric_inputs(metric: &str) -> BTreeMap<SlotName, ScheduledInput> {
        serde_json::from_value(serde_json::json!({ "samples": { "metric": metric } }))
            .expect("invariant: the fixture metric is a valid feature name")
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

    /// Bring one execution's live deadline to database statement time,
    /// leaving an absent deadline absent.
    ///
    /// An execution's lease and retry deadlines belong to PostgreSQL, so a
    /// test whose subject is expiry arranges the database row it exercises
    /// instead of moving any process clock. Whichever of the two deadlines
    /// the execution actually holds becomes immediately due; the other stays
    /// null.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the update fails.
    pub async fn expire_deadlines(
        &self,
        execution: ExecutionId,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        sqlx::query(
            "UPDATE wyrd.verifier_executions \
                SET lease_expires_at = CASE WHEN lease_expires_at IS NULL \
                                            THEN NULL ELSE statement_timestamp() END, \
                    next_attempt_at = CASE WHEN next_attempt_at IS NULL \
                                           THEN NULL ELSE statement_timestamp() END \
              WHERE execution_id = $1",
        )
        .bind(execution.as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(())
    }

    /// Bring a still-`retrying` execution's retry deadline to database
    /// statement time, leaving one in any other status untouched.
    ///
    /// The status guard sits in the same `UPDATE`, so a poller acting on a
    /// stale snapshot cannot expire the fresh lease of an execution a live runtime
    /// has just reclaimed; [`Self::expire_deadlines`] is for tests whose
    /// subject is the lease itself.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the update fails.
    pub async fn make_retry_due(
        &self,
        execution: ExecutionId,
    ) -> Result<(), VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        sqlx::query(
            "UPDATE wyrd.verifier_executions SET next_attempt_at = statement_timestamp() \
              WHERE execution_id = $1 AND status = 'retrying'",
        )
        .bind(execution.as_uuid())
        .execute(&mut **conn.transaction())
        .await?;
        conn.commit().await?;
        Ok(())
    }

    /// Bring one armed schedule cursor to now.
    ///
    /// Dueness is PostgreSQL's decision, so the scheduler claims the
    /// occurrence once PostgreSQL's clock is 30 seconds past the cursor and
    /// callers poll for the execution for longer than that. The cursor is also
    /// the occurrence's exclusive window end, and observations carry
    /// producer-owned `created_at` stamped by this process's clock, which may
    /// run ahead of the database's. The cursor is therefore the later of
    /// `statement_timestamp()` and this process's `now`, so the window holds
    /// every observation the test already wrote whatever the clock skew.
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

    /// Accept one caller submission of `verifier` about `subject` over
    /// `inputs` into the queue, as the batched writer would once Scribe
    /// acknowledged its inputs, due at database statement time.
    ///
    /// The submission runs under the fixture actor's authority.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError::NotEnqueued`] when the queue does
    /// not accept it as new, or a connection or statement error.
    pub async fn submit(
        &self,
        verifier: &CardUid,
        subject: &CardUid,
        inputs: BTreeMap<SlotName, InputDeclaration>,
    ) -> Result<ExecutionId, VerificationFixtureError> {
        let submission = Submission::new(
            self.tenant,
            AcceptedAuthority::new(&self.actor, &[]),
            VerificationRequest {
                verifier_card_uid: verifier.clone(),
                card_uid: subject.clone(),
                run_id: None,
                group_id: None,
                inputs,
                execution_route_overrides: BTreeMap::new(),
            },
            None,
            None,
        )
        .map_err(|error| VerificationFixtureError::NotEnqueued(error.to_string()))?;
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let outcomes = VerifierExecutionQueue::default()
            .accept_submissions(&mut conn, std::slice::from_ref(&submission))
            .await?;
        conn.commit().await?;
        match outcomes.as_slice() {
            [SubmissionOutcome::Accepted] => Ok(submission.execution_id),
            other => Err(VerificationFixtureError::NotEnqueued(format!("{other:?}"))),
        }
    }

    /// Read one execution's durable state.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the execution cannot be read.
    pub async fn execution(
        &self,
        execution: ExecutionId,
    ) -> Result<ExecutionRow, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let (status, attempts, result_id, error_code): (String, i32, Option<Uuid>, Option<String>) =
            sqlx::query_as(
                "SELECT status, attempts, result_id, error->>'code' \
                 FROM wyrd.verifier_executions WHERE execution_id = $1",
            )
            .bind(execution.as_uuid())
            .fetch_one(&mut **conn.transaction())
            .await?;
        Ok(ExecutionRow {
            status,
            attempts,
            result_id,
            error_code,
        })
    }

    /// Every execution of this tenant, oldest first.
    ///
    /// # Errors
    /// Returns [`VerificationFixtureError`] when the executions cannot be read.
    pub async fn executions(&self) -> Result<Vec<ExecutionId>, VerificationFixtureError> {
        let mut conn = self.postgres.tenant_conn(self.tenant).await?;
        let ids: Vec<Uuid> = sqlx::query_scalar(
            "SELECT execution_id FROM wyrd.verifier_executions ORDER BY created_at, execution_id",
        )
        .fetch_all(&mut **conn.transaction())
        .await?;
        ids.into_iter()
            .map(|id| {
                ExecutionId::new(id)
                    .map_err(|error| VerificationFixtureError::Card(error.to_string()))
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
                attribution: Attribution::direct(self.actor.id),
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
                attribution: Attribution::direct(self.actor.id),
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

/// A building-block Verifier spec whose one requirement is the drift check
/// `drift` over a `samples` slot measuring `metric`.
fn drift_spec(metric: &str, drift: Value) -> Value {
    let mut check = serde_json::json!({ "kind": "drift", "on": "inputs.samples" });
    if let (Some(check), Value::Object(drift)) = (check.as_object_mut(), drift) {
        check.extend(drift);
    }
    serde_json::json!({
        "inputs": { "samples": { "type": "samples", "features": [metric] } },
        "requirements": [{ "id": "drift", "statement": "The window does not drift.", "check": check }]
    })
}
