//! Gateway administration owner: authorization, audit, persistence, views.

use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::num::NonZeroU64;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Error as SqlxError;
use wyrd_gateway::{
    GatewayCredentialSnapshot, GatewayCredentialSource, GatewayTenantSnapshot, ManagedSecretBinding,
};
use wyrd_kms::{KmsError, WyrdKms};
use wyrd_runtime::Permission;
use wyrd_spec::DataTenantId;
use wyrd_spec::error::WyrdError;
use wyrd_spec::gateway::{
    GatewayCaptureMode, GatewayCapturePolicy, GatewayCapturePolicyWrite, GatewayContractError,
    GatewayFallbackPolicy, GatewayGovernancePolicy, GatewayModelPricing,
    ProviderCredentialSourceView, ProviderCredentialState, ProviderCredentialView,
    ProviderCredentialWrite, ProviderCredentialWriteSource, ProviderDeployment,
    ProviderDeploymentsApplied, ProviderDeploymentsApply,
};
use wyrd_spec::ids::{ProviderCredentialName, ProviderDeploymentName, ProviderId};
use wyrd_sql::queries::gateway::{
    GatewayCredentialWrite, GatewayDeploymentWrite, GatewayPricingWrite, delete_gateway_credential,
    delete_gateway_deployment, gateway_credential, gateway_credential_for_share,
    gateway_credentials, gateway_deployment, gateway_deployments, gateway_policies,
    gateway_pricing, gateway_snapshot, lock_gateway_policies, revoke_gateway_credential,
    update_gateway_policies, upsert_gateway_credential, upsert_gateway_deployment,
    upsert_gateway_pricing,
};
use wyrd_sql::row_types::gateway::{GatewayCredentialRow, GatewayPricingRow};
use wyrd_sql::{SqlError, TenantConn, WyrdPostgres};

use crate::audit;
use crate::components::auth::Caller;
use crate::config::GatewayConfig;
use crate::postgres::ServerPostgres;
use crate::state::AppState;

/// Dependency-owning handle for every tenant gateway administration operation.
///
/// Each operation runs through [`Self::audited`]: the caller is authorized
/// against the typed gateway permission, a denial is audited in its own
/// transaction, and the allowed decision is recorded exactly once — with the
/// operation when it commits, or standalone when it fails.
pub struct GatewayAdministration<'a> {
    /// Server state carrying the canonical authorization and audit owner.
    state: &'a AppState,
    /// Tenant-scoped Postgres access.
    postgres: &'a ServerPostgres,
    /// Operator-declared bindings and backends.
    config: &'a GatewayConfig,
    /// Deployment stored-secret facade protecting managed secrets; `None`
    /// without `[kms]`, which refuses managed-secret writes.
    kms: Option<&'a WyrdKms>,
}

impl<'a> GatewayAdministration<'a> {
    /// Borrows the administration dependencies from server state.
    #[must_use]
    pub fn new(state: &'a AppState) -> Self {
        Self {
            state,
            postgres: &state.postgres,
            config: state.gateway.config(),
            kms: state.auth.kms.as_deref(),
        }
    }

    /// Creates an active credential or rotates an active one.
    ///
    /// Environment and Vault sources must fall within an operator assignment
    /// for the caller's tenant and the credential provider. A `ManagedSecret`
    /// value is sealed through the deployment stored-secret facade, bound to
    /// the caller's tenant, name, and provider, before the row is written,
    /// so the submitted plaintext exists only for the duration of this call.
    /// Wyrd stores only the redacted source and, for a managed secret, its
    /// opaque envelope; the response is the redacted view either way. A
    /// failed seal or write leaves the previously committed value usable.
    ///
    /// # Errors
    /// Returns a permission error when the caller lacks `gateway:write`,
    /// `GatewayInvalidConfiguration` for a path/body name mismatch, an empty
    /// submitted secret, or a source not assigned to the tenant and provider
    /// (undeclared and unauthorized sources are indistinguishable),
    /// `GatewayResourceConflict` when the name is revoked or a provider change
    /// would orphan a deployment, `ServiceUnavailable` when the deployment has
    /// no `[kms]` or it cannot seal, and `ServiceUnavailable` when storage
    /// fails.
    pub async fn put_credential(
        &self,
        caller: &Caller,
        name: &ProviderCredentialName,
        write: ProviderCredentialWrite,
    ) -> Result<ProviderCredentialView, WyrdError> {
        let resource = format!("gateway_provider_credential:{name}");
        let write = &write;
        self.audited_prepared(
            caller,
            &Permission::gateway_write(),
            "gateway.provider_credential.put",
            &resource,
            async {
                require_path_name(name.as_str(), write.name.as_str())?;
                self.prepare_credential(caller, write).await
            },
            |mut conn, sealed| async move {
                let view = self.write_credential(&mut conn, write, sealed).await?;
                conn.commit().await.map_err(unavailable)?;
                Ok(view)
            },
        )
        .await
    }

