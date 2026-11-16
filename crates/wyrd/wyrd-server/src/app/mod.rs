//! Application composition and process lifecycle.

use std::sync::Arc;

use tracing::info;
use wyrd_telemetry::TelemetryGuard;

use crate::app::metrics::WyrdTelemetryRuntime;
use crate::boot::{BootedServer, StartInputs, production_guards};
use crate::config::{ServeMode, WyrdServerConfig};
use crate::state::AppState;

pub mod metrics;
pub mod peer_plane;
/// Plain HTTP serving of an assembled router with graceful shutdown.
pub mod serve;
pub mod server;
pub mod shutdown;
pub mod supervise;

pub use serve::serve;
pub use server::{BoundServer, WyrdServer};

/// Error wrapper used by the server binary's top-level CLI dispatcher.
#[derive(Debug)]
pub enum BootExit {
    /// Configuration loading or production validation failed.
    Config(Box<dyn std::error::Error + Send + Sync>),
    /// Runtime boot or serving failed.
    Other(Box<dyn std::error::Error + Send + Sync>),
}

/// Build and run the Wyrd server process for the given transport `mode`.
///
/// `mode` is `None` to use the configured default (`config.serve.mode`), or
/// `Some(_)` to override it (the binary passes the `--mode` flag through here).
/// `parent_committed` reads this process's allocator commitment for the
/// pressure sampler.
pub async fn run(mode: Option<ServeMode>, parent_committed: fn() -> u64) -> Result<(), BootExit> {
    let config = WyrdServerConfig::load().map_err(|e| BootExit::Config(Box::new(e)))?;
    production_guards(&config);

    let telemetry_runtime =
        WyrdTelemetryRuntime::install(config.telemetry.clone(), config.metrics.benchmark_snapshot)
            .map_err(|error| BootExit::Other(Box::new(error)))?;
    let telemetry: Arc<TelemetryGuard> = telemetry_runtime.guard();
    let metrics = config.metrics.enabled.then(|| telemetry_runtime.metrics());
    info!(
        service.name = config
            .telemetry
            .service_name
            .as_deref()
            .unwrap_or("wyrd-server"),
        "wyrd-server starting"
    );

    let mode = mode.unwrap_or(config.serve.mode);

    // `coordination_runtime` and `compaction_runtime` are the sole owners of
    // the dedicated Scribe and Forge executors. They are bound here, outside
    // the serving future, so they outlive both the Bifrost drain that
    // `BoundServer::run` performs and the supervised Forge worker, and are
    // released only once serving has returned. Their drops are non-blocking,
    // which is required because this frame is inside `#[tokio::main]`.
    let inputs = StartInputs::production(&config, parent_committed)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;
    let BootedServer {
        state,
        coordination_runtime,
        data_root: _data_root,
        workers,
        compaction_runtime,
    } = AppState::start(&config, inputs)
        .await
        .map_err(|e| BootExit::Other(Box::new(e)))?;

    // Every target, including the public-listener-free Forge maintenance
    // target, is served by the one supervised owner.
    state
        .bifrost
        .node_id()
        .ok_or_else(|| BootExit::Other("configured Bifrost node identity is unavailable".into()))?;
    let result = workers
        .into_iter()
        .fold(
            WyrdServer::new_with_metrics(config, state, metrics)
                .map_err(|e| BootExit::Other(Box::new(e)))?,
            |server, (name, worker)| server.spawn_worker(name, worker),
        )
        .serve(mode)
        .await;

    drop(compaction_runtime); // release Forge compaction threads after supervision drains
    drop(coordination_runtime); // release Scribe coordination threads after drain
    drop(telemetry); // flush OTLP exporters after serving stops
    drop(telemetry_runtime);
    result
}

