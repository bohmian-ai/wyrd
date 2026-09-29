//! Bounded footer proof for an immutable object Forge is asked to promote.
//!
//! Promotion adds a `DataFile` to a table without writing a byte, so the value
//! that lands in the manifest is evidence Scribe recorded, not evidence the
//! promoter computed. Size and checksum prove only that the object is the one
//! the row names — they say nothing about whether the statistics inside that
//! evidence describe the object's actual contents. This module re-derives the
//! `DataFile` projection from the object's own footer under the same bounds the
//! writer honored, so the promoter can compare the two and refuse a
//! disagreement before any catalog work begins.
//!
//! The footer is parsed by the standard Parquet reader; structural validity is
//! its job, not a Wyrd size policy.

use std::sync::Arc;

use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::parquet::footer::footer_schema_fingerprint;
use crate::scribe::promotion::ScribeDataFileV1;

/// What re-reading one promoted object's footer proved about the object.
///
/// The projection is deliberately taken from the *table's* current schema
/// rather than from a schema carried alongside the evidence: an object whose
/// columns no longer project onto the table cannot be promoted into it, and
/// discovering that here keeps it out of the manifest.
pub struct PromotedObjectFooter {
    /// Iceberg projection of the object's own footer.
    data_file: iceberg::spec::DataFile,
    /// Lowercase hex Wyrd schema fingerprint the footer carries.
    schema_fingerprint: String,
}

impl PromotedObjectFooter {
    /// Decodes one promoted object's footer and projects it onto the table.
    ///
    /// The trailer must be present and well formed and the footer must parse;
    /// the object must carry the Bifrost schema fingerprint. Only then is the
    /// footer projected onto the table schema. The object body is never
    /// decoded, so the cost is one footer regardless of object size.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when the trailer magic is absent,
    /// the Parquet metadata cannot be parsed, the Bifrost schema fingerprint is
    /// missing or malformed, or the footer has no Iceberg projection onto the
    /// table schema.
    pub fn decode(
        bytes: &Bytes,
        object_key: &str,
        table_schema: Arc<iceberg::spec::Schema>,
        file_size: u64,
    ) -> Result<Self, String> {
        let length = u64::try_from(bytes.len())
            .map_err(|_| "promoted object size exceeds u64".to_owned())?;
        if length != file_size {
            return Err("promoted object read a different length than it reports".to_owned());
        }
        if bytes.len() < 8 {
            return Err("promoted object is shorter than its Parquet trailer".to_owned());
        }
        let trailer = &bytes[bytes.len() - 8..];
        if &trailer[4..] != b"PAR1" {
            return Err("promoted object trailer magic is invalid".to_owned());
        }
        let reader = SerializedFileReader::new(bytes.clone())
            .map_err(|error| format!("promoted object footer does not parse: {error}"))?;
        let metadata = reader.metadata();
        let schema_fingerprint = footer_schema_fingerprint(metadata.file_metadata())
            .map_err(|refusal| format!("promoted object was not sealed by Bifrost: {refusal}"))?
            .to_owned();
        let written = usize::try_from(file_size)
            .map_err(|_| "promoted object size exceeds address space".to_owned())?;
        let data_file = iceberg::writer::file_writer::ParquetWriter::parquet_to_data_file_builder(
            table_schema,
            Arc::new(metadata.clone()),
            written,
            object_key.to_owned(),
            std::collections::HashMap::new(),
        )
        .map_err(|error| format!("promoted object footer has no Iceberg projection: {error}"))?
        .build()
        .map_err(|error| {
            format!("promoted object footer does not assemble a data file: {error}")
        })?;
        Ok(Self {
            data_file,
            schema_fingerprint,
        })
    }

    /// Projects the decoded footer into the persisted evidence shape.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when a bound the footer produced has
    /// no binary single-value serialization, which would make the physical file
    /// and the persisted evidence incomparable rather than merely unequal.
    pub fn metrics(&self) -> Result<ScribeDataFileV1, String> {
        ScribeDataFileV1::from_data_file(&self.data_file)
            .map_err(|error| format!("promoted object footer has no persisted projection: {error}"))
    }

    /// Borrows the lowercase hex schema fingerprint the footer carries.
    #[must_use]
    pub fn schema_fingerprint(&self) -> &str {
        &self.schema_fingerprint
    }

    /// Borrows the Iceberg projection of the object's own footer.
    #[must_use]
    pub fn data_file(&self) -> &iceberg::spec::DataFile {
        &self.data_file
    }
}
