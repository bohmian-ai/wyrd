//! Public-route helpers for the Forge journey group.
//!
//! Contains no tests. Every item here reaches the server the way a real caller
//! does — registration through the real catalog route, ingest over public gRPC,
//! reads over the public strict fused query route — so a journey that uses them
//! is exercising the shipped surface rather than an in-process engine.

use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};
use uuid::Uuid;
use vala_bifrost_redux::catalog::{CreateTableRequest, TableRef, TenantTableBinding};
use vala_bifrost_redux::namespaces::BifrostNamespace;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use arrow::datatypes::{DataType, Field};

/// Rows every strict fused public read in this process has really returned.
///
/// Counted where the result batches are decoded, so a journey reconciling
/// Oracle's own returned-row counter compares it against what the public route
/// actually streamed back rather than against the fixture rows it expected.
/// Process-global because the runner gives each test its own process, so the
/// count belongs to exactly one journey.
static PUBLIC_ROWS_RETURNED: AtomicU64 = AtomicU64::new(0);

/// Returns the rows strict fused public reads have returned in this process.
pub(crate) fn public_rows_returned() -> u64 {
    PUBLIC_ROWS_RETURNED.load(Ordering::Acquire)
}

/// Domain tag and version for the journey's canonical row digest.
///
/// Domain separation keeps this digest from ever colliding with another
/// hash over the same bytes, and the version makes a deliberate change to
/// the encoding visible rather than silently producing different values.
const ROW_DIGEST_DOMAIN: &[u8] = b"wyrd.bifrost.journey.public-rows.v1";

/// One acknowledged row, identified exactly as public ingest identified it.
///
/// The value is payload; the identity is `(batch_id, row_ordinal)`. Holding
/// them together is what makes a public read an *exactness* check rather than
/// a multiset check: a promotion or rewrite that dropped one row and duplicated
/// another would leave the sorted value column unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManagedRow {
    /// Batch identity the caller submitted with the append.
    pub(crate) batch_id: Uuid,
    /// Zero-based position of the row within its submitted batch.
    pub(crate) row_ordinal: i32,
    /// User payload column.
    pub(crate) value: i64,
}

/// One journey table, held with both the names the journey needs.
///
/// The qualified name is what a public query addresses; the binding is what
/// the *platform* evidence is addressed by. Resolving the binding through the
/// production `TenantTableBinding` rather than formatting a namespace or an
/// object prefix here keeps the journey from re-deriving a physical layout
/// rule it does not own — if that rule changes, this resolves to the new one.
pub(crate) struct JourneyTable {
    /// Public logical name, namespace-qualified, as SQL addresses it.
    pub(crate) qualified: String,
    /// Bare registered table name, as `vala.file_list` records it.
    pub(crate) name: String,
    /// Physical binding the catalog and object store are addressed by.
    pub(crate) binding: TenantTableBinding,
}

