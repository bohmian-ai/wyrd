//! Deployment wrapping-key maintenance over every recoverable stored secret.
//!
//! Activating a wrapping key changes only where new secrets are sealed: no
//! boot, replica, or background runner moves stored envelopes. A platform
//! administrator moves them with an explicit, bounded pass through
//! [`StoredSecretMaintenance::run`], in one of two modes:
//!
//! - `routine` rewraps each data key under the active key and keeps the data
//!   key, payload nonce, and payload ciphertext;
//! - `recovery` opens each value and reseals it under a fresh data key and
//!   nonce wrapped by the active key.
//!
//! The pass walks human connections, workload trusted issuers, the platform
//! connection, Gateway managed credentials, and Operator connections. Each
//! envelope's context is rebuilt from its row's trusted owner metadata, so an
//! envelope copied to another owner fails to open and stays `remaining`.
//! Key-service calls run with no transaction or row lock held, and each value
//! is replaced by a compare-and-swap on the exact bytes read, which touches no
//! other column: a concurrent replacement, revocation, or renewal stands, and
//! the value it lost to counts as `remaining`. Logs and reports carry store,
//! tenant, owner, and a reason code, never key or secret material.

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use tokio::time::Instant;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use wyrd_auth::platform_authz::{PLATFORM_STORED_SECRETS_RESOURCE, PlatformAuthorization};
use wyrd_kms::{KmsError, SecretContext, SecretPurpose, SecretScope, WyrdKms};
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::{WyrdError, WyrdProblem};
use wyrd_spec::security::stored_secret::{
    MAX_REPORTED_FAILURES, StoredSecretFailure, StoredSecretFailureReason,
    StoredSecretMaintenanceMode, StoredSecretMaintenanceReport, StoredSecretMaintenanceRequest,
    StoredSecretStore,
};
use wyrd_sql::queries::auth::{
    StoredSecretRow, StoredSecretTable, stored_secrets, swap_stored_secret,
};
use wyrd_sql::{OperatorPool, SqlError};

use super::identity::{operator, platform_authz_error};
use crate::components::auth::PlatformCaller;
use crate::components::gateway::GatewayAdministration;
use crate::http::error::{WyrdErrorResponse, internal_failure};
use crate::state::AppState;

/// Owner of the cross-store wrapping-key maintenance pass.
#[derive(Clone)]
pub struct StoredSecretMaintenance {
    /// Cross-tenant operator pool; the pass reads and swaps every owner.
    operator: OperatorPool,
    /// Deployment facade; `None` without `[kms]`, where no envelope can move.
    kms: Option<Arc<WyrdKms>>,
}

impl StoredSecretMaintenance {
    /// Build the maintenance owner over the operator pool and deployment
    /// facade.
    #[must_use]
    pub const fn new(operator: OperatorPool, kms: Option<Arc<WyrdKms>>) -> Self {
        Self { operator, kms }
    }

