//! SQL-backed principal-epoch revocation check with an in-process moka cache.

use std::fmt;
use std::hash::Hash;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use moka::future::Cache;
use sqlx::PgPool;
use wyrd_auth_verify::{ResolveError, RevocationCheck};
use wyrd_runtime::PrincipalId;
use wyrd_spec::DataTenantId;
use wyrd_spec::auth::PrincipalKindTag;
use wyrd_sql::TenantConn;
use wyrd_sql::queries::auth::{service_account_revocation_epoch, user_revocation_epoch};

const EPOCH_CACHE_TTL: Duration = Duration::from_secs(5);
const EPOCH_CACHE_MAX: u64 = 50_000;

/// Cache key: (`tenant_id`, `principal_kind`, `principal_id`).
///
/// Service and Agent share the same DB table; they key separately here but a
/// given principal id resolves to exactly one kind, so entries never collide.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct EpochKey {
    tenant: DataTenantId,
    kind: PrincipalKindTag,
    id: PrincipalId,
}

/// SQL-backed revocation check. Reads `tokens_not_before` from the database
/// and caches the result for `EPOCH_CACHE_TTL` seconds.
///
/// A `PgListener`-based invalidator (see `revocation_listener.rs`) clears
/// specific cache entries on cross-pod `wyrd_principal_revoked` NOTIFY so the
/// effective revocation lag is bounded by the NOTIFY delivery latency rather
/// than the full TTL.
#[derive(Clone)]
pub struct SqlRevocationCheck {
    pool: Arc<PgPool>,
    cache: Cache<EpochKey, Option<DateTime<Utc>>>,
}

impl fmt::Debug for SqlRevocationCheck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SqlRevocationCheck").finish_non_exhaustive()
    }
}

impl SqlRevocationCheck {
    /// Construct a new check backed by the Wyrd app pool.
    #[must_use]
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self::new_with_ttl(pool, EPOCH_CACHE_TTL)
    }

    /// Construct a check with an explicit epoch-cache TTL.
    ///
    /// Production uses [`SqlRevocationCheck::new`] (a 5-second TTL bounded by the
    /// `PgListener` invalidator). In-process tests have no NOTIFY listener, so
    /// they pass `Duration::ZERO` to read the revocation epoch fresh on every
    /// verify and observe a revocation deterministically within the test window.
    #[must_use]
    pub fn new_with_ttl(pool: Arc<PgPool>, ttl: Duration) -> Self {
        let cache = Cache::builder()
            .max_capacity(EPOCH_CACHE_MAX)
            .time_to_live(ttl)
            .build();
        Self { pool, cache }
    }

    /// Invalidate the cache for a specific principal.
    ///
    /// Called by `RevocationListener` when a `wyrd_principal_revoked` NOTIFY
    /// arrives so this replica reflects the revocation within the NOTIFY
    /// delivery window rather than waiting for TTL expiry.
    pub async fn invalidate(&self, tenant: DataTenantId, id: PrincipalId, kind: PrincipalKindTag) {
        let key = EpochKey { tenant, kind, id };
        self.cache.invalidate(&key).await;
    }
}

impl RevocationCheck for SqlRevocationCheck {
    fn epoch<'a>(
        &'a self,
        tenant: &'a DataTenantId,
        principal: PrincipalId,
        kind: PrincipalKindTag,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<Option<DateTime<Utc>>, ResolveError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            let key = EpochKey {
                tenant: *tenant,
                kind,
                id: principal,
            };

            self.cache
                .try_get_with(key, async {
                    let mut conn = TenantConn::acquire(&self.pool, *tenant)
                        .await
                        .map_err(|e| ResolveError::Unavailable(e.to_string()))?;
                    let id_uuid = principal.as_uuid();
                    let epoch =
                        match kind {
                            PrincipalKindTag::User => user_revocation_epoch(&mut conn, id_uuid)
                                .await
                                .map_err(|e| ResolveError::Unavailable(e.to_string()))?,
                            PrincipalKindTag::TenantAdmin
                            | PrincipalKindTag::Service
                            | PrincipalKindTag::Agent => {
                                service_account_revocation_epoch(&mut conn, id_uuid)
                                    .await
                                    .map_err(|e| ResolveError::Unavailable(e.to_string()))?
                            }
                            // A platform-scope principal has no tenant-scoped
                            // revocation epoch. Reaching here means a token
                            // claimed platform scope inside a tenant lookup, so
                            // the resolution fails closed rather than returning
                            // an epoch that would admit it.
                            PrincipalKindTag::GlobalAdmin => {
                                return Err(ResolveError::Unavailable(
                                    "platform-scope principal has no tenant revocation epoch"
                                        .to_owned(),
                                ));
                            }
                        };
                    Ok(epoch)
                })
                .await
                .map_err(|e: Arc<ResolveError>| ResolveError::Unavailable(e.to_string()))
        })
    }
}
