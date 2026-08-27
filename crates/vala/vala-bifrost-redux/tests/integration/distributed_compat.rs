//! Native compatibility proof for the managed analytical universe.
//!
//! Wyrd ships no distributed query path. `datafusion-distributed` is pinned as a
//! test-only dependency purely to prove that a third-party crate built against
//! `DataFusion` 55 links into the *same* native `datafusion`, `datafusion-proto`,
//! and `arrow` packages Bifrost uses, rather than a second copy reached through
//! a serialization adapter. A duplicate analytical universe would make the two
//! `SessionState`, `ExecutionPlan`, and `PhysicalExtensionCodec` types distinct
//! and this module would stop compiling. No production code reaches these APIs.

use std::sync::Arc;

use arrow::array::Int64Array;
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use datafusion::datasource::MemTable;
use datafusion::error::DataFusionError;
use datafusion::execution::context::SessionContext;
use datafusion::execution::session_state::SessionStateBuilder;
use datafusion_distributed::{DistributedExt, SessionStateBuilderExt, WorkerResolver};
use url::Url;
use vala_bifrost_redux::oracle::codec::OraclePhysicalExtensionCodec;
use vala_bifrost_redux::resources::OracleSessionShape;

/// Declares a cluster with no workers so the distributed planner keeps every
/// task on the coordinator.
///
/// Wyrd runs no Scribe/Oracle worker fleet for the distributed planner, and this
/// proof is about type and configuration compatibility rather than remote
/// execution, so the resolver is deliberately empty.
struct NoWorkerCluster;

impl WorkerResolver for NoWorkerCluster {
    /// Reports an empty cluster.
    ///
    /// # Errors
    ///
    /// Never returns an error; the signature is fixed by the trait.
    fn get_urls(&self) -> Result<Vec<Url>, DataFusionError> {
        Ok(Vec::new())
    }
}

/// Oracle's own session shape survives the distributed planner and still executes.
///
/// The shared boundary under test is `SessionStateBuilder`: Bifrost derives a
/// `SessionConfig` from a memory grant through [`OracleSessionShape`], the
/// distributed planner extension consumes that same builder, and the resulting
/// state must still carry Bifrost's fixed Parquet pushdown and indexing options
/// and still execute a plan over Arrow batches Bifrost built itself. That is only
/// possible if both crates resolve to one native `DataFusion` 55 / Arrow 59.2 graph.
///
/// Oracle's own `PhysicalExtensionCodec` registers through the distributed crate's
/// public codec API as part of the same call chain, which additionally pins that
/// both crates name one `datafusion-proto` trait — the `DataFusion` 55 signature
/// change each had to adopt independently.
///
/// # Panics
///
/// Panics when the shared session cannot be built or the query does not return
/// the single exact aggregate row, either of which means the analytical universe
/// is no longer shared.
#[tokio::test]
async fn oracle_session_shape_composes_with_the_distributed_planner_natively() {
    let shape = OracleSessionShape::for_grant(512 * 1024 * 1024, 4, 4);
    let config = shape.session_config();
    let expected_partitions = config.options().execution.target_partitions;
    let expected_batch_size = config.options().execution.batch_size;

    // The cross-crate call: Bifrost's config enters the third-party extension.
    let state = SessionStateBuilder::new()
        .with_default_features()
        .with_config(config)
        .with_distributed_planner()
        .with_distributed_worker_resolver(NoWorkerCluster)
        .with_distributed_user_codec(OraclePhysicalExtensionCodec::encoder())
        .build();

    let options = state.config().options();
    assert!(
        options.execution.parquet.pushdown_filters,
        "the distributed planner must not clear Oracle's Parquet pushdown"
    );
    assert!(
        options.execution.parquet.reorder_filters,
        "the distributed planner must not clear Oracle's filter reordering"
    );
    assert!(
        options.execution.parquet.bloom_filter_on_read,
        "the distributed planner must not clear Oracle's bloom-filter reads"
    );
    assert_eq!(
        options.execution.target_partitions, expected_partitions,
        "the distributed planner must not restate Oracle's partition width"
    );
    assert_eq!(
        options.execution.batch_size, expected_batch_size,
        "the distributed planner must not restate Oracle's batch size"
    );

    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![7_i64, 9]))],
    )
    .expect("the fixture batch matches its own schema");
    let table = MemTable::try_new(Arc::clone(&schema), vec![vec![batch]])
        .expect("a single-partition memory table accepts one matching batch");

    let context = SessionContext::new_with_state(state);
    context
        .register_table("compat", Arc::new(table))
        .expect("the fixture table registers exactly once");
    let rows = context
        .sql("SELECT count(*) AS total, sum(value) AS total_value FROM compat")
        .await
        .expect("the shared session plans the aggregate")
        .collect()
        .await
        .expect("the shared session executes the aggregate");

    assert_eq!(rows.len(), 1, "one aggregate batch is returned");
    let total = rows[0]
        .column_by_name("total")
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .expect("count(*) decodes as the native Arrow Int64 array");
    assert_eq!(total.values(), &[2]);
    let total_value = rows[0]
        .column_by_name("total_value")
        .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
        .expect("sum(value) decodes as the native Arrow Int64 array");
    assert_eq!(total_value.values(), &[16]);
}
