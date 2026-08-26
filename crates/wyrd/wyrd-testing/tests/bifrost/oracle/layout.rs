//! Oracle journeys — Declared physical layout: round-trip through ingest and read, and the
//! pruning it enables.
//!
//! Module of the `oracle` binary; see `oracle.rs` for the capability it
//! proves and `support.rs` for the fixtures it shares.

use arrow::array::{Int64Array, StringArray, TimestampMicrosecondArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use chrono::{Timelike, Utc};
use serde::Serialize;
use std::sync::Arc;
use vala_bifrost_redux::catalog::{TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_sdk::BifrostGrpcTransport;
use wyrd_client::WyrdClient;
use wyrd_spec::DataTenantId;
use wyrd_spec::vala::api::{
    BifrostTableDescription, DataTypeSpec, FieldSpec, NullOrderWire, PhysicalLayoutWire,
    QueryTerminalOutcome, RegisterTableRequest, SortDirectionWire, SortKeyWire,
    TimeGranularityWire,
};
use wyrd_testing::WyrdTestServer;
use wyrd_testing::bifrost::{BifrostClusterSpec, WyrdTestCluster};

use crate::support::*;

/// The caller-owned namespace the public HTTP register route admits.
const DECLARED_LAYOUT_NAMESPACE: &str = "vala.datasets";

/// One deliberately non-default declaration for the HTTP round-trip journey.
///
/// It names the managed `wyrd_event_time` column as the leading sort key — a
/// column the previous contract reserved and refused — puts a user column
/// behind it, and adds one user Bloom column on top of the managed floor.
fn declared_journey_layout() -> PhysicalLayoutWire {
    PhysicalLayoutWire {
        partition_granularity: TimeGranularityWire::Hour,
        sort_keys: vec![
            SortKeyWire {
                column: wyrd_spec::vala::WYRD_EVENT_TIME.to_owned(),
                direction: SortDirectionWire::Desc,
                null_order: NullOrderWire::Last,
            },
            SortKeyWire {
                column: "id".to_owned(),
                direction: SortDirectionWire::Asc,
                null_order: NullOrderWire::Last,
            },
        ],
        bloom_columns: vec!["value".to_owned()],
    }
}

/// Build one non-null user field descriptor for the journey's register body.
fn declared_layout_field(name: &str, data_type: DataTypeSpec) -> FieldSpec {
    FieldSpec {
        name: name.to_owned(),
        data_type,
        nullable: false,
        metadata: std::collections::BTreeMap::new(),
    }
}

/// Issue one authenticated public JSON request and return its status and body.
///
/// The journey reads refusal bodies as well as successful ones, so it cannot
/// use a typed client helper that discards the problem document.
///
/// # Errors
///
/// Returns a bearer-exchange, transport, or JSON decode error.
async fn declared_layout_http_json<S: Serialize>(
    base_url: &str,
    client: &WyrdClient,
    method: reqwest::Method,
    path: &str,
    body: Option<&S>,
) -> Result<(reqwest::StatusCode, serde_json::Value), JourneyError> {
    let bearer = client.auth().bearer().await?;
    let base_url = base_url.trim_end_matches('/');
    let mut request = reqwest::Client::new()
        .request(method, format!("{base_url}{path}"))
        .header("x-wyrd-access-token", format!("Bearer {}", bearer.expose()));
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request.send().await?;
    let status = response.status();
    Ok((status, response.json().await?))
}

/// Encode one journey row carrying the caller's `card_ref` and event time.
///
/// The user schema fingerprint strips `card_ref` and every `wyrd_*` column, so
/// a batch presenting both still resolves against the registered `[id, value]`
/// table while binding the row to the writer's card and placing it in an exact
/// hour partition.
fn declared_layout_ipc(id: i64, value: &str, card_ref: &str, event_micros: i64) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, false),
        Field::new(wyrd_spec::vala::CARD_REF, DataType::Utf8, false),
        Field::new(
            wyrd_spec::vala::WYRD_EVENT_TIME,
            DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())),
            false,
        ),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![id])),
            Arc::new(StringArray::from(vec![value])),
            Arc::new(StringArray::from(vec![card_ref])),
            Arc::new(TimestampMicrosecondArray::from(vec![event_micros]).with_timezone("UTC")),
        ],
    )
    .expect("fixed declared-layout arrays share a length");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, schema.as_ref()).expect("valid schema");
    writer.write(&batch).expect("in-memory IPC write");
    writer.finish().expect("in-memory IPC finish");
    bytes
}

