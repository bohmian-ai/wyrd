//! Workload OIDC `jwt-bearer` exchange.

use std::sync::Arc;

use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use uuid::Uuid;
use wyrd_auth_oidc::{WorkloadBinding, WorkloadBindingResolver};
use wyrd_auth_verify::{ExternalVerifier, VerifiedExternalIdentity};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::Attribution;
use wyrd_spec::auth::IssuerTokenPolicy;
use wyrd_spec::auth::IssuerUrl;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::api::{AuditDetail, AuditOutcome};
use wyrd_sql::WyrdPostgres;

use crate::audit::{TOKEN_EXCHANGE_OPERATION, auth_event, auth_failure_code};
use crate::error::{auth_error_to_wyrd, store_error};
use crate::issuance::{ExchangedToken, IssuanceError, TenantGrant, TenantTokenIssuer};
use crate::pg_resolvers::{PgIssuerResolver, PgWorkloadBindingResolver};

/// Workload OIDC `jwt-bearer` exchange service.
///
/// Owns only the workload evidence: the provider assertion and its account
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
    /// Exchange a platform-attested workload assertion for a Wyrd token.
    ///
    /// The configured binding selects the account; the root is the explicit
    /// `workload_uid`, else the binding's pinned default, else none. Either
    /// root is validated at issuance under the account's restriction.
    ///
    /// # Errors
    /// Returns `InvalidToken` for an unverifiable or non-workload assertion,
    /// `PrincipalNotFound` for an unbound subject or inactive bound account,
    /// `AuthVerifyUnavailable` when the binding store is unavailable, and
    /// every issuance refusal, including an unusable root.
    #[tracing::instrument(level = "debug", skip(self, postgres, assertion))]
    pub async fn execute(
        &self,
        postgres: &WyrdPostgres,
        tenant_id: DataTenantId,
        assertion: SecretString,
        workload_uid: Option<CardUid>,
        request_id: &str,
    ) -> Result<ExchangedToken, WyrdError> {
        let mut sub_principal_id = None;
        let result = async {
            let verified = self
                .verify_workload_assertion(tenant_id, assertion.expose_secret())
                .await?;
            let binding = self.resolve_workload_binding(tenant_id, &verified).await?;
            let principal_id = binding.principal_id.as_uuid();
            sub_principal_id = Some(principal_id);
            let grant = TenantGrant::JwtBearer {
                workload_uid: workload_uid.or(binding.workload_uid),
            };
            let mut conn = postgres.tenant_conn(tenant_id).await.map_err(store_error)?;
            let exchanged = self
                .issuer
                .issue(&mut conn, principal_id, grant, request_id)
                .await
                .map_err(|error| match error {
                    IssuanceError::PrincipalInactive => {
                        principal_not_found(&verified.subject, &verified.issuer)
                    }
                    error => error.into(),
                })?;
            conn.commit().await.map_err(store_error)?;
            Ok(exchanged)
        }
        .await;

        match result {
            Ok(token) => Ok(token),
            Err(error) => {
                self.audit_workload_failure(tenant_id, sub_principal_id, request_id, &error);
                Err(error)
            }
        }
    }

    /// Stage the audit of a refused workload `jwt-bearer` exchange.
    ///
    /// Stages one denied `auth.token.exchange` event with the closed failure
    /// code on the issuer's process audit stage, attributed to the resolved
    /// service account or, when none was resolved, to the unresolved-identity
    /// sentinel pair. Staging never waits and never
    /// replaces the caller's refusal.
    fn audit_workload_failure(
        &self,
        tenant_id: DataTenantId,
        principal_id: Option<Uuid>,
        request_id: &str,
        error: &WyrdError,
    ) {
        let event = auth_event(
            request_id,
            TOKEN_EXCHANGE_OPERATION,
            principal_id.map_or_else(Attribution::unresolved, |id| {
                Attribution::direct(PrincipalId::new(id))
            }),
            PrincipalKindTag::Service,
            None,
            AuditOutcome::Denied,
            AuditDetail::AuthFailure {
                error_code: auth_failure_code(error),
            },
        );
        self.issuer.audit().stage(tenant_id, event);
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
    ) -> Result<WorkloadBinding, WyrdError> {
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

/// The refusal for an unbound subject or an inactive bound account, kept
/// identical so neither is distinguishable.
fn principal_not_found(subject: &str, issuer: &IssuerUrl) -> WyrdError {
    WyrdError::PrincipalNotFound {
        message: "no active Service account is bound to this workload subject".to_owned(),
        details: json!({
            "issuer": issuer.as_str(),
            "subject": subject,
        }),
    }
}

fn invalid_token(message: &str) -> WyrdError {
    WyrdError::InvalidToken {
        message: message.to_owned(),
        details: json!({}),
    }
}
