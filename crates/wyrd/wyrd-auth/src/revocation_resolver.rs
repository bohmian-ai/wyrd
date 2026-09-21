//! SQL-backed principal-epoch revocation check.

use std::fmt::{Debug, Formatter, Result as FmtResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use wyrd_auth_verify::{ResolveError, RevocationCheck};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{service_account_admission, user_admission};

/// SQL-backed revocation check that reads tenant admission and the
/// principal's `tokens_not_before` on every verification.
///
/// Nothing is memoized here. The approved boundary is the next request: a
/// role withdrawal, credential or principal revocation, or tenant suspension
/// committed before a verification must be observed by it on every replica, so
/// a per-process epoch cache cannot be correct without a synchronous
/// invalidation protocol. Both answers come from one statement on one tenant
/// connection, and the verifier's own token cache still saves the signature and
/// permission work.
#[derive(Clone)]
pub struct SqlRevocationCheck {
    /// Application pool the per-request tenant connection is acquired from.
    pool: Arc<PgPool>,
}

impl Debug for SqlRevocationCheck {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("SqlRevocationCheck").finish_non_exhaustive()
    }
}

impl SqlRevocationCheck {
    /// Construct a check backed by the Wyrd app pool.
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

impl RevocationCheck for SqlRevocationCheck {
    /// Resolve the instant before which `principal`'s tokens are invalid.
    ///
    /// Acquires a tenant connection and reads tenant admission together with
    /// the principal's epoch from the table its kind lives in. A tenant that
    /// does not admit credentials yields `now`, retiring everything already
    /// issued exactly as the principal's own epoch would.
    ///
    /// # Errors
    /// Returns [`ResolveError::Unavailable`] when the connection or read fails,
    /// or when a platform-scope principal reaches a tenant lookup; the verifier
    /// fails closed on either.
    fn epoch<'a>(
        &'a self,
        tenant: &'a DataTenantId,
        principal: PrincipalId,
        kind: PrincipalKindTag,
    ) -> Pin<Box<dyn Future<Output = Result<Option<DateTime<Utc>>, ResolveError>> + Send + 'a>>
    {
        Box::pin(async move {
            let mut conn = TenantConn::acquire(&self.pool, *tenant)
                .await
                .map_err(|e| ResolveError::Unavailable(e.to_string()))?;
            let id = principal.as_uuid();
            let admission = match kind {
                PrincipalKindTag::User => user_admission(&mut conn, *tenant, id).await,
                PrincipalKindTag::TenantAdmin
                | PrincipalKindTag::Service
                | PrincipalKindTag::Agent => {
                    service_account_admission(&mut conn, *tenant, id).await
                }
                // A platform-scope principal has no tenant-scoped revocation
                // epoch. Reaching here means a token claimed platform scope
                // inside a tenant lookup, so the resolution fails closed rather
                // than returning an epoch that would admit it.
                PrincipalKindTag::GlobalAdmin => {
                    return Err(ResolveError::Unavailable(
                        "platform-scope principal has no tenant revocation epoch".to_owned(),
                    ));
                }
            }
            .map_err(|e| ResolveError::Unavailable(e.to_string()))?;
            if !admission.tenant_admits {
                return Ok(Some(Utc::now()));
            }
            Ok(admission.tokens_not_before)
        })
    }
}
