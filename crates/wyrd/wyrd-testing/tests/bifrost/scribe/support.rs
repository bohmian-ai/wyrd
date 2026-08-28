//! Shared production-harness setup for the Scribe suite.
//!
//! Every test in this binary uses the S9 harness and public routes: a real
//! server, real Postgres, the real object store, the production Scribe service
//! and the production Oracle read path. Nothing here builds a parallel topology
//! or writes a durable row the production path did not produce.

use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::ScribeGeometry;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use arrow::datatypes::{DataType, Field};

/// Starts one bound production server with the default Scribe geometry.
///
/// Bound rather than in-process because every Scribe case here drives public
/// gRPC ingest and the public query route, which need real endpoints.
pub(super) async fn start_scribe_server() -> WyrdTestServer {
    WyrdTestServer::start_bound()
        .await
        .expect("the Scribe production harness starts")
}

/// Starts one bound production server with an explicit Scribe geometry.
///
/// The geometry is the only production control a scaled case may move: it lets
/// a test reach a rotation, a target roll or a residue boundary without writing
/// production-sized data, while every other control stays exactly what
/// production uses.
pub(super) async fn start_scribe_server_with_geometry(geometry: ScribeGeometry) -> WyrdTestServer {
    WyrdTestServer::builder()
        .with_scribe_geometry_for_test(geometry)
        .start_bound()
        .await
        .expect("the Scribe production harness starts with the requested geometry")
}

/// Registers one single-column table for a tenant through the real catalog.
pub(super) async fn register_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    namespace: BifrostNamespace,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: TableRef::new(namespace, name),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the table");
    format!("{}.{name}", namespace.as_str())
}

/// Builds a unique table name for one case.
pub(super) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}
