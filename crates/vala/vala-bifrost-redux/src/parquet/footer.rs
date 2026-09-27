//! Identity fields every Bifrost-written Parquet object carries in its footer.
//!
//! A sealed object names the exact Arrow schema it was written against and the
//! committed object identity it was written for. Readers that hold that schema
//! and identity — the staged writer re-inspecting its own output, and Forge
//! promoting an object Scribe recorded — compare them before trusting the
//! object's contents. Size and shape are not part of this contract: row-group
//! and file sizes are soft writer targets, and standard Parquet parsing owns
//! structural validity.

use arrow::datatypes::Schema;
use parquet::file::metadata::{FileMetaData, KeyValue};

use crate::schema::SchemaFingerprint;

/// Footer key carrying the Wyrd schema fingerprint an object was sealed against.
pub const KEY_SCHEMA: &str = "wyrd.bifrost.schema_fingerprint";
/// Footer key carrying the committed object identity an object was sealed for.
pub const KEY_OBJECT: &str = "wyrd.bifrost.object_identity";

/// Returns the exact schema fingerprint producers stamp and readers compare.
#[must_use]
pub fn schema_fingerprint(schema: &Schema) -> SchemaFingerprint {
    SchemaFingerprint::from_arrow_schema_exact(schema)
}

/// The schema and object an object's footer binds it to.
///
/// Constructed by a writer from the Arrow schema and object identity it is
/// sealing, then either stamped into the footer or compared against a footer a
/// reader holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BifrostFooterIdentity {
    /// Exact fingerprint of the Arrow schema the object was written against.
    schema_fingerprint: SchemaFingerprint,
    /// Committed object identity the object was written for.
    object_identity: String,
}

impl BifrostFooterIdentity {
    /// Binds `schema` to `object_identity`.
    ///
    /// # Errors
    /// Returns a refusal when `object_identity` is empty, since an object with
    /// no identity cannot be published or compared.
    pub fn new(schema: &Schema, object_identity: &str) -> Result<Self, String> {
        if object_identity.is_empty() {
            return Err("Bifrost footer identity requires an object identity".to_owned());
        }
        Ok(Self {
            schema_fingerprint: schema_fingerprint(schema),
            object_identity: object_identity.to_owned(),
        })
    }

    /// Returns the two footer fields a writer appends before closing the object.
    #[must_use]
    pub fn key_values(&self) -> Vec<KeyValue> {
        vec![
            KeyValue::new(
                KEY_SCHEMA.to_owned(),
                hex::encode(self.schema_fingerprint.0),
            ),
            KeyValue::new(KEY_OBJECT.to_owned(), self.object_identity.clone()),
        ]
    }

    /// Checks that `footer` carries exactly this identity.
    ///
    /// # Errors
    /// Returns a refusal when either field is missing, duplicated, or names a
    /// different schema or object.
    pub fn verify(&self, footer: &FileMetaData) -> Result<(), String> {
        if footer_value(footer, KEY_SCHEMA)? != hex::encode(self.schema_fingerprint.0) {
            return Err("Bifrost footer schema fingerprint does not match".to_owned());
        }
        if footer_value(footer, KEY_OBJECT)? != self.object_identity {
            return Err("Bifrost footer object identity does not match".to_owned());
        }
        Ok(())
    }
}

/// Reads the lowercase hex schema fingerprint a Bifrost footer carries.
///
/// Used by a reader that holds no Arrow schema to recompute it from, and so can
/// only compare the stamped value against one recorded elsewhere.
///
/// # Errors
/// Returns a refusal when the field is missing, duplicated, or not 64
/// lowercase hex characters.
pub fn footer_schema_fingerprint(footer: &FileMetaData) -> Result<&str, String> {
    let fingerprint = footer_value(footer, KEY_SCHEMA)?;
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("Bifrost footer schema fingerprint is malformed".to_owned());
    }
    Ok(fingerprint)
}

