//! The process gateway owner.
//!
//! [`Gateway`] retains everything one server's governed gateway needs across
//! calls: the operator configuration tenants' credentials may name, the
//! execution engine every invocation entry shares, the per-tenant admission
//! snapshot cache and its invalidation listener, the API-key exchange cache,
//! and the tracker of in-flight call work that shutdown drains.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use wyrd_crypt::SealingKeyring;
use wyrd_gateway::GatewayEngine;
use wyrd_sql::WyrdPostgres;

use super::{GatewayApiKeys, GatewaySnapshots};
use crate::boot::ServerBootError;
use crate::config::{DeploymentProfile, GatewayConfig};
use crate::state::EngineWorker;

/// One server's governed gateway; see the module docs.
pub struct Gateway {
    /// Operator-declared credential bindings and secret backends.
    config: GatewayConfig,
    /// Shared execution engine behind every governed invocation entry.
    engine: GatewayEngine,
    /// Per-tenant admission snapshots, served only while
    /// [`Self::refresh_worker`] hears every gateway configuration change.
    snapshots: Arc<GatewaySnapshots>,
    /// Tokens exchanged for Wyrd API keys presented to public gateway
    /// ingress, reused per key for at most 60 seconds.
    api_keys: GatewayApiKeys,
    /// In-flight call execution and accounting, including accounting that
    /// settles after a stream reached its caller.
    tasks: TaskTracker,
}

impl Gateway {
    /// A gateway over `config` executing through `engine`.
    #[must_use]
    pub fn new(config: GatewayConfig, engine: GatewayEngine) -> Self {
        Self {
            config,
            engine,
            snapshots: Arc::default(),
            api_keys: GatewayApiKeys::default(),
            tasks: TaskTracker::new(),
        }
    }

    /// The gateway `config` describes: its credential resolver opens managed
    /// secrets with `sealing_key`, and its HTTP provider dispatch screens the
    /// configured endpoints under `profile`.
    ///
    /// Synchronous, but not IO-free: building the credential resolver reads
    /// each configured secret backend's CA bundle once.
    ///
    /// # Errors
    /// Returns [`ServerBootError::GatewayTransport`] when a secret backend's
    /// CA bundle is unreadable or does not parse, a backend client cannot be
    /// built, or the HTTP provider dispatch transport cannot be built.
    pub fn from_config(
        config: GatewayConfig,
        profile: DeploymentProfile,
        sealing_key: Option<Arc<SealingKeyring>>,
    ) -> Result<Self, ServerBootError> {
        let transport =
            |error: &dyn std::fmt::Display| ServerBootError::GatewayTransport(error.to_string());
        let engine = GatewayEngine::new(
            config
                .credential_resolver(sealing_key)
                .map_err(|error| transport(&error))?,
            wyrd_gateway::DeploymentHealth::default(),
            Arc::new(
                wyrd_gateway::HttpProviderDispatch::new(
                    skald_providers::EndpointPolicy::new(profile.is_production()),
                    config.endpoints.clone(),
                )
                .map_err(|error| transport(&error))?,
            ),
        );
        Ok(Self::new(config, engine))
    }

    /// The operator configuration tenants' credentials may name.
    #[must_use]
    pub const fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// The execution engine every invocation entry shares.
    #[must_use]
    pub const fn engine(&self) -> &GatewayEngine {
        &self.engine
    }

    /// The per-tenant admission snapshot cache.
    pub(crate) fn snapshots(&self) -> &GatewaySnapshots {
        &self.snapshots
    }

    /// The API-key exchange cache of public gateway ingress.
    pub(crate) const fn api_keys(&self) -> &GatewayApiKeys {
        &self.api_keys
    }

    /// The tracker every call's execution and accounting runs on.
    #[must_use]
    pub const fn tasks(&self) -> &TaskTracker {
        &self.tasks
    }

    /// The snapshot invalidation listener over `postgres`, which runs
    /// until `shutdown`; snapshots serve only while it listens.
    pub(crate) fn refresh_worker(
        &self,
        postgres: &WyrdPostgres,
        shutdown: CancellationToken,
    ) -> EngineWorker {
        let postgres = postgres.clone();
        let snapshots = Arc::clone(&self.snapshots);
        (
            "gateway_snapshot_cache",
            Box::pin(async move { snapshots.watch(&postgres, shutdown).await }),
        )
    }

    /// Wait until every accepted call's work, including accounting that
    /// settles after its response, has finished or `deadline` passes.
    ///
    /// Admission already refuses new calls once the shutdown token is
    /// cancelled; closing the tracker here makes its wait complete once
    /// empty. Returns whether the work drained. Cancelling the wait leaves
    /// the tracker closed and its tasks running.
    pub async fn drain(&self, deadline: tokio::time::Instant) -> bool {
        self.tasks.close();
        tokio::time::timeout_at(deadline, self.tasks.wait())
            .await
            .is_ok()
    }
}
