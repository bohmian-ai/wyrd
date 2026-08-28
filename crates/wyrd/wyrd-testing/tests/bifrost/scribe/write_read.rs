//! The Scribe write/flush/read user journey.

use wyrd_testing::bifrost::{ScribeCacheMode, ScribeProductionWorkloadV1};

use super::support::start_scribe_server;

/// AC22 journey owner: a client writes, seals, and reads its own rows back.
///
/// This is the complete client-to-server path a real caller takes — public
/// gRPC ingest, the durable seal and publication, and the public query route —
/// driven by the canonical production workload record rather than a fixture
/// local to this file, so the cache task and Forge later run these exact bytes.
///
/// The record is serialized and deserialized before the run so a field that
/// only exists in this process cannot become part of the contract.
///
/// # Panics
///
/// Panics when the record does not survive its wire form, when the run does not
/// satisfy the record's checkpoints and digest, or when a published object's
/// promotion record cannot rebuild the Iceberg `DataFile` it claims.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_write_flush_read_user_journey() {
    let _tmp_guard = wyrd_telemetry::init(wyrd_telemetry::TelemetryConfig {
        filter: "vala_bifrost_redux=debug,wyrd_server=debug,datafusion=debug".to_owned(),
        ..wyrd_telemetry::TelemetryConfig::default()
    });
    let canonical = ScribeProductionWorkloadV1::canonical();
    let bytes = serde_json::to_vec(&canonical).expect("the canonical record serializes");
    let workload: ScribeProductionWorkloadV1 =
        serde_json::from_slice(&bytes).expect("the canonical record deserializes");
    assert_eq!(workload, canonical, "the handoff must be lossless");

    let server = start_scribe_server().await;
    let evidence = server
        .run_scribe_production_workload(&workload, ScribeCacheMode::Disabled)
        .await
        .expect("the production workload runs on public routes");
    evidence
        .assert_matches(&workload)
        .expect("the run satisfies every normative field of the record");

    let published = evidence
        .checkpoints
        .last()
        .expect("the run reached its final checkpoint");
    let objects: usize = published
        .published
        .values()
        .map(|records| records.len())
        .sum();
    assert!(
        objects > 0,
        "a sealed journey must publish at least one hot object"
    );
    for records in published.published.values() {
        for record in records {
            record
                .data_file()
                .expect("every published record rebuilds its Iceberg data file");
        }
    }

    server.shutdown().await.expect("the server drains cleanly");
}
