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
    /// them.
    ///
    /// # Errors
    /// Returns [`SqlError`] when a read fails.
    pub async fn holds_envelopes(&self) -> Result<bool, SqlError> {
        for table in StoredSecretTable::ALL {
            if !stored_secrets(&self.operator, table).await?.is_empty() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Run one bounded pass moving every observed envelope that does not
    /// name the active key onto it.
    ///
    /// The pass attempts at most `limit` transitions and stops starting new
    /// key-service calls once `budget` elapses; envelopes it does not reach
    /// count as `remaining`, and `bounded` reports the cut. Rows already
    /// swapped stay swapped, so repeating the pass resumes from the rest.
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
        };
        for table in StoredSecretTable::ALL {
            for row in &stored_secrets(&self.operator, table).await? {
                self.settle(&mut pass, table, row).await?;
            }
        }
        let report = pass.report;
        tracing::info!(
            changed = report.changed,
            current = report.current,
            remaining = report.remaining,
            bounded = report.bounded,
            "stored secret maintenance pass finished"
        );
        Ok(report)
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
                if swap_stored_secret(&self.operator, table, row, &envelope).await? {
                    pass.report.changed += 1;
                } else {
                    pass.fail(table, row, StoredSecretFailureReason::ConcurrentChange);
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
    /// When the pass stops starting key-service calls.
    deadline: Instant,
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
/// The pass returns its report before the request timeout: it stops starting
/// key-service calls after half of `limits.timeout_ms`, and envelopes it did
/// not reach count as `remaining`. Repeat the same mode until `remaining` is
/// zero or only failures are left.
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
    // ponytail: half the request timeout leaves room to answer before the edge
    // drops the handler; a configurable budget if operators need longer passes.
    let budget = state.settings.limits.timeout / 2;
    StoredSecretMaintenance::new(pool, Some(kms))
        .run(request.mode, request.limit, budget)
        .await
        .map(Json)
        .map_err(|error| {
            WyrdErrorResponse::from(internal_failure("stored secret maintenance failed", &error))
        })
}
