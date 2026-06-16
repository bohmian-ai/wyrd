//! Shared axum application state.

use sqlx::PgPool;
use std::sync::Arc;
use wyrd_auth_issue::IssuingKey;
use wyrd_auth_verify::TokenVerifier;
use wyrd_runtime::{PermissionCheck, RbacCheck};
use wyrd_storage::StorageHandle;

use crate::auth::permission_resolver::SqlPermissionResolver;

/// Process-wide handle registry. One instance is shared by all HTTP handlers.
///
/// Foundation commits append their owned handles here, for example storage,
/// auth verification, and policy evaluation. This skeleton ships only the
/// runtime database pools that already survive boot.
#[derive(Clone)]
pub struct AppState {
    /// Runtime `wyrd_app` pool. Tenant-scoped traffic uses this pool and RLS
    /// applies on tenant tables.
    pub pool: PgPool,
    /// Optional audited `wyrd_platform_admin` pool for cross-tenant platform
    /// operations. Dedicated deployments may leave this unset.
    pub platform_admin_pool: Option<PgPool>,
    /// Process-wide artifact storage handle.
    pub storage: Arc<StorageHandle>,
    /// Runtime preview gate for auth routes that depend on card-registry principal projection.
    pub allow_preview_auth: bool,
    /// RBAC checker for auth route permission gates.
    pub permission_check: Arc<dyn PermissionCheck>,
    /// JWT issuer for auth token routes.
    pub issuing_key: Option<Arc<IssuingKey>>,
    /// JWT verifier for token-exchange routes.
    pub token_verifier: Option<Arc<TokenVerifier<SqlPermissionResolver>>>,
}

impl AppState {
    /// Build runtime state from the pools that survive boot.
    #[must_use]
    pub fn new(
        pool: PgPool,
        platform_admin_pool: Option<PgPool>,
        storage: Arc<StorageHandle>,
    ) -> Self {
        Self {
            pool,
            platform_admin_pool,
            storage,
            allow_preview_auth: cfg!(debug_assertions),
            permission_check: Arc::new(RbacCheck),
            issuing_key: None,
            token_verifier: None,
        }
    }

    /// Override the auth preview gate, primarily for tests and local config wiring.
    #[must_use]
    pub fn with_preview_auth(mut self, allow_preview_auth: bool) -> Self {
        self.allow_preview_auth = allow_preview_auth;
        self
    }

    /// Attach auth signing and verification handles.
    #[must_use]
    pub fn with_auth_handles(
        mut self,
        issuing_key: Arc<IssuingKey>,
        token_verifier: Arc<TokenVerifier<SqlPermissionResolver>>,
    ) -> Self {
        self.issuing_key = Some(issuing_key);
        self.token_verifier = Some(token_verifier);
        self
    }
}
