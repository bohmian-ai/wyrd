//! The session template Oracle's planning and execution sessions start from.
//!
//! `DataFusion`'s default features, including its Variant functions and the
//! `->`/`->>` planner, are built once per process; each query clones the
//! result instead of rebuilding every registry.
//!
//! The template also carries [`super::live::LiveUnionProjection`], so every
//! plan with a live leaf pushes its projection to the Scribe scan.
//!
//! The analytical leader's distributed session is the one exception: it is
//! built from `DataFusion`'s default features directly, with the attempt's own
//! runtime and the distributed planner.

use std::sync::{Arc, LazyLock};

use datafusion::catalog::{CatalogProviderList, MemoryCatalogProviderList};
use datafusion::execution::context::SessionConfig;
use datafusion::execution::{SessionState, SessionStateBuilder, SessionStateDefaults, TaskContext};
use datafusion::logical_expr::Expr;
use datafusion_proto::bytes::Serializeable as _;

/// The process-wide template; it holds no tables and never changes.
static SHARED: LazyLock<OracleSessionTemplate> = LazyLock::new(|| {
    let template = SessionStateBuilder::new()
        .with_default_features()
        .with_physical_optimizer_rule(Arc::new(super::live::LiveUnionProjection))
        .build();
    OracleSessionTemplate {
        functions: TaskContext::from(&template),
        template,
    }
});

/// A default-featured session state every Oracle session is cloned from.
#[derive(Debug)]
pub(crate) struct OracleSessionTemplate {
    /// `DataFusion`'s default features, built once.
    ///
    /// It is never wrapped in a context and never holds a table: its catalog
    /// list is replaced in every session built from it.
    template: SessionState,
    /// The template's function registry as a task context, the shape
    /// `datafusion-proto` resolves a decoded expression's functions against.
    functions: TaskContext,
}

impl OracleSessionTemplate {
    /// Returns the process-wide template.
    #[must_use]
    pub(crate) fn shared() -> &'static Self {
        &SHARED
    }

    /// Decodes one `datafusion-proto` logical expression, resolving its
    /// functions against Oracle's default registry.
    ///
    /// # Errors
    ///
    /// Returns the `DataFusion` error of malformed bytes or a function the
    /// registry lacks.
    pub(crate) fn decode_expr(&self, bytes: &[u8]) -> datafusion::error::Result<Expr> {
        Expr::from_bytes_with_ctx(bytes, &self.functions)
    }

    /// Returns one Oracle session state under `config`: a clone of the
    /// template, as `SessionContext::state` hands out per query, with its own
    /// catalog list so one query's tables never reach another.
    #[must_use]
    pub(crate) fn session(&self, config: SessionConfig) -> SessionState {
        let mut state = self.template.clone();
        *state.config_mut() = config;
        let catalogs = MemoryCatalogProviderList::new();
        catalogs.register_catalog(
            state.config().options().catalog.default_catalog.clone(),
            Arc::new(SessionStateDefaults::default_catalog(
                state.config(),
                state.table_factories(),
                state.runtime_env(),
            )),
        );
        state.register_catalog_list(Arc::new(catalogs));
        state
    }
}