/// Return every Parquet object currently stored under one table's prefix.
///
/// # Errors
///
/// Returns the object-store listing or read error.
async fn declared_layout_parquet_footers(
    cluster: &WyrdTestCluster,
    object_prefix: &str,
) -> Result<Vec<(String, parquet::file::metadata::ParquetMetaData)>, JourneyError> {
    let operator = cluster.storage_operator();
    let entries = operator
        .list_with(&format!("{}/", object_prefix.trim_end_matches('/')))
        .recursive(true)
        .await?;
    let mut footers = Vec::new();
    for entry in entries {
        let path = entry.path().to_owned();
        if !path.ends_with(".parquet") {
            continue;
        }
        let bytes = operator.read(&path).await?.to_bytes();
        use parquet::file::reader::FileReader;
        let reader = parquet::file::serialized_reader::SerializedFileReader::new(bytes)?;
        footers.push((path, reader.metadata().clone()));
    }
    Ok(footers)
}

/// A caller-declared physical layout survives HTTP registration unchanged,
/// projects identically into Iceberg and the Parquet footers, prunes a
/// one-hour predicate, and remains rewritable by production Forge.
///
/// This is the single user-facing proof for the canonical layout contract: it
/// is the only place the new rules — a managed column as a legal sort key, a
/// four-key cap, a Bloom floor that excludes the per-file-constant tenant
/// column, and a wire that names only a granularity — are observed the way a
/// real caller observes them.
#[tokio::test]
#[ignore = "requires the serialized Postgres-backed Oracle journey lane"]
async fn pg_bifrost_declared_layout_round_trips_and_prunes() {
    prove_declared_layout_round_trip()
        .await
        .expect("declared layout HTTP round-trip journey");
}

