//! Validation for the locked DataCard contract.

use std::collections::HashSet;

use serde_json::json;

use crate::card::data::{DataInterface, DataSpec, SplitStrategy};
use crate::error::WyrdError;

/// DataCard validation failures raised by the pure Rust validator.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DataCardError {
    /// A tabular interface had no schema columns.
    #[error("schema must be non-empty for interface kind {kind}")]
    EmptySchema {
        /// Interface kind that requires columns.
        kind: &'static str,
    },
    /// Two fields declared the same column name.
    #[error("duplicate column name: {0}")]
    DuplicateColumn(String),
    /// A column split referenced a column absent from the schema.
    #[error("rule-based split references unknown column: {0}")]
    SplitRuleUnknownColumn(String),
    /// A target column was not present in the schema.
    #[error("target column not present in schema: {0}")]
    TargetColumnUnknown(String),
    /// A split map key differed from the serialized split label.
    #[error("split key {key} does not match strategy label {label}")]
    SplitKeyLabelMismatch {
        /// Split map key.
        key: String,
        /// Serialized split label.
        label: String,
    },
    /// An index range used start greater than stop.
    #[error("split index range invalid: start {start} > stop {stop}")]
    IndexRangeOrder {
        /// Inclusive start index.
        start: i64,
        /// Exclusive stop index.
        stop: i64,
    },
    /// Split indices included negative or duplicate positions.
    #[error("split indices must be non-negative and unique")]
    SplitIndicesInvalid,
    /// `DataStats.sha256` was not a 64-character lowercase hex digest.
    #[error("stats.sha256 must be 64-char lowercase hex")]
    Sha256Invalid,
    /// `DataStats.byte_count` was zero.
    #[error("stats.byte_count must be > 0")]
    ByteCountZero,
    /// A SQL interface had no SQL query bundle.
    #[error("SqlMeta requires non-empty sql.queries")]
    SqlQueriesEmpty,
    /// The default SQL query key was absent from the query map.
    #[error("sql.default_query {0} missing from sql.queries")]
    SqlDefaultMissing(String),
    /// A Hugging Face revision was not a short or full lowercase hex SHA.
    #[error("HuggingfaceMeta.revision must match ^[a-f0-9]{{7,40}}$: {0}")]
    HuggingfaceRevisionInvalid(String),
}

/// Run every locked DataCard validation invariant in deterministic order.
///
/// # Errors
/// Returns the first invariant failure in the locked validation order.
pub fn validate_data_spec(spec: &DataSpec) -> Result<(), DataCardError> {
    check_schema_nonempty_for_tabular(spec)?;
    check_columns_unique(spec)?;
    check_split_rule_columns(spec)?;
    check_target_columns_in_schema(spec)?;
    check_split_label_matches_key(spec)?;
    check_index_range_order(spec)?;
    check_indices_nonnegative_unique(spec)?;
    check_sha256(spec)?;
    check_sql_queries_nonempty_for_sql(spec)?;
    check_sql_default_in_queries(spec)?;
    check_hf_revision(spec)?;
    Ok(())
}

/// Enforce non-empty schema columns for tabular interfaces.
pub fn check_schema_nonempty_for_tabular(spec: &DataSpec) -> Result<(), DataCardError> {
    if spec.interface.requires_schema_columns() && spec.schema.columns.is_empty() {
        Err(DataCardError::EmptySchema {
            kind: spec.interface.kind(),
        })
    } else {
        Ok(())
    }
}

/// Enforce unique `FieldSpec.name` values within the schema.
pub fn check_columns_unique(spec: &DataSpec) -> Result<(), DataCardError> {
    let mut seen = HashSet::new();
    for field in &spec.schema.columns {
        if !seen.insert(field.name.as_str()) {
            return Err(DataCardError::DuplicateColumn(field.name.to_string()));
        }
    }
    Ok(())
}

/// Enforce that rule-based split predicates reference schema columns that exist.
pub fn check_split_rule_columns(spec: &DataSpec) -> Result<(), DataCardError> {
    for split in spec.splits.values() {
        if let Some(name) = split.referenced_column()
            && !spec.schema.contains_column(name)
        {
            return Err(DataCardError::SplitRuleUnknownColumn(name.to_string()));
        }
    }
    Ok(())
}