/// Registers one single-column table for `tenant` through the real catalog.
///
/// # Panics
///
/// Panics when the catalog refuses the registration, or when the registered
/// identity cannot be resolved to its physical binding.
pub(crate) async fn register_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> JourneyTable {
    let table_ref = TableRef::new(BifrostNamespace::Datasets, name);
    server
        .create_bifrost_table_for_test(CreateTableRequest {
            table: table_ref.clone(),
            user_fields: vec![Field::new("value", DataType::Int64, false)],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the journey table");
    JourneyTable {
        qualified: format!("{}.{name}", BifrostNamespace::Datasets.as_str()),
        name: name.to_owned(),
        binding: TenantTableBinding::resolve((tenant, table_ref))
            .expect("the registered table resolves to its physical binding"),
    }
}

/// Builds a unique table name for one journey.
pub(crate) fn unique_table(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// Builds one authenticated public SDK client bound to `tenant`.
///
/// Each tenant gets its own service principal and API key, which is what makes
/// a cross-tenant read a genuine authorization decision rather than a filter
/// applied to a shared credential.
///
/// # Panics
///
/// Panics when the tenant cannot be bootstrapped or the server is not bound.
pub(crate) async fn tenant_client(
    server: &WyrdTestServer,
    tenant: DataTenantId,
) -> wyrd_client::WyrdClient {
    let bootstrap = server
        .bootstrap_service_in_tenant(tenant, &unique_table("forge_client"), &["admin"])
        .await
        .expect("tenant service bootstrap");
    let api_key = bootstrap.api_key().expect("service API key").clone();
    wyrd_client::WyrdClient::with_config(wyrd_client::config::ClientConfig {
        grpc: wyrd_client::transport::GrpcConfig {
            endpoint: server.grpc_url().expect("bound gRPC URL"),
            connect_retries: 0,
            ..wyrd_client::transport::GrpcConfig::default()
        },
        http: wyrd_client::transport::HttpConfig {
            base_url: server.base_url().expect("bound HTTP URL").to_owned(),
            ..wyrd_client::transport::HttpConfig::default()
        },
        credential: Some(api_key),
        ..wyrd_client::config::ClientConfig::default()
    })
    .expect("tenant SDK client")
}

/// Encodes one batch as the native Arrow IPC stream public ingest accepts.
///
/// # Panics
///
/// Panics when the batch cannot be encoded, which is a fixture fault.
fn encode_ipc(batch: &arrow::record_batch::RecordBatch) -> Vec<u8> {
    let mut ipc = Vec::new();
    let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut ipc, batch.schema().as_ref())
        .expect("IPC writer");
    writer.write(batch).expect("IPC batch");
    writer.finish().expect("IPC terminal");
    ipc
}

/// Appends one batch of values through public authenticated gRPC ingest.
///
/// The batch identity is supplied by the caller rather than generated here, so
/// the journey can name every row it acknowledged before the server has said
/// anything back. Returns the exact rows that append must later read back:
/// the ordinal is the zero-based position within this submitted batch, which
/// is the managed contract public ingest assigns.
///
/// # Panics
///
/// Panics when the transport cannot connect or the append is refused; a
/// journey's ingest is expected to be accepted, so a refusal is a failure.
pub(crate) async fn append_values(
    client: &wyrd_client::WyrdClient,
    table: &str,
    batch_id: Uuid,
    rows: &[i64],
) -> Vec<ManagedRow> {
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        false,
    )]));
    let batch = arrow::record_batch::RecordBatch::try_new(
        std::sync::Arc::clone(&schema),
        vec![std::sync::Arc::new(arrow::array::Int64Array::from(
            rows.to_vec(),
        ))],
    )
    .expect("value batch");
    wyrd_testing::bifrost::write::RawIngest::connect(client)
        .await
        .expect("public ingest transport connects")
        .insert(table, batch_id, encode_ipc(&batch))
        .await
        .expect("the public append is acknowledged");
    rows.iter()
        .enumerate()
        .map(|(ordinal, value)| ManagedRow {
            batch_id,
            row_ordinal: i32::try_from(ordinal).expect("a journey batch is small"),
            value: *value,
        })
        .collect()
}

/// Computes the canonical digest of one ordered acknowledged-row vector.
///
/// The encoding is domain-separated and length-delimited over raw identity
/// bytes: the 16-byte batch identity, then the ordinal and value as fixed-width
/// big-endian integers. Nothing about `Debug`, JSON, Arrow buffer layout, or
/// iteration order can reach the hash, so the digest means the same thing
/// whichever tier answered the read.
pub(crate) fn rows_digest(rows: &[ManagedRow]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ROW_DIGEST_DOMAIN);
    hasher.update((rows.len() as u64).to_be_bytes());
    for row in rows {
        hasher.update(row.batch_id.as_bytes());
        hasher.update(row.row_ordinal.to_be_bytes());
        hasher.update(row.value.to_be_bytes());
    }
    hex::encode(hasher.finalize())
}

/// Sorts an acknowledged-row vector into the journey's canonical order.
///
/// `(batch_id, row_ordinal)` is the whole key; the value never participates,
/// because a read that returned the right identities with a wrong payload must
/// fail rather than sort itself back into agreement.
pub(crate) fn canonical_order(mut rows: Vec<ManagedRow>) -> Vec<ManagedRow> {
    rows.sort_unstable_by_key(|row| (row.batch_id, row.row_ordinal));
    rows
}