/// Drives the declared-layout journey end to end.
///
/// # Errors
///
/// Returns a cluster, client, HTTP, catalog, telemetry, or Forge error
/// surfaced by any journey step.
#[allow(clippy::too_many_lines)]
async fn prove_declared_layout_round_trip() -> Result<(), JourneyError> {
    let cluster = WyrdTestCluster::start_spec_with_forge_config_and_completion_observer(
        BifrostClusterSpec::one_mixed().with_system_resources(forge_convergence_system_resources()),
        vala_bifrost_redux::forge::ForgeConfig {
            snapshot_expiry_enabled: false,
            max_files_per_bin: 2,
            ..vala_bifrost_redux::forge::ForgeConfig::default()
        },
    )
    .await?;
    let server = cluster.server(0).ok_or("missing declared-layout server")?;
    let tenant = cluster.data_tenant_id();
    let base_url = server.base_url().ok_or("missing HTTP URL")?.to_owned();
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, "declared-layout-writer", &["admin"])
        .await?;
    let card_ref = bootstrap
        .card_ref()
        .ok_or("machine bootstrap carries a card ref")?
        .to_string();
    let writer = client_from_bootstrap(server, bootstrap).await?;

    // The wire itself: a granularity and two lists, and no field naming a
    // partition column anywhere in the object.
    let declared = declared_journey_layout();
    let wire = serde_json::to_value(&declared)?;
    let mut wire_keys = wire
        .as_object()
        .ok_or("the layout serializes as a JSON object")?
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    wire_keys.sort_unstable();
    if wire_keys != vec!["bloom_columns", "partition_granularity", "sort_keys"] {
        return Err(
            format!("physical layout wire carries unexpected fields: {wire_keys:?}").into(),
        );
    }

    let name = unique_table("declared_layout");
    let fqn = format!("{DECLARED_LAYOUT_NAMESPACE}.{name}");
    let register = RegisterTableRequest {
        namespace: DECLARED_LAYOUT_NAMESPACE.to_owned(),
        name: name.clone(),
        fields: vec![
            declared_layout_field("id", DataTypeSpec::Int64),
            declared_layout_field("value", DataTypeSpec::Utf8),
        ],
        physical_layout: Some(declared.clone()),
    };
    let (status, _) = declared_layout_http_json(
        &base_url,
        &writer,
        reqwest::Method::POST,
        "/v1/bifrost/tables",
        Some(&register),
    )
    .await?;
    if !status.is_success() {
        return Err(format!("declared registration failed with {status}").into());
    }

    // A fifth sort key is refused by the public contract, by its stable code.
    let mut too_many = declared.clone();
    too_many.sort_keys = ["wyrd_event_time", "id", "value", "run_id", "principal_id"]
        .into_iter()
        .map(|column| SortKeyWire {
            column: column.to_owned(),
            direction: SortDirectionWire::Asc,
            null_order: NullOrderWire::Last,
        })
        .collect();
    let mut refused = register.clone();
    refused.name = unique_table("declared_layout_too_many");
    refused.physical_layout = Some(too_many);
    let (refusal_status, refusal_body) = declared_layout_http_json(
        &base_url,
        &writer,
        reqwest::Method::POST,
        "/v1/bifrost/tables",
        Some(&refused),
    )
    .await?;
    if refusal_status != reqwest::StatusCode::BAD_REQUEST {
        return Err(format!("a fifth sort key must be a 400, saw {refusal_status}").into());
    }
    if refusal_body.get("code").and_then(serde_json::Value::as_str)
        != Some("WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT")
    {
        return Err(format!("unexpected layout refusal body: {refusal_body}").into());
    }
    if !refusal_body.to_string().contains("too_many_keys") {
        return Err(format!("the refusal must name too_many_keys: {refusal_body}").into());
    }

    // Describe returns the canonical layout: nothing injected ahead of the
    // caller's keys, the managed floor ahead of the caller's Bloom column, and
    // the per-file-constant tenant column in neither list.
    let describe_path = format!("/v1/bifrost/tables/{DECLARED_LAYOUT_NAMESPACE}/{name}");
    let (describe_status, describe_body) = declared_layout_http_json(
        &base_url,
        &writer,
        reqwest::Method::GET,
        &describe_path,
        None::<&()>,
    )
    .await?;
    if !describe_status.is_success() {
        return Err(format!("describe failed with {describe_status}").into());
    }
    let description: BifrostTableDescription = serde_json::from_value(describe_body)?;
    let canonical = description.physical_layout.clone();
    if canonical.partition_granularity != TimeGranularityWire::Hour {
        return Err("the declared hourly granularity must survive registration".into());
    }
    if canonical.sort_keys != declared.sort_keys {
        return Err(format!(
            "the canonical sort order must equal the declaration exactly, saw {:?}",
            canonical.sort_keys
        )
        .into());
    }
    if canonical.bloom_columns
        != vec![
            "run_id".to_owned(),
            "card_uid".to_owned(),
            "principal_id".to_owned(),
            "value".to_owned(),
        ]
    {
        return Err(format!(
            "the canonical Bloom set must be the managed floor then the caller's column, saw {:?}",
            canonical.bloom_columns
        )
        .into());
    }
    if canonical
        .sort_keys
        .iter()
        .any(|key| key.column == "data_tenant_id")
        || canonical
            .bloom_columns
            .iter()
            .any(|column| column == "data_tenant_id")
    {
        return Err("the per-file-constant tenant column must appear in neither list".into());
    }

    // Two rows in two distinct hour partitions, each bound to the writer's card.
    let now = Utc::now()
        .with_minute(0)
        .and_then(|value| value.with_second(0))
        .and_then(|value| value.with_nanosecond(0))
        .ok_or("truncating to the hour is representable")?;
    let recent_hour = now;
    let earlier_hour = now - chrono::Duration::hours(2);
    let transport = BifrostGrpcTransport::connect(&writer).await?;
    for (id, value, hour) in [
        (1_i64, "earlier", earlier_hour),
        (2_i64, "recent", recent_hour),
    ] {
        let micros = hour
            .timestamp_micros()
            .checked_add(60_000_000)
            .ok_or("hour-offset microseconds fit i64")?;
        transport
            .insert_batch(
                &fqn,
                uuid::Uuid::now_v7().into_bytes(),
                declared_layout_ipc(id, value, &card_ref, micros),
            )
            .await?;
        server.flush_bifrost().await?;
    }
    cluster.refresh_oracle_snapshots().await?;

    // The Iceberg projection agrees with the canonical layout.
    let table_ref = TableRef::new(BifrostNamespace::Datasets, name.clone());
    let binding = TenantTableBinding::resolve((tenant, table_ref.clone()))?;
    let catalog = server
        .state()
        .bifrost_catalog()
        .ok_or("Scribe composition retains the shared catalog")?;
    let physical = catalog
        .iceberg_catalog()
        .load_table(&binding.table_ident())
        .await?;
    let metadata = physical.metadata();
    let iceberg_schema = metadata.current_schema();
    let arrow_schema = iceberg::arrow::schema_to_arrow_schema(iceberg_schema)?;
    let layout = vala_bifrost_redux::catalog::layout::PhysicalLayout::from_stored_wire(
        &fqn,
        &arrow_schema,
        &canonical,
    )?;
    if metadata.default_partition_spec().fields()
        != layout
            .iceberg_partition_spec(iceberg_schema)?
            .bind(iceberg_schema.as_ref().clone())?
            .fields()
    {
        return Err("the Iceberg partition spec must match the canonical layout".into());
    }
    if metadata.default_sort_order().fields != layout.iceberg_sort_order(iceberg_schema)?.fields {
        return Err("the Iceberg sort order must match the canonical layout".into());
    }

    // Footer evidence: every declared Bloom column present in a written file
    // carries a Bloom offset, and the tenant column never does.
    let footers = declared_layout_parquet_footers(&cluster, &binding.object_prefix).await?;
    if footers.is_empty() {
        return Err("the sealed journey wrote no Parquet objects".into());
    }
    for (path, footer) in &footers {
        for row_group in footer.row_groups() {
            for column in row_group.columns() {
                let column_name = column.column_descr().name();
                if column_name == "data_tenant_id" {
                    if column.bloom_filter_offset().is_some() {
                        return Err(
                            format!("{path} Bloomed the per-file-constant tenant column").into(),
                        );
                    }
                    continue;
                }
                if canonical
                    .bloom_columns
                    .iter()
                    .any(|declared| declared == column_name)
                    && column.bloom_filter_offset().is_none()
                {
                    return Err(format!(
                        "{path} is missing a Bloom filter for declared column {column_name}"
                    )
                    .into());
                }
            }
        }
    }

    // The principal-bound card resolves to a non-null card uid for every row.
    let reader = client(server, "declared-layout-reader").await?;
    let (bound_rows, bound_outcome, bound_error) = query_statement(
        &reader,
        format!("SELECT id FROM {fqn} WHERE card_uid IS NOT NULL ORDER BY id"),
    )
    .await?;
    if bound_outcome != QueryTerminalOutcome::Success || bound_error.is_some() {
        return Err(format!(
            "the card-bound read did not succeed: {bound_outcome:?} {bound_error:?}"
        )
        .into());
    }
    if bound_rows != 2 {
        return Err(format!(
            "every principal-bound row must carry a card uid, saw {bound_rows} of 2"
        )
        .into());
    }

    // A one-hour predicate prunes physical work, measured against an
    // unfiltered baseline rather than inferred from the plan.
    let unfiltered_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let (unfiltered_rows, _, _) =
        query_statement(&reader, format!("SELECT id, value FROM {fqn} ORDER BY id")).await?;
    if unfiltered_rows != 2 {
        return Err(format!("unfiltered baseline expected 2 rows, saw {unfiltered_rows}").into());
    }
    let unfiltered_delta = cluster
        .telemetry()
        .delta_since(&unfiltered_checkpoint)
        .map_err(|error| error.to_string())?;
    let unfiltered_files = sum_metric(&unfiltered_delta, "oracle_query_files_scanned_total");
    let unfiltered_bytes = sum_metric(&unfiltered_delta, "oracle_query_bytes_scanned_total");
    let unfiltered_row_groups =
        sum_metric(&unfiltered_delta, "oracle_query_row_groups_scanned_total");

    let selective_checkpoint = cluster
        .telemetry()
        .checkpoint()
        .map_err(|error| error.to_string())?;
    let window_start = recent_hour.format("%Y-%m-%dT%H:%M:%S%.6f").to_string();
    let window_end = (recent_hour + chrono::Duration::hours(1))
        .format("%Y-%m-%dT%H:%M:%S%.6f")
        .to_string();
    let (selective_rows, selective_outcome, selective_error) = query_statement(
        &reader,
        format!(
            "SELECT id, value FROM {fqn} \
             WHERE wyrd_event_time >= arrow_cast('{window_start}', 'Timestamp(Microsecond, Some(\"UTC\"))') \
             AND wyrd_event_time < arrow_cast('{window_end}', 'Timestamp(Microsecond, Some(\"UTC\"))') \
             ORDER BY id"
        ),
    )
    .await?;
    if selective_outcome != QueryTerminalOutcome::Success || selective_error.is_some() {
        return Err(format!(
            "the one-hour predicate did not succeed: {selective_outcome:?} {selective_error:?}"
        )
        .into());
    }
    if selective_rows != 1 {
        return Err(format!(
            "the one-hour predicate expected exactly one residual row, saw {selective_rows}"
        )
        .into());
    }
    let selective_delta = cluster
        .telemetry()
        .delta_since(&selective_checkpoint)
        .map_err(|error| error.to_string())?;
    let selective_files = sum_metric(&selective_delta, "oracle_query_files_scanned_total");
    let selective_bytes = sum_metric(&selective_delta, "oracle_query_bytes_scanned_total");
    let selective_row_groups =
        sum_metric(&selective_delta, "oracle_query_row_groups_scanned_total");
    if !(selective_files < unfiltered_files || selective_row_groups < unfiltered_row_groups) {
        return Err(format!(
            "the one-hour predicate must select strictly fewer files or row groups: \
             files selective={selective_files} unfiltered={unfiltered_files}; \
             row groups selective={selective_row_groups} unfiltered={unfiltered_row_groups}"
        )
        .into());
    }
    if !matches!(
        selective_bytes.partial_cmp(&unfiltered_bytes),
        Some(std::cmp::Ordering::Less)
    ) {
        return Err(format!(
            "the one-hour predicate must scan strictly fewer bytes: selective={selective_bytes} unfiltered={unfiltered_bytes}"
        )
        .into());
    }

    // Production Forge accepts the canonical layout and rewrites the table.
    // It runs last so the pruning measurement above observes the sealed
    // per-hour objects the declared layout produced, not a compacted rewrite.
    prove_declared_layout_forge_rewrite(&cluster, server, tenant, &table_ref).await?;
    cluster.shutdown().await?;
    Ok(())
}

