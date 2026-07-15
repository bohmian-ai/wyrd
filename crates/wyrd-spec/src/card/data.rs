//! Data Card spec and validation helpers.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::card::field::FieldSpec;
use crate::ids::{ColumnName, QueryName, SplitName};
use crate::reference::CardRef;

/// Locked DataCard validation rules.
pub mod validate;

use validate::DataCardError;

/// Pure-data contract for a Data Card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DataSpec {
    /// Declared interface family and interface-specific metadata.
    pub interface: DataInterface,
    /// Ordered schema fields for tabular interfaces.
    pub schema: DataSchema,
    /// Durable Artifact card references linked to this data card.
    #[serde(default)]
    pub card_refs: Vec<CardRef>,
    /// Declared split strategies by stable split label.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub splits: HashMap<SplitName, DataSplit>,
    /// Target columns for supervised workflows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_columns: Vec<ColumnName>,
    /// Optional SQL query bundle for SQL-backed data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sql: Option<SqlLogic>,
    /// Local or durable byte statistics captured for this data source.
    pub stats: DataStats,
}

impl DataSpec {
    /// Re-run locked validation invariants on this spec.
    ///
    /// # Errors
    /// Returns a [`DataCardError`] when any locked DataCard invariant fails.
    pub fn validate(&self) -> Result<(), DataCardError> {
        validate::validate_data_spec(self)
    }

    /// Return the stable interface kind string used by Python, MCP, and docs.
    #[must_use]
    pub fn interface_kind(&self) -> &'static str {
        self.interface.kind()
    }

    /// Return true when the interface represents tabular data that requires schema columns.
    #[must_use]
    pub fn is_tabular(&self) -> bool {
        self.interface.requires_schema_columns()
    }

    /// Iterate durable Artifact card references linked to this data card.
    pub fn card_refs(&self) -> impl Iterator<Item = &CardRef> {
        self.card_refs.iter()
    }

    /// Return the declared local/storage stats captured by the interface.
    #[must_use]
    pub fn stats(&self) -> &DataStats {
        &self.stats
    }

    /// Iterate target columns without exposing the vector for mutation.
    pub fn target_columns(&self) -> impl Iterator<Item = &ColumnName> {
        self.target_columns.iter()
    }

    /// Look up a split by label.
    #[must_use]
    pub fn split(&self, label: &SplitName) -> Option<&DataSplit> {
        self.splits.get(label)
    }

    /// Iterate materialized Artifact card refs declared by split strategies.
    pub fn materialized_split_refs(&self) -> impl Iterator<Item = &CardRef> {
        self.splits.values().filter_map(DataSplit::card_ref)
    }
}

/// Data source interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "meta")]
pub enum DataInterface {
    /// Pandas dataframe saved as parquet by default.
    Pandas(PandasMeta),
    /// Polars dataframe saved as parquet by default.
    Polars(PolarsMeta),
    /// PyArrow table saved as IPC or parquet.
    Arrow(ArrowMeta),
    /// Existing parquet dataset or file.
    Parquet(ParquetMeta),
    /// NumPy array saved as NPY or NPZ.
    Numpy(NumpyMeta),
    /// Torch tensor saved as safetensors or explicit pickle.
    Torch(TorchMeta),
    /// SQL query bundle.
    Sql(SqlMeta),
    /// JSON Lines data.
    Jsonl(JsonlMeta),
    /// Image manifest data.
    Image(ImageMeta),
    /// Text manifest data.
    Text(TextMeta),
    /// Hugging Face dataset pointer or local dataset.
    Huggingface(HuggingfaceMeta),
    /// Custom Python data loader.
    Custom(CustomDataMeta),
}

