//! Tier-2 helpers for driving the managed rewrite seam over real dependencies.
//!
//! Everything here builds on [`PromotionIntegrationFixture`], which owns the
//! production graph. This module adds only what a rewrite scenario needs and
//! promotion did not: a promoted snapshot to rewrite, an exact resource demand
//! derived the way the planner derives it, the delete state Scribe cannot yet
//! produce, and the readers that turn produced objects back into rows.
//!
//! No production behavior is reimplemented. The delete writers are the Iceberg
//! writers the format defines, and the commit that publishes them is an
//! ordinary overwrite through the same catalog every other test uses.

use std::collections::BTreeMap;
use std::sync::Arc;

use iceberg::spec::{
    DataFile, DataFileFormat, PartitionKey, Schema as IcebergSchema, TableProperties,
};
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction as _, Transaction};
use iceberg::writer::base_writer::equality_delete_writer::{
    EqualityDeleteFileWriterBuilder, EqualityDeleteWriterConfig,
};
use iceberg::writer::base_writer::position_delete_file_writer::{
    PositionDeleteFileWriterBuilder, PositionDeleteInput,
};
use iceberg::writer::file_writer::ParquetWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, LocationGenerator,
};
use iceberg::writer::file_writer::rolling_writer::RollingFileWriterBuilder;
use iceberg::writer::{IcebergWriter as _, IcebergWriterBuilder as _};
use iceberg::{Catalog, TableIdent};
use vala_bifrost_redux::forge::{
    Forge, ForgeCapacity, ForgeClock, ForgeEnvelopeSizer, ForgeObjectStore, ForgeRewriteAttempt,
    ForgeSchedulerTrigger, ForgeWorkerCompletionObserver,
};
use vala_bifrost_redux::resources::ForgeRewriteRequest;

use super::support::{CountingObjectStore, PromotionIntegrationFixture, SupervisedPromotion};

/// Writes fixture-authored objects beneath one fixed directory.
///
/// The production location generator resolves `write.data.path`, which points
/// at the Forge recipe directory. Delete files are not rewrite outputs, so they
/// must not land there; this generator keeps them in their own directory while
/// preserving the partition path the spec requires.
#[derive(Clone, Debug)]
pub(crate) struct FixtureLocationGenerator {
    /// Absolute directory every generated location is rooted at.
    root: String,
}

impl FixtureLocationGenerator {
    /// Roots the generator at `{table_location}/data/fixture-deletes`.
    pub(crate) fn new(table: &Table) -> Self {
        Self {
            root: format!(
                "{}/data/fixture-deletes",
                table.metadata().location().trim_end_matches('/')
            ),
        }
    }
}

impl LocationGenerator for FixtureLocationGenerator {
    fn generate_location(&self, partition_key: Option<&PartitionKey>, file_name: &str) -> String {
        match partition_key
            .map(PartitionKey::to_path)
            .filter(|path| !path.is_empty())
        {
            Some(path) => format!("{}/{path}/{file_name}", self.root),
            None => format!("{}/{file_name}", self.root),
        }
    }
}

/// One promoted table plus the seams a managed rewrite scenario drives it with.
///
/// Holds the fixture rather than borrowing it so a scenario is one value: every
/// method below reads the same promoted snapshot the rewrite executes against,
/// which is what makes the before/after assertions comparable.
pub(crate) struct PromotedRewriteFixture {
    /// Production graph, already carrying a promoted `main` snapshot.
    pub(crate) fixture: PromotionIntegrationFixture,
}

impl PromotedRewriteFixture {
    /// Seals two hot objects and promotes them into one real snapshot.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot start or its promotion does not settle.
    pub(crate) async fn start(table_name: &str) -> Self {
        let fixture = PromotionIntegrationFixture::start(table_name).await;
        let object_store = CountingObjectStore::new(Arc::clone(&fixture.staging));
        let mut promotion = SupervisedPromotion::start(
            &fixture,
            fixture.catalog.iceberg_catalog(),
            Arc::clone(&object_store) as Arc<dyn ForgeObjectStore>,
            ForgeClock::system(),
        );
        promotion.run_one_success().await;
        promotion.shutdown().await;
        assert!(
            !fixture.live_data_paths().await.is_empty(),
            "a rewrite scenario starts from a promoted live set"
        );
        Self { fixture }
    }