    /// Reads one redacted credential.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read`,
    /// `GatewayResourceNotFound` when the name is absent, and storage or audit
    /// errors.
    pub async fn credential(
        &self,
        caller: &Caller,
        name: &ProviderCredentialName,
    ) -> Result<ProviderCredentialView, WyrdError> {
        let resource = format!("gateway_provider_credential:{name}");
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.provider_credential.get",
            &resource,
            |mut conn| async move {
                let row = gateway_credential(&mut conn, name.as_str())
                    .await
                    .map_err(unavailable)?
                    .ok_or_else(|| not_found("credential", name.as_str()))?;
                let view = credential_view(row)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(view)
            },
        )
        .await
    }

    /// Lists the tenant's redacted credentials ordered by name.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read` and storage or audit
    /// errors.
    pub async fn credentials(
        &self,
        caller: &Caller,
    ) -> Result<Vec<ProviderCredentialView>, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.provider_credential.list",
            "gateway_provider_credentials",
            |mut conn| async move {
                let views = gateway_credentials(&mut conn)
                    .await
                    .map_err(unavailable)?
                    .into_iter()
                    .map(credential_view)
                    .collect::<Result<_, _>>()?;
                conn.commit().await.map_err(unavailable)?;
                Ok(views)
            },
        )
        .await
    }

    /// Terminally revokes a credential; repeating returns the same revocation.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayResourceNotFound` when the name is absent, and storage or audit
    /// errors.
    pub async fn revoke_credential(
        &self,
        caller: &Caller,
        name: &ProviderCredentialName,
    ) -> Result<ProviderCredentialView, WyrdError> {
        let resource = format!("gateway_provider_credential:{name}");
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.provider_credential.revoke",
            &resource,
            |mut conn| async move {
                let row = revoke_gateway_credential(&mut conn, name.as_str())
                    .await
                    .map_err(unavailable)?
                    .ok_or_else(|| not_found("credential", name.as_str()))?;
                let view = credential_view(row)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(view)
            },
        )
        .await
    }

    /// Deletes an unreferenced credential; deleting an absent name succeeds.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:delete`,
    /// `GatewayResourceConflict` while any deployment references the
    /// credential, and storage or audit errors.
    pub async fn delete_credential(
        &self,
        caller: &Caller,
        name: &ProviderCredentialName,
    ) -> Result<(), WyrdError> {
        let resource = format!("gateway_provider_credential:{name}");
        self.audited(
            caller,
            &Permission::gateway_delete(),
            "gateway.provider_credential.delete",
            &resource,
            |mut conn| async move {
                delete_gateway_credential(&mut conn, name.as_str())
                    .await
                    .map_err(|error| referenced_conflict(error, "credential", name.as_str()))?;
                conn.commit().await.map_err(unavailable)
            },
        )
        .await
    }

    /// Creates or replaces a deployment; only newly admitted calls observe it.
    ///
    /// A credential reference must name an active credential of the same
    /// tenant and provider whose operator assignment permits this tenant,
    /// provider, and — for `OpenAiCompatible` — the exact base URL host. The
    /// credential is share-locked until commit, so a
    /// concurrent revoke or delete serializes behind this write.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayInvalidConfiguration` for a name mismatch, an invalid adapter,
    /// provider, or capability set, or an invalid credential reference, and
    /// storage or audit errors.
    pub async fn put_deployment(
        &self,
        caller: &Caller,
        name: &ProviderDeploymentName,
        deployment: ProviderDeployment,
    ) -> Result<ProviderDeployment, WyrdError> {
        let resource = format!("gateway_provider_deployment:{name}");
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.provider_deployment.put",
            &resource,
            |mut conn| async move {
                require_path_name(name.as_str(), deployment.name.as_str())?;
                self.write_deployment(&mut conn, caller, &deployment)
                    .await?;
                conn.commit().await.map_err(unavailable)?;
                Ok(deployment)
            },
        )
        .await
    }

    /// Reads one deployment.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read`,
    /// `GatewayResourceNotFound` when the name is absent, and storage or audit
    /// errors.
    pub async fn deployment(
        &self,
        caller: &Caller,
        name: &ProviderDeploymentName,
    ) -> Result<ProviderDeployment, WyrdError> {
        let resource = format!("gateway_provider_deployment:{name}");
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.provider_deployment.get",
            &resource,
            |mut conn| async move {
                let document = gateway_deployment(&mut conn, name.as_str())
                    .await
                    .map_err(unavailable)?
                    .ok_or_else(|| not_found("deployment", name.as_str()))?;
                let deployment = decode(document)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(deployment)
            },
        )
        .await
    }

    /// Lists the tenant's deployments ordered by name.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read` and storage or audit
    /// errors.
    pub async fn deployments(&self, caller: &Caller) -> Result<Vec<ProviderDeployment>, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.provider_deployment.list",
            "gateway_provider_deployments",
            |mut conn| async move {
                let deployments = gateway_deployments(&mut conn)
                    .await
                    .map_err(unavailable)?
                    .into_iter()
                    .map(decode)
                    .collect::<Result<_, _>>()?;
                conn.commit().await.map_err(unavailable)?;
                Ok(deployments)
            },
        )
        .await
    }

    /// Deletes a deployment; deleting an absent name succeeds.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:delete` and storage or
    /// audit errors.
    pub async fn delete_deployment(
        &self,
        caller: &Caller,
        name: &ProviderDeploymentName,
    ) -> Result<(), WyrdError> {
        let resource = format!("gateway_provider_deployment:{name}");
        self.audited(
            caller,
            &Permission::gateway_delete(),
            "gateway.provider_deployment.delete",
            &resource,
            |mut conn| async move {
                delete_gateway_deployment(&mut conn, name.as_str())
                    .await
                    .map_err(unavailable)?;
                conn.commit().await.map_err(unavailable)
            },
        )
        .await
    }

    /// Creates or replaces credentials and the deployments using them in one
    /// tenant transaction.
    ///
    /// The document is validated whole and every credential validated and
    /// sealed first, outside any transaction; then each credential and each
    /// deployment is written with the same reference checks as
    /// [`Self::put_credential`] and [`Self::put_deployment`] on one shared
    /// connection, so a deployment observes the submitted credentials and
    /// share-locks every referenced row until commit. Any refusal drops the
    /// transaction, leaving every named resource as it was; names absent from
    /// the document are untouched.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayInvalidConfiguration` for an invalid document, source, or
    /// credential reference, `GatewayResourceConflict` for a revoked
    /// credential name or orphaning provider change, and `ServiceUnavailable`
    /// when sealing or storage fails.
    pub async fn apply_deployments(
        &self,
        caller: &Caller,
        apply: ProviderDeploymentsApply,
    ) -> Result<ProviderDeploymentsApplied, WyrdError> {
        self.audited_prepared(
            caller,
            &Permission::gateway_write(),
            "gateway.provider_deployments.apply",
            "gateway_provider_deployments",
            async move {
                apply.validate().map_err(invalid)?;
                let mut sealed = Vec::with_capacity(apply.credentials.len());
                for write in &apply.credentials {
                    sealed.push(self.prepare_credential(caller, write).await?);
                }
                Ok((apply, sealed))
            },
            |mut conn, (apply, sealed)| async move {
                let mut credentials = Vec::with_capacity(apply.credentials.len());
                for (write, sealed) in apply.credentials.iter().zip(sealed) {
                    credentials.push(self.write_credential(&mut conn, write, sealed).await?);
                }
                for deployment in &apply.deployments {
                    self.write_deployment(&mut conn, caller, deployment).await?;
                }
                conn.commit().await.map_err(unavailable)?;
                Ok(ProviderDeploymentsApplied {
                    credentials,
                    deployments: apply.deployments,
                })
            },
        )
        .await
    }

    /// Replaces the tenant fallback policy.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayInvalidConfiguration` for an invalid policy, and storage or
    /// audit errors.
    pub async fn put_fallback(
        &self,
        caller: &Caller,
        policy: GatewayFallbackPolicy,
    ) -> Result<GatewayFallbackPolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.fallback_policy.put",
            "gateway_fallback_policy",
            |mut conn| async move {
                policy.validate().map_err(invalid)?;
                let mut policies = lock_gateway_policies(&mut conn)
                    .await
                    .map_err(unavailable)?;
                policies.fallback = Some(encode(&policy)?);
                update_gateway_policies(&mut conn, &policies)
                    .await
                    .map_err(unavailable)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(policy)
            },
        )
        .await
    }

    /// Reads the tenant fallback policy, or the empty default.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read` and storage or audit
    /// errors.
    pub async fn fallback(&self, caller: &Caller) -> Result<GatewayFallbackPolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.fallback_policy.get",
            "gateway_fallback_policy",
            |mut conn| async move {
                let policies = gateway_policies(&mut conn).await.map_err(unavailable)?;
                let policy = decode_or_default(policies.fallback)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(policy)
            },
        )
        .await
    }

    /// Restores the empty fallback policy; repeating succeeds.
    ///
    /// A reset is a policy change, so it requires `gateway:write`.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write` and storage or audit
    /// errors.
    pub async fn delete_fallback(&self, caller: &Caller) -> Result<(), WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.fallback_policy.delete",
            "gateway_fallback_policy",
            |mut conn| async move {
                let mut policies = lock_gateway_policies(&mut conn)
                    .await
                    .map_err(unavailable)?;
                policies.fallback = None;
                update_gateway_policies(&mut conn, &policies)
                    .await
                    .map_err(unavailable)?;
                conn.commit().await.map_err(unavailable)
            },
        )
        .await
    }

    /// Replaces the tenant governance policy and returns the stored result.
    ///
    /// Submitted pricing versions must match any stored version exactly
    /// (only `active` may change). Omitted versions are retained inactive so
    /// admitted calls and accounting history keep the version they hold. The
    /// merged policy is validated as a whole.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayInvalidConfiguration` for an invalid policy or a changed
    /// pricing version, and storage or audit errors.
    pub async fn put_governance(
        &self,
        caller: &Caller,
        policy: GatewayGovernancePolicy,
    ) -> Result<GatewayGovernancePolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.governance_policy.put",
            "gateway_governance_policy",
            |mut conn| async move {
                policy.validate().map_err(invalid)?;
                replace_governance(&mut conn, policy).await?;
                let stored = load_governance(&mut conn).await?;
                conn.commit().await.map_err(unavailable)?;
                Ok(stored)
            },
        )
        .await
    }

    /// Reads the tenant governance policy, or the empty allow-unpriced default.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read` and storage or audit
    /// errors.
    pub async fn governance(&self, caller: &Caller) -> Result<GatewayGovernancePolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.governance_policy.get",
            "gateway_governance_policy",
            |mut conn| async move {
                let stored = load_governance(&mut conn).await?;
                conn.commit().await.map_err(unavailable)?;
                Ok(stored)
            },
        )
        .await
    }

    /// Clears limits and budgets, restores `allow_unpriced`, and retires all
    /// pricing while retaining it; repeating succeeds.
    ///
    /// A reset is a policy change, so it requires `gateway:write`.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write` and storage or audit
    /// errors.
    pub async fn delete_governance(&self, caller: &Caller) -> Result<(), WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.governance_policy.delete",
            "gateway_governance_policy",
            |mut conn| async move {
                replace_governance(&mut conn, GatewayGovernancePolicy::default()).await?;
                conn.commit().await.map_err(unavailable)
            },
        )
        .await
    }

    /// Replaces the capture policy, bumping `version` only on a content change.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:write`,
    /// `GatewayInvalidConfiguration` for an invalid mode/field combination,
    /// and storage or audit errors.
    pub async fn put_capture(
        &self,
        caller: &Caller,
        write: GatewayCapturePolicyWrite,
    ) -> Result<GatewayCapturePolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_write(),
            "gateway.capture_policy.put",
            "gateway_capture_policy",
            |mut conn| async move {
                write.validate().map_err(invalid)?;
                let mut policies = lock_gateway_policies(&mut conn)
                    .await
                    .map_err(unavailable)?;
                let current = capture_from(policies.capture.take())?;
                if current.mode == write.mode && current.payload_fields == write.payload_fields {
                    conn.commit().await.map_err(unavailable)?;
                    return Ok(current);
                }
                let version = current
                    .version
                    .checked_add(1)
                    .ok_or_else(|| internal("capture policy version overflow"))?;
                let next = GatewayCapturePolicy {
                    mode: write.mode,
                    payload_fields: write.payload_fields,
                    version,
                };
                policies.capture = Some(encode(&next)?);
                update_gateway_policies(&mut conn, &policies)
                    .await
                    .map_err(unavailable)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(next)
            },
        )
        .await
    }

    /// Reads the capture policy, or the disabled version-1 default.
    ///
    /// # Errors
    /// Returns a permission error without `gateway:read` and storage or audit
    /// errors.
    pub async fn capture(&self, caller: &Caller) -> Result<GatewayCapturePolicy, WyrdError> {
        self.audited(
            caller,
            &Permission::gateway_read(),
            "gateway.capture_policy.get",
            "gateway_capture_policy",
            |mut conn| async move {
                let policies = gateway_policies(&mut conn).await.map_err(unavailable)?;
                let policy = capture_from(policies.capture)?;
                conn.commit().await.map_err(unavailable)?;
                Ok(policy)
            },
        )
        .await
    }

    /// The immutable snapshot of the tenant configuration.
    ///
    /// Served from the process [`GatewaySnapshots`](super::snapshots::GatewaySnapshots)
    /// cache with no IO while it holds the tenant; otherwise loaded once,
    /// shared by every call missing the tenant at once, and cached. Call
    /// admission, model listing and Batches all use it. It performs no
    /// authorization: callers use it only after verifying the caller.
    ///
    /// # Errors
    /// Returns the load's error from
    /// [`GatewaySnapshots::load_through`](super::snapshots::GatewaySnapshots::load_through),
    /// such as `ServiceUnavailable` when storage fails.
    pub async fn snapshot(
        &self,
        tenant: DataTenantId,
    ) -> Result<Arc<GatewayTenantSnapshot>, WyrdError> {
        self.state
            .gateway
            .snapshots()
            .load_through(self.postgres.wyrd(), self.config, tenant)
            .await
    }

    /// Makes `tenant`'s committed configuration and a shared usage view
    /// serve on this server right after an administrative write commits, so
    /// the administrator's next call needs no background load. Each reload
    /// starts after the commit, so one racing that commit's own notification
    /// is retried; failures leave the background reload to recover.
    pub(crate) async fn warm(&self, tenant: DataTenantId) {
        let snapshots = self.state.gateway.snapshots();
        for _ in 0..3 {
            if matches!(
                snapshots
                    .reload(self.postgres.wyrd(), self.config, tenant)
                    .await,
                Ok(true)
            ) {
                break;
            }
        }
        if let Err(error) = self
            .state
            .gateway
            .usage()
            .sync(self.postgres.wyrd(), tenant)
            .await
        {
            tracing::warn!(tenant = %tenant.as_uuid(), error = %error, "gateway usage refresh failed");
        }
    }

    /// Authorizes one operation, stages its decision, and runs it; see
    /// [`Self::audited_prepared`].
    ///
    /// # Errors
    /// Returns the mapped permission denial, an unavailable store, or `work`'s
    /// own error.
    async fn audited<T, Fut>(
        &self,
        caller: &Caller,
        required: &Permission,
        operation: &str,
        resource: &str,
        work: impl FnOnce(TenantConn<'a>) -> Fut,
    ) -> Result<T, WyrdError>
    where
        Fut: Future<Output = Result<T, WyrdError>>,
    {
        self.audited_prepared(
            caller,
            required,
            operation,
            resource,
            async { Ok(()) },
            |conn, ()| work(conn),
        )
        .await
    }

    /// Authorizes one operation, stages its decision, runs `prepare`, and
    /// then runs `work` with its result.
    ///
    /// The decision is staged on the process audit outbox exactly once, before
    /// any work starts; a denial is refused there and `prepare` never runs.
    /// `prepare` runs before any connection is taken, so key-service waits
    /// such as sealing hold no transaction or lock. A fresh tenant transaction
    /// is then handed to `work`, which MUST commit it as its final fallible
    /// step. A successful write then warms this server's caches
    /// ([`Self::warm`]).
    ///
    /// # Errors
    /// Returns the mapped permission denial, `prepare`'s error, an
    /// unavailable store, or `work`'s own error.
    async fn audited_prepared<T, P, Prep, Fut>(
        &self,
        caller: &Caller,
        required: &Permission,
        operation: &str,
        resource: &str,
        prepare: Prep,
        work: impl FnOnce(TenantConn<'a>, P) -> Fut,
    ) -> Result<T, WyrdError>
    where
        Prep: Future<Output = Result<P, WyrdError>>,
        Fut: Future<Output = Result<T, WyrdError>>,
    {
        audit::authorize(self.state, caller, required, operation, resource)?;
        let prepared = prepare.await?;
        let conn = self
            .postgres
            .tenant_conn(caller.data_tenant_id)
            .await
            .map_err(unavailable)?;
        let result = work(conn, prepared).await;
        if result.is_ok() && *required == Permission::gateway_write() {
            self.warm(caller.data_tenant_id).await;
        }
        result
    }

    /// Validates `write`'s source for the caller and seals a submitted
    /// managed secret, before any transaction opens.
    ///
    /// # Errors
    /// Returns the source and sealing errors of [`Self::put_credential`].
    async fn prepare_credential(
        &self,
        caller: &Caller,
        write: &ProviderCredentialWrite,
    ) -> Result<Option<Vec<u8>>, WyrdError> {
        let view = ProviderCredentialSourceView::from(&write.source);
        self.validate_source(caller.data_tenant_id, &write.provider, &view)?;
        self.seal(caller.data_tenant_id, &write.name, write).await
    }

    /// Upserts one prepared credential, with the envelope
    /// [`Self::prepare_credential`] sealed, on the caller's open transaction,
    /// returning its redacted view; the caller commits.
    ///
    /// # Errors
    /// Returns the revoked-name and storage errors of
    /// [`Self::put_credential`].
    async fn write_credential(
        &self,
        conn: &mut TenantConn<'_>,
        write: &ProviderCredentialWrite,
        sealed: Option<Vec<u8>>,
    ) -> Result<ProviderCredentialView, WyrdError> {
        let name = write.name.as_str();
        let view = ProviderCredentialSourceView::from(&write.source);
        let source = encode(&view)?;
        let row = upsert_gateway_credential(
            conn,
            GatewayCredentialWrite {
                name,
                provider: write.provider.as_str(),
                source: &source,
                secret: sealed.as_deref(),
            },
        )
        .await
        .map_err(|error| referenced_conflict(error, "credential", name))?
        .ok_or_else(|| {
            conflict(
                "provider credential is revoked and cannot be replaced",
                name,
            )
        })?;
        credential_view(row)
    }

    /// Validates one deployment and its credential reference, share-locking
    /// the credential, and upserts it on the caller's open transaction; the
    /// caller commits.
    ///
    /// # Errors
    /// Returns the document, reference, and storage errors of
    /// [`Self::put_deployment`].
    async fn write_deployment(
        &self,
        conn: &mut TenantConn<'_>,
        caller: &Caller,
        deployment: &ProviderDeployment,
    ) -> Result<(), WyrdError> {
        deployment.validate().map_err(invalid)?;
        let mut credential_provider = None;
        if let Some(credential) = deployment.auth.credential() {
            let row = gateway_credential_for_share(conn, credential.as_str())
                .await
                .map_err(unavailable)?;
            let reason = match row {
                None => Some("does not name a provider credential in this tenant"),
                Some(row) if row.state != "active" => Some("names a revoked provider credential"),
                Some(row) => match ProviderId::new(&row.provider) {
                    Ok(provider) if deployment.adapter.accepts_credential_provider(&provider) => {
                        let source: ProviderCredentialSourceView = decode(row.source)?;
                        // A managed secret is the tenant's own value, so
                        // it carries no operator assignment to confine.
                        let assigned =
                            matches!(source, ProviderCredentialSourceView::ManagedSecret)
                                || self.config.assignment(&source).is_some_and(|assignment| {
                                    assignment.permits(
                                        caller.data_tenant_id,
                                        &provider,
                                        Some(&deployment.adapter),
                                    )
                                });
                        credential_provider = Some(row.provider);
                        (!assigned).then_some(
                            "names a credential not assigned to this tenant, provider, and endpoint host",
                        )
                    }
                    _ => Some("names a credential for a provider this adapter does not serve"),
                },
            };
            if let Some(reason) = reason {
                return Err(invalid(GatewayContractError::new(
                    "auth.credential",
                    reason,
                )));
            }
        }
        let document = encode(deployment)?;
        upsert_gateway_deployment(
            conn,
            GatewayDeploymentWrite {
                name: deployment.name.as_str(),
                credential_name: deployment
                    .auth
                    .credential()
                    .map(ProviderCredentialName::as_str),
                credential_provider: credential_provider.as_deref(),
                deployment: &document,
            },
        )
        .await
        .map_err(unavailable)?;
        Ok(())
    }

    /// Checks that a credential source lies within an operator assignment for
    /// `tenant` and `provider`.
    ///
    /// Unknown bindings, backends, and Vault paths and those assigned to
    /// another tenant or provider produce one identical rejection, so a
    /// tenant cannot probe which assignments exist. A managed secret has no
    /// operator assignment to check: the submitting tenant is its authority,
    /// and [`Self::seal`] refuses it when no `[kms]` is configured.
    ///
    /// # Errors
    /// Returns `GatewayInvalidConfiguration` for field `source` when no
    /// assignment permits the combination.
    fn validate_source(
        &self,
        tenant: DataTenantId,
        provider: &ProviderId,
        source: &ProviderCredentialSourceView,
    ) -> Result<(), WyrdError> {
        if matches!(source, ProviderCredentialSourceView::ManagedSecret) {
            return Ok(());
        }
        let assigned = self
            .config
            .assignment(source)
            .is_some_and(|assignment| assignment.permits(tenant, provider, None));
        if assigned {
            Ok(())
        } else {
            Err(invalid(GatewayContractError::new(
                "source",
                "is not an operator-assigned source for this tenant and provider",
            )))
        }
    }

    /// Seals a submitted `ManagedSecret` through the deployment stored-secret
    /// facade, or returns `None` for an operator-owned source that carries no
    /// value.
    ///
    /// The envelope authenticates the tenant, credential name, and provider,
    /// so it is unusable under any other identity.
    ///
    /// # Errors
    /// Returns `GatewayInvalidConfiguration` for field `source.secret` when
    /// the submitted value is empty, and `ServiceUnavailable` when the
    /// deployment has no `[kms]` or it cannot protect the value.
    /// Neither error discloses the value, the key, or another tenant's
    /// configuration.
    async fn seal(
        &self,
        tenant: DataTenantId,
        name: &ProviderCredentialName,
        write: &ProviderCredentialWrite,
    ) -> Result<Option<Vec<u8>>, WyrdError> {
        let ProviderCredentialWriteSource::ManagedSecret { secret } = &write.source else {
            return Ok(None);
        };
        let value = secret.expose().trim();
        if value.is_empty() {
            return Err(invalid(GatewayContractError::new(
                "source.secret",
                "must not be empty",
            )));
        }
        let protection = || WyrdError::ServiceUnavailable {
            message: "gateway credential protection is unavailable".to_owned(),
            details: json!({ "resource": "credential" }),
        };
        let kms = self.kms.ok_or_else(|| {
            tracing::warn!("gateway managed secret refused: no [kms] is configured");
            protection()
        })?;
        ManagedSecretBinding {
            tenant,
            name,
            provider: &write.provider,
        }
        .seal(kms, value)
        .await
        .map(Some)
        .map_err(|_| protection())
    }
}

/// Credential object as emitted by the snapshot statement.
#[derive(Deserialize)]
struct StoredCredential {
    /// Credential name.
    name: ProviderCredentialName,
    /// Provider identity.
    provider: ProviderId,
    /// Redacted source.
    source: ProviderCredentialSourceView,
    /// Lifecycle state.
    state: ProviderCredentialState,
    /// Base64 stored-secret envelope of a managed secret; absent otherwise.
    secret_ciphertext: Option<String>,
}

/// Reads `tenant`'s gateway configuration in one SQL statement, so the
/// snapshot is consistent even while administrators replace resources, and
/// attaches `config`'s credential bindings. Environment bindings carry only
/// the operator reference and external secrets only their opaque backend
/// reference.
///
/// With `kms`, each managed secret's envelope is opened after the statement's
/// transaction commits, so no key-service wait holds a connection or lock;
/// without it, managed secrets stay unprepared. Returns the snapshot and
/// whether an open failed retryably, so the caller can load again.
///
/// # Errors
/// Returns `ServiceUnavailable` when storage fails and `Internal` when a
/// stored document no longer decodes.
pub(super) async fn load_snapshot(
    postgres: &WyrdPostgres,
    config: &GatewayConfig,
    tenant: DataTenantId,
    kms: Option<&WyrdKms>,
) -> Result<(GatewayTenantSnapshot, bool), WyrdError> {
    let mut conn = postgres.tenant_conn(tenant).await.map_err(unavailable)?;
    let row = gateway_snapshot(&mut conn).await.map_err(unavailable)?;
    conn.commit().await.map_err(unavailable)?;
    let stored: Vec<StoredCredential> = decode(row.credentials)?;
    let mut governance: GatewayGovernancePolicy = decode_or_default(row.governance)?;
    governance.pricing = decode(row.pricing)?;
    let mut retry = false;
    let mut credentials = Vec::with_capacity(stored.len());
    for credential in stored {
        let (snapshot, retryable) = credential_snapshot(config, tenant, kms, credential).await;
        retry |= retryable;
        credentials.push(snapshot);
    }
    Ok((
        GatewayTenantSnapshot {
            deployments: decode(row.deployments)?,
            credentials,
            fallback: decode_or_default(row.fallback)?,
            governance,
            capture: capture_from(row.capture)?,
        },
        retry,
    ))
}

/// Builds one credential's runtime resolution inputs, attaching the
/// operator reference and the assignment the resolver re-checks before
/// dispatch, and opening a managed secret with `kms` when given.
///
/// Returns the snapshot and whether its managed secret failed to open
/// retryably. A refused envelope stays unprepared and resolves as
/// unavailable; other credentials are unaffected.
async fn credential_snapshot(
    config: &GatewayConfig,
    tenant: DataTenantId,
    kms: Option<&WyrdKms>,
    stored: StoredCredential,
) -> (GatewayCredentialSnapshot, bool) {
    let StoredCredential {
        name,
        provider,
        source,
        state,
        secret_ciphertext,
    } = stored;
    let assignment = config.assignment(&source).cloned();
    let mut retry = false;
    let source = match source {
        ProviderCredentialSourceView::Environment { binding } => {
            GatewayCredentialSource::Environment {
                secret: config
                    .credential_bindings
                    .get(&binding)
                    .map(|configured| configured.secret.clone()),
                binding,
            }
        }
        ProviderCredentialSourceView::ExternalSecret { backend, reference } => {
            GatewayCredentialSource::ExternalSecret { backend, reference }
        }
        ProviderCredentialSourceView::ManagedSecret => {
            let mut secret = None;
            if let Some(kms) = kms {
                let binding = ManagedSecretBinding {
                    tenant,
                    name: &name,
                    provider: &provider,
                };
                match binding.open(kms, &sealed(secret_ciphertext)).await {
                    Ok(opened) => secret = Some(opened),
                    Err(error) => {
                        retry = matches!(error, KmsError::Unavailable { .. });
                        tracing::warn!(
                            tenant = %tenant.as_uuid(), credential = %name, %error,
                            "gateway managed secret could not be prepared"
                        );
                    }
                }
            }
            GatewayCredentialSource::ManagedSecret { secret }
        }
    };
    (
        GatewayCredentialSnapshot {
            name,
            provider,
            state,
            source,
            assignment,
        },
        retry,
    )
}

/// Decodes the envelope of a managed secret from its snapshot column.
///
/// The database `CHECK` constraint keeps the envelope present exactly with the
/// managed source, so a row reaching here without it is corrupt. An absent or
/// undecodable column is still safe: the empty envelope cannot be opened, so
/// resolution fails closed rather than resolving some other value.
fn sealed(ciphertext: Option<String>) -> Vec<u8> {
    ciphertext
        .and_then(|value| BASE64_STANDARD.decode(value).ok())
        .unwrap_or_default()
}

/// Replaces governance on the locked policy row, merging pricing history.
///
/// Pricing is never deleted: an omitted version is retained inactive, so a
/// call admitted on it can still append accounting that references it.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` when a stored pricing version would
/// change or the merged policy is invalid, and storage errors.
async fn replace_governance(
    conn: &mut TenantConn<'_>,
    policy: GatewayGovernancePolicy,
) -> Result<(), WyrdError> {
    let mut policies = lock_gateway_policies(conn).await.map_err(unavailable)?;
    let existing = gateway_pricing(conn)
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(pricing_entry)
        .collect::<Result<Vec<_>, _>>()?;
    let mut merged = policy;
    for (index, entry) in merged.pricing.iter().enumerate() {
        let changed = existing.iter().any(|stored| {
            stored.model == entry.model
                && stored.version == entry.version
                && !stored.same_version_content(entry)
        });
        if changed {
            return Err(invalid(GatewayContractError::new(
                format!("pricing[{index}]"),
                "a pricing version is immutable",
            )));
        }
    }
    let retained: Vec<GatewayModelPricing> = existing
        .into_iter()
        .filter(|stored| {
            !merged
                .pricing
                .iter()
                .any(|entry| entry.model == stored.model && entry.version == stored.version)
        })
        .map(|stored| GatewayModelPricing {
            active: false,
            ..stored
        })
        .collect();
    merged.pricing.extend(retained);
    merged.validate().map_err(invalid)?;

    for entry in &merged.pricing {
        let document = encode(entry)?;
        upsert_gateway_pricing(
            conn,
            GatewayPricingWrite {
                deployment: entry.model.deployment.as_str(),
                model: entry.model.model.as_str(),
                version: &entry.version,
                effective_at: entry.effective_at,
                active: entry.active,
                entry: &document,
            },
        )
        .await
        .map_err(unavailable)?;
    }
    policies.governance = Some(encode(&GatewayGovernancePolicy {
        pricing: Vec::new(),
        ..merged
    })?);
    update_gateway_policies(conn, &policies)
        .await
        .map_err(unavailable)
}

/// Reads governance with its pricing rows.
///
/// # Errors
/// Returns storage errors and `Internal` for undecodable documents.
async fn load_governance(conn: &mut TenantConn<'_>) -> Result<GatewayGovernancePolicy, WyrdError> {
    let policies = gateway_policies(conn).await.map_err(unavailable)?;
    let pricing = gateway_pricing(conn).await.map_err(unavailable)?;
    let mut governance: GatewayGovernancePolicy = decode_or_default(policies.governance)?;
    governance.pricing = pricing
        .into_iter()
        .map(pricing_entry)
        .collect::<Result<_, _>>()?;
    Ok(governance)
}

/// Decodes a pricing row with its authoritative `active` flag.
///
/// # Errors
/// Returns `Internal` when the entry does not decode.
fn pricing_entry(row: GatewayPricingRow) -> Result<GatewayModelPricing, WyrdError> {
    let entry: GatewayModelPricing = decode(row.entry)?;
    Ok(GatewayModelPricing {
        active: row.active,
        ..entry
    })
}

/// Decodes a stored capture policy, or the disabled version-1 default.
///
/// # Errors
/// Returns `Internal` when the document does not decode.
fn capture_from(document: Option<Value>) -> Result<GatewayCapturePolicy, WyrdError> {
    document.map_or_else(
        || {
            Ok(GatewayCapturePolicy {
                mode: GatewayCaptureMode::Disabled,
                payload_fields: BTreeSet::new(),
                version: NonZeroU64::MIN,
            })
        },
        decode,
    )
}

/// Projects a stored credential row to its redacted view.
///
/// # Errors
/// Returns `Internal` when the row does not decode.
fn credential_view(row: GatewayCredentialRow) -> Result<ProviderCredentialView, WyrdError> {
    decode(json!({
        "name": row.name,
        "provider": row.provider,
        "source": row.source,
        "state": row.state,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
        "rotated_at": row.rotated_at,
        "revoked_at": row.revoked_at,
    }))
}

/// Rejects a body name that differs from the path name.
///
/// # Errors
/// Returns `GatewayInvalidConfiguration` for field `name`.
fn require_path_name(path: &str, body: &str) -> Result<(), WyrdError> {
    if path == body {
        Ok(())
    } else {
        Err(invalid(GatewayContractError::new(
            "name",
            "must match the path name",
        )))
    }
}

/// Encodes a contract value as JSON.
///
/// # Errors
/// Returns `Internal` when serialization fails.
pub(super) fn encode<T: Serialize>(value: &T) -> Result<Value, WyrdError> {
    serde_json::to_value(value).map_err(internal)
}

/// Decodes a stored JSON document.
///
/// # Errors
/// Returns `Internal` when the document does not match the contract.
pub(super) fn decode<T: DeserializeOwned>(value: Value) -> Result<T, WyrdError> {
    serde_json::from_value(value).map_err(internal)
}

/// Decodes an optional stored document, defaulting when absent.
///
/// # Errors
/// Returns `Internal` when a present document does not match the contract.
fn decode_or_default<T: DeserializeOwned + Default>(value: Option<Value>) -> Result<T, WyrdError> {
    value.map_or_else(|| Ok(T::default()), decode)
}

/// Stable rejection for an invalid gateway document.
pub(crate) fn invalid(error: GatewayContractError) -> WyrdError {
    WyrdError::GatewayInvalidConfiguration {
        message: error.to_string(),
        details: json!({ "field": error.field, "reason": error.reason }),
    }
}

/// Stable absence of a named tenant resource.
fn not_found(resource: &str, name: &str) -> WyrdError {
    WyrdError::GatewayResourceNotFound {
        message: format!("provider {resource} {name} not found in tenant"),
        details: json!({ "resource": resource, "name": name }),
    }
}

/// Stable lifecycle conflict for a named credential.
fn conflict(message: &str, name: &str) -> WyrdError {
    WyrdError::GatewayResourceConflict {
        message: message.to_owned(),
        details: json!({ "name": name }),
    }
}

/// Maps a foreign-key violation to a reference conflict, else unavailable.
fn referenced_conflict(error: SqlxError, resource: &str, name: &str) -> WyrdError {
    match SqlError::from(error) {
        SqlError::FkViolation { .. } => WyrdError::GatewayResourceConflict {
            message: format!("provider {resource} {name} is referenced by a deployment"),
            details: json!({ "resource": resource, "name": name }),
        },
        other => unavailable(other),
    }
}

/// Maps a storage failure to a retryable 503 without leaking SQL detail.
pub(super) fn unavailable(error: impl fmt::Display) -> WyrdError {
    tracing::warn!(error = %error, "gateway administration storage unavailable");
    WyrdError::ServiceUnavailable {
        message: "gateway administration storage is unavailable".to_owned(),
        details: json!({}),
    }
}

/// Maps a stored-document or serialization invariant failure to a 500.
pub(super) fn internal(error: impl fmt::Display) -> WyrdError {
    tracing::error!(error = %error, "gateway administration state is inconsistent");
    WyrdError::Internal {
        message: "gateway administration state is inconsistent".to_owned(),
        details: Value::Null,
    }
}