    /// Whether any store holds an envelope.
    ///
    /// A deployment without `[kms]` must hold none, since it could not open
    /// them. Boot waits on these reads as long as they take.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a read fails.
    pub async fn holds_envelopes(&self) -> Result<bool, SqlError> {
        for table in StoredSecretTable::ALL {
            if stored_secrets(&self.operator, table, Duration::MAX)
                .await?
                .is_none_or(|rows| !rows.is_empty())
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Run one bounded pass moving every observed envelope that does not
    /// name the active key onto it.
    ///
    /// The pass attempts at most `limit` transitions, and every store read,
    /// key-service call, and swap shares one deadline `budget` from now. An
    /// observed envelope it does not move counts as `remaining`; a store whose
    /// read the deadline cut, and every later store, is not observed and not
    /// counted; `bounded` reports either cut. Rows already swapped stay
    /// swapped, so repeating the pass resumes from the rest.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a read or swap statement fails.
    #[tracing::instrument(level = "info", skip(self), fields(backend = ?self.kms.as_ref().map(|kms| kms.backend())))]
    pub async fn run(
        &self,
        mode: StoredSecretMaintenanceMode,
        limit: Option<NonZeroU32>,
        budget: Duration,
    ) -> Result<StoredSecretMaintenanceReport, SqlError> {
        self.pass(mode, limit, budget).await.map(|pass| pass.report)
    }

    /// Run the pass [`Self::run`] describes and return its full outcome.
    ///
    /// # Errors
    /// Returns the errors of [`Self::run`].
    async fn pass(
        &self,
        mode: StoredSecretMaintenanceMode,
        limit: Option<NonZeroU32>,
        budget: Duration,
    ) -> Result<Pass, SqlError> {
        let deadline = Instant::now() + budget;
        let mut pass = Pass {
            report: StoredSecretMaintenanceReport {
                mode,
                current: 0,
                changed: 0,
                remaining: 0,
                bounded: false,
                failures: Vec::new(),
            },
            attempts_left: limit.map_or(u64::MAX, |limit| u64::from(limit.get())),
            deadline,
            moved_gateway_tenants: BTreeSet::new(),
        };
        for table in StoredSecretTable::ALL {
            let wait = deadline.saturating_duration_since(Instant::now());
            let rows = if wait.is_zero() {
                None
            } else {
                stored_secrets(&self.operator, table, wait).await?
            };
            let Some(rows) = rows else {
                pass.report.bounded = true;
                break;
            };
            for row in &rows {
                self.settle(&mut pass, table, row).await?;
            }
        }
        tracing::info!(
            changed = pass.report.changed,
            current = pass.report.current,
            remaining = pass.report.remaining,
            bounded = pass.report.bounded,
            "stored secret maintenance pass finished"
        );
        Ok(pass)
    }

    /// Run one pass for a request served by `state`, bounded by half its
    /// request timeout (see [`Self::run`]), then warm `state`'s Gateway for
    /// each tenant whose credential the pass moved, as any Gateway write
    /// does, so this server serves the committed envelope at once.
    ///
    /// Warming is best effort and bounded by a further quarter of the
    /// timeout; a cut warm, like a failed one, leaves the background reload
    /// to recover. A credential that lost its swap to a concurrent write is
    /// not warmed: that writer's own warm and notification cover it.
    ///
    /// # Errors
    /// Returns the errors of [`Self::run`].
    pub(crate) async fn run_serving(
        &self,
        state: &AppState,
        mode: StoredSecretMaintenanceMode,
        limit: Option<NonZeroU32>,
    ) -> Result<StoredSecretMaintenanceReport, SqlError> {
        // ponytail: half the request timeout leaves room to answer before the
        // edge drops the handler; a configurable budget if operators need
        // longer passes.
        let timeout = state.settings.limits.timeout;
        let pass = self.pass(mode, limit, timeout / 2).await?;
        let gateway = GatewayAdministration::new(state);
        let warmed = tokio::time::timeout(timeout / 4, async {
            for tenant in &pass.moved_gateway_tenants {
                gateway.warm(*tenant).await;
            }
        })
        .await;
        if warmed.is_err() {
            tracing::warn!("gateway warm after stored secret maintenance was cut by its budget");
        }
        Ok(pass.report)
    }

    /// Classify one envelope, move it when needed and the bound allows, and
    /// count the outcome.
    ///
    /// # Errors
    /// Returns [`SqlError`] when the swap fails.
    async fn settle(
        &self,
        pass: &mut Pass,
        table: StoredSecretTable,
        row: &StoredSecretRow,
    ) -> Result<(), SqlError> {
        let Some(kms) = self.kms.as_deref() else {
            pass.fail(table, row, StoredSecretFailureReason::KeyUnavailable);
            return Ok(());
        };
        match kms.is_current(&row.envelope) {
            Ok(true) => {
                pass.report.current += 1;
                return Ok(());
            }
            Ok(false) => {}
            Err(error) => {
                pass.fail(table, row, reason(&error));
                return Ok(());
            }
        }
        if pass.attempts_left == 0 || Instant::now() >= pass.deadline {
            pass.report.bounded = true;
            pass.report.remaining += 1;
            return Ok(());
        }
        pass.attempts_left -= 1;
        let authority: Vec<&str> = row.authority.iter().map(String::as_str).collect();
        let Some(context) = context(table, row, &authority) else {
            pass.fail(table, row, StoredSecretFailureReason::InvalidOwner);
            return Ok(());
        };
        let moved = match pass.report.mode {
            StoredSecretMaintenanceMode::Routine => {
                tokio::time::timeout_at(pass.deadline, kms.rewrap(&context, &row.envelope)).await
            }
            StoredSecretMaintenanceMode::Recovery => {
                tokio::time::timeout_at(pass.deadline, kms.renew(&context, &row.envelope)).await
            }
        };
        match moved {
            Err(_) => {
                pass.report.bounded = true;
                pass.report.remaining += 1;
            }
            Ok(Err(error)) => pass.fail(table, row, reason(&error)),
            Ok(Ok(envelope)) => {
                let wait = pass.deadline.saturating_duration_since(Instant::now());
                let swapped = if wait.is_zero() {
                    None
                } else {
                    swap_stored_secret(&self.operator, table, row, &envelope, wait).await?
                };
                match swapped {
                    Some(true) => {
                        pass.report.changed += 1;
                        if table == StoredSecretTable::GatewayProviderCredentials
                            && let Some(tenant) = row
                                .data_tenant_id
                                .and_then(|id| DataTenantId::try_from(id).ok())
                        {
                            pass.moved_gateway_tenants.insert(tenant);
                        }
                    }
                    Some(false) => {
                        pass.fail(table, row, StoredSecretFailureReason::ConcurrentChange);
                    }
                    None => {
                        pass.report.bounded = true;
                        pass.report.remaining += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

/// One pass's running report and remaining bound.
struct Pass {
    /// Counts and failures so far.
    report: StoredSecretMaintenanceReport,
    /// Transitions the request limit still allows.
    attempts_left: u64,
    /// When the pass stops waiting on the key service or the database.
    deadline: Instant,
    /// Tenants whose Gateway credential this pass swapped.
    moved_gateway_tenants: BTreeSet<DataTenantId>,
}

impl Pass {
    /// Count `row` as remaining for `reason`, list it while the report has
    /// room, and log it with bounded fields only.
    fn fail(
        &mut self,
        table: StoredSecretTable,
        row: &StoredSecretRow,
        reason: StoredSecretFailureReason,
    ) {
        let store = store(table);
        tracing::warn!(?store, tenant = ?row.data_tenant_id, owner = %row.row_key, ?reason, "stored secret was not moved");
        self.report.remaining += 1;
        if self.report.failures.len() < MAX_REPORTED_FAILURES {
            self.report.failures.push(StoredSecretFailure {
                store,
                data_tenant_id: row
                    .data_tenant_id
                    .and_then(|id| DataTenantId::try_from(id).ok()),
                owner: row.row_key.clone(),
                reason,
            });
        }
    }
}

/// The envelope context `row`'s trusted owner metadata binds, or `None` when
/// that metadata cannot form one.
fn context<'a>(
    table: StoredSecretTable,
    row: &'a StoredSecretRow,
    authority: &'a [&'a str],
) -> Option<SecretContext<'a>> {
    let purpose = match table {
        StoredSecretTable::HumanConnections => SecretPurpose::AuthHumanConnection,
        StoredSecretTable::TrustedIssuers => SecretPurpose::AuthTrustedIssuer,
        StoredSecretTable::PlatformOidc => SecretPurpose::PlatformOidc,
        StoredSecretTable::GatewayProviderCredentials => SecretPurpose::GatewayProviderCredential,
        StoredSecretTable::OperatorConnections => SecretPurpose::OperatorConnection,
    };
    let scope = match (table, row.data_tenant_id) {
        (StoredSecretTable::PlatformOidc, None) => SecretScope::Platform,
        (StoredSecretTable::PlatformOidc, Some(_)) | (_, None) => return None,
        (_, Some(tenant)) => SecretScope::Tenant(DataTenantId::try_from(tenant).ok()?),
    };
    Some(SecretContext {
        scope,
        purpose,
        owner: &row.owner,
        authority,
        version: row.version,
    })
}

/// The public store a column belongs to.
const fn store(table: StoredSecretTable) -> StoredSecretStore {
    match table {
        StoredSecretTable::HumanConnections => StoredSecretStore::HumanConnection,
        StoredSecretTable::TrustedIssuers => StoredSecretStore::TrustedIssuer,
        StoredSecretTable::PlatformOidc => StoredSecretStore::PlatformOidc,
        StoredSecretTable::GatewayProviderCredentials => StoredSecretStore::GatewayCredential,
        StoredSecretTable::OperatorConnections => StoredSecretStore::OperatorConnection,
    }
}

/// The public reason for a facade failure.
const fn reason(error: &KmsError) -> StoredSecretFailureReason {
    match error {
        KmsError::KeyNotAllowed => StoredSecretFailureReason::KeyNotAllowed,
        KmsError::UnknownFormat | KmsError::Authentication | KmsError::InvalidDek => {
            StoredSecretFailureReason::Unopenable
        }
        KmsError::Encrypt | KmsError::Unavailable { .. } | KmsError::Config { .. } => {
            StoredSecretFailureReason::KeyUnavailable
        }
    }
}

/// Build the platform stored-secret maintenance route.
pub fn platform_stored_secrets_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(maintain_stored_secrets))
}

/// Run one wrapping-key maintenance pass over every recoverable stored
/// secret.
///
/// The pass returns its report before the request timeout: its store reads,
/// key-service calls, and swaps stop waiting after half of
/// `limits.timeout_ms`. Observed envelopes it did not move count as
/// `remaining`; stores it never read are not counted, and `bounded` is set.
/// Repeat the same mode until `bounded` is false and `remaining` is zero or
/// only failures are left.
///
/// # Errors
/// Returns `503` when the deployment has no `[kms]`, and a stable Wyrd error
/// when the caller is unauthorized or a store read or write fails.
#[utoipa::path(
    post,
    path = "/platform/stored-secrets/maintenance",
    request_body = StoredSecretMaintenanceRequest,
    responses(
        (status = 200, description = "Pass finished; counts cover only what it observed",
         body = StoredSecretMaintenanceReport),
        (status = 401, description = "No valid platform credential \
          (WYRD_AUTH_401_UNAUTHENTICATED)", body = WyrdProblem),
        (status = 403, description = "The grant does not cover platform credential writes \
          (WYRD_PERMISSION_403_DENIED_RBAC)", body = WyrdProblem),
        (status = 503, description = "The deployment has no [kms] \
          (WYRD_SERVER_503_SERVICE_UNAVAILABLE)", body = WyrdProblem),
    ),
    tag = "Platform"
)]
#[tracing::instrument(
    level = "info",
    skip(state, caller),
    fields(operation = "platform.stored_secrets.maintain")
)]
async fn maintain_stored_secrets(
    State(state): State<AppState>,
    caller: PlatformCaller,
    Json(request): Json<StoredSecretMaintenanceRequest>,
) -> Result<Json<StoredSecretMaintenanceReport>, WyrdErrorResponse> {
    let pool = operator(&state)?;
    let authz =
        PlatformAuthorization::new(pool.clone(), Arc::clone(state.bifrost.scribe_outbox()) as _);
    let required = Permission::platform_credential_write();
    authz
        .decide(
            &caller.context,
            &required,
            caller.request_id.as_str(),
            PLATFORM_STORED_SECRETS_RESOURCE,
        )
        .map_err(|error| platform_authz_error(&error, &required))?;
    let Some(kms) = state.auth.kms.clone() else {
        return Err(WyrdErrorResponse::from(WyrdError::ServiceUnavailable {
            message: "stored-secret maintenance needs [kms] configured".to_owned(),
            details: serde_json::json!({ "reason": "kms_not_configured" }),
        }));
    };
    StoredSecretMaintenance::new(pool, Some(kms))
        .run_serving(&state, request.mode, request.limit)
        .await
        .map(Json)
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure("stored secret maintenance failed", &error))
        })
}