    /// Identity of the promoted table.
    pub(crate) fn table_ident(&self) -> TableIdent {
        self.fixture.binding.table_ident()
    }

    /// Loads the promoted table through the real catalog.
    ///
    /// # Panics
    ///
    /// Panics when the table cannot be read.
    pub(crate) async fn load_table(&self) -> Table {
        self.fixture
            .catalog
            .iceberg_catalog()
            .load_table(&self.table_ident())
            .await
            .expect("promoted fixture table")
    }

    /// Returns every live data file of the promoted snapshot.
    ///
    /// # Panics
    ///
    /// Panics when the snapshot, its manifest list, or a manifest cannot be read.
    pub(crate) async fn live_data_files(&self) -> Vec<DataFile> {
        let table = self.load_table().await;
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return Vec::new();
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("fixture manifest list");
        let mut files = Vec::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("fixture manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                files.push(entry.data_file().clone());
            }
        }
        files
    }

    /// Builds the exact resource demand the promoted live set justifies.
    ///
    /// Derived through the production [`ForgeEnvelopeSizer`] against the
    /// production [`ForgeCapacity`], so a scenario cannot request capacity the
    /// planner would not have persisted.
    ///
    /// # Panics
    ///
    /// Panics when the configured capacity or the derived envelope is invalid.
    pub(crate) async fn rewrite_request(&self) -> ForgeRewriteRequest {
        let files = self.live_data_files().await;
        let total_bytes = files
            .iter()
            .filter(|file| file.content_type() == iceberg::spec::DataContentType::Data)
            .map(iceberg::spec::DataFile::file_size_in_bytes)
            .sum::<u64>()
            .max(1);
        let capacity =
            ForgeCapacity::try_from(&self.fixture.config).expect("fixture Forge capacity");
        let envelope = ForgeEnvelopeSizer::size(
            total_bytes,
            files.len().max(1),
            self.fixture.config.max_concurrent_reads,
            capacity,
        )
        .expect("fixture rewrite envelope");
        ForgeRewriteRequest {
            envelope,
            memory_bytes: usize::try_from(
                envelope
                    .memory_bytes()
                    .expect("fixture envelope resident total"),
            )
            .unwrap_or(usize::MAX),
            scratch_bytes: envelope
                .scratch_bytes()
                .expect("fixture envelope scratch total"),
            reader_permits: 1,
        }
    }

    /// Builds one production Forge owner with no scheduler and no worker.
    ///
    /// # Panics
    ///
    /// Panics when the fixture cannot compose a validated Forge graph.
    pub(crate) fn forge(
        &self,
        catalog: Arc<dyn Catalog>,
        object_store: Arc<dyn ForgeObjectStore>,
    ) -> Arc<Forge> {
        self.fixture.build_forge_for_test(
            catalog,
            object_store,
            ForgeClock::system(),
            ForgeWorkerCompletionObserver::new(),
            ForgeSchedulerTrigger::with_owner_for_test(uuid::Uuid::now_v7()),
        )
    }

    /// Forms one attempt request against the promoted table.
    ///
    /// # Panics
    ///
    /// Panics when the resource demand cannot be derived.
    pub(crate) async fn attempt(&self, attempt_id: uuid::Uuid) -> ForgeRewriteAttempt<'static> {
        ForgeRewriteAttempt {
            task_id: uuid::Uuid::now_v7(),
            attempt_id,
            request: self.rewrite_request().await,
            bloom_columns: &[],
            previous: None,
            cancel: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Publishes delete files against the promoted snapshot in one commit.
    ///
    /// `position` names a row of `target_path` to delete when it is supplied;
    /// `equality_value` names a `value` every partition row carrying it is
    /// deleted for. Both are written with the Iceberg format writers and
    /// published through the real catalog as an overwrite, so the core reads
    /// exactly the delete state a production writer would leave.
    ///
    /// # Panics
    ///
    /// Panics when the delete files cannot be written or the commit is refused.
    pub(crate) async fn publish_deletes(
        &self,
        target_path: &str,
        position: Option<i64>,
        equality_value: Option<i64>,
    ) {
        let table = self.load_table().await;
        let schema = table.metadata().current_schema();
        let spec = table.metadata().default_partition_spec().as_ref().clone();
        let partition = self
            .live_data_files()
            .await
            .into_iter()
            .find(|file| file.file_path() == target_path)
            .expect("the delete target is a live data file")
            .partition()
            .clone();
        let partition_key = PartitionKey::new(spec, Arc::clone(schema), partition);

        let mut deletes = Vec::new();
        if let Some(position) = position {
            deletes.push(
                self.write_position_delete(&table, &partition_key, target_path, position)
                    .await,
            );
        }
        if let Some(equality_value) = equality_value {
            deletes.push(
                self.write_equality_delete(&table, &partition_key, schema, equality_value)
                    .await,
            );
        }
        assert!(!deletes.is_empty(), "a delete commit publishes something");

        let committed = Transaction::new(&table)
            .overwrite_files()
            .add_data_files(deletes)
            .apply(Transaction::new(&table))
            .expect("fixture delete overwrite action")
            .commit(self.fixture.catalog.iceberg_catalog().as_ref())
            .await
            .expect("fixture delete commit");
        drop(committed);
    }

    /// Writes one position-delete file naming a single row of one data object.
    ///
    /// # Panics
    ///
    /// Panics when the writer cannot be built, written, or closed to exactly
    /// one delete file.
    async fn write_position_delete(
        &self,
        table: &Table,
        partition_key: &PartitionKey,
        target_path: &str,
        position: i64,
    ) -> DataFile {
        let rolling = RollingFileWriterBuilder::new(
            ParquetWriterBuilder::new(
                parquet::file::properties::WriterProperties::builder().build(),
                Arc::new(position_delete_schema()),
            ),
            TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES_DEFAULT,
            table.file_io().clone(),
            FixtureLocationGenerator::new(table),
            DefaultFileNameGenerator::new(
                "fixture-pos-delete".to_owned(),
                Some(uuid::Uuid::now_v7().to_string()),
                DataFileFormat::Parquet,
            ),
        );
        let mut writer = PositionDeleteFileWriterBuilder::new(rolling)
            .build(Some(partition_key.clone()))
            .await
            .expect("fixture position delete writer");
        writer
            .write(vec![PositionDeleteInput::new(
                Arc::from(target_path),
                position,
            )])
            .await
            .expect("fixture position delete row");
        let mut files = writer.close().await.expect("fixture position delete close");
        assert_eq!(files.len(), 1, "one row produces one delete file");
        files.remove(0)
    }

    /// Writes one equality-delete file matching on the user `value` column.
    ///
    /// # Panics
    ///
    /// Panics when the column is absent, or the writer cannot be built,
    /// written, or closed to exactly one delete file.
    async fn write_equality_delete(
        &self,
        table: &Table,
        partition_key: &PartitionKey,
        schema: &Arc<IcebergSchema>,
        equality_value: i64,
    ) -> DataFile {
        let value_id = schema
            .field_by_name("value")
            .expect("the fixture table carries a user value column")
            .id;
        let config = EqualityDeleteWriterConfig::new(vec![value_id], Arc::clone(schema))
            .expect("fixture equality delete config");
        let rolling = RollingFileWriterBuilder::new(
            ParquetWriterBuilder::new(
                parquet::file::properties::WriterProperties::builder().build(),
                Arc::new(
                    iceberg::arrow::arrow_schema_to_schema(config.projected_arrow_schema_ref())
                        .expect("fixture equality delete schema"),
                ),
            ),
            TableProperties::PROPERTY_WRITE_TARGET_FILE_SIZE_BYTES_DEFAULT,
            table.file_io().clone(),
            FixtureLocationGenerator::new(table),
            DefaultFileNameGenerator::new(
                "fixture-eq-delete".to_owned(),
                Some(uuid::Uuid::now_v7().to_string()),
                DataFileFormat::Parquet,
            ),
        );
        let mut writer = EqualityDeleteFileWriterBuilder::new(rolling, config.clone())
            .build(Some(partition_key.clone()))
            .await
            .expect("fixture equality delete writer");
        let batch = arrow::array::RecordBatch::try_new(
            Arc::clone(config.projected_arrow_schema_ref()),
            vec![Arc::new(arrow::array::Int64Array::from(vec![
                equality_value,
            ]))],
        )
        .expect("fixture equality delete batch");
        writer.write(batch).await.expect("fixture equality row");
        let mut files = writer.close().await.expect("fixture equality delete close");
        assert_eq!(files.len(), 1, "one row produces one delete file");
        files.remove(0)
    }

    /// Reads the `value` column of every named object, in ascending order.
    ///
    /// Produced rewrite outputs belong to no snapshot, so they can only be read
    /// as objects. Reading them directly is what makes a delete-application
    /// assertion a statement about rows rather than about file counts.
    ///
    /// # Panics
    ///
    /// Panics when an object cannot be read or does not carry the column.
    pub(crate) async fn object_values(&self, paths: &[String]) -> Vec<i64> {
        let mut values = self.object_row_values(paths).await;
        values.sort_unstable();
        values
    }

    /// Reads the `value` column of every named object in physical row order.
    ///
    /// A position delete names a row by its ordinal inside one data object, so
    /// proving the delete landed on the referenced row requires the values in
    /// the order the object stores them rather than sorted.
    ///
    /// # Panics
    ///
    /// Panics when an object cannot be read or does not carry the column.
    pub(crate) async fn object_row_values(&self, paths: &[String]) -> Vec<i64> {
        let mut values = Vec::new();
        for path in paths {
            let key = path
                .split_once(&format!("{}/", self.fixture.binding.object_prefix))
                .map_or_else(
                    || path.clone(),
                    |(_, suffix)| format!("{}/{suffix}", self.fixture.binding.object_prefix),
                );
            let bytes = self
                .fixture
                .staging
                .read(&key)
                .await
                .expect("produced object read")
                .to_bytes();
            let reader =
                parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(bytes)
                    .expect("produced object is Parquet")
                    .build()
                    .expect("produced object reader");
            for batch in reader {
                let batch = batch.expect("produced object batch");
                let column = batch
                    .column_by_name("value")
                    .expect("produced object carries the user column");
                let column = column
                    .as_any()
                    .downcast_ref::<arrow::array::Int64Array>()
                    .expect("the user column stays Int64");
                values.extend(column.iter().flatten());
            }
        }
        values
    }

    /// Snapshots every object under the table prefix with its content hash.
    ///
    /// # Panics
    ///
    /// Panics when the staging operator cannot be listed or read.
    pub(crate) async fn object_digests(&self) -> BTreeMap<String, String> {
        self.fixture.object_digests().await
    }
}

/// The two-field schema every Iceberg position-delete file carries.
///
/// Rebuilt here because the format's own constant is private to the writer;
/// the field ids are the reserved ones the specification fixes, so this cannot
/// drift from what the reader expects.
fn position_delete_schema() -> IcebergSchema {
    use iceberg::spec::{NestedField, PrimitiveType, Type};
    IcebergSchema::builder()
        .with_fields(vec![
            Arc::new(NestedField::required(
                2_147_483_546,
                "file_path",
                Type::Primitive(PrimitiveType::String),
            )),
            Arc::new(NestedField::required(
                2_147_483_545,
                "pos",
                Type::Primitive(PrimitiveType::Long),
            )),
        ])
        .build()
        .expect("the reserved position delete schema is well-formed")
}