impl DataInterface {
    /// Stable variant name: Pandas, Polars, Arrow, Parquet, Numpy, Torch, Sql,
    /// Jsonl, Image, Text, Huggingface, or Custom.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Pandas(_) => "Pandas",
            Self::Polars(_) => "Polars",
            Self::Arrow(_) => "Arrow",
            Self::Parquet(_) => "Parquet",
            Self::Numpy(_) => "Numpy",
            Self::Torch(_) => "Torch",
            Self::Sql(_) => "Sql",
            Self::Jsonl(_) => "Jsonl",
            Self::Image(_) => "Image",
            Self::Text(_) => "Text",
            Self::Huggingface(_) => "Huggingface",
            Self::Custom(_) => "Custom",
        }
    }

    /// Whether `DataSchema.columns` must be non-empty for this interface.
    #[must_use]
    pub const fn requires_schema_columns(&self) -> bool {
        matches!(
            self,
            Self::Pandas(_) | Self::Polars(_) | Self::Arrow(_) | Self::Parquet(_) | Self::Jsonl(_)
        )
    }

    /// Whether `DataSpec.sql` must be present and non-empty.
    #[must_use]
    pub const fn requires_sql_logic(&self) -> bool {
        matches!(self, Self::Sql(_))
    }

    /// Default media type produced by the Python save path for this interface.
    #[must_use]
    pub const fn default_media_type(&self) -> &'static str {
        match self {
            Self::Pandas(_) | Self::Polars(_) | Self::Arrow(_) | Self::Parquet(_) => {
                "application/x-parquet"
            }
            Self::Numpy(_) => "application/x-npy",
            Self::Torch(_) => "application/vnd.safetensors",
            Self::Sql(_) => "application/vnd.wyrd.sql+json",
            Self::Jsonl(_) => "application/jsonl",
            Self::Image(_) | Self::Text(_) => "application/json",
            Self::Huggingface(_) => "application/vnd.wyrd.huggingface+json",
            Self::Custom(_) => "application/octet-stream",
        }
    }

    /// Default file extension produced by the Python save path.
    #[must_use]
    pub const fn default_extension(&self) -> &'static str {
        match self {
            Self::Pandas(_) | Self::Polars(_) | Self::Arrow(_) | Self::Parquet(_) => "parquet",
            Self::Numpy(_) => "npy",
            Self::Torch(_) => "safetensors",
            Self::Sql(_) | Self::Huggingface(_) => "json",
            Self::Jsonl(_) => "jsonl",
            Self::Image(_) | Self::Text(_) => "manifest.json",
            Self::Custom(_) => "bin",
        }
    }

    /// Return image/text manifest refs when the interface carries one.
    #[must_use]
    pub fn manifest_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Image(meta) => meta.manifest_ref.as_ref(),
            Self::Text(meta) => meta.manifest_ref.as_ref(),
            _ => None,
        }
    }

    /// Human-readable loader family used in docs, errors, and MCP output.
    #[must_use]
    pub const fn loader_family(&self) -> &'static str {
        match self {
            Self::Pandas(_) => "pandas",
            Self::Polars(_) => "polars",
            Self::Arrow(_) => "pyarrow",
            Self::Parquet(_) => "parquet",
            Self::Numpy(_) => "numpy",
            Self::Torch(_) => "torch",
            Self::Sql(_) => "sql",
            Self::Jsonl(_) => "jsonl",
            Self::Image(_) => "image-manifest",
            Self::Text(_) => "text-manifest",
            Self::Huggingface(_) => "huggingface-datasets",
            Self::Custom(_) => "custom-python-loader",
        }
    }
}

/// Ordered data schema for DataCard interfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DataSchema {
    /// Ordered columns; column order is part of the Wyrd contract.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub columns: Vec<FieldSpec>,
}

impl DataSchema {
    /// Build a schema from ordered fields; column order is part of the Wyrd contract.
    #[must_use]
    pub fn new(columns: Vec<FieldSpec>) -> Self {
        Self { columns }
    }

    /// Build an empty schema for interfaces that do not require tabular columns.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
        }
    }

    /// Return true when the schema has no declared fields.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    /// Return true when a field with this column name exists.
    #[must_use]
    pub fn contains_column(&self, name: &ColumnName) -> bool {
        self.columns.iter().any(|field| &field.name == name)
    }

    /// Return a field by column name.
    #[must_use]
    pub fn column(&self, name: &ColumnName) -> Option<&FieldSpec> {
        self.columns.iter().find(|field| &field.name == name)
    }

    /// Iterate field names in schema order.
    pub fn column_names(&self) -> impl Iterator<Item = &ColumnName> {
        self.columns.iter().map(|field| &field.name)
    }
}

/// Named DataCard split declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DataSplit {
    /// Stable split label, repeated to protect hand-authored map entries from drift.
    pub label: SplitName,
    /// Split strategy declaration.
    pub strategy: SplitStrategy,
}

