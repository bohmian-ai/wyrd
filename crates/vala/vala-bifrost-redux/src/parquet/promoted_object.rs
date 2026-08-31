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

use std::sync::Arc;

use bytes::Bytes;
use parquet::file::reader::{FileReader, SerializedFileReader};

use crate::parquet::memory::{
    self, KEY_ENVELOPE_VERSION, KEY_RECIPE, KEY_SCHEMA, PARQUET_MEMORY_ENVELOPE_VERSION,
    WRITER_RECIPE,
};
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
    /// Decodes one promoted object's footer under the writer's own bounds.
    ///
    /// The read is bounded before it is parsed: the trailer must be present and
    /// well formed, the encoded footer must be inside the published envelope,
    /// and the compact-Thrift preflight must pass before any decoder allocates.
    /// Only then is the footer projected onto the table schema. The object body
    /// is never decoded, so the cost is one footer regardless of object size.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when the trailer magic is absent,
    /// the footer length is outside the published envelope, the compact-Thrift
    /// preflight refuses the encoding, the Parquet metadata cannot be parsed,
    /// the writer-recipe metadata is not the Bifrost contract, or the footer has
    /// no Iceberg projection onto the table schema.
    ///
    /// # Panics
    ///
    /// Panics if the eight-byte trailer's first four bytes are not exactly four
    /// bytes, which the length check immediately above the read establishes.
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
        let footer_bytes = u64::from(u32::from_le_bytes(
            trailer[..4]
                .try_into()
                .expect("four-byte Parquet footer length is exact"),
        ));
        memory::validate_encoded_footer_bytes(footer_bytes)?;
        let footer_len = usize::try_from(footer_bytes)
            .map_err(|_| "promoted object footer exceeds address space".to_owned())?;
        let footer_start = bytes
            .len()
            .checked_sub(8)
            .and_then(|end| end.checked_sub(footer_len))
            .ok_or_else(|| "promoted object footer extends before its start".to_owned())?;
        crate::parquet::footer_preflight::preflight_compact_thrift(
            &bytes[footer_start..bytes.len() - 8],
        )?;

        let reader = SerializedFileReader::new(bytes.clone())
            .map_err(|error| format!("promoted object footer does not parse: {error}"))?;
        let metadata = reader.metadata();
        let schema_fingerprint = Self::contract_fingerprint(metadata.file_metadata())?;
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

    /// Reads the writer-contract fields a promoted object must carry.
    ///
    /// Only the recipe, envelope version, and schema fingerprint are read here.
    /// The full eight-field envelope check belongs to the writer, which holds
    /// the Arrow schema the fingerprint is computed from; the promoter holds an
    /// Iceberg table and can only compare the fingerprint to the one the
    /// evidence claims.
    ///
    /// # Errors
    ///
    /// Returns a description of the refusal when the metadata is absent, a
    /// contract field is missing or duplicated, the recipe or envelope version
    /// is not the Bifrost contract, or the fingerprint is not lowercase hex.
    fn contract_fingerprint(
        footer: &parquet::file::metadata::FileMetaData,
    ) -> Result<String, String> {
        let metadata = footer
            .key_value_metadata()
            .ok_or_else(|| "promoted object carries no writer contract metadata".to_owned())?;
        let value = |key: &str| -> Result<&str, String> {
            let mut matches = metadata.iter().filter(|entry| entry.key == key);
            let first = matches
                .next()
                .and_then(|entry| entry.value.as_deref())
                .ok_or_else(|| format!("promoted object footer field `{key}` is missing"))?;
            if matches.next().is_some() {
                return Err(format!(
                    "promoted object footer field `{key}` is duplicated"
                ));
            }
            Ok(first)
        };
        if value(KEY_RECIPE)? != WRITER_RECIPE
            || value(KEY_ENVELOPE_VERSION)? != PARQUET_MEMORY_ENVELOPE_VERSION
        {
            return Err("promoted object was not sealed by the Bifrost writer".to_owned());
        }
        let fingerprint = value(KEY_SCHEMA)?;
        if fingerprint.len() != 64
            || !fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err("promoted object schema fingerprint is malformed".to_owned());
        }
        Ok(fingerprint.to_owned())
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