#[cfg(test)]
/// Static composition assertions that keep Forge ownership in the one serving owner.
mod tests {
    /// The compaction runtime outlives every path that supervises Forge work.
    ///
    /// The owner's drop detaches its threads instead of joining them, so a
    /// runner that has written output objects and is about to Prepare its
    /// operation would be abandoned mid-flight if the executor were released
    /// first. Ownership therefore sits outside the serving future in `run`, and
    /// the release must be sequenced strictly after serving — which is the
    /// frame every target drains its supervision inside.
    ///
    /// # Panics
    ///
    /// Panics when the owner is bound inside the serving future, released
    /// before serving returns, or released after the Scribe coordination
    /// runtime it must precede.
    #[test]
    fn forge_runtime_lives_until_worker_supervision_drains() {
        let production = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("application module has production source before tests");

        let bind = production
            .find("compaction_runtime,\n    } = AppState::start(")
            .expect("run binds the compaction runtime out of BootedServer");
        let serve = production
            .find("let result = workers")
            .expect("run serves after binding its runtimes");
        let release = production
            .find("drop(compaction_runtime);")
            .expect("run releases the compaction runtime");
        let coordination = production
            .find("drop(coordination_runtime);")
            .expect("run releases the coordination runtime");

        assert!(
            bind < serve,
            "the executor must be owned outside the serving future, not inside it"
        );
        assert!(
            serve < release,
            "serving — which drains Forge supervision — must return before release"
        );
        assert!(
            release < coordination,
            "Forge threads release before the Scribe coordination threads they may still call"
        );
        assert!(
            production[serve..release].contains(".await"),
            "the release must follow an awaited serving frame, not a detached spawn"
        );
        assert_eq!(
            production.matches("drop(compaction_runtime);").count(),
            1,
            "exactly one release site keeps the ordering above auditable"
        );

        // The harness that composes the same graph honours the same ordering by
        // owning the runtime for the whole server lifetime.
        let harness = include_str!("../../../wyrd-testing/src/server.rs");
        assert!(
            harness.contains("_compaction_runtime: wyrd_server::state::ForgeCompactionRuntime"),
            "the test harness must retain the executor for the server's lifetime"
        );
    }

    /// Every target is served by one owner with one Forge worker and one
    /// scheduler construction path.
    ///
    /// Every Forge loop is owned by exactly one restarting supervisor whose
    /// constructor is that single path, so a restarted instance is the same
    /// boot-composed worker rather than a second owner.
    ///
    /// # Panics
    ///
    /// Panics when a composition site gains a second construction path, loses
    /// its restarting owner, or supervises a Forge loop as process-terminal.
    #[test]
    fn process_role_composition_has_one_forge_owner() {
        let production_runner = include_str!("mod.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("application module has production source before tests");
        assert!(
            !production_runner.contains("spawn_forge_worker(")
                && !production_runner.contains("restarting_worker("),
            "the process entry must not own a second Forge composition"
        );
        let boot = include_str!("../boot/mod.rs");
        assert_eq!(
            boot.matches("ForgeWorker::new(").count(),
            1,
            "boot must construct the Forge worker exactly once"
        );
        assert!(
            boot.contains("node_id.as_uuid()"),
            "normal ForgeWorker construction must use the boot-resolved physical identity"
        );

        let server = include_str!("server.rs");
        assert!(
            server.contains("contains(&BifrostRuntimeRole::ForgeWorker)"),
            "the Forge worker must be composed only where the target selects it"
        );
        assert_eq!(
            server.matches("spawn_maintenance_scheduler(").count(),
            1,
            "API roles must retain one supervised Forge scheduler"
        );
        assert_eq!(
            server.matches("spawn_forge_worker(").count(),
            1,
            "the serving owner composes exactly one Forge worker"
        );

        // Each construction above is the one constructor its restarting owner
        // calls again after a failure, so a restart rebuilds from the same
        // boot-composed worker (and its boot-resolved identity) rather than
        // from a second construction path.
        let owners = ["\"forge_worker\",", "\"maintenance_scheduler\","];
        assert_eq!(
            server.matches("restarting_worker(").count(),
            owners.len(),
            "each Forge loop has exactly one restarting owner"
        );
        for owner in owners {
            assert_eq!(
                server.matches(owner).count(),
                1,
                "{owner} must own its Forge loop exactly once"
            );
        }
        assert!(
            !server.contains("TaskId::Worker(\"forge_worker\")")
                && !server.contains("TaskId::Worker(\"maintenance_scheduler\")"),
            "a Forge loop must not be supervised as a process-terminal task"
        );
    }
}