impl DataSplit {
    /// Build a split that points at a pre-materialized Artifact card.
    #[must_use]
    pub fn materialized(label: SplitName, card_ref: CardRef) -> Self {
        Self {
            label,
            strategy: SplitStrategy::Materialized(card_ref),
        }
    }

    /// Build a column predicate split.
    #[must_use]
    pub fn column(label: SplitName, name: ColumnName, op: Inequality, value: ColValue) -> Self {
        Self {
            label,
            strategy: SplitStrategy::Column { name, op, value },
        }
    }

    /// Build an index-range split with inclusive start and exclusive stop semantics.
    #[must_use]
    pub fn index_range(label: SplitName, start: i64, stop: i64) -> Self {
        Self {
            label,
            strategy: SplitStrategy::IndexRange { start, stop },
        }
    }

    /// Build a split from explicit positional indices.
    #[must_use]
    pub fn indices(label: SplitName, values: Vec<i64>) -> Self {
        Self {
            label,
            strategy: SplitStrategy::Indices(values),
        }
    }

    /// Return the Artifact card reference when this is a materialized split.
    #[must_use]
    pub fn card_ref(&self) -> Option<&CardRef> {
        self.strategy.materialized_card_ref()
    }

    /// Return the referenced schema column when this is a column predicate split.
    #[must_use]
    pub fn referenced_column(&self) -> Option<&ColumnName> {
        self.strategy.referenced_column()
    }
}

/// Data split strategy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(tag = "kind", content = "value")]
pub enum SplitStrategy {
    /// Reference to an already materialized split artifact.
    Materialized(CardRef),
    /// Predicate over a schema column.
    Column {
        /// Schema column referenced by the predicate.
        name: ColumnName,
        /// Predicate operator.
        op: Inequality,
        /// Predicate comparison value.
        value: ColValue,
    },
    /// Positional range using inclusive start and exclusive stop.
    IndexRange {
        /// Inclusive starting index.
        start: i64,
        /// Exclusive stopping index.
        stop: i64,
    },
    /// Explicit positional indices.
    Indices(Vec<i64>),
}

impl SplitStrategy {
    /// Return the Artifact card reference carried by a materialized split.
    #[must_use]
    pub fn materialized_card_ref(&self) -> Option<&CardRef> {
        match self {
            Self::Materialized(card_ref) => Some(card_ref),
            _ => None,
        }
    }

    /// Return the schema column referenced by a column predicate split.
    #[must_use]
    pub fn referenced_column(&self) -> Option<&ColumnName> {
        match self {
            Self::Column { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Return the stable split strategy kind string used by Python, MCP, and docs.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Materialized(_) => "Materialized",
            Self::Column { .. } => "Column",
            Self::IndexRange { .. } => "IndexRange",
            Self::Indices(_) => "Indices",
        }
    }
}

/// Predicate operator for a column split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum Inequality {
    /// Equality comparison.
    Eq,
    /// Inequality comparison.
    Ne,
    /// Greater-than comparison.
    Gt,
    /// Greater-than-or-equal comparison.
    Ge,
    /// Less-than comparison.
    Lt,
    /// Less-than-or-equal comparison.
    Le,
    /// Membership comparison.
    In,
}

/// Literal value used by a column split predicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum ColValue {
    /// Signed integer predicate value.
    Int(i64),
    /// Floating-point predicate value.
    Float(f64),
    /// String predicate value.
    Str(String),
    /// Boolean predicate value.
    Bool(bool),
    /// UTC timestamp predicate value.
    Timestamp(DateTime<Utc>),
    /// List predicate value used by membership checks.
    List(Vec<ColValue>),
}

/// SQL query bundle associated with SQL-backed DataCards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SqlLogic {
    /// Named SQL queries.
    pub queries: HashMap<QueryName, String>,
    /// Optional default query key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_query: Option<QueryName>,
}

/// Storage and byte-level statistics for a DataCard source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct DataStats {
    /// Optional row count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_count: Option<u64>,
    /// Optional column count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col_count: Option<u32>,
    /// Number of bytes represented by this data artifact or source.
    pub byte_count: u64,
    /// Lowercase SHA-256 hex digest for the represented bytes.
    pub sha256: String,
}

/// Pandas interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PandasMeta {
    /// Installed pandas version inferred by the Python boundary.
    pub framework_version: String,
    /// Parquet compression used by local materialization.
    #[serde(default = "ParquetCompression::snappy")]
    pub compression: ParquetCompression,
}

