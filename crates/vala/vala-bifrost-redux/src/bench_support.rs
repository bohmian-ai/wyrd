//! Narrow production seams used by the Scribe benchmark.

use std::path::Path;

use bytes::Bytes;
use wyrd_spec::ids::DataTenantId;
use wyrd_spec::vala::api::QueryTerminalOutcome;

use crate::catalog::TableRef;
use crate::catalog::layout::{TimeGranularity, TimePartition};
use crate::contracts::ScribeError;
use crate::namespaces::BifrostNamespace;
use crate::scribe::seal_key::SealKey;
use crate::scribe::wal::{PreparedWalAppend, WalConfig, WalWriter};

/// Closed Oracle telemetry label domains exported only for benchmark contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OracleTelemetryLabelDomains {
    /// Query class values emitted by production Oracle metrics.
    pub classes: [&'static str; 2],
    /// Admission outcome values emitted by production Oracle metrics.
    pub outcomes: [&'static str; 2],
    /// Admission reason values emitted by production Oracle metrics.
    pub reasons: [&'static str; 5],
    /// Query terminal outcomes emitted by the Oracle stream owner.
    pub terminal_outcomes: [&'static str; 3],
    /// Query cancellation reasons emitted by the Oracle stream owner.
    pub cancellation_reasons: [&'static str; 2],
    /// Fragment terminal outcomes emitted by the Oracle dispatcher.
    pub fragment_outcomes: [&'static str; 2],
}

/// Return the production Oracle closed label domains for projection contracts.
#[must_use]
pub const fn oracle_telemetry_label_domains() -> OracleTelemetryLabelDomains {
    use crate::oracle::telemetry::{
        FragmentOutcome, OracleAdmissionOutcome, OracleAdmissionReason, OracleCancellationReason,
        query_class_label,
    };
    use wyrd_spec::vala::api::QueryClass;

    let outcomes = OracleAdmissionOutcome::ALL;
    let reasons = OracleAdmissionReason::ALL;
    let cancellation_reasons = OracleCancellationReason::ALL;
    let fragment_outcomes = FragmentOutcome::ALL;
    OracleTelemetryLabelDomains {
        classes: [
            query_class_label(QueryClass::Interactive),
            query_class_label(QueryClass::Analytical),
        ],
        outcomes: [outcomes[0].as_str(), outcomes[1].as_str()],
        reasons: [
            reasons[0].as_str(),
            reasons[1].as_str(),
            reasons[2].as_str(),
            reasons[3].as_str(),
            reasons[4].as_str(),
        ],
        terminal_outcomes: [
            query_terminal_outcome_label(QueryTerminalOutcome::Success),
            query_terminal_outcome_label(QueryTerminalOutcome::Degraded),
            query_terminal_outcome_label(QueryTerminalOutcome::Failed),
        ],
        cancellation_reasons: [
            cancellation_reasons[0].as_str(),
            cancellation_reasons[1].as_str(),
        ],
        fragment_outcomes: [fragment_outcomes[0].as_str(), fragment_outcomes[1].as_str()],
    }
}

/// Projects the shared wire terminal enum into the production metric labels.
const fn query_terminal_outcome_label(value: QueryTerminalOutcome) -> &'static str {
    match value {
        QueryTerminalOutcome::Success => "success",
        QueryTerminalOutcome::Degraded => "degraded",
        QueryTerminalOutcome::Failed => "failed",
    }
}

/// A prepared v3 WAL append. Preparation performs the production payload
/// accounting, but no filesystem work.
pub struct PreparedWalAppendFixture {
    append: PreparedWalAppend,
}

impl PreparedWalAppendFixture {
    /// Compute the data payload checksum without touching the filesystem.
    #[must_use]
    pub fn crc32(&self) -> u32 {
        crc32c::crc32c(&self.append.data)
    }
}

/// Result of one production WAL append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalAppendEvidence {
    /// Encoded bytes appended to the WAL.
    pub bytes: u64,
}

/// Evidence from an opportunistic production WAL group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WalGroupEvidence {
    /// Number of prepared frames appended.
    pub frames: u64,
    /// Number of physical segments touched.
    pub segments: u64,
    /// Number of sync calls issued.
    pub fsyncs: u64,
    /// Encoded WAL bytes appended.
    pub bytes: u64,
}

/// Production WAL harness for independent component measurements.
pub struct WalBenchSupport {
    writer: WalWriter,
    tenant: DataTenantId,
}

impl WalBenchSupport {
    /// Create a real WAL writer rooted at `path`.
    pub fn new(path: impl AsRef<Path>, tenant: DataTenantId) -> Result<Self, ScribeError> {
        let node_id = *uuid::Uuid::now_v7().as_bytes();
        Ok(Self {
            writer: WalWriter::new(path, node_id, 1, WalConfig::default())?,
            tenant,
        })
    }

    /// Prepare a v6 data record without filesystem work.
    pub fn prepare(
        &self,
        batch_id: [u8; 16],
        data: &[u8],
    ) -> Result<PreparedWalAppendFixture, ScribeError> {
        let seal_key = SealKey::new(
            self.tenant,
            TableRef::new(BifrostNamespace::Bifrost, "bench"),
            TimePartition::new(TimeGranularity::Day, chrono::DateTime::UNIX_EPOCH).map_err(
                |error| ScribeError::Internal {
                    detail: format!("benchmark WAL epoch partition is invalid: {error}"),
                },
            )?,
        );
        Ok(PreparedWalAppendFixture {
            append: PreparedWalAppend::new(
                crate::scribe::wal::WalLsn::ZERO,
                batch_id,
                Bytes::copy_from_slice(data),
            )
            .for_slice(seal_key, [0; 32]),
        })
    }

    /// Append one prepared record through production WAL IO.
    pub fn append_no_sync(
        &self,
        fixture: PreparedWalAppendFixture,
    ) -> Result<WalAppendEvidence, ScribeError> {
        let result = self.writer.append_prepared(fixture.append)?;
        Ok(WalAppendEvidence {
            bytes: result.encoded_bytes,
        })
    }

    /// Sync the current production WAL segment.
    pub fn sync_alone(&self) -> Result<(), ScribeError> {
        self.writer.sync_data()
    }

    /// Append one prepared frame and sync its production segment.
    pub fn append_and_sync(
        &self,
        fixture: PreparedWalAppendFixture,
    ) -> Result<WalAppendEvidence, ScribeError> {
        let result = self.writer.append_prepared(fixture.append)?;
        WalWriter::sync_segments(&result.touched_segments)?;
        Ok(WalAppendEvidence {
            bytes: result.encoded_bytes,
        })
    }

    /// Append exactly one group and issue one sync per distinct touched segment.
    pub fn append_group_one_sync(
        &self,
        fixtures: Vec<PreparedWalAppendFixture>,
    ) -> Result<WalGroupEvidence, ScribeError> {
        let mut touched_segments = Vec::new();
        let mut bytes = 0_u64;
        let frames = u64::try_from(fixtures.len()).map_err(|_| ScribeError::Internal {
            detail: "benchmark group frame count overflow".to_owned(),
        })?;
        for fixture in fixtures {
            let result = self.writer.append_prepared(fixture.append)?;
            bytes = bytes.saturating_add(result.encoded_bytes);
            touched_segments.extend(result.touched_segments);
        }
        let mut paths = Vec::new();
        for segment in &touched_segments {
            if !paths.iter().any(|path| path == segment.path()) {
                paths.push(segment.path().to_owned());
            }
        }
        let segments = u64::try_from(paths.len()).map_err(|_| ScribeError::Internal {
            detail: "benchmark segment count overflow".to_owned(),
        })?;
        WalWriter::sync_segments(&touched_segments)?;
        Ok(WalGroupEvidence {
            frames,
            segments,
            fsyncs: segments,
            bytes,
        })
    }
}

/// One tenant's Oracle tables over local Parquet files, for the read
/// benchmarks.
///
/// A hot table ([`Self::session`]) is the production [`OracleTableProvider`]
/// over a snapshot-free Iceberg table and one hot file, so a query plans and
/// reads exactly as a leader-local Oracle read of unpublished data does: the
/// Variant SQL surface, the leaf path rule, and the hot per-file reader. A
/// published table ([`Self::publish`], [`Self::published_session`]) is the
/// same provider over a real Iceberg snapshot holding the file and no hot
/// file, so a query plans through Iceberg and reads through the published
/// path. Files must be written under the root with [`Self::writer_properties`],
/// whose footer proves this owner's tenant.
///
/// [`OracleTableProvider`]: crate::oracle::exec::OracleTableProvider
#[cfg(feature = "test-support")]
pub struct OracleBenchTables {
    /// The tenant every footer proves and every query reads as.
    tenant: DataTenantId,
    /// The fixture's storage owner, shared by the hot reader and the catalog.
    storage: std::sync::Arc<crate::storage::BifrostStorage>,
    /// The fixture's warehouse store every hot and published file is read through.
    objects: std::sync::Arc<crate::catalog::iceberg_storage::BifrostIcebergStorage>,
    /// The warehouse root every file must live under.
    root: std::path::PathBuf,
    /// The warehouse URI of `root`, which every published location names.
    warehouse: String,
}

#[cfg(feature = "test-support")]
impl OracleBenchTables {
    /// Creates an owner for a fresh tenant whose test storage lives under the
    /// existing directory `root`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        let storage = crate::storage::BifrostStorage::for_test(root, true);
        let warehouse = format!("file://{}", root.display());
        Self {
            tenant: DataTenantId::new_v7(),
            objects: std::sync::Arc::new(
                crate::catalog::iceberg_storage::BifrostIcebergStorage::new(
                    std::sync::Arc::clone(&storage),
                    &warehouse,
                ),
            ),
            storage,
            root: root.to_path_buf(),
            warehouse,
        }
    }

    /// The Bifrost writer recipe, with a footer proving this owner's tenant and
    /// `row_group_rows` rows per row group.
    #[must_use]
    pub fn writer_properties(
        &self,
        row_group_rows: usize,
    ) -> parquet::file::properties::WriterProperties {
        crate::parquet::writer_properties::bifrost_writer_properties_with_metadata(
            vec![crate::parquet::footer::tenant_key_value(self.tenant)],
            &[],
        )
        .into_builder()
        .set_max_row_group_row_count(Some(row_group_rows))
        .build()
    }

    /// Returns a plain `DataFusion` session with the same configuration an
    /// Oracle session has (see `bench_session_config`) and none of Wyrd's
    /// providers, rules or functions: the native side of a like-for-like
    /// comparison.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test composition has no Oracle capability.
    #[must_use]
    pub fn native_session() -> datafusion::prelude::SessionContext {
        datafusion::prelude::SessionContext::new_with_config(
            crate::oracle::bindings::bench_session_config(),
        )
    }

    /// Returns a production-configured Oracle session (see
    /// `bench_session_config`) with each `(name, logical schema, file)`
    /// registered as a hot table, bound with a live Interactive grant over a
    /// 1 GiB test Oracle composition so its leaves execute without admission.
    ///
    /// The logical schema carries every field's Iceberg field id; a file may
    /// shred its Variants, as hot files do.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test composition has no Oracle capability.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when a schema has no Iceberg form, a file
    /// cannot be stat'ed or lies outside the root, or a provider cannot be
    /// built or registered.
    pub async fn session(
        &self,
        tables: &[(&str, arrow::datatypes::SchemaRef, &Path)],
    ) -> datafusion::error::Result<datafusion::prelude::SessionContext> {
        let context = Self::wyrd_session();
        for (name, schema, path) in tables {
            let table = Self::unpublished_table(name, schema, path)?;
            let provider = self
                .provider(name, table, vec![self.hot_file(path)?])
                .await?;
            context.register_table(*name, std::sync::Arc::new(provider))?;
        }
        Ok(context)
    }

    /// Commits the file at `path` as the only data file of a new Iceberg table
    /// `name` with logical schema `schema`, and returns the committed table.
    ///
    /// The table lives in an iceberg-rust memory catalog over this owner's
    /// Bifrost storage, as production tables live in the Bifrost catalog, and
    /// the data file is described from its own footer with the same Iceberg
    /// call Scribe promotion uses. The returned table reads its metadata
    /// through Bifrost storage; a native reader rebuilds it over its own
    /// `FileIO`.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when the schema has no Iceberg form, the
    /// file cannot be read or lies outside the root, or the catalog refuses the
    /// table or the append.
    pub async fn publish(
        &self,
        name: &str,
        schema: &arrow::datatypes::Schema,
        path: &Path,
    ) -> datafusion::error::Result<iceberg::table::Table> {
        use datafusion::error::DataFusionError;
        use iceberg::memory::{MEMORY_CATALOG_WAREHOUSE, MemoryCatalogBuilder};
        use iceberg::transaction::{ApplyTransactionAction as _, Transaction};
        use iceberg::{Catalog as _, CatalogBuilder as _, NamespaceIdent, TableCreation};

        let external = |error: iceberg::Error| DataFusionError::External(Box::new(error));
        let catalog = MemoryCatalogBuilder::default()
            .with_storage_factory(std::sync::Arc::new(
                crate::catalog::iceberg_storage::BifrostIcebergStorageFactory::new(
                    std::sync::Arc::clone(&self.storage),
                    &self.warehouse,
                ),
            ))
            .load(
                "bench",
                std::collections::HashMap::from([(
                    MEMORY_CATALOG_WAREHOUSE.to_owned(),
                    self.warehouse.clone(),
                )]),
            )
            .await
            .map_err(external)?;
        let namespace = NamespaceIdent::new("bench".to_owned());
        catalog
            .create_namespace(&namespace, std::collections::HashMap::new())
            .await
            .map_err(external)?;
        let creation = TableCreation::builder()
            .name(name.to_owned())
            .schema(iceberg::arrow::arrow_schema_to_schema(schema).map_err(external)?)
            .format_version(iceberg::spec::FormatVersion::V3)
            .build();
        let table = catalog
            .create_table(&namespace, creation)
            .await
            .map_err(external)?;
        let footer = parquet::file::reader::FileReader::metadata(
            &parquet::file::reader::SerializedFileReader::new(std::fs::File::open(path)?)
                .map_err(|error| DataFusionError::External(Box::new(error)))?,
        )
        .clone();
        let data_file = iceberg::writer::file_writer::ParquetWriter::parquet_to_data_file_builder(
            std::sync::Arc::clone(table.metadata().current_schema()),
            std::sync::Arc::new(footer),
            usize::try_from(std::fs::metadata(path)?.len()).unwrap_or(usize::MAX),
            format!("{}/{}", self.warehouse, self.relative(path)?),
            std::collections::HashMap::new(),
        )
        .map_err(external)?
        .build()
        .map_err(|error| DataFusionError::External(Box::new(error)))?;
        let transaction = Transaction::new(&table);
        let append = transaction.fast_append().add_data_files([data_file]);
        append
            .apply(transaction)
            .map_err(external)?
            .commit(&catalog)
            .await
            .map_err(external)
    }

    /// Returns a production-configured Oracle session, bound as
    /// [`Self::session`] binds one, with each `(name, table)` registered over
    /// the table's current snapshot and no hot file, so every row comes through
    /// the published read.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test composition has no Oracle capability.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when a provider cannot be built or
    /// registered.
    pub async fn published_session(
        &self,
        tables: &[(&str, &iceberg::table::Table)],
    ) -> datafusion::error::Result<datafusion::prelude::SessionContext> {
        let context = Self::wyrd_session();
        for (name, table) in tables {
            let provider = self.provider(name, (*table).clone(), Vec::new()).await?;
            context.register_table(*name, std::sync::Arc::new(provider))?;
        }
        Ok(context)
    }

    /// An empty Oracle session: `bench_session_config` with the Variant SQL
    /// surface installed.
    ///
    /// # Panics
    ///
    /// Panics when the fixed test composition has no Oracle capability.
    fn wyrd_session() -> datafusion::prelude::SessionContext {
        datafusion::prelude::SessionContext::new_with_state(
            crate::oracle::variant_sql::OracleVariantSql::shared()
                .session(crate::oracle::bindings::bench_session_config()),
        )
    }

    /// A snapshot-free Iceberg table `name` with logical schema `schema`,
    /// located beside the hot file at `path`.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when the schema has no Iceberg form or the
    /// table cannot be built.
    fn unpublished_table(
        name: &str,
        schema: &arrow::datatypes::Schema,
        path: &Path,
    ) -> datafusion::error::Result<iceberg::table::Table> {
        use iceberg::spec::{FormatVersion, SortOrder, TableMetadataBuilder, UnboundPartitionSpec};

        let external =
            |error: iceberg::Error| datafusion::error::DataFusionError::External(Box::new(error));
        let metadata = TableMetadataBuilder::new(
            iceberg::arrow::arrow_schema_to_schema(schema).map_err(external)?,
            UnboundPartitionSpec::builder().build(),
            SortOrder::unsorted_order(),
            format!("file://{}", path.parent().unwrap_or(path).display()),
            FormatVersion::V3,
            std::collections::HashMap::new(),
        )
        .and_then(TableMetadataBuilder::build)
        .map_err(external)?
        .metadata;
        iceberg::table::Table::builder()
            .file_io(iceberg::io::FileIO::new_with_fs())
            .metadata(metadata)
            .identifier(iceberg::TableIdent::from_strs(["bench", name]).map_err(external)?)
            .runtime(iceberg::Runtime::current())
            .build()
            .map_err(external)
    }

    /// Describes the file at `path` as a hot file, as Scribe's manifest row
    /// would.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when the file cannot be read or lies
    /// outside the root.
    fn hot_file(
        &self,
        path: &Path,
    ) -> datafusion::error::Result<crate::oracle::exec::HotFileSource> {
        let size = std::fs::metadata(path)?.len();
        // Production reads the row count from the durable manifest row; the
        // bench file has no manifest, so its own footer stands in for it.
        let rows = parquet::file::reader::FileReader::metadata(
            &parquet::file::reader::SerializedFileReader::new(std::fs::File::open(path)?)
                .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?,
        )
        .file_metadata()
        .num_rows();
        Ok(crate::oracle::exec::HotFileSource {
            location: self.relative(path)?,
            size_bytes: usize::try_from(size).unwrap_or(usize::MAX),
            row_count: u64::try_from(rows).ok(),
            event_time: crate::catalog::event_time::EventTimeStatistics::Unusable(
                crate::catalog::event_time::EventTimeBoundsDefect::Missing,
            ),
        })
    }

    /// The warehouse key of `path`: its location relative to the root.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when `path` lies outside the root.
    fn relative(&self, path: &Path) -> datafusion::error::Result<String> {
        Ok(path
            .strip_prefix(&self.root)
            .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?
            .to_string_lossy()
            .into_owned())
    }

    /// Builds the production provider for `table` plus `hot_files`, read as
    /// this owner's tenant.
    ///
    /// # Errors
    ///
    /// Returns a `DataFusion` error when the query context or the provider
    /// cannot be built.
    async fn provider(
        &self,
        name: &str,
        table: iceberg::table::Table,
        hot_files: Vec<crate::oracle::exec::HotFileSource>,
    ) -> datafusion::error::Result<crate::oracle::exec::OracleTableProvider> {
        let principal = wyrd_runtime::Principal {
            id: wyrd_spec::auth::PrincipalId::new(uuid::Uuid::now_v7()),
            kind: wyrd_runtime::PrincipalKind::User,
            tenant_id: self.tenant,
            roles: Vec::new(),
            effective_permissions: wyrd_runtime::permission::PermissionSet::default(),
            credential_id: None,
        };
        let context = crate::oracle::AuthorizedQueryContext::try_new(
            principal,
            self.tenant,
            wyrd_spec::request_id::RequestId::now_v7(),
            None,
            wyrd_spec::vala::api::AuthMethod::Internal,
            wyrd_runtime::Permission::bifrost_query_read(),
        )
        .map_err(|error| datafusion::error::DataFusionError::External(Box::new(error)))?;
        crate::oracle::exec::OracleTableProvider::try_new(crate::oracle::exec::OracleTableInputs {
            table,
            objects: std::sync::Arc::clone(&self.objects),
            hot_files,
            context,
            table_name: name.to_owned(),
            live: None,
        })
        .await
    }
}