/// Reads one table's complete acknowledged-row set through the public route.
///
/// Strict freshness and fused visibility are what make the read an authority
/// check: the answer must come from whichever tier currently owns the rows, so
/// a promotion or a rewrite that lost, duplicated, or stranded a row shows up
/// here rather than only in the catalog. The managed identity columns are
/// selected alongside the user column so the result is comparable by identity,
/// and the rows are returned in the journey's canonical order.
///
/// # Panics
///
/// Panics when the query cannot start or stream, or when a result batch does
/// not carry the three columns in their declared managed types.
pub(crate) async fn read_managed_rows(
    client: &wyrd_client::WyrdClient,
    table: &str,
) -> Vec<ManagedRow> {
    let sql = format!("SELECT wyrd_batch_id, wyrd_row_ordinal, value FROM {table}");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&strict_fused(sql.clone()))
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` starts: {error}"));
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .unwrap_or_else(|error| panic!("public query `{sql}` streams: {error}"))
    {
        rows.extend(decode_managed_rows(&batch));
    }
    canonical_order(rows)
}

/// Decodes one public result batch into acknowledged-row identities.
///
/// # Panics
///
/// Panics when the batch does not carry the three columns in their declared
/// managed Arrow types, which would mean the public projection changed shape.
///
/// Every decoded row is added to [`PUBLIC_ROWS_RETURNED`]. Both public read
/// paths funnel through here, so the tally counts exactly the rows the server
/// streamed back and nothing a caller merely expected.
fn decode_managed_rows(batch: &arrow::record_batch::RecordBatch) -> Vec<ManagedRow> {
    let batch_ids = batch
        .column_by_name("wyrd_batch_id")
        .expect("the result carries the managed batch identity")
        .as_any()
        .downcast_ref::<arrow::array::FixedSizeBinaryArray>()
        .expect("the managed batch identity stays FixedSizeBinary(16)");
    let ordinals = batch
        .column_by_name("wyrd_row_ordinal")
        .expect("the result carries the managed row ordinal")
        .as_any()
        .downcast_ref::<arrow::array::Int32Array>()
        .expect("the managed row ordinal stays Int32");
    let values = batch
        .column_by_name("value")
        .expect("the result carries the user column")
        .as_any()
        .downcast_ref::<arrow::array::Int64Array>()
        .expect("the user column stays Int64");
    PUBLIC_ROWS_RETURNED.fetch_add(batch.num_rows() as u64, Ordering::AcqRel);
    (0..batch.num_rows())
        .map(|row| {
            let raw: [u8; 16] = batch_ids
                .value(row)
                .try_into()
                .expect("the managed batch identity is exactly 16 bytes");
            ManagedRow {
                batch_id: Uuid::from_bytes(raw),
                row_ordinal: ordinals.value(row),
                value: values.value(row),
            }
        })
        .collect()
}

/// Reads the rewrite audit rows one table's retained history already holds.
///
/// `vala.audit_outbox` is delivery state: the server's own publisher moves a
/// settled range into `vala.system.audit_log` and retires it, so an audit row
/// written minutes ago is legitimately no longer in the outbox. A journey that
/// inspects Forge's audit cardinality must therefore look in both places, and
/// this is the retained half. A table that has never been published yet is an
/// empty half, not a failure.
///
/// # Panics
///
/// Panics when the retained read fails for any reason other than the table not
/// existing yet, or when a retained row does not carry the audit columns.
pub(crate) async fn retained_rewrite_audit(
    client: &wyrd_client::WyrdClient,
    resource: &str,
) -> Vec<(i64, String, Option<String>)> {
    let sql = format!(
        "SELECT seq, operation, detail FROM vala.system.audit_log \
         WHERE resource = '{resource}' AND operation LIKE 'forge.iceberg_rewrite.%'"
    );
    let mut stream = match vala_sdk::query::QueryClient::new(client)
        .query(&strict_fused(sql))
        .await
    {
        Ok(stream) => stream,
        Err(error) => {
            assert!(
                wyrd_spec::error::WyrdError::from(&error).code()
                    == "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
                "retained audit history is readable: {error:?}"
            );
            return Vec::new();
        }
    };
    let mut rows = Vec::new();
    while let Some(batch) = stream
        .next_batch()
        .await
        .expect("retained audit history streams to its terminal")
    {
        let seqs = batch
            .column_by_name("seq")
            .expect("retained audit history carries its sequence")
            .as_any()
            .downcast_ref::<arrow::array::Int64Array>()
            .expect("the audit sequence stays Int64");
        let operations = batch
            .column_by_name("operation")
            .expect("retained audit history carries its operation")
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .expect("the audit operation stays Utf8");
        let details = batch
            .column_by_name("detail")
            .expect("retained audit history carries its detail")
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .expect("the audit detail stays Utf8");
        // This is a public read like any other: it streams through Oracle, so
        // its rows belong in the same counter the journey holds telemetry to.
        PUBLIC_ROWS_RETURNED.fetch_add(batch.num_rows() as u64, Ordering::AcqRel);
        for row in 0..batch.num_rows() {
            rows.push((
                seqs.value(row),
                operations.value(row).to_owned(),
                (!arrow::array::Array::is_null(details, row)).then(|| details.value(row).to_owned()),
            ));
        }
    }
    rows
}

/// Builds the one customer read shape this journey is allowed to use.
fn strict_fused(sql: String) -> wyrd_spec::vala::api::BifrostQueryRequest {
    wyrd_spec::vala::api::BifrostQueryRequest {
        sql,
        visibility: wyrd_spec::vala::api::VisibilityMode::Fused,
        freshness: wyrd_spec::vala::api::FreshnessPolicy::Strict,
        deadline_ms: Some(120_000),
    }
}

/// Attempts one public query and returns the stable error when it is refused.
///
/// Used only for the cross-tenant probe, where the interesting outcome is the
/// refusal itself: a tenant that can name a neighbour's table at all is a
/// tenancy defect, so the journey must see an error rather than empty rows.
pub(crate) async fn try_read(
    client: &wyrd_client::WyrdClient,
    table: &str,
) -> Result<Vec<ManagedRow>, vala_sdk::ValaSdkError> {
    let sql = format!("SELECT wyrd_batch_id, wyrd_row_ordinal, value FROM {table}");
    let mut stream = vala_sdk::query::QueryClient::new(client)
        .query(&strict_fused(sql))
        .await?;
    let mut rows = Vec::new();
    while let Some(batch) = stream.next_batch().await? {
        rows.extend(decode_managed_rows(&batch));
    }
    Ok(canonical_order(rows))
}

/// Asserts one tenant cannot name a table only its neighbour registered.
///
/// The contract asserted here is the stable public anti-enumeration result: a
/// tenant-scoped *not found*, never an authorization code that would confirm
/// the table exists somewhere. It is checked as a terminal or transport
/// projection of that one stable code — a local decode, protocol, or
/// result-limit failure would also be an `Err` and must not be mistaken for a
/// tenancy decision — and the error text is checked to leak neither the
/// neighbour's tenant identity nor any physical path.
///
/// # Panics
///
/// Panics when the read succeeded, when it failed for any other reason, or
/// when the refusal leaked identity.
pub(crate) async fn assert_tenant_scoped_not_found(
    client: &wyrd_client::WyrdClient,
    table: &str,
    neighbour: DataTenantId,
    context: &str,
) {
    let error = match try_read(client, table).await {
        Ok(rows) => panic!("{context}: a neighbour-only table was readable: {rows:?}"),
        Err(error) => error,
    };
    assert!(
        matches!(
            error,
            vala_sdk::ValaSdkError::Transport(_) | vala_sdk::ValaSdkError::FailedTerminal { .. }
        ),
        "{context}: the refusal is a projection of a server decision, not a local \
         stream failure: {error:?}"
    );
    let projected = wyrd_spec::error::WyrdError::from(&error);
    let problem = projected.as_problem_json();
    assert_eq!(
        projected.code(),
        "WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND",
        "{context}: the tenant boundary answers with its stable not-found code"
    );
    assert_eq!(
        projected.status(),
        404,
        "{context}: the stable not-found code keeps its status"
    );
    // `type` is the stable catalog URI for the code itself, identical for every
    // caller and carrying no request data, so it is not part of what the refusal
    // could leak; every other member is server-authored text about this request.
    let mut disclosed = problem.clone();
    if let Some(object) = disclosed.as_object_mut() {
        object.remove("type");
    }
    let leaked = disclosed.to_string();
    assert!(
        !leaked.contains(&neighbour.as_uuid().to_string()),
        "{context}: the refusal named the neighbouring tenant: {leaked}"
    );
    assert!(
        !leaked.contains("tenants/") && !leaked.contains("://"),
        "{context}: the refusal named a physical object path: {leaked}"
    );
}
