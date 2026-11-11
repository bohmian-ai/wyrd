//! The process gateway owner.
//!
//! [`Gateway`] retains everything one server's governed gateway needs across
//! calls: the operator configuration tenants' credentials may name, the
//! execution engine every invocation entry shares, the per-tenant admission
//! snapshot cache and its invalidation listener, the in-memory quota usage
//! and its background persistence, the API-key exchange cache, and the
//! tracker of in-flight call work that shutdown drains.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use wyrd_crypt::SealingKeyring;
use wyrd_gateway::GatewayEngine;
use wyrd_sql::WyrdPostgres;

use super::{GatewayApiKeys, GatewaySnapshots, GatewayUsage};
use crate::boot::ServerBootError;
use crate::config::{DeploymentProfile, GatewayConfig};
use crate::state::EngineWorker;

/// Wait before retrying reloads after a pass in which a snapshot load failed,
/// so failing storage is retried steadily rather than in a tight loop.
const RELOAD_RETRY: std::time::Duration = std::time::Duration::from_secs(1);

/// One server's governed gateway; see the module docs.
pub struct Gateway {
    /// Operator-declared credential bindings and secret backends, shared with
    /// background snapshot reloads.
    config: Arc<GatewayConfig>,
    /// Shared execution engine behind every governed invocation entry.
    engine: GatewayEngine,
    /// Per-tenant admission snapshots, served only while
    /// [`Self::refresh_worker`] hears every gateway configuration change.
    snapshots: Arc<GatewaySnapshots>,
    /// Quota usage admissions charge in memory, persisted and refreshed by
    /// [`Self::refresh_worker`].
    usage: Arc<GatewayUsage>,
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
            config: Arc::new(config),
            engine,
            snapshots: Arc::default(),
            usage: Arc::default(),
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
    pub fn config(&self) -> &GatewayConfig {
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

    /// The process quota usage.
    pub(crate) const fn usage(&self) -> &Arc<GatewayUsage> {
        &self.usage
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

    /// The gateway's background work over `postgres` until `shutdown`: the
    /// snapshot invalidation listener, snapshot reloads, and quota usage
    /// persistence and refresh. Snapshots serve only while the listener
    /// listens.
    pub(crate) fn refresh_worker(
        &self,
        postgres: &WyrdPostgres,
        shutdown: CancellationToken,
    ) -> EngineWorker {
        let postgres = postgres.clone();
        let config = Arc::clone(&self.config);
        let snapshots = Arc::clone(&self.snapshots);
        let usage = Arc::clone(&self.usage);
        (
            "gateway_snapshot_cache",
            Box::pin(async move {
                let reloads = async {
                    loop {
                        let mut failed = false;
                        for tenant in snapshots.reloads().await {
                            failed |= snapshots.reload(&postgres, &config, tenant).await.is_err();
                        }
                        if failed {
                            tokio::time::sleep(RELOAD_RETRY).await;
                        }
                    }
                };
                tokio::select! {
                    () = shutdown.cancelled() => {}
                    () = snapshots.watch(&postgres, shutdown.clone()) => {}
                    () = usage.run(&postgres, shutdown.clone()) => {}
                    () = reloads => {}
                }
            }),
        )
    }

    /// Wait until every accepted call's work, including accounting that
    /// settles after its response, has finished or `deadline` passes, then
    /// persist the quota usage it charged to `postgres` within the same
    /// deadline.
    ///
    /// Admission already refuses new calls once the shutdown token is
    /// cancelled; closing the tracker here makes its wait complete once
    /// empty. Returns whether the work drained. Usage that cannot be
    /// persisted before the deadline is lost with the process. Cancelling
    /// the wait leaves the tracker closed and its tasks running.
    pub async fn drain(&self, postgres: &WyrdPostgres, deadline: tokio::time::Instant) -> bool {
        self.tasks.close();
        let drained = tokio::time::timeout_at(deadline, self.tasks.wait())
            .await
            .is_ok();
        if tokio::time::timeout_at(deadline, self.usage.flush(postgres))
            .await
            .is_err()
        {
            tracing::warn!("gateway usage was not persisted before the shutdown deadline");
        }
        drained
    }
}
