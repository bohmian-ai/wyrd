//! The session template every Oracle query session starts from.
//!
//! `DataFusion`'s default features, including its Variant functions and the
//! `->`/`->>` planner, are built once per process; each query clones the
//! result instead of rebuilding every registry.

use std::sync::{Arc, LazyLock};

use datafusion::catalog::{CatalogProviderList, MemoryCatalogProviderList};
use datafusion::execution::context::SessionConfig;
use datafusion::execution::{SessionState, SessionStateBuilder, SessionStateDefaults};

/// The process-wide template; it holds no tables and never changes.
static SHARED: LazyLock<OracleSessionTemplate> = LazyLock::new(|| OracleSessionTemplate {
    template: SessionStateBuilder::new().with_default_features().build(),
});

/// A default-featured session state every Oracle session is cloned from.
#[derive(Debug)]
pub(crate) struct OracleSessionTemplate {
    /// `DataFusion`'s default features, built once.
    ///
    /// It is never wrapped in a context and never holds a table: its catalog
    /// list is replaced in every session built from it.
    template: SessionState,
}

impl OracleSessionTemplate {
    /// Returns the process-wide template.
    #[must_use]
    pub(crate) fn shared() -> &'static Self {
        &SHARED
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
