use std::collections::HashMap;

use wyrd_spec::card::data::validate::{DataCardError, validate_data_spec};
use wyrd_spec::card::data::{
    ArrowFormat, ArrowMeta, ColValue, DataInterface, DataSchema, DataSpec, DataSplit, DataStats,
    HuggingfaceMeta, JsonlCompression, JsonlMeta, NumpyFormat, NumpyMeta, PandasMeta,
    ParquetCompression, ParquetMeta, PolarsMeta, SqlLogic, SqlMeta,
};
use wyrd_spec::card::{FieldSpec, Inequality};
use wyrd_spec::ids::{ColumnName, QueryName, SplitName};

fn col(name: &str) -> ColumnName {
    ColumnName::new(name).unwrap()
}

fn split(name: &str) -> SplitName {
    SplitName::new(name).unwrap()
}

fn query(name: &str) -> QueryName {
    QueryName::new(name).unwrap()
}

fn valid_stats() -> DataStats {
    DataStats {
        row_count: Some(10),
        col_count: Some(2),
        byte_count: 42,
        sha256: "a".repeat(64),
    }
}

fn schema() -> DataSchema {
    DataSchema::new(vec![
        FieldSpec::new(col("feature"), "int64"),
        FieldSpec::new(col("target"), "bool"),
    ])
}

fn valid_spec(interface: DataInterface) -> DataSpec {
    let sql = matches!(interface, DataInterface::Sql(_)).then(|| SqlLogic {
        queries: HashMap::from([(query("main"), "select * from t".to_string())]),
        default_query: Some(query("main")),
    });
    DataSpec {
        interface,
        schema: schema(),
        card_refs: Vec::new(),
        splits: HashMap::new(),
        target_columns: vec![col("target")],
        sql,
        stats: valid_stats(),
    }
}

fn pandas_spec() -> DataSpec {
    valid_spec(DataInterface::Pandas(PandasMeta {
        framework_version: "2.2.2".to_string(),
        compression: ParquetCompression::Snappy,
    }))
}

#[test]
fn empty_schema_rejected_for_tabular_interfaces() {
    let interfaces = vec![
        (
            "Pandas",
            DataInterface::Pandas(PandasMeta {
                framework_version: "2.2.2".to_string(),
                compression: ParquetCompression::Snappy,
            }),
        ),
        (
            "Polars",
            DataInterface::Polars(PolarsMeta {
                framework_version: "1.0.0".to_string(),
                compression: ParquetCompression::Snappy,
            }),
        ),
        (
            "Arrow",
            DataInterface::Arrow(ArrowMeta {
                framework_version: "16.0.0".to_string(),
                format: ArrowFormat::Parquet,
            }),
        ),
        (
            "Parquet",
            DataInterface::Parquet(ParquetMeta {
                compression: ParquetCompression::Snappy,
                row_group_size: None,
            }),
        ),
        (
            "Jsonl",
            DataInterface::Jsonl(JsonlMeta {
                compression: JsonlCompression::None,
                lines_per_file: None,
            }),
        ),
    ];

    for (kind, interface) in interfaces {
        let mut spec = valid_spec(interface);
        spec.schema = DataSchema::empty();
        assert!(matches!(
            validate_data_spec(&spec),
            Err(DataCardError::EmptySchema { kind: actual }) if actual == kind
        ));
    }
}

#[test]
fn empty_schema_allowed_for_nontabular() {
    let specs = [
        DataSpec {
            interface: DataInterface::Numpy(NumpyMeta {
                dtype: "float32".to_string(),
                shape: vec![2, 2],
                format: NumpyFormat::Npy,
            }),
            schema: DataSchema::empty(),
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: vec![col("target")],
            sql: None,
            stats: valid_stats(),
        },
        DataSpec {
            interface: DataInterface::Sql(SqlMeta {
                dialect: "postgres".to_string(),
                connection_hint: None,
            }),
            schema: DataSchema::empty(),
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: vec![col("target")],
            sql: Some(SqlLogic {
                queries: HashMap::from([(query("main"), "select * from t".to_string())]),
                default_query: Some(query("main")),
            }),
            stats: valid_stats(),
        },
    ];
    for spec in specs {
        assert!(validate_data_spec(&spec).is_ok());
    }
}

#[test]
fn duplicate_column_rejected() {
    let mut spec = pandas_spec();
    spec.schema
        .columns
        .push(FieldSpec::new(col("feature"), "int64"));
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::DuplicateColumn("feature".to_string()))
    );
}

#[test]
fn bad_split_key_rejected_via_deserialize() {
    let json = serde_json::json!({
        "interface": {"kind": "Pandas", "meta": {"framework_version": "2.2.2", "compression": "Snappy"}},
        "schema": {"columns": [{"name": "feature", "dtype": "int64"}, {"name": "target", "dtype": "bool"}]},
        "splits": {"Bad": {"label": "bad", "strategy": {"kind": "Indices", "value": [1]}}},
        "target_columns": ["target"],
        "stats": {"byte_count": 1, "sha256": "a".repeat(64)}
    });
    assert!(serde_json::from_value::<DataSpec>(json).is_err());
}

#[test]
fn column_split_unknown_column_rejected() {
    let mut spec = pandas_spec();
    spec.splits.insert(
        split("train"),
        DataSplit::column(
            split("train"),
            col("missing"),
            Inequality::Eq,
            ColValue::Int(1),
        ),
    );
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::SplitRuleUnknownColumn("missing".to_string()))
    );
}

#[test]
fn target_column_outside_schema_rejected() {
    let mut spec = pandas_spec();
    spec.target_columns = vec![col("missing")];
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::TargetColumnUnknown("missing".to_string()))
    );
}

