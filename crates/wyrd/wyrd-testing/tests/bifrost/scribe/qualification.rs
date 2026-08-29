//! Production-default physical object qualification for Scribe publication.

use vala_bifrost_redux::namespaces::BifrostNamespace;
use vala_bifrost_redux::scribe::geometry::DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES;
use wyrd_spec::DataTenantId;
use wyrd_testing::WyrdTestServer;

use super::support::{append_batch, read_sql, start_scribe_server, tenant_client, unique_table};

/// Bytes of incompressible payload each row carries.
///
/// The staging target is a physical byte target and every Bifrost object is
/// written with Zstd, so a compressible payload would let the case reach the
/// target in logical bytes while the sealed object stayed far below it. A
/// per-row block of pseudo-random bytes makes encoded size track written size,
/// which is the only way a physical qualification claim can be honest.
const PAYLOAD_BYTES: usize = 1024;
/// Rows in one public ingest request.
///
/// Sized so the encoded Arrow IPC envelope stays under the production 16 MiB
/// ingest request limit rather than needing a relaxed gate.
const ROWS_PER_REQUEST: usize = 12_000;
/// Public ingest requests the case sends.
///
/// 624,000 rows of 1 KiB payload is about 610 MiB, which is the target plus
/// enough surplus that qualification must close one object at the target and
/// leave a smaller residue object behind rather than rolling exactly once.
const REQUESTS: usize = 52;

/// AC22 Tier-2 owner: the production default closes a real 512 MiB object.
///
/// Everything else in the suite proves Scribe publishes the right rows. This
/// owner proves it publishes them into the right *physical* shape at the
/// production geometry: staged members accumulate past the 512 MiB staging
/// target, the assembly merges them, and the rolling writer closes one object
/// at or above the target with many row groups in its footer and leaves the
/// surplus as a smaller residue object. The geometry is untouched — a scaled
/// target could roll an object at any size and would prove nothing about the
/// shipped one.
///
/// The evidence is deliberately the durable evidence: the fenced `file_list`
/// row, the promotion record committed with it, and the exact rows a public
/// strict read gets back from the object that was actually uploaded.
///
/// # Panics
///
/// Panics when an append or read fails, when no published object reaches the
/// production target, when qualification leaves no residue, when the object is
/// a single row group, when a promotion record disagrees with the row that
/// names it, or when the read-back is not exactly what was acknowledged.
#[tokio::test]
#[ignore = "requires Postgres and object storage"]
async fn scribe_512_mib_physical_object_qualifies() {
    let server = start_scribe_server().await;
    let tenant = server.data_tenant_id();
    let name = unique_table("qualification");
    let table = register_payload_table(&server, tenant, &name).await;
    let client = tenant_client(&server, tenant).await;

    // One pinned event time keeps every row in one hour partition, so the whole
    // workload forms one assembly key however long the ingest takes.
    let event_time = chrono::Utc::now();
    let mut generator = PayloadGenerator::new(0x5eed_1234_9abc_def1);
    let mut expected: Vec<i64> = Vec::with_capacity(REQUESTS * ROWS_PER_REQUEST);
    for request in 0..REQUESTS {
        let first = (request * ROWS_PER_REQUEST) as i64;
        let values: Vec<i64> = (first..first + ROWS_PER_REQUEST as i64).collect();
        let batch = payload_batch(&values, &mut generator, event_time);
        append_batch(&client, &table, uuid::Uuid::now_v7(), &batch)
            .await
            .unwrap_or_else(|error| panic!("append {request} is acknowledged: {error:?}"));
        expected.extend_from_slice(&values);
    }

    server
        .flush_bifrost_for_tenant(tenant)
        .await
        .expect("the staged members publish");

    let published = server
        .published_hot_files_for_test(tenant, BifrostNamespace::Datasets.as_str(), &name)
        .await
        .expect("published hot files are inspectable");
    assert!(
        published.len() > 1,
        "a workload past the staging target must close a qualifying object and a residue, not {} object(s)",
        published.len()
    );
    let sizes: Vec<u64> = published.iter().map(|file| file.file_size).collect();
    let qualifying = published
        .iter()
        .find(|file| file.file_size >= DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES)
        .unwrap_or_else(|| {
            panic!(
                "no published object reached the production {DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES}-byte staging target; sizes were {sizes:?}"
            )
        });
    assert!(
        published
            .iter()
            .any(|file| file.file_size < DEFAULT_STAGING_TARGET_FILE_SIZE_BYTES),
        "qualification must leave the surplus as a residue object; sizes were {sizes:?}"
    );

    // Footer evidence: the qualifying object is a real multi-row-group object
    // whose split offsets ascend inside the file it was sealed as.
    let data_file = &qualifying.promotion_record.data_file;
    assert!(
        data_file.split_offsets.len() > 1,
        "a qualifying object must carry many row groups, not {}",
        data_file.split_offsets.len()
    );
    assert!(
        data_file
            .split_offsets
            .windows(2)
            .all(|pair| pair[0] < pair[1]),
        "row-group split offsets must ascend: {:?}",
        data_file.split_offsets
    );
    assert!(
        data_file.split_offsets.last().is_some_and(
            |offset| u64::try_from(*offset).is_ok_and(|offset| offset < qualifying.file_size)
        ),
        "every row group must start inside the sealed object"
    );

    // The committed row, the promotion record, and the sealed object are one
    // fact, so any disagreement between them is a publication defect.
    for file in &published {
        let record = &file.promotion_record;
        assert_eq!(
            record.file_list_id, file.id,
            "a promotion record must name the fenced row it was committed with"
        );
        assert_eq!(
            record.object_key, file.object_key,
            "a promotion record must name the object its row points at"
        );
        assert_eq!(
            record.file_checksum, file.file_checksum,
            "a promotion record must carry the checksum the row published"
        );
        assert_eq!(
            record.data_file.file_size_in_bytes, file.file_size,
            "a promotion record must carry the exact sealed object size"
        );
        assert_eq!(
            record.data_file.record_count, file.row_count,
            "a promotion record must carry the exact published row count"
        );
        assert_eq!(
            file.file_checksum.len(),
            64,
            "a published object must carry a SHA-256 checksum"
        );
        assert!(
            file.file_checksum
                .chars()
                .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character)),
            "a published checksum must be lowercase hex: {}",
            file.file_checksum
        );
    }
    let mut checksums: Vec<&str> = published
        .iter()
        .map(|file| file.file_checksum.as_str())
        .collect();
    checksums.sort_unstable();
    let distinct = checksums.len();
    checksums.dedup();
    assert_eq!(
        checksums.len(),
        distinct,
        "two objects holding different rows may not share a checksum"
    );
    let published_rows: u64 = published.iter().map(|file| file.row_count).sum();
    assert_eq!(
        published_rows,
        expected.len() as u64,
        "the published objects must account for every acknowledged row exactly once"
    );

    // Resource evidence: qualification returns every byte it borrowed.
    let settled = server
        .scribe_inspection_snapshot()
        .expect("Scribe ownership is inspectable");
    assert_eq!(
        settled.writable_bucket_count, 0,
        "publication must leave no writable bucket owning published rows"
    );
    assert_eq!(
        settled.immutable_bucket_count, 0,
        "publication must leave no immutable bucket owning published rows"
    );

    // PUT evidence: the rows come back from the objects that were uploaded.
    let mut read_back = read_sql(&client, &format!("SELECT value FROM {table}")).await;
    read_back.sort_unstable();
    assert_eq!(
        read_back.len(),
        expected.len(),
        "a strict read of the qualified objects must return every acknowledged row"
    );
    assert_eq!(
        read_back, expected,
        "a strict read of the qualified objects must return exactly the acknowledged rows"
    );

    server.shutdown().await.expect("the server drains cleanly");
}

