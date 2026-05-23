//! Wyrd foundation types.
//!
//! `wyrd-spec` is pure data: no IO, no async runtime, no PyO3, and no server
//! framework dependencies. It defines the Card envelope, v1 Spec payloads,
//! cross-cutting identifiers, and generated schema surfaces used by every
//! downstream crate.

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod actor;
pub mod card;
pub mod envelope;
pub mod error;
pub mod format;
pub mod ids;
pub mod redaction;
pub mod reference;
pub mod request_id;
pub mod run;
pub mod schema;
pub mod storage;
pub mod trace;
pub mod version;

pub use card::data::{
    ArrowFormat, ArrowMeta, ColValue, ColorMode, CustomDataMeta, DataInterface, DataSchema,
    DataSpec, DataSplit, DataStats, HuggingfaceMeta, ImageFormat, ImageMeta, Inequality,
    JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta, ParquetCompression,
    ParquetMeta, PolarsMeta, SplitStrategy, SqlLogic, SqlMeta, TextMeta, TorchMeta,
    TorchSaveFormat,
};
pub use card::field::{Dim, FieldSpec};
pub use ids::{ColumnName, QueryName, SplitName};
