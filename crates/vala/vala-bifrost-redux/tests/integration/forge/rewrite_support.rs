//! Tier-2 helpers for driving the managed rewrite seam over real dependencies.
//!
//! Everything here builds on [`PromotionIntegrationFixture`], which owns the
//! production graph. This module adds only what a rewrite scenario needs and
//! promotion did not: a promoted snapshot to rewrite, a driver that runs one
//! attempt across every plan the core returned, the delete state Scribe cannot
//! yet produce, and the readers that turn produced objects back into rows.
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
    Forge, ForgeClock, ForgeError, ForgeManagedRewrite, ForgeObjectStore, ForgeRewriteEvidence,
    ForgeSchedulerTrigger, ForgeWorkerCompletionObserver, RewriteHandoff,
};

use super::support::{
    CountingObjectStore, PromotionCatalogSeam, PromotionIntegrationFixture, SupervisedPromotion,
};

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

    /// Compose the same fixture without publishing the sealed objects.
    ///
    /// [`Self::start`] runs its own supervised promotion, which leaves Forge's
    /// TTL-bound singleton planning fence held by an owner that no longer
    /// exists — so a scenario that afterwards wants to drive the production
    /// scheduler itself would stand by and plan nothing. This constructor hands
    /// the table over unpublished so one supervisor can own the promotion, the
    /// rewrite, and every pass in between.
    pub(crate) async fn start_unpromoted(table_name: &str) -> Self {
        Self {
            fixture: PromotionIntegrationFixture::start(table_name).await,
        }
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

    /// Returns how many snapshots the table's metadata retains.
    ///
    /// One rewrite attempt publishes one snapshot per admitted plan, so the
    /// delta across a run is the count a per-plan assertion compares its
    /// observed catalog commits against.
    ///
    /// # Panics
    ///
    /// Panics when the table cannot be loaded.
    pub(crate) async fn snapshot_count(&self) -> usize {
        self.load_table().await.metadata().snapshots().count()
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

    /// Returns the data sequence number every live file of the current
    /// snapshot carries, keyed by object path.
    ///
    /// Sequence numbers are the only thing that decides whether a delete still
    /// reaches a file, so a scenario proving delete correctness has to be able
    /// to read them rather than infer them from the file set.
    ///
    /// # Panics
    ///
    /// Panics when the snapshot, its manifest list, or a manifest cannot be
    /// read, or when a live entry carries no data sequence number.
    pub(crate) async fn live_file_sequences(&self) -> BTreeMap<String, i64> {
        let table = self.load_table().await;
        let Some(snapshot) = table.metadata().current_snapshot() else {
            return BTreeMap::new();
        };
        let manifests = table
            .manifest_list_reader(snapshot)
            .load()
            .await
            .expect("fixture manifest list");
        let mut sequences = BTreeMap::new();
        for manifest_file in manifests.entries() {
            let manifest = manifest_file
                .load_manifest(table.file_io())
                .await
                .expect("fixture manifest");
            for entry in manifest.entries().iter().filter(|entry| entry.is_alive()) {
                sequences.insert(
                    entry.data_file().file_path().to_owned(),
                    entry
                        .sequence_number()
                        .expect("a live entry carries its data sequence number"),
                );
            }
        }
        sequences
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

    /// Runs one whole attempt: plans once, then rewrites every plan in turn.
    ///
    /// Every ordinary plan is rewritten independently, which is what the worker
    /// does, so a scenario observes the same per-plan handoffs the publication
    /// path receives. Execution stops at the first plan that fails, and the
    /// attempt handle is returned either way so a scenario can read the
    /// attempt-global possible-output set after a drain or a failure.
    pub(crate) async fn run_attempt(
        &self,
        forge: &Arc<Forge>,
        attempt_id: uuid::Uuid,
        cancel: tokio_util::sync::CancellationToken,
    ) -> AttemptRun {
        let rewrite = forge
            .managed_rewrite(
                &self.fixture.binding,
                uuid::Uuid::now_v7(),
                attempt_id,
                &cancel,
            )
            .expect("the attempt context builds");
        let planned = match rewrite.plan().await {
            Ok(planned) => planned,
            Err(failure) => {
                return AttemptRun {
                    rewrite,
                    evidence: None,
                    handoffs: Vec::new(),
                    failure: Some(failure),
                };
            }
        };
        let evidence = planned.evidence.clone();
        let mut handoffs = Vec::with_capacity(planned.plans.len());
        let mut failure = None;
        for plan in planned.plans {
            match rewrite.rewrite_plan(plan, &planned.table).await {
                Ok(handoff) => handoffs.push(handoff),
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        AttemptRun {
            rewrite,
            evidence: Some(evidence),
            handoffs,
            failure,
        }
    }

    /// Builds a catalog seam whose tables read and write through one break.
    ///
    /// The catalog itself is the real one and every commit is still counted, so
    /// a scenario using this seam proves both what the broken output did and
    /// that no publication followed it.
    pub(crate) fn breaking_catalog(
        &self,
        breakage: RewriteOutputBreak,
    ) -> (Arc<PromotionCatalogSeam>, Arc<RewriteOutputSeam>) {
        let store = RewriteOutputSeam::new(self.fixture.catalog.file_io(), breakage);
        let catalog = PromotionCatalogSeam::new(
            self.fixture.catalog.iceberg_catalog(),
            Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        );
        catalog.intercept_file_io(
            iceberg::io::FileIOBuilder::new(Arc::new(RewriteSeamFactory {
                storage: Arc::clone(&store) as Arc<dyn iceberg::io::Storage>,
            }))
            .build(),
        );
        (catalog, store)
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

/// The one message every refused serialization of the rewrite seam reports.
///
/// The seam holds a live `FileIO` and a cancellation token, neither of which
/// can travel across a process boundary, so both directions of Iceberg's
/// `typetag` contract fail locally rather than reconstructing a second backend.
const SEAM_NOT_PORTABLE: &str =
    "the Forge rewrite output seam is bound to one test process and cannot be serialized";

/// What the seam does to the output whose ordinal it was armed for.
///
/// Both variants act at a real boundary the managed core drives: an output is
/// opened, or an opened output is closed. Neither fabricates a state the core
/// could not reach on its own, which is what keeps the resulting outcome the
/// production one rather than an injected shape.
#[derive(Debug, Clone)]
pub(crate) enum RewriteOutputBreak {
    /// Cancels `token` as the `ordinal`-th rewrite output opens.
    ///
    /// The open itself still succeeds. The core decides cancellation after it
    /// drains its writers, so the plan in flight finishes and reports the
    /// attempt-global output set, which is exactly the drain a shutdown
    /// reaching a running attempt produces.
    CancelAtOpen {
        /// One-based open order of the output cancellation lands on.
        ordinal: usize,
        /// Attempt cancellation boundary this seam trips.
        token: tokio_util::sync::CancellationToken,
    },
    /// Fails the close of the `ordinal`-th rewrite output.
    ///
    /// The open is recorded first, so the failed attempt carries that output as
    /// possibly-existing alongside every object an earlier plan settled.
    FailAtClose {
        /// One-based open order of the output whose close is refused.
        ordinal: usize,
    },
}

/// Storage adapter that delegates every operation and can break one output.
///
/// Delegation goes through a real [`FileIO`] rather than a second backend
/// client, so paths, relativization, and byte handling stay the production
/// ones. Only rewrite outputs — objects beneath the Forge recipe marker — are
/// counted and eligible to be broken; manifests, metadata, and fixture-authored
/// delete files pass through untouched.
pub(crate) struct RewriteOutputSeam {
    /// Real adapter every operation is delegated to.
    inner: iceberg::io::FileIO,
    /// Self-reference handed to the `InputFile`/`OutputFile` values it mints.
    ///
    /// Held weakly because those values own an `Arc<dyn Storage>` back to this
    /// seam; a strong self-reference would make the seam immortal.
    me: std::sync::Weak<Self>,
    /// The single break this seam is armed with.
    breakage: RewriteOutputBreak,
    /// Rewrite outputs opened so far, which is the break's ordinal space.
    opened: std::sync::atomic::AtomicUsize,
}

impl std::fmt::Debug for RewriteOutputSeam {
    /// Reports the arming and the progress against it, not the adapter.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RewriteOutputSeam")
            .field("breakage", &self.breakage)
            .field("opened", &self.opened_outputs())
            .finish_non_exhaustive()
    }
}

impl RewriteOutputSeam {
    /// Arms one seam over `inner` with exactly one break.
    pub(crate) fn new(inner: iceberg::io::FileIO, breakage: RewriteOutputBreak) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            inner,
            me: me.clone(),
            breakage,
            opened: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// Returns how many rewrite outputs were opened through this seam.
    pub(crate) fn opened_outputs(&self) -> usize {
        self.opened.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Returns this seam as the storage its own files delegate back to.
    ///
    /// # Panics
    ///
    /// Panics only if called after the seam was dropped, which cannot happen
    /// while one of its own methods is running.
    fn shared(&self) -> Arc<dyn iceberg::io::Storage> {
        self.me
            .upgrade()
            .expect("invariant: the rewrite seam outlives its own calls")
    }

    /// Returns whether `path` names a rewrite output rather than any other object.
    fn is_rewrite_output(path: &str) -> bool {
        path.contains(vala_bifrost_redux::catalog::layout::FORGE_DATA_MARKER)
    }

    /// Counts one rewrite output and applies the break when its turn arrives.
    ///
    /// Returns whether the opened writer must refuse its own close.
    fn arm_output(&self, path: &str) -> bool {
        if !Self::is_rewrite_output(path) {
            return false;
        }
        let opened = self
            .opened
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        match &self.breakage {
            RewriteOutputBreak::CancelAtOpen { ordinal, token } if opened == *ordinal => {
                token.cancel();
                false
            }
            RewriteOutputBreak::FailAtClose { ordinal } => opened == *ordinal,
            RewriteOutputBreak::CancelAtOpen { .. } => false,
        }
    }
}

impl serde::Serialize for RewriteOutputSeam {
    /// Always refuses: the seam names live process resources.
    ///
    /// # Errors
    ///
    /// Always returns [`SEAM_NOT_PORTABLE`].
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(<S::Error as serde::ser::Error>::custom(SEAM_NOT_PORTABLE))
    }
}

impl<'de> serde::Deserialize<'de> for RewriteOutputSeam {
    /// Always refuses: a reconstructed seam would name a different backend.
    ///
    /// # Errors
    ///
    /// Always returns [`SEAM_NOT_PORTABLE`].
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(<D::Error as serde::de::Error>::custom(SEAM_NOT_PORTABLE))
    }
}

#[async_trait::async_trait]
#[typetag::serde]
impl iceberg::io::Storage for RewriteOutputSeam {
    /// Delegates the existence check unchanged.
    async fn exists(&self, path: &str) -> iceberg::Result<bool> {
        self.inner.exists(path).await
    }

    /// Delegates metadata through the real adapter's own input file.
    async fn metadata(&self, path: &str) -> iceberg::Result<iceberg::io::FileMetadata> {
        self.inner.new_input(path)?.metadata().await
    }

    /// Delegates a whole-object read unchanged.
    async fn read(&self, path: &str) -> iceberg::Result<bytes::Bytes> {
        self.inner.new_input(path)?.read().await
    }

    /// Delegates continuous reading unchanged.
    async fn reader(&self, path: &str) -> iceberg::Result<Box<dyn iceberg::io::FileRead>> {
        self.inner.new_input(path)?.reader().await
    }

    /// Delegates a one-shot write, counting it when it names a rewrite output.
    ///
    /// A one-shot write has no separate close, so a seam armed for
    /// [`RewriteOutputBreak::FailAtClose`] refuses the write itself.
    async fn write(&self, path: &str, bs: bytes::Bytes) -> iceberg::Result<()> {
        if self.arm_output(path) {
            return Err(iceberg::Error::new(
                iceberg::ErrorKind::Unexpected,
                format!("injected Forge rewrite output failure at {path}"),
            ));
        }
        self.inner.new_output(path)?.write(bs).await
    }

    /// Opens one writer, counting it when it names a rewrite output.
    async fn writer(&self, path: &str) -> iceberg::Result<Box<dyn iceberg::io::FileWrite>> {
        let refuse_close = self.arm_output(path);
        let writer = self.inner.new_output(path)?.writer().await?;
        if refuse_close {
            return Ok(Box::new(RefusingCloseWriter {
                inner: writer,
                path: path.to_owned(),
            }));
        }
        Ok(writer)
    }

    /// Delegates a single delete unchanged.
    async fn delete(&self, path: &str) -> iceberg::Result<()> {
        self.inner.delete(path).await
    }

    /// Delegates a prefix delete unchanged.
    async fn delete_prefix(&self, path: &str) -> iceberg::Result<()> {
        self.inner.delete_prefix(path).await
    }

    /// Delegates a streamed delete unchanged.
    async fn delete_stream(
        &self,
        paths: futures_util::stream::BoxStream<'static, String>,
    ) -> iceberg::Result<()> {
        self.inner.delete_stream(paths).await
    }

    /// Delegates listing unchanged.
    async fn list(
        &self,
        path: &str,
        recursive: bool,
    ) -> iceberg::Result<
        futures_util::stream::BoxStream<'static, iceberg::Result<iceberg::io::ListEntry>>,
    > {
        self.inner.list(path, recursive).await
    }

    /// Mints an input file that reads back through this seam.
    fn new_input(&self, path: &str) -> iceberg::Result<iceberg::io::InputFile> {
        Ok(iceberg::io::InputFile::new(self.shared(), path.to_owned()))
    }

    /// Mints an output file that writes back through this seam.
    fn new_output(&self, path: &str) -> iceberg::Result<iceberg::io::OutputFile> {
        Ok(iceberg::io::OutputFile::new(self.shared(), path.to_owned()))
    }
}

/// A real writer whose close is refused after every byte was accepted.
///
/// Modeled on the failure that actually threatens reclamation: the object was
/// opened, its identity was reserved and reported, and only the settlement is
/// lost. A writer that refused its first byte would never have been reported at
/// all and so would prove nothing about the cumulative set.
struct RefusingCloseWriter {
    /// The real writer every accepted byte still reaches.
    inner: Box<dyn iceberg::io::FileWrite>,
    /// Path named in the injected refusal, for a readable failure.
    path: String,
}

#[async_trait::async_trait]
impl iceberg::io::FileWrite for RefusingCloseWriter {
    /// Accepts bytes exactly as the real writer would.
    ///
    /// # Errors
    ///
    /// Propagates the delegated writer's own failure.
    async fn write(&mut self, bs: bytes::Bytes) -> iceberg::Result<()> {
        self.inner.write(bs).await
    }

    /// Refuses settlement after the object was opened and written.
    ///
    /// # Errors
    ///
    /// Always returns an injected `Unexpected` failure naming the object.
    async fn close(&mut self) -> iceberg::Result<()> {
        Err(iceberg::Error::new(
            iceberg::ErrorKind::Unexpected,
            format!(
                "injected Forge rewrite output close failure at {}",
                self.path
            ),
        ))
    }
}

/// Storage factory that yields one already-armed rewrite seam.
///
/// Iceberg builds storage lazily through a factory, so the seam has to arrive
/// wrapped in one. There is nothing to configure: the seam it hands back is the
/// exact instance the scenario armed and still holds a handle to.
#[derive(Debug)]
struct RewriteSeamFactory {
    /// The one armed seam every build returns.
    storage: Arc<dyn iceberg::io::Storage>,
}

impl serde::Serialize for RewriteSeamFactory {
    /// Always refuses, for the same reason the seam itself does.
    ///
    /// # Errors
    ///
    /// Always returns [`SEAM_NOT_PORTABLE`].
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(<S::Error as serde::ser::Error>::custom(SEAM_NOT_PORTABLE))
    }
}

impl<'de> serde::Deserialize<'de> for RewriteSeamFactory {
    /// Always refuses, for the same reason the seam itself does.
    ///
    /// # Errors
    ///
    /// Always returns [`SEAM_NOT_PORTABLE`].
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(<D::Error as serde::de::Error>::custom(SEAM_NOT_PORTABLE))
    }
}

#[typetag::serde]
impl iceberg::io::StorageFactory for RewriteSeamFactory {
    /// Returns the armed seam, ignoring the configuration entirely.
    ///
    /// # Errors
    ///
    /// Never: the seam is already constructed.
    fn build(
        &self,
        _config: &iceberg::io::StorageConfig,
    ) -> iceberg::Result<Arc<dyn iceberg::io::Storage>> {
        Ok(Arc::clone(&self.storage))
    }
}

/// One whole attempt's per-plan results and the attempt handle behind them.
///
/// Kept as a struct rather than a tuple because a drained or failed attempt is
/// still read for its attempt-global possible-output set, which only the
/// retained handle can answer.
pub(crate) struct AttemptRun {
    /// The attempt handle every plan of this run executed under.
    pub(crate) rewrite: ForgeManagedRewrite,
    /// Selection evidence, absent only when planning itself failed.
    pub(crate) evidence: Option<ForgeRewriteEvidence>,
    /// One handoff per plan that completed, in planner order.
    pub(crate) handoffs: Vec<RewriteHandoff>,
    /// The failure that stopped the run, if one did.
    pub(crate) failure: Option<ForgeError>,
}

impl AttemptRun {
    /// Returns the selection evidence of a run that reached execution.
    ///
    /// # Panics
    /// Panics when planning failed, which no caller of this accessor expects.
    pub(crate) fn evidence(&self) -> &ForgeRewriteEvidence {
        self.evidence.as_ref().expect("the attempt planned")
    }

    /// Returns every input path consumed across the run's plans, in order.
    pub(crate) fn rewritten_data_files(&self) -> Vec<String> {
        self.handoffs
            .iter()
            .flat_map(|handoff| handoff.rewritten_data_files.iter().cloned())
            .collect()
    }

    /// Returns every produced object path across the run's plans, in order.
    pub(crate) fn output_paths(&self) -> Vec<String> {
        self.handoffs
            .iter()
            .flat_map(|handoff| {
                handoff
                    .output_data_files
                    .iter()
                    .map(|file| file.file_path().to_owned())
            })
            .collect()
    }

    /// Returns every produced object descriptor across the run's plans.
    pub(crate) fn output_data_files(&self) -> Vec<&DataFile> {
        self.handoffs
            .iter()
            .flat_map(|handoff| handoff.output_data_files.iter())
            .collect()
    }

    /// Returns every position-delete path materialized across the run.
    pub(crate) fn applied_position_delete_files(&self) -> Vec<String> {
        self.handoffs
            .iter()
            .flat_map(|handoff| handoff.applied_position_delete_files.iter().cloned())
            .collect()
    }

    /// Returns every equality-delete path materialized across the run.
    pub(crate) fn applied_equality_delete_files(&self) -> Vec<String> {
        self.handoffs
            .iter()
            .flat_map(|handoff| handoff.applied_equality_delete_files.iter().cloned())
            .collect()
    }
}