/// Enforce that target columns exist when the schema is populated.
pub fn check_target_columns_in_schema(spec: &DataSpec) -> Result<(), DataCardError> {
    if spec.schema.columns.is_empty() {
        return Ok(());
    }
    for name in &spec.target_columns {
        if !spec.schema.contains_column(name) {
            return Err(DataCardError::TargetColumnUnknown(name.to_string()));
        }
    }
    Ok(())
}

/// Enforce that each split map key matches its serialized split label.
pub fn check_split_label_matches_key(spec: &DataSpec) -> Result<(), DataCardError> {
    for (key, split) in &spec.splits {
        if key != &split.label {
            return Err(DataCardError::SplitKeyLabelMismatch {
                key: key.to_string(),
                label: split.label.to_string(),
            });
        }
    }
    Ok(())
}

/// Enforce start <= stop for index-range splits.
pub fn check_index_range_order(spec: &DataSpec) -> Result<(), DataCardError> {
    for split in spec.splits.values() {
        if let SplitStrategy::IndexRange { start, stop } = &split.strategy
            && start > stop
        {
            return Err(DataCardError::IndexRangeOrder {
                start: *start,
                stop: *stop,
            });
        }
    }
    Ok(())
}

/// Enforce non-negative unique values for explicit index splits.
pub fn check_indices_nonnegative_unique(spec: &DataSpec) -> Result<(), DataCardError> {
    for split in spec.splits.values() {
        if let SplitStrategy::Indices(values) = &split.strategy {
            let mut seen = HashSet::new();
            for value in values {
                if *value < 0 || !seen.insert(value) {
                    return Err(DataCardError::SplitIndicesInvalid);
                }
            }
        }
    }
    Ok(())
}

/// Enforce lowercase 64-character hex SHA-256 stats.
pub fn check_sha256(spec: &DataSpec) -> Result<(), DataCardError> {
    let sha = &spec.stats.sha256;
    if sha.len() == 64
        && sha
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(DataCardError::Sha256Invalid)
    }
}

/// Enforce positive byte count.
pub fn check_byte_count(spec: &DataSpec) -> Result<(), DataCardError> {
    if spec.stats.byte_count > 0 {
        Ok(())
    } else {
        Err(DataCardError::ByteCountZero)
    }
}

/// Enforce non-empty SQL queries when the interface is SQL.
pub fn check_sql_queries_nonempty_for_sql(spec: &DataSpec) -> Result<(), DataCardError> {
    if spec.interface.requires_sql_logic()
        && spec.sql.as_ref().is_none_or(|sql| sql.queries.is_empty())
    {
        Err(DataCardError::SqlQueriesEmpty)
    } else {
        Ok(())
    }
}

/// Enforce that the default SQL query exists when set.
pub fn check_sql_default_in_queries(spec: &DataSpec) -> Result<(), DataCardError> {
    let Some(sql) = &spec.sql else {
        return Ok(());
    };
    let Some(default_query) = &sql.default_query else {
        return Ok(());
    };
    if sql.queries.contains_key(default_query) {
        Ok(())
    } else {
        Err(DataCardError::SqlDefaultMissing(default_query.to_string()))
    }
}

/// Enforce Hugging Face revision format when present.
pub fn check_hf_revision(spec: &DataSpec) -> Result<(), DataCardError> {
    let DataInterface::Huggingface(meta) = &spec.interface else {
        return Ok(());
    };
    let Some(revision) = &meta.revision else {
        return Ok(());
    };
    if (7..=40).contains(&revision.len())
        && revision
            .chars()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(DataCardError::HuggingfaceRevisionInvalid(
            revision.to_string(),
        ))
    }
}

impl From<DataCardError> for WyrdError {
    fn from(error: DataCardError) -> Self {
        let message = error.to_string();
        match error {
            DataCardError::SplitRuleUnknownColumn(column) => WyrdError::DataInvalidSplitRule {
                message,
                details: json!({ "column": column }),
            },
            DataCardError::IndexRangeOrder { start, stop } => WyrdError::DataInvalidSplitRule {
                message,
                details: json!({ "start": start, "stop": stop }),
            },
            DataCardError::SplitIndicesInvalid => WyrdError::DataInvalidSplitRule {
                message,
                details: json!({ "rule": "indices" }),
            },
            DataCardError::TargetColumnUnknown(column) => WyrdError::DataTargetColumnUnknown {
                message,
                details: json!({ "column": column }),
            },
            other => WyrdError::DataValidation {
                message,
                details: json!({ "reason": format!("{other:?}") }),
            },
        }
    }
}