/// Registers the two-column payload table this owner qualifies objects for.
///
/// The `value` column is what every read asserts on; the `payload` column is
/// only there to make rows physically large enough that the production staging
/// target is reachable without an unreasonable row count.
async fn register_payload_table(
    server: &WyrdTestServer,
    tenant: DataTenantId,
    name: &str,
) -> String {
    server
        .create_bifrost_table_for_test(vala_bifrost_redux::catalog::CreateTableRequest {
            table: vala_bifrost_redux::catalog::TableRef::new(BifrostNamespace::Datasets, name),
            user_fields: vec![
                arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
                arrow::datatypes::Field::new("payload", arrow::datatypes::DataType::Binary, false),
            ],
            tenant,
            physical_layout: None,
            audit: None,
        })
        .await
        .expect("the catalog registers the payload table");
    format!("{}.{name}", BifrostNamespace::Datasets.as_str())
}

/// Builds one ingest batch of values, incompressible payloads, and event times.
fn payload_batch(
    values: &[i64],
    generator: &mut PayloadGenerator,
    event_time: chrono::DateTime<chrono::Utc>,
) -> arrow::record_batch::RecordBatch {
    let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("value", arrow::datatypes::DataType::Int64, false),
        arrow::datatypes::Field::new("payload", arrow::datatypes::DataType::Binary, false),
        arrow::datatypes::Field::new(
            wyrd_spec::vala::managed_columns::WYRD_EVENT_TIME,
            arrow::datatypes::DataType::Timestamp(
                arrow::datatypes::TimeUnit::Microsecond,
                Some("UTC".into()),
            ),
            false,
        ),
    ]));
    let mut payloads = arrow::array::BinaryBuilder::with_capacity(
        values.len(),
        values.len().saturating_mul(PAYLOAD_BYTES),
    );
    let mut block = [0_u8; PAYLOAD_BYTES];
    for _ in values {
        generator.fill(&mut block);
        payloads.append_value(block);
    }
    arrow::record_batch::RecordBatch::try_new(
        schema,
        vec![
            std::sync::Arc::new(arrow::array::Int64Array::from(values.to_vec())),
            std::sync::Arc::new(payloads.finish()),
            std::sync::Arc::new(
                arrow::array::TimestampMicrosecondArray::from(vec![
                    event_time.timestamp_micros();
                    values.len()
                ])
                .with_timezone("UTC"),
            ),
        ],
    )
    .expect("payload batch")
}

/// Deterministic source of payload bytes Zstd cannot shrink.
///
/// A fixed seed keeps the case reproducible while a xorshift stream keeps the
/// bytes incompressible, so the physical object size the assertions depend on
/// is stable from run to run.
struct PayloadGenerator {
    /// Current xorshift64 state; never zero.
    state: u64,
}

impl PayloadGenerator {
    /// Starts a generator from one fixed seed.
    const fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    /// Fills `block` with the next bytes of the stream.
    fn fill(&mut self, block: &mut [u8; PAYLOAD_BYTES]) {
        for chunk in block.chunks_mut(8) {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            let bytes = self.state.to_le_bytes();
            chunk.copy_from_slice(&bytes[..chunk.len()]);
        }
    }
}
