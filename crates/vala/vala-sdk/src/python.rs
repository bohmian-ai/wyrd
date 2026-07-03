//! PyO3 boundary for the Vala client SDK: the `bifrost` write handle and the
//! `observe` fire-and-forget telemetry function.
//!
//! Every method converts its Python inputs to the native C4b types at the edge
//! (JSON-Schema text to an Arrow schema, a card-ref string to [`CardRef`]) and
//! then calls the Rust-native handle — no queue or engine logic is
//! re-implemented here. [`crate::scope::ProducerKey`]-equivalent pool keys and
//! [`ClientScope`] stay opaque: Python never names them.

use std::sync::Arc;

use arrow_schema::SchemaRef;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use wyrd_client::config::ClientConfig;
use wyrd_client::transport::HttpConfig;
use wyrd_queue::{MockSink, QueueConfig};
use wyrd_spec::reference::CardRef;
use wyrd_spec::vala::ids::RunId;

use crate::handle::{Bifrost as BifrostHandle, schema_from_json_schema};
use crate::scope::{ClientScope, SinkKind};

/// Python-facing Bifrost write handle over the pooled C4b producers.
///
/// The handle is keyed under a [`ClientScope`] derived from `server_url` plus a
/// SHA-256 fingerprint of the credential; neither the scope nor the pool keys
/// are reachable from Python. The networked ingest transport is wired in a later
/// commit — until then the handle drains into an in-process loopback sink, so the
/// enqueue/observe/drop-count boundary is exercised without a server.
#[pyclass(module = "wyrd._wyrd.bifrost", name = "Bifrost")]
pub struct Bifrost {
    inner: Arc<BifrostHandle>,
}

#[pymethods]
impl Bifrost {
    /// Build a handle for `(server_url, api_key)`.
    #[new]
    #[pyo3(signature = (server_url, api_key))]
    fn new(server_url: String, api_key: String) -> PyResult<Self> {
        let config = ClientConfig {
            http: HttpConfig {
                base_url: server_url,
                ..HttpConfig::default()
            },
            api_key: Some(api_key.into()),
            ..ClientConfig::default()
        };
        let scope =
            ClientScope::from_config(&config).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let handle = BifrostHandle::new(scope, Arc::new(MockSink::new()), QueueConfig::default());
        Ok(Self {
            inner: Arc::new(handle),
        })
    }

    /// Explicit write path: enqueue one JSON `row`, propagating queue-full.
    #[pyo3(signature = (table, schema, row, card_ref, run_id=None))]
    fn insert(
        &self,
        table: &str,
        schema: &str,
        row: &str,
        card_ref: &str,
        run_id: Option<String>,
    ) -> PyResult<()> {
        let arrow_schema = arrow_schema_from_json(schema)?;
        let card = parse_card_ref(card_ref)?;
        let run = run_id.map(RunId::from_string);
        self.inner
            .insert(SinkKind::Record, table, &arrow_schema, row.as_bytes().to_vec(), card, run)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    }

    /// Rows dropped by the fire-and-forget observe path under backpressure.
    #[getter]
    fn dropped(&self) -> u64 {
        self.inner.dropped()
    }

    /// Number of distinct producers currently pooled.
    #[getter]
    fn producer_count(&self) -> usize {
        self.inner.producer_count()
    }
}

/// Record one telemetry observation, fire-and-forget.
///
/// Converts the JSON-Schema `schema` and `card_ref` string at the boundary, then
/// hands the row to [`crate::observe::record`], which swallows queue-full and
/// bumps the handle's drop counter — the caller is never broken.
#[pyfunction]
#[pyo3(signature = (bifrost, table, schema, row, card_ref, run_id=None))]
fn record(
    bifrost: PyRef<'_, Bifrost>,
    table: &str,
    schema: &str,
    row: &str,
    card_ref: &str,
    run_id: Option<String>,
) -> PyResult<()> {
    let arrow_schema = arrow_schema_from_json(schema)?;
    let card = parse_card_ref(card_ref)?;
    let run = run_id.map(RunId::from_string);
    crate::observe::record(
        &bifrost.inner,
        SinkKind::Record,
        table,
        &arrow_schema,
        row.as_bytes().to_vec(),
        card,
        run,
    );
    Ok(())
}

/// Parse JSON-Schema text into an Arrow schema at the Python boundary.
fn arrow_schema_from_json(schema: &str) -> PyResult<SchemaRef> {
    let value: serde_json::Value =
        serde_json::from_str(schema).map_err(|error| PyValueError::new_err(error.to_string()))?;
    schema_from_json_schema(&value).map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Parse a card-ref string (`space/Kind/name@version`) at the Python boundary.
fn parse_card_ref(card_ref: &str) -> PyResult<CardRef> {
    card_ref
        .parse::<CardRef>()
        .map_err(|error| PyValueError::new_err(error.to_string()))
}

/// Register the `bifrost` write handle on the supplied module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_bifrost(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Bifrost>()?;
    Ok(())
}

/// Register the `observe` fire-and-forget telemetry function on the module.
///
/// # Errors
/// Returns PyO3 registration errors.
pub fn register_observe(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_function(wrap_pyfunction!(record, module)?)?;
    Ok(())
}