/// Drives production Forge over the caller-registered table until it completes
/// one real rewrite, proving the canonical layout passes Forge's own layout
/// validation and its discovery, planning, and replacement path.
///
/// # Errors
///
/// Returns a Forge inspection, scheduling, or convergence error, and reports a
/// stall rather than hanging when the bounded continuation does not settle.
async fn prove_declared_layout_forge_rewrite(
    cluster: &WyrdTestCluster,
    server: &WyrdTestServer,
    tenant: DataTenantId,
    table: &TableRef,
) -> Result<(), JourneyError> {
    server.forge_clock().advance(chrono::Duration::days(1))?;
    let observer = cluster
        .forge_completion_observer()
        .ok_or("Forge completion observer")?;

    // Drain staging debt so the sealed files become planned Iceberg inputs.
    for _ in 0..64 {
        if server
            .inspect_forge_workflow_ref_for_test(tenant, table)
            .await?
            .uncompacted_staging_files
            == 0
        {
            break;
        }
        server
            .forge_clock()
            .advance(chrono::Duration::minutes(15))?;
        let expected_attempts = observer.attempts().saturating_add(1);
        let completed_passes = server.completed_forge_scheduler_passes_for_test();
        cluster.request_forge_scheduler_pass_for_test();
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            server.wait_for_forge_scheduler_passes_for_test(completed_passes + 1),
        )
        .await?;
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            observer.wait_for_attempts_at_least(expected_attempts),
        )
        .await;
    }
    if server
        .inspect_forge_workflow_ref_for_test(tenant, table)
        .await?
        .uncompacted_staging_files
        != 0
    {
        return Err("bounded production continuation did not drain staging debt".into());
    }

    // The staging fold is the production rewrite: it reads the sealed Scribe
    // objects through Forge discovery — which is where the canonical layout
    // must pass `validate_supported_layout` — and commits replacement files it
    // wrote itself under the Forge writer prefix.
    let after = server
        .inspect_forge_table_ref_for_test(tenant, table)
        .await?;
    if after.data_file_count() != 2 {
        return Err(format!(
            "the rewrite must publish exactly the two sealed hour partitions, saw {}",
            after.data_file_count()
        )
        .into());
    }
    if !after
        .live_data_files
        .iter()
        .all(|file| file.path.contains("/data/forge/"))
    {
        return Err(format!(
            "every live file must be a Forge rewrite output: {:?}",
            after.live_data_files
        )
        .into());
    }
    if after.tasks.is_empty() || after.tasks.iter().any(|(_, state)| state != "succeeded") {
        return Err(format!(
            "every production Forge task over the declared layout must succeed: {:?}",
            after.tasks
        )
        .into());
    }
    if after.active_claims != 0 || after.active_attempts != 0 {
        return Err(format!("Forge did not settle after the rewrite: {after:?}").into());
    }
    Ok(())
}