#[test]
fn target_columns_allowed_when_schema_empty() {
    let mut spec = valid_spec(DataInterface::Numpy(NumpyMeta {
        dtype: "float32".to_string(),
        shape: vec![],
        format: NumpyFormat::Npy,
    }));
    spec.schema = DataSchema::empty();
    spec.target_columns = vec![col("missing")];
    assert!(validate_data_spec(&spec).is_ok());
}

#[test]
fn split_label_mismatch_rejected() {
    let mut spec = pandas_spec();
    spec.splits.insert(
        split("train"),
        DataSplit::indices(split("test"), vec![1, 2]),
    );
    assert!(matches!(
        validate_data_spec(&spec),
        Err(DataCardError::SplitKeyLabelMismatch { .. })
    ));
}

#[test]
fn index_range_inverted_rejected() {
    let mut spec = pandas_spec();
    spec.splits.insert(
        split("train"),
        DataSplit::index_range(split("train"), 10, 2),
    );
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::IndexRangeOrder { start: 10, stop: 2 })
    );
}

#[test]
fn indices_negative_or_duplicate_rejected() {
    let mut spec = pandas_spec();
    spec.splits.insert(
        split("train"),
        DataSplit::indices(split("train"), vec![1, 1]),
    );
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::SplitIndicesInvalid)
    );
}

#[test]
fn sha256_must_be_lowercase_hex_64() {
    let mut spec = pandas_spec();
    spec.stats.sha256 = "A".repeat(64);
    assert_eq!(validate_data_spec(&spec), Err(DataCardError::Sha256Invalid));
}

#[test]
fn byte_count_zero_allowed_for_draft_cards() {
    // byte_count=0 is the draft sentinel; validate_data_spec is structural-only.
    // Byte-count enforcement belongs to the save path, not load-time validation.
    let mut spec = pandas_spec();
    spec.stats.byte_count = 0;
    assert!(validate_data_spec(&spec).is_ok());
}

#[test]
fn sql_interface_requires_queries() {
    let mut spec = valid_spec(DataInterface::Sql(SqlMeta {
        dialect: "postgres".to_string(),
        connection_hint: None,
    }));
    spec.sql = Some(SqlLogic {
        queries: HashMap::new(),
        default_query: None,
    });
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::SqlQueriesEmpty)
    );
}

#[test]
fn sql_default_query_must_exist() {
    let mut spec = valid_spec(DataInterface::Sql(SqlMeta {
        dialect: "postgres".to_string(),
        connection_hint: None,
    }));
    spec.sql = Some(SqlLogic {
        queries: HashMap::from([(query("main"), "select 1".to_string())]),
        default_query: Some(query("other")),
    });
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::SqlDefaultMissing("other".to_string()))
    );
}

#[test]
fn hf_revision_rejects_nonhex_and_short() {
    let spec = DataSpec {
        interface: DataInterface::Huggingface(HuggingfaceMeta {
            dataset_id: "acme/data".to_string(),
            revision: Some("bad".to_string()),
            split: None,
            config: None,
        }),
        schema: DataSchema::empty(),
        card_refs: Vec::new(),
        splits: HashMap::new(),
        target_columns: Vec::new(),
        sql: None,
        stats: valid_stats(),
    };
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::HuggingfaceRevisionInvalid("bad".to_string()))
    );
}

#[test]
fn datacard_error_maps_to_public_wyrd_error_codes() {
    let error: wyrd_spec::error::WyrdError =
        DataCardError::TargetColumnUnknown("target".to_string()).into();
    assert_eq!(error.code(), "WYRD_DATA_400_TARGET_COLUMN_UNKNOWN");

    let error: wyrd_spec::error::WyrdError =
        DataCardError::SplitRuleUnknownColumn("feature".to_string()).into();
    assert_eq!(error.code(), "WYRD_DATA_400_INVALID_SPLIT_RULE");
}

#[test]
fn sql_interface_without_sql_block_rejected() {
    let mut spec = valid_spec(DataInterface::Sql(SqlMeta {
        dialect: "postgres".to_string(),
        connection_hint: None,
    }));
    spec.sql = None;
    assert_eq!(
        validate_data_spec(&spec),
        Err(DataCardError::SqlQueriesEmpty)
    );
}

#[test]
fn hf_revision_accepts_7_and_40_char_sha() {
    for revision in ["abcdef0", &"a".repeat(40)] {
        let spec = DataSpec {
            interface: DataInterface::Huggingface(HuggingfaceMeta {
                dataset_id: "acme/data".to_string(),
                revision: Some(revision.to_string()),
                split: None,
                config: None,
            }),
            schema: DataSchema::empty(),
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: Vec::new(),
            sql: None,
            stats: valid_stats(),
        };
        assert!(
            validate_data_spec(&spec).is_ok(),
            "revision {revision} should be valid"
        );
    }
}

#[test]
fn hf_revision_rejects_6_and_41_char() {
    for revision in ["abcde0", &"a".repeat(41)] {
        let spec = DataSpec {
            interface: DataInterface::Huggingface(HuggingfaceMeta {
                dataset_id: "acme/data".to_string(),
                revision: Some(revision.to_string()),
                split: None,
                config: None,
            }),
            schema: DataSchema::empty(),
            card_refs: Vec::new(),
            splits: HashMap::new(),
            target_columns: Vec::new(),
            sql: None,
            stats: valid_stats(),
        };
        assert_eq!(
            validate_data_spec(&spec),
            Err(DataCardError::HuggingfaceRevisionInvalid(
                revision.to_string()
            ))
        );
    }
}
