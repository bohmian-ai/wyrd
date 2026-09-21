//! Workload OIDC `jwt-bearer` exchange.

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_oidc::WorkloadBindingResolver;
use wyrd_auth_verify::{ExternalVerifier, VerifiedExternalIdentity};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::error::WyrdError;
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::queries::auth::service_account_by_card_ref;
use wyrd_sql::{TenantConn, WyrdPostgres};

use crate::audit::{
    TOKEN_EXCHANGE_OPERATION, auth_event, auth_failure_code, record_auth_audit_best_effort,
};
use crate::error::auth_error_to_wyrd;
use crate::issuance::{ExchangedToken, IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::issue_api_key::principal_kind_for_card;
use crate::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};

/// Workload OIDC `jwt-bearer` exchange service.
///
/// Owns only the workload evidence: the provider assertion and its Card
/// binding. The token itself is minted by the shared [`TenantTokenIssuer`].
#[derive(Clone)]
pub struct JwtBearer {
    /// The tenant issuance owner that mints the workload's token.
    pub issuer: TenantTokenIssuer,
    /// External/OIDC assertion verifier.
    pub verifier: Arc<ExternalVerifier<PgIssuerResolver>>,
    /// Workload binding resolver.
    pub workload_binding_resolver: Arc<PgWorkloadBindingResolver>,
}

impl std::fmt::Debug for JwtBearer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtBearer")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

impl JwtBearer {
    /// Exchange a platform-attested workload assertion for a Wyrd token pair.
    #[tracing::instrument(level = "debug", skip(self, postgres, assertion))]
    pub async fn execute(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
        assertion: SecretString,
        request_id: &str,
    ) -> Result<ExchangedToken, WyrdError> {
        let mut audit_principal_id = Uuid::nil();
        let result = async {
            let verified = self
                .verify_workload_assertion(tenant_id, assertion.expose_secret())
                .await?;
            let card_ref = self.resolve_workload_binding(tenant_id, &verified).await?;
            let mut conn = postgres.tenant_conn(tenant_id).await.map_err(sql_error)?;
            let principal_id = bound_service_account(&mut conn, &card_ref).await?;
            audit_principal_id = principal_id;
            let exchanged = self
                .issuer
                .issue(&mut conn, principal_id, TenantGrant::JwtBearer, request_id)
                .await
                .map_err(|error| match error {
                    IssuanceError::PrincipalInactive => principal_not_found_for_card_ref(&card_ref),
                    error => error.into(),
                })?;
            conn.commit().await.map_err(sql_error)?;
            Ok(exchanged)
        }
        .await;

        match result {
            Ok(token) => Ok(token),
            Err(error) => {
                audit_workload_failure(postgres, tenant_id, audit_principal_id, request_id, &error)
                    .await;
                Err(error)
            }
        }
    }

    async fn verify_workload_assertion(
        &self,
        tenant_id: DataTenantId,
        assertion: &str,
    ) -> Result<VerifiedExternalIdentity, WyrdError> {
        let verified = self
            .verifier
            .verify_external(&tenant_id, assertion)
            .await
            .map_err(auth_error_to_wyrd)?;
        if verified.principal_kind != IssuerTokenPolicy::Workload {
            return Err(invalid_token(
                "issuer is not configured for workload identity",
            ));
        }
        Ok(verified)
    }

    async fn resolve_workload_binding(
        &self,
        tenant_id: DataTenantId,
        verified: &VerifiedExternalIdentity,
    ) -> Result<CardRef, WyrdError> {
        self.workload_binding_resolver
            .binding(
                &tenant_id,
                &verified.issuer,
                &verified.subject,
                Some(verified.expected_audience.as_str()),
            )
            .await
            .map_err(|error| {
                tracing::warn!(
                    error = %error,
                    tenant_id = %tenant_id,
                    issuer = %verified.issuer,
                    subject = %verified.subject,
                    "workload binding lookup failed"
                );
                WyrdError::AuthVerifyUnavailable {
                    message: "workload binding lookup unavailable".to_owned(),
                    details: json!({ "retry_after_seconds": 1 }),
                }
            })?
            .ok_or_else(|| principal_not_found(&verified.subject, &verified.issuer))
    }
}

/// Resolve the service account bound to the workload's Card.
///
/// The binding lookup only selects active rows; the shared issuer re-reads
/// the principal inside the issuing transaction and decides activity there.
///
/// # Errors
/// Returns `PrincipalNotFound` when no Service or Agent principal is bound to
/// `card_ref`, and `AuthVerifyUnavailable` when the store read fails.
async fn bound_service_account(
    conn: &mut TenantConn<'_>,
    card_ref: &CardRef,
) -> Result<Uuid, WyrdError> {
    let principal_kind = principal_kind_for_card(card_ref).map_err(WyrdError::from)?;
    service_account_by_card_ref(conn, principal_kind, card_ref)
        .await
        .map_err(sql_error)?
        .map(|row| row.id)
        .ok_or_else(|| principal_not_found_for_card_ref(card_ref))
}

/// Best-effort audit of a refused workload `jwt-bearer` exchange.
///
/// Stages one denied `auth.token.exchange` event with the closed failure code
/// in its own transaction; `principal_id` is nil when the service account was
/// never resolved. Staging failures are logged, never returned.
async fn audit_workload_failure(
    postgres: &WyrdPostgres,
    tenant_id: DataTenantId,
    principal_id: Uuid,
    request_id: &str,
    error: &WyrdError,
) {
    let event = auth_event(
        request_id,
        TOKEN_EXCHANGE_OPERATION,
        PrincipalId::new(principal_id),
        PrincipalKindTag::Service,
        None,
        AuditOutcome::Denied,
        AuditDetail::AuthFailure {
            error_code: auth_failure_code(error),
        },
    );
    record_auth_audit_best_effort(postgres.app_pool(), tenant_id, &event).await;
}

fn principal_not_found(subject: &str, issuer: &IssuerUrl) -> WyrdError {
    WyrdError::PrincipalNotFound {
        message: "no active Service or Agent principal is bound to card_ref".to_owned(),
        details: json!({
            "issuer": issuer.as_str(),
            "subject": subject,
        }),
    }
}

fn principal_not_found_for_card_ref(card_ref: &CardRef) -> WyrdError {
    WyrdError::PrincipalNotFound {
        message: "no active Service or Agent principal is bound to card_ref".to_owned(),
        details: json!({ "card_ref": card_ref }),
    }
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: json!({}),
    }
}

fn sql_error(error: impl Into<wyrd_sql::SqlError>) -> WyrdError {
    let error = error.into();
    tracing::warn!(error = %error, "auth db unavailable");
    WyrdError::AuthVerifyUnavailable {
        message: "auth backend unavailable".to_owned(),
        details: json!({ "retry_after_seconds": 1 }),
    }
}