/// Returns the single value of footer field `key`.
///
/// # Errors
/// Returns a refusal when the footer has no metadata or the field is missing,
/// valueless, or duplicated.
fn footer_value<'footer>(footer: &'footer FileMetaData, key: &str) -> Result<&'footer str, String> {
    let mut matches = footer
        .key_value_metadata()
        .into_iter()
        .flatten()
        .filter(|entry| entry.key == key);
    let first = matches
        .next()
        .and_then(|entry| entry.value.as_deref())
        .ok_or_else(|| format!("Bifrost footer field `{key}` is missing"))?;
    if matches.next().is_some() {
        return Err(format!("Bifrost footer field `{key}` is duplicated"));
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use arrow::array::{RecordBatch, TimestampMicrosecondArray};
    use arrow::datatypes::{DataType, Field, TimeUnit};
    use parquet::arrow::ArrowWriter;
    use parquet::file::metadata::ParquetMetaData;
    use parquet::file::properties::WriterProperties;
    use parquet::file::reader::{FileReader, SerializedFileReader};

    use super::*;

    /// Writes `batch` with `metadata` and returns the parsed footer.
    ///
    /// # Panics
    /// Panics when the fixture cannot write or reread its own object.
    fn footer_with_metadata(batch: &RecordBatch, metadata: Vec<KeyValue>) -> ParquetMetaData {
        let mut bytes = Vec::new();
        let properties = WriterProperties::builder()
            .set_key_value_metadata(Some(metadata))
            .build();
        let mut writer = ArrowWriter::try_new(&mut bytes, batch.schema(), Some(properties))
            .expect("fixture writer");
        writer.write(batch).expect("fixture rows");
        writer.close().expect("fixture footer");
        SerializedFileReader::new(bytes::Bytes::from(bytes))
            .expect("fixture footer parses")
            .metadata()
            .clone()
    }

    /// Footer identity is exact across UTC spelling aliases and nullability,
    /// but accepts a schema a reader rebuilt with its own field metadata.
    ///
    /// # Panics
    /// Panics when an aliased or relaxed schema verifies, or the exact or
    /// rebuilt schema does not.
    #[test]
    fn bifrost_footer_fingerprint_does_not_alias_utc_spellings() {
        let utc = |nullable| {
            Field::new(
                "observed_at",
                DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
                nullable,
            )
        };
        let utc_schema = Arc::new(Schema::new(vec![utc(false)]));
        let batch = RecordBatch::try_new(
            Arc::clone(&utc_schema),
            vec![Arc::new(
                TimestampMicrosecondArray::from(vec![1]).with_timezone("UTC"),
            )],
        )
        .expect("UTC batch");
        let object = "tenants/test/table/day=2026-08-17/source.parquet";
        let identity = BifrostFooterIdentity::new(&utc_schema, object).expect("identity");
        let footer = footer_with_metadata(&batch, identity.key_values());
        let footer = footer.file_metadata();

        identity
            .verify(footer)
            .expect("the exact source schema verifies");
        let offset = Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, Some("+00:00".into())),
            false,
        )]);
        assert!(
            BifrostFooterIdentity::new(&offset, object)
                .expect("identity")
                .verify(footer)
                .is_err(),
            "a different timezone spelling is a different schema"
        );
        let rebuilt = Schema::new(vec![utc(false).with_metadata(
            std::collections::HashMap::from([("reader.note".to_owned(), "rebuilt".to_owned())]),
        )]);
        BifrostFooterIdentity::new(&rebuilt, object)
            .expect("identity")
            .verify(footer)
            .expect("an independently rebuilt equivalent layout verifies");
        assert!(
            BifrostFooterIdentity::new(&Schema::new(vec![utc(true)]), object)
                .expect("identity")
                .verify(footer)
                .is_err(),
            "a nullability change is a layout change"
        );
        assert!(
            BifrostFooterIdentity::new(&utc_schema, "another.parquet")
                .expect("identity")
                .verify(footer)
                .is_err(),
            "another object identity does not verify"
        );
        assert_eq!(
            footer_schema_fingerprint(footer).expect("fingerprint"),
            hex::encode(schema_fingerprint(&utc_schema).0)
        );
    }

    /// A footer missing or duplicating an identity field is refused.
    ///
    /// # Panics
    /// Panics when an incomplete or ambiguous footer verifies.
    #[test]
    fn missing_or_duplicated_footer_identity_is_refused() {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "observed_at",
            DataType::Timestamp(TimeUnit::Microsecond, None),
            false,
        )]));
        let batch = RecordBatch::try_new(
            Arc::clone(&schema),
            vec![Arc::new(TimestampMicrosecondArray::from(vec![1]))],
        )
        .expect("batch");
        let identity = BifrostFooterIdentity::new(&schema, "object.parquet").expect("identity");
        let fields = identity.key_values();

        let missing = footer_with_metadata(&batch, fields[..1].to_vec());
        assert!(identity.verify(missing.file_metadata()).is_err());
        let mut twice = fields.clone();
        twice.push(fields[0].clone());
        let duplicated = footer_with_metadata(&batch, twice);
        assert!(identity.verify(duplicated.file_metadata()).is_err());
        assert!(BifrostFooterIdentity::new(&schema, "").is_err());
    }
}