/// Polars interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct PolarsMeta {
    /// Installed polars version inferred by the Python boundary.
    pub framework_version: String,
    /// Parquet compression used by local materialization.
    #[serde(default = "ParquetCompression::snappy")]
    pub compression: ParquetCompression,
}

/// Arrow interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ArrowMeta {
    /// Installed pyarrow version inferred by the Python boundary.
    pub framework_version: String,
    /// Arrow serialization format.
    pub format: ArrowFormat,
}

/// Parquet interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ParquetMeta {
    /// Parquet compression used by local materialization.
    #[serde(default = "ParquetCompression::snappy")]
    pub compression: ParquetCompression,
    /// Optional parquet row group size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_group_size: Option<u32>,
}

/// NumPy interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct NumpyMeta {
    /// Canonical dtype string.
    pub dtype: String,
    /// Array shape inferred by the Python boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shape: Vec<i64>,
    /// NumPy serialization format.
    pub format: NumpyFormat,
}

/// Torch interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TorchMeta {
    /// Installed torch version inferred by the Python boundary.
    pub framework_version: String,
    /// Torch serialization format.
    pub save_format: TorchSaveFormat,
}

/// SQL interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct SqlMeta {
    /// SQL dialect name.
    pub dialect: String,
    /// Optional connection hint for registry/runtime consumers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_hint: Option<String>,
}

/// JSON Lines interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct JsonlMeta {
    /// JSONL compression.
    pub compression: JsonlCompression,
    /// Optional number of lines written per local file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines_per_file: Option<u64>,
}

/// Image manifest interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct ImageMeta {
    /// Image file format family.
    pub format: ImageFormat,
    /// Optional manifest Artifact card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_ref: Option<CardRef>,
    /// Image color mode.
    pub color_mode: ColorMode,
}

/// Text manifest interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct TextMeta {
    /// Text encoding used by local files.
    #[serde(default = "default_text_encoding")]
    pub encoding: String,
    /// Optional manifest Artifact card reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest_ref: Option<CardRef>,
}

/// Hugging Face dataset interface metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct HuggingfaceMeta {
    /// Dataset identifier.
    pub dataset_id: String,
    /// Optional pinned git revision, represented as a 7- to 40-char lowercase hex SHA.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Optional dataset split.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<String>,
    /// Optional dataset config.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<String>,
}

/// Custom Python loader metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub struct CustomDataMeta {
    /// Importable Python module containing the loader class.
    pub loader_module: String,
    /// Python loader class name.
    pub loader_class: String,
    /// Loader-specific string options.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, String>,
}

/// Parquet compression choices accepted by the DataCard contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ParquetCompression {
    /// No compression.
    None,
    /// Snappy compression.
    Snappy,
    /// Gzip compression.
    Gzip,
    /// Zstandard compression.
    Zstd,
    /// LZ4 compression.
    Lz4,
}

impl ParquetCompression {
    /// Default parquet compression for DataCard parquet materialization.
    #[must_use]
    pub const fn snappy() -> Self {
        Self::Snappy
    }
}

/// Arrow serialization formats accepted by DataCard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ArrowFormat {
    /// Arrow IPC file or stream.
    Ipc,
    /// Parquet output through Arrow.
    Parquet,
}

/// NumPy serialization formats accepted by DataCard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum NumpyFormat {
    /// Single-array NPY format.
    Npy,
    /// Multi-array NPZ format.
    Npz,
}

/// Torch serialization formats accepted by DataCard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum TorchSaveFormat {
    /// Safetensors output.
    Safetensors,
    /// Explicit torch pickle output.
    Pickle,
}

/// JSONL compression choices accepted by DataCard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum JsonlCompression {
    /// No compression.
    None,
    /// Gzip compression.
    Gzip,
    /// Zstandard compression.
    Zstd,
}

/// Image format family accepted by DataCard image manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ImageFormat {
    /// PNG images.
    Png,
    /// JPEG images.
    Jpeg,
    /// WebP images.
    Webp,
    /// Mixed image formats.
    Mixed,
}

/// Image color mode accepted by DataCard image manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
pub enum ColorMode {
    /// RGB color.
    Rgb,
    /// RGBA color with alpha.
    Rgba,
    /// Single-channel grayscale.
    Grayscale,
}

fn default_text_encoding() -> String {
    "utf-8".to_string()
}
